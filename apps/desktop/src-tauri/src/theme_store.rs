//! Where user theme packages live (spec 21, spec 23).
//!
//! # Why a file of its own
//!
//! A theme is a list of packages, not a single value, and `settings.json` is written by
//! the tray menu, the CLI and the settings page. Keeping themes in a second file means
//! importing one cannot race a settings save, and a corrupt themes file costs the user
//! their extra packages rather than their identity and popup preferences.
//!
//! # What is trusted
//!
//! Nothing on disk. Every package is read through
//! `devchievements_themes::parse_theme`, which refuses code-bearing keys and clamps
//! every range, and the byte cap below is enforced *before* the parse so a huge file
//! costs a length check instead of a parse.

use std::path::{Path, PathBuf};

use devchievements_themes::{parse_theme, PopupStyle, Theme, THEME_SCHEMA_VERSION};

/// Largest theme package accepted from disk, in bytes.
///
/// A theme is a name and a dozen integers; the largest real package written by this
/// app is under 1 KiB. The cap is generous and still small enough that the size check,
/// not the parser, is what bounds the work an import can cause.
pub const MAX_THEME_BYTES: usize = 64 * 1024;

/// Longest theme name accepted, in characters.
///
/// The name is shown in a list next to a button, so a 10 KB name would break the row
/// rather than say anything.
pub const MAX_NAME_CHARS: usize = 60;

/// Name of the built-in theme.
///
/// It is synthesised rather than stored, so it cannot be missing, replaced, or left
/// behind by a failed write.
pub const BUILTIN_NAME: &str = "Default";

/// The name an export uses when no stored theme matches the style in effect.
pub const CUSTOM_NAME: &str = "Custom";

/// Path of the themes file, next to `settings.json` and `companion.db`.
pub fn themes_path() -> PathBuf {
    crate::default_data_dir().join("themes.json")
}

/// One row of the theme list, as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSummary {
    /// Display name.
    pub name: String,
    /// Whether this entry is built in and therefore read-only.
    pub builtin: bool,
}

/// What the settings page needs to render the theme section.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeListView {
    /// Built-in first, then user themes in import order.
    pub themes: Vec<ThemeSummary>,
    /// Name of the theme whose popup style is in effect.
    ///
    /// `None` means the stored style matches no theme: the user moved a slider, so the
    /// page says "modified" rather than claiming a theme is active.
    pub active: Option<String>,
}

/// The on-disk document.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemesFile {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    themes: Vec<Theme>,
}

/// Read the stored packages.
///
/// A missing or unreadable file reads as "no user themes": the built-in is always
/// available, so the page still works and the user can re-import. A file that cannot
/// be parsed is deliberately not treated as fatal for the same reason.
pub fn load_themes(path: &Path) -> Vec<Theme> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    // The document shape is the current one; a bare array is what a hand-written file
    // is most likely to contain, and accepting it costs one branch.
    if let Ok(file) = serde_json::from_str::<ThemesFile>(&text) {
        return file.themes;
    }
    serde_json::from_str::<Vec<Theme>>(&text).unwrap_or_default()
}

/// Write the stored packages, creating the parent directory if needed.
pub fn save_themes(path: &Path, themes: &[Theme]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let document = ThemesFile {
        schema_version: THEME_SCHEMA_VERSION,
        themes: themes.to_vec(),
    };
    // Serialising a struct of plain fields cannot fail; mapping instead of
    // defaulting keeps a future field that can fail from writing an empty file.
    let text = serde_json::to_string_pretty(&document)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, text + "\n")
}

/// Read one package from disk and validate it.
///
/// The error is the crate's own wording, passed through unchanged: it names the field
/// or the schema version, and rewriting it here would only make it vaguer.
pub fn read_theme_file(path: &Path) -> Result<Theme, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() > MAX_THEME_BYTES {
        return Err(format!(
            "theme is {} bytes but the limit is {} bytes",
            bytes.len(),
            MAX_THEME_BYTES
        ));
    }
    let text = String::from_utf8(bytes).map_err(|_| "theme is not valid UTF-8".to_string())?;
    parse_theme(&text).map_err(|e| e.to_string())
}

