//! Iterative-deepening alpha-beta with quiescence, a transposition table,
//! and conservative selective search (null move, late-move reductions).
//!
//! Search heuristics below are inherited (typical formulas, never ablated in
//! this engine until KANON.md). Production defaults stay on; an ablation
//! flips one const to `false` and plays that binary against this one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::board::{Color, EMPTY, Move, PAWN, Position, QUEEN, type_of};
use crate::eval::{evaluate, piece_val};
use crate::tb::{Tablebase, score_of_wdl};
use crate::tt::{TT_ALPHA, TT_BETA, TT_EXACT, TranspositionTable};

pub const MATE: i32 = 30_000;
const INF: i32 = 32_000;
const MAX_PLY: usize = 64;
const MATE_WINDOW: i32 = 512;

/// Null-move pruning. Reduction R = 2 + (depth >= 6), min depth 3, skipped
/// in check and in king-and-pawn endings. Inherited; see KANON.md.
const USE_NULL_MOVE: bool = true;
/// LMR: reduce quiet non-killers after 3 searches, extra ply after 8 at
/// depth >= 5. Inherited formula.
const USE_LMR: bool = true;
/// Depth-1 futility: skip a quiet if static eval + 250 <= alpha. Inherited.
const FUTILITY_MARGIN: i32 = 250;
/// Quiescence delta: skip a capture if stand + victim + 200 < alpha. Inherited.
const DELTA_MARGIN: i32 = 200;

#[derive(Clone, Debug)]
pub struct SearchLimits {
    pub depth: Option<i32>,
    pub movetime_ms: Option<u64>,
    pub wtime_ms: Option<u64>,
    pub btime_ms: Option<u64>,
    pub winc_ms: Option<u64>,
    pub binc_ms: Option<u64>,
    pub nodes: Option<u64>,
    pub infinite: bool,
    pub move_overhead_ms: u64,
    pub silent: bool,
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
            nodes: None,
            infinite: false,
            move_overhead_ms: 100,
            silent: false,
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

pub struct SearchResult {
    pub best: Move,
    pub score: i32,
    pub nodes: u64,
    pub depth: i32,
    pub pv: Vec<Move>,
}

struct Ctx<'a> {
    stop: &'a AtomicBool,
    start: Instant,
    budget: Duration,
    node_limit: Option<u64>,
    nodes: u64,
    abort: bool,
    killers: [[Option<Move>; 2]; MAX_PLY],
    hist: [[[i32; 64]; 64]; 2],
    moves: Vec<Vec<Move>>,
    pv: [[Move; MAX_PLY]; MAX_PLY],
    pv_len: [usize; MAX_PLY],
    rep: Vec<u64>,
    tt: &'a mut TranspositionTable,
    prev_best: Option<Move>,
    tb: &'a Tablebase,
}

impl Ctx<'_> {
    fn timed_out(&self) -> bool {
        if self.stop.load(Ordering::Relaxed) {
            return true;
        }
        if let Some(n) = self.node_limit {
            if self.nodes >= n {
                return true;
            }
        }
        self.start.elapsed() >= self.budget
    }

    fn check_abort(&mut self) {
        if let Some(n) = self.node_limit {
            if self.nodes >= n {
                self.abort = true;
                return;
            }
        }
        if self.nodes & 63 == 0 && self.timed_out() {
            self.abort = true;
        }
    }
}

fn time_budget(pos: &Position, limits: &SearchLimits) -> Duration {
    if limits.infinite
        && limits.movetime_ms.is_none()
        && limits.wtime_ms.is_none()
        && limits.btime_ms.is_none()
    {
        return Duration::from_secs(60 * 60);
    }
    let overhead = limits.move_overhead_ms;
    if let Some(mt) = limits.movetime_ms {
        return Duration::from_millis(mt.saturating_sub(overhead.max(20)).max(1));
    }
    let (remain, inc) = if pos.side == Color::White {
        (limits.wtime_ms.unwrap_or(0), limits.winc_ms.unwrap_or(0))
    } else {
        (limits.btime_ms.unwrap_or(0), limits.binc_ms.unwrap_or(0))
    };
    if remain == 0 {
        if limits.depth.is_some() || limits.nodes.is_some() {
            return Duration::from_secs(60 * 60);
        }
        return Duration::from_millis(1000);
    }
    let alloc = remain / 30 + inc.saturating_mul(4) / 5;
    Duration::from_millis(
        alloc
            .min(remain.saturating_sub(overhead.saturating_add(50)))
            .max(5),
    )
}

