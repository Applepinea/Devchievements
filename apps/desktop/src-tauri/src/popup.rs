//! The product's own notification popup (spec 28, 29.2).
//!
//! # Why this replaced the system toast
//!
//! Windows refused every toast from this binary with APPMODEL_ERROR_NO_PACKAGE, for
//! every AppUserModelID including Explorer's, while PowerShell could show one with the
//! same id (measured; see ADR-0015). A notification channel that cannot be demonstrated
//! is not a channel, so the popup -- which the product draws itself -- is the primary
//! and the toast is no longer used at all.
//!
//! # The constraints that shape it
//!
//! Spec 29.2: must not steal focus, must not appear in the taskbar, and must be
//! readable on the monitor the user is looking at. Those map to focus(false),
//! skip_taskbar(true) and a placement against the work area.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Identifier prefix, so popups never collide with the settings window.
const PREFIX: &str = "notify";

/// How long a popup stays before closing itself.
const LIFETIME: std::time::Duration = std::time::Duration::from_secs(5);

/// Fallback size, used when no style is configured.
///
/// The live size comes from the user's PopupStyle (spec 21); these exist so the
/// popup still has sane dimensions if the settings file cannot be read.
const FALLBACK_WIDTH: f64 = 364.0;
const FALLBACK_HEIGHT: f64 = 88.0;

/// Everything a popup needs to render.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopupPayload {
    /// Achievement name.
    pub title: String,
    /// Body copy, already passed through the privacy rules.
    pub body: String,
    /// Rarity label, used only for the accent colour and the text.
    pub rarity: String,
    /// Other achievements merged into this popup (spec 16.6 grouping).
    pub others: Vec<String>,
    /// Asset URL for this achievement's icon, when a custom one was imported.
    ///
    /// Resolved here rather than in the page: the page must not be able to probe
    /// the filesystem, and a missing asset has to degrade to nothing rather than to
    /// a broken image.
    pub icon_url: Option<String>,
}

/// Look for an icon for this achievement.
///
/// Naming convention: an imported file called `achievement-<id>.<ext>` overrides
/// the monogram for that achievement. A convention rather than a mapping table
/// keeps this stateless, and an id that is not a slug cannot match accidentally
/// because slugs cannot contain path separators.
pub fn icon_for(achievement_id: &str) -> Option<String> {
    if !achievement_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }

    let dir = crate::default_data_dir().join("assets");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return None;
    };

    let prefix = format!("achievement-{achievement_id}.");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(&prefix) {
            continue;
        }
        // Only formats the webview can render; an SVG here went through the asset
        // validator, so it contains no executable construct.
        let ext = name.rsplit('.').next().unwrap_or_default();
        if !matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "svg") {
            continue;
        }
        return Some(format!("asset://localhost/{}", entry.path().to_string_lossy().replace('\\', "/")));
    }
    None
}

/// Show a popup for one presentation.
///
/// Must be called on the main thread: WebView2 window creation on Windows has to
/// happen there, which is why callers go through run_on_main_thread.
pub fn show(app: &AppHandle, payload: &PopupPayload, counter: u64) -> Result<(), String> {
    let label = format!("{PREFIX}-{counter}");

    // The payload travels in the fragment so the page needs no IPC round trip before
    // it can draw; a popup that flashes empty is worse than one that appears late.
    let encoded = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    let url = format!("index.html#popup?data={}", urlencoding(&encoded));

    // Translucency is wanted (the popup floats over whatever the user is doing) but
    // it is the first thing that can fail on a given GPU or theme, so a failure here
    // falls back to an opaque window instead of losing the notification entirely.
    // Appearance comes from the user's settings (spec 21), already clamped by the
    // themes crate, so a hand-edited file cannot place the popup off-screen.
    let style = crate::settings::Settings::load(&crate::settings_path()).popup;
    println!(
        "POPUP icon_url={}",
        payload.icon_url.as_deref().unwrap_or("none")
    );

    let (width, height) = style.window_size();
    let size = (f64::from(width), f64::from(height));

    let window = build_window(app, &label, &url, true, size)
        .or_else(|_| build_window(app, &label, &url, false, size))
        .map_err(|e| e.to_string())?;

    // snake_case rather than the Debug spelling: the log is read by tests and by
    // anyone diagnosing a report, and `TopLeft` vs `top_left` is a needless
    // difference between what the docs say and what the log shows.
    let corner = serde_json::to_value(style.corner)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    println!(
        "POPUP style width={} corner={} scale={}",
        style.window_size().0,
        corner,
        style.scale_percent
    );
    place(&window, &style);
    let _ = window.show();

    // Close from the Rust side: the page does not need a window API, and a page that
    // crashes still leaves a popup that disappears.
    let handle = app.clone();
    let label = label.clone();
    std::thread::spawn(move || {
        std::thread::sleep(LIFETIME);
        if let Some(window) = handle.get_webview_window(&label) {
            let _ = window.close();
        }
    });

    Ok(())
}

/// Build the popup window, optionally with a transparent background.
fn build_window(
    app: &AppHandle,
    label: &str,
    url: &str,
    transparent: bool,
    size: (f64, f64),
) -> tauri::Result<tauri::WebviewWindow> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title("Devchievements")
        .inner_size(size.0, size.1)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        // Spec 29.2: a notification must not take the keyboard away from the user.
        .focused(false)
        .skip_taskbar(true)
        .transparent(transparent)
        .shadow(false)
        .visible(false)
        // Same profile as the main window; a second one would start another browser
        // process for no benefit.
        .data_directory(crate::default_data_dir().join("webview"))
        .build()
}

/// Put the popup in the bottom-right corner of the current monitor's work area.
fn place(window: &tauri::WebviewWindow, style: &devchievements_themes::PopupStyle) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let screen = monitor.size();
    let origin = monitor.position();
    let scale = monitor.scale_factor();

    // Monitor sizes are physical pixels; the style works in logical ones. Convert
    // by the scale factor so a 4K display does not produce a quarter-size popup.
    let logical_width = (screen.width as f64 / scale) as u32;
    let logical_height = (screen.height as f64 / scale) as u32;

    // The arithmetic lives in the themes crate so all four corners are testable
    // without a monitor.
    let (x, y) = style.position((logical_width, logical_height));

    let _ = window.set_position(tauri::PhysicalPosition::new(
        origin.x + (f64::from(x) * scale) as i32,
        origin.y + (f64::from(y) * scale) as i32,
    ));
}

/// Percent-encode the payload for the URL fragment.
///
/// Only the characters that would break a fragment need escaping; using a full
/// percent-encoder here would make the URL unreadable in logs for no benefit.
fn urlencoding(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
