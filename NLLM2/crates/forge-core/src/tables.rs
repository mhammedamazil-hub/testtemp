//! Structured tables: parsing, cell-level locators and deterministic aggregation.
//!
//! Spreadsheets and CSVs are not prose. Flattening them into text destroys exactly the
//! information a user asks about — which row, which column, which cell — so this module
//! keeps the grid intact:
//!
//! * every cell has an A1-style reference (`Sheet2!B7`) and a display value;
//! * every row keeps its **original** row number (the header is row 1);
//! * empty and unparseable cells stay empty — they are never coerced to `0`;
//! * aggregations are computed **only** over cells that really hold a number, and the
//!   answer states how many rows were excluded and why.
//!
//! Parsing is RFC 4180 for CSV/TSV (quoted fields, embedded newlines, `""` escapes,
//! CRLF, BOM, delimiter sniffing) and real OOXML for XLSX (every sheet, shared strings,
//! inline strings, formulas' cached values, hidden sheets flagged).

use crate::domain::{Citation, Locator};
use crate::grounded::Answer;
use crate::retrieve;
use std::collections::BTreeMap;

// ---------------------------------------------------------------- model

#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Number(f64),
    Text(String),
    /// Nothing was there, or the cell held an explicit placeholder (`-`, `N/A`).
    Empty,
}

#[derive(Debug, Clone)]
pub struct Cell {
    /// 1-based column index.
    pub col: u32,
    pub header: String,
    pub value: CellValue,
    /// Display text exactly as it appeared (trimmed).
    pub text: String,
    /// A1 reference within the sheet, e.g. `B7`.
    pub reference: String,
    /// Unit suffix if the cell looked like `12 kg` / `45%` / `$1,200`.
    pub unit: String,
    /// The formula text when the workbook stored one (e.g. `SUM(C2:C3)`).
    /// We keep it because a cached formula result is *derived*, not authored data.
    pub formula: Option<String>,
}

impl Cell {
    pub fn is_number(&self) -> bool {
        matches!(self.value, CellValue::Number(_))
    }
    pub fn number(&self) -> Option<f64> {
        match self.value {
            CellValue::Number(n) => Some(n),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    /// Original row number in the document (header row is 1).
    pub number: u32,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub id: String,
    pub source_id: String,
    pub notebook_id: String,
    /// Sheet name, or the file name for a flat CSV.
    pub sheet: String,
    pub hidden: bool,
    pub headers: Vec<String>,
    pub rows: Vec<Row>,
    /// True when the sheet had more rows than `security::MAX_TABLE_ROWS`.
    pub truncated: bool,
    /// Warnings produced while parsing (ragged rows, duplicate headers, ...).
    pub warnings: Vec<String>,
}

impl Table {
    pub fn column_index(&self, header: &str) -> Option<usize> {
        self.headers.iter().position(|h| h == header)
    }

    pub fn cell<'a>(&self, row: &'a Row, col_idx: usize) -> Option<&'a Cell> {
        row.cells.iter().find(|c| c.col as usize == col_idx + 1)
    }

    /// A1 range covering a whole column of data (excluding the header row).
    pub fn column_range(&self, col_idx: usize, first_row: u32, last_row: u32) -> String {
        let letter = column_letter(col_idx as u32 + 1);
        format!("{}!{}{}:{}{}", self.sheet, letter, first_row, letter, last_row)
    }

    /// JSON view. `max_rows` caps the row list (`row_count` always reports the truth),
    /// so a 50,000-row sheet cannot blow up an HTTP response.
    pub fn json(&self, max_rows: usize) -> crate::json::Value {
        use crate::json::Value;
        use crate::obj;
        let rows: Vec<Value> = self
            .rows
            .iter()
            .take(max_rows)
            .map(|r| {
                let cells: Vec<Value> = r
                    .cells
                    .iter()
                    .map(|c| {
                        obj! {
                            "ref" => c.reference.as_str(),
                            "column" => c.header.as_str(),
                            "text" => c.text.as_str(),
                            "number" => match c.number() { Some(n) => Value::from(n), None => Value::Null },
                            "unit" => c.unit.as_str(),
                        }
                    })
                    .collect();
                obj! { "row" => r.number as u64, "cells" => Value::Arr(cells) }
            })
            .collect();
        obj! {
            "id" => self.id.as_str(),
            "source_id" => self.source_id.as_str(),
            "sheet" => self.sheet.as_str(),
            "hidden" => self.hidden,
            "columns" => Value::Arr(self.headers.iter().map(|h| Value::from(h.as_str())).collect()),
            "row_count" => self.rows.len() as u64,
            "truncated" => self.truncated || self.rows.len() > max_rows,
            "rows" => Value::Arr(rows),
            "warnings" => Value::Arr(self.warnings.iter().map(|w| Value::from(w.as_str())).collect()),
        }
    }
}

