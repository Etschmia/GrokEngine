//! UCI protocol: stdin/stdout loop for a launchable engine process.

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::board::Position;
use crate::search::{search_best, SearchLimits};
use crate::tt::TranspositionTable;

const NAME: &str = "Grokengine";
const AUTHOR: &str = "Grok";
const DEFAULT_HASH_MB: usize = 32;
const DEFAULT_OVERHEAD_MS: u64 = 100;

pub struct Engine {
    pub pos: Position,
    pub history: Vec<u64>,
    pub tt: Arc<Mutex<TranspositionTable>>,
    pub move_overhead_ms: u64,
    pub hash_mb: usize,
}

impl Engine {
    pub fn new() -> Self {
        let pos = Position::startpos();
        let hash = pos.hash;
        Self {
            pos,
            history: vec![hash],
            tt: Arc::new(Mutex::new(TranspositionTable::with_mb(DEFAULT_HASH_MB))),
            move_overhead_ms: DEFAULT_OVERHEAD_MS,
            hash_mb: DEFAULT_HASH_MB,
        }
    }

    fn reset_start(&mut self) {
        self.pos = Position::startpos();
        self.history.clear();
        self.history.push(self.pos.hash);
    }
}

pub fn run() {
    let stdin_stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<String>();

    {
        let stdin_stop = Arc::clone(&stdin_stop);
        thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                let trimmed = line.trim();
                // Answer isready here so it is not stuck behind a blocking `go`.
                if trimmed == "isready" {
                    let mut out = io::stdout().lock();
                    let _ = writeln!(out, "readyok");
                    let _ = out.flush();
                    continue;
                }
                if trimmed == "stop" || trimmed.starts_with("stop ") {
                    stdin_stop.store(true, Ordering::Relaxed);
                    continue;
                }
                if trimmed == "quit" || trimmed.starts_with("quit ") {
                    stdin_stop.store(true, Ordering::Relaxed);
                    let _ = tx.send(line);
                    break;
                }
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }

    let mut engine = Engine::new();
    let mut out = io::stdout();

    loop {
        let line = match rx.recv() {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cmd = trimmed.split_whitespace().next().unwrap_or("");
        match cmd {
            "uci" => {
                let _ = writeln!(out, "id name {NAME}");
                let _ = writeln!(out, "id author {AUTHOR}");
                let _ = writeln!(
                    out,
                    "option name Hash type spin default {DEFAULT_HASH_MB} min 1 max 1024"
                );
                let _ = writeln!(
                    out,
                    "option name Move Overhead type spin default {DEFAULT_OVERHEAD_MS} min 0 max 5000"
                );
                let _ = writeln!(out, "uciok");
                let _ = out.flush();
            }
            "isready" => {
                let _ = writeln!(out, "readyok");
                let _ = out.flush();
            }
            "stop" => {}
            "quit" => break,
            "ucinewgame" => {
                engine.reset_start();
                if let Ok(mut tt) = engine.tt.lock() {
                    tt.clear();
                }
            }
            "position" => apply_position(trimmed, &mut engine),
            "go" => {
                stdin_stop.store(false, Ordering::Relaxed);
                let mut limits = parse_go(trimmed);
                limits.move_overhead_ms = engine.move_overhead_ms;
                let res = {
                    let mut tt = engine.tt.lock().unwrap_or_else(|e| e.into_inner());
                    search_best(
                        &engine.pos,
                        &engine.history,
                        &limits,
                        &mut tt,
                        &stdin_stop,
                    )
                };
                let _ = writeln!(out, "bestmove {}", res.best.to_lan());
                let _ = out.flush();
            }
            "setoption" => apply_setoption(trimmed, &mut engine),
            "bench" => crate::bench::run(),
            "debug" | "ponderhit" => {}
            _ => {}
        }
    }
}

/// Returns true if the engine should quit. Synchronous: used by tests.
pub fn handle_line(
    line: &str,
    engine: &mut Engine,
    out: &mut impl Write,
    stop: &AtomicBool,
    searching: &AtomicBool,
) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return false;
    }
    let mut toks = line.split_whitespace();
    let Some(cmd) = toks.next() else {
        return false;
    };
    match cmd {
        "uci" => {
            let _ = writeln!(out, "id name {NAME}");
            let _ = writeln!(out, "id author {AUTHOR}");
            let _ = writeln!(
                out,
                "option name Hash type spin default {DEFAULT_HASH_MB} min 1 max 1024"
            );
            let _ = writeln!(
                out,
                "option name Move Overhead type spin default {DEFAULT_OVERHEAD_MS} min 0 max 5000"
            );
            let _ = writeln!(out, "uciok");
            let _ = out.flush();
        }
        "isready" => {
            let _ = writeln!(out, "readyok");
            let _ = out.flush();
        }
        "ucinewgame" => {
            engine.reset_start();
            if let Ok(mut tt) = engine.tt.lock() {
                tt.clear();
            }
        }
        "position" => {
            apply_position(line, engine);
        }
        "go" => {
            let mut limits = parse_go(line);
            limits.move_overhead_ms = engine.move_overhead_ms;
            stop.store(false, Ordering::Relaxed);
            searching.store(true, Ordering::Relaxed);
            let mv = {
                let mut tt = engine.tt.lock().unwrap_or_else(|e| e.into_inner());
                search_best(&engine.pos, &engine.history, &limits, &mut tt, stop)
                    .best
            };
            searching.store(false, Ordering::Relaxed);
            let _ = writeln!(out, "bestmove {}", mv.to_lan());
            let _ = out.flush();
        }
        "stop" => {}
        "quit" => return true,
        "setoption" => apply_setoption(line, engine),
        "bench" => crate::bench::run(),
        "debug" | "ponderhit" => {}
        _ => {}
    }
    false
}

