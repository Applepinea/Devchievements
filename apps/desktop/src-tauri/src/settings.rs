//! Companion settings stored next to the database (spec 26.4: configuration is versioned).
//!
//! # Why a file and not a database table
//!
//! Settings must be readable before the database opens (the identity gate depends on
//! the configured emails) and editable by a user with a text editor. A small JSON file
//! does both; a table would need the database to exist first, which is circular.
//!
//! Unknown fields are ignored rather than rejected so a file written by a newer build
//! still loads.

use serde::{Deserialize, Serialize};

/// Settings schema version written on save.
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

/// Privacy mode, mirrored from the privacy crate for serialisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyMode {
    /// Show repository names and commit detail.
    #[default]
    Normal,
    /// Hide them and use generic wording (spec 26.2).
    Privacy,
}

impl PrivacyMode {
    /// Parse from a CLI argument.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(PrivacyMode::Normal),
            "privacy" => Some(PrivacyMode::Privacy),
            _ => None,
        }
    }

    /// The opposite mode, for a toggle.
    pub fn toggled(self) -> Self {
        match self {
            PrivacyMode::Normal => PrivacyMode::Privacy,
            PrivacyMode::Privacy => PrivacyMode::Normal,
        }
    }

    /// Stable name used in files and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            PrivacyMode::Normal => "normal",
            PrivacyMode::Privacy => "privacy",
        }
    }
}

/// What the companion remembers between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Schema version of this file.
    #[serde(default = "default_schema_version", rename = "schemaVersion")]
    pub schema_version: u32,
    /// Configured identity emails (spec 7).
    #[serde(default)]
    pub emails: Vec<String>,
    /// Privacy mode (spec 26.2).
    #[serde(default)]
    pub privacy: PrivacyMode,
    /// IANA timezone used for local_date (spec 12).
    #[serde(default)]
    pub timezone: Option<String>,
    /// Whether notifications are shown at all (spec 34.1).
    ///
    /// Achievements are still recorded when this is off: the setting controls
    /// interruption, not whether work counts.
    #[serde(default = "default_true")]
    pub notifications: bool,
    /// Playback volume, 0-100.
    #[serde(default = "default_volume")]
    pub volume: u8,
    /// Whether sound is muted entirely.
    #[serde(default)]
    pub muted: bool,
    /// Whether the first-run wizard has been completed (spec 25).
    #[serde(default, rename = "onboardingCompleted")]
    pub onboarding_completed: bool,
}

fn default_true() -> bool {
    true
}

fn default_volume() -> u8 {
    80
}

fn default_schema_version() -> u32 {
    SETTINGS_SCHEMA_VERSION
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            emails: Vec::new(),
            privacy: PrivacyMode::Normal,
            timezone: None,
            notifications: true,
            volume: 80,
            muted: false,
            onboarding_completed: false,
        }
    }
}

impl Settings {
    /// Load settings, falling back to defaults when the file is absent or unreadable.
    ///
    /// A corrupt file must not stop the app: the worst case is that the user is asked
    /// to confirm their identity again, which is recoverable.
    pub fn load(path: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Settings::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Save settings, creating the parent directory if needed.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, text + "\n")
    }
}
