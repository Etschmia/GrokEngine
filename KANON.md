# Gemessen, geerbt, und was die Zahlen hergeben

Antwort auf die beiden Denkanstöße vom 20.09.2026. Kein Code aus anderen
Engines gelesen.

## Denkanstoß 1 — Was die Messungen hergeben

Keine der drei Serien dieses Wochenendes hätte einen **Nulleffekt** sicher
von einem echten, kleinen Gewinn unterschieden.

| Messung | Was sie tragen kann | Was sie nicht tragen kann |
|---|---|---|
| 4 Partien 30+0, Suche neu gegen `01c3cb2`, 4–0 | Ein großer Effekt (Tiefe 4–5 → 8–9). Ein No-Op geht nicht 4–0. | Elo, Stabilität, irgendeine Änderung unter ~150 Elo. |
| 20 Partien 1+0, Eval-Terme gegen `cee7c62`, 14–6, Bereich +8…+361 | Ein Indiz, dass das Paket nicht harmlos war. 0 liegt knapp außerhalb. | Einen 30-Elo-Gewinn von 0 trennen. Ohne Buch sind die Partien korreliert, der Bereich ist zu eng angegeben. |
| 20 Partien 5+0 gegen sparkengine, 8–12, Bereich −230…+60 | Stimmungsbild: nicht mehr 3–17. | Dass der Rückstand kleiner ist. Gleichstand liegt im Bereich. |

Woran ich einen Nulleffekt erkennen würde: an einem Vertrauensbereich, der
um 0 liegt und eng genug ist, dass der Effekt, den ich behaupte, nicht
hineinpasst. Dafür braucht man unabhängige Partien (Buch) und genug davon,
dass die Streuung den Bereich zusammendrückt. 20 Partien aus der
Grundstellung tun das nicht. Die eigene 14–6-Serie gegen `cee7c62` wäre
unter dieser Latte **kein** Grund gewesen, die Terme zu übernehmen — sie
war ein Grund, sie nicht sofort wegzuwerfen.

Grenze zwischen Indiz und Übernahme, ab hier:

- **Indiz.** Buch, feste Knotenzahl, mindestens ein Durchlauf der 40
  Stellungen (80 Partien, jede Farbe). Punkteschätzer deutlich von 0,5 weg,
  Bereich darf 0 noch enthalten. Die Änderung bleibt vorläufig und steht
  so in der Doku.
- **Übernahme.** Dasselbe Setup. Entweder der 95-%-Bereich schließt 0 aus,
  oder LOS ≥ 95 % *und* der Bench bricht nicht ein. Sonst zurück.
- **Ausnahme für grobe Defekte.** 4–0 plus gemessene Tiefe 4→8 darf eine
  Suche-Überholung als rauchenden Test bestehen. Danach trotzdem eine
  Buchserie, bevor daraus Elo wird.

Partien sind billig, Irrtümer teuer. Die teuren Irrtümer hier wären: eine
geerbte Zahl behalten, weil 20 korrelierte Partien „gepasst“ haben; oder
eine Änderung verwerfen, weil vier Niederlagen am Ende einer Serie ohne
Buch stehen.

Die letzten vier Partien von Serie 3 sind genau so ein Fall. Ohne Buch ist
Weiß bei mir immer 1.e4, Weiß beim Gegner immer 1.Nf3. Runde 17 ist die
Linie von Runde 3 (1.e4 Nc6 2.d4 Nf6 3.d5), Runde 18 die von Runde 2/10
(Nf3 d5 d4 Nf6 Nc3 Nc6 Ne5), Runde 19 die Hauptlinie 1.e4 Nc6 2.d4 d5 3.e5
(Runden 1, 5, 9, 15), Runde 20 die Jobava-ähnliche Linie von Runde 6.
In Runde 18 und 19 steht zwischendurch `+0.00` auf hoher Tiefe in schon
verlorener Stellung: die Zweifach-Wiederholung im Baum zählt 0, ich biete
Remis an, der Gegner nimmt es nicht. Das ist nicht die Ursache — die
Partien waren vorher verloren. Vier Niederlagen in Folge sind bei 20
korrelierten Stichproben kein neuer Defekt.

Was Sicherheit kostet: eine Nacht, 80 Partien, `BOOK=openings.epd` und
`-n`. Das ist die Untergrenze, nicht Stockfish-SPRT.

## Denkanstoß 2 — Woher das Wissen stammt

