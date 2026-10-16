# CLAUDE.md

Bot that plays Balatro with JEV (TypeSafe's typed System One model) as a JEV stress test, plus a
local control panel for watching runs and comparing prompt versions. See README.md for setup.

## Commands

- Venv: `.venv/Scripts/python` (Python 3.14, `typesafe-sdk`). No other deps; panel is stdlib.
- Native scorer (optional, v24+): with Rust installed (rustup, MSVC toolchain), `.venv/Scripts/python -m pip
  install ./native` builds `jev_native`; reinstall after editing `native/src`. `native.backend()` says which
  scorer runs (also `summary.scorer`); `JEV_NATIVE=0` forces Python. Results are identical either way
  (`tests/test_native.py`). Also v24: `.venv/Scripts/python -m pip install ./native/sim` builds `jev_sim`, which
  runs the shop forecast's rollouts on rumbot-sim (vendored in `native/rumbot`, local patches in its
  `VENDORED.md`); `summary.rollouts` says which engine ran. Parity checks: `scripts/diagnostics/rumbot_*.py`.
- Layout: code in `src/` (flat modules, run as scripts; no package, so the hashed files never need
  import edits), `scripts/`, `mods/`, `docs/`, `runs/` at the repo root.
- Panel: `.venv/Scripts/python src/panel.py` → http://127.0.0.1:8765, or `preview_start` with the
  `jev-panel` config in `.claude/launch.json`. **Restart it after any `.py` change**;
  `panel.html` is re-read per request (just reload the page).
- Game: close Balatro, run `scripts/launch_fast.cmd` (sets `BALATROBOT_FAST=1`, `BALATROBOT_AUDIO=1`;
  the user wants shaders and music on, so don't add `NO_SHADERS`/headless). Health check:
  `curl -X POST 127.0.0.1:12346 -d '{"jsonrpc":"2.0","method":"health","id":1}'`.
- Panel API: `GET /api/live?since=<event id>`, `/api/runs`, `/api/runs/<run_id>`, `/api/prompts`;
  `POST /api/{run,pause,step,abandon,settings}`.

## Architecture

- `jev_bot.JevBot.step()`: fetch gamestate → `on_<state>()` handler → `ask()` one JEV `Choice`
  over code-built options `{key: (description, (rpc_method, params))}` → `act()`.
  Single-option decisions skip JEV (`forced`). Unknown states are transitional: sleep and poll.
- `hands.py` builds candidates; `prompts.py` holds every string JEV sees; `strategy.py` holds
  code-level policy (auto-use planets, xMult jokers rightmost via `rearrange`, joker role/synergy
  tags, interest). `step()` runs `strategy.auto_action()` before asking JEV.
- Benchmark: `benchmark.py` holds fixed seed sets (`NAME` = active); `POST /api/benchmark` plays them (stops early
  after 10 seeds if the version trails the best finished one by >2 paired SE), then the bot
  pauses. Runs carry `summary.benchmark`; compare versions in the Benchmark tab
  (`GET /api/benchmark`). Keep the seed list fixed; change `NAME` if you ever change it.
- `recorder.py`: one dir per run (keyed by seed) under `runs/`; `PROMPT_VERSION` = sha1 of
  `prompts.py` + `hands.py` + `strategy.py` (+ `decisions.py` from v6, `jokers.py` from v8, `odds.py` from v9, `build.py` + `tarots.py` from v10, `readiness.py` from v13, `rollout.py` from v19, `rollout_items.py` from v21, `native.py`, `native/` (incl. `kernel/`, `sim/`, `rumbot/`), `sim_bridge.py` + `sim_bridge_jokers.py` from v24). Snapshots in `runs/_prompts/` (v1 was reconstructed
  by hand).
- `analytics.py`: metrics derived from `decisions.jsonl`, cached by mtime.

## Gotchas

- BalatroBot serialises empty Lua tables as `[]`, not `{}`; use `hands._d()` before `.get()`.
- `gs["cards"]` (the draw pile) is in exact draw order: the next draws are its last entries. Never
  send its order to JEV (that's seeing the future); only order-free aggregates like suit/rank counts.
- Plain cards have label `"Base Card"`; build names from `value.rank`/`value.suit` (`describe()`).
- BalatroBot handles one request at a time. If something blocks the game (an overlay pauses it),
  every call hangs; the 60 s client timeout makes the panel pause with a hint.
- Editing `prompts.py`, `hands.py`, `strategy.py`, `decisions.py`, `jokers.py`, `odds.py`, `build.py`, `tarots.py`, `readiness.py`, `rollout.py`, `rollout_items.py`, `native.py` or the Rust code (`native/src/lib.rs`, `native/Cargo.toml`, `native/kernel/`, `native/sim/`, `native/rumbot/`, `sim_bridge.py`, `sim_bridge_jokers.py`) creates a new prompt version: update `VERSION_NOTE`, and avoid
  stray edits mid-experiment.
- Mods live outside the repo in `%APPDATA%\Balatro\Mods`. `mods/jev_tweaks` is the source of truth
  for our mod; copy it there after editing and restart the game. BalatroBot has a local patch in
  `endpoints/start.lua` (viewed_back nil guard) that an update would overwrite.
- Balatro's patched Lua source is dumped to `%APPDATA%\Balatro\Mods\lovely\dump`: read it to
  understand game behaviour. Logs: `Mods\lovely\log`.
- `.env` holds `JEV_API_KEY`; never print it. HTTP 402 from TypeSafe = org has no usable credits.
- The user's weekly Claude usage is tight: prefer targeted reads and avoid long polling loops.
- Prompt-version ids hash the exact bytes of `src/prompts.py`, `src/hands.py`, `src/strategy.py`, `src/decisions.py`, `src/jokers.py`, `src/odds.py`, `src/build.py`, `src/tarots.py`, `src/readiness.py`, `src/rollout.py`, `src/rollout_items.py`
  (`.gitattributes` keeps `*.py` byte-exact). Moving them is fine; editing them (even an import or
  line ending) makes a new version. `src/legacy/v1/` must stay byte-identical (id f67d4d14).
- Write Python edits with a script that preserves each file's line endings, or the Edit tool; heredocs are fine with a quoted delimiter, but
  inside non-raw Python strings `"\b"` becomes a backspace: use raw strings, `chr(92)` or the Edit tool.
