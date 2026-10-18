# JEV × Balatro

A stress test for **JEV**, TypeSafe's typed "System One" model: a bot that plays
[Balatro](https://store.steampowered.com/app/2379780/Balatro/) by reading the live game state,
asking JEV to pick the next move, executing it, and repeating. Every decision is recorded so
prompt changes can be compared run by run.

```
Balatro (+ BalatroBot mod)  ──JSON-RPC──▶  jevbot (Rust)  ──typed Choice──▶  JEV API
        ▲                                     │
        └────────────── action ◀──────────────┘          jevbot panel: control panel + run analytics
```

**How decisions work.** Code enumerates the sensible legal moves (best play per hand type with a
score estimate, a few discard strategies, shop buys, pack picks) and JEV answers one `Choice`
question over them, returning a probability for every option. Code owns the rules and execution;
JEV supplies the judgment. In the shop, code forecasts each option by simulating the next few
blinds (rollouts) and buys outright when one is clearly better; JEV rates the close calls.

**Where games run.** The same bot plays either the real game (through BalatroBot) or
[rumbot-sim](native/rumbot), a Balatro engine in Rust, behind the same interface. The forecasts'
rollouts run on rumbot too. Big experiments run in the simulator; see
[How faithful is the simulator?](#how-faithful-is-the-simulator) for how closely it matches the game.

The bot was written in Python first and ported to Rust line by line (it follows Python v31/v32,
identical seed by seed). The Python code was removed afterwards; it is in the git history up to
`4628654`.

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
4. **Rust**: install with rustup (stable, MSVC), then build (Git Bash, `export PATH="$HOME/.cargo/bin:$PATH"`
   first if cargo is not on PATH):
   ```
   cd native && cargo build --release
   ```
   Binaries land in `native/target/release` (`jevbot`, `jevlab`).
5. **`.env`**: `JEV_API_KEY=...` (a TypeSafe API key; the org needs credits on the API).
6. **Localization for the simulator**: `native/target/release/jevbot extract-loc` copies the game's
   card text from `Balatro.exe` (and Steamodded's overlay) into `runs/_loc`. The text is the game's,
   so it is never committed. Options: `--exe`, `--smods`, `--out` (or set `$BALATRO_LOC_DIR`).

## Running

1. Close Balatro, then start it with **[`scripts/launch_fast.cmd`](scripts/launch_fast.cmd)**: BalatroBot fast
   mode (10× game speed) with audio on. Launching from Steam also works, but at normal speed
   and muted.
2. Start the control panel and open http://127.0.0.1:8765:
   ```
   native\target\release\jevbot panel
   ```
3. Press **Run** (or Space). It starts paused.

Without the panel: `jevbot play [--seed S] [--runs N]` plays the game from the command line.

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

`jevbot panel --backend rumbot` runs the panel on the simulator (runs under `runs/sim`), and
`--code-jev` replaces JEV with CodeJev (the top-ranked option; no key, no tokens). The page is
[`native/bot/panel.html`](native/bot/panel.html), re-read on every load; the binary carries a copy
for when the source tree is not there.

## Tuning prompts

All text JEV reads lives in **[`native/bot/src/prompts.rs`](native/bot/src/prompts.rs)**: game rules, the
question per decision type, and the option templates. Always-right moves and the context JEV gets
live in **[`native/bot/src/strategy.rs`](native/bot/src/strategy.rs)**: auto-using planets, keeping xMult
jokers rightmost, and tagging jokers by role and by synergy with the run's main hands.

The prompt version id is the sha1 of the Rust sources that shape a decision
(`recorder::bot_version`: `src/**/*.rs` of `native/{bot,simcore,kernel,rumbot,pycompat}`), so **any
edit there creates a new version** with its own stats. Update `VERSION_NOTE` in `prompts.rs` to say
what changed and restart the panel.

### Benchmark

To compare versions cleanly, open the **Benchmark** tab and press **Benchmark current version**.
The bot plays a fixed seed set ([`native/bot/src/benchmark.rs`](native/bot/src/benchmark.rs), `NAME`;
30 seeds) and stops when done, or early after 10 seeds if the version clearly trails the best one.
The tab shows a summary per version and a seed × version grid of the ante reached. Seeded runs
don't trigger unlocks. Thirty games only detect differences of about 0.9 ante, so small changes are
tested in the simulator first (next section).

## Simulator and policy lab

- **Sim games with JEV**: `jevbot sim --set sim30-a --jobs 8` plays a seed set on rumbot with the
  production bot and real JEV calls, one game per thread (`--seeds A,B` for chosen seeds,
  `--code-jev` for no JEV). Runs are recorded under `runs/sim/` (`runs/sim_smoke/` with
  `--code-jev`) and never share benchmark cells with real-game runs. One JSON line per game, then a
  summary line.
- **Policy lab (no JEV)**: `jevlab --set sim30-a --arm E|F --jobs 8 [--limit N] [--name X]` plays the
  bot with CodeJev standing in for JEV, in parallel and at no token cost: arm E is
  `bot-forecast-long`, arm F `bot-forecast-items`. Rows go to
  `runs/_policy_lab/<name>/results.jsonl`. Arms compare policies on the same seeds (paired); see
  [docs/policy-lab.md](docs/policy-lab.md).
- **Profiling**: `JEV_PERF=1` prints coarse timers per phase. For a real profile, build with
  `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only` and run `jevlab` under `perf record -g`.
- **Current baseline (v31, 2026-10-17)**, code only (no JEV), arm F (measured with the Python lab):
  - `sim60-a`: mean ante **6.50**, 10 wins out of 60 (v27: 5.90, 4 wins); Rust gives the same;
  - 60 fresh seeds: **6.63**, 10 wins (v27: 6.15, 6 wins);
  - 200 fresh seeds (after those 60): **6.68**, 40 wins (20%; v28: 6.46, 27 wins);
  - v28 planets, v29 Negative jokers, v30 Verdant Leaf, v31 auto-buy bar 1 SE: conclusions in
    [docs/lab-lessons.md](docs/lab-lessons.md), numbers in [docs/policy-lab.md](docs/policy-lab.md)
    ("Mid-game scaling experiments" and after), every game in `docs/samples/cloud-lab/`;
  - about a third of runs reach ante 8, where the finisher bosses (Violet Vessel, Crimson Heart,
    Amber Acorn) take most of them.

  The last JEV sim benchmark (v23, `sim30-a`, before later sim fixes) reached 6.43. Thirty seeds
  detect about 0.9 ante, so test changes code-only on 60+ seeds first. The experiment arms of the
  Python lab (planets, vouchers, engines, ...; none shipped) were not ported to `jevlab`.

## Rust bot

`native/` is one Cargo workspace:

| Crate | What |
|---|---|
| `bot` (`jevbot`) | JevBot, the decisions, the forecasts, the recorder, the TypeSafe client, the panel; binaries `jevbot` (play, panel, sim, extract-loc) and `jevlab` |
| `kernel` (`jev_kernel`) | the joker-aware hand scorer: classify and score every play subset |
| `simcore` | the forecast core: state description, rollout policy, `play_horizon` on rumbot |
| `rumbot` (`rumbot_sim`) | the game engine, our fork of rumbot-sim (changes in [VENDORED.md](native/rumbot/VENDORED.md)) |
| `pycompat` | CPython-exact float formatting, rounding, `random.Random`, `json.dumps` (keeps prompts and records identical to the Python bot's) |
| `typesafe` | the TypeSafe API client |

```
jevbot play [--backend balatrobot|rumbot] [--seed S] [--deck RED] [--stake WHITE] [--model jev-latest] [--runs N] [--code-jev]
jevbot panel [--port 8765] [--backend balatrobot|rumbot] [--code-jev]
jevbot sim (--set NAME | --seeds A,B) [--jobs N] [--code-jev]
jevbot extract-loc [--exe Balatro.exe] [--smods DIR] [--out DIR]
jevlab --set sim30-a --arm E|F --jobs 8 [--limit N] [--name X]
```

- `play --backend balatrobot` plays the modded game at 127.0.0.1:12346; `rumbot` runs the headless
  engine and logs under `runs/sim`. The JEV key comes from `.env`.
- `JEVLAB_TRACE=<dir>` writes each lab game's calls and prints the first call that differs from the
  recorded Python trace in `runs/_port/traces`.
- Porting history and conventions: [docs/rust-port-guide.md](docs/rust-port-guide.md),
  [docs/rust-port-scope.md](docs/rust-port-scope.md); open items:
  [docs/port-handoff/followups.md](docs/port-handoff/followups.md).

**Identity.** Tests: `cd native && cargo test --release`. Most compare against fixtures and traces
recorded from the Python bot in `runs/_port` (not in git; `$JEV_PORT_DATA` overrides the path). They
stay valid without the Python code, but can no longer be regenerated, so a behaviour change makes
them stale.

At Python v31 (references regenerated from v32's Python, see [docs/rust-port-guide.md](docs/rust-port-guide.md)
"Reference data"):

- Bot replay: 60/60 games identical; CodeJev 2535/2535 requests; every decision fixture identical (jokers,
  picker, readiness, scorer, hands, odds, dec_shop_a, fc_stats). Forecasts: items 2913/2916, packs
  2223/2224 (`fc_items` is slow, ~2-9 min); all four misses are one Jackdaw bug (an Arcana pack's tarot
  duplicating one we hold; [followups](docs/port-handoff/followups.md)).
- sim60-a, arm F: Rust 6.50 +- 0.20 ante, 10/60 wins, the same as Python v31; 60/60 games end the same.

At Python v27 (before the v28-v31 port):

- Bot replay: 60/60 games identical. Forecasts: shop 2475/2475, packs pass, items 3193/3193.
  All-Rust forecast replay: 60/60.
- Engine replay (`engine_replay`, `IGNORE="].id"`): 47/60 games identical. The rest are Jackdaw bugs
  and the Mail-In Rebate rank (followups.md).
- Live lab, 8 traced seeds: 15/16 calls identical. The one difference (1YJ4DQ2R, arm E) is a Jackdaw
  Standard-pack bug, not the Rust bot.
- `kernel/tests/matches_reference.rs` checks the allocation-free scorer against its original
  implementation, bit for bit, on random hands and joker rules.

**Speed** (8 seeds, summed per-game seconds, arm E / arm F):

| Path | Arm E | Arm F |
|---|---:|---:|
| Pure Python (`JEV_NATIVE=0`) | 887 s | 1030 s |
| Python + native kernels (v27) | 87 s | 115 s |
| Rust (port) | 49 s | 58 s |

Profiled speed-ups since the port (16 seeds, 4 threads, no tracing, same machine, identical games):

- v33 (on v27's bot): arm E 19.4 s → 11.3 s, arm F 23.2 s → 13.5 s wall. The allocator had taken about
  a third of the time (now mimalloc), and the scorer kernel no longer allocates per subset.
- v34 (on v31's bot, whose planet values made pack picks the hot spot): CPU time arm E 71.7 s → 31.4 s,
  arm F 90.6 s → 37.6 s. Planet values share one scoring of their deals, best plays skip plays with a
  kicker when a kicker cannot matter, rumbot stopped formatting logs nobody reads.
- Where the time goes now: the rollouts (rumbot's `GameState::step`, ~25%), subset scoring for the odds
  forecast, play mix and planet values (~25%), and the BalatroBot-shaped JSON views (~15%).

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
3. **The simulator**: rumbot-sim (`native/rumbot`), which plays whole sim games and the forecasts'
   rollouts. Until the Python removal, Jackdaw (a Python engine) played the sim games.

**SMODS vs vanilla.** SMODS rewrites some game functions so mods can coexist, and a few
rewrites change behaviour in edge cases. Ones we hit:
- xMult arithmetic (rounding).
- Crimson Heart: it only releases the joker it disabled, where vanilla re-enables every joker.
- Hand slots that grow mid-round are filled from the deck at once.
- Hand-type iteration order, which matters for To Do List.

In normal play these rarely matter, and we model the SMODS behaviour because that is what the bot
faces. BalatroBot also differs from the game: it accepts a few consumable uses the game would
refuse, for example Judgement with full joker slots.

**How it was checked.**
- *Jackdaw vs the game* (Python era): 336 recorded real games replayed action by action through
  Jackdaw with its fixes; 329 replayed to the end with every recorded state identical, and 25,304 of
  25,454 states matched exactly. The 7 that diverge are listed in `docs/samples/sim-parity.json`;
  known gaps are in [docs/sim-backend.md](docs/sim-backend.md). This check needed Jackdaw and went
  with the Python code: rumbot has no real-replay parity check of its own yet.
- *rumbot vs Jackdaw*: the engine replay above (47/60 games identical; the rest are Jackdaw bugs).
- *Live tests* in the real game for mechanics a replay can't settle.
- The simulator matches today's real game only with **every item unlocked** (the profile we play
  on). Older recordings were made with 58 items locked.

Mechanics bugs found by comparing engines were real bugs in one of them: Smeared Joker in suit
checks, disabled jokers keeping their passive effects (Stuntman's hand size) and SMODS' immediate
draw were Jackdaw's; Crimson Heart following vanilla instead of SMODS was rumbot's. Fixes to our
rumbot fork are listed in [native/rumbot/VENDORED.md](native/rumbot/VENDORED.md), each citing the Lua.

**What this means for results.** Sim results are a strong guide but not proof: a change that wins
in the sim should still be confirmed on the real game. Sim and real benchmark numbers are
recorded separately and never mixed.

## Recorded data

```
runs/
  <started>-<seed>/
    summary.json       # result, ante/round, decisions, tokens, confidence, prompt_version, backend
    decisions.jsonl    # every JEV call: state sent, instructions, options, choice, probabilities,
                       # confidence, latency, tokens; plus actions, errors and run end
    states.jsonl.gz    # the full game state at every decision
  sim/, sim_smoke/     # simulator runs (JEV / CodeJev), same layout
  _policy_lab/         # jevlab results
  _loc/                # the game's localization (jevbot extract-loc; not in git)
  _prompts/
    index.json         # version order
    <version>.json     # snapshot of the prompt text for that version
```

## Layout

```
native/                 the bot, as one Cargo workspace (see "Rust bot")
  bot/                  jevbot: src/ (bot, decisions, forecasts, recorder, panel), panel.html, tests/
  kernel/ simcore/ rumbot/ pycompat/ typesafe/
mods/jev_tweaks/        companion Balatro mod: skips blocking "unlocked" popups
scripts/launch_fast.cmd starts Balatro in BalatroBot fast mode
docs/                   experiment write-ups, fidelity audits, sample states (many from the Python era)
runs/                   recordings (git-ignored except runs/_prompts version snapshots)
```

## Known issues

- **Something blocks the game** (a popup, a menu): Balatro stops answering, and after 60 s the
  bot pauses with a hint in the feed. Clear it in the game and press Run. Unlock popups are
  handled by `jev_tweaks`.
- **The bot's own scoring is approximate.** Its hand scorer reads joker text and models most
  effects, but not Smeared Joker, Four Fingers or Shortcut when it builds hands. The simulator does
  model them, so forecasts see their value even where the bot won't exploit it.
- **Consumables** are auto-used with build-aware targets. Some unsupported pack interactions
  still use fallback targets after an API error.
- The JEV API returns **HTTP 402** when the TypeSafe org has no usable API credits. The free
  monthly credit alone did not work for us; a paid top-up did.
