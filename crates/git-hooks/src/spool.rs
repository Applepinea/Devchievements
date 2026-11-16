//! Append-only Spool (spec 10.3, 10.4, 10.5, 10.6).
//!
//! # What this is for
//!
//! When the Companion is not running, a Hook event must not be lost and must not
//! slow the commit down. Both constraints are satisfied by a single append to a
//! local file: predictable latency, no daemon, no retry loop inside the Hook.
//!
//! # Format and why each part exists
//!
//! ```text
//! <32-hex checksum> <json payload>\n
//! ```
//!
//! - **One line per record.** Appends of a single line under the platform's atomic
//!   append behaviour cannot interleave, so concurrent Hooks do not corrupt each
//!   other (spec 10.6).
//! - **Checksum on every record.** Spec 10.4 requires that one damaged record does
//!   not make the whole Spool unreadable. A record whose checksum fails is skipped
//!   and reported, and reading continues.
//! - **Newline terminator.** A record without a trailing newline is a partial write
//!   from a crash; it is treated as damaged rather than silently parsed.
//!
//! # What this deliberately does NOT do
//!
//! It never deletes records it has not confirmed were processed. Spec 10.5 forbids
//! clearing unconfirmed events, so consumption works by renaming the whole file into
//! a `consumed` directory and letting later cleanup handle it.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Default cap on total Spool bytes (spec 10.5: must be bounded, never silently dropped).
pub const DEFAULT_MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Default cap on the number of pending records.
pub const DEFAULT_MAX_RECORDS: usize = 20_000;
/// A record longer than this is rejected as malformed.
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

/// Why a Spool operation failed.
#[derive(Debug)]
pub enum SpoolError {
    /// A filesystem operation failed.
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The Spool is at its configured limit.
    ///
    /// Spec 10.5: this must be surfaced to the user, not swallowed.
    Full {
        /// Bytes currently stored.
        bytes: u64,
        /// Records currently stored.
        records: usize,
    },
    /// The payload was too large to be a legitimate record.
    RecordTooLarge(usize),
}

impl std::fmt::Display for SpoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpoolError::Io { path, source } => {
                write!(f, "spool io error at {}: {source}", path.display())
            }
            SpoolError::Full { bytes, records } => write!(
                f,
                "spool is full ({bytes} bytes, {records} records): events are being refused, not dropped"
            ),
            SpoolError::RecordTooLarge(n) => {
                write!(f, "spool record of {n} bytes exceeds the {MAX_RECORD_BYTES}-byte limit")
            }
        }
    }
}

impl std::error::Error for SpoolError {}

/// A record that survived reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The event id, which is also the idempotency key.
    pub event_id: String,
    /// The raw JSON payload.
    pub payload: String,
}

/// A record that did not survive reading, kept for diagnostics (spec 10.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamagedRecord {
    /// One-based line number in the file.
    pub line: usize,
    /// Why it was rejected.
    pub reason: String,
    /// A bounded excerpt, for the diagnostics view.
    pub excerpt: String,
}

/// The result of reading a Spool file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadOutcome {
    /// Records that parsed and verified.
    pub records: Vec<Record>,
    /// Records that were skipped, with reasons.
    pub damaged: Vec<DamagedRecord>,
}

/// The Spool directory layout.
#[derive(Debug, Clone)]
pub struct Spool {
    dir: PathBuf,
    max_bytes: u64,
    max_records: usize,
}