fn apply_setoption(line: &str, engine: &mut Engine) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    // setoption name <Name> [name words] value <v>
    let Some(name_at) = tokens.iter().position(|&t| t == "name") else {
        return;
    };
    let value_at = tokens.iter().position(|&t| t == "value");
    let name_end = value_at.unwrap_or(tokens.len());
    if name_at + 1 >= name_end {
        return;
    }
    let name = tokens[name_at + 1..name_end].join(" ");
    let value = value_at.and_then(|i| tokens.get(i + 1)).copied();
    let lname = name.to_ascii_lowercase();
    match lname.as_str() {
        "hash" => {
            if let Some(v) = value.and_then(|s| s.parse::<usize>().ok()) {
                let mb = v.clamp(1, 1024);
                engine.hash_mb = mb;
                *engine.tt.lock().unwrap_or_else(|e| e.into_inner()) =
                    TranspositionTable::with_mb(mb);
            }
        }
        "move overhead" | "moveoverhead" => {
            if let Some(v) = value.and_then(|s| s.parse::<u64>().ok()) {
                engine.move_overhead_ms = v.min(5000);
            }
        }
        _ => {}
    }
}

fn apply_position(line: &str, engine: &mut Engine) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 2 {
        return;
    }
    let mut i = 1;
    if tokens[i] == "startpos" {
        engine.pos = Position::startpos();
        i += 1;
    } else if tokens[i] == "fen" {
        i += 1;
        let mut fen_parts: Vec<&str> = Vec::new();
        while i < tokens.len() && tokens[i] != "moves" {
            fen_parts.push(tokens[i]);
            i += 1;
        }
        if let Ok(p) = Position::from_fen(&fen_parts.join(" ")) {
            engine.pos = p;
        }
    } else {
        return;
    }
    engine.history.clear();
    engine.history.push(engine.pos.hash);
    if i < tokens.len() && tokens[i] == "moves" {
        i += 1;
        while i < tokens.len() {
            if let Some(m) = engine.pos.move_from_lan(tokens[i]) {
                engine.pos.make(m);
                engine.history.push(engine.pos.hash);
            }
            i += 1;
        }
    }
}

