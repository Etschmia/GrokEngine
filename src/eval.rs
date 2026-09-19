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
//!
//! Mobility: knights, bishops, rooks and queens score a few centipawns per
//! reachable square (empty or opponent). The weights are small so the piece-
//! square tables still decide *where* a piece likes to sit; mobility decides
//! whether it can actually use that square. Bishops get more in the endgame
//! (open diagonals), rooks too (open ranks). Queens are capped at 1 so a
//! 27-square queen does not drown the rest of the eval.
//!
//! King safety (middlegame only): after the king has gone to a wing file
//! (a–c or f–h), the three files in front of it should still hold a friendly
//! pawn on the next one or two ranks. A missing shield pawn is a penalty;
//! a pawn pushed three ranks ahead is a smaller one. On top of that, an
//! enemy queen or knight closer than Chebyshev 7 is a tropism bonus for the
//! attacker (queen more than knight, because a close queen is a mating net).
//! Uncastled kings on d/e are left to the king PST — otherwise 1.e4 would
//! look like a self-inflicted hole.
//!
//! Rooks: a file with no friendly pawn is semi-open; no pawns at all is
//! open. Two rooks on the same open file get a small extra (they double).

use crate::board::{
    BISHOP, BISHOP_D, Color, KNIGHT, KNIGHT_D, PAWN, Position, QUEEN, ROOK, ROOK_D, color_of, dest,
    type_of,
};

pub const VAL: [i32; 7] = [0, 100, 320, 330, 500, 900, 0];

const MAT_MG: [i32; 7] = [0, 100, 320, 330, 500, 900, 0];
const MAT_EG: [i32; 7] = [0, 120, 300, 340, 550, 920, 0];
const PHASE: [i32; 7] = [0, 0, 1, 1, 2, 4, 0];

// a1 = index 0. Generated from the rules in the module docs.
#[rustfmt::skip]
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

#[rustfmt::skip]
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

/// Centipawns per attack square. Index is piece type.
const MOB_MG: [i32; 7] = [0, 0, 2, 3, 2, 1, 0];
const MOB_EG: [i32; 7] = [0, 0, 2, 4, 3, 1, 0];

const OPEN_FILE_MG: i32 = 16;
const OPEN_FILE_EG: i32 = 12;
const SEMI_OPEN_MG: i32 = 8;
const SEMI_OPEN_EG: i32 = 4;
const DOUBLED_ROOK_MG: i32 = 8;
const DOUBLED_ROOK_EG: i32 = 8;

const SHIELD_MISSING: i32 = 14;
const SHIELD_PUSHED: i32 = 6;
const QUEEN_TROPISM: i32 = 3;
const KNIGHT_TROPISM: i32 = 2;

