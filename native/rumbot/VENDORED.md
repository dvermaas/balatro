# Vendored: rumbot-sim

Source of [mmank/rumbot-sim](https://github.com/mmank/rumbot-sim) at commit
`888c5165de58d69bd48d9ddbfbd3209f2b064820` (2026-10-05), MIT (see LICENSE). Only `src/` (without
`src/bin/`), `Cargo.toml` (release profiles removed: this is a dependency of `native/`), the README and
PORTING.md are copied; upstream's tests and fixtures (36 MB) are not.

Stage B of the Rust port (docs/rust-port-scope.md, issue #1) uses it as the rollout and sim engine.
Local changes to this copy are listed here, newest last:

- `GameState.no_shop` (game.rs, game_clone.rs): `_open_shop` leaves the shop empty when set, for
  frozen-build forecast rollouts (like Jackdaw's `rollout_skips_shop_population` patch).
