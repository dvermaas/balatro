# Jackdaw full-game fidelity audit

Question: can [Jackdaw](https://github.com/TylerFlar/jackdaw-balatro) (pinned `e66de788`,
checkout in `runs/_research/jackdaw`) replace the real Balatro client so we can run games
headless, for example in the cloud?

**Verdict: usable, with caveats.** The RNG, deck, blinds, tags, shops and packs match the real
game 1:1, but only after Jackdaw is given our real profile's unlock and discovery state. With
that fix, 304 of 336 recorded bench30-a runs replay through the whole run with no divergence
that matters. A few open gaps remain: one spectral card bug, To Do List, two scoring cases and
legality checks that BalatroBot doesn't make. Also, our bot can't run against Jackdaw as it
stands, because Jackdaw's gamestate has no effect text.

Run it with
`runs/_research/python/Scripts/python.exe scripts/diagnostics/jackdaw_fidelity.py --out docs/samples/jackdaw-fidelity.json`.
It takes about 15 s for 336 runs. The JSON holds the details for each run. The audit is offline:
it reads recorded draw order and shops only to compare them, and feeds nothing to a policy.

## Method

- Jackdaw's `SimBackend` speaks the BalatroBot JSON-RPC. The script starts it with each run's
  seed, Red deck and White stake. It then replays every recorded `action` event from
  `decisions.jsonl` in order: play, discard, select, skip, buy, sell, use, reroll, pack,
  rearrange, cash_out and next_round. Before each recorded decision it diffs the simulated
  gamestate against the gamestate recorded in `states.jsonl.gz`.
- Fields compared:
  - state, ante, round, money
  - hands left, discards left and chips during a round
  - blind names, tags and targets
  - hand: ordered, and as a multiset of key + enhancement, edition and seal
  - draw pile: full order, and as a multiset
  - jokers (ordered) and consumables
  - shop, voucher and booster slots, with costs and editions
  - pack contents
  - hand levels, including the play counts
  - used vouchers
- A divergence counts as **critical** when it changes what a policy can see or do. Hand and draw
  pile order on their own are not critical.
- Some actions were rejected by the real game; we know because no outcome event follows them.
  Those actions must also be rejected by the sim. All 54 such actions were.
- Harness adapters (these are not engine changes):
  - When a fresh recorded snapshot exists, hand indices are remapped by card identity.
  - Joker `rearrange` permutations are split into adjacent swaps, because Jackdaw's bridge
    applies only one swap.

## 1. Seeded RNG parity

Jackdaw reimplements LuaJIT's Tausworthe PRNG, `pseudohash`, `pseudoseed`, `pseudorandom`,
`pseudorandom_element` and `pseudoshuffle` (`jackdaw/engine/rng.py`), with the same seed keys as
the Lua source.

| Check (336 runs, 30 seeds) | Default Jackdaw | With the real profile |
|---|---|---|
| Initial deck order, every card | 336/336 | 336/336 |
| Ante-1 blinds, boss and both tags | 280/336 | 336/336 |
| First shop (jokers, consumables, voucher, packs) | 265/336 | 336/336 |
| Runs with no critical divergence | 39/336 | **312/336** |
| States exactly equal | 10 825/17 256 | **24 223/24 640 (98.3%)** |

What the profile fix does:

- **Unlocks.** Lua's `get_current_pool` replaces each center with `unlocked == false` by
  UNAVAILABLE and then resamples. Legendary jokers are exempt. Jackdaw skips this step on purpose
  ("Sim assumes a fully-unlocked profile"), so every later pick lands on a different item. Our
  profile still has 58 pool items locked. Examples are `j_tribe`, `j_blueprint`, `j_wee`, `v_retcon`
  vouchers and `v_observatory`.
- **Discovered.** A tag with `requires` is in the pool only if its required center is
  discovered. For example, `tag_polychrome` needs `e_polychrome` and `tag_rare` needs
  `j_blueprint`. Neither of those is discovered on our profile.
- **How the script applies it.** It reads `%APPDATA%\Balatro\1\meta.jkr` (raw deflate of a Lua
  table) and wraps `jackdaw.engine.pools.get_current_pool`. Locked keys are injected as banned
  keys, which has the same UNAVAILABLE effect, and the discovered set is passed for Tag pools.
- **Profile drift.** `j_throwback` was unlocked during the benchmark period, between
  20261013-0718 and 20261013-1914. Its unlock condition is "continue a run from the main menu",
  which is what the resume tooling does. Runs before that date have to treat it as locked.
  Without that date, 38 runs diverged at their first Throwback-slot shop. This dependence is
  real: **the real game's shops depend on the profile at the moment of play.**
- **Cosmetic difference.** The Buffoon pack that the first shop always offers takes its art
  variant (`p_buffoon_normal_1` or `_2`) from unseeded `math.random`. The contents are the same.

## 2. Replay parity: first divergences that remain

The 336 runs went through 33 991 applied actions. 304 runs replayed to the end with no critical
divergence and no action error. The remaining 32 runs, by first cause:

