# Port follow-ups (small gaps found while merging)

- recorder: `SystemClock` is UTC; Python names run dirs and stamps in local time. Use local time before
  the Rust bot writes real runs (Windows `GetLocalTime` or a small tz dependency).
- recorder: `states.jsonl.gz` deflate bytes differ from CPython zlib (header and content identical). Fine
  unless something hashes the raw .gz bytes.
- recorder: `snapshot_prompts`, `prompt_versions`, `list_runs` not ported (shell wave: panel/analytics).
  `version_hash` takes its file list as a parameter: decide which Rust sources define a prompt version.
- readiness: `play_mix` keeps Python's process-wide `_MIX_CACHE` (cleared past 256 entries). In Rust keep
  one cache per game; check in the full-game replay that no recorded game hit a mid-game clear.
- odds: only 9 of 1545 fixture plans took the exact-enumeration path; the decisions replay covers the
  real call path (`score_after_discard`).
- strategy: `interest(money)` handles int money only (BalatroBot money is always int).
- pycompat statistics: int and float lists only, no mixed lists; inf/nan approximate.
- typesafe: Retry-After HTTP dates not parsed; connection-error text from ureq; timeouts untested offline.
- engine: card ids differ (Jackdaw sort_id vs rumbot uid); replay ignores `].id`. Decide whether the bot
  or anything recorded depends on ids (the bot addresses cards by index).
- engine view: slot 0 of the first shop shows `p_buffoon_normal_1` until bought, to match Jackdaw
  (shop.py:180 always deals _1; the game and rumbot roll 1-2). Remove once Jackdaw is no longer the
  reference (identical in play).
- forecast_stats: `usable()` copies tarots.py's TARGETED/SAFE_UNTARGETED keys (`// SHARED?`); point it at
  tarots.rs once that is merged. Items-forecast assembly (rollout_items._forecast's result dict) still to port
  (forecast task 9).
- duplicated helpers to consolidate: Python-style `repr` of a JSON value (forecast_state.rs `py_repr`, picker.rs)
  -> pycompat; rollout.py `candidates`/`_rv`/`_straight` copy in picker.rs -> the rollout port; tarots
  MEASURABLE copy in picker.rs -> tarots.rs; simtext `fmt_g` (%.Ng) -> pycompat.
- forecast_apply: selling a ready Invisible Joker (draws RNG) returns Err; untested (not in the sample).
  Campfire/Hologram bumps, negative editions, Juggler/Troubadour/Turtle Bean hand size are coded from
  Jackdaw but may be unexercised by the 400-call fixture.
- engine text: Hiker perma_bonus, Oops! All 6s probability text, and perma_mult/x_mult/h_chips/dollars card
  lines are never emitted because rumbot cards lack those fields. Check how rumbot tracks Hiker's per-card
  chip bonus (does Hiker work in rumbot at all?) and add the fields if needed.
- decisions: `discard_obj` exists twice (shop.rs real, play.rs private `// SHARED?` copy): drop the copy.
  `log_forecast_error` has no traceback, empty run=, and writes runs/_forecast_errors.log under a fixed
  repo path: make the root configurable on JevBot.
- engine: Mail-In Rebate round rank differs on 4R5NYC8E@56 (Python/Jackdaw rank 10, rumbot 7): find which
  engine draws the round card like the game (Lua: reset_mail_rank) and fix that one.
- engine text mirrors two Jackdaw display quirks for identity: Satellite text always shows $0, and Hologram
  stays X1 after Marble Joker adds a Stone card (the Jackdaw Marble/Hologram bug). Revisit once Jackdaw is
  no longer the reference (the real game shows the live values).
- decisions: booster.rs has a private pack-only copy of `score_items` (`// SHARED?`); switch to shop.rs's.
- fc_packs: part 3 compares `describe_lite` (money, names, counts, slots, RNG pools), not the full post-pack
  description; its num_eq treats ints and floats alike. The items end-to-end forecast test covers it.
- forecast_items sets two rumbot flags that mirror Jackdaw's OpenBooster/PickPackCard shortcuts
  (`fool_always_usable`, `pack_ignores_played`) for v27 identity; drop them once Jackdaw is not the reference.
- analytics (Python too): run_metrics "progress" never adds the final-blind share (last_move only holds
  discards_left, so target is 0): progress == rounds_won. Ported as-is; fix in both if the share was intended.
