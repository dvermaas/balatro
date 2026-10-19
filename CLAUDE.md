# CLAUDE.md

Bot that plays Balatro with JEV (TypeSafe's typed System One model) as a JEV stress test, plus a
local control panel for watching runs and comparing prompt versions. Everything is Rust, in the
`native/` workspace; the Python original (through v32, which the Rust bot matches seed by seed) is in
git history up to `4628654`. See README.md for setup.

## Commands

- Build: `export PATH="$HOME/.cargo/bin:$PATH"; cd native && cargo build --release` (rustup, stable MSVC;
  `~/.cargo/bin` may be missing from PATH). Binaries: `native/target/release/{jevbot,jevlab}`.
- Tests: `cd native && cargo test --release`. Most compare against fixtures and traces recorded from the
  Python bot in `runs/_port` (not in git; `JEV_PORT_DATA` overrides the path); without them those tests
  fail with "No such file". `fc_items` is slow (~2-9 min); `engine_replay` with `IGNORE="].id"`.
  Identity bar (v31 references, regenerated from v32's Python; docs/rust-port-guide.md "Reference data"):
  `bot_replay` 60/60, `fc_items` 2913/2916, `fc_packs` 2223/2224 (the misses are a known Jackdaw bug,
  followups.md), `engine_replay` 47/60 (the rest are known Jackdaw bugs). The references can no longer
  be regenerated, so a behaviour change makes them stale; check it with the lab instead.
  `kernel/tests/matches_reference.rs` pins the scorer to its original implementation.
- Lab (no JEV, free): `jevlab --set sim30-a --arm E|F --jobs 8 [--limit N]` (E = bot-forecast-long,
  F = bot-forecast-items); rows in `runs/_policy_lab/<name>/results.jsonl`. `JEVLAB_TRACE=<dir>` writes
  every game's calls (and keeps the run dirs in $TMPDIR) and prints the first call that differs from
  `runs/_port/traces`.
- Sim games with JEV: `jevbot sim --set sim30-a --jobs 8` (recorded in `runs/sim/`; `--code-jev` for no
  JEV, recorded in `runs/sim_smoke/`). Seed sets live in `native/bot/src/benchmark.rs` (sim30-a, sim60-a,
  gap4-a, bench30-a, ...).
- Sim games need the game's localization in `runs/_loc` (`jevbot extract-loc`, or `$BALATRO_LOC_DIR`).
- Profiling: `JEV_PERF=1` prints coarse timers. For function-level profiles build with
  `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only` and run `jevlab` under `perf record -g`.
- Layout: `native/{bot,kernel,simcore,rumbot,pycompat,typesafe}`, `mods/`, `docs/`, `runs/`,
  `scripts/launch_fast.cmd`.
- Panel: `native/target/release/jevbot panel` → http://127.0.0.1:8765 (`--backend rumbot` for the
  sim, `--code-jev` for no JEV), or `preview_start` with the `jev-panel` config in
  `.claude/launch.json`. **Rebuild and restart it after any `.rs` change**; `native/bot/panel.html`
  is re-read per request (just reload the page).
- Game: close Balatro, run `scripts/launch_fast.cmd` (sets `BALATROBOT_FAST=1`, `BALATROBOT_AUDIO=1`;
  the user wants shaders and music on, so don't add `NO_SHADERS`/headless). Health check:
  `curl -X POST 127.0.0.1:12346 -d '{"jsonrpc":"2.0","method":"health","id":1}'`.
- Panel API: `GET /api/live?since=<event id>`, `/api/runs`, `/api/runs/<run_id>`, `/api/prompts`,
  `/api/benchmark`; `POST /api/{run,pause,step,abandon,settings,benchmark}`.

## Architecture

- `jev_bot::JevBot::step()`: fetch gamestate → `on_<state>` handler → `query()` one JEV `Choice`
  over code-built options `{key: (description, (rpc_method, params))}` → `act()`.
  Single-option decisions skip JEV (`forced`). Unknown states are transitional: sleep and poll.
- `hands.rs` builds candidates; `prompts.rs` holds every string JEV sees; `strategy.rs` holds
  code-level policy (auto-use planets, xMult jokers rightmost via `rearrange`, joker role/synergy
  tags, interest). `step()` runs `strategy::auto_action()` before asking JEV. `decisions/` holds the
  handlers (play, shop, booster); `forecast*.rs` the shop and item forecasts (rollouts on rumbot via
  `simcore`); `jev_kernel` scores every play subset.
- Backends implement `iface::Game`: `balatro::Balatro` (BalatroBot JSON-RPC) and `engine::RumbotGame`
  (rumbot behind a BalatroBot-shaped view, `engine/view.rs`, card text in `simtext.rs`).
- Benchmark: `benchmark.rs` holds fixed seed sets (`NAME` = active); the panel's Benchmark button plays
  them (stops early after 10 seeds if the version trails the best finished one by >2 paired SE), then
  the bot pauses. Runs carry `summary.benchmark`; compare versions in the Benchmark tab. Keep the seed
  list fixed; change `NAME` if you ever change it.
- `recorder.rs`: one dir per run (keyed by seed) under `runs/`; the prompt version is
  `recorder::bot_version`, the sha1 of `src/**/*.rs` in `native/{bot,simcore,kernel,rumbot,pycompat}`.
  Snapshots in `runs/_prompts/`.
- `analytics.rs`: metrics derived from `decisions.jsonl`, cached by mtime.
- Simulator fidelity: README "How faithful is the simulator?" explains the three levels:
  1. vanilla Balatro;
  2. the modded game we play (SMODS + BalatroBot), which is the reference;
  3. rumbot-sim (Jackdaw, the Python engine, until the Python removal).

## Gotchas

- BalatroBot serialises empty Lua tables as `[]`, not `{}`; use `hands::_d()` (or `gs::area`) before reading a dict.
- `gs["cards"]` (the draw pile) is in exact draw order: the next draws are its last entries. Never
  send its order to JEV (that's seeing the future); only order-free aggregates like suit/rank counts.
- Plain cards have label `"Base Card"`; build names from `value.rank`/`value.suit` (`describe()`).
- BalatroBot handles one request at a time. If something blocks the game (an overlay pauses it),
  every call hangs; the 60 s client timeout makes the panel pause with a hint.
- Any edit under `native/{bot,simcore,kernel,rumbot,pycompat}/src` is a new prompt version: update
  `VERSION_NOTE` in `prompts.rs`, and avoid stray edits mid-experiment.
- Speed-only changes must leave games identical seed by seed: run both lab arms on the same seeds with
  `JEVLAB_TRACE` before and after and compare the call logs and every run's `decisions.jsonl`
  (ignore card ids, timings, `prompt_version`). Every mismatch so far was a real bug.
- Caches must be per game (thread-local, reset at `start`), never process-wide: `jevlab` and
  `jevbot sim` run games on threads, and a shared cache made results depend on the other games.
- Rust floats must match the old Python bit for bit where they reach JEV or a record: keep the order
  of float operations, and use `pycompat` for formatting, rounding and `random.Random`.
- `.env` holds `JEV_API_KEY`; never print it. HTTP 402 from TypeSafe = org has no usable credits.
- Don't poll running processes or games in loops (repeated status checks, sleeps): wait for them to finish, or run them in the background and check once.
- Mods live outside the repo in `%APPDATA%\Balatro\Mods`. `mods/jev_tweaks` is the source of truth
  for our mod; copy it there after editing and restart the game. BalatroBot has a local patch in
  `endpoints/start.lua` (viewed_back nil guard) that an update would overwrite.
- More BalatroBot patches (copies plus `.orig` in `mods/balatrobot-patches/`; copy them back over
  `Mods\balatrobot\src\lua` after a BalatroBot update): `cash_out.lua` waits for the game's queued events
  (without it, fast clients got last round's payout and a shop from before tags applied), `buy.lua` counts
  slots like the game does (Negative jokers fit full slots), and `gamestate.lua` reports `state.forced` (Cerulean Bell).
- Balatro's patched Lua source is dumped to `%APPDATA%\Balatro\Mods\lovely\dump`: read it to
  understand game behaviour. Logs: `Mods\lovely\log`.
- Sim fidelity: the reference is the modded game's Lua in `Mods\lovely\dump` (SMODS rewrites some
  functions, e.g. Crimson Heart, card-limit draws, xMult rounding), not vanilla. Fix rumbot in
  `native/rumbot` and note it in `VENDORED.md`, citing the Lua line. A sim change is a new sim
  baseline: lab or benchmark numbers from before it aren't comparable. The Jackdaw-mirroring rumbot
  flags (`fool_always_usable`, `pack_ignores_played`, `jackdaw_unmarked_jokers`,
  `jackdaw_card_pay_unseen`) keep v27 identity; dropping them is a behaviour change.
