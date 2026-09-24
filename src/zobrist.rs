//! Incremental position keys for repetition detection and the transposition table.
//!
//! Keys are generated at compile time with SplitMix64 from a seed derived from
//! the crate name, so they are not taken from another engine.

use crate::board::{Color, Position, color_of, type_of};

const SEED: u64 = 0x6772_6F6B_656E_6769; // bytes of "grokengi"

const fn splitmix64(mut x: u64) -> (u64, u64) {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (x, z ^ (z >> 31))
}

const fn fill_piece() -> [[[u64; 64]; 7]; 2] {
    let mut t = [[[0u64; 64]; 7]; 2];
    let mut state = SEED;
    let mut c = 0;
    while c < 2 {
        let mut pt = 1;
        while pt < 7 {
            let mut sq = 0;
            while sq < 64 {
                let (s, r) = splitmix64(state);
                state = s;
                t[c][pt][sq] = r;
                sq += 1;
            }
            pt += 1;
        }
        c += 1;
    }
    t
}

const fn fill_16(start: u64) -> ([u64; 16], u64) {
    let mut t = [0u64; 16];
    let mut state = start;
    let mut i = 0;
    while i < 16 {
        let (s, r) = splitmix64(state);
        state = s;
        t[i] = r;
        i += 1;
    }
    (t, state)
}

const fn fill_8(start: u64) -> ([u64; 8], u64) {
    let mut t = [0u64; 8];
    let mut state = start;
    let mut i = 0;
    while i < 8 {
        let (s, r) = splitmix64(state);
        state = s;
        t[i] = r;
        i += 1;
    }
    (t, state)
}

const PIECE: [[[u64; 64]; 7]; 2] = fill_piece();
const CASTLE_AND_STATE: ([u64; 16], u64) = fill_16(SEED ^ 0xC15A_D15A);
const CASTLE: [u64; 16] = CASTLE_AND_STATE.0;
const EP_AND_STATE: ([u64; 8], u64) = fill_8(CASTLE_AND_STATE.1);
const EP: [u64; 8] = EP_AND_STATE.0;
const SIDE: u64 = {
    let (_, r) = splitmix64(EP_AND_STATE.1 ^ 0x51DE);
    r
};

#[inline]
pub fn piece_key(color: Color, pt: u8, sq: u8) -> u64 {
    PIECE[color.idx()][pt as usize][sq as usize]
}

#[inline]
pub fn castle_key(rights: u8) -> u64 {
    CASTLE[(rights & 15) as usize]
}

#[inline]
pub fn ep_key(file: u8) -> u64 {
    EP[(file & 7) as usize]
}

#[inline]
pub fn side_key() -> u64 {
    SIDE
}

pub fn hash_pos(pos: &Position) -> u64 {
    let mut h = 0u64;
    for sq in 0..64u8 {
        let p = pos.squares[sq as usize];
        if p != 0 {
            h ^= piece_key(color_of(p), type_of(p), sq);
        }
    }
    h ^= castle_key(pos.castling);
    if let Some(ep) = pos.ep {
        h ^= ep_key(ep & 7);
    }
    if pos.side == Color::Black {
        h ^= SIDE;
    }
    h
}
