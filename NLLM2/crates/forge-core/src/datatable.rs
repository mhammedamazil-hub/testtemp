//! Data-table artifacts: a projection of a real source grid, with per-cell evidence.
//!
//! The spec's ART-06 asks for "data tables as an artifact (schema inference, per-cell evidence,
//! export)". Only the middle part is honestly hard, and it is the part this module is built
//! around: **every value in a data-table artifact carries the A1 reference it was copied from**,
//! so the artifact is a *view* of a stored grid, not a new claim about the world.
//!
//! Two deliberate refusals:
//!
//! * **No schema inference from prose.** Forge does not invent a table from a paragraph. It
//!   projects a sheet that a spreadsheet or CSV source already contains, optionally filtering
//!   rows and selecting columns.
//! * **No silent widening.** A filter that would drop everything, a column that does not exist,
//!   a cell that is empty — each is reported (or refused) instead of being quietly filled in.
//!   Exactly one convenience is allowed and it is *not* an invention: an empty cell in the
//!   projection stays empty and is counted.

use crate::domain::Source;
use crate::obj;
use crate::json::Value;
use crate::tables::{Cell, Table};

/// How many rows a projection keeps before it truncates and says so.
pub const ROW_LIMIT_MAX: usize = 200;
pub const ROW_LIMIT_DEFAULT: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Eq,
    Contains,
    Gt,
    Lt,
    IsEmpty,
    NotEmpty,
}