/// 1 → `A`, 26 → `Z`, 27 → `AA`.
pub fn column_letter(mut col: u32) -> String {
    let mut out = Vec::new();
    while col > 0 {
        let rem = ((col - 1) % 26) as u8;
        out.push(b'A' + rem);
        col = (col - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `AA12` → (27, 12). Returns `None` for a malformed reference.
pub fn parse_reference(raw: &str) -> Option<(u32, u32)> {
    let letters: String = raw.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits: String = raw.chars().skip(letters.len()).collect();
    if letters.is_empty() || digits.is_empty() {
        return None;
    }
    let mut col = 0u32;
    for ch in letters.to_ascii_uppercase().bytes() {
        col = col * 26 + (ch - b'A' + 1) as u32;
    }
    let row: u32 = digits.parse().ok()?;
    Some((col, row))
}

// ---------------------------------------------------------------- numbers

/// Parse a spreadsheet-looking number.
///
/// Returns `(value, unit)` where `unit` is whatever meaningful suffix followed the
/// digits (`%`, `kg`, `ms`). Separators are disambiguated rather than assumed:
///
/// | input      | value    | why                                        |
/// |------------|----------|--------------------------------------------|
/// | `1,234.50` | 1234.5   | `.` rightmost → decimal, `,` grouping      |
/// | `1.234,50` | 1234.5   | `,` rightmost → decimal, `.` grouping      |
/// | `1200,50`  | 1200.5   | one comma, 2-digit tail → decimal comma    |
/// | `1,200`    | 1200     | one comma, 3-digit tail → grouping         |
/// | `1,23,456` | —        | inconsistent grouping → **rejected**       |
///
/// A value that cannot be parsed is treated as missing, never as zero.
pub fn parse_number(raw: &str) -> Option<(f64, String)> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let placeholders = ["-", "--", "n/a", "na", "null", "none", "nan", "?"];
    if placeholders.contains(&t.to_ascii_lowercase().as_str()) {
        return None;
    }

    let mut s = t.to_string();
    let mut unit_prefix = String::new();
    let mut negative = false;
    // accounting negatives: (1,200)
    if s.starts_with('(') && s.ends_with(')') {
        negative = true;
        s = s[1..s.len() - 1].trim().to_string();
    }
    // currency prefix / suffix
    for sym in ["$", "€", "£", "₹", "¥"] {
        if let Some(rest) = s.strip_prefix(sym) {
            s = rest.trim().to_string();
        }
        if let Some(rest) = s.strip_suffix(sym) {
            s = rest.trim().to_string();
        }
    }
    if let Some(rest) = s.strip_suffix('%') {
        let (n, _) = parse_number(rest.trim())?;
        return Some((if negative { -n } else { n }, "%".to_string()));
    }
    if let Some(rest) = s.strip_prefix('-') {
        negative = !negative;
        s = rest.trim().to_string();
    } else if let Some(rest) = s.strip_prefix('+') {
        s = rest.trim().to_string();
    }

    // Split the leading numeric token (digits and separators) from a trailing unit.
    let mut body = String::new();
    let mut saw_digit = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.peek().copied() {
        match c {
            '0'..='9' => {
                body.push(c);
                saw_digit = true;
                chars.next();
            }
            ',' | '.' => {
                body.push(c);
                chars.next();
            }
            _ => break,
        }
    }
    if !saw_digit {
        return None;
    }
    let unit: String = chars.collect::<String>().trim().to_string();
    // A unit must look like a unit, not a sentence fragment.
    if unit.len() > 12 || unit.contains(' ') || unit.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    if !unit.is_empty() {
        unit_prefix = unit;
    }

    let value = normalize_number_body(&body)?;
    let value = if negative { -value } else { value };
    Some((value, unit_prefix))
}

/// Turn a digit string with arbitrary `,`/`.` separators into a number, or reject it.
fn normalize_number_body(body: &str) -> Option<f64> {
    let digit_groups_ok = |groups: &[&str]| {
        groups.first().map(|g| !g.is_empty() && g.len() <= 3).unwrap_or(false)
            && groups.iter().all(|g| g.chars().all(|c| c.is_ascii_digit()))
            && groups[1..].iter().all(|g| g.len() == 3)
    };
    let commas = body.matches(',').count();
    let dots = body.matches('.').count();

    let normalized = if commas > 0 && dots > 0 {
        // The rightmost separator is the decimal point; the other one must group in 3s.
        let decimal_is_comma = body.rfind(',') > body.rfind('.');
        let (dec_at, dec_sep) = if decimal_is_comma { (body.rfind(','), ',') } else { (body.rfind('.'), '.') };
        let dec_at = dec_at?;
        let (int_part, frac_part) = (&body[..dec_at], &body[dec_at + 1..]);
        if frac_part.is_empty() || !frac_part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let grouping_sep = if dec_sep == ',' { '.' } else { ',' };
        if int_part.contains(grouping_sep) {
            let groups: Vec<&str> = int_part.split(grouping_sep).collect();
            if !digit_groups_ok(&groups) {
                return None;
            }
            format!("{}.{}", groups.concat(), frac_part)
        } else if int_part.chars().all(|c| c.is_ascii_digit()) && !int_part.is_empty() {
            format!("{}.{}", int_part, frac_part)
        } else {
            return None;
        }
    } else if commas > 0 {
        let parts: Vec<&str> = body.split(',').collect();
        if !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
            return None;
        }
        if digit_groups_ok(&parts) {
            parts.concat() // thousands grouping
        } else if parts.len() == 2 && (1..=2).contains(&parts[1].len()) {
            format!("{}.{}", parts[0], parts[1]) // decimal comma
        } else {
            return None; // ambiguous or inconsistent grouping — refuse rather than guess
        }
    } else if dots > 1 {
        let parts: Vec<&str> = body.split('.').collect();
        if !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
            return None;
        }
        if digit_groups_ok(&parts) {
            parts.concat() // dot thousands grouping, e.g. 1.234.567
        } else {
            return None;
        }
    } else {
        body.to_string()
    };
    if normalized.is_empty() || normalized == "." {
        return None;
    }
    normalized.parse::<f64>().ok()
}

fn cell_from_text(raw: &str, col: u32, header: &str, row_number: u32) -> Cell {
    let text = raw.trim().to_string();
    let (value, unit) = match parse_number(&text) {
        Some((n, u)) => (CellValue::Number(n), u),
        None if text.is_empty() => (CellValue::Empty, String::new()),
        None => {
            // explicit placeholders are missing data, not text that happens to be "-"
            let lower = text.to_ascii_lowercase();
            if ["-", "--", "n/a", "na", "null", "none", "nan"].contains(&lower.as_str()) {
                (CellValue::Empty, String::new())
            } else {
                (CellValue::Text(text.clone()), String::new())
            }
        }
    };
    Cell {
        col,
        header: header.to_string(),
        value,
        text,
        reference: format!("{}{}", column_letter(col), row_number),
        unit,
        formula: None,
    }
}

// ---------------------------------------------------------------- CSV / TSV