| Runs | First divergence | Cause |
|---|---|---|
| 9 (+3 follow-on Déjà Vu errors) | Pack opened with the hand dealt. In the real game the hand has the 3 created enhanced face cards; in the sim they are in the deck | **Jackdaw bug.** Familiar, Grim and Incantation used from a booster pack put their created cards in the deck instead of the hand. Example: `20261012-194300-MW6PBTW4`, ante 4. |
| 6 (+3 follow-on money errors) | Money: the sim has $4 less (or more) | **To Do List target hand.** Lua rolls it with `pseudorandom_element` over `pairs(G.GAME.hands)`, a hash-order iteration in the SMODS-modded game. Jackdaw uses a fixed list order, so the same roll picks a different hand (real: Flush, sim: Pair). |
| 4 (+4 follow-on cash_out errors) | Round chips | 2 are off by 1 from float rounding of Ramen's xMult (14742 vs 14741). 2 are undiagnosed scoring mismatches: Flush + Photograph + Abstract (real mult 41, sim 31) and Flush + Madness on The Fish (real +50 chips). |
| 6 | The sim rejects a pack pick that the real game accepted | **BalatroBot is laxer than the game.** It used Judgement and Wraith with full joker slots (the real game ended up with 7 jokers out of 6 slots) and The Fool with no previous tarot. Jackdaw applies `can_use`, so it is arguably the correct one. Our bot depends on this laxness in 7 runs. |
| 2 | Hand-level play counts | Several plays in a row had no recorded snapshot between them, and the sim's hand order after the draw differed, so raw indices hit different cards. The order rule for mid-round redraws differs in rare cases. |
| 1 each | Joker order, a tarot target, one consumable | Joker rearrange semantics (bridge adapter), and a tarot applied to the wrong same-rank card after a hand-order difference. |

Draw-pile order is not critical, and it also differs after a booster pack closes in the shop: the
hand returns to the deck in a different order. This self-corrects, because the next `nr..ante`
shuffle sorts by `sort_id` first. Pools, tags, bosses and vouchers did not diverge in any run once
the profile was applied.

## 3. Coverage

Every item is implemented: it either has a registered handler or is handled from prototype config.

| Category | Implemented | Seen in our runs |
|---|---|---|
| Jokers | 150/150 | 113 |
| Tarots | 22/22 | 22 |
| Planets | 12/12 | 9 |
| Spectrals | 18/18 | 18 |
| Vouchers | 31/32 | 16 |
| Bosses | 28/28 | 27 |
| Tags | 24/24 | 21 |
| Enhancements | 8/8 | 8 |

- The only voucher with no code is `v_observatory`, which is still locked on our profile.
- The coverage grep doesn't find the boss keys `bl_club`, `bl_goad`, `bl_head`, `bl_psychic`
  and `bl_window`. They are handled through the blind's debuff config (suit, face, `h_size_ge`).
- Jackdaw ships about 250 live-validation scenarios (`jackdaw validate`) that run against
  BalatroBot.
- Coverage of game mechanics is not the problem. The problem is **presentation**. Jackdaw's
  serializer has no effect text: joker `value.effect` is just the name, and vouchers, packs,
  boss `effect` and `tag_effect` are empty. Our policy reads those strings, for example
  "Currently X3.5 Mult", "Level up Flush" and boss and tag effects (see `decisions.py`,
  `build.py` and `jev_bot.py`).

## 4. Speed and running in the cloud

- **Speed.** About **3 400 steps/s, or 81 full games/s**, on one core. That is a greedy heuristic
  policy over 300 games through the RPC layer, which serialises the full gamestate on every step
  (mean final ante 2.2). The engine API without the RPC layer is faster. For our bot the cost is
  JEV calls, not the simulator.
- **Cloud.** The engine and bridge are pure Python 3.12+ and import only the standard library;
  gymnasium and numpy are needed only for `jackdaw.env`. There is no game, display or Windows
  dependency, so it runs on Linux as is.

## Closing the gaps

1. **Profile.** Freeze our profile: snapshot `meta.jkr` and stop runs from unlocking anything
   (no main-menu continues), or record the unlock date of each key. Then pass the
   unlock and discovered sets to Jackdaw. Upstream, that means honouring
   `profile_unlocked` and `discovered` in `pools._filter_key`; the hooks already exist but do
   nothing.
2. **Fix Familiar, Grim and Incantation** when used from a pack: the created cards go to the hand.
3. **To Do List.** Get the real `pairs(G.GAME.hands)` order. It can be recovered from recorded
   To Do List effects or logged from the game. Then use that order for the `to_do` pool.
4. **Scoring.** Diagnose the 2 remaining mismatches (Photograph/Abstract mult, Madness/Fish
   chips) with a forged single play in `scripts/replay.py`. Match Lua's float formatting for the
   Ramen-style ±1.
5. **Bridge adapter.** Apply full joker permutations for `rearrange`. Decide on legality
   semantics: keep Jackdaw's stricter `can_use`, and make the bot avoid the picks that BalatroBot
   let through.
6. **Effect-text layer**, needed to run our bot. Generate BalatroBot-style `value.effect`, boss
   and tag effect strings from the game's `en-us.lua` and each card's ability vars. Alternatively,
   have the policy read structured ability fields when the backend is the sim.
