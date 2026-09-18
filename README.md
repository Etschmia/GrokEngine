# Grokengine

Original UCI chess engine written from scratch in Rust. It is meant to be
driven as a standard UCI process and can later be wired into a Lichess bot
(that wiring is not part of this repository).

This is not a clone of Stockfish, Martuni, Spark, or any other engine.

## Build

```bash
cargo build --release
./target/release/grokengine
```

## UCI

Speaks the Universal Chess Interface on stdin/stdout:

- `uci` → `id name`, `id author`, `uciok`
- `isready` → `readyok`
- `ucinewgame`
- `position startpos moves …` / `position fen <fen> moves …`
- `go movetime <ms>` / `go wtime … btime … winc … binc …` / `go depth …`
- `stop`, `quit`

Unknown commands are ignored.

## Tests

```bash
cargo test
```

Perft on the standard start position (depths 1–4) and Kiwipete, plus a unique
mate-in-1 search test, cover the shipped move generator and search.
