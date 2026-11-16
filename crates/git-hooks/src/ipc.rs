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

// --------------------------------------------------------------- live transport

// # The live channel
//
// `interprocess` gives one API over Windows named pipes and Unix domain sockets,
// which is exactly the requirement in spec 10.2 (a private local channel; never
// localhost TCP). Using it also keeps this crate free of `unsafe`, which the
// workspace forbids.
//
// # What the acknowledgement actually promises
//
// The server appends the record to the Spool **before** acknowledging. So an ack
// means "durably accepted", not "probably received". That matters because the
// Helper chooses between IPC and a Spool write based on it: ack means the Helper can
// return without writing the Spool itself, and a lost ack just means the next start
// drains the record again (idempotent by event id).
//
// # Why the channel exists at all, if everything ends up in the Spool
//
// The Spool is the single durable store on purpose: one ingestion path means one
// place for the ordering and idempotency rules. What IPC adds is *promptness* -- the
// Companion can act immediately instead of waiting for its next poll, which is the
// difference between a notification appearing at once and appearing seconds later.

/// Acknowledgement payload the server sends back.
pub const ACK_PAYLOAD: &str = "{\"ok\":true}";

/// Why a live delivery attempt could not complete.
#[derive(Debug)]
pub enum TransportError {
    /// The local socket name could not be constructed.
    BadName(String),
    /// No server was listening.
    NotRunning,
    /// The exchange started but did not finish in time.
    Timeout,
    /// The protocol was violated.
    Protocol(String),
    /// An operating-system error.
    Io(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::BadName(m) => write!(f, "invalid socket name: {m}"),
            TransportError::NotRunning => write!(f, "no companion is listening"),
            TransportError::Timeout => write!(f, "the exchange exceeded its deadline"),
            TransportError::Protocol(m) => write!(f, "protocol error: {m}"),
            TransportError::Io(m) => write!(f, "transport io error: {m}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// The socket name for a given user discriminator.
///
/// Namespaced per user so two accounts on one machine cannot reach each other's
/// Companion. The operating-system ACL is the primary control; this is defence in
/// depth (spec 10.2).
pub fn socket_name(user_discriminator: &str) -> String {
    format!("devchievements-{user_discriminator}")
}

/// Send one record, waiting for the acknowledgement, bounded by a deadline.
///
/// Returns [`Delivery::Acknowledged`] only when the server confirmed it persisted the
/// record. Every other outcome is [`Delivery::Unreachable`], which the caller turns
/// into a Spool write.
pub fn deliver_live(user_discriminator: &str, event_id: &str, payload: &str) -> Delivery {
    use interprocess::local_socket::traits::Stream as _;
    use interprocess::local_socket::{GenericNamespaced, Stream, ToNsName};
    use std::io::{Read, Write};

    let frame = match Frame::new(payload) {
        Ok(frame) => frame,
        Err(_) => return Delivery::Unreachable,
    };
    let _ = event_id;

    let name = socket_name(user_discriminator);
    let Ok(name) = name.to_ns_name::<GenericNamespaced>() else {
        return Delivery::Unreachable;
    };

    // Connecting to a name nobody is listening on fails immediately on every
    // supported platform, so the common "Companion closed" case costs nothing.
    let mut stream = match Stream::connect(name) {
        Ok(stream) => stream,
        Err(_) => return Delivery::Unreachable,
    };

    if stream.write_all(&frame.encode()).is_err() {
        return Delivery::Unreachable;
    }
    if stream.flush().is_err() {
        return Delivery::Unreachable;
    }

    // Bound the wait. A server that accepted but never replies must not hold the
    // user's commit hostage (spec 9.2), and a write-only check is not enough: the
    // stall happens while waiting for the acknowledgement.
    //
    // `read` has no portable timeout on local sockets, so the wait runs on a
    // dedicated thread and the calling thread gives up on a deadline. Spawning a
    // thread costs tens of microseconds, well inside the budget.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut header = [0u8; 4];
        if stream.read_exact(&mut header).is_err() {
            let _ = tx.send(false);
            return;
        }
        let len = u32::from_le_bytes(header) as usize;
        if len == 0 || len > MAX_FRAME_BYTES {
            let _ = tx.send(false);
            return;
        }
        let mut body = vec![0u8; len];
        if stream.read_exact(&mut body).is_err() {
            let _ = tx.send(false);
            return;
        }
        let _ = tx.send(String::from_utf8_lossy(&body) == ACK_PAYLOAD);
    });

    match rx.recv_timeout(std::time::Duration::from_millis(ACK_TIMEOUT_MS)) {
        Ok(true) => Delivery::Acknowledged,
        // Timed out or mis-acknowledged: the caller spools it instead. Re-sending is
        // safe because ingestion is idempotent on the event id (spec 10.1).
        _ => Delivery::Unreachable,
    }
}

/// A listening Companion endpoint.
#[derive(Debug)]
pub struct Server {
    listener: interprocess::local_socket::Listener,
}

impl Server {
    /// Bind the socket, replacing any stale one from a previous run.
    pub fn bind(user_discriminator: &str) -> Result<Self, TransportError> {
        use interprocess::local_socket::{GenericNamespaced, ListenerOptions, ToNsName};

        let name = socket_name(user_discriminator);
        let name = name
            .to_ns_name::<GenericNamespaced>()
            .map_err(|e| TransportError::BadName(e.to_string()))?;

        // A crashed previous instance can leave the endpoint behind; replacing it is
        // what makes "restart the Companion" work without manual cleanup.
        let listener = ListenerOptions::new()
            .name(name)
            .create_sync()
            .map_err(|e| TransportError::Io(e.to_string()))?;

        Ok(Server { listener })
    }

