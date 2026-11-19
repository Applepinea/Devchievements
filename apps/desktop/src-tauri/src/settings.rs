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

/// Copy an unreadable settings file aside so nothing is destroyed.
///
/// The copy is named after the file plus a timestamp, so two bad files do not
/// overwrite each other, and it lives next to the original because that is where a
/// user looks first. A copy that fails is not fatal: the caller still falls back to
/// defaults, and the report says the backup is missing rather than pretending it
/// succeeded.
fn copy_aside(path: &std::path::Path, contents: &str) -> Option<std::path::PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!(
        "{}.corrupt-{stamp}.bak",
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "settings.json".to_string())
    );
    let destination = path.with_file_name(name);
    std::fs::write(&destination, contents).ok().map(|()| destination)
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

/// What went wrong while loading, when something did.
///
/// The page shows this instead of silently reverting to defaults: a user whose
/// `popup` block is malformed must be told the field was ignored, because otherwise
/// their sliders "reset themselves" for no visible reason.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadReport {
    /// Field paths that were dropped because they did not parse.
    pub dropped_fields: Vec<String>,
    /// Where the unreadable original was copied, when it was copied.
    pub backup_path: Option<std::path::PathBuf>,
    /// Whether the whole file had to be discarded (it was not JSON at all).
    pub unreadable: bool,
}

impl LoadReport {
    /// Whether anything at all was lost.
    pub fn is_clean(&self) -> bool {
        self.dropped_fields.is_empty() && !self.unreadable && self.backup_path.is_none()
    }

    /// One line a page can render, or `None` when nothing was lost.
    ///
    /// English, like the rest of this crate's copy: the UI localises its own strings
    /// and passes this through `t()`-style lookup only where it owns the wording.
    pub fn message(&self) -> Option<String> {
        if self.is_clean() {
            return None;
        }
        let mut parts = Vec::new();
        if !self.dropped_fields.is_empty() {
            parts.push(format!(
                "kept the rest but ignored {} field(s): {}",
                self.dropped_fields.len(),
                self.dropped_fields.join(", ")
            ));
        }
        if self.unreadable {
            parts.push("the settings file was not valid JSON, so only defaults were kept".to_string());
        }
        if let Some(path) = &self.backup_path {
            parts.push(format!("the unreadable file was saved as {}", path.display()));
        }
        Some(format!("Settings were partly unreadable: {}", parts.join("; ")))
    }
}

impl Settings {
    /// Load settings, falling back to defaults when the file is absent or unreadable.
    ///
    /// A corrupt file must not stop the app: the worst case is that the user is asked
    /// to confirm their identity again, which is recoverable.
    ///
    /// # Why this now reads field by field
    ///
    /// `serde_json::from_str::<Settings>` is all-or-nothing: one malformed `popup`
    /// block used to discard `emails` too, and the app then fell back to guessing an
    /// identity from `git config`. Whatever CAN be read is now kept, and a copy of the
    /// unreadable original is written next to it so nothing is destroyed.
    pub fn load(path: &std::path::Path) -> Self {
        Settings::load_with_report(path).0
    }

    /// Load settings and describe anything that had to be dropped.
    pub fn load_with_report(path: &std::path::Path) -> (Self, LoadReport) {
        let mut report = LoadReport::default();

        let Ok(text) = std::fs::read_to_string(path) else {
            // Absent is the normal first-run state, not a problem worth reporting.
            return (Settings::default(), report);
        };

        // A UTF-8 BOM is not valid JSON (RFC 8259 section 8.1), but Windows tooling
        // writes one by default: `Set-Content -Encoding utf8` in Windows PowerShell,
        // and Notepad's "UTF-8 with BOM" option. Without this, a hand-edited settings
        // file is discarded wholesale and every field silently reverts to its default,
        // which is indistinguishable from the app ignoring the edit.
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_string();

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            // Not JSON at all: keep a copy before falling back, because the user may
            // have hand-edited it and would otherwise have no way back.
            report.unreadable = true;
            report.backup_path = copy_aside(path, &text);
            return (Settings::default(), report);
        };

        let mut settings = Settings::default();

        // Known fields, each judged on its own. A field that fails keeps the default
        // and is named in the report; a field that is absent is simply absent.
        macro_rules! read_field {
            ($name:literal, $target:expr) => {
                if let Some(raw) = value.get($name) {
                    match serde_json::from_value(raw.clone()) {
                        Ok(parsed) => $target = parsed,
                        Err(_) => report.dropped_fields.push($name.to_string()),
                    }
                }
            };
        }

        read_field!("emails", settings.emails);
        read_field!("privacy", settings.privacy);
        read_field!("timezone", settings.timezone);
        read_field!("notifications", settings.notifications);
        read_field!("volume", settings.volume);
        read_field!("muted", settings.muted);
        read_field!("githubLogins", settings.github_logins);
        read_field!("soundOverrides", settings.sound_overrides);
        read_field!("userAchievements", settings.user_achievements);
        read_field!("filters", settings.filters);
        read_field!("popup", settings.popup);
        read_field!("onboardingCompleted", settings.onboarding_completed);
        read_field!("startHidden", settings.start_hidden);
        read_field!("startWithWindows", settings.start_with_windows);
        read_field!("language", settings.language);
        read_field!("schemaVersion", settings.schema_version);

        settings.volume = settings.volume.min(100);
        settings.language = normalize_language(&settings.language);

        if !report.dropped_fields.is_empty() {
            // Also keep the original of a partially-readable file: the fields that
            // were dropped are still in there, and a user may want to recover them.
            report.backup_path = copy_aside(path, &text);
        }

