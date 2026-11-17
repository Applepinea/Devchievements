//! Importing and exporting the whole configuration (spec 26.4, 4a).
//!
//! # Why a separate document rather than the settings file itself
//!
//! The settings file is the app's private state. An export is a *promise* to the
//! user: it can be moved to another machine and it says which version wrote it. Mixing
//! the two would mean an export could silently include machine-specific paths.
//!
//! # Why import is validated, not trusted
//!
//! An exported file can be edited by hand or produced by another build. Every field is
//! range-checked or rejected: a volume of 300 is clamped, an unknown schema is
//! refused. Nothing is applied before the whole document validates, so a bad import
//! leaves the previous configuration intact.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// The schema version this build writes and accepts.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Why an import was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// The document is not valid JSON.
    NotJson(String),
    /// The document declares a schema this build does not understand.
    UnsupportedSchema(u32),
    /// A required field was missing or malformed.
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NotJson(e) => write!(f, "not valid JSON: {e}"),
            ConfigError::UnsupportedSchema(v) => {
                write!(f, "config schema {v} is not supported by this build")
            }
            ConfigError::Invalid(what) => write!(f, "invalid config: {what}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// A portable configuration document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDocument {
    /// Schema version of this document.
    pub schema_version: u32,
    /// Identity emails (spec 7).
    pub emails: Vec<String>,
    /// GitHub login to emails (spec 7).
    pub github_logins: std::collections::BTreeMap<String, Vec<String>>,
    /// Privacy mode.
    ///
    /// Deserialised leniently: a hand-edited file with an unknown value is normalised
    /// to "normal" rather than rejected, because the field is a preference.
    #[serde(default = "default_privacy", deserialize_with = "lenient_privacy")]
    pub privacy: String,
    /// Timezone, if pinned.
    pub timezone: Option<String>,
    /// Whether notifications are shown.
    pub notifications: bool,
    /// Volume, 0-100.
    pub volume: u8,
    /// Whether sound is muted.
    pub muted: bool,
    /// Sound overrides, keyed as the sound crate expects.
    pub sound_overrides: std::collections::BTreeMap<String, String>,
    /// User-defined achievements (spec 22).
    ///
    /// Defaulted so a document written before the field existed still loads: an
    /// export is a promise and must not be broken by adding a field.
    #[serde(default)]
    pub user_achievements: Vec<devchievements_achievements::definition::UserAchievement>,
    /// User event filters (spec 15).
    #[serde(default)]
    pub filters: devchievements_filters::FilterSet,
    /// Popup appearance.
    pub popup: devchievements_themes::PopupStyle,
    /// Whether a launch stays in the tray.
    ///
    /// Defaulted for the same reason as the fields above: an export written before
    /// this existed must still import.
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    /// Whether Windows starts the companion at sign-in.
    ///
    /// Defaulted to false for the same reason it is off in the app: an import must
    /// not silently add an autostart entry the user never asked for.
    #[serde(default)]
    pub start_with_windows: bool,
    /// UI language: `system`, `en` or `zh-CN`.
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for ConfigDocument {
    fn default() -> Self {
        ConfigDocument {
            schema_version: CONFIG_SCHEMA_VERSION,
            emails: Vec::new(),
            github_logins: std::collections::BTreeMap::new(),
            privacy: "normal".to_string(),
            timezone: None,
            notifications: true,
            volume: 80,
            muted: false,
            sound_overrides: std::collections::BTreeMap::new(),
            user_achievements: Vec::new(),
            filters: devchievements_filters::FilterSet::default(),
            popup: devchievements_themes::PopupStyle::default(),
            start_hidden: true,
            start_with_windows: false,
            language: default_language(),
        }
    }
}

fn default_privacy() -> String {
    "normal".to_string()
}

/// Missing means "behave like a companion": tray only until asked to open.
fn default_start_hidden() -> bool {
    true
}

/// Missing means "follow the operating system".
fn default_language() -> String {
    "system".to_string()
}

/// Accept any string, then normalise. Rejects nothing, because a preference with an
/// unknown value has an obvious safe interpretation.
fn lenient_privacy<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(if raw == "privacy" {
        raw
    } else {
        "normal".to_string()
    })
}