Beide Engines sehen gleich aus, weil der Korpus gleich ist. Der harte
Beleg war schon da: die PeSTO-Materialwerte und die Simplified-Evaluation-
PST, Zahl für Zahl, unter dem Satz „Values are my own“. Das Kopierverbot
gilt für Code. Es gilt nicht für das, was im Training als *die* Schach-
programmierung vorkommt.

### Was gemessen ist

- Stellungswiederholung im Suchbaum (Tests: haltendes Remis, Gewinnseite
  wiederholt nicht). Zweifach-Wiederholung *im Baum* als 0, weil sonst
  Schleifen. Das ist eine Entscheidung plus Repro, keine Elo-Messung.
- Matt vor der 50-Züge-Regel (Test).
- Suche-Überholung gegen `01c3cb2`: Tiefe und 4–0 bei 30+0. Groß, grob.
- Eigene Material-/PST-Zahlen gegen die übernommenen Tabellen: 7–13
  *dagegen*. Die Terme danach (Mobilität, Königsschild, offene Turmlinien)
  als Paket gegen `cee7c62`: 14–6, und 12,5–7,5 gegen dieselben alten
  Tabellen. Indiz, kein SPRT.
- Knotenkosten der Terme: Bench 1,57 Mio → 1,19 Mio nps, Solltiefe 6
  unverändert 605167 Knoten. Gemessen, einmal.

### Was geerbt ist und nie in dieser Engine allein gemessen wurde

Alles in `search.rs` unter den Konstanten `USE_NULL_MOVE`, `USE_LMR`,
`FUTILITY_MARGIN`, `DELTA_MARGIN`, plus ein paar Zahlen ohne Konstante:

| Baustein | Zahl / Regel | Herkunft |
|---|---|---|
| Nullzug | R = 2 + (Tiefe ≥ 6), ab Tiefe 3, nicht im Schach, nicht in reinen Bauernendspielen | Kanon |
| LMR | r = 1, extra bei ≥ 8 schon gesuchten Zügen und Tiefe ≥ 5; nur ruhig, nicht Killer | Kanon |
| Futility | Margin 250, nur Tiefe ≤ 1 | Kanon |
| Delta in der Ruhesuche | stand + Opfer + 200 | Kanon |
| Schachverlängerung | +1, ungedeckelt | Kanon |
| Zeiteinteilung | Rest / 30 + 4/5 Inkrement | Kanon |
| History | +Tiefe², Deckel 100000 | Kanon |
| Killer | zwei Slots | Kanon |
| MVV-LVA | Opfer − Angreifer/16 | Kanon |
| PVS | erster Zug volles Fenster | Kanon |
| TT | immer ersetzen, tiefere bleibt | Kanon |
| Mobilitätsgewichte | 2/3/2/1 (S/L/T/D) | selbst gesetzt, nicht abliert |
| Schild / Tropismus | 14 / 6 / 3 / 2 | selbst gesetzt, nicht abliert |
| Offene/halboffene Linie | 16/12, 8/4 | selbst gesetzt, nicht abliert |

Es gibt **keine** Aspirationsfenster. Die Frage danach geht ins Leere: der
Baustein ist nicht da, also auch nicht gemessen.

Die Eval-Gewichte nach Defizit 2 sind „eigene Herleitung“ im Sinne von
nicht abgeschrieben. Sie sind trotzdem keine Messung. Eine Zahl, die ich
im Kopf aus Geometrie gebaut habe, ist keine ablatierte Zahl.

### Wo der Kanon nicht zu dieser Gestalt passt

Mailbox, ein Kern, teure Bewertung (Strahlengänge auf jedem Blatt). Der
Kanon ist an Bitboards, Millionen billiger Knoten und später NNUE
gewachsen.

- Jeder eingesparte Knoten ist hier mehr wert als dort, weil der Knoten
  teurer ist. Aggressive Reduktionen (Nullzug, LMR, Futility) müssten
  *eher* greifen — oder die Bewertung muss billiger werden. Beides ist
  eine Messung, kein Glaubenssatz.
- Schachverlängerung ohne Deckel explodiert teurer, wenn der Knoten teuer
  ist.
- Delta 200 und Futility 250 sind Bauernmaß (Bauer = 100). Die Skala passt;
  ob die Margen zu dieser ungenauen Eval passen, weiß ich nicht.
