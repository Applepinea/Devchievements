//! Phase 0 spike, approach C for ADR-0002: read the Git repository directly.
//!
//! WHY THIS EXISTS
//! ---------------
//! The probe measured one 'git' subprocess at ~33 ms on this host, while a trivial
//! native binary costs ~9 ms. A hook that needs ~17 facts therefore costs ~600 ms
//! by shelling out, which busts both budgets (p95 < 50 ms, hard timeout 200 ms).
//!
//! Approach B (link gix) could not be built here: the GNU toolchain's dlltool
//! fails with "CreateProcess" inside a build script under this sandbox.
//! Approach C (parse .git yourself) needs no external crate at all, so it is both
//! the fastest and the only one that compiles unconditionally in this environment.
//!
//! WHAT IT READS
//! -------------
//! - .git discovery, including the "gitdir: <path>" indirection used by worktrees
//!   and submodules,
//! - HEAD -> ref -> commit oid (loose refs and packed-refs),
//! - the commit object from the loose object store, zlib-decompressed by hand,
//!   then header-parsed for tree/parents/author/committer/message.
//!
//! LIMITATIONS (stated, not hidden)
//! --------------------------------
//! - packfiles (delta chains) are not resolved; freshly committed objects are
//!   loose, which is exactly the Hook case, but gc/repack would break it;
//! - SHA-256 repositories are not handled;
//! - dynamic-Huffman deflate blocks are not handled (fixed/stored only).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let t0 = Instant::now();

    let git_dir = match find_git_dir() {
        Some(p) => p,
        None => {
            println!("{{\"ok\":false,\"error\":\"not a repository\"}}");
            return;
        }
    };
    let t_gitdir = t0.elapsed();

    let head_oid = match resolve_head(&git_dir) {
        Ok(o) => o,
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"head: {e}\"}}");
            return;
        }
    };
    let t_head = t0.elapsed();

    match read_commit(&git_dir, &head_oid) {
        Ok(c) => {
            let t_commit = t0.elapsed();
            println!(
                concat!(
                    "{{\"ok\":true,\"approach\":\"direct-file-read\",\"sha\":\"{}\",",
                    "\"tree\":\"{}\",\"parents\":{},\"author_name\":\"{}\",\"author_email\":\"{}\",",
                    "\"author_ts\":{},\"committer_name\":\"{}\",\"committer_email\":\"{}\",\"committer_ts\":{},",
                    "\"summary\":\"{}\",",
                    "\"git_dir_micros\":{},\"head_micros\":{},\"commit_micros\":{},\"total_micros\":{}}}"
                ),
                head_oid,
                c.tree,
                c.parents.len(),
                json_escape(&c.author_name),
                json_escape(&c.author_email),
                c.author_ts,
                json_escape(&c.committer_name),
                json_escape(&c.committer_email),
                c.committer_ts,
                json_escape(&c.summary),
                t_gitdir.as_micros(),
                t_head.as_micros(),
                t_commit.as_micros(),
                t0.elapsed().as_micros()
            );
        }
        Err(e) => println!("{{\"ok\":false,\"error\":\"commit: {e}\"}}"),
    }
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Locate the .git directory, following the "gitdir: <path>" file used by
/// worktrees and submodules.
fn find_git_dir() -> Option<PathBuf> {
    let mut cur = std::env::current_dir().ok()?;
    loop {
        let dot_git = cur.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            if let Ok(text) = fs::read_to_string(&dot_git) {
                if let Some(rest) = text.trim().strip_prefix("gitdir:") {
                    let p = PathBuf::from(rest.trim());
                    let p = if p.is_absolute() { p } else { cur.join(p) };
                    return Some(p);
                }
            }
        }
        cur = cur.parent()?.to_path_buf();
    }
}

