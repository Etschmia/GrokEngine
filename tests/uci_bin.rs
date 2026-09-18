//! Launch the real `grokengine` binary and speak UCI on its stdin/stdout.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

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
