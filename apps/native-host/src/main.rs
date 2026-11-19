//! Native Messaging host.
//!
//! This process is launched by the browser on demand and lives only as long as the
//! extension keeps the port open. It is NOT the Companion: the Companion is the
//! single data authority and the only SQLite writer (spec 18.2).
//!
//! # What it does
//!
//! 0. refuses to run unless the browser says it launched us for our own extension
//!    (see `verify_origin`); that check runs before stdin is touched, so an
//!    unexpected caller never gets a reply and never reaches the Spool;
//! 1. reads length-prefixed JSON from stdin (framing is the browser's, not ours);
//! 2. validates a GitHub report against the same rules the desktop side uses;
//! 3. appends it to the Spool, which the Companion already drains;
//! 4. answers so the extension knows whether to keep a local copy.
//!
//! Everything that decides meaning lives in `devchievements_git_hooks::browser_event`,
//! so the rules are unit-tested without a browser.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::io::{self, Read, Write};
use std::path::PathBuf;

use devchievements_git_hooks::browser_event;
use devchievements_git_hooks::spool::Spool;

fn main() -> io::Result<()> {
    // Before anything else: a host that answers the wrong caller is worse than a host
    // that refuses to start, because the Spool would accept whatever was sent.
    if let Err(reason) = verify_origin(std::env::args().nth(1).as_deref()) {
        eprintln!("devchievements-native-host: {reason}");
        // Non-zero so the browser logs a failed launch instead of a silent port.
        std::process::exit(2);
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();

    loop {
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf) {
            Ok(()) => {}
            // Clean EOF: the browser closed the pipe.
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
        let len = u32::from_ne_bytes(len_buf) as usize;
        // The browser caps messages at 1 MiB in and 4 KiB out; anything larger is a
        // protocol violation rather than something to buffer.
        if len == 0 || len > 1024 * 1024 {
            continue;
        }
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;

        let reply = handle(&buf);
        let out = serde_json::to_vec(&reply).unwrap_or_else(|_| b"{}".to_vec());
        stdout.write_all(&(out.len() as u32).to_ne_bytes())?;
        stdout.write_all(&out)?;
        stdout.flush()?;
    }

    Ok(())
}

/// The only caller allowed to talk to this host: the extension that ships with the app.
///
/// The browser passes the caller's origin as the first argument, and that is the one
/// field a page cannot forge -- unlike anything inside the message body, which is just
/// JSON we were handed. The ID is fixed by the extension's public key
/// (`apps/extension/extension-key.pub`), so it survives rebuilds and matches what
/// `scripts/dev/register-native-host.mjs` writes into the manifest.
const EXPECTED_ORIGIN: &str = "chrome-extension://gfgbmedanlpminnlbbdefojbgphoekbj/";

/// Check the origin the browser passed as `argv[1]`.
///
/// Compared as an exact string: no case folding, no trailing-slash tolerance. Chrome
/// and Edge both pass `chrome-extension://<id>/` with the slash, so being strict costs
/// nothing while a lenient comparison would let a look-alike through.
///
/// Returns the reason on refusal so the caller can log something a user can act on.
/// This is a pure function rather than a `process::exit` buried in `main` so the
/// rules above are unit-tested instead of being taken on faith.
fn verify_origin(origin: Option<&str>) -> Result<(), String> {
    match origin {
        Some(given) if given == EXPECTED_ORIGIN => Ok(()),
        Some(given) => Err(format!(
            "refusing caller origin {given:?}: this host only serves {EXPECTED_ORIGIN}"
        )),
        None => Err(format!(
            "refusing caller with no origin argument: expected {EXPECTED_ORIGIN}"
        )),
    }
}

/// Where the Spool lives.
///
/// `DEVCH_DATA_DIR` exists so a test can point the host at a temporary directory; in
/// normal use the same LOCALAPPDATA path as the hook helper is used.
fn data_dir() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("DEVCH_DATA_DIR") {
        return Some(PathBuf::from(explicit));
    }
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("devchievements"))
}

