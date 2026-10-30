# What the lab taught us (v27 → v31, 2026-10-16/17)

A cloud session ran about 40 policy-lab experiments (code only, no JEV, Jackdaw sim) on the
mid-game scaling problem. This page keeps the conclusions; the numbers are in
[policy-lab.md](policy-lab.md) (from "Mid-game scaling experiments"), one row per experiment in
[samples/cloud-experiments.json](samples/cloud-experiments.json), and every game in
[samples/cloud-lab/](samples/cloud-lab/).

## Where it got to

| | v27 | v31 |
|---|---:|---:|
| sim60-a mean ante (wins of 60) | 5.90 (4) | 6.50 (10) |
| 60 fresh seeds | 6.15 (6) | 6.63 (10) |
| 200 fresh seeds (v28 → v31) | 6.46 (27) | 6.68 (40) |

About a third of v31 runs reach ante 8. Most losses are antes 5-7 at ~75% of the target (not enough
scoring), plus the ante-8 finisher (Violet Vessel kills nearly every run that meets it).

## What worked: fixing what the forecast measured wrong

The shop forecast (play the next 3 bosses with the build frozen, compare blinds cleared against
buying nothing) is a good judge of whatever it is asked about. Every gain came from a place where it
measured the wrong thing or was never asked:

1. **Planets (v28, +0.31 ante, wins doubled).** `jokers.gain` kept each sample hand's play fixed, so a
   planet for a hand we did not play yet was worth 0, and small sure gains never cleared the 2 SE bar.
   v27 had gained ~3 levels by ante 6; v28 ~9.
2. **Verdant Leaf (v30, 27 → 29 wins of 200).** The bot never sold a joker to lift it, and Jackdaw never
   modelled the lift.
3. **The auto-buy bar (v31, +0.23 ± 0.10 over 320 seeds).** 2 paired SEs at 16-24 rollouts rejected
   real gains of +0.3 to +0.9 blinds. 1 SE is the optimum (1.5 and 0.5 are worse).

## What did not work, and why

- **Heuristic nudges on top of the forecast** (xMult or scaling credit, engine credit and protection,
  discard commitment to a hand, suit tarots by measured value, Mouth first plays, tag skips): flat or
  worse every time. Where the forecast and a rule of thumb disagree, the forecast is usually right for
  this bot. Example: engine credit made engines form 4x as often, but they did not score, because the
  bot does not play to exploit them.
- **More forecast compute** (32/48 rollouts, 4-boss horizon, joker-aware discards inside rollouts,
  forecasts that keep shopping): flat or worse. Better forecasts move every option alike.
- **Money rules** (more rerolls, late spending, interest guard, future-value vouchers): flat or worse.
  Money is not the bottleneck; v31 spends hard early and that is fine.
- **A learned value of shop states**: from ~700 games, summary features explain 6-17% of the outcome
  within an ante and joker identities overfit. At this data size the future is mostly luck.
