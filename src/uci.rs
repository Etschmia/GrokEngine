//! UCI protocol: stdin/stdout loop for a launchable engine process.

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use crate::board::Position;
use crate::search::{best_move, SearchLimits};

const NAME: &str = "Grokengine";
const AUTHOR: &str = "Grok";

pub fn run() {
    let stdin_stop = Arc::new(AtomicBool::new(false));
    let searching = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<String>();

    {
        let stdin_stop = Arc::clone(&stdin_stop);
        let searching = Arc::clone(&searching);
        thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                let trimmed = line.trim();
                if searching.load(Ordering::Relaxed)
                    && (trimmed == "stop" || trimmed == "quit" || trimmed.starts_with("stop "))
                {
                    stdin_stop.store(true, Ordering::Relaxed);
                }
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }

    let mut pos = Position::startpos();
    let mut out = io::stdout();

    loop {
        let line = match rx.recv() {
            Ok(l) => l,
            Err(_) => break,
        };
        if handle_line(&line, &mut pos, &mut out, &stdin_stop, &searching) {
            break;
        }
    }
}

/// Returns true if the engine should quit.
pub fn handle_line(
    line: &str,
    pos: &mut Position,
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
            let _ = writeln!(out, "uciok");
            let _ = out.flush();
        }
        "isready" => {
            let _ = writeln!(out, "readyok");
            let _ = out.flush();
        }
        "ucinewgame" => {
            *pos = Position::startpos();
        }
        "position" => {
            apply_position(line, pos);
        }
        "go" => {
            let limits = parse_go(line);
            stop.store(false, Ordering::Relaxed);
            searching.store(true, Ordering::Relaxed);
            let mv = best_move(pos, &limits, stop);
            searching.store(false, Ordering::Relaxed);
            let _ = writeln!(out, "bestmove {}", mv.to_lan());
            let _ = out.flush();
        }
        "stop" => {}
        "quit" => return true,
        "setoption" | "debug" | "ponderhit" => {}
        _ => {}
    }
    false
}

fn apply_position(line: &str, pos: &mut Position) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 2 {
        return;
    }
    let mut i = 1;
    if tokens[i] == "startpos" {
        *pos = Position::startpos();
        i += 1;
    } else if tokens[i] == "fen" {
        i += 1;
        let mut fen_parts: Vec<&str> = Vec::new();
        while i < tokens.len() && tokens[i] != "moves" {
            fen_parts.push(tokens[i]);
            i += 1;
        }
        if let Ok(p) = Position::from_fen(&fen_parts.join(" ")) {
            *pos = p;
        }
    } else {
        return;
    }
    if i < tokens.len() && tokens[i] == "moves" {
        i += 1;
        while i < tokens.len() {
            if let Some(m) = pos.move_from_lan(tokens[i]) {
                pos.make(m);
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
    let mut pos = Position::startpos();
    let mut out = Vec::new();
    let stop = AtomicBool::new(false);
    let searching = AtomicBool::new(false);
    for line in script.lines() {
        if handle_line(line, &mut pos, &mut out, &stop, &searching) {
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
}
