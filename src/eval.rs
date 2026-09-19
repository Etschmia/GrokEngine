//! Tapered evaluation derived from first principles, not from another engine.
//!
//! Material (centipawns). The pawn is the unit (100). Other values follow the
//! classical exchange scale, then a small middlegame/endgame split:
//! - Knight ≈ 3 pawns, slightly less in the endgame (cannot stop distant pawns).
//! - Bishop a touch above a knight; the pair is a separate bonus.
//! - Rook ≈ 5 pawns, more in the endgame when files open.
//! - Queen a little under two rooks, a little over two minors plus two pawns.
//!
//! Piece-square tables (a1 = 0, white's view; black uses sq ^ 56) come from
//! geometric rules, not from published tables:
//! - Pawns: bonus for advancing and occupying central files; d2/e2 penalised
//!   so they do not sit in front of the pieces; a/h files slightly worse.
//!   Endgame bonus grows sharply with rank (passed-pawn race).
//! - Knights: prefer the centre, hate the rim and corners.
//! - Bishops: modest centralisation, extra for the long diagonals and the
//!   fianchetto squares b2/g2.
//! - Rooks: 7th rank, then 6th; central files; a/h only a small middlegame hit.
//! - Queens: light centralisation, no early-outing penalty beyond the table.
//! - King: middlegame wants b1/c1/g1 (castled); endgame wants the centre.
//!
//! Structure: doubled and isolated pawns are penalised; passed pawns are
//! rewarded by rank. Bishop pair is a bonus for covering both colours.

use crate::board::{color_of, type_of, BISHOP, Color, PAWN, Position};

pub const VAL: [i32; 7] = [0, 100, 320, 330, 500, 900, 0];

const MAT_MG: [i32; 7] = [0, 100, 320, 330, 500, 900, 0];
const MAT_EG: [i32; 7] = [0, 120, 300, 340, 550, 920, 0];
const PHASE: [i32; 7] = [0, 0, 1, 1, 2, 4, 0];

// a1 = index 0. Generated from the rules in the module docs.
const PST_MG: [[i32; 64]; 7] = [
    [0; 64],
    // pawn
    [
         0,   0,   0,   0,   0,   0,   0,   0,
        -4,   2,   4,  -6,  -6,   4,   2,  -4,
         0,   6,   8,  10,  10,   8,   6,   0,
         6,  14,  18,  22,  22,  18,  14,   6,
        14,  22,  26,  30,  30,  26,  22,  14,
        24,  32,  36,  40,  40,  36,  32,  24,
        41,  49,  53,  57,  57,  53,  49,  41,
         0,   0,   0,   0,   0,   0,   0,   0,
    ],
    // knight
    [
       -44, -26, -20, -14, -14, -20, -26, -44,
       -26, -12,  -6,   0,   0,  -6, -12, -26,
       -20,  -6,   0,   6,   6,   0,  -6, -20,
       -14,   0,   6,  12,  12,   6,   0, -14,
       -14,   0,   6,  12,  12,   6,   0, -14,
       -20,  -6,   0,   6,   6,   0,  -6, -20,
       -26, -12,  -6,   0,   0,  -6, -12, -26,
       -44, -26, -20, -14, -14, -20, -26, -44,
    ],
    // bishop
    [
        -3,  -4,  -1,   1,   1,  -1,  -4,  -3,
        -5,   8,   0,   3,   3,   0,   8,  -5,
        -3,   0,   6,   5,   5,   6,   0,  -3,
        -1,   1,   4,  11,  11,   4,   1,  -1,
        -1,   1,   4,  11,  11,   4,   1,  -1,
        -3,   0,   6,   5,   5,   6,   0,  -3,
        -5,   2,   0,   3,   3,   0,   2,  -5,
        -3,  -4,  -1,   1,   1,  -1,  -4,  -3,
    ],
    // rook
    [
        -2,   0,   0,   4,   4,   0,   0,  -2,
        -2,   0,   0,   4,   4,   0,   0,  -2,
        -2,   0,   0,   4,   4,   0,   0,  -2,
        -2,   0,   0,   4,   4,   0,   0,  -2,
        -2,   0,   0,   4,   4,   0,   0,  -2,
         4,   6,   6,  10,  10,   6,   6,   4,
        14,  16,  16,  20,  20,  16,  16,  14,
        -2,   0,   0,   4,   4,   0,   0,  -2,
    ],
    // queen
    [
        -6,  -4,  -2,   0,   0,  -2,  -4,  -6,
        -4,  -2,   0,   2,   2,   0,  -2,  -4,
        -2,   0,   2,   4,   4,   2,   0,  -2,
         0,   2,   4,   6,   6,   4,   2,   0,
         0,   2,   4,   6,   6,   4,   2,   0,
        -2,   0,   2,   4,   4,   2,   0,  -2,
        -4,  -2,   0,   2,   2,   0,  -2,  -4,
        -6,  -4,  -2,   0,   0,  -2,  -4,  -6,
    ],
    // king
    [
        18,  32,  22,   0,   0,  18,  32,  18,
        12,  16,   4, -12, -12,   4,  16,  12,
       -20, -24, -28, -32, -32, -28, -24, -20,
       -30, -34, -38, -42, -42, -38, -34, -30,
       -40, -44, -48, -52, -52, -48, -44, -40,
       -50, -54, -58, -62, -62, -58, -54, -50,
       -60, -64, -68, -72, -72, -68, -64, -60,
       -70, -74, -78, -82, -82, -78, -74, -70,
    ],
];

