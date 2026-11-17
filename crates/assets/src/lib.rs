//! Validating user-supplied assets (spec 20, 23).
//!
//! # The rule this module exists to enforce
//!
//! An imported file is untrusted input. Two things must be true before it is stored:
//! the type must be what the bytes actually are, and the content must not be able to
//! execute anything.
//!
//! # Why the extension is not enough
//!
//! A file named icon.png can be an HTML document with a script tag, and a browser
//! asked to render it may run that script. The extension is a claim by the user about
//! their own file; a magic-number check is a fact about it. Only the latter can be
//! relied on.
//!
//! # Why SVG is treated separately
//!
//! SVG is a document format: it can carry script elements, event handlers and
//! external references. The spec allows it only after safe processing (spec 20), so it
//! is accepted solely when it contains none of the constructs that can execute. This
//! is a conservative allowlist: anything suspicious is refused rather than sanitised,
//! because a sanitiser that is subtly wrong fails open.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// Crate version, recorded alongside persisted data so diagnostics can attribute
/// results to the code that produced them.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why an asset was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetRejection {
    /// The bytes are not a format we accept.
    UnknownFormat,
    /// The file claims a type in its name that its bytes contradict.
    ExtensionMismatch {
        /// The extension the name suggested.
        claimed: String,
        /// What the bytes actually are.
        actual: String,
    },
    /// An SVG carried a construct that can execute or fetch.
    UnsafeSvg(String),
    /// The file exceeded the size limits.
    TooLarge(String),
}

impl std::fmt::Display for AssetRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetRejection::UnknownFormat => write!(f, "not a recognised image or audio format"),
            AssetRejection::ExtensionMismatch { claimed, actual } => {
                write!(f, "file is named .{claimed} but its bytes are {actual}")
            }
            AssetRejection::UnsafeSvg(what) => write!(f, "SVG contains {what}"),
            AssetRejection::TooLarge(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AssetRejection {}

/// A format we accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetFormat {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// WebP.
    WebP,
    /// SVG, only after the safety checks below.
    Svg,
    /// WAV audio (spec 19).
    Wav,
    /// MP3 audio (spec 19).
    Mp3,
    /// OGG audio (spec 19).
    Ogg,
}

impl AssetFormat {
    /// The canonical extension for this format.
    pub fn extension(self) -> &'static str {
        match self {
            AssetFormat::Png => "png",
            AssetFormat::Jpeg => "jpg",
            AssetFormat::WebP => "webp",
            AssetFormat::Svg => "svg",
            AssetFormat::Wav => "wav",
            AssetFormat::Mp3 => "mp3",
            AssetFormat::Ogg => "ogg",
        }
    }

    /// Whether this format is an image.
    pub fn is_image(self) -> bool {
        matches!(
            self,
            AssetFormat::Png | AssetFormat::Jpeg | AssetFormat::WebP | AssetFormat::Svg
        )
    }
}

/// Detect the real format from the leading bytes.
///
/// Text-based formats are matched after a BOM and whitespace are skipped, because SVG
/// files routinely start with an XML declaration.
pub fn detect(bytes: &[u8]) -> Option<AssetFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(AssetFormat::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(AssetFormat::Jpeg);
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Some(AssetFormat::Wav);
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(AssetFormat::WebP);
    }
    if bytes.starts_with(b"OggS") {
        return Some(AssetFormat::Ogg);
    }
    if bytes.starts_with(b"ID3") || bytes.starts_with(&[0xFF, 0xFB]) {
        return Some(AssetFormat::Mp3);
    }

    // SVG: an XML declaration or an <svg> root, after BOM and whitespace.
    let head = skip_preamble(bytes);
    let lowered = String::from_utf8_lossy(&head[..head.len().min(512)]).to_ascii_lowercase();
    if lowered.contains("<svg") {
        return Some(AssetFormat::Svg);
    }

    None
}

/// Skip a UTF-8 BOM and leading whitespace.
fn skip_preamble(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        start = 3;
    }
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    &bytes[start..]
}

/// Constructs that make an SVG able to execute or fetch.
///
/// Matched case-insensitively against the whole document. A false positive costs the
/// user one rejected file; a false negative runs their code.
const FORBIDDEN_SVG: &[&str] = &[
    "<script",
    "javascript:",
    "<foreignobject",
    "<iframe",
    "<embed",
    "<object",
    "<use",
    "xlink:href",
    "href=\"http",
    "href='http",
    "href=\"//",
    "href='//",
    "<!entity",
    "onload",
    "onerror",
    "onclick",
    "onmouseover",
];

/// Validate an SVG document.
///
/// Returns the reason it is unsafe, or None when it may be stored.
pub fn check_svg(bytes: &[u8]) -> Option<AssetRejection> {
    // A non-UTF-8 SVG cannot be checked reliably, so it is refused rather than
    // inspected lossily: lossy conversion could hide a construct.
    let text = match std::str::from_utf8(skip_preamble(bytes)) {
        Ok(text) => text,
        Err(_) => return Some(AssetRejection::UnsafeSvg("bytes that are not UTF-8".into())),
    };
    let lowered = text.to_ascii_lowercase();
    for marker in FORBIDDEN_SVG {
        if lowered.contains(marker) {
            return Some(AssetRejection::UnsafeSvg(format!("`{marker}`")));
        }
    }
    None
}

