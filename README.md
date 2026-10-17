# JEV × Balatro

A stress test for **JEV**, TypeSafe's typed "System One" model: a bot that plays
[Balatro](https://store.steampowered.com/app/2379780/Balatro/) by reading the live game state,
asking JEV to pick the next move, executing it, and repeating. Every decision is recorded so
prompt changes can be compared run by run.

```
Balatro (+ BalatroBot mod)  ──JSON-RPC──▶  jev_bot.py  ──typed Choice──▶  JEV API
        ▲                                     │
        └────────────── action ◀──────────────┘          panel.py: control panel + run analytics
```

**How decisions work.** Code enumerates the sensible legal moves (best play per hand type with a
score estimate, a few discard strategies, shop buys, pack picks) and JEV answers one `Choice`
question over them, returning a probability for every option. Code owns the rules and execution;
JEV supplies the judgment. In the shop, code forecasts each option by simulating the next few
blinds (rollouts) and buys outright when one is clearly better; JEV rates the close calls.

**Where games run.** The same bot plays either the real game (through BalatroBot) or a
simulator: [Jackdaw](third_party/jackdaw), a pure-Python Balatro engine, wrapped to look like
BalatroBot. The forecasts' rollouts also run on a simulator, in Rust when the native modules are
built. Big experiments run in the simulator; see [How faithful is the simulator?](#how-faithful-is-the-simulator)
for how closely it matches the game.

## Setup (one-time)

Already done on this machine; listed here so it can be redone.

1. **Lovely injector** (v0.10+): put `winmm.dll` next to `Balatro.exe`
   (`C:\Program Files (x86)\Steam\steamapps\common\Balatro`). Delete it to go back to vanilla.
2. **Mods**, in `%APPDATA%\Balatro\Mods\`:
   - `smods/`: [Steamodded](https://github.com/Steamodded/smods)
   - `balatrobot/`: [BalatroBot](https://github.com/coder/balatrobot) (`balatrobot.json`, `balatrobot.lua`, `src/lua`)
   - `jev_tweaks/`: copy of [`mods/jev_tweaks`](mods/jev_tweaks) from this repo
3. **Local BalatroBot patch.** In `Mods/balatrobot/src/lua/endpoints/start.lua`, guard the
   `viewed_back` line; otherwise starting a new run after a game over crashes:
   ```lua
   if G.GAME.viewed_back then G.GAME.viewed_back:change_to(deck_data) end
   ```
   Re-apply it after updating BalatroBot.
4. **Python env**: `uv venv .venv` then `uv pip install --python .venv typesafe-sdk`, and
   `git submodule update --init` for Jackdaw.
5. **`.env`**: `JEV_API_KEY=...` (a TypeSafe API key; the org needs credits on the API).
6. **Rust modules (optional, about 8x faster forecasts)**: install Rust with rustup (stable, MSVC),
   then `uv pip install --python .venv/Scripts/python.exe ./native ./native/sim`. Rebuild after any
   change under `native/` (add `--reinstall-package jev-sim` or `jev-native`). Without them, or with
   `JEV_NATIVE=0`, the same code runs in Python with identical results.
7. **Localization for the simulator**: `python scripts/extract_localization.py` writes `runs/_loc`
   (card effect text) from the game files.

## Running

1. Close Balatro, then start it with **[`scripts/launch_fast.cmd`](scripts/launch_fast.cmd)**: BalatroBot fast
   mode (10× game speed) with audio on. Launching from Steam also works, but at normal speed
   and muted.
2. Start the control panel and open http://127.0.0.1:8765:
   ```
   .venv\Scripts\python src\panel.py
   ```
3. Press **Run** (or Space). It starts paused.

### Control panel

- **Navbar:** Run/Pause, Step (one decision), `⋯` for delay, auto-start next run, abandon run.
  The pill shows whether the bot is running, which prompt version is live, how many runs it has,
  decisions per minute and average run time. Keys: `Space` run/pause, `S` step, `1/2/3` tabs.
- **Live:** current blind, chip progress, hands/discards, money, hand, jokers, and a feed
  of decisions with JEV's probability for each option (click to see all options).
- **Runs:** every recorded run, filterable by prompt version. Click one for a timeline grouped
  by ante/round, including the exact state sent to JEV for each decision.
- **Prompts:** one card per prompt version with stats (ante reached, wins, confidence per
  decision type, pack take rate, wasted discards, jokers held) and ▲/▼ against the previous
  version, plus the full prompt text with changes highlighted.

## Tuning prompts

All text JEV reads lives in **[`src/prompts.py`](src/prompts.py)**: game rules, the question per decision
type, and the option templates. Always-right moves and the context JEV gets live in
**[`src/strategy.py`](src/strategy.py)**: auto-using planets, keeping xMult jokers rightmost, and tagging
jokers by role and by synergy with the run's main hands. The version id is a hash of every file
that shapes a decision (the full list is `PROMPT_VERSION` in [`src/recorder.py`](src/recorder.py): the
policy modules, the forecasts, and since v24 the Rust sources), so **any edit to one of them
creates a new version** (v1, v2, …) with its own stats. Update `VERSION_NOTE` to say what changed
and restart the panel.

### Benchmark

To compare versions cleanly, open the **Benchmark** tab and press **Benchmark current version**.
The bot plays a fixed seed set ([`src/benchmark.py`](src/benchmark.py), `NAME`; 30 seeds) and stops
when done, or early after 10 seeds if the version clearly trails the best one. The tab shows a
summary per version and a seed × version grid of the ante reached. Seeded runs don't trigger
unlocks. Thirty games only detect differences of about 0.9 ante, so small changes are tested in
the simulator first (next section).

## Simulator and policy lab

- **Sim games with JEV**: `python scripts/sim_run.py --set sim30-a --jobs 8` plays a seed set in
  the simulator with the real bot code and real JEV calls. Runs are recorded under `runs/sim/`
  and never share benchmark cells with real-game runs.
- **Policy lab (no JEV)**: `python scripts/policy_lab.py --set sim30-a --arms bot-forecast-long`
  plays the production code path with JEV replaced by the top-ranked option, in parallel and at no
  token cost. Arms compare policies on the same seeds (paired), with sequential stopping. See
  [docs/policy-lab.md](docs/policy-lab.md).
- **Speed**: with the Rust modules, 56 lab games take about 60 s on 12 workers, against 680 s in
  pure Python.
- **Current baseline (v31, 2026-10-17)**, code only (no JEV), arm `bot-forecast-items`:
  - `sim60-a`: mean ante **6.50**, 10 wins out of 60 (v27: 5.90, 4 wins);
  - 60 fresh seeds (`--seeds 60`): **6.63**, 10 wins (v27: 6.15, 6 wins);
  - 200 fresh seeds (`--seeds 200 --seed-offset 60`): **6.68**, 40 wins (20%; v28: 6.46, 27 wins);
  - v28 planets, v29 Negative jokers, v30 Verdant Leaf, v31 auto-buy bar 1 SE: see
    [docs/policy-lab.md](docs/policy-lab.md), "Mid-game scaling experiments" and after;
  - about a third of runs reach ante 8, where the finisher bosses (Violet Vessel, Crimson Heart,
    Amber Acorn) take most of them.

  The last JEV sim benchmark (v23, `sim30-a`, before later sim fixes) reached 6.43. Thirty seeds
  detect about 0.9 ante, so test changes code-only on 60+ seeds first.

## Rust bot

The whole bot is in Rust: `native/bot` (crate `jevbot`) holds JevBot, the decisions, the forecasts, the
recorder, the TypeSafe client, the panel and the policy lab. The game engine is `native/rumbot`
(rumbot-sim fork). The port is frozen at Python v27: for the same seeds it gives the same results, and
Python stays the reference (`jevbot panel --legacy` is Python-only: run `src/panel.py --legacy`).
Porting history and conventions: [docs/rust-port-guide.md](docs/rust-port-guide.md),
[docs/rust-port-scope.md](docs/rust-port-scope.md); open items: [docs/port-handoff/followups.md](docs/port-handoff/followups.md).

Build (Git Bash, `export PATH="$HOME/.cargo/bin:$PATH"` first):

```
cd native/bot && cargo build --release --bins
target/release/jevbot play [--backend balatrobot|rumbot] [--seed S] [--deck RED] [--stake WHITE]     [--model jev-latest] [--runs N] [--code-jev]
target/release/jevbot panel [--port 8765] [--backend balatrobot|rumbot] [--code-jev]
target/release/jevlab --set sim30-a --arm E|F --jobs 8 [--limit N] [--name X]
```

- `play --backend balatrobot` plays the modded game at 127.0.0.1:12346; `rumbot` runs the headless
  engine and logs under `runs/sim`. `--code-jev` uses CodeJev instead of JEV (no key, no tokens). The
  JEV key comes from `.env`.
- `jevlab` is the Rust policy lab, no JEV: arm E (`bot-forecast-long`) or F (`bot-forecast-items`).
  Rows go to `runs/_policy_lab/<name>/results.jsonl`.
- `JEV_PERF=1` prints coarse profiling timers. `JEVLAB_TRACE=<dir>` writes each game's calls and
  prints the first call that differs from the Python trace in `runs/_port/traces`.
- Sim games need the game's localization in `runs/_loc` (`scripts/extract_localization.py`), as before.

Identity (frozen at Python v27):

- Bot replay: 60/60 games identical. Forecasts: shop 2475/2475, packs pass, items 3193/3193. All-Rust
  forecast replay: 60/60.
- Live lab, 8 traced seeds: 15/16 calls identical. The one difference (1YJ4DQ2R, arm E) is a Jackdaw
  Standard-pack bug, not the Rust bot.
- sim60-a, arm F: Rust 5.93 +- 0.22 ante, 6/60 wins; Python v27 5.90 +- 0.22, 4/60 wins; 54/60 games
  end the same.
- Engine replay (`engine_replay`, IGNORE="].id"): 47/60 games identical. The rest are Jackdaw bugs and
  the Mail-In Rebate rank (followups.md).

Speed (8 seeds, summed per-game seconds, arm E / arm F):

| Path | Arm E | Arm F |
|---|---:|---:|
| Pure Python (`JEV_NATIVE=0`) | 887 s | 1030 s |
| Python + native kernels (v27) | 87 s | 115 s |
| Rust | 49 s | 58 s |

Rust is about 18x faster than pure Python and about 2x faster than v27.

Live tests (2026-10-17): one real game with CodeJev and one with real JEV, both played through `jevbot play`
against the modded game.

Jackdaw-mirroring flags: the forecasts and the engine set a few rumbot flags that copy Jackdaw's
quirks so the results match the Python reference (`fool_always_usable`, `pack_ignores_played`,
`jackdaw_unmarked_jokers`, `jackdaw_card_pay_unseen`; see `native/rumbot/src/game.rs` and
`native/rumbot/VENDORED.md`). Drop them once Jackdaw is no longer the reference.

## How faithful is the simulator?

There are three versions of "the game" here, and they can differ:

1. **Vanilla Balatro**, as sold.
2. **The game the bot plays**: Balatro with Lovely (a loader that patches the game's Lua at
   startup and changes no rules itself), **Steamodded (SMODS)**, which BalatroBot needs, and
   BalatroBot. This is the reference: the simulator must match *this*, and the patched Lua that
   actually runs is dumped to `%APPDATA%\Balatro\Mods\lovely\dump`.
3. **The simulators**: Jackdaw (Python) plays whole games, and rumbot-sim (Rust, `native/rumbot`)
   runs the forecasts' rollouts.

**SMODS vs vanilla.** SMODS rewrites some game functions so mods can coexist, and a few
rewrites change behaviour in edge cases. Ones we hit:
- xMult arithmetic (rounding).
- Crimson Heart: it only releases the joker it disabled, where vanilla re-enables every joker.
- Hand slots that grow mid-round are filled from the deck at once.
- Hand-type iteration order, which matters for To Do List.

In normal play these rarely matter, and we model the SMODS behaviour because that is what the bot
faces. BalatroBot also differs from the game: it accepts a few consumable uses the game would
refuse, for example Judgement with full joker slots.

**Game vs Jackdaw.** Jackdaw had bugs. Each one we confirmed against the Lua dump, or in a live
test in the real game, is fixed in [`src/sim_patches.py`](src/sim_patches.py) (about 20 patches, each citing
its source). Checks:
- *Real-replay parity* (`scripts/diagnostics/sim_parity.py`): 336 recorded real games replayed
  action by action. 329 replay to the end with every recorded state identical, and 25,304 of
  25,454 states match exactly. The 7 that diverge are listed in `docs/samples/sim-parity.json`
  (hand levels, one consumable, one joker order, one scoring case and so on); known gaps are in
  [docs/sim-backend.md](docs/sim-backend.md).
- *Live tests* in the real game for mechanics a replay can't settle, for example
  `scripts/diagnostics/marble_live_test.py`.
- The simulator matches today's real game only with **every item unlocked** (the profile we play
  on). Older recordings were made with 58 items locked.

**Jackdaw vs Rust.** Every speed-only change must give identical results seed by seed
(`JEV_NATIVE=0` vs native, lab arms on 28 seeds). The differences this found so far were all
real mechanics bugs. Several were in Jackdaw and are now fixed in `sim_patches`:
- Smeared Joker in suit checks.
- Disabled jokers keeping their passive effects (Stuntman's hand size).
- SMODS' immediate draw.

One was in rumbot-sim: its Crimson Heart followed vanilla instead of SMODS. Fixes to our rumbot
fork are listed in [native/rumbot/VENDORED.md](native/rumbot/VENDORED.md).

**What this means for results.** Sim results are a strong guide but not proof: a change that wins
in the sim should still be confirmed on the real game. Sim and real benchmark numbers are
recorded separately and never mixed.

## Recorded data

```
runs/
  <started>-<seed>/
    summary.json       # result, ante/round, decisions, tokens, confidence, prompt_version
    decisions.jsonl    # every JEV call: state sent, instructions, options, choice, probabilities,
                       # confidence, latency, tokens; plus actions, errors and run end
  _prompts/
    index.json         # version order (v1, v2, …)
    <version>.json     # snapshot of the prompt text for that version
```

## Layout

```
src/                    the bot and control panel (run: python src/panel.py)
  balatro.py            JSON-RPC client for BalatroBot (127.0.0.1:12346)
  decisions.py          v6+ handlers: batched typed questions per decision, thresholds in code
  jokers.py             joker-aware hand scoring and measured shop value, from live joker text (v8+)
  odds.py               unordered-draw forecasts of the complete next hand (v12+), legacy draw odds
  build.py              the run's committed build (target hand + flush suit) (v10+)
  tarots.py             tarot/spectral mechanics: targets for the build, auto-use (v10+)
  readiness.py          can the deck beat the next blinds? drives shop invest/bank mode (v13+)
  rollout.py            shop forecast: simulate each joker/planet/voucher buy through the next bosses (v19+)
  rollout_items.py      the same for packs, shop tarots and spectrals (v21+)
  native.py             Rust hand-scoring kernel bridge (v24+; Python fallback)
  sim_bridge*.py        Jackdaw state -> rumbot-sim (Rust) rollouts (v24+; Python fallback)
  sim_backend.py        Jackdaw wrapped as a BalatroBot-shaped game (JEV_BACKEND=sim)
  sim_patches.py        fixes to Jackdaw so it matches the game (see "How faithful...")
  sim_text.py           card effect text for sim states, from runs/_loc
  hands.py              hand evaluation, score estimates, play and discard candidates
  prompts.py            everything JEV reads (rules, questions, option templates)
  strategy.py           code-level policy: auto planets, joker ordering, joker tags, interest
  jev_bot.py            the game loop: one handler per game state, asks JEV, executes
  recorder.py           per-run recording and prompt-version snapshots/ids
  analytics.py          per-run, per-version and benchmark metrics
  benchmark.py          the fixed seed sets (NAME = the panel's benchmark)
  panel.py, panel.html  control panel server and single-page UI
  legacy/               byte-exact rebuild of v1, runnable with `python src/panel.py --legacy v1`
native/                 Rust: scoring kernel (jev_native), rollouts (sim -> jev_sim), rumbot-sim fork
third_party/jackdaw/    Jackdaw, the Python Balatro engine (git submodule)
mods/jev_tweaks/        companion Balatro mod: skips blocking "unlocked" popups
scripts/                launch_fast.cmd, sim_run.py (sim games), policy_lab.py (no-JEV lab),
                        audit_*.py (loss/win audits), diagnostics/ (parity and live tests)
docs/                   experiment write-ups, fidelity audits, sample states
runs/                   recordings (git-ignored except runs/_prompts version snapshots)
```

## Known issues

- **Something blocks the game** (a popup, a menu): Balatro stops answering, and after 60 s the
  bot pauses with a hint in the feed. Clear it in the game and press Run. Unlock popups are
  handled by `jev_tweaks`.
- **The bot's own scoring is approximate.** Its hand scorer reads joker text and models most
  effects, but not Smeared Joker, Four Fingers or Shortcut when it builds hands. The simulators do
  model them, so forecasts see their value even where the bot won't exploit it.
- **Consumables** are auto-used with build-aware targets. Some unsupported pack interactions
  still use fallback targets after an API error.
- The JEV API returns **HTTP 402** when the TypeSafe org has no usable API credits. The free
  monthly credit alone did not work for us; a paid top-up did.