        (settings, report)
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


#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// A scratch directory that cleans up after itself.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("devch-settings-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch");
            Scratch(dir)
        }

        fn file(&self) -> std::path::PathBuf {
            self.0.join("settings.json")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn one_bad_field_does_not_lose_the_rest() {
        // The reported bug: a malformed `popup` block discarded `emails` too, so the
        // app fell back to guessing an identity from git config.
        let scratch = Scratch::new("bad-popup");
        std::fs::write(
            scratch.file(),
            r#"{
                "schemaVersion": 1,
                "emails": ["me@example.com"],
                "githubLogins": {"me": ["me@example.com"]},
                "popup": {"displaySeconds": 1}
            }"#,
        )
        .expect("write");

        let (settings, report) = Settings::load_with_report(&scratch.file());

        assert_eq!(
            settings.emails,
            vec!["me@example.com".to_string()],
            "the identity must survive a broken popup block"
        );
        assert_eq!(settings.github_logins.len(), 1, "and so must the login mapping");
        assert_eq!(
            settings.popup,
            devchievements_themes::PopupStyle::default(),
            "the broken field falls back to its default"
        );
        assert_eq!(report.dropped_fields, vec!["popup".to_string()]);
        assert!(!report.is_clean());
        assert!(report.message().is_some(), "the page needs something to show");
    }

    #[test]
    fn a_partial_file_is_backed_up_before_anything_is_dropped() {
        let scratch = Scratch::new("backup");
        std::fs::write(scratch.file(), r#"{"emails":["me@example.com"],"volume":9999}"#)
            .expect("write");

        let (settings, report) = Settings::load_with_report(&scratch.file());

        // 9999 is not a valid u8, so the field is dropped rather than wrapping.
        assert_eq!(settings.emails, vec!["me@example.com".to_string()]);
        assert_eq!(settings.volume, 80, "the default volume");
        let backup = report.backup_path.expect("a copy of the original");
        assert!(backup.exists(), "the reported backup must actually exist");
        assert!(
            std::fs::read_to_string(&backup).unwrap().contains("9999"),
            "the copy must hold the original text, not the repaired one"
        );
    }

    #[test]
    fn a_file_that_is_not_json_is_copied_aside_and_defaults_are_used() {
        let scratch = Scratch::new("not-json");
        std::fs::write(scratch.file(), "this is not json at all").expect("write");

        let (settings, report) = Settings::load_with_report(&scratch.file());

        assert!(settings.emails.is_empty());
        assert!(report.unreadable, "the report must say the file was unusable");
        let backup = report.backup_path.expect("a copy of the original");
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "this is not json at all"
        );
    }

    #[test]
    fn a_utf8_bom_does_not_discard_the_file() {
        // Measured while diagnosing "the margin will not go to zero": PowerShell's
        // `Set-Content -Encoding utf8` writes a BOM, `serde_json` rejects the whole
        // document, and every value silently falls back to its default. From the UI
        // that is indistinguishable from the app ignoring the setting.
        let scratch = Scratch::new("bom");
        let json = r#"{"emails":["me@example.com"],"popup":{"width":400,"height":100,"scalePercent":100,"iconSize":52,"cornerRadius":0,"screenMargin":0,"corner":"top_left","opacityPercent":94,"displaySeconds":5,"animationMs":240,"backgroundBlur":0}}"#;
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(json.as_bytes());
        std::fs::write(scratch.file(), &bytes).expect("write with BOM");

        let (settings, report) = Settings::load_with_report(&scratch.file());

        assert!(report.is_clean(), "a BOM is not corruption: {report:?}");
        assert_eq!(settings.emails, vec!["me@example.com".to_string()]);
        assert_eq!(
            settings.popup.screen_margin, 0,
            "the margin the user actually wrote must survive"
        );
        assert_eq!(settings.popup.width, 400);
        assert_eq!(
            settings.popup.corner,
            devchievements_themes::Corner::TopLeft
        );
    }

    #[test]
    fn a_missing_file_is_not_a_problem() {
        let scratch = Scratch::new("missing");
        let (settings, report) = Settings::load_with_report(&scratch.file());
        assert!(settings.emails.is_empty());
        assert!(
            report.is_clean(),
            "a first run has no settings file; that is normal, not a warning"
        );
        assert!(report.message().is_none());
    }

    #[test]
    fn a_healthy_file_produces_no_warning() {
        let scratch = Scratch::new("healthy");
        let mut original = Settings::default();
        original.emails = vec!["me@example.com".to_string()];
        original.volume = 55;
        original.save(&scratch.file()).expect("save");

        let (settings, report) = Settings::load_with_report(&scratch.file());
        assert_eq!(settings.emails, original.emails);
        assert_eq!(settings.volume, 55);
        assert!(report.is_clean(), "{report:?}");
        assert!(report.backup_path.is_none(), "nothing to back up");
    }

    #[test]
    fn load_keeps_its_signature_and_still_reads_a_good_file() {
        // The 17 existing call sites take a Settings, not a tuple.
        let scratch = Scratch::new("signature");
        let mut original = Settings::default();
        original.emails = vec!["me@example.com".to_string()];
        original.save(&scratch.file()).expect("save");

        let settings: Settings = Settings::load(&scratch.file());
        assert_eq!(settings.emails, vec!["me@example.com".to_string()]);
    }

    #[test]
    fn an_unknown_language_degrades_to_following_the_system() {
        let scratch = Scratch::new("language");
        std::fs::write(scratch.file(), r#"{"language":"kl-KL"}"#).expect("write");
        let (settings, _) = Settings::load_with_report(&scratch.file());
        assert_eq!(settings.language, "system");
    }
}

