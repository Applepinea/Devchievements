//! Packfile reading (ADR-0013, risk A3 in the risk analysis).
//!
//! # Why this exists
//!
//! Phase 0's reader only handled loose objects. Real repositories get `git gc`'d,
//! and after that every object lives in a `.pack` file, frequently as a delta
//! against another object. The measured consequence is total failure:
//!
//! ```text
//! loose object (.../objects/7d/b0bb...): The system cannot find the path specified
//! ```
//!
//! That is not an edge case. It is "the feature stops working after a week".
//!
//! # Scope
//!
//! Implemented: pack index v2 parsing, pack header/object header parsing, zlib
//! inflate of the object body, and delta application (`OFS_DELTA` and `REF_DELTA`),
//! including delta chains.
//!
//! NOT implemented, and reported as errors rather than guessed:
//! - pack index v1 (pre-2007; only if a repository is very old);
//! - thin packs (a pack that references objects outside itself);
//! - SHA-256 repositories.
//!
//! # The one rule
//!
//! Every failure is a typed error. A reader that returns "no commit" when it cannot
//! decode one would silently lose events, which the spec forbids outright.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::gitread::inflate::zlib_inflate;

/// Why a packfile could not be used.
#[derive(Debug)]
pub enum PackError {
    /// The `.idx` file could not be read.
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The index is not in the v2 format this reader supports.
    UnsupportedIndexVersion(u32),
    /// The pack header is malformed.
    BadHeader(String),
    /// The object was not present in this pack.
    ObjectNotFound(String),
    /// The object type is not one this reader understands.
    UnsupportedObjectType(u8),
    /// A delta could not be applied.
    BadDelta(String),
    /// Decompression failed.
    Inflate(String),
    /// A delta chain exceeded the safety limit.
    DeltaChainTooDeep(usize),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::Io { path, source } => write!(f, "io error at {}: {source}", path.display()),
            PackError::UnsupportedIndexVersion(v) => {
                write!(f, "pack index version {v} is not supported (need v2)")
            }
            PackError::BadHeader(m) => write!(f, "bad pack header: {m}"),
            PackError::ObjectNotFound(o) => write!(f, "object {o} is not in this pack"),
            PackError::UnsupportedObjectType(t) => write!(f, "unsupported object type code {t}"),
            PackError::BadDelta(m) => write!(f, "bad delta: {m}"),
            PackError::Inflate(m) => write!(f, "inflate failed: {m}"),
            PackError::DeltaChainTooDeep(n) => {
                write!(f, "delta chain deeper than the {n}-level safety limit")
            }
        }
    }
}

impl std::error::Error for PackError {}

/// Maximum delta chain depth we will follow.
///
/// Git's own default packing depth is 50, so 128 is comfortably above anything a
/// real repository produces while still bounding the work an adversarial pack can
/// cause.
const MAX_DELTA_DEPTH: usize = 128;

/// The kind of object stored in a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    /// A commit object.
    Commit,
    /// A tree object.
    Tree,
    /// A blob.
    Blob,
    /// An annotated tag.
    Tag,
}

impl ObjectKind {
    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(ObjectKind::Commit),
            2 => Some(ObjectKind::Tree),
            3 => Some(ObjectKind::Blob),
            4 => Some(ObjectKind::Tag),
            _ => None,
        }
    }
}

/// A decoded object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedObject {
    /// Its kind.
    pub kind: ObjectKind,
    /// Its decompressed content, with any delta already applied.
    pub data: Vec<u8>,
}

/// One pack together with its index.
#[derive(Debug)]
pub struct PackFile {
    pack_path: PathBuf,
    /// Sorted object ids and their offsets into the pack.
    entries: HashMap<[u8; 20], u64>,
}

impl PackFile {
    /// Open a `.pack` file and its sibling `.idx`.
    pub fn open(pack_path: &Path) -> Result<Self, PackError> {
        let idx_path = pack_path.with_extension("idx");
        let idx = fs::read(&idx_path).map_err(|e| PackError::Io {
            path: idx_path.clone(),
            source: e,
        })?;

        let entries = parse_index_v2(&idx)?;

        Ok(PackFile {
            pack_path: pack_path.to_path_buf(),
            entries,
        })
    }

    /// Whether this pack contains an object.
    pub fn contains(&self, oid: &str) -> bool {
        parse_oid(oid).is_some_and(|bytes| self.entries.contains_key(&bytes))
    }

    /// Read and fully resolve an object, following delta chains.
    pub fn read(&self, oid: &str) -> Result<PackedObject, PackError> {
        let key = parse_oid(oid).ok_or_else(|| PackError::ObjectNotFound(oid.to_string()))?;
        let offset = *self
            .entries
            .get(&key)
            .ok_or_else(|| PackError::ObjectNotFound(oid.to_string()))?;
        self.read_at(offset, 0)
    }

