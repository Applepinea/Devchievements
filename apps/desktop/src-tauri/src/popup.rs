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

/// Window size in logical pixels, matching the wide-and-short proportion Steam uses.
const WIDTH: f64 = 364.0;
const HEIGHT: f64 = 88.0;

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
    let window = build_window(app, &label, &url, true)
        .or_else(|_| build_window(app, &label, &url, false))
        .map_err(|e| e.to_string())?;

    place_bottom_right(&window);
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
) -> tauri::Result<tauri::WebviewWindow> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title("Devchievements")
        .inner_size(WIDTH, HEIGHT)
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
fn place_bottom_right(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let size = monitor.size();
    let position = monitor.position();
    let scale = monitor.scale_factor();

    let width = (WIDTH * scale) as i32;
    let height = (HEIGHT * scale) as i32;

    // Flush against the bottom-right corner: no margin, because the card is meant to
    // read as attached to the edge rather than floating near it.
    let x = position.x + size.width as i32 - width;
    let y = position.y + size.height as i32 - height;

    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
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