/// Resolve HEAD to a commit oid, honouring symbolic refs, packed-refs and the
/// commondir indirection inside worktrees.
fn resolve_head(git_dir: &Path) -> Result<String, String> {
    let head = fs::read_to_string(git_dir.join("HEAD")).map_err(|e| format!("HEAD: {e}"))?;
    let head = head.trim();

    if let Some(refname) = head.strip_prefix("ref:") {
        let refname = refname.trim();
        let roots = ref_roots(git_dir);

        for root in &roots {
            if let Ok(s) = fs::read_to_string(root.join(refname)) {
                return Ok(s.trim().to_string());
            }
        }
        for root in &roots {
            if let Ok(text) = fs::read_to_string(root.join("packed-refs")) {
                for line in text.lines() {
                    if line.starts_with('#') || line.starts_with('^') || line.trim().is_empty() {
                        continue;
                    }
                    let mut it = line.splitn(2, ' ');
                    let oid = it.next().unwrap_or("");
                    let name = it.next().unwrap_or("").trim();
                    if name == refname {
                        return Ok(oid.to_string());
                    }
                }
            }
        }
        Err(format!("ref not found: {refname}"))
    } else {
        Ok(head.to_string())
    }
}

/// Candidate roots that may contain refs/ and packed-refs.
fn ref_roots(git_dir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![git_dir.to_path_buf()];
    if let Ok(cd) = fs::read_to_string(git_dir.join("commondir")) {
        let p = PathBuf::from(cd.trim());
        let p = if p.is_absolute() { p } else { git_dir.join(p) };
        if let Ok(canon) = p.canonicalize() {
            if !roots.iter().any(|r| r == &canon) {
                roots.push(canon);
            }
        }
    }
    roots
}

struct CommitFacts {
    tree: String,
    parents: Vec<String>,
    author_name: String,
    author_email: String,
    author_ts: i64,
    committer_name: String,
    committer_email: String,
    committer_ts: i64,
    summary: String,
}

