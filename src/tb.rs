//! Syzygy reader for the 3–5 piece tables behind `SyzygyPath`.
//!
//! WDL (`.rtbw`) is used in the search when the halfmove clock is 0, which is
//! when that table is exact. DTZ (`.rtbz`) ranks root moves so a win is
//! shortened and a loss is delayed. Cursed wins and blessed losses follow the
//! 50-move rule. A root mate still wins on the 100th half-move.
//!
//! Indexing, Re-Pair symbols and canonical Huffman blocks follow Ronald de
//! Man's published file format. The reader is original.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::board::{Color, EMPTY, Move, PAWN, Position, color_of, type_of};

include!("tb_tables.rs");

pub const DEFAULT_PATH: &str = "/home/librechat/syzygy/3-4-5";
/// Below mate, above any static evaluation.
pub const TB_SCORE: i32 = 20_000;
const MAX_MEN: usize = 5;
const MAX_DTZ_DEPTH: i32 = 64;

const WDL_MAGIC: [u8; 4] = [0x71, 0xe8, 0x23, 0x5d];
const DTZ_MAGIC: [u8; 4] = [0xd7, 0x66, 0x0c, 0xa5];
const WDL_TO_MAP: [usize; 5] = [1, 3, 0, 2, 0];
const PA_FLAGS: [u8; 5] = [8, 0, 0, 0, 4];
const WDL_TO_DTZ: [i32; 5] = [-1, -101, 0, 101, 1];
const PIVOT: [i64; 3] = [31_332, 28_056, 462];
const PIECE_CHAR: [char; 6] = ['K', 'Q', 'R', 'B', 'N', 'P'];

pub fn score_of_wdl(wdl: i32, ply: i32) -> i32 {
    match wdl {
        2 => TB_SCORE - ply,
        1 => 1,
        0 => 0,
        -1 => -1,
        -2 => -(TB_SCORE - ply),
        _ => 0,
    }
}

fn wdl_of_dtz(dtz: i32, halfmove: i32) -> i32 {
    if dtz > 100 {
        1
    } else if dtz > 0 {
        if dtz + halfmove <= 100 { 2 } else { 0 }
    } else if dtz < -100 {
        -1
    } else if dtz < 0 {
        if -dtz + halfmove <= 100 { -2 } else { 0 }
    } else {
        0
    }
}

fn dtz_before_zeroing(wdl: i32) -> i32 {
    let sign = (wdl > 0) as i32 - (wdl < 0) as i32;
    let unit = if wdl.abs() == 2 { 1 } else { 101 };
    sign * unit
}

struct Geom {
    pawn_idx: [[i64; 24]; 5],
    pfactor: [[i64; 4]; 5],
    #[allow(dead_code)]
    mult_idx: [[i64; 10]; 5],
    #[allow(dead_code)]
    mfactor: [i64; 5],
}

fn geom() -> &'static Geom {
    static G: OnceLock<Geom> = OnceLock::new();
    G.get_or_init(|| {
        let mut pawn_idx = [[0i64; 24]; 5];
        let mut pfactor = [[0i64; 4]; 5];
        for i in 0..5 {
            let mut j = 0;
            for f in 0..4 {
                let mut s = 0i64;
                let end = (f + 1) * 6;
                while j < end {
                    pawn_idx[i][j] = s;
                    s += if i == 0 {
                        1
                    } else {
                        binom(PTWIST[INV_FLAP[j] as usize] as i32, i as i32)
                    };
                    j += 1;
                }
                pfactor[i][f] = s;
            }
        }
        let mut mult_idx = [[0i64; 10]; 5];
        let mut mfactor = [0i64; 5];
        for i in 0..5 {
            let mut s = 0i64;
            for j in 0..10 {
                mult_idx[i][j] = s;
                s += if i == 0 {
                    1
                } else {
                    binom(MTWIST[INV_TRIANGLE[j] as usize] as i32, i as i32)
                };
            }
            mfactor[i] = s;
        }
        Geom {
            pawn_idx,
            pfactor,
            mult_idx,
            mfactor,
        }
    })
}

fn binom(n: i32, k: i32) -> i64 {
    if n < 0 || k < 0 || k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut r = 1i64;
    for i in 0..k {
        r = r * (n - i) as i64 / (i + 1) as i64;
    }
    r
}

fn subfactor(k: i32, n: i32) -> i64 {
    if k <= 0 {
        return 0;
    }
    let mut f = n as i64;
    let mut l = 1i64;
    for i in 1..k {
        f *= n as i64 - i as i64;
        l *= i as i64 + 1;
    }
    if l == 0 { 0 } else { f / l }
}

fn div_floor(a: i64, b: i64) -> i64 {
    let mut q = a / b;
    if a % b != 0 && a < 0 {
        q -= 1;
    }
    q
}

fn offdiag(sq: u8) -> i32 {
    (sq >> 3) as i32 - (sq & 7) as i32
}

fn flipdiag(sq: u8) -> u8 {
    ((sq >> 3) | (sq << 3)) & 63
}

fn men(pos: &Position) -> usize {
    pos.squares.iter().filter(|&&p| p != EMPTY).count()
}

fn piece_ord(c: char) -> usize {
    PIECE_CHAR.iter().position(|&p| p == c).unwrap_or(9)
}

fn sort_piece_chars(s: &str) -> Vec<char> {
    let mut c: Vec<char> = s.chars().collect();
    c.sort_by_key(|ch| piece_ord(*ch));
    c
}

fn normalize_name(name: &str, mirror: bool) -> String {
    let Some((w0, b0)) = name.split_once('v') else {
        return name.to_string();
    };
    let w = sort_piece_chars(w0);
    let b = sort_piece_chars(b0);
    let left = (w.len(), b.iter().map(|c| piece_ord(*c)).collect::<Vec<_>>());
    let right = (b.len(), w.iter().map(|c| piece_ord(*c)).collect::<Vec<_>>());
    let swap = mirror ^ (left < right);
    let ws: String = w.into_iter().collect();
    let bs: String = b.into_iter().collect();
    if swap {
        format!("{bs}v{ws}")
    } else {
        format!("{ws}v{bs}")
    }
}

fn material_key(pos: &Position) -> String {
    let mut count = [[0u8; 7]; 2];
    for &p in &pos.squares {
        if p == EMPTY {
            continue;
        }
        let t = type_of(p) as usize;
        if (1..7).contains(&t) {
            count[color_of(p).idx()][t] += 1;
        }
    }
    let mut s = String::new();
    for c in 0..2 {
        if c == 1 {
            s.push('v');
        }
        for (i, ch) in PIECE_CHAR.iter().enumerate() {
            let pt = 6 - i;
            for _ in 0..count[c][pt] {
                s.push(*ch);
            }
        }
    }
    s
}

fn recalc_key(pieces: &[u8], mirror: bool) -> String {
    let wx = if mirror { 8 } else { 0 };
    let bx = if mirror { 0 } else { 8 };
    let mut s = String::new();
    for (xor, sep) in [(wx, false), (bx, true)] {
        if sep {
            s.push('v');
        }
        for (i, ch) in PIECE_CHAR.iter().enumerate() {
            let code = (6 - i as u8) ^ xor;
            for &p in pieces {
                if p == code {
                    s.push(*ch);
                }
            }
        }
    }
    s
}

fn enc_type_of(stem: &str) -> i32 {
    let Some((a, b)) = stem.split_once('v') else {
        return -1;
    };
    let mut j = 0;
    for ch in PIECE_CHAR {
        if b.chars().filter(|&c| c == ch).count() == 1 {
            j += 1;
        }
        if a.chars().filter(|&c| c == ch).count() == 1 {
            j += 1;
        }
    }
    if j >= 3 {
        0
    } else if j == 2 {
        2
    } else {
        -1
    }
}

fn pawn_counts(stem: &str) -> [usize; 2] {
    let Some((black_part, white_part)) = stem.split_once('v') else {
        return [0, 0];
    };
    let mut pawns = [
        white_part.chars().filter(|&c| c == 'P').count(),
        black_part.chars().filter(|&c| c == 'P').count(),
    ];
    if pawns[1] > 0 && (pawns[0] == 0 || pawns[1] < pawns[0]) {
        pawns.swap(0, 1);
    }
    pawns
}

