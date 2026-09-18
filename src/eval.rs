//! Material plus original piece-square tables, tapered by remaining material.

use crate::board::{color_of, type_of, BISHOP, Color, PAWN, Position};

const MAT_MG: [i32; 7] = [0, 82, 337, 365, 477, 1025, 0];
const MAT_EG: [i32; 7] = [0, 94, 281, 297, 512, 936, 0];
const PHASE: [i32; 7] = [0, 0, 1, 1, 2, 4, 0];

// White's perspective, a1 = index 0. Values are my own, not copied from another engine.
const PST_MG: [[i32; 64]; 7] = [
    [0; 64],
    // pawn
    [
         0,  0,  0,  0,  0,  0,  0,  0,
        50, 50, 50, 50, 50, 50, 50, 50,
        10, 10, 20, 30, 30, 20, 10, 10,
         5,  5, 10, 25, 25, 10,  5,  5,
         0,  0,  0, 20, 20,  0,  0,  0,
         5, -5,-10,  0,  0,-10, -5,  5,
         5, 10, 10,-20,-20, 10, 10,  5,
         0,  0,  0,  0,  0,  0,  0,  0,
    ],
    // knight
    [
       -50,-40,-30,-30,-30,-30,-40,-50,
       -40,-20,  0,  0,  0,  0,-20,-40,
       -30,  0, 10, 15, 15, 10,  0,-30,
       -30,  5, 15, 20, 20, 15,  5,-30,
       -30,  0, 15, 20, 20, 15,  0,-30,
       -30,  5, 10, 15, 15, 10,  5,-30,
       -40,-20,  0,  5,  5,  0,-20,-40,
       -50,-40,-30,-30,-30,-30,-40,-50,
    ],
    // bishop
    [
       -20,-10,-10,-10,-10,-10,-10,-20,
       -10,  0,  0,  0,  0,  0,  0,-10,
       -10,  0,  5, 10, 10,  5,  0,-10,
       -10,  5,  5, 10, 10,  5,  5,-10,
       -10,  0, 10, 10, 10, 10,  0,-10,
       -10, 10, 10, 10, 10, 10, 10,-10,
       -10,  5,  0,  0,  0,  0,  5,-10,
       -20,-10,-10,-10,-10,-10,-10,-20,
    ],
    // rook
    [
         0,  0,  0,  0,  0,  0,  0,  0,
         5, 10, 10, 10, 10, 10, 10,  5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
         0,  0,  0,  5,  5,  0,  0,  0,
    ],
    // queen
    [
       -20,-10,-10, -5, -5,-10,-10,-20,
       -10,  0,  0,  0,  0,  0,  0,-10,
       -10,  0,  5,  5,  5,  5,  0,-10,
        -5,  0,  5,  5,  5,  5,  0, -5,
         0,  0,  5,  5,  5,  5,  0, -5,
       -10,  5,  5,  5,  5,  5,  0,-10,
       -10,  0,  5,  0,  0,  0,  0,-10,
       -20,-10,-10, -5, -5,-10,-10,-20,
    ],
    // king
    [
       -30,-40,-40,-50,-50,-40,-40,-30,
       -30,-40,-40,-50,-50,-40,-40,-30,
       -30,-40,-40,-50,-50,-40,-40,-30,
       -30,-40,-40,-50,-50,-40,-40,-30,
       -20,-30,-30,-40,-40,-30,-30,-20,
       -10,-20,-20,-20,-20,-20,-20,-10,
        20, 20,  0,  0,  0,  0, 20, 20,
        20, 30, 10,  0,  0, 10, 30, 20,
    ],
];

