# lab (jevlab, RustOracle)

- `native/bot/src/oracle.rs`: `RustOracle` (Rust shop forecast, Picker values/pick_pub). `items_forecast` calls
  `forecast_items::items_forecast` (the Rust items forecast).
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

## perf (profile of jevlab arm F, 2YLF77B5+1YJ4DQ2R, --jobs 1)
- No sampling profiler (samply needs admin; declined). Coarse timers: `JEV_PERF=1` prints `PROF <label> <s> <calls>` at exit (`src/prof.rs`).
- Before (20.3 s total, 2 games): items_forecast 13.9 s (68%): apply_item_option 7.0 s (35%; pack options 925 us/call: engine full-gamestate view
  twice per pick), play_horizon 3.3 s (16%), policy_of_game 3.2 s (16%, rebuilt even when the policy cache hit). forecast (shop) 2.7 s (13%,
  play_horizon 2.2). Rest 3.7 s: h.selecting_hand 3.5 s (139 calls, 25 ms; jokers_build/play_candidates/setup_plays/build_decide only ~0.9 s of
  it, ~2.6 s still unattributed), game.act+gamestate 0.6 s.
- Fixed (results identical): policy_of_game only on a policy-cache miss (3.2 s -> 0.1 s); `ItemCache` caches the pub->game build per option /
  pack (apply_nopack 750 -> 30 us); pack loop reads `RumbotGame::pack_view()` (money/jokers/hand/pack only) and `act_only` skips the discarded
  post-action view; RecGame no longer logs/clones every call unless JEVLAB_TRACE.
- After: items_forecast ~9.8 s of 16.5 s total in the same measurement (machine load varies a lot between runs, so compare ratios): apply_item_option
  now ~2.8 s vs play_horizon 3.6 s.
- Next: selecting_hand's unattributed ~2.6 s, the remaining h.shop non-oracle ~3 s, pack `pk.state` (173 us) / `pk.pick`, per-rollout play_horizon itself.
- Verified: fc_items (FC_STRIDE=10 and full), fc_packs, live 8 seeds identical on arms E (1YJ4DQ2R call 330 known) and F.

## perf round 2 (arm F, 2YLF77B5+1YJ4DQ2R, --jobs 1; same session, before -> after)
- New timers: `sh.odds_forecast/preserve/discard_cands/query` (selecting_hand), `s.shop_forecast/item_forecast/readiness/build/gain/recent/score_items` (shop).
- Attribution (before): total 14.9 s. h.shop 12.1: s.shop_forecast 3.7 (f.play_horizon 2.9), s.item_forecast 3.8, **s.score_items 3.0** (the CodeJev stand-in
  re-runs `items_forecast` n=FORECAST_N on 75 shops: it is a different n/keys, so not cacheable; it disappears with a real JEV), s.build 0.9 + sh.build_decide 0.4 +
  s.readiness 0.3 (play_mix/power kernel scoring; play_mix already process-cached by signature). h.selecting_hand 2.15: odds_forecast 1.15, build 0.43, query 0.13,
  setup_plays 0.07 (the old "unattributed 2.6 s" was odds::forecast: 32 samples x discards of kernel best_score, not JSON/describe).
  items: fi.play_horizon 4.4, fi.apply_PACK 1.47 (pk.state 0.57, pk.pick 0.31, open_pack 0.17).
- Fixed: odds/readiness sorts allocated `[String;6]` keys per comparison -> `sort_by_cached_key` (stable, same order). Total 14.9 -> 13.9 s, odds_forecast 1.15 -> 1.04 s,
  selecting_hand 2.15 -> 1.99 s. The rest is real kernel/rollout compute; JSON building (pack_view etc.) is already lean.
- Verified: fc_items STRIDE=10, fc_packs, bot_replay 8, live 8 seeds identical on F and E (1YJ4DQ2R call 330 known).
- Next if needed: the kernel itself (each_play enumerates all <=5-subsets per sample), play_horizon.