- **The 5k-games dataset as training data** (money, jokers and hand levels at each ante's first shop):
  a win model trained on its 5k teacher games ranks our games about as well as one trained on our 712
  (AUC 0.57-0.63 vs 0.53-0.65 at antes 3-7); training on both is no better. Our own model does improve
  with more of our games (ante 4-6 AUC: 142 games 0.55, 284 games 0.60, 569 games 0.63), so more of our own
  games would help a little per doubling; the teacher's games add nothing on top.
- **1,000 fresh Rust-lab games** (v32 Rust, arm F, `JEVLAB_SNAPS=1`; 16.1% wins, mean ante 6.39;
  `samples/cloud-lab/v1000-snaps.jsonl.gz`, script `samples/cloud-lab/vcurve.py`). Held-out AUC of P(win)
  from the first shop of an ante, by training games (~100 / 190 / 380 / 760): ante 5 0.51 / 0.53 / 0.61 /
  0.66, ante 7 (rich features) 0.55 / 0.57 / 0.62 / 0.68; antes 3-7 end at 0.57-0.68. Still climbing about
  +0.05 per doubling, so a 4,000-game run would likely reach ~0.70-0.75. Rich features (every hand's level
  and play count, deck, enhancements, hand size) add ~0.02-0.03 at antes 4, 6 and 7. Open: whether such a
  model separates shop options from the same position, which is what a decision needs.
- **5,000 games** (the 1,000 above + 4,000 more, `samples/cloud-lab/v4k-snaps.jsonl.gz`; 16.7% wins).
  Held-out AUC at ~480 / 960 / 1,900 / 3,800 training games, simple features: ante 3 0.51 / 0.54 / 0.56 /
  0.59, ante 5 0.57 / 0.61 / 0.65 / 0.68, ante 7 0.58 / 0.64 / 0.69 / 0.70. The gain per doubling falls to
  +0.02-0.03: the ceiling for these features is near 0.70-0.72 at antes 5-7 and ~0.6 at antes 3-4 (early
  outcomes are mostly the luck still to come). A weighted sum (logistic regression, rich features) is as
  good as boosted trees: 0.62 / 0.65 / 0.69 / 0.71 / 0.71 at antes 3-7, so it is cheap to put in the bot.
  What it values at antes 4-6 (per SD, correlation not cause): hand size +0.26, editions +0.20, money
  +0.20, Pair level +0.15, Stuntman, Spare Trousers, Burglar, Photograph, Runner, Baseball Card, Castle;
  against: hands already played at the ante (a struggling build), Turtle Bean, Diet Cola, Shoot the Moon,
  Sixth Sense, Cloud 9.
- **That model as the forecast's tail value** (`JEVLAB_EXP=value-tail=W`, `native/bot/src/value_model.rs`):
  each rollout scores blinds cleared + W x P(win) x blinds left at its end. Pooled sim60-a + 200 fresh
  seeds (none in the training games): W = 0.5 -0.02 +- 0.06 ante (49 vs 51 wins), W = 2 -0.12 +- 0.07 (51
  vs 51); W = 1 on sim60-a -0.20 +- 0.14. It changes decisions (Mercury buys up) but not outcomes: a model
  that ranks whole games at AUC 0.7 does not separate shop options from one position, which differ by far
  less than two random games do. Rows: `samples/cloud-lab/value-tail.jsonl.gz`.
- **Red Deck advice vs our games** (5,000 lab games, ante 5): Castle wins 32% when held (all runs 18%), so
  the advice agrees there, but the bot holds it in only 2.3% of runs; the discard jokers (Mail-In Rebate,
  Faceless, Hit the Road, Burnt, Trading Card, Yorick) are held in under 1% each. Burglar (34%) and Green
  Joker (24%), which the advice says to avoid on Red, are among our best: this bot gets more from hands
  than from discards.
- **Why the bot skips Castle and the discard jokers** (200 fresh seeds traced; `samples/cloud-lab/castle_why.py`,
  `discards.py`): the bot uses 1.8 of its 4 discards a round (0.9-1.0 at antes 2-3; none at all in 35% of
  rounds), because it plays as soon as a hand clears and discards only when behind. The forecast's simulated
  play (`simcore` `Policy::decide`) has the same rule, so a discard joker barely grows in its rollouts: on a
  free slot it scores +0.00 blinds vs buying nothing, and as a swap (most offers come with full slots) it
  loses to the joker sold (Mail-In Rebate -1.19, Faceless -1.08, Trading Card -1.11, Burnt Joker -1.02,
  Castle -0.45). Offers per game: Mail-In 1.0, Faceless 1.0, Castle 0.3, Trading Card 0.3, Hit the Road
  0.2, Burnt Joker 0.2; bought: Castle 8 of 58, Mail-In 4 of 203, Burnt Joker 0 of 35. The forecast is
  right for how the bot plays; the play leaves free discards unused.
- **Free discards** (`JEVLAB_EXP=free-discard`; `simcore::free_discard`, used by the bot's `decisions::play` and
  the rollouts' `Policy::decide`): before a play, spare cards a held joker pays for are discarded (Castle's
  suit, Mail-In Rebate's rank, Jacks, 3+ face cards, Burnt Joker's hand, Yorick), and Trading Card cuts the
  round's first discard to one card; skipped with Green Joker, Ramen, Delayed Gratification or Banner. The
  rollouts take them (46,537 in one game), yet the forecast still rates these jokers below skip (16 seeds:
  -0.84 -> -0.96 blinds) and buys about as many. Pooled 260 seeds: +0.01 +- 0.03 ante, 48 vs 51 wins; 25 games
  changed, 13 better, 12 worse. Two reasons: most of these jokers pay money, which the frozen-build forecast
  values at zero (rollouts never shop), and the scoring ones (Castle, Hit the Road) grow too slowly inside a
  3-boss horizon to beat the joker a swap would sell. Rows: `samples/cloud-lab/free-discard.jsonl.gz`.
- **Money in the forecast** (`JEVLAB_EXP=money-tail=W`): rollouts never shop, so money and prices counted for
  nothing; each rollout alive at its end adds W x blinds per dollar x money (blinds per $ from 5,000 games with
  build controls: 0.10 at ante 2, 0.07 at 3, 0.04 at 4-5, 0.03 at 6, 0.02 at 7, 0.01 at 8). W = 0.5 on 260
  seeds: -0.07 +- 0.06 ante, 46 vs 51 wins (91 games changed). Dropped; W = 1 and 2 were stopped unfinished.

## Bugs found by reading what the bot did (v36-v39, Rust only)

- **Audit of predictions** (`samples/cloud-lab/audit.py`, 6,923 plays in 200 traced games): 1.1% of plays
  scored under 2/3 of their estimate, but 27% under Cerulean Bell. Cloud 9 (34%) was a parse bug; Seltzer
  plays scored over 1.5x their estimate 69% of the time (another parse bug); Misprint, Hit the Road, Madness,
  Pareidolia and Mime are underestimated (not modelled), which costs play choice, not the forecast.
- **v36, Cerulean Bell:** the game adds the forced card to every play (cut to 5 cards), and the bot did not know
  which card it was: a planned Straight scored 11,664 against an estimate of 393,120. The view marks the card
  and plays keep it. Of 108 seeds from the 5,000 games that died to Cerulean Bell, 47 now win (it killed ~2%
  of all runs). 260 seeds: no game changes before ante 8, wins 51 -> 53.
- **v37, joker text:** "(Currently $4)" (Cloud 9) was read as +4 Mult; Seltzer's "for the next 10 hands" as
  "retrigger 10s". 260 seeds: -0.004 +- 0.032 ante, 55 vs 53 wins.
- **v38, Verdant Leaf bait:** its ante-8 deaths are near misses (95-98% of the target) after selling a real
  joker; before it the bot now buys the cheapest sellable joker when a slot is free and sells that. Slots are
  almost always full by ante 8 (4 of 128 Leaf-death seeds), so it rarely applies (3 of 128 now win).
- **Boss death rates when faced** (200 traced games): Violet Vessel 83%, Crimson Heart 47%, Verdant Leaf 39%,
  The Serpent 25%, Cerulean Bell 20% (before v36), The Plant 19%, The Eye 17%; most bosses 2-8%.
  On 2,000 fresh seeds (v38, `samples/cloud-lab/b2k-v38-snaps.jsonl.gz`, `bossrate.py`): Violet Vessel 78%,
  Crimson Heart 43%, Verdant Leaf 38%, The Serpent 28%, Cerulean Bell 22% (after v36), The Wall 21%, The Mouth
  18%, Amber Acorn 14%, The Water 14%, The Eye 13%; the rest 3-13%.
- **v39, The Serpent:** it tops the hand up with exactly 3 cards after any play or discard, whatever room there
  is (as in the game), so a one-card discard grows the hand by 2. Until a play clears, the bot and the rollouts
  discard the lowest card outside the best play, one at a time. 2,000 seeds: Serpent deaths 62 -> 15 (28% ->
  7%), +0.033 +- 0.011 ante, 345 vs 337 wins (`b2k-v39-snaps.jsonl.gz`); 260 seeds vs v37: +0.015 +- 0.035.
- **v40, The Mouth's lock:** the game scores only the round's first hand type, but every type played (even one
  that scored 0) counts as played this round, and the bot allowed any played type: after one off-type play it
  kept playing that type for 0 and called it a sure clear. The bot remembers the first type; the rollouts use
  the game's lock. Alone it rescues nothing (an off-type play scores 0 either way): 2,000 seeds -0.011 +- 0.007.
  (The earlier note that later off-type plays were "deliberate card cycling" was wrong.)
- **v41, The Mouth's first play:** each hand type in the bot's plays is valued as its best play now + (hands left
  - 1) x its expected best score in a fresh deal (16 order-free samples of the draw pile); the bot leads with
  the best. 88 Mouth-death seeds: 27 get past it, 11 win. 2,000 seeds vs v40: +0.019 +- 0.007 ante, 356 vs 346
  wins, Mouth deaths 87 -> 70 (`b2k-v41-snaps.jsonl.gz`). The Python arms that only allowed frequent types
  (`bot-mouth60/80`) lost because they ignored what each type scores.
- **The Eye plan (dropped, lab flag `eye-plan`):** the same valuation for The Eye (no hand type twice in the
  round): each type is its best play now + (hands left - 1) x the expected best of a fresh deal without that
  type and the types already used. 2,000 seeds vs v41: -0.007 +- 0.004 ante, 352 vs 356 wins, Eye deaths
  54 -> 62. Unlike The Mouth, The Eye's plays already lead with the biggest hand, and saving a strong type
  for later wastes it when the later deals can't make it; the greedy order is better.
- **v42, Misprint:** its exported effect is only "+" (the real game's too, v16 note), so the scorer counted 0:
  37% of the 632 plays holding it in 200 traced games scored over 1.5x their estimate (without Misprint only
  78 of 5,943 plays did). It now counts its average +11.5 Mult. 2,000 seeds vs v41: +0.015 +- 0.013 ante,
  350 vs 356 wins (neutral; 314 games changed). Kept as a correctness fix.
- **v43, Raised Fist:** the scorer took the lowest held value with Stone cards as 0 (any held Stone zeroed it)
  and ignored Mime and the card's Red Seal (the Mime plays the audit flagged: 19 of 40 over 1.5x). It now
  mirrors rumbot's `lowest_held`. 2,000 seeds vs v42: +0.005 +- 0.008 ante, 347 vs 350 wins (neutral). v42+v43
  vs v41: +0.019 +- 0.015 ante, 347 vs 356 wins. Estimate errors this size barely change outcomes: the best
  play is usually the same, and most fixes only move the "clears" check.
- **Late interest floor (dropped, lab flag `late-floor=N`):** 1,127 of 2,000 runs die at antes 5-7, mostly near
  misses (80-100% of the target: 521), and the bot enters those shops with ~$36 because it keeps a $25 floor
  for loose buys, rerolls and strong vouchers all game. Dropping the floor $5 per ante after 4 ($20/$15/$10/$5):
  2,000 seeds vs v43 +0.009 +- 0.013 ante, 340 vs 347 wins (523 games changed). Spending the reserve late buys
  nothing that matters; the sure (forecast > 2 SE) buys already ignore the floor.
- **v44, Hieroglyph and Petroglyph never bought:** v43 bought Hieroglyph (-1 ante, -1 hand every round) in 605 of
  2,000 games. The forecast applies it as ante - 1 and counts blinds cleared over 3 bosses, where easier blinds
  look like a gain; a win needs that ante again with a hand fewer to the end. 2,000 seeds vs v43: 367 vs 347
  wins (z = 1.77), +0.023 +- 0.018 ante; the 605 Hieroglyph games won 135 instead of 115. The same kind of bias
  hits any item whose cost falls after the horizon. Fresh 2,000 seeds: 359 vs 352 wins (z = 0.60); pooled 4,000:
  726 vs 699 wins (z = 1.66). (Mean ante is no fair measure here: Hieroglyph itself lowers the ante reached.)
- **v45, skipping blinds for tags:** the lab's code-top-option JEV always plays, so the
  lab bot never skipped a blind. Skipping a Small/Big blind at antes 1-3 for an Investment, Negative or Rare Tag
  (fresh 2,000 seeds vs v44): **428 vs 359 wins** (only-one-side 154/85, z = 4.46), mean ante -0.073 +- 0.035:
  ante-1 deaths 36 -> 80, every later ante fewer. Adding Polychrome, Charm, Buffoon, Double, Orbital, Coupon,
  Voucher and Economy: 369 vs 359 wins, mean ante -0.72 (a skip costs a cash-out and a shop; most tags don't
  pay for that). rumbot's tags follow the game (Rare/Uncommon put a free joker in the shop, edition tags make
  the next shop joker free, Investment pays $25 at the boss cash-out).
  Variants (same seeds): from ante 2 only 406 wins (ante-1 deaths back to 36, but less gain); Investment only
  404 (z = 3.96, +0.030 ante); Negative + Rare only 388 (z = 2.31). The parts add up (+45 and +29 ~ +69).
  Confirmation on the first 2,000 seeds: 391 vs 367 wins (z = 1.58), mean ante -0.124 +- 0.034; pooled 4,000:
  819 vs 726 wins (only-one-side 281/188, z = 4.3). The cost is ante-1 boss deaths (25 -> 62), ~30 of them with
  no joker: both ante-1 blinds skipped means no shop before the boss.
- **Tag sweep (each tag added to v46's skip list, fresh 2,000 seeds, v46 = 431 wins):** Polychrome 472 (z = 3.45,
  +0.050 +- 0.021 ante: a free Polychrome joker, like the Negative Tag; first seed set 433 vs 396, z = 3.23);
  Foil 476 (z = 4.09); Holographic 466 (z = 3.16); Uncommon 426, Orbital 424, Economy 422, Double 412. Foil does
  as well as Polychrome: the gain is the free shop joker an edition tag gives, more than the edition.
- **v47, edition tags:** Polychrome, Holographic and Foil together, 2 x 2,000 seeds vs v46: 515 vs 431 and 461 vs
  396 wins (z = 4.83, 3.92), +0.155 +- 0.035 and +0.180 +- 0.032 ante; ante 5-7 deaths ~100 fewer on each set.
  On top of v47 (515 wins) every other tag tried loses: Standard 507, Garbage 506, Meteor 505, Coupon 504, Handy 503,
  Speed 502, Top-up 502, Buffoon 497, Charm 495, Ethereal 495, Voucher 488, D6 487, Juggle 477, Boss 464. The
  sweep covered every tag; v47's six (a free joker or $25) are the ones worth a skip.
  Why it pays: at ante 5 the bot holds as many jokers as without skips (5.07) but twice the editioned ones (0.73 ->
  1.36): skips trade a shop the cautious shop logic barely uses for joker quality it rarely finds otherwise.
  How late (`skip-max`, v47 = ante 3): up to ante 2 491 wins, 4 525 (z = 0.79), 5 515. **v48** skips through ante 4:
  first seed set 478 vs 461 (z = 1.37), pooled z ~ 1.5, mean ante flat. Hone / Glow Up (more editions in later
  shops) bought in 358 games: 507 vs 515 wins; it is the free early edition joker that pays, not editions later. A skip pays only for a free joker or $25; packs, rerolls and coupons don't cover the lost
  cash-out and shop. (The sim's view names tag_skip "Skip Tag" (tables.json); the game's is "Speed Tag".)
- **Real game vs rumbot (v50, 2026-10-19, `jevbot play --lab` + `sync_check.py`, docs/real-game-sync.md):** 7 seeds
  (RZSTXX2S G6MTPMNN Q5RLLNT9 XKJN7TMN DUQVNM87 FAXCVJ5M D424YXQH) play identically on the real game and on rumbot,
  state by state, after three BalatroBot patches (`mods/balatrobot-patches/`): `cash_out` now waits for the game's
  queued events (a fast client was paid last round's total and saw the shop before tags applied, so the tag
  skips' free jokers could be missed in the real game), `buy` counts slots like the game (Negative jokers), and
  `gamestate` reports Cerulean Bell's forced card (v36 only worked on rumbot before). v50 fixes the one bot-side
  split: a tag-freed joker's cost 0 read as extra sell value in forecasts.
- **Forecast compute, retested on 2,000 seeds (lab flags `fc-n`, `fc-bosses`; v48 = 525 wins):** 32 rollouts per
  option instead of the lab's 16: **576 wins** (z = 2.50), +0.076 +- 0.032 ante, 1,024 games changed. A 4-boss
  horizon: 520 (neutral). The 2026-10-16 "more compute does not help" came from 128-200 seeds; with 16 rollouts
  the forecast's noise picks wrong buys. (Production plays 24 rollouts within a 4 s budget.) First seed set:
  585 vs 478 wins (z = 5.0), +0.112 +- 0.032 ante; pooled 1,161 vs 1,003.
  64 rollouts (second seed set): 635 wins (16: 525, 32: 576; 64 vs 32 z = 2.84), 9.2 s a game (16: 3.8, 32: 5.5).
  128 rollouts (first 1,000 seeds): 308 vs 326 wins for 64, no better. **v52** plays 64 everywhere (production was 24
  within its 4 s budget, the lab 16), so lab numbers from v52 on are not comparable with earlier ones. 128 rollouts
  hit a crash (2 of 1,000 games): a tarot targeting the best/held card with every hand card debuffed aborted the
  rollout twice (Python fell back to Jackdaw there); **v51** makes it wait instead (400 of 400 games unchanged).
  v52 on the first seed set: 605 vs 478 wins for v48 (z = 5.8), 605 vs 585 for 32 rollouts (z = 0.97).
- **On v52 (64 rollouts; second seed set, v52 = 635 wins):** up to 5 rerolls a shop (was 2): 615 wins (z = -1.77);
  rerolls only went 8.51 -> 8.58 a game, the interest floor binds first.
  Swaps over the 4 weakest jokers (was 2): 622 wins (+0.037 +- 0.019 ante, z = -0.74).
  The pack Picker with 4x samples (48 synthetic hands, 96 planet deals): 630 wins (z = -0.30). The forecast's
  rollout count was the noise that mattered.
  Auto-buy bar 0.5 SE (was 1): 630 wins (-0.042 +- 0.027 ante); 2 SE: 566 wins (z = -3.23). 1 SE stays.
  Value tail 0.5 (lost at 16 rollouts: -0.02 ante): 659 wins (z = 1.39, +0.018 +- 0.017 ante); first seed set 640
  vs 605 (z = 2.10); pooled z ~ 2.45. **v53** makes it the default (production too). A less noisy forecast let a
  learned value past the horizon help where it had hurt.
  Weight 1.0: 648 vs 658 wins (z = -0.81); 0.5 stays. v54 (Cerulean Bell discard fix + rumbot forced-card fix):
  all 2,000 games of the second seed set identical to v53.
  Money tail 0.5 retested on v54: 644 vs 659 wins (z = -1.02); money is still not the bottleneck.
  Uncommon Tag skips retested on v54: 648 wins (-0.088 +- 0.023 ante). Where v54 dies (second seed set): ante 1 85
  (v44 36: Big 29, boss 54; 31 with no joker), ante 8 377 (Violet Vessel 128, Crimson Heart 77, Verdant Leaf 62).
  No skips in ante 1 (skip-min=2) on v54: 601 vs 659 wins (z = -3.24): the ante-1 deaths are worth the tags.
  Boss rerolls (lab flag `boss-reroll`; buy Director's Cut/Retcon, reroll Violet Vessel, Crimson Heart, Verdant
  Leaf, The Wall, The Needle): owned in 319 games, 661 vs 659 wins, Violet Vessel deaths 128 -> 123. A rerolled
  finisher is mostly another hard finisher.
- **Forecast calibration (v54, 200 traced games, `samples/cloud-lab/calib.py`):** the rollouts are pessimistic. Predicted
  P(clear the next boss) vs actual: ante 6 68% vs 85%, ante 7 52% vs 78%, ante 8 37% vs 63%; forecasts under 50%
  come true 64% of the time even from the shop right before the boss (no shop skipped), so it is the rollouts'
  play, not only the frozen build: they play the best hand now and discard only when behind, keeping a 4-card
  suit, else pairs, else the 2 highest cards (no straights, no joker goals).
  Probe: rollouts that never discard: 541 vs 659 wins (z = -5.3). The rollouts' discards matter a lot to the forecast.
  Smarter rollout discards (`rollout-discard2=4`: the old rule, flush draw, straight draw, pairs or the best play,
  whichever scores best over 4 order-free redraws; 2x slower): first 1,000 seeds 306 vs 332 wins, -0.135 +- 0.053
  ante. The bot's own discard works much the same way (odds over 32 draws), so "copy the bot" is not the fix; better
  rollout play shifts the forecast's comparisons in ways that did not help. Dropped for now.
- **Value model retrained (`samples/cloud-lab/vtrain.py`)** on v52-v54 games of the second seed set (7 runs), tested on
  the first set: lower log loss at every ante (ante 6 0.569 vs 0.625) and AUC 0.70-0.76 at antes 5-8 (old 0.69-0.73).
  The v32-trained model was badly pessimistic for today's bot (ante 2: 19% predicted, 33% actual).
  In the lab (first seed set, held out from training) at weight 0.5: 624 vs 640 wins (z = -1.06). It predicts
  higher P(win), so the tail weighs about twice as much (like weight 1.0, also slightly worse).
  At weight 0.25: 641 vs 640. A better-calibrated, better-ranking model changes nothing: the tail's gain does not
  hinge on the model's accuracy. The v32-trained model stays.
- **Play vs discard (`play-now-ratio`, CodeJev plays if its best play >= R x the best discard's modeled next play):**
  R = 1.15 (discard more): 592 vs 659 wins (z = -5.25). **R = 0.9 (play more readily): 794 vs 659 wins (z = 6.6),
  +0.297 +- 0.033 ante**; R = 0.75: **852 wins** (+0.401 +- 0.037; vs 0.9 z = 3.28). The modeled next play is not
  optimistic (2,730 discards in 200 traced v54 games: the next play scored 1.33x its prediction on average, 1.15x
  median, and beat the hand given up 92% of the time). The rule was wrong, not the forecast: it treated play-now
  vs discard-then-play as either/or with discards free, but a play also replaces its cards, and a discard kept is
  worth a lot later in the round. Discard only for a clear gain.
  R = 0.6: 862 wins (vs 0.75 z = 0.55): flattening. First seed set: 825 vs 640 (z = 8.4). **v55** makes 0.6 the
  default of the code stand-in for JEV (codejev::PLAY_NOW_RATIO; the lab, `jevbot play --lab/--code-jev`, the panel
  with `--code-jev`). With real JEV the play_now answer is JEV's own; its prompt would need the same lesson.
  R = 0.4: 809 vs 862 (z = -2.56); 0.6 stays.
  A looser 0.8 while discards left >= hands left (`play-now-spare=0.8`): 874 vs 862 wins (z = 0.69, -0.017 +- 0.020
  ante): neutral, not shipped.
  Rollouts that discard only when further behind (`rollout-behind=0.6`, the rollouts' own play-vs-discard rule):
  first 1,000 seeds 427 vs 419 wins (+0.001 +- 0.039 ante, 392 games changed): neutral, dropped.
  Discards ranked by the chance the next play makes pace (`discard-key=pace`, was: the mean next play): first
  1,000 seeds 413 vs 419 wins, -0.090 +- 0.048 ante: dropped. The mean next play stays.
  No clear margin (`clear-margin=1.0`; a play with jokers counted as clearing only at 1.1 x chips needed, so a
  play that clears by under 10% could be passed up for a discard, even on the last hand): second seed set 882 vs
  862 wins (52/32, z = 2.18), first set 828 vs 825 (38/35); pooled 90/67 (z = 1.84), +0.005 +- 0.006 ante.
  Keeping 1.1 only where the estimate is uncertain (`clear-risky=1.1`: Misprint, Bloodstone, an unmodelled scoring
  joker, a Lucky card in hand): first 1,000 seeds of the second set 430 vs 419 (20/9, +0.016 +- 0.010 ante), 16
  games differ from plain 1.0. **v56** ships that rule (production and lab): a small gain, but the margin protected
  nothing where the estimate is exact.
  v56 itself: second seed set 877 vs 862 wins (37/22), first set 834 vs 825 (34/25, +0.015 +- 0.007 ante); pooled
  71/47 (z = 2.21).
  CodeJev's item forecast over shop planets too: a no-op (0 of 1,000 games changed; the shop's planet rule and its
  own item forecast decide planets first). Removed. Check that a new flag changes games on ~20 seeds first.
  Out of discards, CodeJev picks the play with the best two-play average (`setup-play=avg`; the outlook JEV is shown,
  default: best score now): first 1,000 seeds 432 vs 430 wins, +0.029 +- 0.020 ante, 27 games changed. Rare; parked.
  A 4-boss forecast horizon at 64 rollouts (16 rollouts: neutral): first 1,000 seeds 439 vs 430 wins (69/60),
  -0.007 +- 0.021 ante: neutral; 3 stays.
  CodeJev's own item forecast (shop tarots, spectrals, packs) ran 16 rollouts, not the lab's 64; at 64 (`cj-fc-n=64`):
  first 1,000 seeds 420 vs 430 wins (79/89), +0.002 +- 0.024 ante, 248 games changed: no gain; 16 stays.
  Joker order by sampled score (`order-search`; in the shop, pairwise swaps from the current order while one raises
  the mean best play over 12 order-free deals by 0.1%): the fixed rule (xMult jokers right, nothing around a copier)
  misses joker editions, which apply at their joker's place (a Polychrome joker is an xMult wherever it sits), and
  per-card effects, which run in joker order (Gluttonous Joker before Photograph). Second seed set 895 vs 877 wins
  (98/80), +0.042 +- 0.013 ante (231 games changed). Trying all 720 orders gave the same 24-seed results, 7x slower.
  First seed set 886 vs 834 wins (134/82, z = 3.54), +0.018 +- 0.013 ante; pooled 232/162 (z = 3.5). **v57** makes it
  the default (lab and production; `no-order-search` restores the old rule).
  `blueprint-fix` on v57: first 1,000 seeds of the second set 457 vs 457 wins, 3 games changed. Correct, too rare to
  measure; to ship with the next change that needs new baselines.
  Played cards in their best-scoring order (`card-order`: rearrange the hand before a play; estimates scaled by the
  reorder's gain): traced reorders scored 10-40% more than in place, but nearly all on plays that cleared anyway.
  First 1,000 seeds of the second set 460 vs 457 wins, -0.001 +- 0.011 ante, 50 games changed: neutral, dropped. (A
  first run rescored plays without boss rules and lifted Mouth-banned plays: Mouth deaths 21 -> 42.)
  Tarot targets by measurement (`tarot-measure`: every target set applied to a copy of the deck, best mean best
  play over 24 deals; was tarots::plan's card-value rule): first 1,000 seeds 449 vs 457 wins, -0.004 +- 0.029 ante,
  299 games changed: dropped. (Measuring inside the Picker, which the forecasts' rollouts also run, was ~10x slower.)
  The shop forecast's rollouts holding each option's jokers in v57's searched order (`fc-order`; a bought joker sat
  on the right): first 1,000 seeds 455 vs 457 wins, -0.003 +- 0.013 ante: dropped.
  Early slot fill to 5 jokers or to ante 2 / 5 (`early-jokers`, `early-ante`): 0-1 of 24 games change; the forecast's
  1 SE bar already buys those jokers. Rollouts that buy the top hand's planet (the bot's planet rule; rollout shops
  had to be stocked, `no_shop`): 0 of 24 games change; every option's rollouts get the same planets. Not kept.
  Skips for tags up to ante 5 on v57 (`skip-max=5`; v48: 4): first 1,000 seeds 455 vs 457 wins, -0.009 +- 0.016
  ante, 407 games changed: neutral; 4 stays.
  Value tail 0.75 on v57 (0.5): first 1,000 seeds 460 vs 457 wins, +0.003 +- 0.010 ante, 56 games changed: neutral.
  The planet rule's floor at $25 (`planet-floor=25`; $4 spends interest money): first 1,000 seeds 460 vs 457 wins,
  +0.003 +- 0.015 ante, 136 games changed: neutral. Money is still not the bottleneck.
  Discard plans throwing plain cards before enhanced, editioned or sealed ones (`toss-plain`; rank order threw a Steel 2
  first): first 1,000 seeds 450 vs 457 wins, -0.013 +- 0.008 ante: dropped.
- **Estimate audit (`samples/cloud-lab/estaudit.py`, 200 traced v56 games, 7,411 plays):** the play's estimate matched the
  chips scored within 1% in 65% of plays (73% without Misprint and Bloodstone), was low in 26% and high in 8%. Jokers the
  game updates before the hand scores (rumbot `update_before_scoring`: Green Joker, Obelisk, Ride the Bus, Runner,
  Spare Trousers, Square Joker, Vampire, Midas Mask) are read at their displayed value. Obelisk was exact in 30 of 177
  plays: it gains X0.2 before scoring, or resets to X1 when the hand (counted with this play) is the run's most played,
  so the bot could count on a clear that scored a third. Green Joker gains +1 first (exact in 21 of 777). Lab flag
  `scorer-fix` (with the Blueprint fix): Obelisk 220 of 267 exact, Green Joker 623 of 776, all plays 86%.
  In the lab (Obelisk, Green Joker, Blueprint): first 1,000 seeds of the second set 474 vs 457 wins (34/17, z = 2.38),
  +0.039 +- 0.013 ante, 63 games changed. Ride the Bus added (kernel hand cond 12: no face card scored).
  With Ride the Bus, second half: 437 vs 438. Second set 911 vs 895 (62/46), first set 890 vs 886 (56/52, +0.009 +-
  0.012 ante); pooled 118/98 (z = 1.36). **v58** ships the fixes (lab and production; `no-scorer-fix` restores v57's
  scorer): small, but they are the game's rules and the bot's numbers are what every decision reads.
  v58 itself (Ride the Bus included throughout): second set 908 vs 895 (66/53), +0.017 +- 0.011 ante; first set 890.
  More scorer gaps (lab flags): Hiker's permanent chips on cards (`extra-chips`, read from the card's "+N extra
  chips" text minus Bonus's 30; kernel Card.extra): Hiker plays exact 38/142 -> 86/118, lab 469 vs 471 (10 games
  changed). Splash (`splash`, every played card scores; kernel set_splash, no kicker pruning): Splash plays exact 63/168
  -> 169/193; with extra-chips 469 vs 471, -0.013 +- 0.010 ante (29 changed). Baseball Card (`baseball-fix`, X1.5 at
  each Uncommon joker's place, after its effect; it was all at Baseball's place): 141/208 -> 168/193 exact; lab 470 vs
  471 (2 games changed). Rare jokers: right, but too few games turn on them to show; kept as lab flags, not shipped.
- **Value model retrained on v55-v58 games** (`samples/cloud-lab/value_model_v58_candidate.json`; lab `VALUE_MODEL=<file>`):
  held out on the first seed set, log loss lower at every ante (ante 2: 0.735 vs 0.924; the v32 model says 18% where
  today's bot wins 45%), AUC 0.74-0.75 at antes 6-7 (old 0.72-0.73). In the lab at weight 0.25 (its P runs ~2x the
  old one's): first seed set 442 vs 449 wins, +0.004 +- 0.017 ante, 134 games changed: neutral, as at v54.
- **Pack jokers through the shop forecast** (`pack-forecast`; the Picker valued them by measured gain on recent hands,
  shop jokers by rollouts): the pack's jokers shown to the forecast as $0 shop jokers, the best beating nothing by
  1 SE taken. Second seed set 905 vs 908 wins, +0.000 +- 0.017 ante, 181 games changed: neutral, dropped.
  The three rare-joker scorer fixes together (`extra-chips,splash,baseball-fix`): first 1,000 seeds of the first set
  447 vs 449 wins, +0.019 +- 0.012 ante, 35 games changed: neutral; kept as lab flags.
- **Shopping rollouts** (`rollout-shop`: forecast rollouts stock their shops and buy the priciest affordable joker
  while a slot is free and $10 stays, then the top hand's planet; the frozen build valued money and open slots at 0):
  on v58, first 1,000 seeds of the second set 475 vs 471 wins, +0.078 +- 0.045 ante, 369 games changed. To retest on
  v59 (new sim baseline), with `rollout-shop-policy` (the play policy rebuilt after a buy).
  v59 vs v58 on that half: 475 vs 471 wins, no game's final ante or round changed (Crimson Heart fixes in last rounds).
  `rollout-shop-policy` on v59: 490 vs 475 wins (141/126), +0.027 +- 0.047 ante, 395 games changed; 2x slower (14.3 s
  a game vs 7.2). Against v1 (`rollout-shop`): -0.051 +- 0.031 ante, 490 vs 475 wins. Noisy: both change ~40% of games.
  `rollout-shop` over the second set (first half run on v58's binary, second on v59): 955 vs 914 wins (283/242, z =
  1.79), +0.049 +- 0.032 ante, 768 games changed.
  `rollout-shop-policy` over the second set: **987 vs 914 wins** (303/230, z = 3.16), +0.030 +- 0.031 ante; vs
  `rollout-shop` 987 vs 955 (203/171).
  First seed set (vs v58; v59 changed no game's final ante or round on the second set): **956 vs 890 wins** (317/251,
  z = 2.77), +0.095 +- 0.033 ante. **v60** makes shopping rollouts with policy rebuilds the default (`no-rollout-shop`
  restores v59's). Lab games take ~2x as long (14 s vs 7 s); production's 4 s budget plays fewer rollouts.
  Rollouts keeping $25 before a joker (`rollout-shop=25`; v60: $10): first 1,000 seeds 489 vs 490 wins, +0.018 +-
  0.043 ante: neutral.
  Rollouts taking the first affordable joker in the shop (in effect random; `rollout-shop-first`) instead of the
  priciest: first 1,000 seeds 484 vs 490 wins, +0.016 +- 0.035 ante. Which joker the rollouts buy barely matters: the
  v60 gain is money and open slots being worth something to the forecast, not a better imagined build.
  A 4-boss horizon on v60 (`fc-bosses=4`): first 1,000 seeds 484 vs 490 wins, -0.029 +- 0.034 ante: neutral, as on v56.
  Spending the interest reserve late on v60 (`late-floor=9`: $16 at ante 5, $7 at 6, $0 from 7): first 800 seeds 340
  vs 393 wins (40/93, z = -4.6), +0.020 +- 0.020 ante: worse. Money held at ante 8 scores (Bull, Bootstraps).
  Violet Vessel losses on v60 (345 of 4,000 games) end at a median 48% of the target (37 reach 80%): a 2x power gap,
  not a finish-line slip. Boss rerolls retested on v60 (`boss-reroll=...`): the bot never buys Director's Cut or Retcon
  (offered in 13 of 48 traced games with $8-14; `buy-vouchers` + `voucher-floor=10` still buy none: CodeJev's shop
  answer never takes a "strong" voucher), so nothing changes; parked.
  Up to 4 rerolls a shop on v60 (`max-rerolls=4`; 2): first 800 seeds 382 vs 393 wins, +0.005 +- 0.013 ante: neutral.
- **Skip decisions by forecast** (`skip-forecast=A`, `skip-forecast-add=A`; forecast::skip_forecast: rollouts from the
  blind select with the real tags, a play branch and a skip branch, the skipped blind counted as passed; rumbot's
  math.random stream must be seeded first or a pack tag panics): the forecast rates the proven tags (Rare, Polychrome,
  Negative) below playing and vetoes those skips: 24 seeds 6 vs 12 wins. Keeping the static skips and letting it add
  others: 10 vs 12. Rollouts buying a tag's free joker first (`rollout-free-first`) changes neither (6 vs 12, 10 vs 12):
  the forecast just does not see what the six tags are worth. The static list stays.
- **Measured build power as the learned value** (`simcore::power`: the best play's score on 32 hands dealt from the
  full deck, scorer as at a round's first hand; logged per shop by `JEVLAB_SNAPS`; `samples/cloud-lab/ptrain.py`,
  800 v60 games `samples/cloud-lab/c2k-pw-v60-snaps.jsonl.gz`, logging leaves all 800 identical). At the first shop of
  antes 6-8, its log10 p25/median/p75/mean plus money, jokers and hand size (7 inputs) predict the win at held-out AUC
  0.70-0.72, vs 0.68 for the old 100+ inputs (joker identities overfit: log loss 0.60-0.70 vs 0.53-0.62). The median
  alone is worse (0.61-0.67): how evenly a build scores counts. Antes 2-4: ~0.5-0.6 for every input set.
  As the forecast's tail (`VALUE_MODEL=samples/cloud-lab/value_model_power_c2k800.json`, value-tail 0.25, calibrated so
  about v60's weight): first 600 seeds of the first set (held out) 264 vs 280 wins, -0.010 +- 0.038 ante, 157 changed.
  It buys fewer level-1 planets and Celestial/Standard/Arcana packs, more Buffoon packs: the model's money weight
  (winners hold money) makes the forecast hoard, which the shopping rollouts already price.
  Power alone (`ptrain.py --only`, AUC 0.67-0.70 at antes 6-8): 255 vs 279 wins, -0.008 +- 0.039 ante (z = -2.12), the
  same shift (fewer Arcana/Celestial/Standard packs, Mars, Hermit, Temperance): a power tail at the rollouts' end
  also counts the jokers their shops buy with the money an option saves, so it rewards saving over cheap upgrades.
  The option's own power gain instead, before any rollout (`power-bonus=1`, + 1 blind per 10x p75 power over the skip
  option, ~the model's mid-game slope): 271 vs 280 wins, +0.005 +- 0.029 ante, 131 changed: neutral.
  Why a planner for early build choices has little to learn: p75 power minus log10 of the ante's boss target, median at
  each ante's first shop (800 v60 games), wins / died at ante 8 / died at 7 / died by 6: ante 3 -0.24 / -0.26 / -0.30 /
  -0.37, ante 5 -0.40 / -0.46 / -0.53 / -0.62, ante 7 -0.56 / -0.66 / -0.74, ante 8 -0.61 / -0.73. Winners and the ante-8
  losers build the same power through ante 4; the gap opens at antes 5-8, and every group falls behind the targets
  each ante (4 hands of p75 cover 10^0.6). The runs split on late scaling, not on the early build.
  So a late-game shop rule: the power bonus only from ante 5, doubled (`power-bonus=2,power-bonus-ante=5`): 280 vs 280
  wins (37/37), -0.008 +- 0.020 ante, 88 games changed (swaps and Celestial packs both ways). From ante 5 the 3-boss
  rollouts reach the end of the run and already score what power measures; the measure adds nothing they miss.
- **Joker valuation study (v60; `samples/cloud-lab/joker-study/`):** which jokers held at the first shop of ante 5
  go with winning (4,000 games, logistic win model with joker count, money, levels, editions; `jval.py`): Constellation
  93% wins held vs 51%, Hologram 88%, Baseball Card 83%, then Duo, Stuntman, Castle, Blackboard, Seeing Double, Mr. Bones;
  worst Faceless, Ceremonial Dagger, 8 Ball, Stencil, Gluttonous, Crafty, Bootstraps, Joker, Greedy, Mystic Summit.
  Split-half (each seed set alone) correlation of the values 0.75 at ante 5, 0.5 at ante 3. Pairs (`pairs.py`): too
  sparse at 4,000 games (201 pairs, top z 2.5 ~ chance); the top two are real synergies, Hanging Chad + Photograph and
  Burglar + Green Joker. Every shop's options with their forecast numbers (`JEVLAB_OFFERS=<file>`, 701 games,
  `offers-v60-701.jsonl.gz`, `offan.py`): the bot passes on the best jokers and the forecast rates them below buying
  nothing: Constellation offered 215 times, taken 9%, median forecast z -3.5; Hologram 6% (-3.4), Castle 17%, Blackboard
  12%, Gift Card 8%; economy jokers (Golden, Egg, Delayed Gratification) under 8%. Across jokers, value vs take rate
  correlates only 0.26. Since v60 the skip option keeps a slot the rollouts fill with the priciest joker at the next shop,
  so a joker that starts weak and scales loses inside 3 bosses (Hologram never grows: rollouts add no cards).
  Caveat: the bot holds these only where the forecast liked them, so their values are inflated by selection.
- **Joker prior** (`joker-prior=B`, values `JOKER_PRIOR=samples/cloud-lab/joker_prior_c2k_a5.json`: the ante-5 values
  from the second seed set alone, clipped to [-1, 1.5]; through ante 6 a shop joker's forecast delta gains B x (its value
  - the sold joker's) blinds, before hiding, rerolls and the buy rule): B = 1, first 600 seeds of the first set (held
  out): **319 vs 280 wins (93/54, z = 3.22), +0.090 +- 0.046 ante**, 205 games changed. Held at ante 5: Hologram 7 -> 30,
  Constellation 13 -> 34, Castle 14 -> 34, Baseball 21 -> 36, Scholar, Green Joker, Photograph up; Mystic Summit 42 -> 15,
  Even Steven, Misprint, Crafty, Seltzer, Faceless down. Next 600 seeds: 304 vs 277. **First 1,200 seeds: 623 vs 557
  wins (180/114, z = 3.85)**, +0.032 +- 0.031 ante, 395 games changed.
  B = 2: first 600 seeds 309 wins (vs v60 280, vs B = 1 319: 63/73), -0.072 +- 0.054 ante: pushing harder buys more
  early deaths. B = 1 stays.
  On top of B = 1: pack picks valued + 0.1 x the learned value (`pack-prior=0.1`) and the lowest-valued held joker
  offered for sale (`prior-sell`): first 600 seeds 322 vs 319 wins (42/39), -0.060 +- 0.030 ante: not better; dropped.
  **Shipped as v61** (`data/joker_values.json` = `samples/cloud-lab/joker_prior_c2k_a5.json`, B = 1). Whole first set
  (held out from the values): **1,048 vs 956 wins (52.4% vs 47.8%; 281/189, z = 4.24)**, mean ante -0.021 +- 0.024.
  Deaths at antes 1-4 / 5-7 / 8: 122 / 455 / 375 (v60 99 / 516 / 429): it buys builds that start weaker and scale.
  Second set (the values' own games, so in-sample): 1,061 vs 987 wins (53.0% vs 49.4%; 276/202, z = 3.38), mean ante
  +0.004 +- 0.023; The Mouth deaths 35 -> 49.
  The prior only from ante 3 (`joker-prior-from=3`, to spare the fragile first antes): first 600 seeds of the first set
  309 vs 319 wins (28/38): worse. Buying the scaling jokers early is part of the gain.
  Values re-estimated on v61's own 2,000 second-set games (the bot now buys the good jokers widely, so less selection
  bias; correlation with the v60 table 0.75; Castle 0.76 -> 0.15, Scholar 0.47 -> -0.20, Stuntman 0.84 -> 1.26, Egg
  0.02 -> 0.70), averaged with the old table (`joker-study/joker_prior_iter2.json`): first 600 seeds 323 vs 319 wins
  (44/40), -0.020 +- 0.029 ante: neutral. The v61 table stays.
  A bigger table from 11,330 v61 games that reach ante 5 (the second set plus 10,000 new seeds, `e16k`; first set held
  out; at least 20 holds; 141 jokers, 38 new, e.g. Cavendish 1.5, Madness 1.08, Vampire 0.72, Family -0.82, Driver's
  License -0.71; correlation with the v61 table 0.78; Flower Pot 0.64 -> -0.11, Certificate -0.42 -> 0.57): first
  600 seeds 310 vs 319 wins (73/82), -0.042 +- 0.044 ante: not better. More data on the bot's own games does not improve
  the table: once the prior steers what it buys, a value measured on those games partly measures the prior.
  Pair terms: of 1,015 pairs held together at ante 5 at least 30 times in those games, 4 differ from the main-effects
  prediction by |z| > 3 (about 3 expected by chance): Hanging Chad + Photograph +5.0, Burglar + Green Joker +4.0, Half
  Joker + Ramen +3.6, Ice Cream + Photograph -3.1. A held-out fit of all pairs (`pairfit.py`) gained nothing (log loss
  0.6123 both ways). The 4 as bonuses (`JOKER_PAIRS=joker-study/strong_pairs.json`, shrunk logit excess): first 600
  seeds 320 vs 319 wins, 5 games changed. No usable synergy signal at this sample size; dropped.
  v61's extra early deaths (antes 1-4: 122 vs 99 of 2,000) are spread over all seed blocks, at strength-check bosses
  (The Wall, Mouth, Water, Arm at ante 4). The prior scaled by min(1, P(clear the 3 bosses | buy) / P(... | skip))^K
  (`prior-safe=K`): K = 1 319 vs 319 wins (17/17, 43 changed), K = 3 313 vs 319 (21/27): the forecast's survival
  numbers do not single out the risky buys; dropped.
  B = 1.5: first 600 seeds 311 vs 319 wins (43/51), -0.085 +- 0.036 ante: worse (B = 2 was too); B = 1 stays.
  The prior through ante 7 (`joker-prior-to=7`): 310 vs 319 (17/26), -0.023 +- 0.011 ante: worse. From ante 7 the
  3-boss forecast reaches the end of the run, so it no longer needs the prior.
  Boss rerolls at ante 8: in 479 v60 games that reached an ante-8 shop, Director's Cut or Retcon was offered with Violet
  Vessel ahead in 3. No lever there.
  Early shops on their own table (`JOKER_PRIOR_EARLY=joker-study/joker_prior_c2k_a3.json`, values held at ante 3 of the
  same v60 games, 54 jokers, through ante 3; correlation with the ante-5 table 0.69; economy jokers rate higher there,
  Delayed Gratification 0.56 vs 0.16, Egg 0.52 vs 0.02, Golden 0.33 vs -0.11, since the ante-5 model holds money fixed):
  315 vs 319 wins (32/36), 99 changed: neutral. Every variant of the prior so far (table, B, antes, pairs) lands within
  noise of v61 or below it.
  Pack picks valued with the prior alone (`pack-prior=P`: the Picker's joker value, a relative score gain, + P x the
  learned value): P = 0.3 first 600 seeds 328 vs 319 wins (25/16), next 600 306 vs 304; P = 1 333 vs 319 (37/23),
  next 600 316 vs 304: **first 1,200 seeds 649 vs 623 (73/47, z = 2.37)**, 142 changed; P = 3 325 vs 319 (44/38).
  Held at ante 5 with P = 1: Blackboard 49 -> 60, Zany 32 -> 44, Constellation 57 -> 65, Mr. Bones 63 -> 71;
  Wrathful 39 -> 25.
  **Shipped as v62** (`PACK_PRIOR` = 1). Whole first set: **1,083 vs 1,048 wins (54.2% vs 52.4%; 118/83, z = 2.47)**,
  +0.005 +- 0.019 ante; second set: 1,070 vs 1,061 (98/89), +0.017 +- 0.017 ante. Both: 2,153 vs 2,109 (216/172,
  z = 2.23); without the 600 seeds P was picked on, 179/149 (z = 1.66). Small, positive in every chunk, ~11% of games
  changed. v62 with no flags replays the `pack-prior=1` games exactly (40 seeds).
  `prior-sell` alone on v62: first 600 seeds 331 vs 333 wins (32/34), -0.040 +- 0.023 ante: dropped.
  Rollouts buying the highest-valued joker instead of the priciest (`rollout-shop-value`): 330 vs 333 (64/67), 150
  changed, -0.035 +- 0.043 ante: neutral; price stays.
- **Boss death rates when faced, v62** (4,000 lab games): Violet Vessel 56% (v38 78%), Crimson Heart 21% (43%),
  Verdant Leaf 19% (38%), The Wall 13%, Amber Acorn 11%, Cerulean Bell 9%, The Water 9%, the rest under 8%. Losses end
  at a median 50% of the target against Violet Vessel (a power gap), 68% against The Wall, 76-78% against Crimson Heart
  and Verdant Leaf (a quarter reach 90%). 75 of 104 Verdant Leaf deaths hold 4 jokers: slots were full, so it sold a
  real one.
- **Forecast calibration, v62 (200 traced games, 5,429 shop forecasts, `samples/cloud-lab/calib62.py`).** P(clear the
  next boss) predicted vs actual by ante: 1 96/99.5%, 2 93/98.5%, 3 94/100%, 4 90/98%, 5 86/97%, 6 74/89%, 7 67/90%,
  8 42/77%. The rollouts' median boss score as a share of the bot's: 0.76-0.92 at every ante, 0.89 even from the
  ante's last shop (build all but final), so most of the gap is the rollouts' play, not later purchases. Forecasts
  under 10% come true 46% of the time. By boss (last two shops, bosses the bot then cleared): Verdant Leaf 33%
  predicted, Crimson Heart 41%, Violet Vessel 47%, The Needle 62%, Amber Acorn 63%, Cerulean Bell 71%, The Mouth 79%;
  ordinary bosses 90-96%. Rollouts handle only The Psychic, Eye, Mouth and Serpent; they never sell a joker (Verdant
  Leaf) and never discard before their last hand (`hands_left > 1`), which is every hand under The Needle.
  Discarding before a losing last hand (`rollout-last-discard`): The Needle 67% -> 81% predicted, antes 1-4 1-2 points
  closer; 200 seeds 109 vs 111 wins. Rollout targets / 1.15 (`rollout-easy=1.15`, a blunt stand-in for the bot's
  stronger play): score ratio 0.90-0.95 at antes 1-7 (0.81 at 8), but P(clear) still low at ante 8 (49 vs 73%):
  the rollouts lose more of their bad draws than the bot does, not only by a constant factor.
  Both together on 600 seeds (`rollout-easy=1.15,rollout-last-discard`): **300 vs 333 wins (76/109, z = -2.43)**, ante-8
  deaths 106 -> 134. A forecast closer to the truth plays worse: late in the run the pessimism is what pushes it to buy
  power before the finishers, and v62's tuning (the joker prior included) sits on top of it.
  Calibrated only through ante 5 (`rollout-easy=1.15,rollout-easy-to=5`, the farm antes; the end left pessimistic):
  301 vs 333 (74/106, z = -2.39), ante-8 deaths 106 -> 124. A less fearful early forecast also builds worse late.
  Neither arm buys less (33 buys a game, $32 and 5.1 jokers at ante 5 in all three): the change is in which items.
  Chosen options' forecast gains are ~0.2-0.3 blinds from ante 3 (0.08 at ante 8) against the prior's +-1, so since
  v61 the learned values choose the jokers and the rollouts the rest.
- **What winners have beyond the build's power** (`samples/cloud-lab/beyond/`: ~17,000 v61/v62 games with power
  snapshots; logistic win models at the first shop of an ante, trained on the second set and e16k, tested on the
  first). Power (the best play's score over 32 deals, 4 hands, which is roughly what the forecast sees) alone predicts
  little: held-out AUC 0.53 at ante 3, 0.64 at 5, 0.68 at 7; with everything below 0.63 / 0.73 / 0.77. Per 1 SD, with
  the rest held fixed (ante 5): joker values +0.62 logit (z 28), money +0.31 per $16 (z 15), editions +0.27 (z 12),
  vouchers +0.24 (z 12), power's 90th percentile +0.26 (z 8; its median +0.11, its 10th percentile slightly negative:
  the ceiling matters, not the floor), fewer hand types played +0.05 (z 3); deck size, hand size and Pair vs Flush vs
  Straight builds ~0. By voucher: Grabber +0.62 (z 12), Paint Brush +0.33 (z 7), Nacho Tong +0.75, Palette +0.58; but the
  bot already owns Grabber in 83% and Paint Brush in 81% of games that see them (v60 offers), so that is not a lever.
  By edition: Negative +0.62, Polychrome +0.41, Foil +0.21, Holographic +0.08 per joker. Early on, what decides a run is
  what the build will become (joker quality, money), not how strong it is now: the reason the joker prior helped.
- **8-boss forecast horizon on v62** (`fc-bosses=8`; only ~1.4x slower, 22 vs 15 s a game, since most rollouts die or
  reach ante 8 first): first 600 seeds 306 vs 333 wins (52/79, z = -2.36), -0.137 +- 0.043 ante; more deaths at
  antes 1-7 (189 vs 161), The Mouth 12 -> 25. Longer horizons have now been neutral (4) or worse (8).
  8 bosses without the joker values (`fc-bosses=8,no-joker-prior,no-pack-prior`, does the long horizon find what the
  values add?): first 480 seeds 211 vs 263 wins (43/95, z = -4.43), and below v60 (3 bosses, no values: 224). No: the
  longer the rollout, the more its weak play and frozen build compound; the learned values carry what it cannot see.
- **v63: edition values in the joker prior** (`edition-prior=W`; per joker at ante 5 with power, money and joker values
  held fixed: Negative 0.62, Polychrome 0.41, Foil 0.21, Holographic 0.08 logit; refit without the first seed set:
  0.60 / 0.41 / 0.19 / 0.09, so its test is effectively held out). A bought joker's prior gains its edition's value, a
  sold joker's goes. W = 1: first set **1,109 vs 1,083 wins (55.5% vs 54.2%; 67/41, z = 2.50)**, +0.021 +- 0.010 ante;
  second set 1,089 vs 1,070 (69/50), +0.027 +- 0.010 ante; both 2,198 vs 2,153 (136/91, z = 2.99). ~6% of games change.
  **Shipped as v63** (`EDITION_PRIOR` = 1); v63 with no flags replays the `edition-prior=1` games exactly (40 seeds).
  Edition values in pack picks too (`pack-edition=1`): first 600 seeds 345 vs 346 wins, 9 games changed (editioned
  jokers rarely come in packs); dropped.
  Edition weight 1.5 (`edition-prior=1.5`): 346 vs 346 wins, 12 games changed; W = 1 stays.
- **Hand levels and the deck beyond power** (`samples/cloud-lab/beyond/levels.py`, antes 4 and 6): per level, with power,
  money and joker + edition values held fixed, every hand type is worth -0.03..+0.06 logit (the most-played hand's level
  +0.05, z 3-4); enhanced cards +0.03 each at ante 6 (z 5); deck size and hand size ~0. Vouchers +0.33 each (z 11-14).
  Planets are priced about right; vouchers are the next blind spot (the bot buys Overstock in 11% of the games that
  offer it, Hone 16%, Seed Money 29%, Wasteful 44%).
  Voucher values in the shop forecast (`voucher-prior=1`, |z| > 2 vouchers only): first 600 seeds 331 vs 346 wins
  (27/42, z = -1.81); vouchers 1.22 -> 1.77 a game (Hone 39 -> 140, Overstock 33 -> 139, Seed Money 62 -> 128 of 600).
  Owning them marked runs with money to spare, not a cause: unlike jokers and editions, the correlation does not
  survive being acted on. Dropped.
- **v64: round search for in-round play** (`round-search=N`, `round-search-tol=T`, `round-search-to=A`;
  `odds::round_search`). In a round no play clears, each first move (the play options and the discards) is played out
  to the end of the round on N shuffles of the draw pile (sorted first, so the real order never matters; the same
  shuffles for every move) with a simple policy: play the best hand; behind pace, or on the last hand, discard around it
  (keeping a 4-flush). The scorer stays fixed (no joker growth within the round). The move that clears most often
  replaces the usual choice when it clears at least T more often. Skipped under The Mouth, The Eye, Cerulean Bell and The
  Hook, whose rules it does not model (Cerulean Bell deaths 8 -> 14 on 880 seeds before that). 33 ms a decision; games
  ~22 vs ~17 s.
  N = 48, T = 0.03: 450 seeds 248 vs 252 wins; with a 3-point bar it mostly chases sampling noise.
  N = 128, T = 0.1: first set **1,171 vs 1,109 wins (58.6% vs 55.5%; 136/74, z = 4.28)**, +0.136 +- 0.025 ante; second
  set 1,121 vs 1,089 (127/95, z = 2.15), +0.080 +- 0.025; both 2,292 vs 2,198 (263/169, z = 4.5). ~15% of games
  change. Where it helps (first set): ante-1 deaths 32 -> 13; Big Blind 107 -> 81 and The Wall 64 -> 50 (first 1,800); more runs reach
  ante 8 (1,526 vs 1,453). The second set's first 740 seeds were flat (only 4 ante-1 deaths to fix there), the rest
  +30 wins. **Shipped as v64** (`ROUND_SEARCH` = 128, `ROUND_SEARCH_TOL` = 0.1).
  A higher bar, T = 0.15: first 450 seeds 264 vs 272 wins (10/18) vs v64; 0.1 stays.
  Under The Eye and The Mouth too (`round-search-boss`, each simulated play marks its type played): first 450 seeds
  267 vs 272 wins (5/10), 19 games changed; Mouth deaths 10 -> 6, Eye 5 -> 7. Dropped.
  More shuffles, N = 256, T = 0.07: first 420 seeds 258 vs 254 wins (16/12), -0.019 +- 0.044 ante, twice the cost;
  128 stays.
  The play-now ratio under round search: 0.8 (discard more) 229 vs 254 wins (29/54, z = -2.74); 0.45 (play more) 235
  vs 254 (34/53, z = -2.04), first 420 seeds. The search corrects the rule's worst calls; it does not replace it. 0.6
  stays.
  Rollouts that also discard on their last hand (`rollout-last-discard`, alone this time): first 420 seeds 222 vs 254
  wins (37/69, z = -3.11) vs v64. Like `rollout-easy=1.15`, a more optimistic rollout makes worse shop calls.
  Harder rollout blinds (`rollout-easy=0.9`, targets / 0.9): 221 vs 254 (36/69, z = -3.22): worse too; the rollouts sit
  at an optimum both ways. Caution: these v64 variants were screened on the seeds the 0.1 bar was picked on (b2k
  0-419, +19 there), so that baseline is flattered by selection; later v64 variants are screened on c2k 0-449.
  The search's own discard as one more option (`round-search-extra`): c2k 0-449 236 vs 234 wins (10/8), 21 changed.
  Neutral; dropped.
  The value tail retested on v64 (`value-tail=W`; 1.0 lost at v53): W = 0.7 c2k 0-449 240 vs 234 wins (18/12); W = 1.0
  second set 1,151 vs 1,121 (174/144, z = 1.68), +0.037 +- 0.017 ante; first set seeds 420-1999 929 vs 917 (126/114),
  -0.009 +- 0.020 ante; both 2,080 vs 2,038 (300/258, z = 1.78). It opened at +35 wins on 900 seeds and faded to
  noise; 0.5 stays.
  A paired test instead of the 10-point bar (`round-search-z=2`: override when the searched move's extra clears on the
  shared shuffles beat the usual move's by 2 SE): second set 0-899 508 vs 491 wins (49/32), first set 420-1319 514 vs
  520 (37/43); both 1,022 vs 1,011 (86/75). Neutral; the bar stays.
- **Spending out at ante 8** (`final-spend`: no interest floor and up to 10 rerolls a shop at ante 8; runs that die there
  hold $35 at their last shop): c2k 0-449 227 vs 234 wins (9/16) vs v64; rerolls 10.0 -> 12.1 a game. As with
  `late-floor` (v43), spending the reserve buys nothing that matters. Dropped.
- **128 forecast rollouts on v64** (`fc-n=128`, 32 vs 22 s a game): c2k 0-539 297 vs 288 wins (72/63, z = 0.77), -0.013 +-
  0.050 ante. Neutral, as at v52; 64 stays.
- **v65, Ceremonial Dagger kept rightmost** (found reading a traced loss: ante 7 with 2 jokers). The Dagger destroys the
  joker to its right each time a blind is selected, and the v57 order search (which scores one hand, not the blind
  select) left it with jokers to its right: in one run it ate 10 bought jokers from ante 4. Runs that ever hold it
  (57 of 4,000) averaged 3.7 jokers from ante 4 (all runs 5.1) and won 30% (all 57%). Now the shop moves it last, the
  order search only keeps orders that leave it there (moving it back after each search cycled two orders and froze
  the shop), and the forecast's rollouts hold it last too. On those 57 seeds: 31 vs 17 wins (18/4, z = 3.0), 5.1 vs
  3.7 jokers from ante 4; every other game is unchanged. **Shipped as v65** (`no-dagger-last` restores the old
  order). Whole sets (v64 rows with those seeds replaced): 1,175 and 1,131 wins.
- **Scan of 200 traced v65 games** (`samples/cloud-lab/trace-scan/`: vanish.py, anom.py, cons.py, churn.py). Jokers
  that leave without a sale: Gros Michel 45, Mr. Bones 22 (each a saved run), Popcorn 15, Ice Cream 7 (all by design);
  Madness ate 10 jokers in one run, yet runs holding Madness win 65% (37 of 4,000). No losing round ended with
  discards unused; 6 plays scored 0. Consumables held through 3+ shops: Judgement (needs a free joker slot) in 10 of
  12 games that had one, The Lovers 10 of 18, Death 4 of 22. 975 joker sales (4.9 a game), 154 within a round of
  arriving. Nothing else like the Dagger.
- **The shop planet rule** (top hand's shop planet, else the biggest Celestial pack, while $4 stays; ~9 Celestial packs
  a game): without its Celestial packs (`planet-rule=shop`) c2k 0-449 231 vs 236 wins (44/49); without the rule
  (`planet-rule=none`) 230 vs 236 (44/50). Neutral both ways; it stays.
- **v66, the round search's scorer per state** (found reading a traced near miss lost by 6 chips with Banner held). The
  search scored every simulated play with the round's current hands and discards left, so after a simulated discard
  Banner (+30 chips per discard left) kept its chips, Mystic Summit (+15 Mult at 0 discards) never fired, and Acrobat
  (x3) and Dusk (retrigger) never saw the final hand. Now a scorer per (hands left, discards left) scores each play in
  its own state. Few games change (~2%), but nearly all for the better: c2k 0-899 511 vs 494 wins (20/3), b2k 420-869
  268 vs 259 (9/0); together 29 vs 3 games won by only one side (z = 4.6). **Shipped as v66** (`no-round-search-rnd`
  restores the fixed scorer).
  The same for hand types played earlier in the simulated round (`round-search-played`: Card Sharp's x3, Supernova):
  c2k 0-449 249 vs 249 wins, 7 games changed (2/2). Correct, too rare to measure; parked to ship with the next change.
  Full sets: c2k 1,165 vs 1,131 wins vs v65 (46/12 won by only one side, z = 4.5).
- **Blind skips retested on v66** (v45-v48 tuned them when the bot won ~25%; it now wins ~58%). Skipping through
  ante 5 (`skip-max=5`): c2k 0-449 250 vs 249 wins (21/20), -0.036 +- 0.021 ante. Uncommon Tag added to the list:
  233 vs 249 (12/28, z = -2.5). v48's list and limit still hold.
- **v59 (local agent, real-game check of v58):** all 8 seeds identical on the real game and rumbot after two rumbot
  fixes (Crimson Heart releasing Juggler dealt a card; a tag-free joker kept its price once owned); the 18 v57
  rearranges all match; real-game estaudit 223/297 plays exact. A new sim baseline: lab numbers before it are not
  comparable (`docs/samples/real-sync/2026-10-22.md`).
  The scorer's rightmost Blueprint copied the leftmost joker (Brainstorm's rule); in the game it copies nothing
  (lab flag `blueprint-fix`).
- **v46, no Big-blind skip without a joker:** 2 x 2,000 seeds vs v45: +0.025 +- 0.011 and +0.046 +- 0.015 ante,
  431 vs 428 and 396 vs 391 wins (101 games change).
- **Shop-side vouchers (dropped, lab flags `strong-vouchers`, `buy-vouchers`, `voucher-floor`):** the lab bot bought
  0.74 vouchers a game: `DecisionCfg::lab` turns off production's strong-voucher rule and rollouts never shop, so
  Overstock, Clearance Sale, Reroll Surplus and Blank (-> Antimatter) were worth 0 to it. Fresh 2,000 seeds vs v46
  (431 wins): the strong rule 427 wins (+0.005 +- 0.009 ante, 1.06 vouchers a game); + Blank with a $10 floor 436
  (-0.010 +- 0.014; Antimatter in 33 games); + Reroll Surplus, Seed Money, Telescope, Crystal Ball 436 (-0.050 +-
  0.018, 2.0 a game). Buying them changes nothing measurable.
- **Crimson Heart re-picks only after a play** (dropped idea): the debuffed joker moves on the draw after a hand
  (`press_play` preps the blind; rumbot `_play_hand`), never after a discard, so discarding to move it only
  wastes discards (lab flag tried on its 55 death seeds: 1 changed). The bot already scores without the
  debuffed joker; Crimson Heart deaths are a strength check like Violet Vessel's.

## What the stronger bot does differently

The [5k-balatro-games](https://huggingface.co/datasets/makemake/5k-balatro-games) dataset (a heuristic
with an RNG oracle and search shops, Blue Deck, 74.6% wins) builds much like us through ante 5, but
commits to **Pair**: its highest-level hand at ante 6 is Pair in 77% of its wins (ours: spread over
Straight, Full House, Two Pair, Pair), with Pair-repeat jokers (Card Sharp, Supernova, Green Joker,
Half Joker) and more xMult late. Our own data agrees: Pair decks win ~30%, Flush decks ~11% and drift
into other hands after ante 5. Simple planet nudges toward Pair did not change outcomes, and neither
did a full commitment (below).

**Pair commitment (Rust lab, v32 Rust, sim60-a, `JEVLAB_EXP` in `native/bot/src/experiment.rs`).** Every
shop planet and pack pick goes to Pair (Mercury buys 0.17 -> 0.85 a game); Pair jokers (Jolly, Sly, The Duo,
Half Joker, Supernova, Card Sharp, Spare Trousers, Mad, Clever) clear the auto-buy bar 1 SE earlier and are
never sold (0.5 -> 1.7 held at death); a Pair within 85% of the best play is played. Baseline 6.50 (10 wins):
planets -0.08 +- 0.13 (8 wins), + jokers -0.23 +- 0.14 (8), + plays -0.08 +- 0.15 (8). Not confirmed on 200
seeds: no arm was ahead. The commitment happens and does not pay. Probable reasons (not tested): the
teacher's Pair wins come from its oracle and shop search finding the right Pair jokers, and our Pair decks
win more because good seeds deal pairs, not because Pair is the better plan. Rows: `samples/cloud-lab/pair-sim60.jsonl.gz`.

## How to experiment (method lessons)

- Pair every comparison by seed and keep the lab deterministic; confirm on 200+ seeds before shipping.
  Several +0.2 ante or +9 win bumps on 60 seeds vanished at 200-320.
- Count wins with "seeds won by only one side" (z = (a - b) / sqrt(a + b)), not raw totals.
- Read the logs before building: the three wins above were found by looking at what the bot did
  (levels by ante, a boss it never beat, items it never bought), not by tuning knobs.
- Prototype new rules as `BOT_ARMS` in `scripts/policy_lab.py` (Python, monkeypatched, no version
  change); port to production and to the Rust bot once they win.
- In the Rust lab, add a flag to `native/bot/src/experiment.rs` (`JEVLAB_EXP`, unset = production) and run
  `jevlab --jobs 1` in separate processes on a fixed seed split: threads share `readiness::play_mix`'s cache,
  so `--jobs N` games can differ between identical runs. ~17 CPU-s a game; screen on 60 seeds, confirm the
  arms that lead on 200 more.

## Open ideas

- A learned value at the forecast horizon, trained on far more games (hundreds of thousands, which the
  Rust lab makes feasible) and checked out of sample before use.
- A JEV benchmark of v31 on the real game, to see whether the lab gains carry over.
