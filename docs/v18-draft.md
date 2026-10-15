# V18 draft: take usable economy and value an early joker generator

The user requested iterations on particular weak seeds and saved positions,
including the ante-1 loss, before recommending another version. This draft
builds on v17 and the [new tutorial review](ultimate-guide-lessons.md).

Decision-source version: **d22adb59**. The changes are in `decisions.py`,
`tarots.py`, and `prompts.py`. JEV remains the decision model. No seed-specific
rule or future draw order enters the policy.

## Recommended changes

1. **Explicit cash transactions.** Calculate Hermit/Temperance payouts after
   paying the card's price, including net profit and final cash/interest.
   If a shop has an affordable cash tarot with positive immediate net profit
   and an available consumable slot, add a narrow JEV choice: buy and use it,
   or continue shopping. This is batched with the normal item questions.
   JEV can decline; the existing item scores cannot override that decline.
   Taking the transaction executes buy, finds the acquired card in the
   returned inventory, and uses it before further shopping. Pack cash picks
   receive a focused value question because they compete with other free
   choices. Loss-making and break-even purchases get no profit-choice shortcut.
2. **Riff-raff's actual available capacity.** Show how many Common jokers can
   be generated at the next blind, given the slots left after taking it.
   Handle a replacement and a Negative edition. The generated cards' effects
   remain unknown, and their scoring is not fabricated. This addresses an
   observed empty-lineup skip without changing the 1.5/4 free-joker cutoff.

V17 hand play, readiness estimates, build selection, ordinary shopping rules
and ordinary consumable timing remain the basis of this draft. Conditional
readiness and delayed Hermit/Temperance use are separate future investigations.

## Why these changes, and what the iterations rejected

V17 `2RGEX6EN` paid for its first Buffoon pack, rated Riff-raff 1.12/4 with an
empty lineup, skipped both choices, and died at the ante-1 boss with two blinds
cleared. Its exact pack decision is event 6768 in
`20261014-140906-2RGEX6EN`.

The tutorial audit also found 49 Hermit/Temperance decision exposures in the
full benchmark, none selected. These include repeated offers, so they are
not 49 unique missed cards. At XUTRVN1Y event 6544, buying the $3 Hermit from
$21 and using it would leave $36; the bot instead left the shop.

We tested three repetitions per question variant using the exact recorded
public requests, preserving their original recent-hand measurements:

- Numeric cash facts alone improved some ratings but did not reliably get
  profitable transactions through the shop's existing selection thresholds.
- A focused cash question improved ratings further, but still undervalued
  taking cash *before* buying something else in the same shop.
- The explicit cash-before-shopping choice selected all three profitable
  development examples in all three repetitions. The loss-making example
  stayed low-rated and got no profit shortcut.
- Directly choosing a free joker versus skipping also recovered Riff-raff,
  but the smaller capacity annotation alone already did so in all three
  repetitions. The broader change to pack decision format was not adopted.

Eight additional recorded decisions included more cash-pack alternatives,
a profitable strong-seed shop, and a break-even Hermit. The profitable shop
choices repeated; the break-even control retained the old preferred item.
Several pack choices switched from enhancements to cash. That is evidence of
changed decisions, not proof that every cash pick is superior for survival.

There were **156 JEV calls across 17 recorded positions**, including all
variants and repeats. These are selected development cases, not a held-out
performance evaluation. The probe's `top_choice` is a rating winner or explicit
choice; for ordinary shops it is not necessarily the final thresholded action.
Full handler and execution checks are covered separately below.

- [Development cases](samples/v18-probe-cases.json)
- [Initial alternatives](samples/v18-item-probes.jsonl)
- [Explicit cash-choice repetitions](samples/v18-cash-step-probes.jsonl)
- [Additional cases](samples/v18-validation-cases.json)
- [Additional repetitions](samples/v18-validation-probes.jsonl)
- [Combined replay summary](samples/v18-replay-summary.json)

## Offline verification

**60 tests passed**, including nine new regression checks covering:

- Actual post-price payout, low-cash losses, break-even cases, and payout caps.
- The recorded $21 Hermit example yielding $36, with matching prompt facts.
- JEV declining a transaction even when its separate item rating is high.
- Full consumable slots and using the acquired card's actual inventory slot.
- Refusing to use an unrelated card if the purchased cash tarot is missing.
- Riff-raff with zero/one/two available creation slots, replacement, Negative
  edition, and no guaranteed scoring claim for unknown generated jokers.

The existing hand forecasts, setup plays, terminal handling and benchmark
resumption tests also pass. Run:
`python -B -m unittest discover -s tests -v`.

## Paired live checkpoint protocol

Recreated the first shops of `2RGEX6EN`, `BAPQJMY5`, and `75WVKF42` by replaying
their recorded opening actions. Saved the actual game to opaque `.jkr` files;
the agent never reads their contents. Saved visible-hand play history gives
both versions the same complete recent-hand context at the checkpoint.

For each seed, run v17 and the draft twice, reversing condition order for the
second repetition. Stop on a loss or after clearing ante 3. Validate money,
seed, round, shop cards, packs and jokers when each checkpoint is loaded.
Use Red Deck, White Stake, `jev-latest`, and the normal 0.3-second action delay.
Keep source bytes frozen throughout all twelve continuations.

Records live under `runs/_experiments/v18-paired-checkpoints`, outside the
production run listing and benchmark cells. They are partial continuations,
not new full benchmark games. The first attempt completed, but a report writer
failed on a relative/absolute path mismatch. The writer was repaired and the
completed attempt recovered from its original recording; it was not replayed
or discarded.

