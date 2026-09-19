//! Single-threaded transposition table. One cluster entry per slot; always-replace.

use crate::board::Move;

pub const TT_EXACT: u8 = 0;
pub const TT_ALPHA: u8 = 1; // upper bound
pub const TT_BETA: u8 = 2; // lower bound

#[derive(Clone, Copy)]
pub struct TTEntry {
    pub key: u64,
    pub score: i16,
    pub depth: i8,
    pub flag: u8,
    pub from: u8,
    pub to: u8,
    pub promo: u8,
    pub flags: u8,
}

impl TTEntry {
    pub fn mv(self) -> Option<Move> {
        if self.from == 0 && self.to == 0 && self.promo == 0 && self.flags == 0 {
            return None;
        }
        Some(Move {
            from: self.from,
            to: self.to,
            promo: self.promo,
            flags: self.flags,
        })
    }
}

pub struct TranspositionTable {
    entries: Vec<TTEntry>,
    mask: usize,
}

impl TranspositionTable {
    pub fn with_mb(mb: usize) -> Self {
        let bytes = mb.max(1).saturating_mul(1024 * 1024);
        let size = (bytes / std::mem::size_of::<TTEntry>()).next_power_of_two() / 2;
        let size = size.max(1024);
        Self {
            entries: vec![
                TTEntry {
                    key: 0,
                    score: 0,
                    depth: -1,
                    flag: 0,
                    from: 0,
                    to: 0,
                    promo: 0,
                    flags: 0,
                };
                size
            ],
            mask: size - 1,
        }
    }

    pub fn clear(&mut self) {
        for e in &mut self.entries {
            e.key = 0;
            e.depth = -1;
            e.from = 0;
            e.to = 0;
            e.promo = 0;
            e.flags = 0;
        }
    }

    #[inline]
    pub fn probe(&self, key: u64) -> Option<TTEntry> {
        let e = self.entries[key as usize & self.mask];
        if e.key == key && e.depth >= 0 {
            Some(e)
        } else {
            None
        }
    }

    #[inline]
    pub fn store(&mut self, key: u64, depth: i32, score: i32, flag: u8, mv: Option<Move>) {
        let slot = key as usize & self.mask;
        let e = &mut self.entries[slot];
        // Keep a deeper entry unless the new one is at least as deep.
        if e.key == key && e.depth as i32 > depth {
            return;
        }
        let (from, to, promo, flags) = match mv {
            Some(m) => (m.from, m.to, m.promo, m.flags),
            None => (0, 0, 0, 0),
        };
        *e = TTEntry {
            key,
            score: score.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            depth: depth.clamp(0, 127) as i8,
            flag,
            from,
            to,
            promo,
            flags,
        };
    }
}
