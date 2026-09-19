# Grokengine

UCI-Schachengine in Rust, von Grund auf geschrieben. Nicht geklont von
Stockfish, Martuni, Spark oder einer anderen Engine. Vorgesehener Betrieb:
UCI-Prozess, später an einen Lichess-Bot anschließbar (die Anbindung liegt
nicht in diesem Repository).

Was die Engine kann: legale Züge (Perft), iterative Alpha-Beta-Suche mit
Ruhesuche, Transpositionstabelle, Wiedererkennung von Stellungen, eigene
tapered Bewertung (Material, PST, Bauernstruktur, Mobilität,
Königssicherheit, Türme auf offenen Linien), UCI inklusive `Hash` und
`Move Overhead`.

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

Zuletzt: 605167 Knoten, 507 ms, ~1.19e6 nps, 8 Stellungen, Solltiefe 6.
(`cee7c62` zum Vergleich: 881541 Knoten, 560 ms, ~1.57e6 nps — die Eval
ist teurer, Startpos `go movetime 1000` bleibt bei Tiefe 10.)

Wettkampf 5+0 gegen sparkengine (Stand `01c3cb2`, sechs Partien): 2–4.
Nach `cee7c62`, 20 Partien 5+0: 3–17. Median-Suchtiefe zuletzt 12 gegen 14.
Das ist die Ausgangslage gegen den Gegner, kein aktueller Elo.

Match `cee7c62` gegen `01c3cb2`, vier Partien 30+0: **4–0**. Details in
`CHANGES.md`.

Bewertungsterme (Mobilität, Königssicherheit, offene Turmlinien), 20 Partien
1+0, Farbwechsel, eine Partie gleichzeitig:

- gegen `grokengine-cee7c62`: **13W 2R 5L** (14,0/20). Grob +147 Elo,
  95-%-Bereich etwa +8 bis +361. PGN: `../engine-arena/eval-terms-2026-09-19/`.
- gegen `grokengine-altpst` (alte Tabellen, sonst gleicher Code):
  **10W 5R 5L** (12,5/20). Grob +89 Elo, 95-%-Bereich etwa −40 bis +248
  (schließt 0 ein). PGN: `../engine-arena/eval-vs-altpst-2026-09-19/`.

Zwanzig Partien ohne Buch sind ein Indiz, kein kalibrierter Elo.

## Offen

- Kein neues Match gegen sparkengine mit den Bewertungstermen.
- Keine Endspieltabellen, kein Buch, ein Thread.
- Spielstärke ist nicht kalibriert; die Elo-Zahlen oben gelten nur für
  die genannten 20-Partien-Serien.