/// Two-fold in the search tree is scored as a draw; at the root we never
/// call this, so the root always plays a real move. A 3-fold from the game
/// history is a 2-fold once the repeating move is made, so it is found.
fn is_repetition(pos: &Position, rep: &[u64], ply: usize) -> bool {
    if pos.halfmove < 4 || ply == 0 {
        return false;
    }
    let key = pos.hash;
    let len = rep.len();
    if len < 3 {
        return false;
    }
    let oldest = (len - 1).saturating_sub(pos.halfmove as usize);
    let mut i = len - 3;
    loop {
        if i < oldest {
            break;
        }
        if rep[i] == key {
            return true;
        }
        if i < 2 {
            break;
        }
        i -= 2;
    }
    false
}

fn mvv_lva(pos: &Position, m: Move) -> i32 {
    let victim = if m.is_ep() {
        PAWN
    } else {
        type_of(pos.piece_at(m.to))
    };
    let attacker = type_of(pos.piece_at(m.from));
    let mut s = piece_val(victim) - piece_val(attacker) / 16;
    if m.promo != 0 {
        s += piece_val(m.promo);
    }
    s
}

fn order_key(pos: &Position, m: Move, ply: usize, ctx: &Ctx, tt_move: Option<Move>) -> i32 {
    if tt_move == Some(m) {
        return 2_000_000;
    }
    if ply == 0 && ctx.prev_best == Some(m) {
        return 1_900_000;
    }
    if pos.is_capture(m) {
        return 1_000_000 + mvv_lva(pos, m);
    }
    if m.promo == QUEEN {
        return 900_000;
    }
    if ctx.killers[ply][0] == Some(m) {
        return 800_000;
    }
    if ctx.killers[ply][1] == Some(m) {
        return 700_000;
    }
    ctx.hist[pos.side.idx()][m.from as usize][m.to as usize]
}

fn store_killer(ctx: &mut Ctx, ply: usize, m: Move) {
    if ctx.killers[ply][0] != Some(m) {
        ctx.killers[ply][1] = ctx.killers[ply][0];
        ctx.killers[ply][0] = Some(m);
    }
}

fn add_history(ctx: &mut Ctx, side: Color, m: Move, depth: i32) {
    let h = &mut ctx.hist[side.idx()][m.from as usize][m.to as usize];
    *h = (*h + depth * depth).min(100_000);
}

fn update_pv(ctx: &mut Ctx, ply: usize, m: Move) {
    ctx.pv[ply][0] = m;
    let n = if ply + 1 < MAX_PLY {
        ctx.pv_len[ply + 1]
    } else {
        0
    };
    for i in 0..n {
        ctx.pv[ply][i + 1] = ctx.pv[ply + 1][i];
    }
    ctx.pv_len[ply] = n + 1;
}

fn pv_string(ctx: &Ctx) -> String {
    let n = ctx.pv_len[0];
    let mut s = String::new();
    for i in 0..n {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&ctx.pv[0][i].to_lan());
    }
    s
}

fn tb_cutoff(pos: &mut Position, ply: usize, tb: &Tablebase) -> Option<i32> {
    if tb.is_empty() || ply == 0 || pos.halfmove != 0 || pos.castling != 0 {
        return None;
    }
    let n = pos.squares.iter().filter(|&&p| p != EMPTY).count();
    if n > 5 || n < 2 {
        return None;
    }
    // Immediate mate outranks a table loss, including on the search horizon.
    if pos.in_check() {
        let mut ms = Vec::new();
        pos.gen_legal_into(&mut ms);
        if ms.is_empty() {
            return Some(-MATE + ply as i32);
        }
    }
    let wdl = tb.probe_wdl(pos)?;
    Some(score_of_wdl(wdl, ply as i32))
}

