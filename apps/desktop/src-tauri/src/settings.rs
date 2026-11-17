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
    /// GitHub login to email mappings (spec 7).
    ///
    /// Required for the browser path: a report says who committed by login, and the
    /// companion will not count it without an explicit mapping.
    #[serde(default, rename = "githubLogins")]
    pub github_logins: std::collections::BTreeMap<String, Vec<String>>,
    /// Sound overrides, keyed as the sound crate expects (spec 19).
    #[serde(default, rename = "soundOverrides")]
    pub sound_overrides: std::collections::BTreeMap<String, String>,
    /// User-defined achievements (spec 22).
    ///
    /// Stored as the validated definitions, not the raw submissions: an invalid
    /// document never reaches this list.
    #[serde(default, rename = "userAchievements")]
    pub user_achievements: Vec<devchievements_achievements::definition::UserAchievement>,
    /// User event filters (spec 15).
    ///
    /// Applied to the user-filtered statistics profile only: excluding a
    /// repository here must never change a built-in achievement's outcome.
    #[serde(default, rename = "filters")]
    pub filters: devchievements_filters::FilterSet,
    /// Popup appearance (spec 21).
    #[serde(default)]
    pub popup: devchievements_themes::PopupStyle,
    /// Whether the first-run wizard has been completed (spec 25).
    #[serde(default, rename = "onboardingCompleted")]
    pub onboarding_completed: bool,
    /// Whether a normal launch stays in the tray instead of opening the window.
    ///
    /// Defaults to true because that is what a companion is: something that runs
    /// quietly until it has something to say. A user who opens the window every
    /// morning can turn it off, and then a launch behaves like any other app.
    #[serde(default = "default_true", rename = "startHidden")]
    pub start_hidden: bool,
    /// Whether Windows starts the companion at sign-in.
    ///
    /// Off by default: installing software is a request to be present when asked,
    /// not to run before the first login. Turning it on writes the Run key the
    /// uninstaller already knows how to remove.
    #[serde(default, rename = "startWithWindows")]
    pub start_with_windows: bool,
    /// UI language: `system`, `en` or `zh-CN`.
    ///
    /// Stored as a string rather than an enum so an unknown value from a newer
    /// build degrades to "follow the system" instead of failing the whole file.
    #[serde(default = "default_language", rename = "language")]
    pub language: String,
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

/// Keep the language value to the three the UI understands.
///
/// Anything else degrades to "follow the system": a value written by a newer build,
/// or a hand-edited file, must not leave the window with no strings at all.
pub fn normalize_language(value: &str) -> String {
    match value {
        "en" => "en".to_string(),
        "zh-CN" => "zh-CN".to_string(),
        _ => "system".to_string(),
    }
}

fn default_language() -> String {
    "system".to_string()
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
            github_logins: std::collections::BTreeMap::new(),
            sound_overrides: std::collections::BTreeMap::new(),
            user_achievements: Vec::new(),
            filters: devchievements_filters::FilterSet::default(),
            popup: devchievements_themes::PopupStyle::default(),
            onboarding_completed: false,
            start_hidden: true,
            start_with_windows: false,
            language: default_language(),
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

impl Settings {
    /// Build the identity model this file describes.
    ///
    /// One place, because a caller that remembered the emails but forgot the logins
    /// would silently refuse every browser event.
    pub fn to_identity(&self) -> devchievements_events::Identity {
        let mut identity = devchievements_events::Identity::default();
        for email in &self.emails {
            identity.add_email(
                email.clone(),
                devchievements_events::EmailOrigin::UserConfigured,
            );
        }
        for (login, emails) in &self.github_logins {
            identity.add_login_mapping(login.clone(), emails.clone());
        }
        identity
    }
}

impl Settings {
    /// Build the portable configuration document this file describes.
    pub fn to_document(&self) -> devchievements_config::ConfigDocument {
        devchievements_config::ConfigDocument {
            schema_version: devchievements_config::CONFIG_SCHEMA_VERSION,
            emails: self.emails.clone(),
            github_logins: self.github_logins.clone(),
            privacy: self.privacy.as_str().to_string(),
            timezone: self.timezone.clone(),
            notifications: self.notifications,
            volume: self.volume,
            muted: self.muted,
            sound_overrides: self.sound_overrides.clone(),
            user_achievements: self.user_achievements.clone(),
            filters: self.filters.clone(),
            popup: self.popup.clone(),
            start_hidden: self.start_hidden,
            start_with_windows: self.start_with_windows,
            language: self.language.clone(),
        }
    }

    /// Apply an imported document.
    ///
    /// The document has already been validated by the config crate, so this only
    /// maps fields; it does not re-judge them.
    pub fn apply_document(&mut self, document: devchievements_config::ConfigDocument) {
        self.emails = document.emails;
        self.github_logins = document.github_logins;
        self.privacy = PrivacyMode::parse(&document.privacy).unwrap_or(PrivacyMode::Normal);
        self.timezone = document.timezone;
        self.notifications = document.notifications;
        self.volume = document.volume.min(100);
        self.muted = document.muted;
        self.sound_overrides = document.sound_overrides;
        self.user_achievements = document.user_achievements;
        self.filters = document.filters;
        self.popup = document.popup;
        self.start_hidden = document.start_hidden;
        self.start_with_windows = document.start_with_windows;
        self.language = normalize_language(&document.language);
    }
}