    /// Read the object at a byte offset, resolving deltas up to `depth`.
    fn read_at(&self, offset: u64, depth: usize) -> Result<PackedObject, PackError> {
        if depth > MAX_DELTA_DEPTH {
            return Err(PackError::DeltaChainTooDeep(MAX_DELTA_DEPTH));
        }

        let mut file = fs::File::open(&self.pack_path).map_err(|e| PackError::Io {
            path: self.pack_path.clone(),
            source: e,
        })?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| PackError::Io {
                path: self.pack_path.clone(),
                source: e,
            })?;

        // Object header: type (3 bits) + size (4 bits, then 7-bit continuation).
        let first = read_byte(&mut file, &self.pack_path)?;
        let type_code = (first >> 4) & 0x07;
        let mut size = u64::from(first & 0x0f);
        let mut shift = 4;
        let mut byte = first;
        while byte & 0x80 != 0 {
            byte = read_byte(&mut file, &self.pack_path)?;
            size |= u64::from(byte & 0x7f) << shift;
            shift += 7;
        }

        match type_code {
            // OFS_DELTA: the base is given as a negative offset in this pack.
            6 => {
                let mut c = read_byte(&mut file, &self.pack_path)?;
                let mut base_offset = u64::from(c & 0x7f);
                while c & 0x80 != 0 {
                    c = read_byte(&mut file, &self.pack_path)?;
                    base_offset = ((base_offset + 1) << 7) | u64::from(c & 0x7f);
                }
                let base_position = offset.checked_sub(base_offset).ok_or_else(|| {
                    PackError::BadDelta(format!(
                        "OFS_DELTA base offset {base_offset} is beyond the object start"
                    ))
                })?;

                let delta = self.inflate_body(&mut file, size)?;
                let base = self.read_at(base_position, depth + 1)?;
                let data = apply_delta(&base.data, &delta)?;
                Ok(PackedObject {
                    kind: base.kind,
                    data,
                })
            }

            // REF_DELTA: the base is named by object id, possibly in another pack.
            7 => {
                let mut base_oid = [0u8; 20];
                file.read_exact(&mut base_oid).map_err(|e| PackError::Io {
                    path: self.pack_path.clone(),
                    source: e,
                })?;

                let delta = self.inflate_body(&mut file, size)?;

                // The base may live in this pack (common after gc) or elsewhere.
                let base = match self.entries.get(&base_oid) {
                    Some(base_offset) => self.read_at(*base_offset, depth + 1)?,
                    None => {
                        return Err(PackError::BadDelta(format!(
                            "REF_DELTA base {} is not in this pack (thin packs are unsupported)",
                            to_hex(&base_oid)
                        )))
                    }
                };
                let data = apply_delta(&base.data, &delta)?;
                Ok(PackedObject {
                    kind: base.kind,
                    data,
                })
            }

            // Plain objects.
            code => {
                let kind =
                    ObjectKind::from_code(code).ok_or(PackError::UnsupportedObjectType(code))?;
                let data = self.inflate_body(&mut file, size)?;
                Ok(PackedObject { kind, data })
            }
        }
    }

    /// Inflate `size` bytes of zlib data starting at the file's current position.
    fn inflate_body(&self, file: &mut fs::File, size: u64) -> Result<Vec<u8>, PackError> {
        // Read a generous chunk: zlib streams in packs are contiguous, and reading
        // past the end is harmless because the decompressor stops at the stream end.
        let mut buffer = vec![0u8; size as usize + 1024];
        let read = file.read(&mut buffer).map_err(|e| PackError::Io {
            path: self.pack_path.clone(),
            source: e,
        })?;
        buffer.truncate(read);
        zlib_inflate(&buffer).map_err(PackError::Inflate)
    }
}

