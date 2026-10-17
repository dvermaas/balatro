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