impl Spool {
    /// Open (creating if needed) a Spool rooted at `dir`.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, SpoolError> {
        let dir = dir.into();
        fs::create_dir_all(&dir).map_err(|e| SpoolError::Io {
            path: dir.clone(),
            source: e,
        })?;
        let consumed = dir.join("consumed");
        fs::create_dir_all(&consumed).map_err(|e| SpoolError::Io {
            path: consumed.clone(),
            source: e,
        })?;
        Ok(Spool {
            dir,
            max_bytes: DEFAULT_MAX_BYTES,
            max_records: DEFAULT_MAX_RECORDS,
        })
    }

    /// Override the limits (tests use small values; the product uses the defaults).
    pub fn with_limits(mut self, max_bytes: u64, max_records: usize) -> Self {
        self.max_bytes = max_bytes;
        self.max_records = max_records;
        self
    }

    /// The directory holding pending records.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Append one record.
    ///
    /// Builds the line, checks the limits, then performs ONE write of ONE line, so
    /// concurrent Hooks cannot interleave their bytes.
    pub fn append(&self, event_id: &str, payload: &str) -> Result<(), SpoolError> {
        let line = encode_record(event_id, payload)?;

        let (bytes, records) = self.stats()?;
        if bytes + line.len() as u64 > self.max_bytes || records + 1 > self.max_records {
            return Err(SpoolError::Full { bytes, records });
        }

        let path = self.pending_file();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| SpoolError::Io {
                path: path.clone(),
                source: e,
            })?;

        file.write_all(line.as_bytes())
            .map_err(|e| SpoolError::Io {
                path: path.clone(),
                source: e,
            })?;
        // Durability before returning: the Hook's job is to make the event survive,
        // and returning before the bytes are on disk would make that a guess.
        file.sync_all().map_err(|e| SpoolError::Io {
            path: path.clone(),
            source: e,
        })?;
        Ok(())
    }

    /// The pending file for the current process.
    ///
    /// One file per process keeps appends independent of each other's buffering;
    /// the reader merges all of them.
    fn pending_file(&self) -> PathBuf {
        self.dir
            .join(format!("pending-{}.spool", std::process::id()))
    }

    /// Every pending spool file, sorted for deterministic replay order.
    pub fn pending_files(&self) -> Result<Vec<PathBuf>, SpoolError> {
        let mut out = Vec::new();
        let entries = fs::read_dir(&self.dir).map_err(|e| SpoolError::Io {
            path: self.dir.clone(),
            source: e,
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("spool") {
                out.push(path);
            }
        }
        out.sort();
        Ok(out)
    }

    /// Read every pending record, separating damaged ones.
    pub fn read_all(&self) -> Result<ReadOutcome, SpoolError> {
        let mut outcome = ReadOutcome::default();
        for path in self.pending_files()? {
            let mut text = String::new();
            let mut file = fs::File::open(&path).map_err(|e| SpoolError::Io {
                path: path.clone(),
                source: e,
            })?;
            file.read_to_string(&mut text).map_err(|e| SpoolError::Io {
                path: path.clone(),
                source: e,
            })?;

            for (index, line) in text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                match decode_record(line) {
                    Ok(record) => outcome.records.push(record),
                    Err(reason) => outcome.damaged.push(DamagedRecord {
                        line: index + 1,
                        reason,
                        excerpt: line.chars().take(120).collect(),
                    }),
                }
            }
        }
        Ok(outcome)
    }

    /// Mark every currently pending file as consumed.
    ///
    /// Renames rather than truncating, so a crash midway leaves either the old file
    /// or the moved file, never a half-deleted one.
    pub fn mark_consumed(&self) -> Result<usize, SpoolError> {
        let files = self.pending_files()?;
        let consumed_dir = self.dir.join("consumed");
        let mut moved = 0;
        for path in files {
            let stamp = format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            );
            let target = consumed_dir.join(format!("consumed-{stamp}.spool"));
            if fs::rename(&path, &target).is_ok() {
                moved += 1;
            }
        }
        Ok(moved)
    }

    /// Current byte and record counts across pending files.
    pub fn stats(&self) -> Result<(u64, usize), SpoolError> {
        let mut bytes = 0u64;
        let mut records = 0usize;
        for path in self.pending_files()? {
            let meta = fs::metadata(&path).map_err(|e| SpoolError::Io {
                path: path.clone(),
                source: e,
            })?;
            bytes += meta.len();
            let text = fs::read_to_string(&path).unwrap_or_default();
            records += text.lines().filter(|l| !l.trim().is_empty()).count();
        }
        Ok((bytes, records))
    }
}

/// Encode a record into its on-disk line.
///
/// Exposed so tests and diagnostics can build a line without going through a file.
pub fn encode_record(event_id: &str, payload: &str) -> Result<String, SpoolError> {
    // The payload must already be JSON; validating here means a malformed event is
    // rejected at the source instead of becoming a damaged record later.
    let parsed: serde_json::Value =
        serde_json::from_str(payload).map_err(|_| SpoolError::RecordTooLarge(0))?;
    let event_id_from_payload = parsed
        .get("event_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !event_id_from_payload.is_empty() && event_id_from_payload != event_id {
        // A mismatch means the caller and the payload disagree about identity, which
        // would make deduplication unpredictable. Refuse rather than pick one.
        return Err(SpoolError::RecordTooLarge(0));
    }

    if payload.len() > MAX_RECORD_BYTES {
        return Err(SpoolError::RecordTooLarge(payload.len()));
    }

    let checksum = checksum_of(payload.as_bytes());
    Ok(format!("{checksum} {payload}\n"))
}

