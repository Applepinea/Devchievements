//! Popup appearance (spec 21) and theme packages (spec 23).
//!
//! # Why the ranges live here
//!
//! Spec 21 fixes numeric ranges for width, scale, radius, margin and so on. A UI
//! slider can express any number, so the range has to be enforced somewhere that a
//! test can reach; otherwise an imported theme can set width to 4000 and the popup
//! covers the screen.
//!
//! # Why clamping is not silent
//!
//! Every setter returns what it actually stored. A caller that asked for 900 and got
//! 600 can say so; a silent clamp would leave the user wondering why their setting
//! does not stick.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// Crate version, recorded alongside persisted data so diagnostics can attribute
/// results to the code that produced them.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where on the screen the popup appears (spec 21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    /// Top left.
    TopLeft,
    /// Top right.
    TopRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom right.
    #[default]
    BottomRight,
}

/// The popup's appearance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopupStyle {
    /// Width in logical pixels.
    pub width: u32,
    /// Height in logical pixels; None means it grows with the content.
    pub height: Option<u32>,
    /// Scale in percent.
    pub scale_percent: u32,
    /// Icon size in logical pixels.
    pub icon_size: u32,
    /// Corner radius in logical pixels.
    pub corner_radius: u32,
    /// Distance from the screen edge in logical pixels.
    pub screen_margin: u32,
    /// Which corner to sit in.
    pub corner: Corner,
    /// Opacity in percent.
    pub opacity_percent: u32,
    /// How long it stays, in seconds.
    pub display_seconds: u32,
    /// Entrance animation duration in milliseconds (spec 21).
    ///
    /// Zero means no animation. Stored in milliseconds because that is what a CSS
    /// transition takes; storing seconds would force a lossy conversion at the edge.
    #[serde(default = "default_animation_ms")]
    pub animation_ms: u32,
    /// Background blur radius in logical pixels (spec 21).
    ///
    /// Zero means no blur. A blurred background is a real cost on some GPUs, so it
    /// is opt-in rather than part of the default look.
    #[serde(default)]
    pub background_blur: u32,
}

/// The default entrance animation duration.
fn default_animation_ms() -> u32 {
    240
}

impl Default for PopupStyle {
    fn default() -> Self {
        PopupStyle {
            width: 364,
            height: None,
            scale_percent: 100,
            icon_size: 52,
            corner_radius: 0,
            screen_margin: 0,
            corner: Corner::BottomRight,
            opacity_percent: 94,
            display_seconds: 5,
            animation_ms: 240,
            background_blur: 0,
        }
    }
}

/// A range a value must fall in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// Smallest allowed value.
    pub min: u32,
    /// Largest allowed value.
    pub max: u32,
}

impl Range {
    /// Clamp a value into this range.
    pub fn clamp(self, value: u32) -> u32 {
        value.clamp(self.min, self.max)
    }

    /// Whether a value is inside the range.
    pub fn contains(self, value: u32) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

/// The ranges from spec 21, in one place.
pub mod limits {
    use super::Range;

    /// Popup width.
    pub const WIDTH: Range = Range { min: 280, max: 600 };
    /// Popup height, when fixed.
    pub const HEIGHT: Range = Range { min: 80, max: 400 };
    /// Scale.
    pub const SCALE: Range = Range { min: 70, max: 150 };
    /// Icon size.
    pub const ICON: Range = Range { min: 32, max: 160 };
    /// Corner radius.
    pub const RADIUS: Range = Range { min: 0, max: 32 };
    /// Screen margin.
    pub const MARGIN: Range = Range { min: 0, max: 100 };
    /// Display duration in seconds.
    pub const DISPLAY_SECONDS: Range = Range { min: 1, max: 30 };
    /// Opacity.
    pub const OPACITY: Range = Range { min: 20, max: 100 };
    /// Entrance animation duration in milliseconds.
    pub const ANIMATION_MS: Range = Range { min: 0, max: 1000 };
    /// Background blur radius.
    pub const BLUR: Range = Range { min: 0, max: 24 };
}

impl PopupStyle {
    /// Clamp every field into its documented range.
    ///
    /// Applied after loading any style from disk or from a theme package, because
    /// those inputs are untrusted (spec 23) and an out-of-range width would put the
    /// popup off the screen.
    pub fn clamped(mut self) -> Self {
        self.width = limits::WIDTH.clamp(self.width);
        self.height = self.height.map(|h| limits::HEIGHT.clamp(h));
        self.scale_percent = limits::SCALE.clamp(self.scale_percent);
        self.icon_size = limits::ICON.clamp(self.icon_size);
        self.corner_radius = limits::RADIUS.clamp(self.corner_radius);
        self.screen_margin = limits::MARGIN.clamp(self.screen_margin);
        self.opacity_percent = limits::OPACITY.clamp(self.opacity_percent);
        self.display_seconds = limits::DISPLAY_SECONDS.clamp(self.display_seconds);
        self.animation_ms = limits::ANIMATION_MS.clamp(self.animation_ms);
        self.background_blur = limits::BLUR.clamp(self.background_blur);
        self
    }

