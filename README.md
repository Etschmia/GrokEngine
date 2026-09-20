# Grokengine

UCI-Schachengine in Rust, von Grund auf geschrieben. Nicht geklont von
Stockfish, Martuni, Spark oder einer anderen Engine. Vorgesehener Betrieb:
UCI-Prozess, später an einen Lichess-Bot anschließbar (die Anbindung liegt
nicht in diesem Repository).

Was die Engine kann: legale Züge (Perft), iterative Alpha-Beta-Suche mit
Ruhesuche, Transpositionstabelle, Wiedererkennung von Stellungen, eigene
tapered Bewertung (Material, PST, Bauernstruktur, Könignähe zum Freibauern
im Endspiel, Mobilität, Königssicherheit, Türme auf offenen Linien), UCI
inklusive `Hash`, `Move Overhead` und `go nodes`.

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
- `go movetime <ms>` / `go wtime … btime … winc … binc …` / `go depth …` / `go nodes …`
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
Binärtests für `isready` während `go`, für `stop`, und dafür, dass
`go nodes` die Suche an der angegebenen Knotenzahl beendet.

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

Zuletzt: 596513 Knoten, 537 ms, ~1.11e6 nps, 8 Stellungen, Solltiefe 6.
(`a65bad9` / `grokengine-nodes` zum Vergleich: 605167 Knoten, ~1.15e6 nps
— der Freibauern-Königsterm ändert Cutoffs, nicht die Knotenkosten.
`cee7c62`: 881541 Knoten, ~1.57e6 nps.)

Wettkampf 5+0 gegen sparkengine, ohne Buch, Schiedsrichter:

- Stand `01c3cb2`, sechs Partien: 2–4.
- `cee7c62`, 20 Partien: 3–17. Median-Suchtiefe 12 gegen 14.
- `a65bad9`, 20 Partien (Serie 3): **8–12**. Median-Suchtiefe 12 gegen 15.
  Score 40 %, grob −70 Elo, 95-%-Bereich etwa −230 bis +60 (schließt
  Gleichstand ein). PGNs: `../engine-arena/match-2026-09-19-b/`.

Zwanzig Partien aus der Grundstellung sind korrelierte Stichproben, kein
kalibrierter Elo. Einordnung in `KANON.md`.

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

Buchserien, 80 Partien, 40 Stellungen je beide Farben, 200000 Knoten/Zug
(`auswertung.py`):

- Nullzug an gegen aus: **45 : 35** (+44 Elo, Bereich −23…+114, LOS 89,8 %,
  Tiefe 9 gegen 8). Indiz, bleibt an. `../engine-arena/ablation-null-2026-09-20/`.
- Könignähe zum Freibauern gegen denselben Stand ohne den Term: **44,5 : 35,5**
  (+39 Elo, Bereich −27…+109, LOS 87,5 %, Tiefe beide 9). Indiz, bleibt.
  `../engine-arena/passer-2026-09-20/`.
- Schachzüge von LMR/Futility ausnehmen: **41,5 : 38,5** (+13 Elo, Bereich
  −54…+81, Tiefe 8 gegen 9). Nicht übernommen.

Alle drei Bereiche schließen 0 ein. Details in `KANON.md` und `CHANGES.md`.

## Offen

- Keine Endspieltabellen, kein Buch in der Engine, ein Thread.
- Spielstärke ist nicht kalibriert. Was gemessen ist und was geerbt, steht
  in `KANON.md`. Neue Serien mit `BOOK=openings.epd` und `-n`.