/// An asset that passed validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedAsset {
    /// What the bytes actually are.
    pub format: AssetFormat,
    /// Size in bytes.
    pub bytes: u64,
}

/// Validate an imported asset.
///
/// `file_name` is used only to detect a mismatch between claim and reality; the
/// decision never depends on it.
pub fn accept(
    file_name: &str,
    bytes: &[u8],
    current_total: u64,
) -> Result<AcceptedAsset, AssetRejection> {
    let size = bytes.len() as u64;

    // Quota first: it is cheap and it bounds everything that follows.
    devchievements_privacy::check_asset(size, current_total)
        .map_err(|e| AssetRejection::TooLarge(e.to_string()))?;

    let Some(format) = detect(bytes) else {
        return Err(AssetRejection::UnknownFormat);
    };

    // The name is a claim; the bytes are the fact. A mismatch is refused rather than
    // silently corrected, because it usually means the user picked the wrong file.
    if let Some(claimed) = extension_of(file_name) {
        let matches = matches!(
            (claimed.as_str(), format),
            ("png", AssetFormat::Png)
                | ("jpg" | "jpeg", AssetFormat::Jpeg)
                | ("webp", AssetFormat::WebP)
                | ("svg", AssetFormat::Svg)
                | ("wav", AssetFormat::Wav)
                | ("mp3", AssetFormat::Mp3)
                | ("ogg", AssetFormat::Ogg)
        );
        if !matches {
            return Err(AssetRejection::ExtensionMismatch {
                claimed,
                actual: format.extension().to_string(),
            });
        }
    }

    if format == AssetFormat::Svg {
        if let Some(rejection) = check_svg(bytes) {
            return Err(rejection);
        }
    }

    Ok(AcceptedAsset {
        format,
        bytes: size,
    })
}

/// Lowercased extension of a file name, without the dot.
fn extension_of(name: &str) -> Option<String> {
    let dot = name.rfind('.')?;
    let ext = name[dot + 1..].trim().to_ascii_lowercase();
    if ext.is_empty() {
        None
    } else {
        Some(ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];

    fn wav() -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&[0; 20]);
        v
    }

    #[test]
    fn a_png_is_recognised_from_its_bytes() {
        assert_eq!(detect(PNG), Some(AssetFormat::Png));
    }

    #[test]
    fn a_jpeg_is_recognised_from_its_bytes() {
        assert_eq!(detect(JPEG), Some(AssetFormat::Jpeg));
    }

    #[test]
    fn a_wav_is_recognised_from_its_header() {
        assert_eq!(detect(&wav()), Some(AssetFormat::Wav));
    }

    #[test]
    fn a_renamed_html_file_is_not_an_image() {
        // The attack this module exists to stop: a script document named .png.
        let html = b"<html><script>alert(1)</script></html>";
        assert_eq!(detect(html), None);
        assert!(matches!(
            accept("icon.png", html, 0),
            Err(AssetRejection::UnknownFormat)
        ));
    }

    #[test]
    fn a_file_whose_name_contradicts_its_bytes_is_refused() {
        assert!(matches!(
            accept("icon.jpg", PNG, 0),
            Err(AssetRejection::ExtensionMismatch { .. })
        ));
    }

    #[test]
    fn a_plain_svg_is_accepted() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><circle r="4"/></svg>"#;
        assert_eq!(detect(svg), Some(AssetFormat::Svg));
        assert!(accept("icon.svg", svg, 0).is_ok());
    }

    #[test]
    fn an_svg_with_a_script_is_refused() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        assert!(matches!(
            accept("icon.svg", svg, 0),
            Err(AssetRejection::UnsafeSvg(_))
        ));
    }

    #[test]
    fn an_svg_with_a_javascript_url_is_refused() {
        let svg =
            br#"<svg xmlns="http://www.w3.org/2000/svg"><a href="javascript:alert(1)"/></svg>"#;
        assert!(matches!(
            accept("icon.svg", svg, 0),
            Err(AssetRejection::UnsafeSvg(_))
        ));
    }

    #[test]
    fn an_svg_referencing_a_remote_document_is_refused() {
        let svg =
            br#"<svg xmlns="http://www.w3.org/2000/svg"><image href="http://evil/x.png"/></svg>"#;
        assert!(matches!(
            accept("icon.svg", svg, 0),
            Err(AssetRejection::UnsafeSvg(_))
        ));
    }

    #[test]
    fn an_svg_with_an_event_handler_is_refused() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"></svg>"#;
        assert!(matches!(
            accept("icon.svg", svg, 0),
            Err(AssetRejection::UnsafeSvg(_))
        ));
    }

    #[test]
    fn an_oversized_asset_is_refused_before_anything_else() {
        let big = vec![0u8; 10 * 1024 * 1024];
        assert!(matches!(
            accept("big.png", &big, 0),
            Err(AssetRejection::TooLarge(_))
        ));
    }

    #[test]
    fn a_leading_byte_order_mark_does_not_hide_an_svg() {
        let mut svg = vec![0xEF, 0xBB, 0xBF];
        svg.extend_from_slice(br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#);
        assert_eq!(detect(&svg), Some(AssetFormat::Svg));
    }

    #[test]
    fn a_file_without_an_extension_is_accepted_when_its_bytes_are_valid() {
        assert!(accept("icon", PNG, 0).is_ok());
    }

    #[test]
    fn crate_version_is_not_empty() {
        assert!(!CRATE_VERSION.is_empty());
    }
}