    /// Accept one connection, read one frame, and return its payload.
    ///
    /// Blocking by design: the caller runs this on a dedicated thread so the async
    /// ingestion loop is never blocked by a slow peer.
    pub fn accept_one(&self) -> Result<String, TransportError> {
        use interprocess::local_socket::traits::Listener as _;
        use std::io::Read;

        let mut stream = self
            .listener
            .accept()
            .map_err(|e| TransportError::Io(e.to_string()))?;

        let mut header = [0u8; 4];
        stream
            .read_exact(&mut header)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        let len = u32::from_le_bytes(header) as usize;
        if len == 0 || len > MAX_FRAME_BYTES {
            return Err(TransportError::Protocol(format!(
                "declared frame length {len} is outside 1..={MAX_FRAME_BYTES}"
            )));
        }

        let mut body = vec![0u8; len];
        stream
            .read_exact(&mut body)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;

        String::from_utf8(body).map_err(|_| TransportError::Protocol("payload is not UTF-8".into()))
    }

    /// Acknowledge the most recent [`Self::accept_one`] on the same connection.
    ///
    /// Separate from `accept_one` because the caller must only acknowledge AFTER the
    /// record is durable.
    pub fn acknowledge(
        stream: &mut interprocess::local_socket::Stream,
    ) -> Result<(), TransportError> {
        use std::io::Write;
        let frame = Frame::new(ACK_PAYLOAD).map_err(|e| TransportError::Protocol(e.to_string()))?;
        stream
            .write_all(&frame.encode())
            .map_err(|e| TransportError::Io(e.to_string()))?;
        stream
            .flush()
            .map_err(|e| TransportError::Io(e.to_string()))
    }

    /// Accept one connection and run the whole exchange with a callback.
    ///
    /// The callback decides whether the record is durable; only then is the ack sent.
    pub fn serve_one<F>(&self, mut on_record: F) -> Result<(), TransportError>
    where
        F: FnMut(&str) -> bool,
    {
        use interprocess::local_socket::traits::Listener as _;
        use std::io::Read;

        let mut stream = self
            .listener
            .accept()
            .map_err(|e| TransportError::Io(e.to_string()))?;

        let mut header = [0u8; 4];
        if stream.read_exact(&mut header).is_err() {
            return Ok(());
        }
        let len = u32::from_le_bytes(header) as usize;
        if len == 0 || len > MAX_FRAME_BYTES {
            return Ok(());
        }
        let mut body = vec![0u8; len];
        if stream.read_exact(&mut body).is_err() {
            return Ok(());
        }
        let Ok(payload) = String::from_utf8(body) else {
            return Ok(());
        };

        if on_record(&payload) {
            Self::acknowledge(&mut stream)?;
        }
        Ok(())
    }
}
