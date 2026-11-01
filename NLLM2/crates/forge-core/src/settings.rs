//! Server-stored preferences, and honest usage reporting.
//!
//! The spec's §7.17 asks for user control: theme, appearance, defaults, storage, privacy and
//! accessibility. The temptation with a settings screen is to offer everything and wire up half
//! of it. This module takes the opposite line: every field here **changes behaviour somewhere**,
//! and the things Forge cannot do (interface languages, model selection without a provider,
//! server-side font rendering) are named as unsupported in `/api/capabilities` instead of being
//! shown as dead toggles.
//!
//! Settings are stored in the same `db.json` as everything else, so a backup carries them and a
//! restart keeps them — a preference that resets on restart is not a preference.

use crate::grounded::{Length, Style};
use crate::json::Value;
use crate::obj;

/// Theme names the UI understands.
pub const THEMES: [&str; 3] = ["dark", "light", "system"];
/// Library sort orders the notebook list accepts.
pub const SORTS: [&str; 4] = ["recent", "title", "created", "sources"];
/// Font scale bounds. Below 0.9 the interface stops being readable; above 1.4 the layout breaks.
pub const FONT_SCALE_MIN: f64 = 0.9;
pub const FONT_SCALE_MAX: f64 = 1.4;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: String,
    /// Multiplier applied to the root font size by the client.
    pub font_scale: f64,
    /// Removes non-essential animation and smooth scrolling.
    pub reduced_motion: bool,
    /// Raises contrast for text and borders.
    pub high_contrast: bool,
    /// Default answer presentation for a new chat.
    pub default_style: String,
    /// Default citation budget for a new chat.
    pub default_length: String,
    /// Default order of the notebook library.
    pub library_sort: String,
    /// Whether the library hides archived notebooks by default.
    pub hide_archived: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "dark".into(),
            font_scale: 1.0,
            reduced_motion: false,
            high_contrast: false,
            default_style: "evidence".into(),
            default_length: "standard".into(),
            library_sort: "recent".into(),
            hide_archived: true,
        }
    }
}

impl Settings {
    pub fn to_json(&self) -> Value {
        obj! {
            "theme" => self.theme.as_str(),
            "font_scale" => self.font_scale,
            "reduced_motion" => self.reduced_motion,
            "high_contrast" => self.high_contrast,
            "default_style" => self.default_style.as_str(),
            "default_length" => self.default_length.as_str(),
            "library_sort" => self.library_sort.as_str(),
            "hide_archived" => self.hide_archived,
        }
    }

    pub fn from_json(v: &Value) -> Settings {
        let d = Settings::default();
        Settings {
            theme: v.str_of("theme").map(|s| s.to_string()).unwrap_or(d.theme),
            font_scale: v.get("font_scale").and_then(|x| x.as_f64()).unwrap_or(d.font_scale),
            reduced_motion: v.get("reduced_motion").and_then(|x| x.as_bool()).unwrap_or(d.reduced_motion),
            high_contrast: v.get("high_contrast").and_then(|x| x.as_bool()).unwrap_or(d.high_contrast),
            default_style: v.str_of("default_style").map(|s| s.to_string()).unwrap_or(d.default_style),
            default_length: v.str_of("default_length").map(|s| s.to_string()).unwrap_or(d.default_length),
            library_sort: v.str_of("library_sort").map(|s| s.to_string()).unwrap_or(d.library_sort),
            hide_archived: v.get("hide_archived").and_then(|x| x.as_bool()).unwrap_or(d.hide_archived),
        }
    }

    /// Apply a partial update. Each rejected value is reported by name, so a client that sends one
    /// bad field learns which one it was instead of losing the whole update.
    pub fn apply(&mut self, patch: &Value) -> Vec<String> {
        let mut rejected: Vec<String> = Vec::new();
        if let Some(theme) = patch.str_of("theme") {
            let t = theme.trim().to_ascii_lowercase();
            if THEMES.contains(&t.as_str()) {
                self.theme = t;
            } else {
                rejected.push(format!("theme must be one of {}", THEMES.join(", ")));
            }
        }
        if let Some(scale) = patch.get("font_scale").and_then(|v| v.as_f64()) {
            if (FONT_SCALE_MIN..=FONT_SCALE_MAX).contains(&scale) {
                // Two decimals is what the UI renders; keeping the raw value would round-trip noise.
                self.font_scale = (scale * 100.0).round() / 100.0;
            } else {
                rejected.push(format!(
                    "font_scale must be between {} and {}",
                    FONT_SCALE_MIN, FONT_SCALE_MAX
                ));
            }
        }
        if let Some(flag) = patch.get("reduced_motion").and_then(|v| v.as_bool()) {
            self.reduced_motion = flag;
        }
        if let Some(flag) = patch.get("high_contrast").and_then(|v| v.as_bool()) {
            self.high_contrast = flag;
        }
        if let Some(style) = patch.str_of("default_style") {
            match Style::parse(style) {
                Some(s) => self.default_style = s.as_str().to_string(),
                None => rejected.push("default_style must be evidence, concise or detailed".into()),
            }
        }
        if let Some(length) = patch.str_of("default_length") {
            match Length::parse(length) {
                Some(l) => self.default_length = l.as_str().to_string(),
                None => rejected.push("default_length must be brief, standard or thorough".into()),
            }
        }
        if let Some(sort) = patch.str_of("library_sort") {
            let s = sort.trim().to_ascii_lowercase();
            if SORTS.contains(&s.as_str()) {
                self.library_sort = s;
            } else {
                rejected.push(format!("library_sort must be one of {}", SORTS.join(", ")));
            }
        }
        if let Some(flag) = patch.get("hide_archived").and_then(|v| v.as_bool()) {
            self.hide_archived = flag;
        }
        rejected
    }
}