/// Handle one message and produce the reply.
///
/// Nothing from the browser is trusted: the report must parse, and the spool must be
/// writable. Every failure is reported to the extension instead of being swallowed,
/// because a silent drop looks identical to a working host.
fn handle(raw: &[u8]) -> serde_json::Value {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return serde_json::json!({ "type": "devch:ack", "stored": false, "reason": "invalid json" });
    };

    let Some(payload) = value.get("payload") else {
        return serde_json::json!({ "type": "devch:ack", "stored": false, "reason": "missing payload" });
    };

    let Some(report) = browser_event::parse(payload) else {
        return serde_json::json!({ "type": "devch:ack", "stored": false, "reason": "payload not understood" });
    };

    let Some(dir) = data_dir() else {
        return serde_json::json!({ "type": "devch:ack", "stored": false, "reason": "no data directory" });
    };

    let spool = match Spool::open(dir.join("spool")) {
        Ok(spool) => spool,
        Err(e) => {
            return serde_json::json!({ "type": "devch:ack", "stored": false, "reason": format!("spool unavailable: {e}") });
        }
    };

    // The payload handed to the Companion is the validated report, not the raw
    // browser message: the Companion must never re-parse untrusted input.
    let record = serde_json::json!({
        "kind": "browser_github",
        "event_id": report.event_id,
        "repository_id": report.repository_id,
        "activity": match &report.activity {
            browser_event::BrowserActivity::Commit { .. } => "commit",
            browser_event::BrowserActivity::Release { .. } => "release",
            browser_event::BrowserActivity::PullRequest { .. } => "pull_request",
            browser_event::BrowserActivity::Issue { .. } => "issue",
        },
        "commit_sha": match &report.activity {
            browser_event::BrowserActivity::Commit { sha } => Some(sha.clone()),
            _ => None,
        },
        "external_id": match &report.activity {
            browser_event::BrowserActivity::Commit { .. } => None,
            browser_event::BrowserActivity::Release { tag } => Some(tag.clone()),
            browser_event::BrowserActivity::PullRequest { number, .. } => {
                Some(format!("pr:{number}"))
            }
            browser_event::BrowserActivity::Issue { number, .. } => {
                Some(format!("issue:{number}"))
            }
        },
        // The flags that separate opened from merged/closed must survive this hop;
        // dropping them here would silently reclassify every PR as merely opened.
        "merged": match &report.activity {
            browser_event::BrowserActivity::PullRequest { merged, .. } => Some(*merged),
            _ => None,
        },
        "closed": match &report.activity {
            browser_event::BrowserActivity::Issue { closed, .. } => Some(*closed),
            _ => None,
        },
        "number": match &report.activity {
            browser_event::BrowserActivity::PullRequest { number, .. }
            | browser_event::BrowserActivity::Issue { number, .. } => Some(*number),
            _ => None,
        },
        "occurred_ms": report.occurred_ms,
        // The attribution has to survive this hop. Dropping it here made the
        // companion refuse every browser report, and the record looked perfectly
        // well-formed from the host's point of view.
        "author_login": report.author_login,
    })
    .to_string();

    match spool.append(&report.event_id, &record) {
        Ok(()) => serde_json::json!({ "type": "devch:ack", "stored": true }),
        Err(e) => {
            serde_json::json!({ "type": "devch:ack", "stored": false, "reason": format!("spool full or unwritable: {e}") })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(bytes: &[u8]) -> serde_json::Value {
        handle(bytes)
    }

    #[test]
    fn the_shipping_extension_is_accepted() {
        assert!(
            verify_origin(Some("chrome-extension://gfgbmedanlpminnlbbdefojbgphoekbj/")).is_ok()
        );
    }

    #[test]
    fn another_extension_is_refused() {
        // Same shape, different ID: a second copy of the extension, or one rebuilt
        // with a different key, looks exactly like this.
        let err = verify_origin(Some("chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"))
            .expect_err("another extension must not be served");
        assert!(err.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), "{err}");
        assert!(err.contains("gfgbmedanlpminnlbbdefojbgphoekbj"), "{err}");
    }

    #[test]
    fn a_web_origin_is_refused() {
        assert!(verify_origin(Some("http://localhost:3000")).is_err());
        assert!(verify_origin(Some("https://github.com")).is_err());
    }

    #[test]
    fn a_missing_origin_is_refused() {
        // Launched by hand, or by something that is not a browser at all.
        let err = verify_origin(None).expect_err("no origin must not be served");
        assert!(err.contains("gfgbmedanlpminnlbbdefojbgphoekbj"), "{err}");
    }

    #[test]
    fn a_near_miss_origin_is_refused() {
        // Case and the trailing slash are the two ways a lenient comparison would
        // silently widen the check.
        assert!(verify_origin(Some(
            "chrome-extension://GFGBMEDANLPMINNLBBDEFOJBGFPHOEKBJ/"
        ))
        .is_err());
        assert!(
            verify_origin(Some("chrome-extension://gfgbmedanlpminnlbbdefojbgphoekbj")).is_err()
        );
        assert!(verify_origin(Some(
            "chrome-extension://gfgbmedanlpminnlbbdefojbgphoekbj//"
        ))
        .is_err());
    }

    #[test]
    fn a_valid_report_is_stored() {
        let dir = std::env::temp_dir().join(format!("devch-host-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::env::set_var("DEVCH_DATA_DIR", &dir);

        let raw = serde_json::json!({
            "type": "devch:github_event",
            "payload": {
                "kind": "browser_github",
                "event_id": "browser:github:o/r#abcdef0123456789abcdef0123456789abcdef01",
                "repository_id": "github:o/r",
                "activity": "commit",
                "commit_sha": "abcdef0123456789abcdef0123456789abcdef01",
                "occurred_ms": 1_700_000_000_000i64,
            }
        })
        .to_string();
        let out = reply(raw.as_bytes());
        assert_eq!(out["stored"], serde_json::json!(true), "{out}");

        std::env::remove_var("DEVCH_DATA_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_malformed_message_is_refused_with_a_reason() {
        let out = reply(b"not json");
        assert_eq!(out["stored"], serde_json::json!(false));
        assert!(out["reason"].as_str().is_some_and(|r| !r.is_empty()));
    }

    #[test]
    fn a_message_without_a_payload_is_refused() {
        let out = reply(br#"{"type":"devch:github_event"}"#);
        assert_eq!(out["stored"], serde_json::json!(false));
    }

    #[test]
    fn an_unparseable_report_is_refused() {
        let raw = serde_json::json!({
            "type": "devch:github_event",
            "payload": { "kind": "browser_github", "event_id": "x" }
        })
        .to_string();
        let out = reply(raw.as_bytes());
        assert_eq!(out["stored"], serde_json::json!(false));
    }
}
