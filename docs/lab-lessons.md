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