impl ConfigDocument {
    /// Serialise for export.
    ///
    /// Pretty-printed on purpose: an export is something a user may read or diff
    /// before trusting it.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Parse and validate an imported document.
    pub fn from_json(text: &str) -> Result<Self, ConfigError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| ConfigError::NotJson(e.to_string()))?;

        // The version is checked before the full parse, for the same reason as themes:
        // a newer document may legitimately omit fields this build requires, and
        // reporting "missing field" would send the user looking in the wrong place.
        if let Some(version) = value.get("schemaVersion").and_then(|v| v.as_u64()) {
            if version > u64::from(CONFIG_SCHEMA_VERSION) {
                return Err(ConfigError::UnsupportedSchema(version as u32));
            }
        }

        // Numeric fields are normalised BEFORE the typed parse. serde would reject
        // volume: 300 with "expected u8", which tells the user nothing; clamping
        // first means an edited file still loads.
        let mut value = value;
        if let Some(volume) = value.get("volume").and_then(|v| v.as_u64()) {
            let clamped = volume.min(100);
            value["volume"] = serde_json::json!(clamped);
        }

        let mut document: ConfigDocument =
            serde_json::from_value(value).map_err(|e| ConfigError::Invalid(e.to_string()))?;

        // Normalise rather than reject: these are user preferences, and the closest
        // usable value is friendlier than a refusal. Anything semantically impossible
        // (an empty login key) IS rejected, because it changes meaning.
        document.volume = document.volume.min(100);
        document.privacy = match document.privacy.as_str() {
            "privacy" => "privacy".to_string(),
            _ => "normal".to_string(),
        };
        document.emails = document
            .emails
            .into_iter()
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty())
            .collect();

        let mut clean_logins = std::collections::BTreeMap::new();
        for (login, emails) in document.github_logins {
            let login = login.trim().to_string();
            if login.is_empty() {
                return Err(ConfigError::Invalid(
                    "a GitHub login mapping has no login".into(),
                ));
            }
            let emails: Vec<String> = emails
                .into_iter()
                .map(|e| e.trim().to_string())
                .filter(|e| !e.is_empty())
                .collect();
            if emails.is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "login `{login}` maps to no email"
                )));
            }
            clean_logins.insert(login, emails);
        }
        document.github_logins = clean_logins;

        // Ranges from spec 21 are enforced on import, so an edited file cannot place
        // the popup off the screen.
        document.popup = document.popup.clamped();

        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_written_before_start_hidden_still_imports() {
        // The field arrived with the "start hidden" setting. An export is a promise,
        // so a document that predates the field must import with the companion
        // default rather than failing validation.
        //
        // Built by removing the field from a real document rather than by hand-writing
        // one: a hand-written fixture silently stops resembling the real format the
        // moment another required field is added.
        let text = ConfigDocument::default().to_json();
        let mut value: serde_json::Value = serde_json::from_str(&text).expect("json");
        let removed = value
            .as_object_mut()
            .expect("document is an object")
            .remove("startHidden");
        assert!(removed.is_some(), "the field is present before removal");

        let older = serde_json::to_string(&value).expect("serialise");
        let document = ConfigDocument::from_json(&older).expect("older export imports");
        assert!(
            document.start_hidden,
            "a missing startHidden must default to tray-only"
        );
    }

    #[test]
    fn the_default_document_round_trips() {
        let document = ConfigDocument::default();
        let text = document.to_json();
        let back = ConfigDocument::from_json(&text).expect("parse");
        assert_eq!(back, document);
    }

    #[test]
    fn a_document_from_a_newer_build_is_refused_by_version() {
        let text = r#"{ "schemaVersion": 99 }"#;
        assert!(matches!(
            ConfigDocument::from_json(text),
            Err(ConfigError::UnsupportedSchema(99))
        ));
    }

    #[test]
    fn broken_json_is_reported_as_such() {
        assert!(matches!(
            ConfigDocument::from_json("not json"),
            Err(ConfigError::NotJson(_))
        ));
    }

    #[test]
    fn an_out_of_range_volume_is_clamped() {
        let text = r#"{
            "schemaVersion": 1,
            "emails": [],
            "githubLogins": {},
            "privacy": "normal",
            "timezone": null,
            "notifications": true,
            "volume": 300,
            "muted": false,
            "soundOverrides": {},
            "popup": {
                "width": 400, "height": null, "scalePercent": 100,
                "iconSize": 52, "cornerRadius": 0, "screenMargin": 0,
                "corner": "bottom_right", "opacityPercent": 94, "displaySeconds": 5
            }
        }"#;
        let document = ConfigDocument::from_json(text).expect("parse");
        assert_eq!(document.volume, 100);
    }

    #[test]
    fn a_popup_outside_the_documented_range_is_clamped_on_import() {
        let text = r#"{
            "schemaVersion": 1,
            "emails": [],
            "githubLogins": {},
            "privacy": "normal",
            "timezone": null,
            "notifications": true,
            "volume": 80,
            "muted": false,
            "soundOverrides": {},
            "popup": {
                "width": 9000, "height": null, "scalePercent": 400,
                "iconSize": 52, "cornerRadius": 0, "screenMargin": 0,
                "corner": "bottom_right", "opacityPercent": 94, "displaySeconds": 5
            }
        }"#;
        let document = ConfigDocument::from_json(text).expect("parse");
        assert_eq!(
            document.popup.width,
            devchievements_themes::limits::WIDTH.max
        );
        assert_eq!(
            document.popup.scale_percent,
            devchievements_themes::limits::SCALE.max
        );
    }

    #[test]
    fn an_unknown_privacy_value_becomes_normal_rather_than_sticking() {
        let document = ConfigDocument {
            privacy: "privacy".to_string(),
            ..ConfigDocument::default()
        };
        let text = document.to_json().replace("\"privacy\"", "\"weird\"");
        let back = ConfigDocument::from_json(&text).expect("parse");
        assert_eq!(back.privacy, "normal");
    }

    #[test]
    fn blank_emails_are_dropped() {
        let document = ConfigDocument {
            emails: vec!["  ".to_string(), "me@example.com".to_string()],
            ..ConfigDocument::default()
        };
        let back = ConfigDocument::from_json(&document.to_json()).expect("parse");
        assert_eq!(back.emails, vec!["me@example.com".to_string()]);
    }

    #[test]
    fn a_login_mapping_without_a_login_is_refused() {
        let mut document = ConfigDocument::default();
        document
            .github_logins
            .insert("   ".to_string(), vec!["me@example.com".to_string()]);
        assert!(matches!(
            ConfigDocument::from_json(&document.to_json()),
            Err(ConfigError::Invalid(_))
        ));
    }

    #[test]
    fn a_login_mapping_without_an_email_is_refused() {
        let mut document = ConfigDocument::default();
        document.github_logins.insert("me".to_string(), vec![]);
        assert!(matches!(
            ConfigDocument::from_json(&document.to_json()),
            Err(ConfigError::Invalid(_))
        ));
    }

    #[test]
    fn sound_overrides_survive_a_round_trip() {
        let mut document = ConfigDocument::default();
        document
            .sound_overrides
            .insert("rarity:rare".to_string(), "achievement".to_string());
        let back = ConfigDocument::from_json(&document.to_json()).expect("parse");
        assert_eq!(
            back.sound_overrides.get("rarity:rare").map(String::as_str),
            Some("achievement")
        );
    }

    #[test]
    fn a_legacy_document_without_new_fields_still_loads() {
        // Forward compatibility in the other direction: an export made before a field
        // existed must not be rejected.
        let text = r#"{
            "schemaVersion": 1,
            "emails": ["me@example.com"],
            "githubLogins": {},
            "privacy": "normal",
            "timezone": null,
            "notifications": true,
            "volume": 80,
            "muted": false,
            "soundOverrides": {},
            "popup": {
                "width": 364, "height": null, "scalePercent": 100,
                "iconSize": 52, "cornerRadius": 0, "screenMargin": 0,
                "corner": "bottom_right", "opacityPercent": 94, "displaySeconds": 5
            }
        }"#;
        let document = ConfigDocument::from_json(text).expect("parse");
        assert_eq!(document.emails, vec!["me@example.com".to_string()]);
    }
}
