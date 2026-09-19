# Änderungen nach dem Defizit-Auftrag

Stand der Baseline: `01c3cb2`, Binary unter `../engine-arena/grokengine-01c3cb2`.
Messungen auf derselben Maschine (2 vCPU). Kein Code aus anderen Engines gelesen.

## Defizit 1 — Stellungswiederholung

**Ursache.** `position … moves …` hat nur die Endstellung behalten. Die Suche
kannte die Partie davor nicht und hatte keinen Stellungsschlüssel.

**Entscheidung.** Zobrist-Schlüssel an der Stellung, Partiehistorie in der UCI-
Schicht. Im Suchbaum zählt die zweite Wiederholung als Remis (sonst Schleifen).
An der Wurzel wird die aktuelle Stellung nie als Remis abgeschnitten — die
Wurzel liefert immer einen gesuchten Zug. Ein Zug, der die dritte Wiederholung
herbeiführt, wird im Kindknoten mit 0 bewertet. Bauernzüge und Schläge setzen
die Prüfung über die Halbzuguhr zurück.

**Umsetzung.** `src/zobrist.rs`, Historie in `uci.rs`, Prüfung in `search.rs`.

**Messung.** Test `threefold_is_chosen_when_losing`: Weiß spielt `h1g1`, Score 0.
Test `winning_side_avoids_repetition`: die gewinnende Seite wiederholt nicht.
Repro-Kommando aus dem Auftrag: `bestmove h1g1` / `score cp 0`.

## Defizit 2 — Bewertung

**Ursache.** `MAT_MG`/`MAT_EG` waren die PeSTO-Materialwerte, die PST die
„Simplified Evaluation Function“ aus dem Chess Programming Wiki. Der Kommentar
„Values are my own“ war falsch.

**Entscheidung.** Eigene Herleitung, im Kopf von `src/eval.rs` dokumentiert:
Bauer = 100, klassische Relation der Figuren mit MG/EG-Split, PST aus
geometrischen Regeln (Zentrum, Rand, 7. Reihe, Rochade-Felder, Freibauer nach
Rang). Läuferpaar, Doppelbauer, Isolani, Freibauer.

**Messung.** `startpos_eval_is_near_zero`, `extra_queen_is_winning`,
`passed_pawn_beats_blocked_pawn`. Kein Elo-Claim aus der Eval allein.

## Defizit 3 — flache Suche

**Ursache (gemessen, bevor etwas geändert wurde).** Startstellung, Release,
`go movetime 1000` auf `01c3cb2`: Tiefe 6, 168 055 Knoten in 159 ms. Kiwipete:
Tiefe 5, 681 142 Knoten in 621 ms. Blätter haben alle Legalzüge erzeugt und die
Ruhesuche hat dasselbe nochmal getan; `legal_moves` hat die Stellung kopiert;
iterative Vertiefung hat außer dem Wurzelzug nichts mitgenommen.

**Entscheidung.** Transpositionstabelle, Hashzug + Killer + History, Zugliste
pro Tiefe wiederverwendet, Ruhesuche nur Schlagzüge/Umwandlungen, PVS,
Nullzug (nicht im Schach, nicht in reinen Bauernendspielen), späte
Zugreduktion nur auf ruhige Nicht-Killer, Schachverlängerung um 1. Kein
aggressives Pruning von Schlagzügen — der Gegner hat genau daran Partien
verloren.

**Messung.** Dieselbe Maschine, neues Release:

| Stellung    | alt 1 s        | neu 1 s         |
|-------------|----------------|-----------------|
| Startpos    | Tiefe 6        | Tiefe 10        |
| Kiwipete    | Tiefe 5        | Tiefe 8         |

`grokengine bench` (8 Stellungen, feste Tiefe 6): 881 541 Knoten, 560 ms,
~1.57e6 nps. Startpos Tiefe 6: 62 219 Knoten (vorher 168 055) — der Baum ist
dünner, nicht nur der Knoten billiger.

Tiefe 10 ist nicht Tiefe 14–15. Der Abstand zum Gegner aus dem Wettkampf bleibt
offen.

## Defizit 4 — Betriebsreife

**Zeitreserve.** UCI-Option `Move Overhead` (Default 100 ms, Alias
`MoveOverhead`). `lichess-bot` setzt serverseitig zusätzlich `move_overhead`
(hier 1000 ms) und übergibt der Engine typischerweise `Move Overhead` /
`MoveOverhead`. `movetime` behält mindestens 20 ms, die Uhr mindestens
Overhead+50 ms.

**UCI während der Suche.** `isready` beantwortet der stdin-Thread sofort.
`stop` setzt die Abbruchflagge immer, nicht nur wenn schon `searching` gilt.

**50-Züge vs. Matt.** Matt wird vor der 50-Züge-Regel erkannt. Test
`mate_beats_fifty_move_rule`.

**PV.** `info`-Zeilen enthalten die ganze Hauptvariante.

**Cargo.toml.** „Helix search“ entfernt — im Code gab es das nicht.

## Defizit 5 — Nachweis

- Baseline-Binary: `../engine-arena/grokengine-01c3cb2`.
- Bench: `cargo build --release && ./target/release/grokengine bench`.
- Tests: `cargo test` (Perft unverändert, Wiederholung, Anti-Wiederholung,
  Matt vor 50 Zügen, `isready` während `go`, `stop`).
- Match neuer Stand gegen `01c3cb2`, 30+0, je zwei Partien pro Farbe,
  `../engine-arena/engine_match.py -t 30`:

  | Runde | Weiß | Schwarz | Ergebnis | Halbzüge |
  |------:|------|---------|----------|----------|
  | 1 | neu | 01c3cb2 | 1–0 | 93 |
  | 2 | 01c3cb2 | neu | 0–1 | 78 |
  | 3 | neu | 01c3cb2 | 1–0 | 51 |
  | 4 | 01c3cb2 | neu | 0–1 | 50 |

  **4–0 für den neuen Stand.** Median-Suchtiefe in den PGN-Kommentaren: neu 8–9,
  alt 4–5. Vier Partien ohne Buch sind nur ein Indiz, kein Elo. Ohne Buch
  können Partien mit derselben Farbe ähnlich verlaufen.