fn read_byte(file: &mut fs::File, path: &Path) -> Result<u8, PackError> {
    let mut b = [0u8; 1];
    file.read_exact(&mut b).map_err(|e| PackError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(b[0])
}

/// Parse a version-2 pack index.
///
/// Layout: magic \0xff\x74\x4f\x63, version, 256-entry fanout, then sorted object ids,
/// CRCs, 4-byte offsets, 8-byte large offsets, and trailing checksums. We only need
/// the ids and offsets.
fn parse_index_v2(idx: &[u8]) -> Result<HashMap<[u8; 20], u64>, PackError> {
    const MAGIC: [u8; 4] = [0xff, 0x74, 0x4f, 0x63];

    if idx.len() < 8 + 256 * 4 {
        return Err(PackError::BadHeader("index too short".into()));
    }
    if idx[0..4] != MAGIC {
        return Err(PackError::UnsupportedIndexVersion(1));
    }

    let version = u32::from_be_bytes([idx[4], idx[5], idx[6], idx[7]]);
    if version != 2 {
        return Err(PackError::UnsupportedIndexVersion(version));
    }

    // The last fanout entry is the total object count.
    let fanout_start = 8;
    let count = read_u32_be(idx, fanout_start + 255 * 4) as usize;

    let ids_start = fanout_start + 256 * 4;
    let crcs_start = ids_start + count * 20;
    let offsets_start = crcs_start + count * 4;
    let large_start = offsets_start + count * 4;

    if idx.len() < large_start {
        return Err(PackError::BadHeader("index truncated".into()));
    }

    let mut entries = HashMap::with_capacity(count);

    for i in 0..count {
        let mut oid = [0u8; 20];
        oid.copy_from_slice(&idx[ids_start + i * 20..ids_start + i * 20 + 20]);

        let raw = read_u32_be(idx, offsets_start + i * 4);
        let offset = if raw & 0x8000_0000 != 0 {
            // High bit set: the low 31 bits index into the 8-byte offset table.
            let large_index = (raw & 0x7fff_ffff) as usize;
            let at = large_start + large_index * 8;
            if idx.len() < at + 8 {
                return Err(PackError::BadHeader("large offset out of range".into()));
            }
            u64::from_be_bytes([
                idx[at],
                idx[at + 1],
                idx[at + 2],
                idx[at + 3],
                idx[at + 4],
                idx[at + 5],
                idx[at + 6],
                idx[at + 7],
            ])
        } else {
            u64::from(raw)
        };

        entries.insert(oid, offset);
    }

    Ok(entries)
}

fn read_u32_be(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// Parse a 40-character hex object id.
fn parse_oid(hex: &str) -> Option<[u8; 20]> {
    if hex.len() != 40 {
        return None;
    }
    let mut out = [0u8; 20];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(out)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ---------------------------------------------------------------- delta format

/// Apply a Git delta to a base object (format documented in `gitformat-pack`).
///
/// A delta is: source-length varint, target-length varint, then instructions.
/// Each instruction is either a copy (high bit set) or an insert (high bit clear).
pub fn apply_delta(base: &[u8], delta: &[u8]) -> Result<Vec<u8>, PackError> {
    let mut pos = 0usize;

    let source_len = read_varint(delta, &mut pos)?;
    if source_len != base.len() as u64 {
        return Err(PackError::BadDelta(format!(
            "delta expects a {source_len}-byte base but the base is {} bytes",
            base.len()
        )));
    }
    let target_len = read_varint(delta, &mut pos)?;

    let mut out = Vec::with_capacity(target_len as usize);

    while pos < delta.len() {
        let op = delta[pos];
        pos += 1;
        if op & 0x80 != 0 {
            // Copy from base.
            let mut offset = 0usize;
            let mut size = 0usize;
            for i in 0..4 {
                if op & (1 << i) != 0 {
                    let b = *delta
                        .get(pos)
                        .ok_or_else(|| PackError::BadDelta("truncated copy offset".into()))?;
                    pos += 1;
                    offset |= usize::from(b) << (8 * i);
                }
            }
            for i in 0..3 {
                if op & (1 << (4 + i)) != 0 {
                    let b = *delta
                        .get(pos)
                        .ok_or_else(|| PackError::BadDelta("truncated copy size".into()))?;
                    pos += 1;
                    size |= usize::from(b) << (8 * i);
                }
            }
            // A size of zero means 0x10000, per the format.
            if size == 0 {
                size = 0x10000;
            }

            let end = offset
                .checked_add(size)
                .ok_or_else(|| PackError::BadDelta("copy range overflow".into()))?;
            if end > base.len() {
                return Err(PackError::BadDelta(format!(
                    "copy range {offset}..{end} exceeds base length {}",
                    base.len()
                )));
            }
            out.extend_from_slice(&base[offset..end]);
        } else if op != 0 {
            // Insert the next `op` bytes literally.
            let n = usize::from(op);
            let end = pos
                .checked_add(n)
                .ok_or_else(|| PackError::BadDelta("insert range overflow".into()))?;
            if end > delta.len() {
                return Err(PackError::BadDelta("truncated insert payload".into()));
            }
            out.extend_from_slice(&delta[pos..end]);
            pos = end;
        } else {
            // op == 0 is reserved and never valid.
            return Err(PackError::BadDelta("reserved delta opcode 0".into()));
        }
    }

    if out.len() as u64 != target_len {
        return Err(PackError::BadDelta(format!(
            "delta produced {} bytes but declared {target_len}",
            out.len()
        )));
    }

    Ok(out)
}

/// Read a little-endian base-128 varint.
fn read_varint(data: &[u8], pos: &mut usize) -> Result<u64, PackError> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *data
            .get(*pos)
            .ok_or_else(|| PackError::BadDelta("truncated varint".into()))?;
        *pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift > 63 {
            return Err(PackError::BadDelta("varint too long".into()));
        }
    }
}

/// Encode a value as a little-endian base-128 varint.
///
/// Only used when constructing deltas (tests and future fixture builders); the reader
/// itself never needs to encode.
pub fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}

