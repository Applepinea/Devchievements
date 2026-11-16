//! Native Messaging host.
//!
//! This process is a short-lived bridge that the browser launches on demand.
//! It is NOT the Companion: the Companion is the single data authority and the
//! only SQLite writer (spec 18.2).
//!
//! Protocol: 4-byte native-endian length prefix followed by UTF-8 JSON.
//! That framing is specified by the browser, not by us.
//!
//! Phase 0 only proves the framing round-trips. Handshake, version negotiation
//! and event forwarding arrive in Phase 3b.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::io::{self, Read, Write};

fn main() -> io::Result<()> {
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
        if len == 0 {
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

/// Minimal message handler: echoes a typed envelope without trusting the input.
///
/// Every field is validated before use; nothing from the browser is treated as
/// a command (spec 18.4).
fn handle(raw: &[u8]) -> serde_json::Value {
    match serde_json::from_slice::<serde_json::Value>(raw) {
        Ok(v) => serde_json::json!({
            "ok": true,
            "echo_type": v.get("type").and_then(|t| t.as_str()).unwrap_or("<none>"),
            "protocol_version": 0,
            "stage": "phase0-framing-only"
        }),
        Err(_) => serde_json::json!({
            "ok": false,
            "error": "invalid_json"
        }),
    }
}