/// Score in centipawns from the side to move's point of view.
pub fn evaluate(pos: &Position) -> i32 {
    let mut mg = 0;
    let mut eg = 0;
    let mut phase = 0;
    let mut bishops = [0i32; 2];
    let mut pawn_files = [[0u8; 8]; 2];
    let mut pawn_sq = [[0u8; 8]; 2];
    let mut pawn_n = [0usize; 2];
    let mut rook_sq = [[0u8; 2]; 2];
    let mut rook_n = [0usize; 2];

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
        if pt == ROOK as usize && rook_n[c.idx()] < 2 {
            let i = rook_n[c.idx()];
            rook_sq[c.idx()][i] = sq as u8;
            rook_n[c.idx()] += 1;
        }

        if pt == KNIGHT as usize
            || pt == BISHOP as usize
            || pt == ROOK as usize
            || pt == QUEEN as usize
        {
            let mob = mobility(pos, sq as u8, pt as u8, c);
            mg += sign * MOB_MG[pt] * mob;
            eg += sign * MOB_EG[pt] * mob;
        }
        if pt == QUEEN as usize || pt == KNIGHT as usize {
            let ek = pos.king[c.flip().idx()] as usize;
            let d = chebyshev(sq, ek);
            let w = if pt == QUEEN as usize {
                QUEEN_TROPISM
            } else {
                KNIGHT_TROPISM
            };
            mg += sign * w * (7 - d);
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

    let (wr_mg, wr_eg) = rook_files(&rook_sq[0], rook_n[0], &pawn_files[0], &pawn_files[1]);
    let (br_mg, br_eg) = rook_files(&rook_sq[1], rook_n[1], &pawn_files[1], &pawn_files[0]);
    mg += wr_mg - br_mg;
    eg += wr_eg - br_eg;

    mg -= pawn_shield(pos.king[0] as usize, &pawn_sq[0], pawn_n[0], true);
    mg += pawn_shield(pos.king[1] as usize, &pawn_sq[1], pawn_n[1], false);

    let phase = phase.min(24);
    let score = (mg * phase + eg * (24 - phase)) / 24;
    if pos.side == Color::White {
        score
    } else {
        -score
    }
}

fn mobility(pos: &Position, sq: u8, pt: u8, us: Color) -> i32 {
    match pt {
        KNIGHT => leaper_mob(pos, sq, &KNIGHT_D, us),
        BISHOP => slider_mob(pos, sq, &BISHOP_D, us),
        ROOK => slider_mob(pos, sq, &ROOK_D, us),
        QUEEN => slider_mob(pos, sq, &BISHOP_D, us) + slider_mob(pos, sq, &ROOK_D, us),
        _ => 0,
    }
}

fn leaper_mob(pos: &Position, sq: u8, dirs: &[(i8, i8)], us: Color) -> i32 {
    let mut n = 0;
    for &(df, dr) in dirs {
        if let Some(s) = dest(sq, df, dr) {
            let p = pos.squares[s as usize];
            if p == 0 || color_of(p) != us {
                n += 1;
            }
        }
    }
    n
}

fn slider_mob(pos: &Position, sq: u8, dirs: &[(i8, i8)], us: Color) -> i32 {
    let mut n = 0;
    for &(df, dr) in dirs {
        let mut f = df;
        let mut r = dr;
        while let Some(s) = dest(sq, f, r) {
            let p = pos.squares[s as usize];
            if p == 0 {
                n += 1;
            } else {
                if color_of(p) != us {
                    n += 1;
                }
                break;
            }
            f += df;
            r += dr;
        }
    }
    n
}

fn chebyshev(a: usize, b: usize) -> i32 {
    let df = ((a as i32) & 7) - ((b as i32) & 7);
    let dr = ((a as i32) >> 3) - ((b as i32) >> 3);
    df.abs().max(dr.abs())
}

fn rook_files(rooks: &[u8; 2], n: usize, ours: &[u8; 8], theirs: &[u8; 8]) -> (i32, i32) {
    let mut mg = 0;
    let mut eg = 0;
    let mut open_seen = [false; 8];
    for i in 0..n {
        let f = (rooks[i] as usize) & 7;
        if ours[f] != 0 {
            continue;
        }
        if theirs[f] == 0 {
            mg += OPEN_FILE_MG;
            eg += OPEN_FILE_EG;
            if open_seen[f] {
                mg += DOUBLED_ROOK_MG;
                eg += DOUBLED_ROOK_EG;
            }
            open_seen[f] = true;
        } else {
            mg += SEMI_OPEN_MG;
            eg += SEMI_OPEN_EG;
        }
    }
    (mg, eg)
}

/// Middlegame penalty for a wing king whose shield pawns are gone or pushed.
fn pawn_shield(king: usize, pawns: &[u8; 8], n: usize, white: bool) -> i32 {
    let kf = king & 7;
    let kr = king >> 3;
    if kf == 3 || kf == 4 {
        return 0;
    }
    let lo = if kf == 0 { 0 } else { kf - 1 };
    let hi = if kf == 7 { 7 } else { kf + 1 };
    let mut penalty = 0;
    for f in lo..=hi {
        let mut dist = 99;
        for i in 0..n {
            let sq = pawns[i] as usize;
            if sq & 7 != f {
                continue;
            }
            let r = sq >> 3;
            if white {
                if r > kr {
                    dist = dist.min(r - kr);
                }
            } else if r < kr {
                dist = dist.min(kr - r);
            }
        }
        if dist == 1 || dist == 2 {
            continue;
        }
        if dist == 3 {
            penalty += SHIELD_PUSHED;
        } else {
            penalty += SHIELD_MISSING;
        }
    }
    penalty
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

    #[test]
    fn rook_on_semi_open_file_beats_blocked_file() {
        // Same material: white rook on a, one white pawn, one black pawn.
        // Semi-open: white's pawn is not on a. Closed: it is.
        let semi = Position::from_fen("4k3/p7/8/8/8/8/7P/R3K3 w - - 0 1").unwrap();
        let closed = Position::from_fen("4k3/7p/8/8/8/8/P7/R3K3 w - - 0 1").unwrap();
        assert!(
            evaluate(&semi) > evaluate(&closed),
            "semi {} closed {}",
            evaluate(&semi),
            evaluate(&closed)
        );
    }

    #[test]
    fn open_bishop_beats_blocked_bishop() {
        // Same material. Bishop on h1 has a long diagonal; bishop on f1 is
        // boxed in by the e-pawns.
        let open = Position::from_fen("4k3/4p3/8/8/8/8/4P3/4K2B w - - 0 1").unwrap();
        let blocked = Position::from_fen("4k3/4p3/8/8/8/8/4P3/4KB2 w - - 0 1").unwrap();
        assert!(
            evaluate(&open) > evaluate(&blocked),
            "open {} blocked {}",
            evaluate(&open),
            evaluate(&blocked)
        );
    }

    #[test]
    fn pawn_shield_beats_exposed_wing_king() {
        // Same material, middlegame (queens keep the phase above 0). White's
        // king sits on g1 behind f/g/h pawns, or on b1 with those pawns still
        // on the other wing. King PST is the same on b1 and g1.
        let safe = Position::from_fen("3q2k1/5ppp/8/8/8/8/5PPP/3Q2K1 w - - 0 1").unwrap();
        let exposed = Position::from_fen("3q2k1/5ppp/8/8/8/8/5PPP/1K1Q4 w - - 0 1").unwrap();
        assert!(
            evaluate(&safe) > evaluate(&exposed),
            "safe {} exposed {}",
            evaluate(&safe),
            evaluate(&exposed)
        );
    }
}