    /// The physical size of the window, after scale.
    pub fn window_size(&self) -> (u32, u32) {
        let scale = self.scale_percent as f64 / 100.0;
        let width = (self.width as f64 * scale).round() as u32;
        let height = (self.height.unwrap_or(88) as f64 * scale).round() as u32;
        (width, height)
    }

    /// Top-left position for a window of this size inside a usable area.
    ///
    /// `screen` is the area the window may occupy, not the monitor: a window placed
    /// against the bottom of the monitor disappears behind the taskbar, which is what
    /// a zero margin used to do. Pure arithmetic so placement is testable without a
    /// monitor.
    pub fn position(&self, screen: (u32, u32)) -> (i32, i32) {
        let (width, height) = self.window_size();
        // A small gap is enforced even when the user asks for zero: touching the edge
        // looks like a rendering bug and, at the bottom, is hidden outright.
        let margin = (self.screen_margin as i32).max(MIN_SCREEN_MARGIN);
        let (sw, sh) = (screen.0 as i32, screen.1 as i32);
        let x = match self.corner {
            Corner::TopLeft | Corner::BottomLeft => margin,
            Corner::TopRight | Corner::BottomRight => sw - width as i32 - margin,
        };
        let y = match self.corner {
            Corner::TopLeft | Corner::TopRight => margin,
            Corner::BottomLeft | Corner::BottomRight => sh - height as i32 - margin,
        };
        // Never let a small screen or a large popup push the window off the area.
        (
            x.clamp(0, (sw - width as i32).max(0)),
            y.clamp(0, (sh - height as i32).max(0)),
        )
    }
}

/// Why a theme package was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeRejection {
    /// The package declared a schema this build does not understand.
    UnsupportedSchema(u32),
    /// The package referenced an asset that failed validation.
    BadAsset(String),
    /// The package carried a field that would allow code to run.
    ContainsCode(String),
}

impl std::fmt::Display for ThemeRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeRejection::UnsupportedSchema(v) => {
                write!(f, "theme schema {v} is not supported")
            }
            ThemeRejection::BadAsset(what) => write!(f, "theme asset rejected: {what}"),
            ThemeRejection::ContainsCode(what) => write!(f, "theme contains {what}"),
        }
    }
}

impl std::error::Error for ThemeRejection {}

/// The smallest gap kept between a popup and the edge of the usable area.
///
/// Logical pixels. Zero looks like a clipped window, and at the bottom it is clipped.
pub const MIN_SCREEN_MARGIN: i32 = 16;

/// The schema version this build writes and accepts.
pub const THEME_SCHEMA_VERSION: u32 = 1;

/// Validated theme metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    /// Schema version.
    pub schema_version: u32,
    /// Display name.
    pub name: String,
    /// Popup appearance.
    pub popup: PopupStyle,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            schema_version: THEME_SCHEMA_VERSION,
            name: "Default".to_string(),
            popup: PopupStyle::default(),
        }
    }
}

/// Keys that would let a theme run code (spec 23).
const FORBIDDEN_KEYS: &[&str] = &["script", "javascript", "eval", "css", "stylesheet", "html"];