/// Read and parse a commit object from the loose object store.
fn read_commit(git_dir: &Path, oid: &str) -> Result<CommitFacts, String> {
    if oid.len() < 3 {
        return Err("oid too short".into());
    }
    let (dir_part, file_part) = oid.split_at(2);
    let obj_path = git_dir.join("objects").join(dir_part).join(file_part);
    let raw = fs::read(&obj_path).map_err(|e| {
        format!("loose object ({obj_path:?}): {e} -- packed objects are not handled by this spike")
    })?;

    let inflated = zlib_inflate(&raw)?;
    let text = String::from_utf8_lossy(&inflated);

    let mut tree = String::new();
    let mut parents = Vec::new();
    let mut author_name = String::new();
    let mut author_email = String::new();
    let mut author_ts = 0i64;
    let mut committer_name = String::new();
    let mut committer_email = String::new();
    let mut committer_ts = 0i64;
    let mut summary = String::new();
    let mut in_message = false;

    // The object is "<type> <len>\0<content>"; drop the header first.
    let body = match text.find('\0') {
        Some(i) => &text[i + 1..],
        None => return Err("malformed object header".into()),
    };

    for line in body.split('\n') {
        if in_message {
            if summary.is_empty() && !line.trim().is_empty() {
                summary = line.to_string();
            }
            continue;
        }
        if line.is_empty() {
            in_message = true;
            continue;
        }
        if let Some(v) = line.strip_prefix("tree ") {
            tree = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("parent ") {
            parents.push(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("author ") {
            let (n, e, ts) = parse_ident(v);
            author_name = n;
            author_email = e;
            author_ts = ts;
        } else if let Some(v) = line.strip_prefix("committer ") {
            let (n, e, ts) = parse_ident(v);
            committer_name = n;
            committer_email = e;
            committer_ts = ts;
        }
    }

    Ok(CommitFacts {
        tree,
        parents,
        author_name,
        author_email,
        author_ts,
        committer_name,
        committer_email,
        committer_ts,
        summary,
    })
}

/// Parse "Name <email> 1700000000 +0000".
fn parse_ident(s: &str) -> (String, String, i64) {
    let (name, rest) = match s.split_once(" <") {
        Some((n, r)) => (n.to_string(), r),
        None => (s.to_string(), ""),
    };
    let (email, tail) = match rest.split_once("> ") {
        Some((e, t)) => (e.to_string(), t),
        None => (rest.trim_end_matches('>').to_string(), ""),
    };
    let ts = tail
        .split_whitespace()
        .next()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    (name, email, ts)
}

/// zlib (RFC1950) inflate with stored, fixed-Huffman and dynamic-Huffman blocks.
///
/// Git writes loose objects with ordinary zlib, which in practice means dynamic
/// Huffman blocks. An earlier version of this spike only handled fixed blocks and
/// silently returned an error on real objects; the benchmark's correctness check
/// caught it. That is why approach C must be validated against git's own output
/// and not just timed.
fn zlib_inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 2 {
        return Err("too short".into());
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0f != 8 {
        return Err("not deflate".into());
    }
    if (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 {
        return Err("bad zlib header".into());
    }
    let mut pos = 2usize;
    if flg & 0x20 != 0 {
        pos += 4; // preset dictionary
    }
    inflate_deflate(data, pos)
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    acc: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        BitReader { data, pos, bit: 0, acc: 0 }
    }
    fn need(&mut self) -> Result<(), String> {
        while self.bit < 24 {
            if self.pos >= self.data.len() {
                return Err("unexpected end of deflate stream".into());
            }
            self.acc |= u32::from(self.data[self.pos]) << self.bit;
            self.pos += 1;
            self.bit += 8;
        }
        Ok(())
    }
    fn bits(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.need()?;
        let v = self.acc & ((1u32 << n) - 1);
        self.acc >>= n;
        self.bit -= n;
        Ok(v)
    }
    fn align(&mut self) {
        let drop = self.bit % 8;
        self.acc >>= drop;
        self.bit -= drop;
    }
}

/// A Huffman decoding table: for every symbol, its canonical code and length.
#[derive(Clone)]
struct Huffman {
    codes: Vec<u16>,
    lens: Vec<u8>,
    max_len: u8,
}

impl Huffman {
    /// Build canonical codes from code lengths (RFC1951 3.2.2).
    fn from_lengths(lens: &[u8]) -> Result<Self, String> {
        let max_len = *lens.iter().max().unwrap_or(&0);
        if max_len == 0 {
            return Err("empty huffman table".into());
        }
        let mut bl_count = [0u32; 16];
        for &l in lens {
            if l > 15 {
                return Err("code length too large".into());
            }
            bl_count[l as usize] += 1;
        }
        bl_count[0] = 0;

        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for bits in 1..=15usize {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        // The decoder below consumes one bit at a time and shifts it in from the
        // right, which reconstructs the code MSB-first. That is exactly how
        // DEFLATE packs Huffman codes, so we store the canonical code UNREVERSED.
        // (An earlier version bit-reversed them, which breaks every multi-bit
        // code -- the correctness check caught it.)
        let mut codes = vec![0u16; lens.len()];
        for (sym, &l) in lens.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let c = next_code[l as usize];
            next_code[l as usize] += 1;
            codes[sym] = c as u16;
        }
        Ok(Huffman { codes, lens: lens.to_vec(), max_len })
    }

    /// Decode one symbol, reading bits LSB-first as DEFLATE requires.
    fn decode(&self, br: &mut BitReader<'_>) -> Result<u16, String> {
        let mut code = 0u16;
        for len in 1..=self.max_len {
            code = (code << 1) | br.bits(1)? as u16;
            for (sym, (&c, &l)) in self.codes.iter().zip(self.lens.iter()).enumerate() {
                if l == len && c == code {
                    return Ok(sym as u16);
                }
            }
        }
        Err("invalid huffman code".into())
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn inflate_deflate(data: &[u8], pos: usize) -> Result<Vec<u8>, String> {
    let mut br = BitReader::new(data, pos);
    let mut out: Vec<u8> = Vec::new();

    loop {
        let last = br.bits(1)?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.align();
                let len = br.bits(16)? as usize;
                let _nlen = br.bits(16)?;
                for _ in 0..len {
                    out.push(br.bits(8)? as u8);
                }
            }
            1 => {
                let (lit, dist) = fixed_tables()?;
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut br)?;
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            _ => return Err("bad block type".into()),
        }
        if last == 1 {
            break;
        }
    }
    Ok(out)
}

fn inflate_block(
    br: &mut BitReader<'_>,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    loop {
        let sym = lit.decode(br)?;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let idx = (sym - 257) as usize;
            if idx >= LEN_BASE.len() {
                return Err("bad length symbol".into());
            }
            let len = LEN_BASE[idx] as usize + br.bits(LEN_EXTRA[idx] as u32)? as usize;
            let dsym = dist.decode(br)? as usize;
            if dsym >= DIST_BASE.len() {
                return Err("bad distance symbol".into());
            }
            let distance = DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
            if distance > out.len() {
                return Err("distance beyond output".into());
            }
            let start = out.len() - distance;
            for i in 0..len {
                let b = out[start + i];
                out.push(b);
            }
        }
    }
}

fn fixed_tables() -> Result<(Huffman, Huffman), String> {
    let mut lit_lens = vec![0u8; 288];
    for (i, l) in lit_lens.iter_mut().enumerate() {
        *l = if i < 144 {
            8
        } else if i < 256 {
            9
        } else if i < 280 {
            7
        } else {
            8
        };
    }
    let dist_lens = vec![5u8; 30];
    Ok((Huffman::from_lengths(&lit_lens)?, Huffman::from_lengths(&dist_lens)?))
}

/// Read a dynamic block's Huffman tables (RFC1951 3.2.7).
fn dynamic_tables(br: &mut BitReader<'_>) -> Result<(Huffman, Huffman), String> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;

    // The code-length alphabet is stored in this scrambled order.
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let mut cl_lens = vec![0u8; 19];
    for i in 0..hclen {
        cl_lens[ORDER[i]] = br.bits(3)? as u8;
    }
    let cl_huff = Huffman::from_lengths(&cl_lens)?;

    // Decode the literal/length and distance code lengths.
    let mut lens: Vec<u8> = Vec::with_capacity(hlit + hdist);
    while lens.len() < hlit + hdist {
        let sym = cl_huff.decode(br)?;
        match sym {
            0..=15 => lens.push(sym as u8),
            16 => {
                let prev = *lens.last().ok_or("repeat with no previous length")?;
                let repeat = 3 + br.bits(2)? as usize;
                for _ in 0..repeat {
                    lens.push(prev);
                }
            }
            17 => {
                let repeat = 3 + br.bits(3)? as usize;
                for _ in 0..repeat {
                    lens.push(0);
                }
            }
            18 => {
                let repeat = 11 + br.bits(7)? as usize;
                for _ in 0..repeat {
                    lens.push(0);
                }
            }
            _ => return Err("bad code-length symbol".into()),
        }
    }
    if lens.len() > hlit + hdist {
        return Err("code lengths overran".into());
    }

    let lit_lens = &lens[..hlit];
    let dist_lens = &lens[hlit..hlit + hdist];

    // A distance table with no codes is legal; substitute a single dummy code so
    // construction cannot fail when the block only contains literals.
    let lit = Huffman::from_lengths(lit_lens)?;
    let dist = if dist_lens.iter().any(|&l| l > 0) {
        Huffman::from_lengths(dist_lens)?
    } else {
        Huffman::from_lengths(&[1u8])?
    };
    Ok((lit, dist))
}

