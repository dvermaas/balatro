# decisions handoff: layout of `native/bot/src/decisions/` (task dec-core)

Branch `port/dec-core`. All signatures are final; fill in the `todo!("decisions <name>")` bodies.
Python line ranges are for `src/decisions.py`. Replay: stops at `selecting_hand todo!` after 10 matched
records in all 60 games (before: 4, at blind_select).

## Naming convention
Python names with the leading underscore dropped (`_round_info` -> `round_info`), `pub(crate)`; handlers are
`pub`. Re-exports from hands.rs keep hands.rs names (`_boss_levels`, `_boss_allows`, `BossAllow`, in play.rs).
Plays, infos, forecasts, objs are `serde_json::Value` objects (Python dicts; key order = Python order).
`RpcError` results where `bot.act` can raise. `bot.dec` = DecisionCfg (below). Helpers in mod.rs:
`instr(key)` = INSTRUCTIONS[key], `option_tpl(key)` = OPTIONS[key], `voucher(cfg, name)`.

## File map
mod.rs (DONE): constants 28-92 (except the lab-overridable ones), VOUCHERS (`VOUCHERS` slice of
(name, desc, strong) + `voucher()`), `round_info` 95, `deck_remaining` 107, `levels` 119 (names: HashSet),
`run_context` 124 -> (mains, ctx), `blind_select` 143, `remember` 181, `recent` 191, `play` 195.

play.rs (all todo except consts/aliases): `CLEAR_MARGIN`, `PLAY_NOW_UNDER_PACE`, `PLAY_FORECAST_BOSSES` done.
`clears` 162, `play_obj` 166, `preserve_discard` 232, `setup_plays` 264, `selecting_hand` 335-452.
Aliases `ScorerFn` / `AllowedFn` (closures over `scorer(infos, combo, name, scoring)` /
`allowed(combo, name)`); wrap `jokers::Scorer` / `BossAllow` into them, or use hands.rs
`ScorerArg`/`Allowed` where the hands.rs functions need the kernel path.

shop.rs (all todo): `discard_obj` 453, `score_items` 468 (type `ShopItems = Vec<(key, (desc, obj, Action))>`),
`weakest_jokers` 487, `shop_forecast` 499, `log_forecast_error` 529, `item_forecast` 548, `pct` 577,
`forecast_fields` 581, `forecast_note` 597, `economic` 609, `shop` 616-823 (nested `obj` at 647 stays a
private helper/closure inside), `shop_act` 826-848 (adds `dumps(action)` to `bot._failed` then returns Err).

booster.rs (all todo): `booster` 851-947, `gain` 950-987.

## DecisionCfg (`bot.dec`, field on JevBot, set to `production()` by `JevBot::new`)
Replaces the module globals the lab overrides (policy_lab.py `_play_bot` 439-444):
`forecast_n` (production 24, lab 16), `forecast_budget: Option<f64>` (4.0 / None), `forecast_bosses` (3 / 3),
`strong_vouchers` (true / false = every VOUCHERS strong flag reads False; use `voucher(&bot.dec, name)`).
`DecisionCfg::lab()` = arms bot-forecast-long/items; `lab_with(n, bosses)` for BOT_ARMS with "n"/"bosses".
`replay.rs` sets `bot.dec = lab()`. Use `bot.dec.forecast_n` etc. wherever Python reads FORECAST_N,
FORECAST_BUDGET, FORECAST_BOSSES, VOUCHERS. Other constants (FORECAST_TIMEOUT, SWAP_SELLS, ...) are plain consts in mod.rs.

## Notes
- mod.rs functions are unit-untested beyond the replay (round_info, run_context, blind_select are exercised by
  the replay up to the first selecting_hand; deck_remaining/levels first used by the hand handler).
- `remember`/`play` take `combo: &[usize]`; `recent()` returns a clone of `bot._recent`.
