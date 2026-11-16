//! Local IPC between the Hook Helper and Companion (spec 10.2, ADR-0007).
//!
//! # Why the timeouts are tiny
//!
//! The Helper runs inside `git commit`. §9.2 gives it a 200 ms hard budget and
//! §44.1 a 50 ms p95 target. Connecting locally should take single-digit
//! milliseconds, so a connection that has not completed in 30 ms is not going to
//! complete usefully; the Helper should write to the Spool instead and get out of
//! the user's way.
//!
//! # What this deliberately is not
//!
//! Not a general RPC channel. Messages are length-prefixed JSON with a hard size
//! cap, and the only thing the Companion can be asked to do is accept an event.
//! Spec 10.2: no command execution, no tokens, no remote access.
//!
//! # Platform status
//!
//! - **Windows**: implemented with Named Pipes.
//! - **Unix**: NOT implemented. Spec 10.2 requires SOCK_STREAM sockets under a
//!   0700 private directory with SO_PEERCRED validation. That code cannot be
//!   compiled or tested on this host, so it is deliberately absent rather than
//!   written blind. See the status note in STATUS.md.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// Hard cap on a single IPC frame (spec 10.2 requires a size limit).
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

/// How long the Helper waits to connect before giving up (ADR-0007).
pub const CONNECT_TIMEOUT_MS: u64 = 30;

/// How long the Helper waits for the Companion's acknowledgement.
pub const ACK_TIMEOUT_MS: u64 = 50;

/// The wire frame: a 4-byte little-endian length followed by UTF-8 JSON.
///
/// The same framing is used by the Native Messaging host, because that is what the
/// browser requires; sharing it here means one parser to audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The JSON payload.
    pub payload: String,
}

/// Why a frame could not be built or parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// The payload was larger than the frame limit.
    TooLarge(usize),
    /// The declared length did not match the bytes available.
    Truncated {
        /// Length the header declared.
        declared: usize,
        /// Bytes actually present.
        available: usize,
    },
    /// The payload was not valid UTF-8.
    NotUtf8,
    /// The declared length exceeded the hard cap.
    ExceedsLimit(usize),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::TooLarge(n) => write!(
                f,
                "frame of {n} bytes exceeds the {MAX_FRAME_BYTES}-byte limit"
            ),
            FrameError::Truncated {
                declared,
                available,
            } => write!(
                f,
                "frame declared {declared} bytes but only {available} were available"
            ),
            FrameError::NotUtf8 => write!(f, "frame payload is not valid UTF-8"),
            FrameError::ExceedsLimit(n) => write!(
                f,
                "frame declares {n} bytes, beyond the {MAX_FRAME_BYTES}-byte cap"
            ),
        }
    }
}

impl std::error::Error for FrameError {}

impl Frame {
    /// Build a frame from a JSON payload.
    pub fn new(payload: impl Into<String>) -> Result<Self, FrameError> {
        let payload = payload.into();
        if payload.len() > MAX_FRAME_BYTES {
            return Err(FrameError::TooLarge(payload.len()));
        }
        Ok(Frame { payload })
    }

    /// Serialise to the length-prefixed wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.payload.len());
        out.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(self.payload.as_bytes());
        out
    }

    /// Parse one frame from the start of a buffer.
    ///
    /// Returns the frame and how many bytes it consumed, so a caller reading a
    /// stream can loop without re-scanning.
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), FrameError> {
        if bytes.len() < 4 {
            return Err(FrameError::Truncated {
                declared: 4,
                available: bytes.len(),
            });
        }
        let declared = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

        // Check the cap BEFORE allocating: a hostile or corrupt length must not make
        // us reserve gigabytes (spec 10.2 size validation).
        if declared > MAX_FRAME_BYTES {
            return Err(FrameError::ExceedsLimit(declared));
        }
        if bytes.len() < 4 + declared {
            return Err(FrameError::Truncated {
                declared,
                available: bytes.len() - 4,
            });
        }

        let payload = std::str::from_utf8(&bytes[4..4 + declared])
            .map_err(|_| FrameError::NotUtf8)?
            .to_string();

        Ok((Frame { payload }, 4 + declared))
    }
}