const PST_EG: [[i32; 64]; 7] = [
    [0; 64],
    // pawn: rank is almost everything
    [
         0,   0,   0,   0,   0,   0,   0,   0,
         0,   2,   4,   6,   6,   4,   2,   0,
         6,   8,  10,  12,  12,  10,   8,   6,
        14,  16,  18,  20,  20,  18,  16,  14,
        28,  30,  32,  34,  34,  32,  30,  28,
        48,  50,  52,  54,  54,  52,  50,  48,
        80,  82,  84,  86,  86,  84,  82,  80,
         0,   0,   0,   0,   0,   0,   0,   0,
    ],
    // knight: same geometry, slightly flatter
    [
       -36, -22, -16, -10, -10, -16, -22, -36,
       -22, -10,  -4,   0,   0,  -4, -10, -22,
       -16,  -4,   2,   6,   6,   2,  -4, -16,
       -10,   0,   6,  10,  10,   6,   0, -10,
       -10,   0,   6,  10,  10,   6,   0, -10,
       -16,  -4,   2,   6,   6,   2,  -4, -16,
       -22, -10,  -4,   0,   0,  -4, -10, -22,
       -36, -22, -16, -10, -10, -16, -22, -36,
    ],
    // bishop
    [
        -4,  -2,  -2,   0,   0,  -2,  -2,  -4,
        -2,   4,   2,   4,   4,   2,   4,  -2,
        -2,   2,   8,   6,   6,   8,   2,  -2,
         0,   4,   6,  10,  10,   6,   4,   0,
         0,   4,   6,  10,  10,   6,   4,   0,
        -2,   2,   8,   6,   6,   8,   2,  -2,
        -2,   4,   2,   4,   4,   2,   4,  -2,
        -4,  -2,  -2,   0,   0,  -2,  -2,  -4,
    ],
    // rook
    [
         0,   0,   0,   4,   4,   0,   0,   0,
         0,   0,   0,   4,   4,   0,   0,   0,
         0,   0,   0,   4,   4,   0,   0,   0,
         0,   0,   0,   4,   4,   0,   0,   0,
         0,   0,   0,   4,   4,   0,   0,   0,
         6,   6,   6,  10,  10,   6,   6,   6,
        16,  16,  16,  20,  20,  16,  16,  16,
         0,   0,   0,   4,   4,   0,   0,   0,
    ],
    // queen
    [
        -6,  -4,  -2,   0,   0,  -2,  -4,  -6,
        -4,  -2,   0,   2,   2,   0,  -2,  -4,
        -2,   0,   2,   4,   4,   2,   0,  -2,
         0,   2,   4,   6,   6,   4,   2,   0,
         0,   2,   4,   6,   6,   4,   2,   0,
        -2,   0,   2,   4,   4,   2,   0,  -2,
        -4,  -2,   0,   2,   2,   0,  -2,  -4,
        -6,  -4,  -2,   0,   0,  -2,  -4,  -6,
    ],
    // king: centralise
    [
       -32, -24, -16,  -8,  -8, -16, -24, -32,
       -24, -16,  -8,   0,   0,  -8, -16, -24,
       -16,  -8,   0,   8,   8,   0,  -8, -16,
        -8,   0,   8,  16,  16,   8,   0,  -8,
        -8,   0,   8,  16,  16,   8,   0,  -8,
       -16,  -8,   0,   8,   8,   0,  -8, -16,
       -24, -16,  -8,   0,   0,  -8, -16, -24,
       -32, -24, -16,  -8,  -8, -16, -24, -32,
    ],
];

