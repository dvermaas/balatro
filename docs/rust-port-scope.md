# Rust port: scope, spike and plan

2026-10-16. The goal is the whole lab game in Rust (engine and policy), so benchmarks scale with
cores instead of with Python. Today 600 seeds take about 80 minutes on a 7800X3D. This doc covers
three things: where the time goes, a lockstep spike against two existing Rust engines, and the
staged plan.

## Summary

- **Feasible.** About 20–28 sessions if we build on rumbot-sim, against 35–50 if we port Jackdaw
  ourselves.
- **Spike result.** We played 300 seeds with identical actions in Jackdaw (with our `sim_patches`),
  rumbot-sim and Gambit.
  - **rumbot-sim matched Jackdaw on 281 of 300 seeds, all the way to game over.**
  - Each of its 19 divergences traces to a specific mechanic (one is still unexplained), and in 18
    of them Gambit, an independent engine, does the same thing as rumbot-sim. That makes Jackdaw the likely outlier.
  - rumbot-sim runs **25x** faster than Jackdaw on the same scripted games.
- **Recommendation: fork rumbot-sim as the engine** instead of porting Jackdaw.
  - Then add Steamodded (SMODS) xMult rounding and decide the four open mechanics against the real
    game.
  - Accept it only once it replays our 336 recorded real games at least as well as Jackdaw
    (329/336).
  - The policy, the public-state view and the effect text are ours to port either way.
  - Gambit is not recommended: it diverged on 45 seeds, 10 of them with unexplained large score
    differences.
- **Order:**
  - A: scorer kernel (quick win, results identical);
  - B: rumbot-sim fork as engine, validated, with PyO3 bindings;
  - C: the whole policy in Rust, used by both the lab and the real-game bot (one implementation).

## Where the time goes today

v22 profile, one arm-E game ([rollout-forecast.md](rollout-forecast.md)). A lab game averages about
120–131 s, which matches 600 games in 80 minutes on 15–16 workers.

| part | share | moves to Rust in |
|---|---:|---|
| bot text scorer: `jokers.score` over every play subset, from `odds.forecast` and `readiness.play_mix` | ~52% | A |
| rollouts: the Jackdaw engine plus `rollout.Policy` | ~34% | B (engine), A/C (policy) |
| rest of the bot: `decisions.py`, `build_state`, auto-use planning, recorder | ~15% | C |

Measured here with `scripts/diagnostics/profile_rollout_engine.py` (CPython 3.13, 2.8 GHz Xeon):
- an engine-only rollout takes 9.5 ms with 5 jokers and 15–17 ms with 8;
- unpickling the snapshot takes 0.4–0.5 ms;
- the profile is spread out (scoring loop, `calculate_joker`, hand evaluation, TW223 RNG), so there
  is no single hot spot.

| what is in Rust | ≈ s/game | 600 seeds on the 7800X3D |
|---|---:|---:|
| nothing (v22) | ~120 | 80 min |
| A: scorer kernel | ~55 | ~35 min |
| A + B: engine and rollouts too | ~25 | ~15 min |
| A + B + C: whole lab game | ~5 | **~4 min** |

These use about 20x for ported code. That figure is the rumbot-sim measurement below; the scorer
kernel alone may do better.

## Spike: lockstep against existing Rust engines

### Candidates