/// Every pack in a repository's object store.
pub fn packs_in(objects_dir: &Path) -> Vec<PathBuf> {
    let pack_dir = objects_dir.join("pack");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(pack_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("pack") {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(v: u64) -> Vec<u8> {
        encode_varint(v)
    }

    #[test]
    fn delta_copy_and_insert_round_trip() {
        // base = "hello world" (11 bytes)
        //
        // Delta instructions are built with a helper so the varint encoding cannot be
        // mistyped. An earlier version of this test pushed the length 12 directly,
        // which is invalid: values >= 128 need a continuation byte, and more
        // importantly the encoded target length must match what the instructions
        // actually produce.
        let base = b"hello world";

        // copy(offset=6, size=5) -> "world"
        let copy_world = {
            let mut v = vec![0x80u8 | 0x01 | 0x10];
            v.push(6); // offset byte 0
            v.push(5); // size byte 0
            v
        };
        // insert "!"
        let insert_bang = vec![1u8, b'!'];
        // copy(offset=0, size=5) -> "hello"
        let copy_hello = {
            let mut v = vec![0x80u8 | 0x01 | 0x10];
            v.push(0);
            v.push(5);
            v
        };

        let target: &[u8] = b"world!hello";
        let mut delta = Vec::new();
        delta.extend_from_slice(&varint(base.len() as u64));
        delta.extend_from_slice(&varint(target.len() as u64));
        delta.extend_from_slice(&copy_world);
        delta.extend_from_slice(&insert_bang);
        delta.extend_from_slice(&copy_hello);

        let out = apply_delta(base, &delta).expect("delta applies");
        assert_eq!(out, target);
    }

    #[test]
    fn delta_rejects_a_base_length_mismatch() {
        // Guards against applying a delta to the wrong object, which would corrupt
        // data silently.
        let delta = vec![99u8, 3, 1, b'x'];
        let err = apply_delta(b"abc", &delta).unwrap_err();
        assert!(matches!(err, PackError::BadDelta(_)));
    }

    #[test]
    fn delta_rejects_a_copy_beyond_the_base() {
        let base = b"abc";
        let mut delta = vec![3u8, 5];
        delta.push(0x80 | 0x01 | 0x10); // copy offset 0
        delta.push(0);
        delta.push(9); // size 9 > base length
        assert!(apply_delta(base, &delta).is_err());
    }

    #[test]
    fn delta_rejects_opcode_zero() {
        let mut delta = vec![3u8, 1];
        delta.push(0);
        assert!(apply_delta(b"abc", &delta).is_err());
    }

    #[test]
    fn varints_decode_across_multiple_bytes() {
        // 300 = 0b1_0010_1100 -> little-endian base-128: 0xAC 0x02
        let data = [0xAC, 0x02];
        let mut pos = 0;
        assert_eq!(read_varint(&data, &mut pos).unwrap(), 300);
        assert_eq!(pos, 2);
    }

    #[test]
    fn truncated_varint_is_an_error_not_a_panic() {
        let data = [0x80];
        let mut pos = 0;
        assert!(read_varint(&data, &mut pos).is_err());
    }

    #[test]
    fn object_ids_parse_only_at_the_right_length() {
        assert!(parse_oid(&"a".repeat(40)).is_some());
        assert!(parse_oid(&"a".repeat(39)).is_none());
        assert!(parse_oid("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
    }

    #[test]
    fn hex_round_trips() {
        let bytes = [0xde, 0xad, 0xbe, 0xef];
        assert_eq!(to_hex(&bytes), "deadbeef");
        let hex = format!("{}{}", "deadbeef", "0".repeat(32));
        let parsed = parse_oid(&hex).expect("valid id");
        assert_eq!(&parsed[0..4], &bytes[..], "the first four bytes round-trip");
        assert_eq!(to_hex(&parsed), hex, "and the whole id round-trips");
    }

    #[test]
    fn opening_a_missing_index_reports_io_not_a_panic() {
        let path = std::env::temp_dir().join("devch-nonexistent.pack");
        assert!(matches!(PackFile::open(&path), Err(PackError::Io { .. })));
    }

    #[test]
    fn a_v1_index_is_reported_as_unsupported() {
        // Real .idx v1 files start with a fanout table, not the magic bytes.
        let bogus = vec![0u8; 8 + 256 * 4];
        let err = parse_index_v2(&bogus).unwrap_err();
        assert!(matches!(err, PackError::UnsupportedIndexVersion(1)));
    }
}