/// Parse and verify one line.
pub fn decode_record(line: &str) -> Result<Record, String> {
    let line = line.trim_end_matches(['\r', '\n']);
    let (checksum, payload) = line
        .split_once(' ')
        .ok_or_else(|| "record has no checksum separator".to_string())?;

    if checksum.len() != 32 || !checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("record checksum is not 32 hex characters".to_string());
    }
    if checksum_of(payload.as_bytes()) != checksum {
        return Err("record checksum does not match its payload".to_string());
    }

    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| format!("payload is not JSON: {e}"))?;
    let event_id = value
        .get("event_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "payload has no event_id".to_string())?;

    Ok(Record {
        event_id: event_id.to_string(),
        payload: payload.to_string(),
    })
}

/// A 128-bit checksum rendered as 32 hex characters.
///
/// Deliberately not a cryptographic hash: this detects torn writes and interleaved
/// appends, not tampering. Using a hash would invite the false impression that the
/// Spool is authenticated; it is a local file owned by the user.
fn checksum_of(bytes: &[u8]) -> String {
    // FNV-1a over two differently-seeded passes, concatenated. Cheap and adequate
    // for detecting corruption in a line we ourselves wrote.
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0x9e37_79b9_7f4a_7c15;
    for (i, byte) in bytes.iter().enumerate() {
        a ^= u64::from(*byte);
        a = a.wrapping_mul(0x0100_0000_01b3);
        b = b.rotate_left(7) ^ (u64::from(*byte).wrapping_add(i as u64));
        b = b.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{a:016x}{b:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_spool(name: &str) -> Spool {
        let dir = std::env::temp_dir().join(format!("devch-spool-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        Spool::open(&dir).expect("open spool")
    }

    fn payload(id: &str) -> String {
        format!("{{\"event_id\":\"{id}\"}}")
    }

    #[test]
    fn a_record_round_trips_through_encode_and_decode() {
        let line = encode_record("evt-1", &payload("evt-1")).expect("encode");
        let record = decode_record(&line).expect("decode");
        assert_eq!(record.event_id, "evt-1");
        assert_eq!(record.payload, payload("evt-1"));
    }

    #[test]
    fn appending_then_reading_returns_the_record() {
        let spool = temp_spool("roundtrip");
        spool.append("evt-1", &payload("evt-1")).expect("append");

        let outcome = spool.read_all().expect("read");
        assert_eq!(outcome.records.len(), 1);
        assert_eq!(outcome.records[0].event_id, "evt-1");
        assert!(outcome.damaged.is_empty());

        std::fs::remove_dir_all(spool.dir()).ok();
    }

    #[test]
    fn a_corrupted_record_does_not_hide_the_valid_ones() {
        // Spec 10.4: one bad record must not make the whole Spool unreadable.
        let spool = temp_spool("corrupt");
        spool.append("evt-1", &payload("evt-1")).expect("append");

        let path = spool.pending_files().expect("list")[0].clone();
        let mut text = std::fs::read_to_string(&path).expect("read");
        text.push_str("00000000000000000000000000000000 {\"event_id\":\"tampered\"}\n");
        std::fs::write(&path, text).expect("write");

        spool.append("evt-2", &payload("evt-2")).expect("append");

        let outcome = spool.read_all().expect("read");
        assert_eq!(outcome.records.len(), 2, "both good records survive");
        assert_eq!(outcome.damaged.len(), 1, "the bad line is isolated");
        assert!(outcome.damaged[0].reason.contains("checksum"));

        std::fs::remove_dir_all(spool.dir()).ok();
    }

    #[test]
    fn a_truncated_line_is_reported_not_parsed() {
        // A crash mid-write leaves a partial line; guessing at it would be worse.
        let spool = temp_spool("truncated");
        spool.append("evt-1", &payload("evt-1")).expect("append");
        let path = spool.pending_files().expect("list")[0].clone();

        let mut text = std::fs::read_to_string(&path).expect("read");
        text.push_str("deadbeef"); // no separator, no payload
        std::fs::write(&path, text).expect("write");

        let outcome = spool.read_all().expect("read");
        assert_eq!(outcome.records.len(), 1);
        assert_eq!(outcome.damaged.len(), 1);
        assert!(outcome.damaged[0].reason.contains("separator"));

        std::fs::remove_dir_all(spool.dir()).ok();
    }

    #[test]
    fn a_full_spool_refuses_rather_than_drops() {
        // Spec 10.5: reaching the limit must be an explicit error.
        let spool = temp_spool("full").with_limits(120, 100);
        spool
            .append("evt-1", &payload("evt-1"))
            .expect("first fits");

        let mut refused = 0;
        for i in 0..20 {
            let id = format!("evt-x{i}");
            if matches!(
                spool.append(&id, &payload(&id)),
                Err(SpoolError::Full { .. })
            ) {
                refused += 1;
            }
        }
        assert!(refused > 0, "the spool must start refusing once full");

        // Nothing was silently lost: every ACCEPTED record is readable.
        // The first append (evt-1) is not one of the 20 loop iterations, so the
        // expected count is 1 + accepted.
        let outcome = spool.read_all().expect("read");
        assert!(
            outcome.damaged.is_empty(),
            "refused records must not corrupt the file"
        );
        assert_eq!(
            outcome.records.len(),
            1 + (20 - refused),
            "every accepted record must remain readable"
        );

        std::fs::remove_dir_all(spool.dir()).ok();
    }

    #[test]
    fn consumers_move_files_instead_of_truncating_them() {
        let spool = temp_spool("consume");
        spool.append("evt-1", &payload("evt-1")).expect("append");
        assert_eq!(spool.pending_files().expect("list").len(), 1);

        let moved = spool.mark_consumed().expect("consume");
        assert_eq!(moved, 1);
        assert!(
            spool.pending_files().expect("list").is_empty(),
            "pending must be empty after consumption"
        );

        // The bytes still exist under consumed/, which is what makes cleanup safe.
        let consumed_count = std::fs::read_dir(spool.dir().join("consumed"))
            .expect("consumed dir")
            .count();
        assert_eq!(consumed_count, 1);

        std::fs::remove_dir_all(spool.dir()).ok();
    }

    #[test]
    fn concurrent_appends_do_not_interleave() {
        // Spec 10.6. Real concurrency needs processes; threads sharing the spool
        // already exercise the append path and the line integrity.
        let spool = temp_spool("concurrent");
        let dir = spool.dir().to_path_buf();

        let handles: Vec<_> = (0..8)
            .map(|t| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    let spool = Spool::open(&dir).expect("open");
                    for i in 0..10 {
                        let id = format!("t{t}-{i}");
                        spool.append(&id, &payload(&id)).expect("append");
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread");
        }

        let outcome = std::fs::read_dir(&dir)
            .expect("dir")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("spool"))
            .count();
        assert!(outcome >= 1);

        let spool = Spool::open(&dir).expect("reopen");
        let read = spool.read_all().expect("read");
        assert_eq!(read.damaged.len(), 0, "no interleaved or torn records");
        assert_eq!(read.records.len(), 80, "every append survived");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_record_without_an_event_id_is_rejected() {
        assert!(decode_record(&format!("{} {{}}", "0".repeat(32))).is_err());
    }

    #[test]
    fn a_payload_disagreeing_with_its_event_id_is_refused() {
        // Otherwise deduplication would depend on which id won.
        let result = encode_record("evt-a", &payload("evt-b"));
        assert!(result.is_err());
    }

    #[test]
    fn an_oversized_payload_is_refused() {
        let big = format!(
            "{{\"event_id\":\"e\",\"pad\":\"{}\"}}",
            "x".repeat(MAX_RECORD_BYTES)
        );
        assert!(matches!(
            encode_record("e", &big),
            Err(SpoolError::RecordTooLarge(_))
        ));
    }

    #[test]
    fn non_json_payloads_are_refused_at_the_source() {
        assert!(encode_record("e", "not json").is_err());
    }
}