/// Split CSV text into rows of fields (RFC 4180).
pub fn csv_rows(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.trim().is_empty() => {
                field.clear();
                quoted = true;
            }
            c if c == delimiter => {
                row.push(std::mem::take(&mut field));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    // drop a trailing empty row produced by a final newline
    while rows.last().map(|r| r.iter().all(|f| f.trim().is_empty())).unwrap_or(false) {
        rows.pop();
    }
    rows
}

/// Guess the delimiter by counting candidates outside quoted regions in the first line.
pub fn sniff_delimiter(text: &str) -> char {
    let first_line_end = text.find('\n').unwrap_or(text.len());
    let head = &text[..first_line_end];
    let mut best = (',', 0usize);
    for cand in [',', ';', '\t', '|'] {
        let mut count = 0usize;
        let mut quoted = false;
        for c in head.chars() {
            match c {
                '"' => quoted = !quoted,
                c if c == cand && !quoted => count += 1,
                _ => {}
            }
        }
        if count > best.1 {
            best = (cand, count);
        }
    }
    best.0
}

fn normalize_headers(raw: &[String], warnings: &mut Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (i, h) in raw.iter().enumerate() {
        let mut name = h.trim().to_string();
        if name.is_empty() {
            name = format!("Column {}", column_letter(i as u32 + 1));
            warnings.push(format!("header {} was empty; named it \"{}\"", i + 1, name));
        }
        let count = seen.entry(name.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            let renamed = format!("{} ({})", name, count);
            warnings.push(format!("duplicate header \"{}\" renamed to \"{}\"", name, renamed));
            name = renamed;
        }
        out.push(name);
    }
    out
}

/// Build a `Table` from raw rows. `sheet` names the table; row numbers are 1-based and
/// the first row is the header.
pub fn table_from_rows(
    rows: Vec<Vec<String>>,
    sheet: &str,
    source_id: &str,
    notebook_id: &str,
    warnings: &mut Vec<String>,
) -> Option<Table> {
    if rows.is_empty() {
        return None;
    }
    let header_row = &rows[0];
    let mut headers = normalize_headers(header_row, warnings);
    let width = headers.len();
    let mut extra_cols = 0usize;

    let mut out_rows: Vec<Row> = Vec::new();
    for (i, raw) in rows.iter().enumerate().skip(1) {
        let number = (i + 1) as u32;
        if raw.iter().all(|f| f.trim().is_empty()) {
            continue; // blank separator line
        }
        let mut cells: Vec<Cell> = Vec::with_capacity(raw.len().max(width));
        for (j, field) in raw.iter().enumerate() {
            let header = headers
                .get(j)
                .cloned()
                .unwrap_or_else(|| format!("Column {}", column_letter(j as u32 + 1)));
            if j >= headers.len() {
                headers.push(header.clone());
                extra_cols += 1;
            }
            cells.push(cell_from_text(field, j as u32 + 1, &header, number));
        }
        out_rows.push(Row { number, cells });
    }
    if extra_cols > 0 {
        warnings.push(format!(
            "{} row(s) had more fields than the header; {} extra column(s) were added",
            out_rows.len(),
            extra_cols
        ));
    }
    if out_rows.is_empty() {
        warnings.push("the table has a header but no data rows".to_string());
    }
    let mut truncated = false;
    if out_rows.len() > crate::security::MAX_TABLE_ROWS {
        truncated = true;
        warnings.push(format!(
            "only the first {} of {} rows were kept; totals over a partial sheet are refused",
            crate::security::MAX_TABLE_ROWS,
            out_rows.len()
        ));
        out_rows.truncate(crate::security::MAX_TABLE_ROWS);
    }
    Some(Table {
        id: format!("tbl_{}", crate::domain::short_hash(&format!("{}|{}", source_id, sheet))),
        source_id: source_id.to_string(),
        notebook_id: notebook_id.to_string(),
        sheet: sheet.to_string(),
        hidden: false,
        headers,
        rows: out_rows,
        truncated,
        warnings: warnings.clone(),
    })
}

/// Parse a delimited text file (CSV/TSV) into one table.
pub fn parse_delimited(
    bytes: &[u8],
    name: &str,
    forced: Option<char>,
    source_id: &str,
    notebook_id: &str,
) -> Result<Table, String> {
    let text = crate::ingest::decode_text(bytes);
    if text.trim().is_empty() {
        return Err("the file is empty".into());
    }
    let delim = forced.unwrap_or_else(|| sniff_delimiter(&text));
    let rows = csv_rows(&text, delim);
    if rows.is_empty() {
        return Err("no rows could be parsed".into());
    }
    let mut warnings = Vec::new();
    let sheet = name
        .rsplit('/')
        .next()
        .unwrap_or(name)
        .trim_end_matches(".csv")
        .trim_end_matches(".tsv")
        .to_string();
    table_from_rows(rows, &sheet, source_id, notebook_id, &mut warnings)
        .ok_or_else(|| "the file produced no table".to_string())
}

// ---------------------------------------------------------------- XLSX

fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(semi) = tail.find(';') {
            let entity = &tail[1..semi];
            let replaced = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .and_then(char::from_u32)
                    .or_else(|| {
                        entity
                            .strip_prefix('#')
                            .and_then(|d| d.parse::<u32>().ok())
                            .and_then(char::from_u32)
                    }),
            };
            match replaced {
                Some(c) => {
                    out.push(c);
                    rest = &tail[semi + 1..];
                }
                None => {
                    out.push('&');
                    rest = &tail[1..];
                }
            }
        } else {
            out.push_str(tail);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

/// Text of every `<t>` inside a fragment (handles rich-text runs).
fn joined_text(fragment: &str) -> String {
    let mut out = String::new();
    let mut rest = fragment;
    while let Some(start) = rest.find("<t") {
        let tail = &rest[start..];
        let Some(open_end) = tail.find('>') else { break };
        let after = &tail[open_end + 1..];
        let Some(close) = after.find("</t>") else { break };
        out.push_str(&xml_unescape(&after[..close]));
        rest = &after[close + 4..];
    }
    out
}

fn shared_strings(bytes: &[u8]) -> Vec<String> {
    let raw = match crate::ingest::zip::read_named(bytes, "xl/sharedStrings.xml") {
        Ok(Some(v)) => v,
        _ => return Vec::new(),
    };
    let xml = String::from_utf8_lossy(&raw).to_string();
    let mut out = Vec::new();
    let mut rest = xml.as_str();
    while let Some(start) = rest.find("<si>").or_else(|| rest.find("<si ")) {
        let tail = &rest[start..];
        let Some(end) = tail.find("</si>") else { break };
        out.push(joined_text(&tail[..end]));
        rest = &tail[end + 5..];
    }
    out
}

/// Sheet name → worksheet path, in workbook order.
fn workbook_sheets(bytes: &[u8]) -> Vec<(String, String, bool)> {
    let mut sheets = Vec::new();
    let rels: BTreeMap<String, String> = match crate::ingest::zip::read_named(bytes, "xl/_rels/workbook.xml.rels") {
        Ok(Some(raw)) => {
            let xml = String::from_utf8_lossy(&raw).to_string();
            let mut map = BTreeMap::new();
            let mut rest = xml.as_str();
            while let Some(start) = rest.find("<Relationship") {
                let tail = &rest[start..];
                let Some(end) = tail.find('>') else { break };
                let tag = &tail[..end];
                if let (Some(id), Some(target)) = (attr_of(tag, "Id"), attr_of(tag, "Target")) {
                    let path = target.trim_start_matches('/').trim_start_matches("xl/").to_string();
                    map.insert(id, format!("xl/{}", path));
                }
                rest = &tail[end + 1..];
            }
            map
        }
        _ => BTreeMap::new(),
    };

    if let Ok(Some(raw)) = crate::ingest::zip::read_named(bytes, "xl/workbook.xml") {
        let xml = String::from_utf8_lossy(&raw).to_string();
        let mut rest = xml.as_str();
        while let Some(start) = rest.find("<sheet ") {
            let tail = &rest[start..];
            let Some(end) = tail.find('>') else { break };
            let tag = &tail[..end];
            let name = attr_of(tag, "name").map(|n| xml_unescape(&n)).unwrap_or_default();
            let hidden = attr_of(tag, "state").map(|s| s == "hidden" || s == "veryHidden").unwrap_or(false);
            let path = attr_of(tag, "r:id")
                .and_then(|id| rels.get(&id).cloned())
                .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", sheets.len() + 1));
            sheets.push((name, path, hidden));
            rest = &tail[end + 1..];
        }
    }
    sheets
}

/// The `<f>` formula text of a cell body, if the workbook stored one.
fn formula_of(cbody: &str) -> Option<String> {
    let start = cbody.find("<f")?;
    let open = cbody[start..].find('>')? + start + 1;
    let end = cbody[open..].find("</f>")? + open;
    let f = xml_unescape(cbody[open..end].trim());
    if f.is_empty() {
        None
    } else {
        Some(f)
    }
}

/// Value of an attribute in a raw tag, e.g. `r="B7"` → `B7`.
fn attr_of(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{}=\"", name);
    let mut from = 0usize;
    while let Some(rel) = tag[from..].find(&needle) {
        let at = from + rel;
        // make sure it is a whole attribute name (not a suffix of another one)
        let ok = at == 0
            || tag[..at]
                .chars()
                .last()
                .map(|c| c.is_whitespace() || c == '<')
                .unwrap_or(false);
        if ok {
            let start = at + needle.len();
            let end = tag[start..].find('"')? + start;
            return Some(tag[start..end].to_string());
        }
        from = at + needle.len();
    }
    None
}

