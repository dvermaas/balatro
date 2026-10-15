# V15 benchmark continuation — 13 October 2026

The user requested continuation of Claude's interrupted benchmark. The current
decision version is **v15 `6725b5bb`**, set **`bench30-a`**, Red Deck, White Stake,
model setting `jev-latest`. No decision source was changed for this continuation.

**Completed: 30/30 seeds, 2 verified wins, mean 14.90 blinds cleared.** V13,
the strongest earlier completed version on this metric, averaged 14.47. The
observed gain is small and uncertain. The panel is paused with no queued seeds
and the game at MENU. The victory-popup bug was repaired and checked separately
with a controlled live UI test after completion.

## Recovery evidence

Both the panel (port 8765) and game (port 12346) were stopped. The existing v15
results were:

| Game | Seed | Recorded outcome before recovery |
| --- | --- | --- |
| 1 | `7X7WSDCX` | Lost, ante 5 / round 15 |
| 2 | `6CAXN9I6` | Lost, ante 5 / round 15 |
| 3 | `BAPQJMY5` | In progress, ante 9 / round 24, one timeout |

Game 3 was already a confirmed victory. In
`runs/20261012-224505-BAPQJMY5/decisions.jsonl`, event 1733 records Verdant Leaf's
100,000-chip target beaten with 122,980 chips. The raw gamestate saved for event
1741 explicitly has `won: true`, seed `BAPQJMY5`, ante 9 and round 24. A later
planet-pack action timed out. The win is established by the successful final-boss
clear and subsequent nonterminal ante-9 state, corroborated by the game flag.
The flag alone is insufficient: the audit below found false positives on losses.

The interrupted summary was preserved as `summary.pre-resume.json` in that run's
directory. Its existing summary now records `won`, with a recovery annotation
identifying the source state. Recovery information and a `run_end` event were
appended to the decision log. Original decisions and raw states were preserved.
The end timestamp uses the last recorded event before the interruption, excluding
the overnight downtime. This result stops at the confirmed victory; no further
endless-mode progress is claimed.

## Continuation

The installed game was launched with the existing fast-mode and audio settings.
After confirming its state was MENU, `scripts/resume_benchmark.py` started the
panel and restored only games **4–30**, beginning with `TBIR2XMU` at 06:56 local
time. The original first three seeds were not replayed. Panel delay remains 0.3 s.
The existing early-stop rules remain enabled. Random runs after the benchmark
are disabled so this continuation cannot spill into unrelated games.

The launcher defaults to a read-only recovery plan, verifies the expected source
version, refuses duplicate or mismatched seed results, requires an explicit saved
win for recovery of an unfinished result, and refuses to start if the panel port
is occupied or the game is not at MENU. Five offline recovery tests passed. No
test called JEV or sent a game action.

### Observed execution issue

Game 10 (`20261013-070500-V98SL5G2`) logged a recoverable shop error. Events
4084–4088 selected `replace_5_1`, sold Dusk from a six-joker lineup, then failed to
buy Blue Joker because the game reported 5 cards in 5 slots. This indicates a
slot-capacity problem in the proposed swap, not a service outage. The bot
continued with another shop decision and finished at ante 6 / round 18. This
behavior was recorded rather than changing decision code during the experiment.

### False victory flags discovered during continuation

The panel initially reported game 12 (`W4YA1M4U`) as another win. An audit found
that its final play reached GAME_OVER with only 32,942 / 300,000 chips against
Violet Vessel. Its last preterminal raw state had `won: false`. It is a loss.

The installed patched game source, `lovely/dump/functions/state_events.lua`,
sets `G.GAME.won = true` at the end of an ante-8-or-later boss round **before**
branching on `game_over` (lines 110–114 as inspected). BalatroBot directly exports
that flag, and the panel trusts it when recording GAME_OVER. Thus reaching and
losing the final boss can be reported as a victory.

The same audit disproved v11's recorded win, run `20261012-192347-2RGEX6EN`:
14,497 / 100,000 chips against Verdant Leaf, GAME_OVER at ante 8 / round 24,
last preterminal `won: false`, and no successful final-boss clear. V15 game 3
remains verified by its 122,980 / 100,000 clear and ante-9 continuation. It is the
first verified victory among these recorded benchmark versions.

The benchmark's strategy and game code remained unchanged. All reported wins
were audited and original summaries preserved before result corrections.
Cleared-blind counts, and therefore the existing stopping decisions, do not
depend on these erroneous win labels.

## Result

All 30 seeds finished without triggering early stop. Completion event 6054 says
`Benchmark bench30-a complete`; the panel reports 30 finished runs, no current
run, no remaining seeds, and paused mode. See the saved
[completion evidence](samples/v15-benchmark-completion.json) and
[per-seed results, win audit and paired calculations](samples/v15-benchmark-results.json).

V15 finished with **2/30 verified wins**, mean **14.90 cleared blinds**, mean
ante **5.47**, and median ante **6**. The two wins are `BAPQJMY5` (game 3) and
`X62IZYJ4` (game 17); their final-boss evidence is recorded above and below.

| Baseline | Baseline mean blinds | V15 minus baseline | Paired SE | Better / tied / worse seeds |
| --- | ---: | ---: | ---: | ---: |
| v13 `ac9101d7` | 14.47 | +0.43 | 1.05 | 12 / 9 / 9 |
| v12 `d1c693b2` | 13.90 | +1.00 | 0.96 | 16 / 6 / 8 |
| v11 `8cfd03bf` | 11.97 | +2.93 | 1.19 | 20 / 5 / 5 |
| v9 `d8045b3c` | 12.77 | +2.13 | 1.00 | 18 / 8 / 4 |