fn read_file(path: &Path) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    if buf.len() < 16 || buf.len() % 64 != 16 {
        return None;
    }
    Some(buf)
}

fn read_prefix(path: &Path, n: usize) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    let mut buf = vec![0u8; n];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn ru16(b: &[u8], i: usize) -> Option<u16> {
    let s = b.get(i..i + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn ru32(b: &[u8], i: usize) -> Option<u32> {
    let s = b.get(i..i + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn ru32be(b: &[u8], i: usize) -> Option<u32> {
    let s = b.get(i..i + 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn ru64be(b: &[u8], i: usize) -> Option<u64> {
    let s = b.get(i..i + 8)?;
    Some(u64::from_be_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

fn align2(p: usize) -> usize {
    p + (p & 1)
}

fn align64(p: usize) -> usize {
    (p + 63) & !63
}

struct Pairs {
    constant: bool,
    value: i32,
    blocksize: u32,
    idxbits: u32,
    min_len: i32,
    base: Vec<u128>,
    huff: isize,
    symlen: Vec<i32>,
    sympat: usize,
    indextable: usize,
    sizetable: usize,
    data: usize,
    n_index: i64,
    n_blocks: i64,
    real_blocks: i64,
    tb_size: i64,
}

impl Pairs {
    fn constant(value: i32) -> Self {
        Self {
            constant: true,
            value,
            blocksize: 0,
            idxbits: 0,
            min_len: 0,
            base: Vec::new(),
            huff: 0,
            symlen: Vec::new(),
            sympat: 0,
            indextable: 0,
            sizetable: 0,
            data: 0,
            n_index: 0,
            n_blocks: 0,
            real_blocks: 0,
            tb_size: 0,
        }
    }

    fn decompress(&self, bytes: &[u8], idx: i64, dtz: bool) -> Option<i32> {
        if self.constant {
            return Some(self.value);
        }
        if idx < 0 || idx >= self.tb_size || self.idxbits == 0 || self.idxbits >= 31 {
            return None;
        }
        let idxbits = self.idxbits;
        let mainidx = idx >> idxbits;
        if mainidx < 0 || mainidx >= self.n_index {
            return None;
        }
        let mut litidx = (idx & ((1i64 << idxbits) - 1)) - (1i64 << (idxbits - 1));
        let at = self.indextable + mainidx as usize * 6;
        let mut block = ru32(bytes, at)? as i64;
        litidx += ru16(bytes, at + 4)? as i64;

        let mut guard = 0;
        if litidx < 0 {
            while litidx < 0 {
                guard += 1;
                if guard > 1_000_000 {
                    return None;
                }
                block -= 1;
                if block < 0 || block >= self.n_blocks {
                    return None;
                }
                litidx += ru16(bytes, self.sizetable + block as usize * 2)? as i64 + 1;
            }
        } else {
            loop {
                if block < 0 || block >= self.n_blocks {
                    return None;
                }
                let span = ru16(bytes, self.sizetable + block as usize * 2)? as i64;
                if litidx <= span {
                    break;
                }
                guard += 1;
                if guard > 1_000_000 {
                    return None;
                }
                litidx -= span + 1;
                block += 1;
            }
        }
        if block < 0 || block >= self.real_blocks {
            return None;
        }

        let mut ptr = self.data + ((block as usize) << self.blocksize);
        let mut code = ru64be(bytes, ptr)? as u128;
        ptr += 8;
        let mut bitcnt = 0i32;
        let m = self.min_len;
        // A block holds at most 65536 values, and a symbol may encode only one.
        for _ in 0..70_000 {
            let mut l = m;
            while (l - m) >= 0
                && (l - m) as usize + 1 < self.base.len()
                && code < self.base[(l - m) as usize]
            {
                l += 1;
            }
            let bi = l - m;
            if bi < 0 || bi as usize >= self.base.len() {
                return None;
            }
            let sym_at = self.huff + l as isize * 2;
            if sym_at < 0 {
                return None;
            }
            let mut sym = ru16(bytes, sym_at as usize)? as u128;
            let base = self.base[bi as usize];
            if code < base || l <= 0 || l >= 64 {
                return None;
            }
            sym += (code - base) >> (64 - l as u32);
            let sym_i = sym as usize;
            if sym_i >= self.symlen.len() {
                return None;
            }
            if litidx < self.symlen[sym_i] as i64 + 1 {
                return expand_symbol(bytes, self, sym_i, litidx, dtz);
            }
            litidx -= self.symlen[sym_i] as i64 + 1;
            code <<= l as u32;
            bitcnt += l;
            if bitcnt >= 32 {
                bitcnt -= 32;
                let next = ru32be(bytes, ptr)? as u128;
                ptr += 4;
                code |= next << bitcnt;
            }
            code &= 0xffff_ffff_ffff_ffff;
        }
        None
    }
}

fn expand_symbol(
    bytes: &[u8],
    pairs: &Pairs,
    mut sym: usize,
    mut litidx: i64,
    dtz: bool,
) -> Option<i32> {
    for _ in 0..300 {
        if pairs.symlen.get(sym).copied().unwrap_or(1) == 0 {
            let w = pairs.sympat + 3 * sym;
            if dtz {
                let b0 = *bytes.get(w)? as i32;
                let b1 = *bytes.get(w + 1)? as i32;
                return Some(((b1 & 0x0f) << 8) | b0);
            }
            return Some(*bytes.get(w)? as i32);
        }
        let w = pairs.sympat + 3 * sym;
        let b0 = *bytes.get(w)? as usize;
        let b1 = *bytes.get(w + 1)? as usize;
        let b2 = *bytes.get(w + 2)? as usize;
        let s1 = ((b1 & 0xf) << 8) | b0;
        if s1 >= pairs.symlen.len() {
            return None;
        }
        if litidx < pairs.symlen[s1] as i64 + 1 {
            sym = s1;
        } else {
            litidx -= pairs.symlen[s1] as i64 + 1;
            let s2 = (b2 << 4) | (b1 >> 4);
            if s2 == 0x0fff || s2 >= pairs.symlen.len() {
                return None;
            }
            sym = s2;
        }
    }
    None
}

fn calc_symlen(
    bytes: &[u8],
    sympat: usize,
    symlen: &mut [i32],
    s: usize,
    seen: &mut [u8],
    depth: i32,
) -> bool {
    if s >= symlen.len() || depth > 300 {
        return false;
    }
    if seen[s] != 0 {
        return true;
    }
    let w = sympat + 3 * s;
    if w + 2 >= bytes.len() {
        return false;
    }
    let s2 = ((bytes[w + 2] as usize) << 4) | (bytes[w + 1] as usize >> 4);
    if s2 == 0x0fff {
        symlen[s] = 0;
    } else {
        let s1 = (((bytes[w + 1] & 0xf) as usize) << 8) | bytes[w] as usize;
        if !calc_symlen(bytes, sympat, symlen, s1, seen, depth + 1) {
            return false;
        }
        if !calc_symlen(bytes, sympat, symlen, s2, seen, depth + 1) {
            return false;
        }
        symlen[s] = symlen[s1] + symlen[s2] + 1;
    }
    seen[s] = 1;
    true
}

#[derive(Clone, Copy)]
struct Sizes {
    index: i64,
    size: i64,
    data: i64,
}

fn setup_pairs(bytes: &[u8], ptr: usize, tb_size: i64, wdl: bool) -> Option<(Pairs, usize, Sizes)> {
    let flags = *bytes.get(ptr)?;
    if flags & 0x80 != 0 {
        let value = if wdl { *bytes.get(ptr + 1)? as i32 } else { 0 };
        return Some((
            Pairs::constant(value),
            ptr + 2,
            Sizes {
                index: 0,
                size: 0,
                data: 0,
            },
        ));
    }
    let blocksize = *bytes.get(ptr + 1)? as u32;
    let idxbits = *bytes.get(ptr + 2)? as u32;
    if blocksize > 16 || idxbits == 0 || idxbits >= 31 {
        return None;
    }
    let real_num_blocks = ru32(bytes, ptr + 4)? as i64;
    let num_blocks = real_num_blocks + *bytes.get(ptr + 3)? as i64;
    let max_len = *bytes.get(ptr + 8)? as i32;
    let min_len = *bytes.get(ptr + 9)? as i32;
    if min_len <= 0 || max_len < min_len || max_len > 32 {
        return None;
    }
    let h = (max_len - min_len + 1) as usize;
    let num_syms = ru16(bytes, ptr + 10 + 2 * h)? as usize;
    if num_syms == 0 || num_syms > 4096 {
        return None;
    }
    let huff_off = ptr + 10;
    let sympat = ptr + 12 + 2 * h;
    let next = sympat + 3 * num_syms + (num_syms & 1);
    if next > bytes.len() {
        return None;
    }
    let mut symlen = vec![0i32; num_syms];
    let mut seen = vec![0u8; num_syms];
    for i in 0..num_syms {
        if !calc_symlen(bytes, sympat, &mut symlen, i, &mut seen, 0) {
            return None;
        }
    }
    let mut base_i = vec![0i64; h];
    if h > 1 {
        for i in (0..h - 1).rev() {
            let a = ru16(bytes, huff_off + i * 2)? as i64;
            let b = ru16(bytes, huff_off + (i + 1) * 2)? as i64;
            base_i[i] = div_floor(base_i[i + 1] + a - b, 2);
        }
    }
    let mut base = vec![0u128; h];
    for i in 0..h {
        let shift = 64 - (min_len + i as i32);
        if base_i[i] > 0 && (0..64).contains(&shift) {
            base[i] = (base_i[i] as u128) << shift;
        }
    }
    let num_indices = (tb_size + (1i64 << idxbits) - 1) >> idxbits;
    let pairs = Pairs {
        constant: false,
        value: 0,
        blocksize,
        idxbits,
        min_len,
        base,
        huff: huff_off as isize - 2 * min_len as isize,
        symlen,
        sympat,
        indextable: 0,
        sizetable: 0,
        data: 0,
        n_index: num_indices,
        n_blocks: num_blocks,
        real_blocks: real_num_blocks,
        tb_size,
    };
    Some((
        pairs,
        next,
        Sizes {
            index: 6 * num_indices,
            size: 2 * num_blocks,
            data: (1i64 << blocksize) * real_num_blocks,
        },
    ))
}

fn norm_piece(enc: i32, pieces: &[u8]) -> Option<Vec<usize>> {
    let n = pieces.len();
    if n == 0 {
        return None;
    }
    let mut norm = vec![0usize; n];
    norm[0] = if enc == 0 { 3 } else { 2 };
    if norm[0] > n {
        return None;
    }
    let mut i = norm[0];
    while i < n {
        let mut j = i;
        while j < n && pieces[j] == pieces[i] {
            norm[i] += 1;
            j += 1;
        }
        if norm[i] == 0 {
            return None;
        }
        i += norm[i];
    }
    Some(norm)
}

fn norm_pawn(pawns: [usize; 2], pieces: &[u8]) -> Option<Vec<usize>> {
    let n = pieces.len();
    if pawns[0] == 0 || pawns[0] > n {
        return None;
    }
    let mut norm = vec![0usize; n];
    norm[0] = pawns[0];
    if pawns[1] > 0 {
        let at = pawns[0];
        if at >= n {
            return None;
        }
        norm[at] = pawns[1];
    }
    let mut i = pawns[0] + pawns[1];
    while i < n {
        let mut j = i;
        while j < n && pieces[j] == pieces[i] {
            norm[i] += 1;
            j += 1;
        }
        if norm[i] == 0 {
            return None;
        }
        i += norm[i];
    }
    Some(norm)
}

fn factors_piece(enc: i32, num: usize, order: usize, norm: &[usize]) -> Option<(Vec<i64>, i64)> {
    if enc != 0 && enc != 2 {
        return None;
    }
    let mut factor = vec![0i64; num.max(1)];
    let mut n = 64 - norm.first().copied()? as i32;
    let mut f = 1i64;
    let mut i = norm[0];
    let mut k = 0usize;
    for _ in 0..24 {
        if i >= num && k != order {
            break;
        }
        if k == order {
            factor[0] = f;
            f = f.checked_mul(PIVOT[enc as usize])?;
        } else {
            if i >= factor.len() || norm.get(i).copied().unwrap_or(0) == 0 {
                return None;
            }
            factor[i] = f;
            f = f.checked_mul(subfactor(norm[i] as i32, n))?;
            n -= norm[i] as i32;
            i += norm[i];
        }
        k += 1;
    }
    Some((factor, f))
}

fn factors_pawn(
    num: usize,
    order: usize,
    order2: usize,
    norm: &[usize],
    file: usize,
) -> Option<(Vec<i64>, i64)> {
    let g = geom();
    let mut factor = vec![0i64; num.max(1)];
    let mut i = *norm.first()?;
    if order2 < 0x0f {
        i += *norm.get(i)?;
    }
    let mut n = 64 - i as i32;
    let mut fac = 1i64;
    let mut k = 0usize;
    for _ in 0..24 {
        if i >= num && k != order && k != order2 {
            break;
        }
        if k == order {
            factor[0] = fac;
            let pi = norm[0].checked_sub(1)?;
            fac = fac.checked_mul(*g.pfactor.get(pi)?.get(file)?)?;
        } else if k == order2 {
            let idx = norm[0];
            if idx >= factor.len() {
                return None;
            }
            factor[idx] = fac;
            fac = fac.checked_mul(subfactor(norm[idx] as i32, 48 - norm[0] as i32))?;
        } else {
            if i >= factor.len() || norm.get(i).copied().unwrap_or(0) == 0 {
                return None;
            }
            factor[i] = fac;
            fac = fac.checked_mul(subfactor(norm[i] as i32, n))?;
            n -= norm[i] as i32;
            i += norm[i];
        }
        k += 1;
    }
    Some((factor, fac))
}

fn fold_triangle(p: &mut [u8], n: usize, group: usize) {
    if p[0] & 0x04 != 0 {
        for s in p.iter_mut().take(n) {
            *s ^= 0x07;
        }
    }
    if p[0] & 0x20 != 0 {
        for s in p.iter_mut().take(n) {
            *s ^= 0x38;
        }
    }
    let mut k = 0;
    while k < n && offdiag(p[k]) == 0 {
        k += 1;
    }
    if k < group && k < n && offdiag(p[k]) > 0 {
        for s in p.iter_mut().take(n) {
            *s = flipdiag(*s);
        }
    }
}

fn encode_piece(enc: i32, num: usize, norm: &[usize], p: &mut [u8], factor: &[i64]) -> Option<i64> {
    if enc == 0 {
        fold_triangle(p, num, 3);
    } else if enc == 2 {
        fold_triangle(p, num, 2);
    } else {
        return None;
    }
    let (mut idx, mut i) = if enc == 0 {
        if num < 3 {
            return None;
        }
        let ii = i64::from(p[1] > p[0]);
        let jj = i64::from(p[2] > p[0]) + i64::from(p[2] > p[1]);
        let idx = if offdiag(p[0]) != 0 {
            TRIANGLE[p[0] as usize] as i64 * 63 * 62 + (p[1] as i64 - ii) * 62 + (p[2] as i64 - jj)
        } else if offdiag(p[1]) != 0 {
            6 * 63 * 62
                + DIAG[p[0] as usize] as i64 * 28 * 62
                + LOWER[p[1] as usize] as i64 * 62
                + p[2] as i64
                - jj
        } else if offdiag(p[2]) != 0 {
            6 * 63 * 62
                + 4 * 28 * 62
                + DIAG[p[0] as usize] as i64 * 7 * 28
                + (DIAG[p[1] as usize] as i64 - ii) * 28
                + LOWER[p[2] as usize] as i64
        } else {
            6 * 63 * 62
                + 4 * 28 * 62
                + 4 * 7 * 28
                + DIAG[p[0] as usize] as i64 * 7 * 6
                + (DIAG[p[1] as usize] as i64 - ii) * 6
                + (DIAG[p[2] as usize] as i64 - jj)
        };
        (idx, 3usize)
    } else {
        if num < 2 {
            return None;
        }
        let idx = KK_IDX[TRIANGLE[p[0] as usize] as usize][p[1] as usize] as i64;
        if idx < 0 {
            return None;
        }
        (idx, 2usize)
    };
    idx = idx.checked_mul(*factor.first()?)?;
    while i < num {
        let t = *norm.get(i)?;
        if t == 0 || i + t > num {
            return None;
        }
        p[i..i + t].sort_unstable();
        let mut s = 0i64;
        for m in i..i + t {
            let pv = p[m] as i32;
            let mut j = 0i32;
            for &earlier in p.iter().take(i) {
                if pv > earlier as i32 {
                    j += 1;
                }
            }
            s += binom(pv - j, (m - i + 1) as i32);
        }
        idx = idx.checked_add(s.checked_mul(*factor.get(i)?)?)?;
        i += t;
    }
    Some(idx)
}

fn encode_pawn(
    pawns: [usize; 2],
    num: usize,
    norm: &[usize],
    p: &mut [u8],
    factor: &[i64],
) -> Option<i64> {
    let g = geom();
    let n0 = pawns[0];
    if n0 == 0 || n0 > num {
        return None;
    }
    if p[0] & 0x04 != 0 {
        for s in p.iter_mut().take(num) {
            *s ^= 0x07;
        }
    }
    if n0 > 1 {
        p[1..n0].sort_by(|&a, &b| PTWIST[b as usize].cmp(&PTWIST[a as usize]));
    }
    let t = n0 - 1;
    let flap = FLAP[p[0] as usize] as usize;
    let mut idx = *g.pawn_idx.get(t)?.get(flap)?;
    for i in (1..=t).rev() {
        idx += binom(PTWIST[p[i] as usize] as i32, (t - i + 1) as i32);
    }
    idx = idx.checked_mul(*factor.first()?)?;

    let mut i = n0;
    let end_pawns = n0 + pawns[1];
    if end_pawns > i {
        if end_pawns > num {
            return None;
        }
        p[i..end_pawns].sort_unstable();
        let mut s = 0i64;
        for m in i..end_pawns {
            let pv = p[m] as i32;
            let mut j = 0i32;
            for &earlier in p.iter().take(i) {
                if pv > earlier as i32 {
                    j += 1;
                }
            }
            s += binom(pv - j - 8, (m - i + 1) as i32);
        }
        idx = idx.checked_add(s.checked_mul(*factor.get(i)?)?)?;
        i = end_pawns;
    }
    while i < num {
        let grp = *norm.get(i)?;
        if grp == 0 || i + grp > num {
            return None;
        }
        p[i..i + grp].sort_unstable();
        let mut s = 0i64;
        for m in i..i + grp {
            let pv = p[m] as i32;
            let mut j = 0i32;
            for &earlier in p.iter().take(i) {
                if pv > earlier as i32 {
                    j += 1;
                }
            }
            s += binom(pv - j, (m - i + 1) as i32);
        }
        idx = idx.checked_add(s.checked_mul(*factor.get(i)?)?)?;
        i += grp;
    }
    Some(idx)
}

fn pawn_file(p: &mut [u8], n_pawns: usize) -> usize {
    for i in 1..n_pawns {
        if FLAP[p[0] as usize] > FLAP[p[i] as usize] {
            p.swap(0, i);
        }
    }
    FILE_TO_FILE[(p[0] & 7) as usize] as usize
}

fn squares_of(pos: &Position, code: u8, cmirror: u8) -> Vec<u8> {
    let code = code ^ cmirror;
    let pt = code & 7;
    let color = if code >> 3 == 0 {
        Color::White
    } else {
        Color::Black
    };
    let mut v = Vec::new();
    for sq in 0..64u8 {
        let p = pos.squares[sq as usize];
        if p != EMPTY && type_of(p) == pt && color_of(p) == color {
            v.push(sq);
        }
    }
    v
}

fn fill_pieces(
    pos: &Position,
    pieces: &[u8],
    cmirror: u8,
    mirror: u8,
    into: &mut [u8],
) -> Option<()> {
    let n = pieces.len();
    let mut i = 0;
    while i < n {
        let sqs = squares_of(pos, pieces[i], cmirror);
        let run = sqs.len();
        if run == 0 || i + run > n {
            return None;
        }
        for k in 0..run {
            if pieces[i + k] != pieces[i] {
                return None;
            }
            into[i + k] = sqs[k] ^ mirror;
        }
        i += run;
    }
    if i != n { None } else { Some(()) }
}

struct Side {
    pieces: Vec<u8>,
    norm: Vec<usize>,
    factor: Vec<i64>,
    pairs: Pairs,
}

struct WdlTable {
    bytes: Vec<u8>,
    ok: bool,
    num: usize,
    has_pawns: bool,
    symmetric: bool,
    enc_type: i32,
    pawns: [usize; 2],
    key: String,
    sides: [Option<Side>; 2],
    files: Vec<[Option<Side>; 2]>,
}

impl WdlTable {
    fn broken() -> Self {
        Self {
            bytes: Vec::new(),
            ok: false,
            num: 0,
            has_pawns: false,
            symmetric: false,
            enc_type: 0,
            pawns: [0, 0],
            key: String::new(),
            sides: [None, None],
            files: Vec::new(),
        }
    }

    fn load(path: &Path) -> Self {
        Self::load_inner(path).unwrap_or_else(Self::broken)
    }

    fn load_inner(path: &Path) -> Option<Self> {
        let stem = path.file_stem()?.to_str()?;
        let bytes = read_file(path)?;
        if bytes.len() < 16 || bytes[..4] != WDL_MAGIC {
            return None;
        }
        let num = stem.len() - 1;
        if num < 3 || num > MAX_MEN || *bytes.get(4)? >> 4 != num as u8 {
            return None;
        }
        let split = bytes[4] & 1 != 0;
        let has_pawns = bytes[4] & 2 != 0;
        let symmetric = normalize_name(stem, false) == normalize_name(stem, true);
        let enc_type = if has_pawns { 0 } else { enc_type_of(stem) };
        let pawns = if has_pawns { pawn_counts(stem) } else { [0, 0] };
        if !has_pawns && enc_type < 0 {
            return None;
        }

        let mut table = Self {
            bytes,
            ok: false,
            num,
            has_pawns,
            symmetric,
            enc_type,
            pawns,
            key: String::new(),
            sides: [None, None],
            files: Vec::new(),
        };

        if !has_pawns {
            let mut ptr = 5usize;
            let pieces0: Vec<u8> = (0..num).map(|i| table.bytes[ptr + 1 + i] & 0x0f).collect();
            let pieces1: Vec<u8> = (0..num).map(|i| table.bytes[ptr + 1 + i] >> 4).collect();
            let order0 = (table.bytes[ptr] & 0x0f) as usize;
            let order1 = (table.bytes[ptr] >> 4) as usize;
            table.key = recalc_key(&pieces0, false);
            ptr += num + 1;
            ptr = align2(ptr);
            let norm0 = norm_piece(enc_type, &pieces0)?;
            let (factor0, size0) = factors_piece(enc_type, num, order0, &norm0)?;
            let norm1 = norm_piece(enc_type, &pieces1)?;
            let (factor1, size1) = factors_piece(enc_type, num, order1, &norm1)?;
            let (mut pairs0, next, sz0) = setup_pairs(&table.bytes, ptr, size0, true)?;
            ptr = next;
            let pairs1 = if split {
                let (p, next, sz1) = setup_pairs(&table.bytes, ptr, size1, true)?;
                ptr = next;
                Some((p, sz1))
            } else {
                None
            };
            pairs0.indextable = ptr;
            ptr += sz0.index as usize;
            if let Some((_, sz1)) = pairs1.as_ref() {
                // indextable of side 1 set below
                let _ = sz1;
            }
            let mut pairs1 = pairs1;
            if let Some((ref mut p1, sz1)) = pairs1 {
                p1.indextable = ptr;
                ptr += sz1.index as usize;
            }
            pairs0.sizetable = ptr;
            ptr += sz0.size as usize;
            if let Some((ref mut p1, sz1)) = pairs1 {
                p1.sizetable = ptr;
                ptr += sz1.size as usize;
            }
            ptr = align64(ptr);
            pairs0.data = ptr;
            ptr += sz0.data as usize;
            if let Some((ref mut p1, sz1)) = pairs1 {
                ptr = align64(ptr);
                p1.data = ptr;
                let _ = sz1;
            }
            table.sides[0] = Some(Side {
                pieces: pieces0,
                norm: norm0,
                factor: factor0,
                pairs: pairs0,
            });
            if let Some((p1, _)) = pairs1 {
                table.sides[1] = Some(Side {
                    pieces: pieces1,
                    norm: norm1,
                    factor: factor1,
                    pairs: p1,
                });
            }
        } else {
            table.key = normalize_name(stem, false);
            let s = 1 + usize::from(pawns[1] > 0);
            let mut ptr = 5usize;
            let mut meta = Vec::with_capacity(4);
            for _f in 0..4 {
                let order0 = (table.bytes[ptr] & 0x0f) as usize;
                let order1 = (table.bytes[ptr] >> 4) as usize;
                let order2_0 = if pawns[1] > 0 {
                    (table.bytes[ptr + 1] & 0x0f) as usize
                } else {
                    0x0f
                };
                let order2_1 = if pawns[1] > 0 {
                    (table.bytes[ptr + 1] >> 4) as usize
                } else {
                    0x0f
                };
                let base = ptr + s;
                let pieces0: Vec<u8> = (0..num).map(|i| table.bytes[base + i] & 0x0f).collect();
                let pieces1: Vec<u8> = (0..num).map(|i| table.bytes[base + i] >> 4).collect();
                meta.push((order0, order1, order2_0, order2_1, pieces0, pieces1));
                ptr += num + s;
            }
            ptr = align2(ptr);
            let mut built: Vec<(Side, Option<Side>, Sizes, Option<Sizes>)> = Vec::new();
            for f in 0..4 {
                let (order0, order1, order2_0, order2_1, pieces0, pieces1) = &meta[f];
                let norm0 = norm_pawn(pawns, pieces0)?;
                let (factor0, size0) = factors_pawn(num, *order0, *order2_0, &norm0, f)?;
                let (pairs0, next, sz0) = setup_pairs(&table.bytes, ptr, size0, true)?;
                ptr = next;
                let side1 = if split {
                    let norm1 = norm_pawn(pawns, pieces1)?;
                    let (factor1, size1) = factors_pawn(num, *order1, *order2_1, &norm1, f)?;
                    let (p1, next, sz1) = setup_pairs(&table.bytes, ptr, size1, true)?;
                    ptr = next;
                    Some((
                        Side {
                            pieces: pieces1.clone(),
                            norm: norm1,
                            factor: factor1,
                            pairs: p1,
                        },
                        sz1,
                    ))
                } else {
                    None
                };
                let side0 = Side {
                    pieces: pieces0.clone(),
                    norm: norm0,
                    factor: factor0,
                    pairs: pairs0,
                };
                match side1 {
                    Some((s1, sz1)) => built.push((side0, Some(s1), sz0, Some(sz1))),
                    None => built.push((side0, None, sz0, None)),
                }
            }
            // The pairs inside side0 were moved before index assignment. Re-walk pointers.
            // Rebuild placement from `built`, whose pairs still have zero index fields.
            let mut ptr_idx = ptr;
            for (side0, side1, sz0, sz1) in built.iter_mut() {
                side0.pairs.indextable = ptr_idx;
                ptr_idx += sz0.index as usize;
                if let (Some(s1), Some(sz1)) = (side1.as_mut(), sz1.as_ref()) {
                    s1.pairs.indextable = ptr_idx;
                    ptr_idx += sz1.index as usize;
                }
            }
            for (side0, side1, sz0, sz1) in built.iter_mut() {
                side0.pairs.sizetable = ptr_idx;
                ptr_idx += sz0.size as usize;
                if let (Some(s1), Some(sz1)) = (side1.as_mut(), sz1.as_ref()) {
                    s1.pairs.sizetable = ptr_idx;
                    ptr_idx += sz1.size as usize;
                }
            }
            for (side0, side1, sz0, sz1) in built.iter_mut() {
                ptr_idx = align64(ptr_idx);
                side0.pairs.data = ptr_idx;
                ptr_idx += sz0.data as usize;
                if let (Some(s1), Some(sz1)) = (side1.as_mut(), sz1.as_ref()) {
                    ptr_idx = align64(ptr_idx);
                    s1.pairs.data = ptr_idx;
                    ptr_idx += sz1.data as usize;
                }
            }
            table.files = built
                .into_iter()
                .map(|(s0, s1, _, _)| [Some(s0), s1])
                .collect();
        }
        table.ok = true;
        Some(table)
    }

    fn probe(&self, pos: &Position) -> Option<i32> {
        if !self.ok {
            return None;
        }
        let key = material_key(pos);
        let (cmirror, mirror, bside) = orient(self.symmetric, &key, &self.key, pos.side);
        let raw = if !self.has_pawns {
            let side = self.sides[bside].as_ref()?;
            let mut p = [0u8; 6];
            fill_pieces(pos, &side.pieces, cmirror, 0, &mut p)?;
            let idx = encode_piece(self.enc_type, self.num, &side.norm, &mut p, &side.factor)?;
            side.pairs.decompress(&self.bytes, idx, false)?
        } else {
            let lead = self.files.first()?.get(0)?.as_ref()?;
            let mut p = [0u8; 6];
            let lead_sq = squares_of(pos, lead.pieces[0], cmirror);
            if lead_sq.len() != self.pawns[0] {
                return None;
            }
            for (i, sq) in lead_sq.into_iter().enumerate() {
                p[i] = sq ^ mirror;
            }
            let f = pawn_file(&mut p, self.pawns[0]);
            let side = self.files.get(f)?.get(bside)?.as_ref()?;
            let mut i = self.pawns[0];
            while i < self.num {
                let sqs = squares_of(pos, side.pieces[i], cmirror);
                let run = sqs.len();
                if run == 0 || i + run > self.num {
                    return None;
                }
                for k in 0..run {
                    if side.pieces[i + k] != side.pieces[i] {
                        return None;
                    }
                    p[i + k] = sqs[k] ^ mirror;
                }
                i += run;
            }
            let idx = encode_pawn(self.pawns, self.num, &side.norm, &mut p, &side.factor)?;
            side.pairs.decompress(&self.bytes, idx, false)?
        };
        Some(raw - 2)
    }
}

fn orient(symmetric: bool, key: &str, table_key: &str, side: Color) -> (u8, u8, usize) {
    if !symmetric {
        if key != table_key {
            let bside = usize::from(side == Color::White);
            (8, 0x38, bside)
        } else {
            let bside = usize::from(side != Color::White);
            (0, 0, bside)
        }
    } else if side == Color::White {
        (0, 0, 0)
    } else {
        (8, 0x38, 0)
    }
}

struct DtzSide {
    pieces: Vec<u8>,
    norm: Vec<usize>,
    factor: Vec<i64>,
    pairs: Pairs,
    flags: u8,
    map_idx: [usize; 4],
    mapped: bool,
    wide: bool,
}

struct DtzTable {
    bytes: Vec<u8>,
    ok: bool,
    num: usize,
    has_pawns: bool,
    symmetric: bool,
    enc_type: i32,
    pawns: [usize; 2],
    key: String,
    map_base: usize,
    piece: Option<DtzSide>,
    files: Vec<Option<DtzSide>>,
}

impl DtzTable {
    fn broken() -> Self {
        Self {
            bytes: Vec::new(),
            ok: false,
            num: 0,
            has_pawns: false,
            symmetric: false,
            enc_type: 0,
            pawns: [0, 0],
            key: String::new(),
            map_base: 0,
            piece: None,
            files: Vec::new(),
        }
    }

    fn load(path: &Path) -> Self {
        Self::load_inner(path).unwrap_or_else(Self::broken)
    }

    fn load_inner(path: &Path) -> Option<Self> {
        let stem = path.file_stem()?.to_str()?;
        let bytes = read_file(path)?;
        if bytes.len() < 16 || bytes[..4] != DTZ_MAGIC {
            return None;
        }
        let num = stem.len() - 1;
        if num < 3 || num > MAX_MEN || bytes[4] >> 4 != num as u8 {
            return None;
        }
        let has_pawns = bytes[4] & 2 != 0;
        let symmetric = normalize_name(stem, false) == normalize_name(stem, true);
        let enc_type = if has_pawns { 0 } else { enc_type_of(stem) };
        let pawns = if has_pawns { pawn_counts(stem) } else { [0, 0] };
        if !has_pawns && enc_type < 0 {
            return None;
        }
        let mut table = Self {
            bytes,
            ok: false,
            num,
            has_pawns,
            symmetric,
            enc_type,
            pawns,
            key: String::new(),
            map_base: 0,
            piece: None,
            files: Vec::new(),
        };
        if !has_pawns {
            let ptr0 = 5usize;
            let pieces: Vec<u8> = (0..num).map(|i| table.bytes[ptr0 + 1 + i] & 0x0f).collect();
            let order = (table.bytes[ptr0] & 0x0f) as usize;
            table.key = recalc_key(&pieces, false);
            let mut ptr = align2(ptr0 + num + 1);
            let norm = norm_piece(enc_type, &pieces)?;
            let (factor, tb_size) = factors_piece(enc_type, num, order, &norm)?;
            let (mut pairs, next, sz) = setup_pairs(&table.bytes, ptr, tb_size, false)?;
            let flags = table.bytes[ptr];
            ptr = next;
            table.map_base = ptr;
            let mut map_idx = [0usize; 4];
            let mapped = flags & 2 != 0;
            let wide = flags & 16 != 0;
            if mapped {
                if !wide {
                    for slot in &mut map_idx {
                        let n = *table.bytes.get(ptr)? as usize;
                        *slot = ptr + 1 - table.map_base;
                        ptr += 1 + n;
                    }
                } else {
                    for slot in &mut map_idx {
                        *slot = (ptr + 2 - table.map_base) / 2;
                        let n = ru16(&table.bytes, ptr)? as usize;
                        ptr += 2 + 2 * n;
                    }
                }
            }
            ptr = align2(ptr);
            pairs.indextable = ptr;
            ptr += sz.index as usize;
            pairs.sizetable = ptr;
            ptr += sz.size as usize;
            ptr = align64(ptr);
            pairs.data = ptr;
            table.piece = Some(DtzSide {
                pieces,
                norm,
                factor,
                pairs,
                flags,
                map_idx,
                mapped,
                wide,
            });
        } else {
            table.key = normalize_name(stem, false);
            let s = 1 + usize::from(pawns[1] > 0);
            let mut ptr = 5usize;
            let mut meta = Vec::with_capacity(4);
            for _ in 0..4 {
                let order = (table.bytes[ptr] & 0x0f) as usize;
                let order2 = if pawns[1] > 0 {
                    (table.bytes[ptr + 1] & 0x0f) as usize
                } else {
                    0x0f
                };
                let base = ptr + s;
                let pieces: Vec<u8> = (0..num).map(|i| table.bytes[base + i] & 0x0f).collect();
                meta.push((order, order2, pieces));
                ptr += num + s;
            }
            ptr = align2(ptr);
            let mut recs = Vec::with_capacity(4);
            let mut flags_list = Vec::with_capacity(4);
            for f in 0..4 {
                let (order, order2, pieces) = &meta[f];
                let norm = norm_pawn(pawns, pieces)?;
                let (factor, tb_size) = factors_pawn(num, *order, *order2, &norm, f)?;
                let flags = *table.bytes.get(ptr)?;
                let (pairs, next, sz) = setup_pairs(&table.bytes, ptr, tb_size, false)?;
                ptr = next;
                flags_list.push(flags);
                recs.push((pieces.clone(), norm, factor, pairs, sz, flags));
            }
            table.map_base = ptr;
            let mut map_idxs = vec![[0usize; 4]; 4];
            for (f, flags) in flags_list.iter().enumerate() {
                if flags & 2 == 0 {
                    continue;
                }
                if flags & 16 == 0 {
                    for slot in &mut map_idxs[f] {
                        let n = *table.bytes.get(ptr)? as usize;
                        *slot = ptr + 1 - table.map_base;
                        ptr += 1 + n;
                    }
                } else {
                    ptr = align2(ptr);
                    for slot in &mut map_idxs[f] {
                        *slot = (ptr + 2 - table.map_base) / 2;
                        let n = ru16(&table.bytes, ptr)? as usize;
                        ptr += 2 + 2 * n;
                    }
                }
            }
            ptr = align2(ptr);
            for (rec, map_idx) in recs.iter_mut().zip(map_idxs.iter()) {
                rec.3.indextable = ptr;
                ptr += rec.4.index as usize;
                let _ = map_idx;
            }
            for rec in recs.iter_mut() {
                rec.3.sizetable = ptr;
                ptr += rec.4.size as usize;
            }
            for rec in recs.iter_mut() {
                ptr = align64(ptr);
                rec.3.data = ptr;
                ptr += rec.4.data as usize;
            }
            table.files = recs
                .into_iter()
                .zip(map_idxs)
                .map(|((pieces, norm, factor, pairs, _, flags), map_idx)| {
                    Some(DtzSide {
                        pieces,
                        norm,
                        factor,
                        pairs,
                        flags,
                        map_idx,
                        mapped: flags & 2 != 0,
                        wide: flags & 16 != 0,
                    })
                })
                .collect();
        }
        table.ok = true;
        Some(table)
    }

    /// Raw stored distance and 1, or `(0, -1)` when this side is not in the file.
    fn probe(&self, pos: &Position, wdl: i32) -> Option<(i32, i32)> {
        if !self.ok || !(-2..=2).contains(&wdl) {
            return None;
        }
        let key = material_key(pos);
        let (cmirror, mirror, bside) = orient(self.symmetric, &key, &self.key, pos.side);
        if !self.has_pawns {
            let side = self.piece.as_ref()?;
            if (side.flags & 1) != bside as u8 && !self.symmetric {
                return Some((0, -1));
            }
            let mut p = [0u8; 6];
            fill_pieces(pos, &side.pieces, cmirror, 0, &mut p)?;
            let idx = encode_piece(self.enc_type, self.num, &side.norm, &mut p, &side.factor)?;
            let res = side.pairs.decompress(&self.bytes, idx, true)?;
            let res = apply_dtz_map(&self.bytes, self.map_base, side, wdl, res)?;
            Some((res, 1))
        } else {
            let lead = self.files.first()?.as_ref()?;
            let mut p = [0u8; 6];
            let lead_sq = squares_of(pos, lead.pieces[0], cmirror);
            if lead_sq.len() != self.pawns[0] {
                return None;
            }
            for (i, sq) in lead_sq.into_iter().enumerate() {
                p[i] = sq ^ mirror;
            }
            let f = pawn_file(&mut p, self.pawns[0]);
            let side = self.files.get(f)?.as_ref()?;
            if (side.flags & 1) != bside as u8 {
                return Some((0, -1));
            }
            let mut i = self.pawns[0];
            while i < self.num {
                let sqs = squares_of(pos, side.pieces[i], cmirror);
                let run = sqs.len();
                if run == 0 || i + run > self.num {
                    return None;
                }
                for k in 0..run {
                    if side.pieces[i + k] != side.pieces[i] {
                        return None;
                    }
                    p[i + k] = sqs[k] ^ mirror;
                }
                i += run;
            }
            let idx = encode_pawn(self.pawns, self.num, &side.norm, &mut p, &side.factor)?;
            let res = side.pairs.decompress(&self.bytes, idx, true)?;
            let res = apply_dtz_map(&self.bytes, self.map_base, side, wdl, res)?;
            Some((res, 1))
        }
    }
}

fn apply_dtz_map(
    bytes: &[u8],
    map_base: usize,
    side: &DtzSide,
    wdl: i32,
    mut res: i32,
) -> Option<i32> {
    if side.mapped {
        let m = WDL_TO_MAP[(wdl + 2) as usize];
        if !side.wide {
            let i = map_base + side.map_idx[m] + res as usize;
            res = *bytes.get(i)? as i32;
        } else {
            let i = map_base + 2 * (side.map_idx[m] + res as usize);
            res = ru16(bytes, i)? as i32;
        }
    }
    if side.flags & PA_FLAGS[(wdl + 2) as usize] == 0 || wdl & 1 != 0 {
        res *= 2;
    }
    Some(res)
}

struct Slot<T> {
    path: PathBuf,
    loaded: OnceLock<T>,
}

pub struct Tablebase {
    wdl: std::collections::HashMap<String, Arc<Slot<WdlTable>>>,
    dtz: std::collections::HashMap<String, Arc<Slot<DtzTable>>>,
}

impl Tablebase {
    pub fn empty() -> Self {
        Self {
            wdl: std::collections::HashMap::new(),
            dtz: std::collections::HashMap::new(),
        }
    }

    pub fn open(dir: impl AsRef<Path>) -> Self {
        let mut tb = Self::empty();
        let dir = dir.as_ref();
        let Ok(rd) = std::fs::read_dir(dir) else {
            return tb;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
            if !stem.contains('v') || stem.len() < 4 || stem.len() > MAX_MEN + 1 {
                continue;
            }
            let has_pawns = stem.contains('P');
            let (key, mirror) = if has_pawns {
                (normalize_name(stem, false), normalize_name(stem, true))
            } else {
                let Some(prefix) = read_prefix(&path, 32) else {
                    continue;
                };
                let num = stem.len() - 1;
                if prefix.len() < 6 + num {
                    continue;
                }
                let pieces: Vec<u8> = (0..num).map(|i| prefix[6 + i] & 0x0f).collect();
                (recalc_key(&pieces, false), recalc_key(&pieces, true))
            };
            if ext == "rtbw" {
                let slot = Arc::new(Slot {
                    path: path.clone(),
                    loaded: OnceLock::new(),
                });
                tb.wdl.insert(key, Arc::clone(&slot));
                tb.wdl.insert(mirror, slot);
            } else if ext == "rtbz" {
                let slot = Arc::new(Slot {
                    path,
                    loaded: OnceLock::new(),
                });
                tb.dtz.insert(key, Arc::clone(&slot));
                tb.dtz.insert(mirror, slot);
            }
        }
        tb
    }

    pub fn is_empty(&self) -> bool {
        self.wdl.is_empty()
    }

    fn wdl_table(&self, key: &str) -> Option<&WdlTable> {
        let slot = self.wdl.get(key)?;
        let path = slot.path.clone();
        let table = slot.loaded.get_or_init(|| WdlTable::load(&path));
        if table.ok { Some(table) } else { None }
    }

    fn dtz_table(&self, key: &str) -> Option<&DtzTable> {
        let slot = self.dtz.get(key)?;
        let path = slot.path.clone();
        let table = slot.loaded.get_or_init(|| DtzTable::load(&path));
        if table.ok { Some(table) } else { None }
    }

    pub fn probe_wdl(&self, pos: &mut Position) -> Option<i32> {
        if self.wdl.is_empty() || pos.castling != 0 {
            return None;
        }
        let n = men(pos);
        if n > MAX_MEN || n < 2 {
            return None;
        }
        if pos.in_check() {
            let ms = legal_moves(pos);
            if ms.is_empty() {
                return Some(-2);
            }
        }
        if n == 2 {
            return Some(0);
        }
        let (mut v, _) = self.probe_ab(pos, -2, 2)?;
        if pos.ep.is_none() {
            return Some(v);
        }
        let eps: Vec<Move> = legal_moves(pos).into_iter().filter(|m| m.is_ep()).collect();
        let mut v1 = -3;
        for m in eps {
            let u = pos.make(m);
            let child = self.probe_ab(pos, -2, 2).map(|(x, _)| -x);
            pos.unmake(m, u);
            let v0 = child?;
            if v0 > v1 {
                v1 = v0;
            }
        }
        if v1 > -3 {
            if v1 >= v {
                v = v1;
            } else if v == 0 {
                let legal = legal_moves(pos);
                if !legal.is_empty() && legal.iter().all(|m| m.is_ep()) {
                    v = v1;
                }
            }
        }
        Some(v)
    }

    fn probe_ab(&self, pos: &mut Position, mut alpha: i32, beta: i32) -> Option<(i32, i32)> {
        let caps: Vec<Move> = legal_moves(pos)
            .into_iter()
            .filter(|m| pos.is_capture(*m) && !m.is_ep())
            .collect();
        for m in caps {
            let u = pos.make(m);
            let child = self.probe_ab(pos, -beta, -alpha);
            pos.unmake(m, u);
            let (v_plus, _) = child?;
            let v = -v_plus;
            if v > alpha {
                if v >= beta {
                    return Some((v, 2));
                }
                alpha = v;
            }
        }
        let v = self.probe_wdl_table(pos)?;
        if alpha >= v {
            Some((alpha, 1 + i32::from(alpha > 0)))
        } else {
            Some((v, 1))
        }
    }

    fn probe_wdl_table(&self, pos: &Position) -> Option<i32> {
        if men(pos) == 2 {
            return Some(0);
        }
        let key = material_key(pos);
        self.wdl_table(&key)?.probe(pos)
    }

    pub fn probe_dtz(&self, pos: &mut Position) -> Option<i32> {
        self.probe_dtz_at(pos, 0)
    }

    fn probe_dtz_at(&self, pos: &mut Position, depth: i32) -> Option<i32> {
        if depth > MAX_DTZ_DEPTH || self.wdl.is_empty() || pos.castling != 0 {
            return None;
        }
        let n = men(pos);
        if n > MAX_MEN || n < 2 {
            return None;
        }
        if pos.in_check() {
            let ms = legal_moves(pos);
            if ms.is_empty() {
                return Some(-1);
            }
        }
        if n == 2 {
            return Some(0);
        }
        let mut v = self.probe_dtz_no_ep(pos, depth)?;
        if pos.ep.is_none() {
            return Some(v);
        }
        let eps: Vec<Move> = legal_moves(pos).into_iter().filter(|m| m.is_ep()).collect();
        let mut v1 = -3;
        for m in eps {
            let u = pos.make(m);
            let child = self.probe_ab(pos, -2, 2).map(|(x, _)| -x);
            pos.unmake(m, u);
            let v0 = child?;
            if v0 > v1 {
                v1 = v0;
            }
        }
        if v1 > -3 {
            let mapped = WDL_TO_DTZ[(v1 + 2) as usize];
            if v < -100 {
                if mapped >= 0 {
                    v = mapped;
                }
            } else if v < 0 {
                if mapped >= 0 || mapped < -100 {
                    v = mapped;
                }
            } else if v > 100 {
                if mapped > 0 {
                    v = mapped;
                }
            } else if v > 0 {
                if mapped == 1 {
                    v = mapped;
                }
            } else if mapped >= 0 {
                v = mapped;
            } else {
                let legal = legal_moves(pos);
                if !legal.is_empty() && legal.iter().all(|m| m.is_ep()) {
                    v = mapped;
                }
            }
        }
        Some(v)
    }

    fn probe_dtz_no_ep(&self, pos: &mut Position, depth: i32) -> Option<i32> {
        let (wdl, success) = self.probe_ab(pos, -2, 2)?;
        if wdl == 0 {
            return Some(0);
        }
        if success == 2 {
            return Some(dtz_before_zeroing(wdl));
        }
        if wdl > 0 {
            let pushes: Vec<Move> = legal_moves(pos)
                .into_iter()
                .filter(|m| type_of(pos.piece_at(m.from)) == PAWN && !pos.is_capture(*m))
                .collect();
            for m in pushes {
                let u = pos.make(m);
                let child = self.probe_wdl(pos).map(|w| -w);
                pos.unmake(m, u);
                let v = child?;
                if v == wdl {
                    return Some(if v == 2 { 1 } else { 101 });
                }
            }
        }
        let key = material_key(pos);
        if let Some(table) = self.dtz_table(&key) {
            if let Some((dtz, ok)) = table.probe(pos, wdl) {
                if ok >= 0 {
                    let base = dtz_before_zeroing(wdl);
                    return Some(base + if wdl > 0 { dtz } else { -dtz });
                }
            }
        }
        if wdl > 0 {
            let mut best = 0xffff;
            let quiets: Vec<Move> = legal_moves(pos)
                .into_iter()
                .filter(|m| type_of(pos.piece_at(m.from)) != PAWN && !pos.is_capture(*m))
                .collect();
            for m in quiets {
                let u = pos.make(m);
                let child = self.probe_dtz_at(pos, depth + 1).map(|d| -d);
                let mate = pos.in_check() && legal_moves(pos).is_empty();
                pos.unmake(m, u);
                let v = child?;
                if v == 1 && mate {
                    best = 1;
                } else if v > 0 && v + 1 < best {
                    best = v + 1;
                }
            }
            Some(best)
        } else {
            let mut best = -1;
            for m in legal_moves(pos) {
                let u = pos.make(m);
                let v = if pos.halfmove == 0 {
                    if wdl == -2 {
                        Some(-1)
                    } else {
                        self.probe_ab(pos, 1, 2)
                            .map(|(vv, _)| if vv == 2 { 0 } else { -101 })
                    }
                } else {
                    self.probe_dtz_at(pos, depth + 1).map(|d| -d - 1)
                };
                pos.unmake(m, u);
                let v = v?;
                if v < best {
                    best = v;
                }
            }
            Some(best)
        }
    }

    fn move_outcome(&self, pos: &mut Position, m: Move, history: &[u64]) -> Option<(i32, i32)> {
        let u = pos.make(m);
        let out = if repeats(pos, history) {
            Some((0, 0))
        } else {
            self.outcome_after(pos)
        };
        pos.unmake(m, u);
        out
    }

    fn outcome_after(&self, pos: &mut Position) -> Option<(i32, i32)> {
        let ms = legal_moves(pos);
        if ms.is_empty() {
            return Some(if pos.in_check() { (2, 1) } else { (0, 0) });
        }
        if pos.halfmove == 0 {
            let wdl = -self.probe_wdl(pos)?;
            let dtz = match wdl {
                2 => 1,
                1 => 101,
                0 => 0,
                -1 => -101,
                -2 => -1,
                _ => 0,
            };
            return Some((wdl, dtz));
        }
        let child_dtz = self.probe_dtz(pos)?;
        let child_wdl = wdl_of_dtz(child_dtz, pos.halfmove as i32);
        let our_wdl = -child_wdl;
        let our_dtz = if child_dtz < 0 {
            -child_dtz + 1
        } else if child_dtz > 0 {
            -child_dtz - 1
        } else {
            0
        };
        Some((our_wdl, our_dtz))
    }

    pub fn root_pick(&self, pos: &mut Position, history: &[u64]) -> Option<RootPick> {
        if self.is_empty() || pos.castling != 0 {
            return None;
        }
        let n = men(pos);
        if n > MAX_MEN || n < 2 {
            return None;
        }
        let moves = legal_moves(pos);
        if moves.is_empty() {
            return None;
        }
        let mut best_m = moves[0];
        let mut best_w = i32::MIN;
        let mut best_d = i32::MAX;
        for m in moves {
            let (wdl, dtz) = self.move_outcome(pos, m, history)?;
            if wdl > best_w || (wdl == best_w && dtz < best_d) {
                best_m = m;
                best_w = wdl;
                best_d = dtz;
            }
        }
        Some(RootPick {
            mv: best_m,
            wdl: best_w,
            dtz: best_d,
        })
    }
}

/// Same rule as the search: a position repeated within the halfmove window
/// is a draw. `history` does not yet include `pos`.
fn repeats(pos: &Position, history: &[u64]) -> bool {
    if pos.halfmove < 4 || history.is_empty() {
        return false;
    }
    let len = history.len() + 1;
    if len < 3 {
        return false;
    }
    let oldest = (len - 1).saturating_sub(pos.halfmove as usize);
    let mut i = len - 3;
    loop {
        if i < oldest {
            break;
        }
        if history.get(i).copied() == Some(pos.hash) {
            return true;
        }
        if i < 2 {
            break;
        }
        i -= 2;
    }
    false
}

pub struct RootPick {
    pub mv: Move,
    pub wdl: i32,
    pub dtz: i32,
}

fn legal_moves(pos: &mut Position) -> Vec<Move> {
    let mut m = Vec::with_capacity(48);
    pos.gen_legal_into(&mut m);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fen(s: &str) -> Position {
        Position::from_fen(s).expect(s)
    }

    fn loaded() -> Option<Tablebase> {
        let tb = Tablebase::open(DEFAULT_PATH);
        if tb.is_empty() { None } else { Some(tb) }
    }

    #[test]
    fn normalize_swaps_the_weaker_side() {
        assert_eq!(normalize_name("KPvKQ", false), "KQvKP");
        assert_eq!(normalize_name("KQvK", false), "KQvK");
        assert_eq!(
            normalize_name("KPvKP", false),
            normalize_name("KPvKP", true)
        );
    }

    #[test]
    fn known_wdl_and_dtz() {
        let Some(tb) = loaded() else { return };
        let cases = [
            ("8/8/8/4k3/8/8/4Q3/4K3 w - - 0 1", 2, "white KQ vs K"),
            (
                "8/8/8/4k3/8/8/4Q3/4K3 b - - 0 1",
                -2,
                "black to move KQ vs K",
            ),
            ("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", 0, "stalemate"),
            ("8/8/8/4k3/8/4N3/8/4K3 w - - 0 1", 0, "KN vs K"),
            ("8/8/8/4k3/8/4R3/8/4K3 w - - 0 1", 2, "KR vs K"),
            ("8/2K5/4B3/3N4/8/8/4k3/8 b - - 0 1", -2, "KBN vs K"),
            ("k7/P7/K7/8/8/8/8/8 w - - 0 1", 0, "rook pawn draw"),
            ("6k1/4P3/4K3/8/8/8/8/8 w - - 0 1", 2, "supported pawn"),
            (
                "8/8/8/8/8/8/1q6/2K4k w - - 0 1",
                0,
                "hanging queen is a draw",
            ),
            ("8/8/8/4k3/8/4q3/8/4K3 w - - 0 1", -2, "black has the queen"),
            ("4k3/8/8/8/8/8/8/4K3 w - - 0 1", 0, "KvK"),
        ];
        for (f, expect, name) in cases {
            let mut pos = fen(f);
            let got = tb.probe_wdl(&mut pos);
            assert_eq!(got, Some(expect), "{name}: {f}");
            assert_eq!(pos.to_fen(), fen(f).to_fen(), "{name} mutated the board");
        }
        let mut kbn = fen("8/2K5/4B3/3N4/8/8/4k3/8 b - - 0 1");
        assert_eq!(tb.probe_dtz(&mut kbn), Some(-53), "KBNK dtz");
    }

    #[test]
    fn root_mates_and_respects_the_clock() {
        let Some(tb) = loaded() else { return };
        let mut mate = fen("k7/8/1K6/8/8/8/8/7R w - - 0 1");
        let pick = tb.root_pick(&mut mate, &[]).expect("root");
        mate.make(pick.mv);
        assert!(
            mate.in_check() && legal_moves(&mut mate).is_empty(),
            "{}",
            pick.mv
        );

        let mut late = fen("k7/8/1K6/8/8/8/8/7R w - - 99 1");
        let pick = tb.root_pick(&mut late, &[]).expect("late mate");
        assert_eq!(pick.wdl, 2, "mate on the 100th half-move stays a win");
        late.make(pick.mv);
        assert!(late.in_check() && legal_moves(&mut late).is_empty());

        let mut drawn = fen("8/8/8/4k3/8/8/4K3/4R3 w - - 99 1");
        let pick = tb.root_pick(&mut drawn, &[]).expect("clock draw");
        assert_eq!(
            pick.wdl, 0,
            "KRK with 99 half-moves is a draw, dtz {}",
            pick.dtz
        );

        let mut trap = fen("7k/8/5QK1/8/8/8/8/8 w - - 0 1");
        let before = trap.to_fen();
        let pick = tb.root_pick(&mut trap, &[]).expect("trap");
        assert_eq!(trap.to_fen(), before);
        assert_eq!(pick.wdl, 2);
        trap.make(pick.mv);
        assert!(
            trap.in_check() && legal_moves(&mut trap).is_empty(),
            "root should mate, played {}",
            pick.mv
        );
    }

    #[test]
    fn en_passant_probe_returns_a_wdl() {
        let Some(tb) = loaded() else { return };
        let mut ep = fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
        let wdl = tb.probe_wdl(&mut ep).expect("ep position");
        assert!((-2..=2).contains(&wdl), "{wdl}");
        let mut quiet = fen("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1");
        assert!(tb.probe_wdl(&mut quiet).is_some());
    }

    #[test]
    fn castling_and_six_men_are_outside() {
        let Some(tb) = loaded() else { return };
        let mut castle = fen("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1");
        assert_eq!(tb.probe_wdl(&mut castle), None);
        let mut six = fen("6k1/5ppp/8/8/8/8/8/4R2K w - - 0 1");
        assert_eq!(tb.probe_wdl(&mut six), None);
    }
}