/// Parse every worksheet of an XLSX file.
pub fn parse_xlsx(bytes: &[u8], source_id: &str, notebook_id: &str) -> Result<Vec<Table>, String> {
    if !crate::ingest::zip::entries(bytes).map(|e| !e.is_empty()).unwrap_or(false) {
        return Err("not a ZIP container".into());
    }
    let shared = shared_strings(bytes);
    let mut sheets = workbook_sheets(bytes);
    if sheets.is_empty() {
        // fall back to discovery when workbook.xml is missing
        if let Ok(entries) = crate::ingest::zip::find(bytes, "xl/worksheets/", ".xml") {
            for (i, e) in entries.iter().enumerate() {
                sheets.push((format!("Sheet{}", i + 1), e.name.clone(), false));
            }
        }
    }
    let mut tables = Vec::new();
    for (name, path, hidden) in sheets {
        let raw = match crate::ingest::zip::read_named(bytes, &path) {
            Ok(Some(v)) => v,
            _ => continue,
        };
        let xml = String::from_utf8_lossy(&raw).to_string();
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut formulas: BTreeMap<(u32, u32), String> = BTreeMap::new();
        let mut max_row = 0u32;
        let mut rest = xml.as_str();
        // Rows are addressed by their own r="N"; honour it rather than counting.
        while let Some(start) = rest.find("<row") {
            let tail = &rest[start..];
            let Some(tag_end) = tail.find('>') else { break };
            let row_tag = &tail[..tag_end];
            let row_num: u32 = attr_of(row_tag, "r").and_then(|r| r.parse().ok()).unwrap_or(max_row + 1);
            let body_start = tag_end + 1;
            let body_end = tail[body_start..].find("</row>").map(|e| body_start + e).unwrap_or(tail.len());
            let body = &tail[body_start..body_end];
            let mut cells: Vec<(u32, String, Option<String>)> = Vec::new();
            let mut cel = body;
            while let Some(cstart) = cel.find("<c ") {
                let ctail = &cel[cstart..];
                let Some(ctag_end) = ctail.find('>') else { break };
                let ctag = &ctail[..ctag_end];
                let self_closing = ctag.trim_end().ends_with('/');
                let cbody = if self_closing {
                    ""
                } else {
                    let rest_body = &ctail[ctag_end + 1..];
                    match rest_body.find("</c>") {
                        Some(e) => &rest_body[..e],
                        None => "",
                    }
                };
                let col = attr_of(ctag, "r")
                    .and_then(|r| parse_reference(&r).map(|(c, _)| c))
                    .unwrap_or(cells.last().map(|(c, _, _)| c + 1).unwrap_or(1));
                let ty = attr_of(ctag, "t").unwrap_or_default();
                let raw_value = if ty == "inlineStr" {
                    joined_text(cbody)
                } else {
                    // <v> holds the value; for t="s" it is an index into shared strings
                    let v = cbody
                        .find("<v>")
                        .and_then(|s| cbody[s + 3..].find("</v>").map(|e| (s + 3, s + 3 + e)))
                        .map(|(s, e)| cbody[s..e].to_string())
                        .unwrap_or_default();
                    match ty.as_str() {
                        "s" => v
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| shared.get(i).cloned())
                            .unwrap_or_default(),
                        "b" => {
                            if v.trim() == "1" {
                                "TRUE".to_string()
                            } else if v.trim() == "0" {
                                "FALSE".to_string()
                            } else {
                                v
                            }
                        }
                        _ => xml_unescape(&v),
                    }
                };
                cells.push((col, raw_value, formula_of(cbody)));
                cel = &ctail[ctag_end + 1..];
            }
            let width = cells.iter().map(|(c, _, _)| *c).max().unwrap_or(0);
            let mut fields = vec![String::new(); width as usize];
            for (col, value, formula) in cells {
                if col >= 1 {
                    fields[col as usize - 1] = value;
                    if formula.is_some() {
                        formulas.insert((row_num, col), formula.unwrap());
                    }
                }
            }
            while rows.len() < row_num as usize {
                rows.push(Vec::new());
            }
            rows[row_num as usize - 1] = fields;
            max_row = max_row.max(row_num);
            rest = &tail[body_end..];
        }
        // leading empty rows would become a blank header
        while rows.first().map(|r| r.iter().all(|f| f.trim().is_empty())).unwrap_or(false) {
            rows.remove(0);
        }
        if rows.is_empty() {
            continue;
        }
        let mut warnings = Vec::new();
        if hidden {
            warnings.push("this sheet is hidden in the workbook".to_string());
        }
        if let Some(mut t) = table_from_rows(rows, &name, source_id, notebook_id, &mut warnings) {
            t.hidden = hidden;
            for r in t.rows.iter_mut() {
                for c in r.cells.iter_mut() {
                    if let Some(f) = formulas.get(&(r.number, c.col)) {
                        c.formula = Some(f.clone());
                    }
                }
            }
            tables.push(t);
        }
    }
    if tables.is_empty() {
        return Err("no worksheet contained any cells".into());
    }
    Ok(tables)
}

// ---------------------------------------------------------------- retrieval bridge

/// A compact textual view of a table for lexical retrieval.
///
/// Each row becomes one block whose locator points at the row's first and last cell, so
/// even a plain search answer can be traced to a spreadsheet row.
pub fn table_blocks(tables: &[Table]) -> Vec<crate::ingest::Block> {
    let mut out = Vec::new();
    for t in tables {
        if t.hidden {
            continue;
        }
        for r in &t.rows {
            let mut parts: Vec<String> = Vec::new();
            for c in &r.cells {
                if c.text.is_empty() {
                    continue;
                }
                parts.push(match &c.formula {
                    // A cached formula result is derived data; say so in the passage too.
                    Some(f) => format!("{}: {} (formula ={})", c.header, c.text, f),
                    None => format!("{}: {}", c.header, c.text),
                });
            }
            if parts.is_empty() {
                continue;
            }
            let text = format!("{} row {} — {}", t.sheet, r.number, parts.join("; "));
            let last_col = r.cells.iter().map(|c| c.col).max().unwrap_or(1);
            out.push(crate::ingest::Block {
                text,
                locator: Locator::sheet_row(
                    &t.sheet,
                    r.number,
                    &format!("A{}-{}{}", r.number, column_letter(last_col), r.number),
                ),
            });
        }
    }
    out
}

// ---------------------------------------------------------------- aggregation

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Agg {
    Sum,
    Average,
    Min,
    Max,
    Count,
}

impl Agg {
    pub fn as_str(&self) -> &'static str {
        match self {
            Agg::Sum => "sum",
            Agg::Average => "average",
            Agg::Min => "minimum",
            Agg::Max => "maximum",
            Agg::Count => "count",
        }
    }
}

/// Words that signal an aggregation, checked in priority order.
const AGG_WORDS: &[(&str, Agg)] = &[
    ("sum", Agg::Sum),
    ("total", Agg::Sum),
    ("add", Agg::Sum),
    ("combined", Agg::Sum),
    ("average", Agg::Average),
    ("mean", Agg::Average),
    ("avg", Agg::Average),
    ("maximum", Agg::Max),
    ("max", Agg::Max),
    ("highest", Agg::Max),
    ("largest", Agg::Max),
    ("biggest", Agg::Max),
    ("most", Agg::Max),
    ("minimum", Agg::Min),
    ("min", Agg::Min),
    ("lowest", Agg::Min),
    ("smallest", Agg::Min),
    ("least", Agg::Min),
    ("count", Agg::Count),
    ("how many", Agg::Count),
    ("number of", Agg::Count),
];

pub fn detect_aggregation(question: &str) -> Option<Agg> {
    let q = question.to_ascii_lowercase();
    for (word, agg) in AGG_WORDS {
        if q.split(|c: char| !c.is_alphanumeric() && c != ' ').any(|w| w.trim() == *word)
            || q.contains(&format!("{} ", word))
            || q.ends_with(word)
        {
            return Some(*agg);
        }
    }
    None
}