/// Import a package: validate it, then store it under its own name.
///
/// Re-importing a name replaces that entry, which is how a theme gets updated: the
/// alternative is two identically named rows and no way to tell which is in effect.
pub fn import_theme(source: &Path, store: &Path) -> Result<Theme, String> {
    let mut theme = read_theme_file(source)?;

    let name = theme.name.trim().to_string();
    if name.is_empty() {
        return Err("theme has no name".to_string());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "theme name is {} characters but the limit is {MAX_NAME_CHARS}",
            name.chars().count()
        ));
    }
    if name.eq_ignore_ascii_case(BUILTIN_NAME) {
        return Err(format!("{BUILTIN_NAME} is built in and cannot be replaced"));
    }
    theme.name = name;

    let mut themes = load_themes(store);
    match themes.iter().position(|t| t.name == theme.name) {
        Some(index) => themes[index] = theme.clone(),
        None => themes.push(theme.clone()),
    }
    save_themes(store, &themes).map_err(|e| e.to_string())?;
    Ok(theme)
}

/// The popup style a named theme stands for.
pub fn popup_for(name: &str, store: &Path) -> Result<PopupStyle, String> {
    if name == BUILTIN_NAME {
        return Ok(PopupStyle::default());
    }
    load_themes(store)
        .into_iter()
        .find(|t| t.name == name)
        .map(|t| t.popup)
        // Clamped on the way out too: a hand-edited themes.json is not trusted any
        // more than an imported file, and an out-of-range width puts the popup
        // off-screen.
        .map(PopupStyle::clamped)
        .ok_or_else(|| format!("no theme named `{name}`"))
}

/// Delete a stored theme.
///
/// Refuses the built-in name explicitly rather than relying on it being absent from
/// the file: a hand-written `themes.json` could otherwise contain a "Default" entry,
/// and deleting it would look like the built-in had been removed.
pub fn remove_theme(name: &str, store: &Path) -> Result<(), String> {
    if name == BUILTIN_NAME {
        return Err(format!("{BUILTIN_NAME} is built in and cannot be deleted"));
    }
    let mut themes = load_themes(store);
    let before = themes.len();
    themes.retain(|t| t.name != name);
    if themes.len() == before {
        return Err(format!("no theme named `{name}`"));
    }
    save_themes(store, &themes).map_err(|e| e.to_string())
}

/// Build the list the page shows.
pub fn list_themes(store: &Path, active: &PopupStyle) -> ThemeListView {
    let themes = load_themes(store);
    let mut summaries = vec![ThemeSummary {
        name: BUILTIN_NAME.to_string(),
        builtin: true,
    }];
    summaries.extend(themes.iter().map(|t| ThemeSummary {
        name: t.name.clone(),
        builtin: false,
    }));

    // "Active" means the style in effect equals a theme exactly. The built-in wins a
    // tie: a stored theme identical to the default would otherwise make the row the
    // user just picked look inactive after a reset.
    let active_name = if *active == PopupStyle::default() {
        Some(BUILTIN_NAME.to_string())
    } else {
        themes
            .iter()
            .find(|t| t.popup == *active)
            .map(|t| t.name.clone())
    };

    ThemeListView {
        themes: summaries,
        active: active_name,
    }
}

