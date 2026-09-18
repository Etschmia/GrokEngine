//! Iterative-deepening alpha-beta with quiescence and a time bound.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::board::{type_of, Color, Move, Position, PAWN, QUEEN};
use crate::eval::evaluate;

pub const MATE: i32 = 30_000;
const INF: i32 = 32_000;
const MAX_PLY: usize = 64;

#[derive(Clone, Debug)]
pub struct SearchLimits {
    pub depth: Option<i32>,
    pub movetime_ms: Option<u64>,
    pub wtime_ms: Option<u64>,
    pub btime_ms: Option<u64>,
    pub winc_ms: Option<u64>,
    pub binc_ms: Option<u64>,
    pub infinite: bool,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            depth: None,
            movetime_ms: None,
            wtime_ms: None,
            btime_ms: None,
            winc_ms: None,
            binc_ms: None,
            infinite: false,
        }
    }
}

impl SearchLimits {
    pub fn depth(d: i32) -> Self {
        Self {
            depth: Some(d),
            ..Self::default()
        }
    }
}

struct Ctx<'a> {
    stop: &'a AtomicBool,
    start: Instant,
    budget: Duration,
    nodes: u64,
    abort: bool,
    killers: [[Option<Move>; 2]; MAX_PLY],
}

impl Ctx<'_> {
    fn timed_out(&self) -> bool {
        if self.stop.load(Ordering::Relaxed) {
            return true;
        }
        self.start.elapsed() >= self.budget
    }

    fn check_abort(&mut self) {
        if self.nodes & 63 == 0 && self.timed_out() {
            self.abort = true;
        }
    }
}

fn time_budget(pos: &Position, limits: &SearchLimits) -> Duration {
    if limits.infinite && limits.movetime_ms.is_none() && limits.wtime_ms.is_none() && limits.btime_ms.is_none()
    {
        return Duration::from_secs(60 * 60);
    }
    if let Some(mt) = limits.movetime_ms {
        return Duration::from_millis(mt.saturating_sub(8).max(1));
    }
    let (remain, inc) = if pos.side == Color::White {
        (
            limits.wtime_ms.unwrap_or(0),
            limits.winc_ms.unwrap_or(0),
        )
    } else {
        (
            limits.btime_ms.unwrap_or(0),
            limits.binc_ms.unwrap_or(0),
        )
    };
    if remain == 0 {
        if limits.depth.is_some() {
            return Duration::from_secs(60 * 60);
        }
        return Duration::from_millis(1000);
    }
    let alloc = remain / 30 + inc.saturating_mul(4) / 5;
    Duration::from_millis(alloc.min(remain.saturating_sub(25)).max(5))
}

fn mvv_lva(pos: &Position, m: Move) -> i32 {
    let victim = if m.is_ep() {
        PAWN
    } else {
        type_of(pos.piece_at(m.to))
    };
    let attacker = type_of(pos.piece_at(m.from));
    let mut s = victim as i32 * 16 - attacker as i32;
    if m.promo != 0 {
        s += m.promo as i32 * 32;
    }
    s
}

fn order_key(pos: &Position, m: Move, ply: usize, ctx: &Ctx) -> i32 {
    if pos.is_capture(m) {
        1_000_000 + mvv_lva(pos, m)
    } else if ctx.killers[ply][0] == Some(m) {
        900_000
    } else if ctx.killers[ply][1] == Some(m) {
        800_000
    } else if m.promo == QUEEN {
        700_000
    } else {
        0
    }
}

fn store_killer(ctx: &mut Ctx, ply: usize, m: Move) {
    if ctx.killers[ply][0] != Some(m) {
        ctx.killers[ply][1] = ctx.killers[ply][0];
        ctx.killers[ply][0] = Some(m);
    }
}

fn qsearch(pos: &mut Position, mut alpha: i32, beta: i32, ply: usize, ctx: &mut Ctx) -> i32 {
    ctx.nodes += 1;
    ctx.check_abort();
    if ctx.abort || ply >= MAX_PLY {
        return evaluate(pos);
    }

    if pos.in_check() {
        let mut moves = pos.legal_moves();
        if moves.is_empty() {
            return -MATE + ply as i32;
        }
        moves.sort_by_key(|&m| -order_key(pos, m, ply, ctx));
        let mut best = -INF;
        for m in moves {
            let u = pos.make(m);
            let score = -qsearch(pos, -beta, -alpha, ply + 1, ctx);
            pos.unmake(m, u);
            if ctx.abort {
                return 0;
            }
            if score > best {
                best = score;
            }
            if score > alpha {
                alpha = score;
            }
            if alpha >= beta {
                break;
            }
        }
        return best;
    }

    let stand = evaluate(pos);
    if stand >= beta {
        return stand;
    }
    if stand > alpha {
        alpha = stand;
    }

    let mut caps: Vec<Move> = pos
        .legal_moves()
        .into_iter()
        .filter(|&m| pos.is_capture(m) || m.promo != 0)
        .collect();
    caps.sort_by_key(|&m| -mvv_lva(pos, m));

    for m in caps {
        let u = pos.make(m);
        let score = -qsearch(pos, -beta, -alpha, ply + 1, ctx);
        pos.unmake(m, u);
        if ctx.abort {
            return 0;
        }
        if score >= beta {
            return score;
        }
        if score > alpha {
            alpha = score;
        }
    }
    alpha
}

