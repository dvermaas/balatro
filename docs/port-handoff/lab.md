# lab (jevlab, RustOracle)

- `native/bot/src/oracle.rs`: `RustOracle` (Rust shop forecast, Picker values/pick_pub). `items_forecast` is a stub returning a
  forecast without a `skip` baseline (the bot treats it like Python's failed forecast); TODO: call `forecast_items::items_forecast`.
  `CheckedOracle` serves recorded calls and panics if the computed result differs (ignoring `seconds`, `_runs`).
- Identity: `JEV_REPLAY_ORACLE=rust JEV_REPLAY_GAMES=30 cargo test --release --test bot_replay bot_replay -- --nocapture`
  -> 30/30 arm E games identical with computed forecasts (120 s).
- `native/bot/src/bin/jevlab.rs` (arm E, DecisionCfg::lab(), CodeJev items=false, RumbotGame, RED/WHITE):
  `cargo build --release --bin jevlab; target/release/jevlab --set sim30-a [--limit N] --jobs 8 [--name X] [--out runs/_policy_lab]`
  (or `--seeds A,B`). Sets: sim30-a, sim60-a, gap4-a. Jobs capped at 8. Rows -> `<out>/<name>/results.jsonl` with `_play_bot` keys
  (forecast_s/seconds are wall; spent/reroll_spent are 0 like Python).
- Speed: 8 seeds of sim30-a, 8 threads, release: 7.6 s wall (0.95 s/game wall; slowest game 7.6 s).
- Vs Python arm E traces (8 seeds): same ante/won: TU9KANLF, 2YLF77B5, UDBU5JA1. Different: YZAI217Z (5 vs 6), XFIKZ36U (5 vs 6),
  MGBZKPLC (6 vs 5), 1YJ4DQ2R (8 lost vs 9 won), 665WDGX8 (8 won vs 5 lost). 5/8 differ, more than the 11/60 known Jackdaw
  differences would suggest: NOT yet diagnosed (needs a first-divergence trace of the live Rust run against the Python trace).
- Note: rumbot's card sort-id counter is thread-local and not reset per game; threads playing several games get offset ids
  (only relative order matters to the engine).

## lab-diverge (first-divergence of live jevlab vs Python arm-E traces)
- `jevlab` with env `JEVLAB_TRACE=<dir>` writes `<dir>/<seed>.jsonl.gz` (only "call" lines + "end"; RecGame wraps the game) and prints the
  first differing call against `runs/_port/traces/bot-forecast-long/<seed>.jsonl.gz` (card `id`/`sort_id` and timing keys ignored). It also keeps
  the run dir in %TEMP%/plab_<seed>_<pid> (decisions.jsonl) when tracing.
- Result: YZAI217Z@12, XFIKZ36U@10, 665WDGX8@16, 1YJ4DQ2R@14: Python `buy` (pack), Rust `next_round`. Class (c), NOT engine: the gamestate
  before the call is identical (only card ids differ; voucher/pack ids are 0 in rumbot vs a counter in Jackdaw, and the forecasts do not read
  ids). Cause: `RustOracle::items_forecast` is still a stub (no `skip` baseline), so the Rust bot drops the item layer and the first shop with
  an affordable pack/tarot decides differently ("item forecast: beats buying nothing ..." in Python). The replay never saw it because
  `CheckedOracle` serves the recorded items forecast. MGBZKPLC@34 (Python has no `card` param, Rust `buy {"card":0}`) is the same shop
  situation (not inspected further).
- Needed to fix (not done, out of budget): `forecast_items::items_forecast` = rollout_items._forecast: options(pub) (skip, shop tarot/spectral,
  packs), per rollout k and option: `apply_item_option` then `simcore::play_horizon_traced(.., Some(ItemsDesc))` (targeted/safe tables from
  tarots.rs by rumbot name, suit from the picker's build), `assemble` + vs_skip; on STRICT_ABORT (joker added / copier) rebuild the policy from the
  game's jokers and continue (Python does; Rust has no non-strict continuation yet).
- Profile (item 2) not done.
