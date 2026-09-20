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

# Bewertungsterme (19.09.2026)

Material und PST bleiben die selbst hergeleiteten Werte aus Defizit 2. Neu
sind drei Terme, die `cee7c62` nicht hatte. Gewichte und Geometrie stehen
im Kopf von `src/eval.rs`; nichts davon ist aus einer anderen Engine
übernommen.

**Idee.** Der Abstand zu sparkengine lag nach `cee7c62` nicht mehr in der
Suchtiefe. Ein separates Experiment (`grokengine-altpst`, PeSTO-Material plus
Simplified-Evaluation-Tabellen, sonst identischer Code) schlug `cee7c62` mit
13–7 bei gleicher Tiefe. Die alten Zahlen sind keine Option. Die Lücke sollte
sich mit eigenen Termen schließen: Mobilität, Königssicherheit, Türme auf
offenen Linien.

**Umsetzung.** Ein Paket, nicht drei isolierte Patches:

- Mobilität: Springer, Läufer, Turm, Dame zählen erreichbare Felder
  (leer oder Gegner). Kleine cp-Gewichte, Dame nur 1, damit sie die Eval
  nicht dominiert.
- Königssicherheit, nur Mittelspiel: Bauernschild auf den drei Linien vor
  einem Königsflügel (a–c / f–h). Fehlender Bauer 14, um drei Reihen
  vorgeschoben 6. Zusätzlich Tropismus: gegnerische Dame und Springer nach
  Tschebyschew-Abstand zum König. Unrochierte Könige auf d/e bleiben beim
  König-PST, sonst würde 1.e4 wie ein Loch aussehen.
- Türme: halboffene Linie (kein eigener Bauer), offene Linie (keine Bauern),
  kleiner Extra-Bonus für verdoppelte Türme auf derselben offenen Linie.

**Messung 1.** 20 Partien 1+0 gegen das eingefrorene Binary
`../engine-arena/grokengine-cee7c62`. Farbwechsel, eine Partie gleichzeitig,
martuni-Regel. Binary der neuen Eval: `../engine-arena/grokengine-eval1`.
PGNs: `../engine-arena/eval-terms-2026-09-19/`.

| | |
|---|---|
| Ergebnis | **13W 2R 5L** (14,0/20, 70 %) |
| grob Elo | **+147** für die neuen Terme |
| grober 95-%-Bereich | etwa **+8 bis +361** |
| Suchtiefe (Median der Partiemediane) | neu 10, `cee7c62` 10,5 |
| Enden | 19× normal/Matt, 1× dreifache Wiederholung |

Die Terme bleiben. Der 95-%-Bereich schließt 0 nicht ein, bei 20 Partien
ohne Buch ist das trotzdem nur ein Indiz.

**Messung 2.** Dieselbe neue Eval, 20 Partien 1+0 gegen
`../engine-arena/grokengine-altpst` (alte Tabellen, neuer Rest).
PGNs: `../engine-arena/eval-vs-altpst-2026-09-19/`.

| | |
|---|---|
| Ergebnis | **10W 5R 5L** (12,5/20, 62,5 %) |
| grob Elo | **+89** für die eigenen Terme gegen die alten Tabellen |
| grober 95-%-Bereich | etwa **−40 bis +248** (schließt 0 ein) |
| Suchtiefe (Median der Partiemediane) | neu 10, altpst 11 |
| Enden | 17× normal/Matt, 3× dreifache Wiederholung |

Ziel war, die alten Tabellen mit eigenen Mitteln einzuholen oder zu
übertreffen. Der Punktestand geht in diese Richtung; der 95-%-Bereich
lässt 0 zu. Kein Elo gegen sparkengine in diesem Auftrag.

**Bench** (8 Stellungen, Solltiefe 6), Release, dieselbe Maschine:

```
# cee7c62: 881541 Knoten, 560 ms, ~1.57e6 nps
# aktuell: 605167 Knoten, 507 ms, ~1.19e6 nps
```

Die Eval ist teurer (Strahlengänge). Startstellung `go movetime 1000`
erreicht weiter Tiefe 10.

Tests: `cargo test` grün, inklusive `rook_on_semi_open_file_beats_blocked_file`,
`open_bishop_beats_blocked_bishop`, `pawn_shield_beats_exposed_wing_king`.
Release-Build ohne Warnungen.

# Kanon und Messung (20.09.2026)

Antwort auf die Denkanstöße: `KANON.md`. Kurz: keine der 20-Partien-Serien
ohne Buch hätte einen Nulleffekt von einem kleinen Gewinn getrennt. Die
Suchparameter (Nullzug-R, LMR, Futility 250, Delta 200, Schachverlängerung,
Zeiteinteilung Rest/30) sind geerbt und waren nie in dieser Engine allein
gemessen. Die eigenen Material-/PST-Zahlen bleiben trotz 7–13 gegen die
Simplified Evaluation — das ist die eine Entscheidung gegen das Wiki;
getragen hat sie bisher nur die Term-Serie, und deren Bereich war zu weit.