fn alphabeta(
    pos: &mut Position,
    depth: i32,
    mut alpha: i32,
    beta: i32,
    ply: usize,
    ctx: &mut Ctx,
) -> i32 {
    ctx.nodes += 1;
    ctx.check_abort();
    if ctx.abort {
        return 0;
    }

    if pos.halfmove >= 100 {
        return 0;
    }

    let mut moves = pos.legal_moves();
    if moves.is_empty() {
        return if pos.in_check() {
            -MATE + ply as i32
        } else {
            0
        };
    }

    if depth <= 0 || ply >= MAX_PLY {
        return qsearch(pos, alpha, beta, ply, ctx);
    }

    moves.sort_by_key(|&m| -order_key(pos, m, ply, ctx));

    let mut best = -INF;
    for m in moves {
        let capture = pos.is_capture(m);
        let u = pos.make(m);
        let score = -alphabeta(pos, depth - 1, -beta, -alpha, ply + 1, ctx);
        pos.unmake(m, u);
        if ctx.abort {
            return 0;
        }
        if score > best {
            best = score;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            if !capture {
                store_killer(ctx, ply, m);
            }
            break;
        }
    }
    best
}

fn root_search(pos: &mut Position, depth: i32, ctx: &mut Ctx) -> (Move, i32) {
    let mut moves = pos.legal_moves();
    moves.sort_by_key(|&m| -order_key(pos, m, 0, ctx));
    let mut best_move = moves[0];
    let mut best_score = -INF;
    let mut alpha = -INF;
    let beta = INF;

    for m in moves {
        let u = pos.make(m);
        let score = -alphabeta(pos, depth - 1, -beta, -alpha, 1, ctx);
        pos.unmake(m, u);
        if ctx.abort {
            break;
        }
        if score > best_score {
            best_score = score;
            best_move = m;
        }
        if score > alpha {
            alpha = score;
        }
    }
    (best_move, best_score)
}

/// Look-ahead search. Returns a legal move for the side to move.
pub fn best_move(pos: &Position, limits: &SearchLimits, stop: &AtomicBool) -> Move {
    search_best(pos, limits, stop).0
}

pub fn search_best(pos: &Position, limits: &SearchLimits, stop: &AtomicBool) -> (Move, i32) {
    let mut pos = pos.clone();
    let moves = pos.legal_moves();
    if moves.is_empty() {
        return (Move::new(0, 0), if pos.in_check() { -MATE } else { 0 });
    }

    let budget = time_budget(&pos, limits);
    let max_depth = limits.depth.unwrap_or(64).clamp(1, 64);
    let mut ctx = Ctx {
        stop,
        start: Instant::now(),
        budget,
        nodes: 0,
        abort: false,
        killers: [[None; 2]; MAX_PLY],
    };

    let mut best = moves[0];
    let mut best_score = -INF;

    for depth in 1..=max_depth {
        if depth > 1 && ctx.timed_out() {
            break;
        }
        ctx.abort = false;
        let (mv, score) = root_search(&mut pos, depth, &mut ctx);
        if ctx.abort && depth > 1 {
            break;
        }
        best = mv;
        best_score = score;
        let ms = ctx.start.elapsed().as_millis();
        let sc = if best_score.abs() > MATE - 128 {
            format!("mate {}", mate_plies(best_score))
        } else {
            format!("cp {best_score}")
        };
        println!(
            "info depth {depth} score {sc} nodes {} time {ms} pv {}",
            ctx.nodes,
            best.to_lan()
        );
        if best_score.abs() >= MATE - 32 {
            break;
        }
        if ctx.timed_out() {
            break;
        }
    }
    (best, best_score)
}

fn mate_plies(score: i32) -> i32 {
    if score > 0 {
        (MATE - score + 1) / 2
    } else {
        -((MATE + score + 1) / 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Position;

    #[test]
    fn unique_mate_in_one() {
        let pos = Position::from_fen("6k1/5ppp/8/8/8/8/8/4R2K w - - 0 1").unwrap();
        let stop = AtomicBool::new(false);
        let limits = SearchLimits {
            depth: Some(3),
            movetime_ms: Some(4000),
            ..SearchLimits::default()
        };
        let (mv, score) = search_best(&pos, &limits, &stop);
        assert_eq!(
            mv.to_lan(),
            "e1e8",
            "expected back-rank mate e1e8, got {}",
            mv.to_lan()
        );
        assert!(
            score >= MATE - 32,
            "mate-in-1 should score as mate, got {score}"
        );
    }

    #[test]
    fn search_returns_legal_startpos_move() {
        let pos = Position::startpos();
        let stop = AtomicBool::new(false);
        let limits = SearchLimits {
            depth: Some(3),
            movetime_ms: Some(2000),
            ..SearchLimits::default()
        };
        let mv = best_move(&pos, &limits, &stop);
        let legal: Vec<String> = pos.legal_moves().iter().map(|m| m.to_lan()).collect();
        assert!(
            legal.contains(&mv.to_lan()),
            "search move {} not legal",
            mv.to_lan()
        );
    }
}