const PASSED_MG: [i32; 8] = [0, 0, 4, 8, 16, 28, 42, 0];
const PASSED_EG: [i32; 8] = [0, 4, 10, 20, 36, 60, 100, 0];

/// Score in centipawns from the side to move's point of view.
pub fn evaluate(pos: &Position) -> i32 {
    let mut mg = 0;
    let mut eg = 0;
    let mut phase = 0;
    let mut bishops = [0i32; 2];
    let mut pawn_files = [[0u8; 8]; 2];
    let mut pawn_sq = [[0u8; 8]; 2];
    let mut pawn_n = [0usize; 2];

    for sq in 0..64 {
        let p = pos.squares[sq];
        if p == 0 {
            continue;
        }
        let pt = type_of(p) as usize;
        let c = color_of(p);
        let white_sq = if c == Color::White { sq } else { sq ^ 56 };
        let sign = if c == Color::White { 1 } else { -1 };
        mg += sign * (MAT_MG[pt] + PST_MG[pt][white_sq]);
        eg += sign * (MAT_EG[pt] + PST_EG[pt][white_sq]);
        phase += PHASE[pt];
        if pt == BISHOP as usize {
            bishops[c.idx()] += 1;
        }
        if pt == PAWN as usize && pawn_n[c.idx()] < 8 {
            let i = pawn_n[c.idx()];
            pawn_sq[c.idx()][i] = sq as u8;
            pawn_n[c.idx()] += 1;
            pawn_files[c.idx()][sq & 7] += 1;
        }
    }

    if bishops[Color::White.idx()] >= 2 {
        mg += 30;
        eg += 50;
    }
    if bishops[Color::Black.idx()] >= 2 {
        mg -= 30;
        eg -= 50;
    }

    let (w_mg, w_eg) = pawn_terms(
        &pawn_files[0],
        &pawn_sq[0],
        pawn_n[0],
        &pawn_sq[1],
        pawn_n[1],
        true,
    );
    let (b_mg, b_eg) = pawn_terms(
        &pawn_files[1],
        &pawn_sq[1],
        pawn_n[1],
        &pawn_sq[0],
        pawn_n[0],
        false,
    );
    mg += w_mg - b_mg;
    eg += w_eg - b_eg;

    let phase = phase.min(24);
    let score = (mg * phase + eg * (24 - phase)) / 24;
    if pos.side == Color::White {
        score
    } else {
        -score
    }
}

fn pawn_terms(
    ours: &[u8; 8],
    sqs: &[u8; 8],
    n: usize,
    opp_sqs: &[u8; 8],
    opp_n: usize,
    white: bool,
) -> (i32, i32) {
    let mut mg = 0;
    let mut eg = 0;
    for f in 0..8 {
        if ours[f] > 1 {
            let extra = ours[f] as i32 - 1;
            mg -= 15 * extra;
            eg -= 25 * extra;
        }
        if ours[f] > 0 {
            let isolated = (f == 0 || ours[f - 1] == 0) && (f == 7 || ours[f + 1] == 0);
            if isolated {
                mg -= 12;
                eg -= 20;
            }
        }
    }
    for i in 0..n {
        let sq = sqs[i] as usize;
        let file = sq & 7;
        let rank = sq >> 3;
        if is_passed(file, rank, opp_sqs, opp_n, white) {
            let r = if white { rank } else { 7 - rank };
            mg += PASSED_MG[r];
            eg += PASSED_EG[r];
        }
    }
    (mg, eg)
}

fn is_passed(file: usize, rank: usize, opp_sqs: &[u8; 8], opp_n: usize, white: bool) -> bool {
    for i in 0..opp_n {
        let sq = opp_sqs[i] as usize;
        let f = sq & 7;
        let r = sq >> 3;
        if f.abs_diff(file) > 1 {
            continue;
        }
        if white {
            if r > rank {
                return false;
            }
        } else if r < rank {
            return false;
        }
    }
    true
}

/// Material of a piece type, for capture ordering / delta pruning.
#[inline]
pub fn piece_val(pt: u8) -> i32 {
    VAL[pt as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Position;

    #[test]
    fn startpos_eval_is_near_zero() {
        let pos = Position::startpos();
        let s = evaluate(&pos);
        assert!(s.abs() < 50, "startpos eval {s}");
    }

    #[test]
    fn extra_queen_is_winning() {
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/3QK3 w - - 0 1").unwrap();
        assert!(evaluate(&pos) > 700);
    }

    #[test]
    fn passed_pawn_beats_blocked_pawn() {
        let passed = Position::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        let blocked = Position::from_fen("4k3/4p3/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        assert!(evaluate(&passed) > evaluate(&blocked));
    }
}
