# Loss audit: where and by how much runs die

2026-10-15. `scripts/audit_losses.py` reads `summary.json` + `decisions.jsonl` of the bench30-a
cells for v15 `6725b5bb`, v17 `11845ce8` and v18 `d22adb59` (12 cells, abandoned). No game, no API.
Data: [samples/loss-audit.json](samples/loss-audit.json).

| | v15 | v17 | v18 (12) |
| --- | ---: | ---: | ---: |
| Losses | 28 | 30 | 11 |
| Died at a boss | 20 | 25 | 9 |
| Died at ante 5–7 | 20 | 24 | 10 |
| Median final score / target | 0.71 | 0.69 | 0.73 |
| Within 75–100% of target (close calls) | 9 | 9 | 5 |
| Median best single hand in fatal round / target | 0.39 | 0.31 | 0.32 |
| Deaths to The Water (0 discards) | 6 | 6 | 4 |

Median margin on cleared blinds (scored / target) by ante, v17:
1.34, 1.50, 1.63, 1.36, 1.22, 1.18, 1.09 (antes 1–7). v15 and v18 have the same shape.

## Reading

- **It is mainly a scaling problem.** Margins peak at ante 3 and then erode every ante: the
  build's power grows slower than blind targets. In the round that kills us, the best single hand
  is about a third of the target, so better sequencing could not have saved it. These deaths come
  from earlier shop and build decisions.
- **Tactics still matter for about 30%.** Roughly 9 of 30 losses end at 75–100% of the target.
  Those are close calls where better round planning or a stronger final hand could have saved
  the run.
- **The Water is a specific weakness.** It caused about 20% of all deaths in every version, many
  more than any other boss. Our hand-building depends on discards; with none, the lineup falls
  apart. The boss is visible from the start of the ante, so this is plannable (skip or reroll
  boss decisions, buy hand-size or flat-scoring jokers, keep a scoring hand that does not need
  discards).

## Implication

The next lever is **shop and build value measured as future power against the target curve**,
for example the projected ante+1/+2 boss score of each purchase, plus a targeted fix for
no-discard bosses. Better tactical mechanics (the Raised Fist and Pareidolia patch) affect at most
the close-call third of losses.
