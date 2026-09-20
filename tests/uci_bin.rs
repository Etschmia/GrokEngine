//! Launch the real `grokengine` binary and speak UCI on its stdin/stdout.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn run_script(script: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_grokengine"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch grokengine binary");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(script.as_bytes())
        .expect("write uci script");
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .expect("wait for grokengine");
    assert!(
        output.status.success(),
        "engine exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn bestmove_token(out: &str) -> &str {
    out.lines()
        .rev()
        .find(|l| l.starts_with("bestmove "))
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("")
}

const STARTPOS_LEGAL: &[&str] = &[
    "a2a3", "a2a4", "b1a3", "b1c3", "b2b3", "b2b4", "c2c3", "c2c4", "d2d3", "d2d4", "e2e3",
    "e2e4", "f2f3", "f2f4", "g1f3", "g1h3", "g2g3", "g2g4", "h2h3", "h2h4",
];

#[test]
fn binary_uci_startpos_movetime() {
    let script = "uci\nisready\nucinewgame\nposition startpos\ngo movetime 200\nquit\n";
    let t0 = std::time::Instant::now();
    let out = run_script(script);
    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "engine hung: {elapsed:?}\n{out}"
    );
    assert!(out.contains("uciok"), "{out}");
    assert!(out.contains("readyok"), "{out}");
    let mv = bestmove_token(&out);
    assert!(
        STARTPOS_LEGAL.contains(&mv),
        "illegal or missing bestmove '{mv}' in:\n{out}"
    );
}

#[test]
fn binary_go_nodes_stops_near_limit() {
    let script = "uci\nisready\nposition startpos\ngo nodes 20000\nquit\n";
    let t0 = Instant::now();
    let out = run_script(script);
    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_secs(3),
        "go nodes hung: {elapsed:?}\n{out}"
    );
    let mv = bestmove_token(&out);
    assert!(
        STARTPOS_LEGAL.contains(&mv),
        "illegal or missing bestmove '{mv}' in:\n{out}"
    );
    let nodes = out
        .lines()
        .filter(|l| l.starts_with("info "))
        .rev()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            while let Some(t) = it.next() {
                if t == "nodes" {
                    return it.next().and_then(|n| n.parse::<u64>().ok());
                }
            }
            None
        })
        .expect("missing nodes in info");
    assert!(
        nodes > 0 && nodes <= 20_000 + 512,
        "go nodes 20000 searched {nodes}:\n{out}"
    );
}

#[test]
fn binary_uci_fen_clock() {
    let fen = "rnbqkb1r/pppp1ppp/5n2/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 4 3";
    let script = format!(
        "uci\nisready\nposition fen {fen}\ngo wtime 5000 btime 5000 winc 0 binc 0\nquit\n"
    );
    let t0 = std::time::Instant::now();
    let out = run_script(&script);
    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_secs(3),
        "engine hung on clock go: {elapsed:?}\n{out}"
    );
    let mv = bestmove_token(&out);
    assert!(
        grokengine::Position::from_fen(fen)
            .unwrap()
            .move_from_lan(mv)
            .is_some(),
        "bestmove '{mv}' is not legal in {fen}\n{out}"
    );
}

#[test]
fn binary_isready_during_search() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_grokengine"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch grokengine binary");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    writeln!(stdin, "uci").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line.starts_with("uciok") {
            break;
        }
    }

    writeln!(stdin, "position startpos").unwrap();
    writeln!(stdin, "go movetime 2000").unwrap();
    writeln!(stdin, "isready").unwrap();
    stdin.flush().unwrap();

    let t0 = Instant::now();
    let mut ready_at = None;
    let mut got_best = false;
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        if line.starts_with("readyok") {
            ready_at = Some(t0.elapsed());
        }
        if line.starts_with("bestmove ") {
            got_best = true;
            break;
        }
        if t0.elapsed() > Duration::from_secs(6) {
            break;
        }
    }
    let _ = writeln!(stdin, "quit");
    drop(stdin);
    let _ = child.wait();

    let ready_at = ready_at.expect("isready was not answered");
    assert!(
        ready_at < Duration::from_millis(500),
        "isready blocked until search finished: {ready_at:?}"
    );
    assert!(got_best, "search did not return a bestmove");
}

#[test]
fn binary_stop_returns_quickly() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_grokengine"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch grokengine binary");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    writeln!(stdin, "uci").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line.starts_with("uciok") {
            break;
        }
    }

    writeln!(stdin, "position startpos").unwrap();
    writeln!(stdin, "go movetime 8000").unwrap();
    stdin.flush().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    writeln!(stdin, "stop").unwrap();
    stdin.flush().unwrap();

    let t0 = Instant::now();
    let mut got_best = false;
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        if line.starts_with("bestmove ") {
            got_best = true;
            break;
        }
        if t0.elapsed() > Duration::from_secs(2) {
            break;
        }
    }
    let elapsed = t0.elapsed();
    let _ = writeln!(stdin, "quit");
    drop(stdin);
    let _ = child.wait();
    assert!(got_best, "stop did not produce bestmove");
    assert!(
        elapsed < Duration::from_millis(800),
        "stop took too long: {elapsed:?}"
    );
}
