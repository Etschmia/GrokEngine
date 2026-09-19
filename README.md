# Grokengine

UCI-Schachengine in Rust, von Grund auf geschrieben. Nicht geklont von
Stockfish, Martuni, Spark oder einer anderen Engine. Vorgesehener Betrieb:
UCI-Prozess, später an einen Lichess-Bot anschließbar (die Anbindung liegt
nicht in diesem Repository).

Was die Engine kann: legale Züge (Perft), iterative Alpha-Beta-Suche mit
Ruhesuche, Transpositionstabelle, Wiedererkennung von Stellungen, eigene
tapered Bewertung, UCI inklusive `Hash` und `Move Overhead`.

Was sie nicht kann: Mehrkern-Suche, Eröffnungsbuch, Syzygy, Ponder.

## Build

```bash
cargo build --release
./target/release/grokengine
```

## UCI

- `uci` → `id name`, `id author`, Optionen, `uciok`
- `isready` → `readyok` (auch während der Suche)
- `setoption name Hash value <MB>` (Default 32)
- `setoption name Move Overhead value <ms>` (Default 100; Alias `MoveOverhead`)
- `ucinewgame`
- `position startpos moves …` / `position fen <fen> moves …`
- `go movetime <ms>` / `go wtime … btime … winc … binc …` / `go depth …`
- `stop`, `quit`
- `bench` (auch als `./target/release/grokengine bench`)

Unbekannte Befehle werden ignoriert.

## Tests

```bash
cargo test
```

Perft (Startstellung 1–4, Kiwipete, Position 3, Talkchess), Matt in 1,
dreifache Wiederholung (Weiß hält mit Turm gegen Dame durch `h1g1` und Score 0),
Gewinnseite vermeidet Wiederholung, Matt schlägt die 50-Züge-Regel,
Binärtests für `isready` während `go` und für `stop`.

## Gemessen

Baseline ist Commit `01c3cb2`, Binary `../engine-arena/grokengine-01c3cb2`.
Dieselbe Maschine, Release-Build, ein Thread.

Startstellung, `go movetime 1000`:

```
# 01c3cb2: Tiefe 6, 168055 Knoten, 159 ms
# aktuell: Tiefe 10, 991768 Knoten, 488 ms
```

Kiwipete, `go movetime 1000`: Tiefe 5 → Tiefe 8.

```
cargo build --release
./target/release/grokengine bench
```

Zuletzt: 881541 Knoten, 560 ms, ~1.57e6 nps, 8 Stellungen, Solltiefe 6.

Wettkampf 5+0 gegen sparkengine (Stand `01c3cb2`, sechs Partien): 2–4. Beide
Siege durch einen Patzer des Gegners. Median-Suchtiefe damals 6 gegen 14–15.
Das ist die Ausgangslage, kein aktueller Elo.

Match neuer Stand gegen `01c3cb2`:

```
../engine-arena/engine_match.py \
  ./target/release/grokengine ../engine-arena/grokengine-01c3cb2 -t 30 -o r1.pgn
```

Vier Partien 30+0 (je zwei pro Farbe): **4–0 für den neuen Stand**.
Median-Suchtiefe in den PGN: neu 8–9, alt 4–5. Das ist ein Indiz, kein Elo.
PGN: `../engine-arena/grok_new_vs_old_r{1,2,3,4}.pgn`. Details in `CHANGES.md`.

## Offen

- Suchtiefe gegen eine stärkere Engine (14–15 im Blitz) ist nicht erreicht.
- Keine Endspieltabellen, kein Buch, ein Thread.
- Spielstärke ist nicht kalibriert; es gibt keine Elo-Angabe.