fn parse_go(line: &str) -> SearchLimits {
    let mut limits = SearchLimits::default();
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut i = 1;
    while i < tokens.len() {
        let t = tokens[i];
        let next = tokens.get(i + 1).copied();
        match t {
            "depth" => {
                if let Some(v) = next.and_then(|s| s.parse().ok()) {
                    limits.depth = Some(v);
                }
                i += 2;
            }
            "movetime" => {
                if let Some(v) = next.and_then(|s| s.parse().ok()) {
                    limits.movetime_ms = Some(v);
                }
                i += 2;
            }
            "wtime" => {
                if let Some(v) = next.and_then(|s| s.parse().ok()) {
                    limits.wtime_ms = Some(v);
                }
                i += 2;
            }
            "btime" => {
                if let Some(v) = next.and_then(|s| s.parse().ok()) {
                    limits.btime_ms = Some(v);
                }
                i += 2;
            }
            "winc" => {
                if let Some(v) = next.and_then(|s| s.parse().ok()) {
                    limits.winc_ms = Some(v);
                }
                i += 2;
            }
            "binc" => {
                if let Some(v) = next.and_then(|s| s.parse().ok()) {
                    limits.binc_ms = Some(v);
                }
                i += 2;
            }
            "infinite" => {
                limits.infinite = true;
                i += 1;
            }
            _ => i += 1,
        }
    }
    limits
}

/// Drive the UCI handler without the stdin thread. Used by tests.
pub fn handle_script(script: &str) -> String {
    let mut engine = Engine::new();
    let mut out = Vec::new();
    let stop = AtomicBool::new(false);
    let searching = AtomicBool::new(false);
    for line in script.lines() {
        if handle_line(line, &mut engine, &mut out, &stop, &searching) {
            break;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Position;

    #[test]
    fn uci_handshake() {
        let out = handle_script("uci\nisready\nquit\n");
        assert!(out.contains("id name"), "{out}");
        assert!(out.contains("id author"), "{out}");
        assert!(out.contains("uciok"), "{out}");
        assert!(out.contains("readyok"), "{out}");
        assert!(out.contains("option name Hash"), "{out}");
        assert!(out.contains("option name Move Overhead"), "{out}");
    }

    #[test]
    fn go_startpos_returns_legal_bestmove() {
        let out = handle_script("position startpos\ngo depth 2\nquit\n");
        let best = out
            .lines()
            .find(|l| l.starts_with("bestmove "))
            .unwrap_or("");
        let mv = best.split_whitespace().nth(1).unwrap_or("");
        let pos = Position::startpos();
        let legal: Vec<String> = pos.legal_moves().iter().map(|m| m.to_lan()).collect();
        assert!(legal.contains(&mv.to_string()), "illegal bestmove {mv} in {out}");
    }

    #[test]
    fn unknown_command_does_not_crash() {
        let out = handle_script("foo bar\nuci\nquit\n");
        assert!(out.contains("uciok"));
    }

    #[test]
    fn position_moves_keep_history_for_repetition() {
        let script = "\
position fen 3q1k2/5ppp/8/8/8/8/8/4R1K1 w - - 0 1 moves g1h1 f8g8 h1g1 g8f8 g1h1 f8g8 h1g1 g8f8 g1h1 f8g8
go depth 4
quit
";
        let out = handle_script(script);
        let best = out
            .lines()
            .rev()
            .find(|l| l.starts_with("bestmove "))
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("");
        assert_eq!(best, "h1g1", "expected draw by repetition, got:\n{out}");
    }

    #[test]
    fn setoption_move_overhead() {
        let mut engine = Engine::new();
        apply_setoption("setoption name Move Overhead value 250", &mut engine);
        assert_eq!(engine.move_overhead_ms, 250);
        apply_setoption("setoption name MoveOverhead value 80", &mut engine);
        assert_eq!(engine.move_overhead_ms, 80);
    }
}
