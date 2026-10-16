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
