//! Fixed-position bench: nodes, time, NPS, and reached depth.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crate::board::Position;
use crate::search::{search_best, SearchLimits};
use crate::tt::TranspositionTable;

pub const DEPTH: i32 = 6;

pub const FENS: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    "3q1k2/5ppp/8/8/8/8/8/4R1K1 w - - 0 1",
    "6k1/5ppp/8/8/8/8/8/4Q1K1 w - - 0 1",
    "8/k7/3p4/p2P1p2/P2P1P2/8/8/K7 w - - 0 1",
];

pub fn run() {
    let stop = AtomicBool::new(false);
    let mut tt = TranspositionTable::with_mb(32);
    let limits = SearchLimits {
        depth: Some(DEPTH),
        silent: false,
        ..SearchLimits::default()
    };
    let t0 = Instant::now();
    let mut nodes = 0u64;
    let mut depth_sum = 0i32;
    for (i, fen) in FENS.iter().enumerate() {
        let pos = Position::from_fen(fen).expect("bench fen");
        tt.clear();
        let res = search_best(&pos, &[pos.hash], &limits, &mut tt, &stop);
        nodes += res.nodes;
        depth_sum += res.depth;
        println!(
            "bench pos {} depth {} nodes {} best {}",
            i + 1,
            res.depth,
            res.nodes,
            res.best.to_lan()
        );
    }
    let ms = t0.elapsed().as_millis().max(1);
    let nps = nodes.saturating_mul(1000) / ms as u64;
    println!(
        "bench total nodes {nodes} time {ms} nps {nps} avg_depth {:.1} positions {}",
        depth_sum as f64 / FENS.len() as f64,
        FENS.len()
    );
}
