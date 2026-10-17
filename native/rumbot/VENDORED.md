# Vendored: rumbot-sim

Source of [mmank/rumbot-sim](https://github.com/mmank/rumbot-sim) at commit
`888c5165de58d69bd48d9ddbfbd3209f2b064820` (2026-10-05), MIT (see LICENSE). Only `src/` (without
`src/bin/`), `Cargo.toml` (release profiles removed: this is a dependency of `native/`), the README and
PORTING.md are copied; upstream's tests and fixtures (36 MB) are not.

Stage B of the Rust port (docs/rust-port-scope.md, issue #1) uses it as the rollout and sim engine.
Local changes to this copy are listed here, newest last:

- `GameState.no_shop` (game.rs, game_clone.rs): `_open_shop` leaves the shop empty when set, for
  frozen-build forecast rollouts (like Jackdaw's `rollout_skips_shop_population` patch).
- SMODS xMult arithmetic (effects.rs `ScoreContext::times_mult`): `mult + mult * (x - 1)` instead of
  `mult * x`, as Steamodded does (Jackdaw: `sim_patches.smods_xmult_arithmetic`).
- Ramen (joker_specs.rs `ramen_discard`): loses X0.01 once per discarded card, as card.lua's
  per-card `context.discard` does, instead of one `-0.01 * n` step (last-bit xMult difference).
- Certificate (joker_specs.rs `certificate`): the created card goes through the blind's `debuff_card`, as
  card.lua's event does (`G.GAME.blind:debuff_card(_card)`), so it is debuffed under The Head etc.
- Crimson Heart (game.rs `_drawn_to_hand`, new `crimson_chosen`): the SMODS rule the game runs (blind.lua
  drawn_to_hand in the lovely dump): only the joker it held is released, and the pick is over non-debuffed
  jokers except that one (all of them if none is left). Vanilla re-enabled every joker first
  (Jackdaw: `sim_patches.crimson_heart`). Found by the v25 identity check (9YUMEW2U).
- Lost round (game.rs `_lose_round`): the end_of_round joker effects now run on the losing play too (when no
  Mr. Bones saves it). state_events.lua:107 (end_round) calls SMODS.calculate_context({end_of_round}) before
  it looks at game_over, and card.lua:3283-3349 (Popcorn, ...) has no game_over guard, so Popcorn is eaten,
  Rocket/Egg/Gift Card tick on a loss. Visible only in the final view (R8ZXJN6L@342). Found by the engine replay.
- Swashbuckler checked, no rumbot change (KGA553W7@375): card.lua:4609-4617 sums `sell_cost` of every other
  joker in G.jokers with no debuff test; rumbot matches. Jackdaw (jokers.py `_swashbuckler`) skips debuffed
  jokers (`and not j.debuff`), so under Crimson Heart it scores less (Mult 8 vs 11: 102 vs 111 x 122).

- shop_pool.rs `soulable`: a Spectral Soul hit still makes the Black Hole roll (common_events.lua:2088-2100).
  `Tarot_Planet` pool (interleaved by `order`) + `random_consumables_set`; The Fool with nothing to copy draws from it
  (Jackdaw patch). GameState flags `fool_always_usable`, `pack_ignores_played` (default false) mirror Jackdaw's pack
  path in item forecasts.

## fc-items3: forecast-only Jackdaw mirror
`GameState.jackdaw_card_pay_unseen` (default false; set only by the Rust items forecast when Python would have fallen back to Jackdaw): Bootstraps and Bull
do not count the hand's Gold Seal/Lucky pay. The game counts it (card.lua:1400-1403 dollar_buffer, read at card.lua:4297 and 4407); Jackdaw does not. No
change to rumbot's own behaviour.

## fc-items4: forecast-only Jackdaw mirror (pack path)
`pack_ignores_played` now stays on for the whole pick and `add_random_joker` (game_jokers.rs) then ignores the seen centers, because Jackdaw never fills
`used_jokers` after build_state, so Judgement/Wraith may create a joker the player owns (the game excludes it). Default false; rumbot's own behaviour unchanged.

## fc-items5: forecast-only Jackdaw mirror (unmarked owned jokers)
`GameState.jackdaw_unmarked_jokers` (default empty; set only by the Rust items forecast on a Jackdaw-replay restart): joker keys owned when the state was built.
`seen_centers` skips one occurrence of each, so shops and packs may roll a joker the player owns. The game excludes it (G.GAME.used_jokers is set at
creation, card.lua:349-354); Jackdaw's build_state leaves used_jokers empty. No change to rumbot's own behaviour.

## Speed only (no behaviour change)
Checked game by game on the lab arms (call logs and every recorded decision identical):
- `HandLevels.levels` / `plays` (hands.rs) are a `HandMap<i32>`, a 12-slot array indexed by `HandType`, instead of a
  `HashMap<HandType, i32>`. The two `.plays.iter()` callers (planet pools) only build sets from it, so the order
  (now weakest hand first instead of the hash order) does not matter.
- `GameState::active_jokers_iter` (game.rs): the row of non-debuffed jokers without copying it; the read-only scans
  (`has_pareidolia`, `boss`, hand and discard counts, ...) use it. `active_jokers` / `calculating_jokers` still copy,
  for the loops that change jokers while they walk the row.
- `RunRng::pseudoseed` (rng.rs) advances an existing pool in place instead of allocating its key again.
- `build_consumable_pool` (shop_pool.rs) matches `c_<key>` without a `format!` per row.
- `GameState.keep_logs` (default false) and the `glog!` macro (lib.rs): log lines are formatted and kept
  only when `keep_logs` (or printed when `verbose`). Nothing reads `logs`; formatting them cost several
  percent of every scored hand in rollouts.
- `ScoreContext.events` counts chips/mult changes; `held_card_once` (scoring.rs) compares it instead of
  the length of `ScoreContext.log`, which is now filled only when `keep_log` is set.
- `RunRng::pseudoseed` hashes a new stream's key and seed without joining them first (`pseudohash2`).