These comparisons pair the same 30 seeds using the panel's existing progress
metric; SE is the sample standard deviation of within-seed differences divided
by the square root of the pair count. V15 has the highest observed average among
these versions, but the +0.43 gain over v13 is smaller than its paired SE. This
does not establish general superiority: the sample is small, and development has
repeatedly used this fixed set rather than a held-out evaluation.

Excluding interrupted game 17 leaves 29 pairs: v15 averages **14.59**, v13
**14.41**, and their difference is **+0.17, SE 1.06** (11 better, 9 tied, 9 worse).
The other differences become +0.79 against v12, +2.45 against v11, and +1.86
against v9. Capping all progress at the 24 blinds needed to win leaves every
comparison unchanged: neither earlier v15 win cleared another endless blind.

The panel's raw average duration includes game 17's long user-requested pause;
it must not be interpreted as active runtime. Resumption metadata preserves the
idle interval. No v16 or additional benchmark was started.

## Recovery after the user's pause

The user paused during game 17, `X62IZYJ4`, and later requested continuation.
The game had closed, but its autosave matched the last recorded outcome (event
7895): SELECTING_HAND, ante 2, round 6, $25, zero chips, four hands and three
discards remaining, and the same four jokers. The autosave was backed up as
`runs/_checkpoints/20261013-072352-X62IZYJ4-autosave.jkr` before loading, SHA-256
`0B1FB3F7E6CBCF59134B501AFC059DBB918F8156BCB8A793D0094257B43EDF49`.

The original run directory and event sequence were reattached, and games 18–30
were queued. No seed was replayed. The bot's shop history could be reconstructed
exactly for four of its nine earlier plays. The five forced plays had not saved
full hands, so their missing history was not fabricated. This is a limitation for
game 17; final paired comparisons also exclude this interrupted seed as a
sensitivity check above. The summary records the resumption, restored-history count and
idle interval; `summary.pre-attachment.json` preserves the pre-resumption summary.

Before continuing, the two conclusively false victory summaries (v11 game 9 and
v15 game 12) were corrected to losses. Each original summary is preserved as
`summary.pre-win-audit.json`, with evidence in `win_audit` and an appended
`result_correction` event. Raw states and original decisions were not altered.
The panel was restarted with one verified v15 win. No strategy source changed.
The recovery checks now have eight passing tests, and the read-only win audit has
four passing tests.

## Victory cleanup repair between games 18 and 19

The recovered game 17 genuinely won: Amber Acorn, 137,034 / 100,000 chips, event
8521. Under the old controller it then entered ante 9 and lost an extra endless
round. The user observed actions continuing beneath the victory popup.

The cause is distinct from the false win flag: BalatroBot's `play` endpoint
returns a real victory as ROUND_EVAL, while our controller previously always
cashed out that state and recorded completion only at GAME_OVER. Its next shop
actions could therefore run while the victory overlay was appearing.

After game 18 finished, the panel was paused at MENU and restarted with a
controller-only repair in `src/jev_bot.py`. It now records a win when ROUND_EVAL
has `won=True`, ante > 8, the previous blind was a boss, and actual chips meet its
target. It saves that winning raw state, completes the result, and calls `menu`
without cashing out. If menu cleanup fails, further steps only retry `menu`;
they cannot ask JEV, replay the finished hand, or create another result.
GAME_OVER at ante 8 is a loss even if the game incorrectly says `won=True`.
Ambiguous victory evidence pauses for review.

The nine hashed decision files remain v15 `6725b5bb`; scoring, candidate choices,
shop policy, JEV settings and the seed set are unchanged. Games 19–30 use the
correct terminal-win handling, with original early-stop rules intact. Earlier
verified wins' extra endless progress is preserved in their raw logs; capping
progress at 24 blinds leaves the final comparisons unchanged.
All **32 offline tests passed**, including six completion tests covering genuine
wins, false win flags, normal cash-out, ambiguous evidence, preserved queues and
idempotent menu retry.

## Separate live victory-popup check

Games 19–30 all lost, so the repaired controller did not encounter another
natural victory during the benchmark. After all 30 results were recorded, a
separate check exercised the actual game's victory overlay and menu endpoint.
It made no JEV calls and used an in-memory recorder, so it added no benchmark run.

The first attempt replayed recorded game-17 actions from the preserved checkpoint.
It stopped at the first public-state mismatch: cash-out event 7938 returned $41
instead of the recorded $39, after 11 matched actions. It returned to MENU. This
replay did **not** pass; the [failure evidence](samples/v15-victory-cleanup-check.json)
is preserved, and the cause of that cash-out difference was not established.

The subsequent controlled fixture loaded the checkpoint and explicitly set ante
8, round 24 and chips equal to the current boss target (1,600), then played one
hand to trigger the real victory popup. These are synthetic test overrides,
**not a performance result or a third win**. The production `on_round_eval`
handler recorded exactly one in-memory victory and issued exactly one action:
`menu`. It did not cash out, enter a shop or play another hand. Before/after
screenshots visibly confirm the YOU WIN overlay followed by the clean main menu.

- [Passed controlled-check evidence](samples/v15-victory-cleanup-controlled-check.json)
- [Victory popup before cleanup](samples/v15-victory-cleanup-controlled-check-before.png)
- [Main menu after cleanup](samples/v15-victory-cleanup-controlled-check-after.png)
- Reproducible check: `scripts/check_victory_cleanup.py --run --controlled-fixture`

After the check, the panel still reports v15 `6725b5bb`, 30 finished runs, 2 wins,
no current run, no queued seeds, and paused mode. The game is at MENU. The original
checkpoint, benchmark decisions and benchmark raw states were preserved.