/// Real on-disk usage, measured rather than estimated from record counts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub database_bytes: u64,
    pub blob_bytes: u64,
    pub blob_count: u64,
    pub backup_count: u64,
    pub backup_bytes: u64,
    pub notebook_count: u64,
    pub source_count: u64,
    pub chunk_count: u64,
    pub artifact_count: u64,
    pub conversation_count: u64,
}

impl Usage {
    pub fn total_bytes(&self) -> u64 {
        self.database_bytes + self.blob_bytes + self.backup_bytes
    }

    pub fn to_json(&self) -> Value {
        obj! {
            "database_bytes" => self.database_bytes,
            "blob_bytes" => self.blob_bytes,
            "blob_count" => self.blob_count,
            "backup_bytes" => self.backup_bytes,
            "backup_count" => self.backup_count,
            "total_bytes" => self.total_bytes(),
            "records" => obj! {
                "notebooks" => self.notebook_count,
                "sources" => self.source_count,
                "chunks" => self.chunk_count,
                "artifacts" => self.artifact_count,
                "conversations" => self.conversation_count,
            },
            "note" => "measured from the data directory: db.json plus the retained source blobs and rolling backups",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_documented_defaults() {
        let s = Settings::default();
        assert_eq!(s.theme, "dark");
        assert_eq!(s.default_style, "evidence");
        assert_eq!(s.default_length, "standard");
        assert!(s.hide_archived);
        assert_eq!(Settings::from_json(&s.to_json()), s, "settings round-trip through JSON");
    }

    #[test]
    fn a_patch_changes_only_what_it_names() {
        let mut s = Settings::default();
        let rejected = s.apply(&crate::obj! { "theme" => "light" });
        assert!(rejected.is_empty());
        assert_eq!(s.theme, "light");
        assert_eq!(s.default_length, "standard", "untouched fields keep their value");
    }

    #[test]
    fn bad_values_are_named_and_not_applied() {
        let mut s = Settings::default();
        let rejected = s.apply(&crate::obj! {
            "theme" => "neon",
            "font_scale" => 9.0,
            "default_style" => "shouty",
            "default_length" => "epic",
            "library_sort" => "vibes",
        });
        assert_eq!(rejected.len(), 5, "{:?}", rejected);
        assert!(rejected.iter().any(|r| r.contains("theme")));
        assert!(rejected.iter().any(|r| r.contains("font_scale")));
        assert!(rejected.iter().any(|r| r.contains("default_style")));
        assert!(rejected.iter().any(|r| r.contains("default_length")));
        assert!(rejected.iter().any(|r| r.contains("library_sort")));
        // Nothing bad was applied.
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn good_values_in_the_same_patch_still_apply_when_others_are_rejected() {
        let mut s = Settings::default();
        let rejected = s.apply(&crate::obj! { "theme" => "light", "font_scale" => 99.0 });
        assert_eq!(rejected.len(), 1);
        assert_eq!(s.theme, "light", "a good field is not thrown away because a sibling was bad");
    }

    #[test]
    fn font_scale_is_rounded_to_what_the_ui_renders() {
        let mut s = Settings::default();
        assert!(s.apply(&crate::obj! { "font_scale" => 1.234 }).is_empty());
        assert_eq!(s.font_scale, 1.23);
    }

    #[test]
    fn an_old_database_with_no_settings_decodes_to_defaults() {
        let s = Settings::from_json(&Value::Null);
        assert_eq!(s, Settings::default());
        // A partial object fills the rest from defaults rather than zeroing them.
        let partial = Settings::from_json(&crate::obj! { "theme" => "light" });
        assert_eq!(partial.theme, "light");
        assert_eq!(partial.font_scale, 1.0);
        assert_eq!(partial.default_style, "evidence");
    }

    #[test]
    fn usage_adds_up_and_reports_what_it_measured() {
        let u = Usage {
            database_bytes: 1000,
            blob_bytes: 250,
            backup_bytes: 500,
            blob_count: 2,
            ..Default::default()
        };
        assert_eq!(u.total_bytes(), 1750);
        let json = u.to_json();
        assert_eq!(json.get("total_bytes").and_then(|v| v.as_f64()), Some(1750.0));
        assert!(json.str_of("note").unwrap_or("").contains("measured"));
    }
}
