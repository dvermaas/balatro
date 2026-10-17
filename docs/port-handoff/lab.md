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