/// The pipe name the Companion listens on.
///
/// Includes a user discriminator so two logged-in users on the same machine cannot
/// connect to each other's Companion. This is defence in depth: the Named Pipe ACL
/// is the primary control (spec 10.2).
pub fn pipe_name_for(user_discriminator: &str) -> String {
    format!("\\.\\pipe\\devchievements-{user_discriminator}")
}

/// The result of trying to deliver an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// The Companion acknowledged receipt.
    Acknowledged,
    /// No Companion was reachable in time.
    ///
    /// Not an error: it is the normal path when the Companion is closed, and the
    /// caller writes to the Spool instead.
    Unreachable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_round_trips() {
        let frame = Frame::new("{\"event_id\":\"e1\"}").expect("frame");
        let encoded = frame.encode();
        let (decoded, consumed) = Frame::decode(&encoded).expect("decode");
        assert_eq!(decoded, frame);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn two_frames_in_one_buffer_are_read_sequentially() {
        let a = Frame::new("{\"n\":1}").expect("a").encode();
        let b = Frame::new("{\"n\":2}").expect("b").encode();
        let mut buffer = a.clone();
        buffer.extend_from_slice(&b);

        let (first, used) = Frame::decode(&buffer).expect("first");
        assert_eq!(first.payload, "{\"n\":1}");
        let (second, _) = Frame::decode(&buffer[used..]).expect("second");
        assert_eq!(second.payload, "{\"n\":2}");
    }

    #[test]
    fn an_oversized_declared_length_is_rejected_before_allocating() {
        // A corrupt or hostile header must not cause a huge allocation (spec 10.2).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(u32::MAX).to_le_bytes());
        bytes.extend_from_slice(b"payload");
        assert!(matches!(
            Frame::decode(&bytes),
            Err(FrameError::ExceedsLimit(_))
        ));
    }

    #[test]
    fn a_truncated_frame_reports_the_deficit() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(b"short");
        match Frame::decode(&bytes) {
            Err(FrameError::Truncated {
                declared,
                available,
            }) => {
                assert_eq!(declared, 10);
                assert_eq!(available, 5);
            }
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    #[test]
    fn an_incomplete_header_is_truncated_not_a_panic() {
        assert!(matches!(
            Frame::decode(&[1, 2]),
            Err(FrameError::Truncated { .. })
        ));
    }

    #[test]
    fn invalid_utf8_in_the_payload_is_reported() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&[0xff, 0xfe]);
        assert_eq!(Frame::decode(&bytes), Err(FrameError::NotUtf8));
    }

    #[test]
    fn an_oversized_outgoing_payload_is_refused() {
        let big = "x".repeat(MAX_FRAME_BYTES + 1);
        assert!(matches!(Frame::new(big), Err(FrameError::TooLarge(_))));
    }

    #[test]
    fn the_pipe_name_is_namespaced_per_user() {
        // Two users on one machine must not share a pipe.
        let a = pipe_name_for("user-a");
        let b = pipe_name_for("user-b");
        assert_ne!(a, b);
        assert!(a.contains("devchievements-"));
    }

    #[test]
    fn the_timeouts_fit_inside_the_hook_budget() {
        // Connect + ack must stay a small fraction of the 200 ms hard timeout, otherwise
        // the IPC path itself becomes the reason a commit feels slow.
        //
        // Compared through bindings so this stays a real runtime check rather than a
        // constant assertion the compiler folds away.
        let connect: u64 = CONNECT_TIMEOUT_MS;
        let ack: u64 = ACK_TIMEOUT_MS;
        assert!(connect + ack < 200, "ipc path must fit in the hook budget");
        assert!(connect <= 50, "connecting must not dominate the budget");
    }
}