fn qsearch(pos: &mut Position, mut alpha: i32, beta: i32, ply: usize, ctx: &mut Ctx) -> i32 {
    ctx.nodes += 1;
    ctx.check_abort();
    if ctx.abort || ply >= MAX_PLY {
        return evaluate(pos);
    }
    if let Some(s) = tb_cutoff(pos, ply, ctx.tb) {
        return s;
    }

    if pos.in_check() {
        pos.gen_legal_into(&mut ctx.moves[ply]);
        if ctx.moves[ply].is_empty() {
            return -MATE + ply as i32;
        }
        let mut scored: Vec<(i32, Move)> = ctx.moves[ply]
            .iter()
            .map(|&m| (mvv_lva(pos, m), m))
            .collect();
        scored.sort_by_key(|&(s, _)| -s);
        let mut best = -INF;
        for (_, m) in scored {
            let u = pos.make(m);
            ctx.rep.push(pos.hash);
            let score = -qsearch(pos, -beta, -alpha, ply + 1, ctx);
            ctx.rep.pop();
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

    pos.gen_captures_into(&mut ctx.moves[ply]);
    let mut scored: Vec<(i32, Move)> = ctx.moves[ply]
        .iter()
        .map(|&m| (mvv_lva(pos, m), m))
        .collect();
    scored.sort_by_key(|&(s, _)| -s);

    for (_, m) in scored {
        let victim = if m.is_ep() {
            PAWN
        } else {
            type_of(pos.piece_at(m.to))
        };
        if m.promo == 0 && stand + piece_val(victim) + DELTA_MARGIN < alpha {
            continue;
        }
        let u = pos.make(m);
        ctx.rep.push(pos.hash);
        let score = -qsearch(pos, -beta, -alpha, ply + 1, ctx);
        ctx.rep.pop();
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
    mut depth: i32,
    mut alpha: i32,
    mut beta: i32,
    ply: usize,
    ctx: &mut Ctx,
) -> i32 {
    ctx.nodes += 1;
    ctx.check_abort();
    if ctx.abort {
        return 0;
    }
    ctx.pv_len[ply] = 0;

    if is_repetition(pos, &ctx.rep, ply) {
        return 0;
    }
    if let Some(s) = tb_cutoff(pos, ply, ctx.tb) {
        return s;
    }

    if depth <= 0 || ply >= MAX_PLY {
        return qsearch(pos, alpha, beta, ply, ctx);
    }

    // Mate takes precedence over the 50-move rule: if we are mated, it is mate
    // even on the 100th half-move. If we are not in check, both 50-move and
    // stalemate score as 0, so we can return 0 without generating moves.
    let in_check = pos.in_check();
    if pos.halfmove >= 100 && !in_check {
        return 0;
    }

    alpha = alpha.max(-MATE + ply as i32);
    beta = beta.min(MATE - ply as i32 - 1);
    if alpha >= beta {
        return alpha;
    }

    let orig_alpha = alpha;
    let mut tt_move = None;
    if let Some(e) = ctx.tt.probe(pos.hash) {
        tt_move = e.mv();
        if e.depth as i32 >= depth && e.score.abs() as i32 <= MATE - MATE_WINDOW {
            let s = e.score as i32;
            match e.flag {
                TT_EXACT => return s,
                TT_ALPHA if s <= alpha => return s,
                TT_BETA if s >= beta => return s,
                _ => {}
            }
        }
    }

    if in_check {
        depth += 1;
    }

    if USE_NULL_MOVE && !in_check && depth >= 3 && ply > 0 && pos.has_non_pawn_material(pos.side) {
        let eval = evaluate(pos);
        if eval >= beta {
            let u = pos.make_null();
            ctx.rep.push(pos.hash);
            let r = 2 + i32::from(depth >= 6);
            let score = -alphabeta(pos, depth - 1 - r, -beta, -beta + 1, ply + 1, ctx);
            ctx.rep.pop();
            pos.unmake_null(u);
            if ctx.abort {
                return 0;
            }
            if score >= beta {
                return beta;
            }
        }
    }

    pos.gen_legal_into(&mut ctx.moves[ply]);
    if ctx.moves[ply].is_empty() {
        return if in_check { -MATE + ply as i32 } else { 0 };
    }
    if pos.halfmove >= 100 {
        return 0;
    }

    let tt_mv = tt_move;
    let mut moves = std::mem::take(&mut ctx.moves[ply]);
    moves.sort_by_cached_key(|&m| -order_key(pos, m, ply, ctx, tt_mv));

    let mut best = -INF;
    let mut best_move = None;
    let mut searched = 0;

    for i in 0..moves.len() {
        let m = moves[i];
        let capture = pos.is_capture(m);
        let quiet = !capture && m.promo == 0;

        if !in_check && depth <= 1 && quiet && searched >= 1 {
            let eval = evaluate(pos);
            if eval + FUTILITY_MARGIN <= alpha {
                searched += 1;
                continue;
            }
        }

        let u = pos.make(m);
        ctx.rep.push(pos.hash);

        let mut new_depth = depth - 1;
        let mut reduced = false;
        if USE_LMR
            && quiet
            && !in_check
            && depth >= 3
            && searched >= 3
            && ctx.killers[ply][0] != Some(m)
            && ctx.killers[ply][1] != Some(m)
        {
            let r = 1 + i32::from(searched >= 8 && depth >= 5);
            new_depth = (depth - 1 - r).max(0);
            reduced = new_depth < depth - 1;
        }

        let score = if searched == 0 {
            -alphabeta(pos, new_depth, -beta, -alpha, ply + 1, ctx)
        } else {
            let mut s = -alphabeta(pos, new_depth, -alpha - 1, -alpha, ply + 1, ctx);
            if !ctx.abort && s > alpha && (reduced || s < beta) {
                s = -alphabeta(pos, depth - 1, -beta, -alpha, ply + 1, ctx);
            }
            s
        };

        ctx.rep.pop();
        pos.unmake(m, u);
        if ctx.abort {
            return 0;
        }
        searched += 1;

        if score > best {
            best = score;
            best_move = Some(m);
            update_pv(ctx, ply, m);
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            if quiet {
                store_killer(ctx, ply, m);
                add_history(ctx, pos.side, m, depth);
            }
            break;
        }
    }
    ctx.moves[ply] = moves;

    if !ctx.abort {
        let flag = if best <= orig_alpha {
            TT_ALPHA
        } else if best >= beta {
            TT_BETA
        } else {
            TT_EXACT
        };
        ctx.tt.store(pos.hash, depth, best, flag, best_move);
    }
    best
}

fn root_search(pos: &mut Position, depth: i32, ctx: &mut Ctx) -> (Move, i32) {
    ctx.pv_len[0] = 0;
    pos.gen_legal_into(&mut ctx.moves[0]);
    let tt_move = ctx.tt.probe(pos.hash).and_then(|e| e.mv());
    let mut moves = std::mem::take(&mut ctx.moves[0]);
    moves.sort_by_cached_key(|&m| -order_key(pos, m, 0, ctx, tt_move));

    let mut best_move = moves[0];
    let mut best_score = -INF;
    let mut alpha = -INF;
    let beta = INF;

    for i in 0..moves.len() {
        let m = moves[i];
        let u = pos.make(m);
        ctx.rep.push(pos.hash);
        let score = -alphabeta(pos, depth - 1, -beta, -alpha, 1, ctx);
        ctx.rep.pop();
        pos.unmake(m, u);
        if ctx.abort {
            break;
        }
        if score > best_score {
            best_score = score;
            best_move = m;
            update_pv(ctx, 0, m);
        }
        if score > alpha {
            alpha = score;
        }
    }
    ctx.moves[0] = moves;
    (best_move, best_score)
}

/// Look-ahead search. Returns a legal move for the side to move.
pub fn best_move(pos: &Position, limits: &SearchLimits, stop: &AtomicBool, tb: &Tablebase) -> Move {
    let mut tt = TranspositionTable::with_mb(8);
    search_best(pos, &[pos.hash], limits, &mut tt, stop, tb).best
}

pub fn search_best(
    pos: &Position,
    history: &[u64],
    limits: &SearchLimits,
    tt: &mut TranspositionTable,
    stop: &AtomicBool,
    tb: &Tablebase,
) -> SearchResult {
    let mut pos = pos.clone();
    let mut root_moves = Vec::with_capacity(64);
    pos.gen_legal_into(&mut root_moves);
    if root_moves.is_empty() {
        return SearchResult {
            best: Move::new(0, 0),
            score: if pos.in_check() { -MATE } else { 0 },
            nodes: 0,
            depth: 0,
            pv: Vec::new(),
        };
    }

    if let Some(pick) = tb.root_pick(&mut pos, history) {
        let mut score = score_of_wdl(pick.wdl, 0);
        let u = pos.make(pick.mv);
        let mates = pos.in_check() && {
            let mut ms = Vec::new();
            pos.gen_legal_into(&mut ms);
            ms.is_empty()
        };
        pos.unmake(pick.mv, u);
        if mates {
            score = MATE - 1;
        }
        if !limits.silent {
            let sc = if score.abs() > MATE - 128 {
                format!("mate {}", mate_plies(score))
            } else {
                format!("cp {score}")
            };
            println!(
                "info depth 1 score {sc} nodes 1 nps 1 time 1 pv {}",
                pick.mv.to_lan()
            );
        }
        return SearchResult {
            best: pick.mv,
            score,
            nodes: 1,
            depth: 1,
            pv: vec![pick.mv],
        };
    }

    let budget = time_budget(&pos, limits);
    let max_depth = limits.depth.unwrap_or(64).clamp(1, 64);
    let mut rep = if history.is_empty() {
        vec![pos.hash]
    } else {
        history.to_vec()
    };
    if rep.last() != Some(&pos.hash) {
        rep.push(pos.hash);
    }

    let dummy = Move::new(0, 0);
    let mut ctx = Ctx {
        stop,
        start: Instant::now(),
        budget,
        node_limit: limits.nodes,
        nodes: 0,
        abort: false,
        killers: [[None; 2]; MAX_PLY],
        hist: [[[0; 64]; 64]; 2],
        moves: (0..=MAX_PLY).map(|_| Vec::with_capacity(64)).collect(),
        pv: [[dummy; MAX_PLY]; MAX_PLY],
        pv_len: [0; MAX_PLY],
        rep,
        tt,
        prev_best: None,
        tb,
    };

    let mut best = root_moves[0];
    let mut best_score = -INF;
    let mut reached = 0i32;
    let mut best_pv: Vec<Move> = vec![best];

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
        reached = depth;
        ctx.prev_best = Some(best);
        best_pv = (0..ctx.pv_len[0]).map(|i| ctx.pv[0][i]).collect();
        if best_pv.is_empty() {
            best_pv.push(best);
        }
        let ms = ctx.start.elapsed().as_millis().max(1);
        let nps = ctx.nodes.saturating_mul(1000) / ms as u64;
        let sc = if best_score.abs() > MATE - 128 {
            format!("mate {}", mate_plies(best_score))
        } else {
            format!("cp {best_score}")
        };
        if !limits.silent {
            let pv = pv_string(&ctx);
            println!(
                "info depth {depth} score {sc} nodes {} nps {nps} time {ms} pv {pv}",
                ctx.nodes
            );
        }
        if best_score.abs() >= MATE - 32 {
            break;
        }
        if ctx.timed_out() {
            break;
        }
    }
    SearchResult {
        best,
        score: best_score,
        nodes: ctx.nodes,
        depth: reached,
        pv: best_pv,
    }
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

    fn search_depth(pos: &Position, history: &[u64], depth: i32) -> SearchResult {
        let stop = AtomicBool::new(false);
        let mut tt = TranspositionTable::with_mb(4);
        let limits = SearchLimits {
            depth: Some(depth),
            movetime_ms: Some(8000),
            silent: true,
            ..SearchLimits::default()
        };
        let tb = Tablebase::open(crate::tb::DEFAULT_PATH);
        search_best(pos, history, &limits, &mut tt, &stop, &tb)
    }

    fn apply_moves(fen: &str, moves: &[&str]) -> (Position, Vec<u64>) {
        let mut pos = Position::from_fen(fen).unwrap();
        let mut hist = vec![pos.hash];
        for lan in moves {
            let m = pos
                .move_from_lan(lan)
                .unwrap_or_else(|| panic!("illegal {lan} in {}", pos.to_fen()));
            pos.make(m);
            hist.push(pos.hash);
        }
        (pos, hist)
    }

    #[test]
    fn unique_mate_in_one() {
        let pos = Position::from_fen("6k1/5ppp/8/8/8/8/8/4R2K w - - 0 1").unwrap();
        let res = search_depth(&pos, &[pos.hash], 3);
        assert_eq!(
            res.best.to_lan(),
            "e1e8",
            "expected back-rank mate e1e8, got {}",
            res.best.to_lan()
        );
        assert!(
            res.score >= MATE - 32,
            "mate-in-1 should score as mate, got {}",
            res.score
        );
    }

    #[test]
    fn search_returns_legal_startpos_move() {
        let pos = Position::startpos();
        let res = search_depth(&pos, &[pos.hash], 3);
        let legal: Vec<String> = pos.legal_moves().iter().map(|m| m.to_lan()).collect();
        assert!(
            legal.contains(&res.best.to_lan()),
            "search move {} not legal",
            res.best.to_lan()
        );
    }

    #[test]
    fn threefold_is_chosen_when_losing() {
        // Rook vs queen: repeating with h1g1 is the only way to draw.
        let (pos, hist) = apply_moves(
            "3q1k2/5ppp/8/8/8/8/8/4R1K1 w - - 0 1",
            &[
                "g1h1", "f8g8", "h1g1", "g8f8", "g1h1", "f8g8", "h1g1", "g8f8", "g1h1", "f8g8",
            ],
        );
        let res = search_depth(&pos, &hist, 4);
        assert_eq!(
            res.best.to_lan(),
            "h1g1",
            "expected repetition h1g1, got {} score {}",
            res.best.to_lan(),
            res.score
        );
        assert_eq!(res.score, 0, "repetition must score 0, got {}", res.score);
    }

    #[test]
    fn winning_side_avoids_repetition() {
        // Queen vs king+pawns: mate is available, repeating the king is a draw.
        let (pos, hist) = apply_moves(
            "6k1/5ppp/8/8/8/8/8/4Q1K1 w - - 0 1",
            &["g1h1", "g8h8", "h1g1", "h8g8", "g1h1", "g8h8"],
        );
        let res = search_depth(&pos, &hist, 4);
        assert_ne!(
            res.best.to_lan(),
            "h1g1",
            "winning side must not repeat, got {} score {}",
            res.best.to_lan(),
            res.score
        );
        assert!(res.score > 200, "should keep the win, score {}", res.score);
    }

    #[test]
    fn node_limit_is_respected() {
        let pos = Position::startpos();
        let stop = AtomicBool::new(false);
        let mut tt = TranspositionTable::with_mb(4);
        let limits = SearchLimits {
            nodes: Some(5_000),
            silent: true,
            ..SearchLimits::default()
        };
        let tb = Tablebase::open(crate::tb::DEFAULT_PATH);
        let res = search_best(&pos, &[pos.hash], &limits, &mut tt, &stop, &tb);
        assert!(res.nodes > 0, "searched no nodes");
        assert!(
            res.nodes <= 5_000 + 512,
            "go nodes 5000 searched {}",
            res.nodes
        );
        let legal: Vec<String> = pos.legal_moves().iter().map(|m| m.to_lan()).collect();
        assert!(
            legal.contains(&res.best.to_lan()),
            "search move {} not legal",
            res.best.to_lan()
        );
    }

    #[test]
    fn tablebase_plays_the_ending() {
        let pos = Position::from_fen("8/8/8/4k3/8/8/4Q3/4K3 w - - 0 1").unwrap();
        let res = search_depth(&pos, &[pos.hash], 2);
        assert!(
            res.score >= crate::tb::TB_SCORE - 2,
            "KQ vs K should be a table win, score {} move {}",
            res.score,
            res.best
        );
        let drawn = Position::from_fen("8/8/8/4k3/8/8/4K3/4R3 w - - 99 1").unwrap();
        let res = search_depth(&drawn, &[drawn.hash], 2);
        assert_eq!(res.score, 0, "KRK on the 50-move boundary is a draw");
        let mate = Position::from_fen("k7/8/1K6/8/8/8/8/7R w - - 99 1").unwrap();
        let res = search_depth(&mate, &[mate.hash], 2);
        assert!(
            res.score >= MATE - 2,
            "mate on the 100th half-move, score {}",
            res.score
        );
    }

    #[test]
    fn mate_beats_fifty_move_rule() {
        // Halfmove clock is 99; the mate move is not a capture or pawn move, so
        // it is the 100th half-move. Checkmate still wins.
        let pos = Position::from_fen("6k1/5ppp/8/8/8/8/8/4R2K w - - 99 1").unwrap();
        let res = search_depth(&pos, &[pos.hash], 3);
        assert_eq!(res.best.to_lan(), "e1e8");
        assert!(
            res.score >= MATE - 32,
            "mate on the 100th half-move must not be a draw, got {}",
            res.score
        );
    }
}