const PST_EG: [[i32; 64]; 7] = [
    [0; 64],
    // pawn
    [
         0,  0,  0,  0,  0,  0,  0,  0,
        80, 80, 80, 80, 80, 80, 80, 80,
        40, 40, 40, 40, 40, 40, 40, 40,
        20, 20, 20, 20, 20, 20, 20, 20,
        10, 10, 10, 10, 10, 10, 10, 10,
         5,  5,  5,  5,  5,  5,  5,  5,
         0,  0,  0,  0,  0,  0,  0,  0,
         0,  0,  0,  0,  0,  0,  0,  0,
    ],
    // knight
    [
       -50,-40,-30,-30,-30,-30,-40,-50,
       -40,-20,  0,  0,  0,  0,-20,-40,
       -30,  0, 10, 15, 15, 10,  0,-30,
       -30,  5, 15, 20, 20, 15,  5,-30,
       -30,  0, 15, 20, 20, 15,  0,-30,
       -30,  5, 10, 15, 15, 10,  5,-30,
       -40,-20,  0,  0,  0,  0,-20,-40,
       -50,-40,-30,-30,-30,-30,-40,-50,
    ],
    // bishop
    [
       -20,-10,-10,-10,-10,-10,-10,-20,
       -10,  0,  0,  0,  0,  0,  0,-10,
       -10,  0,  5, 10, 10,  5,  0,-10,
       -10,  5,  5, 10, 10,  5,  5,-10,
       -10,  0, 10, 10, 10, 10,  0,-10,
       -10, 10, 10, 10, 10, 10, 10,-10,
       -10,  5,  0,  0,  0,  0,  5,-10,
       -20,-10,-10,-10,-10,-10,-10,-20,
    ],
    // rook
    [
         0,  0,  0,  0,  0,  0,  0,  0,
         5, 10, 10, 10, 10, 10, 10,  5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
         0,  0,  0,  0,  0,  0,  0,  0,
    ],
    // queen
    [
       -20,-10,-10, -5, -5,-10,-10,-20,
       -10,  0,  0,  0,  0,  0,  0,-10,
       -10,  0,  5,  5,  5,  5,  0,-10,
        -5,  0,  5,  5,  5,  5,  0, -5,
         0,  0,  5,  5,  5,  5,  0, -5,
       -10,  5,  5,  5,  5,  5,  0,-10,
       -10,  0,  5,  0,  0,  0,  0,-10,
       -20,-10,-10, -5, -5,-10,-10,-20,
    ],
    // king
    [
       -50,-40,-30,-20,-20,-30,-40,-50,
       -30,-20,-10,  0,  0,-10,-20,-30,
       -30,-10, 20, 30, 30, 20,-10,-30,
       -30,-10, 30, 40, 40, 30,-10,-30,
       -30,-10, 30, 40, 40, 30,-10,-30,
       -30,-10, 20, 30, 30, 20,-10,-30,
       -30,-30,  0,  0,  0,  0,-30,-30,
       -50,-30,-30,-30,-30,-30,-30,-50,
    ],
];

/// Tables above are written rank-8-first (a8 = 0). Convert an a1=0 square.
#[inline]
fn pst_index(white_sq: usize) -> usize {
    let file = white_sq & 7;
    let rank = white_sq >> 3;
    (7 - rank) * 8 + file
}

/// Score in centipawns from the side to move's point of view.
pub fn evaluate(pos: &Position) -> i32 {
    let mut mg = 0;
    let mut eg = 0;
    let mut phase = 0;
    let mut bishops = [0i32; 2];

    for sq in 0..64 {
        let p = pos.squares[sq];
        if p == 0 {
            continue;
        }
        let pt = type_of(p) as usize;
        let c = color_of(p);
        let white_sq = if c == Color::White { sq } else { sq ^ 56 };
        let idx = pst_index(white_sq);
        let sign = if c == Color::White { 1 } else { -1 };
        mg += sign * (MAT_MG[pt] + PST_MG[pt][idx]);
        eg += sign * (MAT_EG[pt] + PST_EG[pt][idx]);
        phase += PHASE[pt];
        if pt == BISHOP as usize {
            bishops[c.idx()] += 1;
        }
    }

    if bishops[Color::White.idx()] >= 2 {
        mg += 25;
        eg += 40;
    }
    if bishops[Color::Black.idx()] >= 2 {
        mg -= 25;
        eg -= 40;
    }

    mg += pawn_structure(pos, Color::White) - pawn_structure(pos, Color::Black);

    let phase = phase.min(24);
    let score = (mg * phase + eg * (24 - phase)) / 24;
    if pos.side == Color::White {
        score
    } else {
        -score
    }
}

fn pawn_structure(pos: &Position, color: Color) -> i32 {
    let pawn = if color == Color::White { PAWN } else { PAWN | 8 };
    let mut files = [0u8; 8];
    let mut bonus = 0;
    for sq in 0..64 {
        if pos.squares[sq] == pawn {
            files[(sq & 7) as usize] += 1;
        }
    }
    for f in 0..8 {
        if files[f] > 1 {
            bonus -= 12 * (files[f] as i32 - 1);
        }
        if files[f] > 0 {
            let isolated = (f == 0 || files[f - 1] == 0) && (f == 7 || files[f + 1] == 0);
            if isolated {
                bonus -= 8;
            }
        }
    }
    bonus
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
        let pos =
            Position::from_fen("4k3/8/8/8/8/8/8/3QK3 w - - 0 1").unwrap();
        assert!(evaluate(&pos) > 700);
    }
}