## Completed checkpoint results

Completed 2026-10-15. All **12 planned continuations** finished with **zero
recorded errors**. Values below are cleared blinds, including the small blind
before the checkpoint. **9 is the test cap: arriving at ante 4, not winning.**

| Seed | V17, repeats 1 / 2 | V18 draft, repeats 1 / 2 | What changed |
| --- | --- | --- | --- |
| `2RGEX6EN` | 2 / 9 | 9 / 9 | Draft took Riff-raff both times; v17 skipped it both times |
| `BAPQJMY5` | 9 / 9 | 9 / 9 | Draft executed 2 / 1 profitable cash transactions; baseline executed none |
| `75WVKF42` | 5 / 5 | 5 / 5 | Both versions still lost at the ante-2 Water boss |

Across the six matched repetitions: **one improvement, five ties, no
regressions observed**. Capped mean clears were 6.50 for v17 and 7.67 for the
draft, a difference of +1.17. These selected three seeds and two repetitions
are insufficient to establish a population improvement. Repeats are not six
independent seeds, and reaching the cap hides any later differences.

The ante-1 seed is encouraging, but its baseline variation matters. Both v17
attempts skipped the same first-pack Riff-raff. Their first action difference
was decision 36 at The Hook: one played its current three of a kind, whereas
the other discarded five cards while retaining that set. The latter survived
and later assembled scoring jokers. It would therefore be misleading to claim
that v17 always loses ante 1 or that Riff-raff is the only route out.

The cash change worked in the actual game, beyond altered ratings. Both
`BAPQJMY5` draft continuations bought and used the first-shop Hermit: $9 before
purchase, $6 afterward, $12 after use. One later bought another Hermit from
$14 and finished its transaction at $22. Survival through the cap nevertheless
tied the baseline. Extra cash is an observed behavior improvement, not an
observed win-rate improvement.

`75WVKF42` remains unresolved. These changes leave its early choices unchanged;
the first baseline/draft pair both scored 1,271 before losing The Water. The
next investigation should target boss-aware readiness and the reliability of
conditional scoring, as described in the tutorial review, rather than claiming
this economy patch fixes that seed.

### Interruption and recovery disclosure

The original batch stopped during `BAPQJMY5-v17-1`, at ante 2 / round 4, with
26 decisions and no recorded errors. On resumption the panel and game were
offline; no terminal result had been recorded. Its entire original directory
is retained. That trial was restarted from its original first-shop checkpoint
under `BAPQJMY5-v17-1-recovery1`; the other nine completed attempts were reused,
and the last two scheduled attempts then ran normally. This gives 12 complete
trials plus one separately reported interrupted partial, which is not scored
as a win or loss. The replacement is explicitly journaled, not selected by
its performance.

The recovery helper was checked to refuse implicit replacement, retain the
original summary, and reuse a completed replacement on further resumes.
Decision-source bytes stayed `d22adb59` throughout the completed batch.

- [Complete results, pairs, and interruption record](samples/v18-checkpoint-results.json)
- Live records: `runs/_experiments/v18-paired-checkpoints/`
- Fixed protocol: `runs/_experiments/v18-paired-checkpoints/protocol.json`
- Recovery journal: `runs/_experiments/v18-paired-checkpoints/interrupted-attempts.json`
- Byte-exact draft source snapshot: `runs/_sources/d22adb59/`
- Runner: `scripts/evaluate_v18_checkpoints.py` (`--resume` reuses completed trials;
  `--retry-interrupted` explicitly retains and replaces an unfinished trial)

## Recommendation and handoff

Keep these two narrow changes as the **v18 draft**. They address reproducible
decision mistakes and have a promising early-survival result, but they are not
yet a demonstrated benchmark upgrade. Do not add more strategy changes on the
strength of these selected tests. The next performance evaluation should use
the existing fixed `bench30-a` settings and all 30 seeds, with v17 as the direct
comparison and v15 retained as the stronger historical reference.

At the initial draft handoff, no full v18 benchmark had been started. The panel
was paused with v18 loaded, no queued seeds and automatic new runs disabled;
the game was at its menu. Production benchmark recordings and unrelated working
changes were preserved.

## Full benchmark authorized and started

On 2026-10-15 the user approved the proposed complete 30-seed evaluation.
The game and server had been closed and were restored. The nine decision files
still matched the byte-exact `d22adb59` draft, and there were no existing full
v18 runs to duplicate.

Started `bench30-a` with its unchanged seeds, Red Deck, White Stake,
`jev-latest`, and 0.3-second action delay. Performance early stopping is disabled
for this explicitly requested full batch (runtime threshold 31); automatic
random new runs are disabled, and the panel pauses after the final seed.
Game speed, music and shaders follow the existing fast launcher settings.
No decision-policy changes were made for this evaluation.

The launcher rejects an existing panel, an active game, a source-version
mismatch or any existing attempt for the same version/set. It is
`scripts/start_fixed_benchmark.py --expected-version d22adb59 --set bench30-a
--finish-set`. If interrupted, use the existing resumption workflow rather
than relaunching fresh or replaying completed seeds.

[Launch settings and verified startup](samples/v18-benchmark-launch.json).
Results remain pending; compare the completed batch with v17 and v15 before
making any performance claim or further strategy changes. No per-game model
monitoring was scheduled.
