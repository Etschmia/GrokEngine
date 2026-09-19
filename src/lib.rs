//! Grokengine: an original UCI chess engine.
//!
//! Board representation, legal-move generation, evaluation, search, and the
//! UCI loop are all implemented in this crate. No third-party chess engine
//! code is used.

pub mod bench;
pub mod board;
pub mod eval;
pub mod search;
pub mod tt;
pub mod uci;
pub mod zobrist;

pub use board::{Move, Position};
pub use search::{best_move, SearchLimits};