impl Op {
    pub fn parse(raw: &str) -> Option<Op> {
        Some(match raw.trim().to_ascii_lowercase().as_str() {
            "" | "eq" | "=" | "equals" => Op::Eq,
            "contains" => Op::Contains,
            "gt" | ">" => Op::Gt,
            "lt" | "<" => Op::Lt,
            "is_empty" | "empty" => Op::IsEmpty,
            "not_empty" | "notempty" => Op::NotEmpty,
            _ => return None,
        })
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Op::Eq => "eq",
            Op::Contains => "contains",
            Op::Gt => "gt",
            Op::Lt => "lt",
            Op::IsEmpty => "is_empty",
            Op::NotEmpty => "not_empty",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spec {
    /// Which stored grid to project (a `tables` row id).
    pub table_id: String,
    /// Which columns to keep; empty means every column.
    pub columns: Vec<String>,
    /// Optional row filter.
    pub filter_column: String,
    pub filter_op: String,
    pub filter_value: String,
    pub limit: usize,
    pub title: String,
}

fn number(text: &str) -> Option<f64> {
    let cleaned: String = text
        .chars()
        .filter(|c| !matches!(c, ',' | '%' | '$' | ' '))
        .collect();
    cleaned.parse::<f64>().ok()
}

fn cell_text(cell: &Cell) -> String {
    // `text` is the display text the parser recorded (`12 kg`, `$1,200`), not a re-derivation.
    cell.text.trim().to_string()
}

fn matches(op: &Op, cell: &Cell, needle: &str) -> bool {
    let text = cell_text(cell);
    match op {
        Op::IsEmpty => text.is_empty(),
        Op::NotEmpty => !text.is_empty(),
        Op::Eq => text.eq_ignore_ascii_case(needle.trim()),
        Op::Contains => text.to_ascii_lowercase().contains(&needle.trim().to_ascii_lowercase()),
        Op::Gt | Op::Lt => match (number(&text), number(needle)) {
            (Some(a), Some(b)) => {
                if *op == Op::Gt {
                    a > b
                } else {
                    a < b
                }
            }
            // A non-numeric cell never satisfies a numeric comparison. It is not "false by
            // default" silently: the count of uncomparable cells is reported by `build`.
            _ => false,
        },
    }
}

/// Build the artifact payload. `Err` carries the reason the user should see.
pub fn build(spec: &Spec, tables: &[Table], sources: &[Source]) -> Result<(String, Value), String> {
    if spec.table_id.trim().is_empty() {
        return Err("a data table artifact needs table_id: pick a sheet from GET /api/sources/:id/tables".into());
    }
    let table = tables
        .iter()
        .find(|t| t.id == spec.table_id)
        .ok_or_else(|| format!("no stored grid with id \"{}\"", spec.table_id))?;
    let source = sources.iter().find(|s| s.id == table.source_id);
    let source_title = source
        .map(|s| s.title.clone())
        .unwrap_or_else(|| format!("{} (source removed)", table.source_id));

    // Column selection: names must exist, in the table's own order, so the projection is stable.
    let wanted: Vec<usize> = if spec.columns.is_empty() {
        (0..table.headers.len()).collect()
    } else {
        let mut out = Vec::new();
        for name in &spec.columns {
            let idx = table
                .column_index(name)
                .ok_or_else(|| format!("no column \"{}\" in {}", name, table.sheet))?;
            if !out.contains(&idx) {
                out.push(idx);
            }
        }
        out.sort_unstable();
        out
    };
    if wanted.is_empty() {
        return Err(format!("{} has no columns to project", table.sheet));
    }

    let op = if spec.filter_column.trim().is_empty() {
        None
    } else {
        let parsed = Op::parse(&spec.filter_op)
            .ok_or_else(|| format!("unknown filter op \"{}\"", spec.filter_op))?;
        Some(parsed)
    };
    let filter_idx = match (&op, spec.filter_column.trim().is_empty()) {
        (Some(_), false) => Some(
            table
                .column_index(spec.filter_column.trim())
                .ok_or_else(|| format!("no column \"{}\" in {}", spec.filter_column, table.sheet))?,
        ),
        _ => None,
    };
    if let (Some(o), Some(_)) = (&op, filter_idx) {
        if !matches!(o, Op::IsEmpty | Op::NotEmpty) && spec.filter_value.trim().is_empty() {
            return Err(format!("filter op \"{}\" needs a filter_value", o.as_str()));
        }
        if matches!(o, Op::Gt | Op::Lt) && number(&spec.filter_value).is_none() {
            return Err(format!(
                "filter op \"{}\" needs a numeric filter_value, got \"{}\"",
                o.as_str(),
                spec.filter_value
            ));
        }
    }

    let limit = if spec.limit == 0 { ROW_LIMIT_DEFAULT } else { spec.limit.min(ROW_LIMIT_MAX) };
    let mut rows: Vec<Value> = Vec::new();
    let mut considered = 0usize;
    let mut filtered_out = 0usize;
    let mut uncomparable = 0usize;
    let mut empty_cells = 0usize;
    let mut truncated = false;

    for row in &table.rows {
        considered += 1;
        if let (Some(o), Some(fidx)) = (&op, filter_idx) {
            let cell = table.cell(row, fidx);
            let hit = match cell {
                Some(c) => {
                    if matches!(o, Op::Gt | Op::Lt) && number(&cell_text(c)).is_none() && !cell_text(c).is_empty() {
                        uncomparable += 1;
                    }
                    matches(o, c, &spec.filter_value)
                }
                None => false,
            };
            if !hit {
                filtered_out += 1;
                continue;
            }
        }
        if rows.len() >= limit {
            truncated = true;
            break;
        }
        let mut cells: Vec<Value> = Vec::new();
        for &idx in &wanted {
            let (value, reference) = match table.cell(row, idx) {
                Some(c) => {
                    let text = cell_text(c);
                    if text.is_empty() {
                        empty_cells += 1;
                    }
                    // The A1 reference the value came from: this is the per-cell evidence. The
                    // parser's own reference wins; the computed one is the fallback.
                    let reference = if c.reference.is_empty() {
                        format!("{}{}", column_letters(idx), row.number)
                    } else {
                        c.reference.clone()
                    };
                    (text, reference)
                }
                // A ragged row has no cell here. Saying so is not the same as writing "".
                None => {
                    empty_cells += 1;
                    (String::new(), format!("{}{}", column_letters(idx), row.number))
                }
            };
            cells.push(obj! { "value" => value, "ref" => reference });
        }
        let first = format!("{}{}", column_letters(wanted[0]), row.number);
        let last_idx = *wanted.last().unwrap();
        let last = format!("{}{}", column_letters(last_idx), row.number);
        rows.push(obj! {
            "number" => row.number as u64,
            "cells" => Value::Arr(cells),
            "ref" => format!("{}:{}", first, last),
        });
    }

    if rows.is_empty() {
        return Err(format!(
            "the projection is empty: {} row(s) read from {}, all excluded by the filter",
            considered, table.sheet
        ));
    }

    let headers: Vec<Value> = wanted
        .iter()
        .map(|&i| {
            obj! {
                "name" => table.headers.get(i).cloned().unwrap_or_default(),
                "letter" => column_letters(i),
                "numeric" => {
                    // "numeric" is a fact about the projected values, not a guess: true only when
                    // every non-empty projected value parses as a number.
                    let values: Vec<&Cell> = table.rows.iter().filter_map(|r| table.cell(r, i)).collect();
                    let non_empty: Vec<&&Cell> = values.iter().filter(|c| !cell_text(c).is_empty()).collect();
                    !non_empty.is_empty() && non_empty.iter().all(|c| number(&cell_text(c)).is_some())
                },
            }
        })
        .collect();

    let title = if spec.title.trim().is_empty() {
        format!("{} · {} rows", table.sheet, rows.len())
    } else {
        spec.title.trim().to_string()
    };

    let row_count = rows.len() as u64;
    let payload = obj! {
        "kind" => "data_table",
        "sheet" => table.sheet.as_str(),
        "table_id" => table.id.as_str(),
        "source_id" => table.source_id.as_str(),
        "source_title" => source_title.as_str(),
        "columns" => Value::Arr(headers),
        "rows" => Value::Arr(rows),
        "row_count" => row_count,
        "rows_read" => considered as u64,
        "filtered_out" => filtered_out as u64,
        "uncomparable_cells" => uncomparable as u64,
        "empty_cells" => empty_cells as u64,
        "truncated" => truncated,
        "limit" => limit as u64,
        "filter" => if op.is_some() {
            obj! {
                "column" => spec.filter_column.trim(),
                "op" => op.as_ref().map(|o| o.as_str()).unwrap_or(""),
                "value" => spec.filter_value.as_str(),
            }
        } else { Value::Null },
        "evidence" => "every value carries the A1 reference it was copied from; the grid itself is unchanged",
        "note" => "a projection of a stored grid, not a table inferred from prose",
    };
    Ok((title, payload))
}

/// Spreadsheet column letters for a 0-based index: 0→A, 25→Z, 26→AA.
pub fn column_letters(mut index: usize) -> String {
    let mut out: Vec<u8> = Vec::new();
    loop {
        out.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// RFC 4180 CSV of the projection: header row, then the projected values in order.
/// Cells that contain a comma, a quote or a newline are quoted; quotes are doubled.
pub fn to_csv(payload: &Value) -> String {
    let mut out = String::new();
    let columns: Vec<String> = payload
        .get("columns")
        .and_then(|c| c.as_arr())
        .map(|list| list.iter().map(|c| c.str_of("name").unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    let quote = |value: &str| -> String {
        if value.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        }
    };
    out.push_str(&columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(","));
    out.push_str("\r\n");
    if let Some(rows) = payload.get("rows").and_then(|r| r.as_arr()) {
        for row in rows {
            let values: Vec<String> = row
                .get("cells")
                .and_then(|c| c.as_arr())
                .map(|list| list.iter().map(|c| quote(c.str_of("value").unwrap_or(""))).collect())
                .unwrap_or_default();
            out.push_str(&values.join(","));
            out.push_str("\r\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::{CellValue, Row};

    fn cell(col: usize, header: &str, value: &str, row: u32) -> Cell {
        let number = value.parse::<f64>().ok();
        Cell {
            col: col as u32 + 1,
            header: header.to_string(),
            value: match (&number, value.is_empty()) {
                (Some(n), _) => CellValue::Number(*n),
                (None, true) => CellValue::Empty,
                (None, false) => CellValue::Text(value.to_string()),
            },
            text: value.to_string(),
            reference: format!("{}{}", column_letters(col), row),
            unit: String::new(),
            formula: None,
        }
    }

    fn source() -> Source {
        Source {
            id: "src-1".into(),
            notebook_id: "nb".into(),
            title: "sales.xlsx".into(),
            kind: "xlsx".into(),
            status: "ready".into(),
            ..Default::default()
        }
    }

    fn grid() -> Table {
        let mut t = Table {
            id: "tbl-1".into(),
            source_id: "src-1".into(),
            notebook_id: "nb".into(),
            sheet: "Sales".into(),
            hidden: false,
            headers: vec!["Region".into(), "Revenue".into(), "Notes".into()],
            rows: vec![],
            truncated: false,
            warnings: vec![],
        };
        for (n, (region, revenue, note)) in [
            ("North", "1250", "steady, with a comma"),
            ("South", "980", ""),
            ("North", "3100", "promo \"Q3\""),
        ]
        .iter()
        .enumerate()
        {
            let row = n as u32 + 2;
            t.rows.push(Row {
                number: row,
                cells: vec![
                    cell(0, "Region", region, row),
                    cell(1, "Revenue", revenue, row),
                    cell(2, "Notes", note, row),
                ],
            });
        }
        t
    }

    #[test]
    fn a_projection_keeps_the_a1_reference_of_every_value() {
        let spec = Spec { table_id: "tbl-1".into(), ..Default::default() };
        let (title, payload) = build(&spec, &[grid()], &[source()]).expect("builds");
        assert_eq!(title, "Sales · 3 rows");
        assert_eq!(payload.str_of("source_title"), Some("sales.xlsx"));
        assert_eq!(payload.get("row_count").and_then(|v| v.as_f64()), Some(3.0));
        let rows = payload.get("rows").and_then(|r| r.as_arr()).unwrap();
        let first = &rows[0];
        assert_eq!(first.str_of("ref"), Some("A2:C2"), "the row range is the evidence");
        let cells = first.get("cells").and_then(|c| c.as_arr()).unwrap();
        assert_eq!(cells[0].str_of("ref"), Some("A2"));
        assert_eq!(cells[1].str_of("value"), Some("1250"));
        // The real row number from the sheet is preserved, not renumbered from 1.
        assert_eq!(first.get("number").and_then(|v| v.as_f64()), Some(2.0));
    }

    #[test]
    fn columns_can_be_selected_and_are_kept_in_sheet_order() {
        let spec = Spec {
            table_id: "tbl-1".into(),
            columns: vec!["Revenue".into(), "Region".into()],
            ..Default::default()
        };
        let (_, payload) = build(&spec, &[grid()], &[source()]).expect("builds");
        let names: Vec<String> = payload
            .get("columns")
            .and_then(|c| c.as_arr())
            .unwrap()
            .iter()
            .map(|c| c.str_of("name").unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["Region", "Revenue"], "selection does not reorder the sheet");
        let cells = payload.get("rows").and_then(|r| r.as_arr()).unwrap()[0]
            .get("cells")
            .and_then(|c| c.as_arr())
            .unwrap();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[1].str_of("ref"), Some("B2"), "the reference follows the real column");
    }

    #[test]
    fn filters_use_real_values_and_report_what_they_excluded() {
        let spec = Spec {
            table_id: "tbl-1".into(),
            filter_column: "Region".into(),
            filter_op: "eq".into(),
            filter_value: "north".into(),
            ..Default::default()
        };
        let (_, payload) = build(&spec, &[grid()], &[source()]).expect("builds");
        assert_eq!(payload.get("row_count").and_then(|v| v.as_f64()), Some(2.0));
        assert_eq!(payload.get("rows_read").and_then(|v| v.as_f64()), Some(3.0));
        assert_eq!(payload.get("filtered_out").and_then(|v| v.as_f64()), Some(1.0));
        assert_eq!(
            payload.get("filter").and_then(|f| f.str_of("value")),
            Some("north"),
            "the filter is recorded as asked, not normalised away"
        );

        // Numeric comparison works on numbers, not on lexicographic text.
        let spec = Spec {
            table_id: "tbl-1".into(),
            filter_column: "Revenue".into(),
            filter_op: "gt".into(),
            filter_value: "1000".into(),
            ..Default::default()
        };
        let (_, payload) = build(&spec, &[grid()], &[source()]).expect("builds");
        assert_eq!(payload.get("row_count").and_then(|v| v.as_f64()), Some(2.0));
    }

    #[test]
    fn an_empty_projection_is_refused_with_the_reason() {
        let spec = Spec {
            table_id: "tbl-1".into(),
            filter_column: "Region".into(),
            filter_op: "eq".into(),
            filter_value: "Mars".into(),
            ..Default::default()
        };
        let err = build(&spec, &[grid()], &[source()]).unwrap_err();
        assert!(err.contains("projection is empty"), "{}", err);
        assert!(err.contains("Sales"), "{}", err);
    }

    #[test]
    fn bad_requests_are_named_rather_than_guessed() {
        assert!(build(&Spec::default(), &[grid()], &[source()]).unwrap_err().contains("table_id"));
        let missing = Spec { table_id: "nope".into(), ..Default::default() };
        assert!(build(&missing, &[grid()], &[source()]).unwrap_err().contains("no stored grid"));
        let column = Spec { table_id: "tbl-1".into(), columns: vec!["Profit".into()], ..Default::default() };
        assert!(build(&column, &[grid()], &[source()]).unwrap_err().contains("no column \"Profit\""));
        let op = Spec {
            table_id: "tbl-1".into(),
            filter_column: "Region".into(),
            filter_op: "like".into(),
            filter_value: "x".into(),
            ..Default::default()
        };
        assert!(build(&op, &[grid()], &[source()]).unwrap_err().contains("unknown filter op"));
        let numeric = Spec {
            table_id: "tbl-1".into(),
            filter_column: "Revenue".into(),
            filter_op: "gt".into(),
            filter_value: "lots".into(),
            ..Default::default()
        };
        assert!(build(&numeric, &[grid()], &[source()]).unwrap_err().contains("numeric filter_value"));
    }

    #[test]
    fn truncation_is_reported_and_the_limit_is_capped() {
        let mut table = grid();
        for n in 0..20u32 {
            let row = n + 5;
            table.rows.push(Row {
                number: row,
                cells: vec![
                    cell(0, "Region", "East", row),
                    cell(1, "Revenue", "10", row),
                    cell(2, "Notes", "", row),
                ],
            });
        }
        let spec = Spec { table_id: "tbl-1".into(), limit: 4, ..Default::default() };
        let (_, payload) = build(&spec, &[table.clone()], &[source()]).expect("builds");
        assert_eq!(payload.get("row_count").and_then(|v| v.as_f64()), Some(4.0));
        assert_eq!(payload.get("truncated"), Some(&Value::Bool(true)));

        let huge = Spec { table_id: "tbl-1".into(), limit: 100_000, ..Default::default() };
        let (_, payload) = build(&huge, &[table], &[source()]).expect("builds");
        assert_eq!(payload.get("limit").and_then(|v| v.as_f64()), Some(ROW_LIMIT_MAX as f64));
    }

    #[test]
    fn empty_cells_are_counted_not_invented() {
        let spec = Spec { table_id: "tbl-1".into(), ..Default::default() };
        let (_, payload) = build(&spec, &[grid()], &[source()]).expect("builds");
        assert_eq!(payload.get("empty_cells").and_then(|v| v.as_f64()), Some(1.0));
        let south = &payload.get("rows").and_then(|r| r.as_arr()).unwrap()[1];
        let cells = south.get("cells").and_then(|c| c.as_arr()).unwrap();
        assert_eq!(cells[2].str_of("value"), Some(""), "an empty cell stays empty");
        assert_eq!(cells[2].str_of("ref"), Some("C3"), "and still points at the cell it was read from");
    }

    #[test]
    fn the_csv_export_is_rfc_4180_and_carries_exactly_the_projected_values() {
        let spec = Spec { table_id: "tbl-1".into(), ..Default::default() };
        let (_, payload) = build(&spec, &[grid()], &[source()]).expect("builds");
        let csv = to_csv(&payload);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines[0], "Region,Revenue,Notes");
        assert_eq!(lines[1], "North,1250,\"steady, with a comma\"");
        assert_eq!(lines[2], "South,980,");
        assert_eq!(lines[3], "North,3100,\"promo \"\"Q3\"\"\"");
        assert_eq!(lines[4], "", "the file ends with a line break");
    }

    #[test]
    fn column_letters_follow_spreadsheet_convention() {
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(25), "Z");
        assert_eq!(column_letters(26), "AA");
        assert_eq!(column_letters(51), "AZ");
        assert_eq!(column_letters(52), "BA");
        assert_eq!(column_letters(701), "ZZ");
        assert_eq!(column_letters(702), "AAA");
    }
}