| project | pinned at | size | notes |
|---|---|---|---|
| [mmank/rumbot-sim](https://github.com/mmank/rumbot-sim) | `888c516` (2026-10-05) | 38.6k lines of Rust | MIT, no dependencies. Module-by-module port of the Python jimbot-sim: 150 jokers, 52 consumables, shop, packs, tags. RNG matches LuaJIT bit for bit (2,270-draw fixture). 622 tests. No Python bindings |
| [PrimalMentat15/Gambit](https://github.com/PrimalMentat15/Gambit) `sim/core` | `8dc1716` (2026-10-13) | 12k lines (core) + PyO3 crate | MIT, claims "seed-faithful" with bit-exact RNG, full content |

Not tried: evanofslack/balatro-rs (no license, 74/150 jokers), hack-balatro (its RNG does not
match the game), and smaller projects. Seed searchers such as Immolate and TheSoul reproduce the
RNG only.

### Method

`scripts/rust_spike/` holds the harness:
- a small stdin/stdout driver per Rust engine;
- `lockstep.py`, which steps all three engines with the same actions;
- `trace.py` and `classify.py` to inspect divergences.

Setup:
- Red Deck, White stake, all-unlocked profile (the default in all three engines);
- 300 random seeds;
- the policy is computed from Jackdaw's state and sent to the others by card identity and slot
  index:
  - skip the small blind on even antes;
  - play a flush, else the biggest rank groups, else the 5 highest cards;
  - discard toward a suit or pairs;
  - buy the first affordable joker, then the voucher, then the first pack;
  - in packs, take a planet, playing card or joker, otherwise skip;
  - cash out.

**Comparisons are phase-aware.** For example, the boss and tags are compared at blind select, and
money after cash-out, because the engines update those between screens at slightly different
moments. Each seed stops at its first difference. Raw results:
[samples/rust-spike-lockstep.json](samples/rust-spike-lockstep.json).

**Coverage:** 17,197 steps, antes 1–6. It covers bosses, shops, joker buys (whatever the shop
offers, so it reaches many jokers), vouchers, Buffoon, Celestial and Standard pack picks, and skip
tags. It does **not** cover:
- using tarots or spectrals;
- selling, rerolls;
- antes 7–8;
- other decks and stakes.

The 336-game real-replay check in stage B covers those.

### Results

| | seeds identical to game over | first divergences |
|---|---:|---:|
| rumbot-sim | **281 / 300** | 19 (18 shared with Gambit at the same step) |
| Gambit | 255 / 300 | 45 |

**Update after `main`'s Jackdaw fixes (`eacdcbe`, `8e2aa48`, issue #2).** The local agent checked
all four open mechanics against the game's Lua, and live in the game for Marble Joker. All four
were Jackdaw bugs. Re-run against the fixed Jackdaw:
- rumbot-sim is identical on **296/300** seeds. The rest is 3 SMODS rounding cases and 1 To Do List
  case (`P23R7MAH`).
- Gambit is identical on 269/300.

The raw results file now holds this re-run. The tables below are the original run.

Causes of rumbot-sim's 19 divergences (`classify.py`, each checked with `trace.py`):

| cause | seeds | who is right |
|---|---:|---|
| **Marble Joker**: the dealt hand differs after blind select | 7 | Unknown. Both Rust engines agree with each other; Jackdaw seems to add the Stone card at a different point relative to the `nr` shuffle. Check against the game |
| **Mr. Bones** saves a lost round, and Jackdaw still pays the blind reward | 5 | Probably Jackdaw is wrong. As I remember `evaluate_round`, a saved round adds a $0 "blind1" row. Confirm in the Lua dump |
| **SMODS xMult rounding** (Ramen, Obelisk), off by 1 point | 3 | Expected. Our `sim_patches` model Steamodded's `mult + mult*(x-1)`, which the real BalatroBot game uses; rumbot-sim is vanilla. Port the patch |
| **Satellite** payout, $1–2 less in Jackdaw | 2 | Unknown. Both Rust engines agree. Probably the unique-planets count after planets used from packs |
| **Madness + Ceremonial Dagger** | 1 | Jackdaw is wrong. It collects every blind-select joker effect before applying any. So when Madness picks the Dagger, the doomed Dagger still destroys its right-hand neighbour. In the game a Dagger that `getting_sliced` does not act |
| To Do List / The Mouth / Bloodstone (`P23R7MAH`): a hand scores 186 vs 310 | 1 | Not explained yet |

Gambit's extra divergences:
- **18 To Do List money differences.** That is the joker whose `pairs(G.GAME.hands)` order our
  `sim_patches` fixed against real-game recordings, and rumbot-sim agrees with Jackdaw there.
- **10 unexplained**, several with large score gaps (Splash, The Order, Walkie Talkie, Hologram
  builds).

### Speed (same scripted policy and seed names, one core)

| engine | steps/s | games/s |
|---|---:|---:|
| Jackdaw + `sim_patches` (CPython 3.13) | 2,408 | 411 |
| rumbot-sim (release) | 60,469 | 10,277 |

That is **25x**. Both engines average about 5.9 steps per game, which is consistent with them
playing the same games. rumbot-sim's own benchmark reaches 2,976 steps/s here on its
preview-heavy rollout, where each step scores about 400 candidate plays.

## Build or adopt

**Adopt (fork) rumbot-sim for the engine.**

What it saves:
- the biggest piece of work: porting about 17k lines of Jackdaw engine to 20–25k lines of Rust,
  roughly 20 sessions;
- the RNG work. RNG matching the game bit for bit is the riskiest part of any port, and it is
  already done and fixture-tested.

What the spike shows: on our action mix it already agrees with Jackdaw plus our patches almost
everywhere. Where it disagrees, an independent second engine sides with it in 18 of 19 cases.

What it costs:
1. **A new engine baseline.** Sim benchmarks on rumbot-sim are not comparable cell by cell with
   v1–v22 on Jackdaw. The panel already separates results by `summary.backend`. Re-run the current
   version once on the new backend to set the baseline.
2. **Ownership.** One author, weeks old. Vendor it as a pinned submodule and fix it ourselves.
   Upstreaming fixes is optional.
3. **The SMODS layer.** Port the relevant `sim_patches` behaviour: xMult rounding, plus each of
   the other patch items that rumbot-sim does not already match. Check them one by one with the
   lockstep harness.
4. **Its design.** It uses shared `Rc<RefCell<_>>` handles and `fn`-pointer hooks. That is fine for
   one game per thread, and game states are built inside each worker. It also has a
   `game_clone.rs` for rollout copies, whose speed is not measured yet.

**When to port Jackdaw ourselves instead:** if rumbot-sim does clearly worse than Jackdaw on the
336-game real replay (B3), and the gap does not come down to a few fixable mechanics.

The lockstep harness then remains the safety net. Jackdaw plus patches stays available as a
reference in Python until rumbot-sim passes the real-replay check.

## Plan

Each stage ends with an identity check: identical per-seed lab results against the previous step,
on the same engine.

### Stage A: scorer kernel in Rust (2–4 sessions, about 2.4x)

**Status (2026-10-16): built as the v23 draft (prompt version `f3bab272`). The lab check on the PC
is still to do.**

What changed:
- `jokers.parse` returns data tuples, and `jokers.Scorer` and `decisions.BossAllow` replace the
  scorer and filter lambdas.
- `native/` (PyO3 0.30, abi3) holds `classify`, the subset loop and `score`. `src/native.py`
  encodes the inputs and falls back to Python on anything unusual.
- `rollout.Policy` scores its candidates in one native call.
- The kernel is in the `PROMPT_VERSION` hash, and runs record `summary.scorer` (`rust` or
  `python`).

Measured in the cloud container:

| check | result |
|---|---|
| identity: v22 lambdas vs v23 Python vs the Rust kernel | identical on 4,000 random cases (281k subset rows) |
| identity: `tests/test_native.py` | 3,000 random cases plus `score_many` and the fallbacks |
| identity: rollouts with the kernel on and off | 240 rollouts on Jackdaw states, identical |
| speed: one `best_score` (8–9 cards, 4–6 jokers) | 7–16 ms → 0.3 ms (20–43x) |
| speed: `readiness.power` on 41 recorded states | 77 → 2.3 ms |
| speed: `readiness.play_mix` | 14.7 → 0.3 ms |
| speed: a rollout | 22 → 20 ms (the Jackdaw engine dominates: stage B) |

**Still to do on the PC (needs `runs/_loc`):**
1. Lab arms E and F on the 28 main seeds, identical per seed to v22, with games/hour.
2. Then run `.venv/Scripts/python -m pip install ./native`, which needs the Rust MSVC toolchain.

Plan as written:

1. **Make the rules data.** `jokers.parse` returns lambdas today. Make it return declarative rule
   specs: about 20 hand-rule kinds, 3 card predicates (suit-or-wild, rank set, all), held rules and
   retriggers. Keep a Python evaluator of the specs as the reference.
2. **PyO3 crate `native/`** for `hands.classify`, the subset loop of `evaluated_plays` and
   `best_score`, and `jokers.score`. Floats are f64 with the same order of operations, so results
   are bit-identical.
3. **Callers:** `hands`, `readiness`, `odds`, `rollout.Policy`.
4. **Checks:**
   - every subset score equal, Python vs Rust, on recorded hand states;
   - lab arms E and F on the 28 main seeds identical per seed, as was done for v22;
   - a new prompt version with `VERSION_NOTE` "speed only", and the Rust sources added to the
     `PROMPT_VERSION` hash.
5. **Build:** maturin, the Rust MSVC toolchain on Windows, and a PyO3 release that supports Python
   3.14. Fall back to Python when the module is missing, and record in `summary` which backend ran.

### Stage B: rumbot-sim fork as the engine (8–12 sessions)

1. **Vendor and fix.**
   - Pin rumbot-sim as `third_party/rumbot-sim` (our fork).
   - Add SMODS xMult arithmetic and the other `sim_patches` behaviours it lacks.
   - Settle Marble Joker, Mr. Bones, Satellite and the `P23R7MAH` case against the Lua dump or
     the real game. Fix whichever engine is wrong, Jackdaw included through `sim_patches`, so the
     current Python sim gets better too.
2. **Public view and text.** Build the gamestate the bot sees (`sim_backend.serialize`'s contract:
   `[]` for empty tables, hidden draw order, `Base Card` labels) and the effect text (`sim_text`)
   from a rumbot-sim state. This is about 1.5–2k lines of Rust and reads `en-us.lua` at runtime.
3. **Real-replay parity.** Port `scripts/diagnostics/sim_parity.py`'s replay to the rumbot backend,
   mapping recorded BalatroBot actions to rumbot-sim actions. Run it on the 336 recorded games.
   Target: at least 329/336 replaying to the end with every state identical, like Jackdaw.
4. **Bindings.**
   - A PyO3 `SimBalatro` backend with the same `call(method, params)` interface, so today's
     Python bot runs on it unchanged. Re-run the current version's benchmark to set the new
     baseline.
   - Rollouts: `build_state` (public state to engine state) and `play_horizon` in Rust, with
     `Policy` on the Stage A kernel.

### Stage C: the whole policy in Rust (8–12 sessions)

1. **Port the policy** into a `policy` crate: `decisions.py`, `hands`, `jokers`, `odds`,
   `readiness`, `build`, `tarots`, `strategy`, `rollout`, `rollout_items` and the lab arm driver.
   That is about 165k characters of dense Python, roughly 6–8k lines of Rust. It reads only the
   public view from B2, so the "never see the draw order" rule still holds by construction.
2. **One implementation.**
   - The real-game bot calls the same crate from Python. Python keeps the JEV calls, the
     BalatroBot connection, the recorder and the panel.
   - The options and descriptions JEV sees must stay byte-identical, so prompt versions stay
     meaningful.
3. **The lab runs pure Rust**, one game per worker thread or process. Check: the Rust lab equals
   the Python bot on the rumbot backend, seed by seed.

### Stage 0 (optional, under 1 h on the PC)

Profile arms E and F on 3 seeds to confirm the time split above. It needs `runs/_loc`, which the
cloud container does not have.

## Follow-ups found by the spike

- Confirm the four open mechanics in `%APPDATA%\Balatro\Mods\lovely\dump`: Marble Joker timing,
  Mr. Bones' blind reward, Satellite, and Madness with Dagger.
- Fix the confirmed Jackdaw ones in `sim_patches`. That changes sim outcomes, so treat it as a new
  sim baseline, not a speed-only change.

## What was not verified

- **The per-game time split:** from the v22 doc's single profiled game, not re-measured here
  (no localization in the container).
- **The rulings above:** "Jackdaw wrong" for Madness/Dagger comes from Jackdaw's own code and the
  game's rule. For Mr. Bones it is my memory of `evaluate_round`. Marble Joker and Satellite are
  open. None was checked against the Lua dump here.
- **Spike coverage:** see the coverage list above.
- **rumbot-sim's clone speed and per-thread behaviour:** not measured.