/// Parse and validate a theme package.
///
/// The package is JSON. Anything that could carry executable behaviour is refused
/// rather than stripped: a theme is data, and a field we do not understand is a field
/// we cannot promise is safe.
pub fn parse_theme(json: &str) -> Result<Theme, ThemeRejection> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| ThemeRejection::BadAsset(e.to_string()))?;

    // Reject any key that could introduce code, anywhere in the document.
    fn scan(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::Object(map) => {
                for (key, inner) in map {
                    let lowered = key.to_ascii_lowercase();
                    if FORBIDDEN_KEYS.iter().any(|k| lowered.contains(k)) {
                        return Some(format!("`{key}`"));
                    }
                    if let Some(found) = scan(inner) {
                        return Some(found);
                    }
                }
                None
            }
            serde_json::Value::Array(items) => items.iter().find_map(scan),
            serde_json::Value::String(text) if text.contains("javascript:") => {
                Some("a javascript: reference".to_string())
            }
            _ => None,
        }
    }
    if let Some(found) = scan(&value) {
        return Err(ThemeRejection::ContainsCode(found));
    }

    // The version is checked BEFORE the full parse: a newer schema may legitimately
    // omit or rename fields this build requires, and reporting "missing field" for a
    // file that is merely too new sends the user looking in the wrong place.
    if let Some(version) = value.get("schemaVersion").and_then(|v| v.as_u64()) {
        if version > u64::from(THEME_SCHEMA_VERSION) {
            return Err(ThemeRejection::UnsupportedSchema(version as u32));
        }
    }

    let theme: Theme =
        serde_json::from_value(value).map_err(|e| ThemeRejection::BadAsset(e.to_string()))?;

    Ok(Theme {
        // Clamp on the way in, so an imported theme cannot place the popup off-screen.
        popup: theme.popup.clamped(),
        ..theme
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_style_is_inside_every_range() {
        let style = PopupStyle::default();
        assert!(limits::WIDTH.contains(style.width));
        assert!(limits::SCALE.contains(style.scale_percent));
        assert!(limits::ICON.contains(style.icon_size));
        assert!(limits::RADIUS.contains(style.corner_radius));
        assert!(limits::MARGIN.contains(style.screen_margin));
        assert!(limits::DISPLAY_SECONDS.contains(style.display_seconds));
        assert!(limits::OPACITY.contains(style.opacity_percent));
    }

    #[test]
    fn a_width_outside_the_range_is_clamped_not_kept() {
        let style = PopupStyle {
            width: 4000,
            ..PopupStyle::default()
        }
        .clamped();
        assert_eq!(style.width, limits::WIDTH.max);
    }

    #[test]
    fn a_too_small_width_is_clamped_up() {
        let style = PopupStyle {
            width: 10,
            ..PopupStyle::default()
        }
        .clamped();
        assert_eq!(style.width, limits::WIDTH.min);
    }

    #[test]
    fn scale_is_clamped_to_the_documented_range() {
        for (input, expected) in [(10, 70), (999, 150), (110, 110)] {
            let style = PopupStyle {
                scale_percent: input,
                ..PopupStyle::default()
            }
            .clamped();
            assert_eq!(style.scale_percent, expected, "scale {input}");
        }
    }

    #[test]
    fn a_fixed_height_is_clamped_but_auto_height_is_kept() {
        let auto = PopupStyle::default().clamped();
        assert_eq!(auto.height, None, "auto must stay auto");

        let fixed = PopupStyle {
            height: Some(1000),
            ..PopupStyle::default()
        }
        .clamped();
        assert_eq!(fixed.height, Some(limits::HEIGHT.max));
    }

    #[test]
    fn the_window_size_follows_the_scale() {
        let style = PopupStyle {
            width: 400,
            height: Some(100),
            scale_percent: 150,
            ..PopupStyle::default()
        };
        assert_eq!(style.window_size(), (600, 150));
    }

    #[test]
    fn an_auto_height_window_still_has_a_size() {
        let (_, height) = PopupStyle::default().window_size();
        assert!(height > 0, "a window cannot be zero tall");
    }

    #[test]
    fn each_corner_places_the_window_where_it_says() {
        let screen = (1920, 1080);
        let base = PopupStyle {
            width: 400,
            height: Some(100),
            screen_margin: 20,
            corner_radius: 0,
            ..PopupStyle::default()
        };

        let top_left = PopupStyle {
            corner: Corner::TopLeft,
            ..base.clone()
        };
        assert_eq!(top_left.position(screen), (20, 20));

        let bottom_right = PopupStyle {
            corner: Corner::BottomRight,
            ..base.clone()
        };
        assert_eq!(
            bottom_right.position(screen),
            (1920 - 400 - 20, 1080 - 100 - 20)
        );

        let top_right = PopupStyle {
            corner: Corner::TopRight,
            ..base.clone()
        };
        assert_eq!(top_right.position(screen), (1920 - 400 - 20, 20));

        let bottom_left = PopupStyle {
            corner: Corner::BottomLeft,
            ..base
        };
        assert_eq!(bottom_left.position(screen), (20, 1080 - 100 - 20));
    }

    #[test]
    fn a_default_theme_round_trips_through_json() {
        let theme = Theme::default();
        let json = serde_json::to_string(&theme).expect("serialise");
        let back = parse_theme(&json).expect("parse");
        assert_eq!(back, theme);
    }

    #[test]
    fn an_imported_theme_with_a_huge_width_is_clamped() {
        let json = r#"{
            "schemaVersion": 1,
            "name": "Huge",
            "popup": {
                "width": 5000,
                "height": null,
                "scalePercent": 100,
                "iconSize": 52,
                "cornerRadius": 0,
                "screenMargin": 0,
                "corner": "bottom_right",
                "opacityPercent": 94,
                "displaySeconds": 5
            }
        }"#;
        let theme = parse_theme(json).expect("parse");
        assert_eq!(theme.popup.width, limits::WIDTH.max);
    }

    #[test]
    fn a_theme_containing_a_script_key_is_refused() {
        let json = r#"{ "schemaVersion": 1, "name": "Bad", "script": "alert(1)" }"#;
        assert!(matches!(
            parse_theme(json),
            Err(ThemeRejection::ContainsCode(_))
        ));
    }

    #[test]
    fn a_theme_containing_a_stylesheet_key_is_refused() {
        let json = r#"{ "schemaVersion": 1, "name": "Bad", "css": "body{}" }"#;
        assert!(matches!(
            parse_theme(json),
            Err(ThemeRejection::ContainsCode(_))
        ));
    }

    #[test]
    fn a_theme_with_a_javascript_url_is_refused() {
        let json =
            r#"{ "schemaVersion": 1, "name": "Bad", "popup": {}, "note": "javascript:alert(1)" }"#;
        assert!(matches!(
            parse_theme(json),
            Err(ThemeRejection::ContainsCode(_))
        ));
    }

    #[test]
    fn a_future_schema_version_is_refused_rather_than_guessed() {
        let json = r#"{ "schemaVersion": 99, "name": "Future", "popup": {} }"#;
        assert!(matches!(
            parse_theme(json),
            Err(ThemeRejection::UnsupportedSchema(99))
        ));
    }

    #[test]
    fn crate_version_is_not_empty() {
        assert!(!CRATE_VERSION.is_empty());
    }

    #[test]
    fn animation_and_blur_default_inside_their_ranges() {
        let style = PopupStyle::default();
        assert!(limits::ANIMATION_MS.contains(style.animation_ms));
        assert!(limits::BLUR.contains(style.background_blur));
    }

    #[test]
    fn an_overlong_animation_is_clamped() {
        let style = PopupStyle {
            animation_ms: 99_999,
            ..PopupStyle::default()
        }
        .clamped();
        assert_eq!(style.animation_ms, limits::ANIMATION_MS.max);
    }

    #[test]
    fn a_zero_animation_is_allowed_because_it_means_no_animation() {
        let style = PopupStyle {
            animation_ms: 0,
            ..PopupStyle::default()
        }
        .clamped();
        assert_eq!(style.animation_ms, 0, "zero must not be clamped up");
    }

    #[test]
    fn an_excessive_blur_is_clamped() {
        let style = PopupStyle {
            background_blur: 500,
            ..PopupStyle::default()
        }
        .clamped();
        assert_eq!(style.background_blur, limits::BLUR.max);
    }

    #[test]
    fn a_theme_without_the_new_fields_still_loads() {
        // An export or theme written before these properties existed must not break.
        let json = r#"{
            "schemaVersion": 1,
            "name": "Old",
            "popup": {
                "width": 364, "height": null, "scalePercent": 100,
                "iconSize": 52, "cornerRadius": 0, "screenMargin": 0,
                "corner": "bottom_right", "opacityPercent": 94, "displaySeconds": 5
            }
        }"#;
        let theme = parse_theme(json).expect("parse");
        assert_eq!(theme.popup.animation_ms, 240);
        assert_eq!(theme.popup.background_blur, 0);
    }

    #[test]
    fn a_zero_margin_still_keeps_a_gap_from_the_edge() {
        // Reported from the app: a popup with margin 0 sat against the bottom edge and
        // the taskbar covered it, so the preview looked like it did nothing.
        let style = PopupStyle {
            screen_margin: 0,
            corner: Corner::BottomRight,
            ..PopupStyle::default()
        };
        let (width, height) = style.window_size();
        let (x, y) = style.position((1920, 1040));
        assert_eq!(x, 1920 - width as i32 - MIN_SCREEN_MARGIN);
        assert_eq!(y, 1040 - height as i32 - MIN_SCREEN_MARGIN);
        assert!(y + height as i32 <= 1040 - MIN_SCREEN_MARGIN);
    }

    #[test]
    fn a_popup_wider_than_the_area_does_not_produce_a_negative_coordinate() {
        // A negative position puts the title bar off-screen, where a window cannot be
        // dragged back. Only the axis that overflows is pinned; the other keeps its
        // corner, which is why this asserts them separately.
        let style = PopupStyle {
            width: 4000,
            ..PopupStyle::default()
        };
        let (x, y) = style.position((800, 600));
        assert_eq!(x, 0, "a popup wider than the area starts at its left edge");
        let (_, height) = style.window_size();
        assert!(
            (0..=600 - height as i32).contains(&y),
            "y stays inside the area: {y}"
        );
    }

    #[test]
    fn the_configured_margin_is_honoured_when_it_is_larger_than_the_minimum() {
        let style = PopupStyle {
            screen_margin: 40,
            corner: Corner::TopLeft,
            ..PopupStyle::default()
        };
        assert_eq!(style.position((1920, 1040)), (40, 40));
    }
}