/// Write one package, built from the style in effect, to a path the user picked.
///
/// Export takes the current style rather than a stored theme on purpose: what a user
/// wants to share is the popup they are looking at, including a slider they moved.
pub fn export_theme(popup: &PopupStyle, name: &str, destination: &Path) -> Result<(), String> {
    let theme = Theme {
        schema_version: THEME_SCHEMA_VERSION,
        name: name.to_string(),
        // cloned() because clamped() consumes the style; the caller still owns theirs.
        popup: popup.clone().clamped(),
    };
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&theme).map_err(|e| e.to_string())?;
    std::fs::write(destination, text + "\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that removes itself, so a failing test leaves no state
    /// behind for the next run to read.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("devchievements-theme-store-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Scratch(dir)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn package(name: &str) -> String {
        serde_json::json!({
            "schemaVersion": 1,
            "name": name,
            "popup": {
                "width": 480,
                "height": null,
                "scalePercent": 110,
                "iconSize": 64,
                "cornerRadius": 12,
                "screenMargin": 24,
                "corner": "top_left",
                "opacityPercent": 90,
                "displaySeconds": 6,
                "animationMs": 300,
                "backgroundBlur": 8
            }
        })
        .to_string()
    }

    #[test]
    fn an_oversized_package_is_refused_before_it_is_parsed() {
        let scratch = Scratch::new("oversized");
        let file = scratch.file("big.json");
        // A valid package padded past the cap: the refusal must come from the size,
        // not from a parse error, or the message would not name the limit.
        let mut text = package("Big");
        text.push_str(&" ".repeat(MAX_THEME_BYTES));
        std::fs::write(&file, text).expect("write");

        let error = read_theme_file(&file).expect_err("must be refused");
        assert!(error.contains("limit is 65536 bytes"), "{error}");
    }

    #[test]
    fn a_script_bearing_package_is_refused_with_the_crates_wording() {
        let scratch = Scratch::new("script");
        let file = scratch.file("script.json");
        let mut value: serde_json::Value =
            serde_json::from_str(&package("Scripted")).expect("json");
        value["script"] = serde_json::json!("alert(1)");
        std::fs::write(&file, value.to_string()).expect("write");

        let error = read_theme_file(&file).expect_err("must be refused");
        assert_eq!(error, "theme contains `script`");
    }

    #[test]
    fn an_imported_theme_is_stored_under_its_own_name() {
        let scratch = Scratch::new("import");
        let source = scratch.file("neon.json");
        let store = scratch.file("themes.json");
        std::fs::write(&source, package("Neon")).expect("write");

        let imported = import_theme(&source, &store).expect("import");
        assert_eq!(imported.name, "Neon");

        let stored = load_themes(&store);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].popup.width, 480);
    }

    #[test]
    fn importing_the_same_name_replaces_that_entry() {
        let scratch = Scratch::new("replace");
        let store = scratch.file("themes.json");
        let first = scratch.file("first.json");
        let second = scratch.file("second.json");
        std::fs::write(&first, package("Neon")).expect("write");
        import_theme(&first, &store).expect("first import");

        let mut value: serde_json::Value = serde_json::from_str(&package("Neon")).expect("json");
        value["popup"]["width"] = serde_json::json!(300);
        std::fs::write(&second, value.to_string()).expect("write");
        import_theme(&second, &store).expect("second import");

        let stored = load_themes(&store);
        assert_eq!(stored.len(), 1, "a re-import must not add a second row");
        assert_eq!(stored[0].popup.width, 300);
    }

    #[test]
    fn the_builtin_name_cannot_be_shadowed() {
        let scratch = Scratch::new("builtin");
        let source = scratch.file("default.json");
        let store = scratch.file("themes.json");
        std::fs::write(&source, package("Default")).expect("write");

        let error = import_theme(&source, &store).expect_err("must be refused");
        assert!(error.contains("built in"), "{error}");
        assert!(load_themes(&store).is_empty());
    }

    #[test]
    fn a_corrupt_store_reads_as_no_user_themes() {
        let scratch = Scratch::new("corrupt");
        let store = scratch.file("themes.json");
        std::fs::write(&store, "{ this is not json").expect("write");

        // The page must still render, so a broken list is not an error.
        assert!(load_themes(&store).is_empty());
        let view = list_themes(&store, &PopupStyle::default());
        assert_eq!(view.themes.len(), 1);
        assert!(view.themes[0].builtin);
        assert_eq!(view.active.as_deref(), Some(BUILTIN_NAME));
    }

    #[test]
    fn the_list_marks_the_style_in_effect_and_otherwise_reports_no_active_theme() {
        let scratch = Scratch::new("list");
        let store = scratch.file("themes.json");
        let source = scratch.file("neon.json");
        std::fs::write(&source, package("Neon")).expect("write");
        let imported = import_theme(&source, &store).expect("import");

        let view = list_themes(&store, &imported.popup);
        assert_eq!(view.active.as_deref(), Some("Neon"));
        assert_eq!(view.themes.len(), 2);
        assert!(view.themes[1].name == "Neon" && !view.themes[1].builtin);

        // A moved slider matches nothing, and saying so is the point.
        let tweaked = PopupStyle {
            width: 301,
            ..imported.popup.clone()
        };
        assert_eq!(list_themes(&store, &tweaked).active, None);
    }

    #[test]
    fn an_export_imports_back_unchanged() {
        let scratch = Scratch::new("export");
        let exported = scratch.file("shared.json");
        let store = scratch.file("themes.json");
        let style = PopupStyle {
            width: 520,
            corner_radius: 18,
            ..PopupStyle::default()
        };

        export_theme(&style, "Shared", &exported).expect("export");
        let text = std::fs::read_to_string(&exported).expect("read");
        assert!(text.contains("\"schemaVersion\": 1"), "{text}");

        let imported = import_theme(&exported, &store).expect("re-import");
        assert_eq!(imported.name, "Shared");
        assert_eq!(imported.popup, style);
    }
}