Eingefrorene Baseline bleibt `../engine-arena/grokengine-a65bad9`.

## `go nodes`

UCI-Standard. Ohne das Token hat die Engine bei `go nodes 20000` weiter nach
ihrem Default gesucht (2,7 Mio. Knoten). Jetzt bricht sie bei der
angegebenen Knotenzahl ab; `movetime`/`wtime` gelten zusätzlich, wer zuerst
kommt. Tests: `node_limit_is_respected`, `parse_go_nodes`,
`binary_go_nodes_stops_near_limit`. Bench bei Solltiefe 6 unverändert
605167 Knoten — die Suche selbst hat sich dadurch nicht geändert.

Binary: `../engine-arena/grokengine-nodes`.

## Ablation Nullzug

Baustein aus, nicht nur R verändert. `USE_NULL_MOVE = false`, sonst identisch
mit `grokengine-nodes`. Binary `../engine-arena/grokengine-nonull`.

```
BOOK=openings.epd ./run_series.sh ./grokengine-nodes ./grokengine-nonull \
    80 ablation-null-2026-09-20 -n 200000
```

Ergebnis (aus `auswertung.py`, Sicht `grokengine-nodes` = mit Nullzug):

| | |
|---|---|
| Ergebnis | **36W 18R 26L** (45,0/80, 56,2 %) |
| Elo | **+44** |
| 95-%-Bereich | **−23 bis +114** (schließt 0 ein) |
| LOS | 89,8 % |
| Tiefe (Median der Partiemediane) | mit Nullzug 9, ohne 8 |
| Enden | 70× normal/Matt, 9× Wiederholung, 1× 50-Züge-Regel |
| Buch | 40 Stellungen, jede mit beiden Farben |

Kein illegaler Zug, kein Absturz. Der Nullzug bleibt an, als Indiz: gleiche
Knotenzahl, eine extra Suchtiefe, Schätzer über 50 %. Der Bereich enthält 0,
also keine Übernahme im engen Sinn von `KANON.md`. PGNs:
`../engine-arena/ablation-null-2026-09-20/`.

## Nicht übernommen: Schachzüge von LMR/Futility ausnehmen

Idee: ruhige Züge, die Schach geben, nicht reduzieren und nicht als
futil schneiden. Binary `../engine-arena/grokengine-checks` gegen
`grokengine-nodes`, gleiches Setup wie die Nullzug-Ablation.

```
BOOK=openings.epd ./run_series.sh ./grokengine-checks ./grokengine-nodes \
    80 lmr-checks-2026-09-20 -n 200000
```

| | |
|---|---|
| Ergebnis | **32W 19R 29L** (41,5/80, 51,9 %) |
| Elo | **+13** |
| 95-%-Bereich | **−54 bis +81** (schließt 0 ein) |
| LOS | 65,0 % |
| Tiefe (Median) | checks 8, nodes 9 |

Kein Beleg für einen Gewinn, eine Suchtiefe weniger bei gleicher
Knotenzahl. Der Code ist zurückgenommen. PGNs:
`../engine-arena/lmr-checks-2026-09-20/`.

## Übernommen (Indiz): Könignähe zum Freibauern

Idee: Im Endspiel soll ein Freibauer unseren König nah und den gegnerischen
fern haben. Chebyshev-Abstand, mal Rang — eigene Geometrie, keine
veröffentlichte Tabelle. Mittelspiel unverändert. Test
`king_near_passed_pawn_beats_king_far_from_it` (c2 gegen f2, gleiche King-PST).

Binary `../engine-arena/grokengine-passer` gegen `grokengine-nodes`
(ohne diesen Term, mit Nullzug). Gleiches Setup wie die Nullzug-Ablation.

```
BOOK=openings.epd ./run_series.sh ./grokengine-passer ./grokengine-nodes \
    80 passer-2026-09-20 -n 200000
```

| | |
|---|---|
| Ergebnis | **35W 19R 26L** (44,5/80, 55,6 %) |
| Elo | **+39** |
| 95-%-Bereich | **−27 bis +109** (schließt 0 ein) |
| LOS | 87,5 % |
| Tiefe (Median) | beide 9 |
| Mit Weiß / Schwarz | 22 / 40 und 22,5 / 40 |
| Enden | 65× normal/Matt, 13× Wiederholung, 2× 50-Züge-Regel |

Kein Beleg im engen Sinn. Der Term bleibt, vorläufig: Schätzer über 50 %,
keine Tiefenstrafe, beide Farben gleich. Bench Solltiefe 6: 596513 Knoten
(vorher 605167 — andere Cutoffs, nicht mehr Arbeit), ~1,11e6 nps gegen
~1,15e6 ohne den Term. PGNs: `../engine-arena/passer-2026-09-20/`.

Eingefrorene Binaries dieses Durchgangs: `grokengine-a65bad9` (Auftrag),
`grokengine-nodes` (a65bad9 plus `go nodes`), `grokengine-nonull`,
`grokengine-checks` (verworfen), `grokengine-passer` (aktueller Stand).
