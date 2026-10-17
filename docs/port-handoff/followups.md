# Port follow-ups (small gaps found while merging)

- recorder: `states.jsonl.gz` deflate bytes differ from CPython zlib (header and content identical). Fine
  unless something hashes the raw .gz bytes.
- recorder: `list_runs` ported (`recorder::list_runs(root, limit)`, `RunRecorder::list_runs`; fixture test
  `list_runs_matches_python`). The panel's /api/runs still uses analytics::all_runs, as Python's panel does (no limit,
  with metrics). Prompt version = `recorder::bot_version` (sha1 of
  native/{bot,simcore,kernel,rumbot,pycompat} `src/**/*.rs`, sorted); `behaviour_sources` lists them. Needs the
  `native/` dir at runtime (compile-time path, or JEV_NATIVE_DIR); a deployed binary without sources hashes nothing.
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
  tarots.rs once that is merged.
- forecast_apply: selling a ready Invisible Joker (draws RNG) returns Err; untested (not in the sample).
  Campfire/Hologram bumps, negative editions, Juggler/Troubadour/Turtle Bean hand size are coded from
  Jackdaw but may be unexercised by the 400-call fixture.
- engine text: Hiker perma_bonus, Oops! All 6s probability text, and perma_mult/x_mult/h_chips/dollars card
  lines are never emitted because rumbot cards lack those fields. Check how rumbot tracks Hiker's per-card
  chip bonus (does Hiker work in rumbot at all?) and add the fields if needed.
- decisions: `log_forecast_error` now takes the error message and writes `<JevBot.runs_root>/_forecast_errors.log`
  (default "runs", relative to the repo root), with run= set to the current run dir. Python's traceback is replaced
  by the caller's file:line:col (`#[track_caller]`). Python's jev_bot.py step errors log no traceback (only the
  message); panel.py's worker adds `trace=` (format_exc limit 3) to unexpected errors. Rust does not add a trace.
- engine: Mail-In Rebate round rank differs on 4R5NYC8E@56 (Python/Jackdaw rank 10, rumbot 7): find which
  engine draws the round card like the game (Lua: reset_mail_rank) and fix that one.
- engine text mirrors two Jackdaw display quirks for identity: Satellite text always shows $0, and Hologram
  stays X1 after Marble Joker adds a Stone card (the Jackdaw Marble/Hologram bug). Revisit once Jackdaw is
  no longer the reference (the real game shows the live values).
- fc_packs: part 3 compares `describe_lite` (money, names, counts, slots, RNG pools), not the full post-pack
  description; its num_eq treats ints and floats alike. The items end-to-end forecast test covers it.
- forecast_items sets rumbot flags that mirror Jackdaw's OpenBooster/PickPackCard shortcuts
  (`fool_always_usable`, `pack_ignores_played`, `jackdaw_unmarked_jokers`) for v27 identity; drop them once Jackdaw is not the reference.
- analytics (Python too): run_metrics "progress" never adds the final-blind share (last_move only holds
  discards_left, so target is 0): progress == rounds_won. Ported as-is; fix in both if the share was intended.
- panel: `--legacy` (v1 bot) is not ported: `jevbot panel --legacy` prints "the v1 bot is Python-only: run
  src/panel.py --legacy" to stderr and exits 2. Error-event tracebacks not ported (see decisions above).
- forecast_items (fc-items4): more Jackdaw-mirroring for v27 identity, forecast only: Cryptid copies don't grow
  Hologram (Jackdaw game.py:2606-2625 skips playing_card_added), `pack_ignores_played` now also makes rumbot
  `add_random_joker` ignore seen centers (Jackdaw never fills used_jokers); not yet noted in native/rumbot/VENDORED.md.
  Deck/draw pile are re-sorted by uid after a pick (Jackdaw sort_id order). Drop with the other flags once
  Jackdaw is not the reference. fc_packs' describe_lite still skips deck cards, hand levels, joker counters.
- v31 port (2026-10-17): fc_items 2913/2916 and fc_packs post-option state 2223/2224. All four are Arcana
  packs: Jackdaw's pack-created tarot can duplicate one we hold (fc_packs row: The Emperor in a pack with
  The Lovers held makes a second Lovers; rumbot makes The Magician). The game excludes held consumables
  from the pool (G.GAME.used_jokers), so this is a Jackdaw bug: some acquisition path leaves a held card
  unflagged. Not fixed (a sim change is a new baseline and needs sim_parity on the main machine).