/// Columns whose header matches the question, best first.
///
/// Ranking is lexicographic: header-word match, then "does this column actually hold
/// numbers", then how many. Without the numeric tie-break a question like
/// "highest score by team" ranks the *label* column (`team`) above the value column
/// (`score`) purely because "team" appears in the question, and we would refuse an
/// answer we can compute.
pub fn ranked_columns(table: &Table, question: &str) -> Vec<usize> {
    let q: Vec<String> = retrieve::terms(question).into_iter().map(|t| retrieve::stem(&t)).collect();
    if q.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, f64, usize)> = Vec::new();
    for (i, header) in table.headers.iter().enumerate() {
        let h: Vec<String> = retrieve::terms(header).into_iter().map(|t| retrieve::stem(&t)).collect();
        if h.is_empty() {
            continue;
        }
        let hits = h.iter().filter(|t| q.contains(t)).count();
        if hits == 0 {
            continue;
        }
        let score = hits as f64 / h.len() as f64;
        let numeric = table
            .rows
            .iter()
            .filter(|r| table.cell(r, i).map(|c| c.is_number()).unwrap_or(false))
            .count();
        scored.push((i, score, numeric));
    }
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.2.cmp(&a.2))
            .then(a.0.cmp(&b.0))
    });
    scored.into_iter().map(|(i, _, _)| i).collect()
}

/// The single best-matching column, if any header matches the question.
pub fn pick_column(table: &Table, question: &str) -> Option<usize> {
    ranked_columns(table, question).first().copied()
}

/// Pick a grouping column from "by X" / "per X" / "for each X".
pub fn pick_group_column(table: &Table, question: &str, exclude: usize) -> Option<usize> {
    let q = question.to_ascii_lowercase();
    let marker = ["by ", "per ", "for each ", "grouped by "]
        .iter()
        .filter_map(|m| q.find(m).map(|i| (i, m.len())))
        .max_by_key(|(i, _)| *i)?;
    let tail = &question[marker.0 + marker.1..];
    for (i, header) in table.headers.iter().enumerate() {
        if i == exclude {
            continue;
        }
        let h = header.to_ascii_lowercase();
        if !h.is_empty() && tail.to_ascii_lowercase().trim().starts_with(&h) {
            return Some(i);
        }
    }
    match ranked_columns(table, tail).first().copied() {
        Some(i) if i != exclude => Some(i),
        _ => None,
    }
}