- Inkrementelle Bewertung, Bauernhash, billigere Damenmobilität: das sind
  die Hebel, die der Kanon für *diese* Gestalt nahelegt. Keiner davon ist
  eingebaut. Serie 3 blieb bei Tiefe 12 gegen 15, trotz stärkerer Eval.
  Die neuen Terme haben ein Viertel der nps gekostet und das einmal mit
  Ja beantwortet. Die Frage „wie teuer darf ein Knoten sein“ ist offen.

### Eine Entscheidung gegen das Wiki

Die eigenen Material-/PST-Zahlen. Das Wiki und der eigene Test sagten:
Simplified Evaluation schlägt das 13–7. Ich habe die Tabellen trotzdem
nicht zurückgeholt, sondern Terme darübergesetzt, die geometrisch zu
denselben Regeln gehören. Begründung: die Simplified Evaluation ist eine
Einstiegshilfe, kein Ziel. Die alten Zahlen zurückzunehmen wäre die
bequemere Kopie gewesen. Ob die eigenen Terme das wirklich eingeholt
haben, ist mit 20 Partien ohne Buch nicht belegt — 12,5–7,5 gegen altpst,
Bereich schließt 0 ein. Die Entscheidung steht, die Messung nicht.

Zweite, kleinere: kein aggressives Schlagzug-Schnitt in der Ruhesuche
(nur Delta). Begründung in CHANGES.md, Defizit 3: der Gegner hat genau
daran Partien verloren. Das ist eine Anekdote, keine Ablation. Sie bleibt
als Absicht, bis eine Ablation sie trägt oder widerlegt.

Wenn das Wiki zu Nullzug-R, LMR-Formel oder Futility-Marge das Gegenteil
der Konstanten oben sagt, habe ich bisher keine eigene Zahl, mit der ich
widersprechen könnte. Genau das soll die Ablation ändern.

## Was es braucht, damit sich das ändert

1. `go nodes` in der Engine (UCI-Standard, hier die Messlücke).
2. Eine Ablation pro Baustein, gleiche Knotenzahl, Buch, genug Partien,
   Unsicherheit daneben.
3. Neue Terme und neue Reduktionen nur noch gegen den vorigen Stand,
   dasselbe Setup. Baseline `grokengine-a65bad9` bleibt eingefroren.

## Ablation: Nullzug

Der Baustein, nicht die Reduktionstiefe. Binary `grokengine-nonull` ist
derselbe Stand wie `grokengine-nodes` (a65bad9 plus `go nodes`), nur
`USE_NULL_MOVE = false`.

```
BOOK=openings.epd ./run_series.sh ./grokengine-nodes ./grokengine-nonull 80 ablation-null-2026-09-20 -n 200000
```

80 Partien, jede der 40 Buchstellungen einmal je Farbe, 200000 Knoten/Zug.
Sicht `grokengine-nodes` (Nullzug an) gegen `grokengine-nonull`:

| | |
|---|---|
| Ergebnis | **36W 18R 26L** (45,0/80, 56,2 %) |
| Elo | **+44** |
| 95-%-Bereich | **−23 bis +114** (schließt 0 ein) |
| LOS | 89,8 % |
| Tiefe (Median) | mit Nullzug 9, ohne 8 |
| Enden | 70× normal/Matt, 9× Wiederholung, 1× 50-Züge-Regel |

Das ist ein **Indiz**, keine Übernahme nach der Latte oben: der Schätzer
liegt über 50 %, der Bereich enthält 0, LOS unter 95 %. Der Nullzug bleibt
trotzdem an — vorläufig, weil dieselbe Knotenzahl eine extra Suchtiefe
kauft und der Punktestand nicht in die andere Richtung zeigt. Eine
andere Reduktionstiefe (nur R) ist damit nicht gemessen. PGNs:
`../engine-arena/ablation-null-2026-09-20/`.

Zwei Verbesserungen nach derselben Latte, beide 80 Partien mit Buch und
`-n 200000` gegen `grokengine-nodes`:

- Schachzüge von LMR/Futility ausnehmen: **41,5 : 38,5** (+13 Elo, Bereich
  −54…+81, LOS 65 %, Tiefe 8 gegen 9). **Nicht übernommen.**
- Könignähe zum Freibauern (Endspiel, eigene Geometrie): **44,5 : 35,5**
  (+39 Elo, Bereich −27…+109, LOS 87,5 %, Tiefe beide 9). **Indiz, bleibt
  vorläufig.** Details in `CHANGES.md`.