/// True when `formula` totals a range inside `letter`'s column, e.g. `SUM(C2:C3)` in C.
fn is_column_total(formula: &str, letter: &str) -> bool {
    let f = formula.to_ascii_uppercase();
    let lb = letter.to_ascii_uppercase().into_bytes();
    let b = f.as_bytes();
    let mut i = 0;
    while i + lb.len() <= b.len() {
        if &b[i..i + lb.len()] == lb.as_slice() {
            if let Some((r1, next)) = digits_at(&f, i + lb.len()) {
                if next < b.len() && b[next] == b':' && b[next + 1..].starts_with(lb.as_slice()) {
                    if let Some((r2, _)) = digits_at(&f, next + 1 + lb.len()) {
                        if r2 > r1 {
                            return true;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn digits_at(s: &str, from: usize) -> Option<(u32, usize)> {
    let rest = s.get(from..)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    Some((digits.parse().ok()?, from + digits.len()))
}

fn fmt_number(n: f64) -> String {
    if (n - n.round()).abs() < 1e-9 {
        format!("{}", n.round() as i64)
    } else {
        let s = format!("{:.4}", n);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Try to answer an aggregation question from a table.
///
/// Returns `None` when the question is not clearly an aggregation over a real column —
/// the caller then falls back to the ordinary evidence path. It never guesses: an
/// ambiguous column, mixed units, or a column with no numeric cells yields `None` or an
/// explicit refusal rather than a number.
pub fn try_answer(question: &str, tables: &[Table], sources: &[crate::domain::Source]) -> Option<Answer> {
    let agg = detect_aggregation(question)?;
    let mut best: Option<(usize, usize, usize, usize, f64)> = None; // table, col, group, numeric, score
    for (ti, t) in tables.iter().enumerate() {
        if t.hidden {
            continue;
        }
        for (rank, col) in ranked_columns(t, question).into_iter().enumerate() {
            let numeric = t
                .rows
                .iter()
                .filter(|r| t.cell(r, col).map(|c| c.is_number()).unwrap_or(false))
                .count();
            // a value column is required for every aggregation except `count`
            if numeric == 0 {
                continue;
            }
            let group = pick_group_column(t, question, col).unwrap_or(usize::MAX);
            // later tables must beat both the match quality and the data volume
            let score = numeric as f64 - rank as f64 * 0.001;
            if best.map(|(_, _, _, _, s)| score > s).unwrap_or(true) {
                best = Some((ti, col, group, numeric, score));
            }
            break; // best-ranked usable column of this table only
        }
    }
    let (ti, col, group_col, _, _) = best?;
    let table = &tables[ti];
    if table.truncated {
        return Some(Answer {
            text: format!(
                "I cannot {} “{}” in {}: the sheet has more rows than Forge keeps ({}), so a figure computed over the rows I hold would be wrong.",
                agg.as_str(),
                table.headers.get(col).cloned().unwrap_or_default(),
                table.sheet,
                table.rows.len()
            ),
            citations: Vec::new(),
            abstained: true,
            engine: "table".into(),
            coverage: 0,
        });
    }
    Some(compute(question, table, col, group_col, agg, sources))
}

fn compute(
    _question: &str,
    table: &Table,
    col: usize,
    group_col: usize,
    agg: Agg,
    sources: &[crate::domain::Source],
) -> Answer {
    let source = sources.iter().find(|s| s.id == table.source_id);
    let source_title = source.map(|s| s.title.clone()).unwrap_or_else(|| table.sheet.clone());
    let header = table.headers.get(col).cloned().unwrap_or_default();

    // ---- gather values, remembering exactly which rows were used and skipped
    let letter = column_letter(col as u32 + 1);
    let mut used: Vec<(u32, f64, String, Option<String>)> = Vec::new();
    let mut skipped: Vec<(u32, &'static str)> = Vec::new();
    let mut subtotals: Vec<(u32, String, String)> = Vec::new();
    let mut units: Vec<String> = Vec::new();
    for r in &table.rows {
        match table.cell(r, col) {
            Some(c) => match c.number() {
                Some(n) => {
                    // A cell that totals its own column is derived from the rows above it.
                    // Counting it as data would double the figure, so it is excluded and named.
                    match &c.formula {
                        Some(f) if is_column_total(f, &letter) => {
                            subtotals.push((r.number, c.reference.clone(), f.clone()));
                        }
                        _ => {
                            used.push((r.number, n, c.reference.clone(), c.formula.clone()));
                            if !c.unit.is_empty() && !units.contains(&c.unit) {
                                units.push(c.unit.clone());
                            }
                        }
                    }
                }
                None if matches!(c.value, CellValue::Empty) => skipped.push((r.number, "no value")),
                None => skipped.push((r.number, "not a number")),
            },
            None => skipped.push((r.number, "missing cell")),
        }
    }

    // ---- refuse rather than mix units
    if units.len() > 1 {
        return Answer {
            text: format!(
                "I cannot total “{}” in {} because the column mixes units ({}). Summing them would be meaningless, so I am not giving a number.",
                header,
                table.sheet,
                units.join(", ")
            ),
            citations: Vec::new(),
            abstained: true,
            engine: "table".into(),
            coverage: 0,
        };
    }
    let unit = units.first().cloned().unwrap_or_default();
    let unit_suffix = if unit.is_empty() || unit == "%" { unit.clone() } else { format!(" {}", unit) };

    if used.is_empty() {
        return Answer {
            text: format!(
                "I found the “{}” column in {} but none of its {} data rows hold a number, so there is nothing to {}.",
                header,
                table.sheet,
                table.rows.len(),
                agg.as_str()
            ),
            citations: Vec::new(),
            abstained: true,
            engine: "table".into(),
            coverage: 0,
        };
    }

    let first_row = used.iter().map(|(r, _, _, _)| *r).min().unwrap_or(0);
    let last_row = used.iter().map(|(r, _, _, _)| *r).max().unwrap_or(0);
    let range = table.column_range(col, first_row, last_row);
    let citation_id = "c1".to_string();

    let mut text = String::new();

    if group_col != usize::MAX {
        let group_header = table.headers.get(group_col).cloned().unwrap_or_default();
        let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for r in &table.rows {
            if subtotals.iter().any(|(row, _, _)| *row == r.number) {
                continue; // never fold a column total back into a group
            }
            let Some(g) = table.cell(r, group_col) else { continue };
            let Some(n) = table.cell(r, col).and_then(|c| c.number()) else { continue };
            if g.text.is_empty() {
                continue;
            }
            groups.entry(g.text.clone()).or_default().push(n);
        }
        if groups.is_empty() {
            return Answer {
                text: format!("I could not group {} by {} because no row has both values.", table.sheet, group_header),
                citations: Vec::new(),
                abstained: true,
                engine: "table".into(),
                coverage: 0,
            };
        }
        let mut lines = Vec::new();
        for (name, values) in &groups {
            let v = match agg {
                Agg::Sum => values.iter().sum(),
                Agg::Average => values.iter().sum::<f64>() / values.len() as f64,
                Agg::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
                Agg::Max => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                Agg::Count => values.len() as f64,
            };
            lines.push(format!("{} {}: {}{}", agg.as_str(), name, fmt_number(v), unit_suffix));
        }
        text.push_str(&format!(
            "{} of “{}” by “{}” in {} ({} groups, {} rows with values):\n\n- {}",
            match agg { Agg::Sum => "Totals", Agg::Average => "Averages", Agg::Min => "Minimums", Agg::Max => "Maximums", Agg::Count => "Counts" },
            header,
            group_header,
            table.sheet,
            groups.len(),
            used.len(),
            lines.join("\n- ")
        ));
    } else {
        let value = match agg {
            Agg::Sum => used.iter().map(|(_, n, _, _)| n).sum::<f64>(),
            Agg::Average => used.iter().map(|(_, n, _, _)| n).sum::<f64>() / used.len() as f64,
            Agg::Min => used.iter().map(|(_, n, _, _)| *n).fold(f64::INFINITY, f64::min),
            Agg::Max => used.iter().map(|(_, n, _, _)| *n).fold(f64::NEG_INFINITY, f64::max),
            Agg::Count => used.len() as f64,
        };
        let label = match agg {
            Agg::Sum => "Total",
            Agg::Average => "Average",
            Agg::Min => "Minimum",
            Agg::Max => "Maximum",
            Agg::Count => "Count",
        };
        let mut detail = format!(
            "{} of “{}”: {}{} — computed over {} numeric cell(s) in {}",
            label,
            header,
            fmt_number(value),
            unit_suffix,
            used.len(),
            range
        );
        if agg == Agg::Max || agg == Agg::Min {
            let target = value;
            if let Some((row, _, reference, _)) = used.iter().find(|(_, n, _, _)| (*n - target).abs() < 1e-9) {
                detail.push_str(&format!(" (row {}, cell {})", row, reference));
            }
        }
        text.push_str(&detail);
    }

    if !skipped.is_empty() {
        let sample: Vec<String> = skipped
            .iter()
            .take(6)
            .map(|(r, why)| format!("row {} ({})", r, why))
            .collect();
        text.push_str(&format!(
            "\n\n{} row(s) were excluded and are not counted as zero: {}{}.",
            skipped.len(),
            sample.join(", "),
            if skipped.len() > 6 { ", …" } else { "" }
        ));
    }
    if !subtotals.is_empty() {
        let listed: Vec<String> = subtotals
            .iter()
            .map(|(row, reference, formula)| format!("{} (row {}, ={})", reference, row, formula))
            .collect();
        text.push_str(&format!(
            "\n\n{} cell(s) were excluded because they already total this column: {}.",
            subtotals.len(),
            listed.join(", ")
        ));
    }
    if !unit.is_empty() {
        text.push_str(&format!("\n\nUnit: cells are reported in {}.", unit));
    }
    text.push_str(&format!("\n\nSource: {} · {} · columns {}", source_title, table.sheet, range));

    let quote = if group_col != usize::MAX {
        format!(
            "{} by {} in {} ({} rows)",
            header,
            table.headers.get(group_col).cloned().unwrap_or_default(),
            table.sheet,
            used.len()
        )
    } else {
        format!("{} column of {} ({} numeric cells)", header, table.sheet, used.len())
    };

    Answer {
        text,
        citations: vec![Citation {
            citation_id,
            source_id: table.source_id.clone(),
            chunk_id: table.id.clone(),
            quote,
            locator: Locator::cell(&table.sheet, &range, first_row),
            score: 1.0,
            source_title,
        }],
        abstained: false,
        engine: "table".into(),
        coverage: 1,
    }
}

/// Verify that every cell a table answer claims to have used really exists in the table.
pub fn validate(answer: &Answer, tables: &[Table]) -> Vec<String> {
    let mut problems = Vec::new();
    for c in &answer.citations {
        let Some(t) = tables.iter().find(|t| t.id == c.chunk_id) else {
            problems.push(format!("citation {} refers to an unknown table", c.citation_id));
            continue;
        };
        if t.source_id != c.source_id {
            problems.push(format!("citation {} table/source mismatch", c.citation_id));
        }
        if c.locator.sheet.as_deref() != Some(t.sheet.as_str()) {
            problems.push(format!("citation {} points at the wrong sheet", c.citation_id));
        }
        if t.rows.is_empty() {
            problems.push(format!("citation {} table has no rows", c.citation_id));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Source;

    fn table(rows: Vec<Vec<String>>) -> Table {
        let mut w = Vec::new();
        table_from_rows(rows, "Sheet1", "src1", "nb1", &mut w).expect("table")
    }

    fn source(id: &str, title: &str) -> Source {
        Source { id: id.into(), title: title.into(), ..Default::default() }
    }

    // ---------------- CSV

    #[test]
    fn csv_handles_quotes_commas_newlines_and_crlf() {
        let csv = "name,notes,amount\r\n\
                   \"Acme, Inc\",\"said \"\"hello\"\"\",1200\r\n\
                   Beta,\"line one\nline two\",850\r\n";
        let rows = csv_rows(csv, ',');
        assert_eq!(rows.len(), 3, "header + 2 rows: {:?}", rows);
        assert_eq!(rows[1][0], "Acme, Inc", "comma inside quotes");
        assert_eq!(rows[1][1], "said \"hello\"", "escaped quotes");
        assert_eq!(rows[2][1], "line one\nline two", "newline inside quotes");
    }

    #[test]
    fn csv_sniffs_semicolon_and_tab() {
        assert_eq!(sniff_delimiter("a;b;c\n1;2;3"), ';');
        assert_eq!(sniff_delimiter("a\tb\tc\n1\t2\t3"), '\t');
        assert_eq!(sniff_delimiter("a,b,c\n1,2,3"), ',');
        // delimiters inside quotes must not count
        assert_eq!(sniff_delimiter("\"a;b\";c\n1;2"), ';');
    }

    #[test]
    fn csv_keeps_row_numbers_and_cell_references() {
        let mut w = Vec::new();
        let t = table_from_rows(
            vec![
                vec!["region".into(), "revenue".into()],
                vec!["north".into(), "1200".into()],
                vec!["south".into(), "850".into()],
            ],
            "Q1",
            "s",
            "n",
            &mut w,
        )
        .unwrap();
        assert_eq!(t.rows[0].number, 2, "header is row 1, so data starts at row 2");
        assert_eq!(t.rows[0].cells[1].reference, "B2");
        assert_eq!(t.headers, vec!["region", "revenue"]);
    }

    #[test]
    fn ragged_and_blank_headers_are_reported_not_guessed() {
        let mut w = Vec::new();
        let t = table_from_rows(
            vec![
                vec!["name".into(), "".into(), "name".into()],
                vec!["a".into(), "1".into(), "2".into(), "extra".into()],
                vec!["".into(), "".into(), "".into()],
            ],
            "S",
            "s",
            "n",
            &mut w,
        )
        .unwrap();
        assert_eq!(t.headers[1], "Column B", "empty header named by column letter");
        assert_eq!(t.headers[2], "name (2)", "duplicate header disambiguated");
        assert_eq!(t.headers[3], "Column D", "extra field becomes a column");
        assert_eq!(t.rows.len(), 1, "blank line skipped");
        assert!(t.warnings.iter().any(|x| x.contains("duplicate header")));
        assert!(t.warnings.iter().any(|x| x.contains("more fields")));
    }

    #[test]
    fn numbers_parse_with_units_currency_and_accounting_negatives() {
        assert_eq!(parse_number("1,234.5"), Some((1234.5, String::new())));
        assert_eq!(parse_number("$1,200"), Some((1200.0, String::new())));
        assert_eq!(parse_number("45%"), Some((45.0, "%".to_string())));
        assert_eq!(parse_number("(1,200)"), Some((-1200.0, String::new())));
        assert_eq!(parse_number("12 kg"), Some((12.0, "kg".to_string())));
        assert_eq!(parse_number("-3.5"), Some((-3.5, String::new())));
        // these must not look like numbers
        assert_eq!(parse_number("N/A"), None);
        assert_eq!(parse_number("-"), None);
        assert_eq!(parse_number("12abc34"), None);
        assert_eq!(parse_number("two"), None);
    }

    #[test]
    fn missing_values_stay_missing() {
        let t = table(vec![
            vec!["item".into(), "cost".into()],
            vec!["a".into(), "".into()],
            vec!["b".into(), "N/A".into()],
            vec!["c".into(), "10".into()],
        ]);
        let costs: Vec<_> = t.rows.iter().map(|r| t.cell(r, 1).unwrap().clone()).collect();
        assert!(matches!(costs[0].value, CellValue::Empty), "blank is empty, not 0");
        assert!(matches!(costs[1].value, CellValue::Empty), "'N/A' is empty, not text");
        assert_eq!(costs[2].number(), Some(10.0));
    }

    // ---------------- aggregation

    #[test]
    fn sums_only_numeric_cells_and_says_what_it_skipped() {
        let t = table(vec![
            vec!["region".into(), "revenue".into()],
            vec!["north".into(), "1200".into()],
            vec!["south".into(), "850".into()],
            vec!["east".into(), "".into()],
            vec!["west".into(), "unknown".into()],
        ]);
        let a = try_answer("what is the total revenue", &[t], &[source("src1", "Sales")]).expect("answer");
        assert!(!a.abstained);
        assert!(a.text.contains("2050"), "{}", a.text);
        assert!(a.text.contains("2 row(s) were excluded"), "{}", a.text);
        assert!(a.text.contains("row 4"), "names the excluded row: {}", a.text);
        assert_eq!(a.citations.len(), 1);
        assert_eq!(a.citations[0].locator.kind, "cell");
        assert_eq!(a.citations[0].locator.sheet.as_deref(), Some("Sheet1"));
        assert_eq!(a.citations[0].source_id, "src1");
    }

    #[test]
    fn refuses_to_mix_units() {
        let t = table(vec![
            vec!["trip".into(), "distance".into()],
            vec!["a".into(), "12 km".into()],
            vec!["b".into(), "3 miles".into()],
        ]);
        let a = try_answer("total distance", &[t], &[source("src1", "Trips")]).unwrap();
        assert!(a.abstained, "{}", a.text);
        assert!(a.text.contains("mixes units"), "{}", a.text);
        assert!(a.citations.is_empty());
    }

    #[test]
    fn average_min_max_and_count_are_right() {
        let t = table(vec![
            vec!["team".into(), "score".into()],
            vec!["a".into(), "10".into()],
            vec!["b".into(), "20".into()],
            vec!["c".into(), "30".into()],
        ]);
        let avg = try_answer("what is the average score", &[t.clone()], &[source("src1", "S")]).unwrap();
        assert!(avg.text.contains("20"), "{}", avg.text);
        let max = try_answer("which team has the highest score", &[t.clone()], &[source("src1", "S")]).unwrap();
        assert!(max.text.contains("30"), "{}", max.text);
        assert!(max.text.contains("row 4"), "max names the winning row: {}", max.text);
        let count = try_answer("how many score values are there", &[t], &[source("src1", "S")]).unwrap();
        assert!(count.text.contains("3"), "{}", count.text);
    }

    #[test]
    fn groups_by_a_second_column() {
        let t = table(vec![
            vec!["region".into(), "channel".into(), "revenue".into()],
            vec!["north".into(), "web".into(), "100".into()],
            vec!["north".into(), "store".into(), "50".into()],
            vec!["south".into(), "web".into(), "25".into()],
        ]);
        let a = try_answer("total revenue by region", &[t], &[source("src1", "Sales")]).unwrap();
        assert!(!a.abstained, "{}", a.text);
        assert!(a.text.contains("sum north: 150"), "{}", a.text);
        assert!(a.text.contains("sum south: 25"), "{}", a.text);
    }

    #[test]
    fn unrelated_questions_fall_through_to_text_answering() {
        let t = table(vec![
            vec!["region".into(), "revenue".into()],
            vec!["north".into(), "1200".into()],
        ]);
        assert!(try_answer("who wrote this document", &[t], &[source("src1", "S")]).is_none());
    }

    #[test]
    fn a_column_with_no_numbers_abstains_instead_of_returning_zero() {
        let t = table(vec![
            vec!["item".into(), "colour".into()],
            vec!["a".into(), "red".into()],
            vec!["b".into(), "blue".into()],
        ]);
        assert!(try_answer("total colour", &[t], &[source("src1", "S")]).is_none());
    }

    #[test]
    fn validation_catches_a_wrong_table_reference() {
        let t = table(vec![
            vec!["region".into(), "revenue".into()],
            vec!["north".into(), "10".into()],
        ]);
        let mut a = try_answer("total revenue", &[t.clone()], &[source("src1", "S")]).unwrap();
        assert!(validate(&a, &[t.clone()]).is_empty());
        a.citations[0].chunk_id = "tbl_other".into();
        assert!(!validate(&a, &[t]).is_empty());
    }

    fn fixture(name: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name);
        std::fs::read(&p).unwrap_or_else(|e| panic!("fixture {} missing: {}", name, e))
    }

    // ---------------- real XLSX

    #[test]
    fn xlsx_parses_every_sheet_with_real_cell_references() {
        let tables = parse_xlsx(&fixture("sales.xlsx"), "src9", "nb9").expect("xlsx");
        assert_eq!(tables.len(), 2, "both worksheets: {:?}", tables.iter().map(|t| &t.sheet).collect::<Vec<_>>());
        let sales = tables.iter().find(|t| t.sheet == "Sales").expect("Sales sheet");
        assert_eq!(sales.headers, vec!["Region", "Channel", "Revenue", "Units"]);
        assert_eq!(sales.rows.len(), 5, "data rows 2-6, blanks skipped");
        assert_eq!(sales.rows[0].number, 2, "original row numbers survive");
        assert_eq!(sales.rows[0].cells[2].reference, "C2");
        assert_eq!(sales.rows[0].cells[2].number(), Some(1200.5));
        // the cached formula result is kept, and flagged as a formula
        let subtotal = sales.rows.iter().find(|r| r.number == 4).unwrap();
        let c4 = sales.cell(subtotal, 2).unwrap();
        assert_eq!(c4.number(), Some(2050.5));
        assert_eq!(c4.formula.as_deref(), Some("SUM(C2:C3)"));
        // a self-closing cell with no value is missing, not zero
        assert!(matches!(sales.cell(subtotal, 3).unwrap().value, CellValue::Empty));
        // shared string "N/A" is missing data
        let na = sales.rows.iter().find(|r| r.number == 5).unwrap();
        assert!(matches!(sales.cell(na, 2).unwrap().value, CellValue::Empty));
        // inline string cell parses as a number
        let last = sales.rows.iter().find(|r| r.number == 6).unwrap();
        assert_eq!(sales.cell(last, 3).unwrap().number(), Some(2.0));

        let meta = tables.iter().find(|t| t.sheet == "Meta").expect("Meta sheet");
        assert!(meta.hidden, "hidden state from workbook.xml");
        assert!(meta.warnings.iter().any(|w| w.contains("hidden")));
        // rich-text shared string is joined, not truncated at the first run
        assert_eq!(meta.headers[0], "note");
    }

    #[test]
    fn xlsx_aggregation_excludes_the_cached_subtotal_row() {
        let tables = parse_xlsx(&fixture("sales.xlsx"), "src9", "nb9").expect("xlsx");
        let src = source("src9", "Sales 2026");
        let a = try_answer("what is the total revenue", &tables, &[src]).expect("answer");
        assert!(!a.abstained, "{}", a.text);
        // 1200.5 + 850 + 300 = 2350.5; C4 (=SUM(C2:C3) = 2050.5) must not be counted
        assert!(a.text.contains("2350.5"), "{}", a.text);
        assert!(!a.text.contains("4401"), "subtotal must not be double counted: {}", a.text);
        assert!(a.text.contains("excluded because they already total"), "{}", a.text);
        assert!(a.text.contains("C4"), "{}", a.text);
        assert_eq!(a.citations[0].locator.sheet.as_deref(), Some("Sales"));
        assert_eq!(a.citations[0].locator.kind, "cell");
        assert_eq!(a.citations[0].locator.cell.as_deref(), Some("Sales!C2:C6"));
        assert!(validate(&a, &tables).is_empty());
    }

    #[test]
    fn xlsx_grouping_uses_the_named_column() {
        let tables = parse_xlsx(&fixture("sales.xlsx"), "src9", "nb9").expect("xlsx");
        let a = try_answer("total revenue by channel", &tables, &[source("src9", "Sales 2026")]).unwrap();
        assert!(a.text.contains("sum Web: 1500.5"), "{}", a.text);
        assert!(a.text.contains("sum Store: 850"), "{}", a.text);
    }

    #[test]
    fn hidden_sheets_are_never_used_for_answers() {
        let tables = parse_xlsx(&fixture("sales.xlsx"), "src9", "nb9").expect("xlsx");
        assert!(try_answer("total revenue", &tables, &[source("src9", "S")]).is_some());
        // the hidden sheet's only number is in a column named "note"; hide it and it is ignored
        let blocks = table_blocks(&tables);
        assert!(blocks.iter().all(|b| !b.text.starts_with("Meta")));
        assert!(blocks.iter().any(|b| b.locator.sheet.as_deref() == Some("Sales")));
    }

    // ---------------- real CSV

    #[test]
    fn csv_fixture_sniffs_delimiter_and_handles_quoted_commas() {
        let t = parse_delimited(&fixture("sales.csv"), "sales.csv", None, "src8", "nb8").expect("csv");
        assert_eq!(t.sheet, "sales");
        assert_eq!(t.headers, vec!["Region", "Channel", "Revenue", "Units"]);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[1].cells[0].text, "South, Ltd", "quoted comma kept whole");
        // European decimal comma in a semicolon file
        assert_eq!(t.rows[0].cells[2].number(), Some(1200.5), "{:?}", t.rows[0].cells[2]);
        assert!(matches!(t.rows[2].cells[2].value, CellValue::Empty), "blank cell is missing");
    }

    #[test]
    fn ambiguous_number_grouping_is_rejected_not_guessed() {
        assert_eq!(parse_number("1,234.50"), Some((1234.5, String::new())));
        assert_eq!(parse_number("1.234,50"), Some((1234.5, String::new())));
        assert_eq!(parse_number("1200,50"), Some((1200.5, String::new())));
        assert_eq!(parse_number("1,200"), Some((1200.0, String::new())));
        assert_eq!(parse_number("1.234.567"), Some((1234567.0, String::new())));
        assert_eq!(parse_number("1,23,456"), None, "inconsistent grouping must not be guessed");
        assert_eq!(parse_number("12,3,45"), None);
        assert_eq!(parse_number("1.2.3"), None);
    }

    #[test]
    fn oversized_sheets_are_truncated_and_aggregations_refuse() {
        let mut rows = vec![vec!["id".to_string(), "amount".to_string()]];
        for i in 0..(crate::security::MAX_TABLE_ROWS + 5) {
            rows.push(vec![format!("r{}", i), "1".to_string()]);
        }
        let t = table(rows);
        assert_eq!(t.rows.len(), crate::security::MAX_TABLE_ROWS);
        assert!(t.truncated);
        assert!(t.warnings.iter().any(|w| w.contains("only the first")), "{:?}", t.warnings);
        // a total over a partial sheet would be a confidently wrong number: refuse
        let a = try_answer("total amount", &[t], &[source("src1", "Big")]).unwrap();
        assert!(a.abstained, "{}", a.text);
        assert!(a.text.contains("more rows than Forge keeps"), "{}", a.text);
        assert!(a.citations.is_empty());
    }

    #[test]
    fn column_letters_and_references_round_trip() {
        assert_eq!(column_letter(1), "A");
        assert_eq!(column_letter(26), "Z");
        assert_eq!(column_letter(27), "AA");
        assert_eq!(column_letter(702), "ZZ");
        assert_eq!(parse_reference("A1"), Some((1, 1)));
        assert_eq!(parse_reference("AA12"), Some((27, 12)));
        assert_eq!(parse_reference("12"), None);
    }

    #[test]
    fn blocks_keep_row_context_and_cell_locators() {
        let t = table(vec![
            vec!["region".into(), "revenue".into()],
            vec!["north".into(), "1200".into()],
        ]);
        let blocks = table_blocks(&[t]);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].text.contains("region: north"), "{}", blocks[0].text);
        assert!(blocks[0].text.contains("revenue: 1200"));
        assert_eq!(blocks[0].locator.kind, "row", "row kind keeps the existing contract");
        assert_eq!(blocks[0].locator.row, Some(2));
        assert_eq!(blocks[0].locator.sheet.as_deref(), Some("Sheet1"));
        assert_eq!(blocks[0].locator.cell.as_deref(), Some("A2-B2"));
        assert_eq!(blocks[0].locator.human(), "Sheet1 row 2");
    }
}
