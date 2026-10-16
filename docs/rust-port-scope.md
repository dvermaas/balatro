# Rust port: scope and plan

2026-10-16. Scoping only: no code is ported yet. The question was whether to port Jackdaw
(`third_party/jackdaw`) to Rust to speed up the forecasts, whether a usable Rust Balatro engine
already exists, and which parts give the biggest speedup for the least work.

## Summary

- **Most of the time is not in Jackdaw.** In the v22 profile, about half of a lab game is the bot's
  own Python text scorer (`jokers.score` over every play subset), called from `odds.forecast` and
  `readiness.play_mix`. Porting only the engine would give about 1.3x more games per hour.
- **First target: the scorer kernel, not the engine.** That means `hands.classify`,
  `hands.evaluated_plays`/`best_score` and `jokers.score` behind a small PyO3 module. It is about
  2k lines of Rust and an estimated **~2.4x** more games per hour. It can give results identical
  to v22.
- **Second target: the rollout engine.** This is the Jackdaw subset that `rollout.play_horizon`
  touches, plus `Policy`, which reuses the kernel. It is 12–16k lines of Rust, for an estimated
  **~5–6x** total. Before writing it, spend 1–2 sessions testing an existing Rust engine, which
  might replace most of this work.
- **Not worth it: a full port of Jackdaw.** The live sim game costs about 1.5 s of engine time per
  game, so the shop, packs, tags, challenges and env layers only add work.
- **Ruled out: PyPy.** Jackdaw requires Python ≥ 3.12, and PyPy stops at 3.11. Also, a published
  measurement on a comparable simulator found PyPy only 1.3x faster than CPython on rollouts.
- **Existing Rust engines:** two candidates claim full content and RNG that matches the game bit
  for bit (see below). Neither is a port of Jackdaw, both are a few weeks old, and both have one
  author. They are worth a spike, but not worth adopting blind.

## Where the time goes

### From the v22 profile

These numbers are from one profiled arm-E game ([rollout-forecast.md](rollout-forecast.md), v22).
The lab average is 7,333 summed game seconds over 56 games, about 131 s per game.

| part | s/game | what it is |
|---|---:|---|
| `odds.forecast` → `best_score` | ~40 | `jokers.score` on every ≤5-card subset of each sampled future hand |
| `readiness.play_mix` | ~28 | the same kernel: 24 deals × every subset, recomputed per round |
| rollouts (`rollout.Policy` + engine) | ~45 | part of the 59 s of shop forecasts |
| everything else | ~20 | `decisions.py`, `build_state`, auto-use planning, recorder, ... |

### Measured here

`scripts/diagnostics/profile_rollout_engine.py` (new) runs the real rollout loop: pickled
snapshot, reseeded, 2 bosses ahead, shop population off. It uses a trivial policy, so the time is
Jackdaw plus the loop. It ran on CPython 3.13 on a 2.8 GHz Xeon in the cloud container.

| build | per rollout | of which unpickle |
|---|---:|---:|
| 5 jokers (Joker, Ride the Bus, Photograph, Baron, Blueprint) | 9.5 ms | 0.5 ms |
| 8 jokers (+ Hanging Chad, Greedy, Cavendish) | 15–17 ms | 0.4 ms |

The docs report about 12 ms per rollout with the real `Policy`, so most of a rollout is the engine.
The cProfile is spread out:
- `scoring.score_hand` holds 60% cumulative, with dict `.get` calls in its hot loop;
- `calculate_joker`: ~90k calls per 100 rollouts;
- hand evaluation (`get_x_same`, `get_flush`, `is_suit`);
- the TW223 RNG.

There is no single hot spot to rewrite. This is the typical shape for a straight port: a big
constant-factor gain on everything.

### Expected Rust speedup

rumbot-sim publishes CPython-vs-Rust measurements of a comparable pure-Python Balatro simulator,
same work, i7-8700:
- **24x** on whole-run rollouts;
- **17x** on score previews;
- 14.5x on `legal_actions`.

The estimates below assume about 20x.

| scenario | odds + readiness | rollouts | other | ≈ s/game | games/hour |
|---|---:|---:|---:|---:|---:|
| today (v22) | 68 | 45 | 20 | ~131 | 1x |
| A: scorer kernel | ~4 | ~32 (engine still Python) | 20 | ~55 | **~2.4x** |
| A + B: rollout engine too | ~4 | ~2 | 20 | ~25 | **~5–6x** |
| full port of Jackdaw | ~4 | ~2 | ~19 | ~24 | no further gain |

What is left after A + B is ordinary bot Python (Amdahl). JEV games are bound by API latency and
gain little. The speedup matters for the lab arms. It can also buy precision instead of throughput:
the same forecast time budget could run several times more rollouts per option.

The weakest assumption is the split inside the 45 s of rollouts: how much is `Policy` (text
scorer) and how much is engine. Step 0 settles it.

## Existing Rust (and other native) engines

Checked from public READMEs, PORTING notes and a read of the cloned source. Nothing was built or
run here.

| project | license | size | Python bindings | fidelity claim | fit |
|---|---|---|---|---|---|
| [mmank/rumbot-sim](https://github.com/mmank/rumbot-sim) | MIT | 38.6k lines of Rust, no dependencies, last commit 2026-10-05 | none | Module-for-module port of the Python **jimbot-sim**: 150/150 jokers, 52/52 consumables, shop, packs, tags. RNG matches bit for bit on a 2,270-draw fixture. 622 tests | best engine to start from; strong RNG reference |
| [PrimalMentat15/Gambit](https://github.com/PrimalMentat15/Gambit) `sim/` | MIT | 24.3k lines of Rust, last commit 2026-10-12 | PyO3, vectorised env | "Seed-faithful": 150 jokers, consumables, vouchers, tags, bosses, 15 decks × 8 stakes. Bit-exact RNG with LuaJIT oracle vectors. A cross-validation harness against the real game. Shop and packs are not mentioned | worth a spike; claims unverified |
| [evanofslack/balatro-rs](https://github.com/evanofslack/balatro-rs) | none stated | 397 commits | PyO3 (`pylatro`) | 74/150 jokers. No tags, decks or stakes | RNG reference only; no license |
| azazo1/balatro | not stated | 228 commits | yes | `tests/luajit_parity.rs`, no counts given | maybe look later |
| Immolate (C/OpenCL), TheSoul (JS), Motely (C#), Triboulet (Rust) | various | — | — | Seed searchers: they reproduce the RNG only, with no game loop | RNG reference |

Others are too incomplete to consider: hack-balatro (its RNG does not match the game),
bmp-rs (5/150 jokers, GPL), simulatro (C, makes no RNG claim), and several toy simulators.

Neither strong candidate is a drop-in replacement:
1. **Different engine.** Their parity target is jimbot-sim or the real game, not Jackdaw plus our
   `sim_patches`. Rollouts would no longer match the live sim exactly. That adds engine-mismatch
   error on top of today's reconstruction error (97.4% survival match, rollout-forecast.md).
   Results would also differ from v22, so a new benchmark is needed.
2. **Different state model.** `rollout.build_state` builds a Jackdaw state. Using another engine
   means converting every field, including the internals of all ~150 joker kinds. That converter
   is 1–2k lines and is where bugs would hide.
3. **Single authors, weeks old.** Expect to own the code.

They still help:
- If both engines really match the game's RNG, the same state and the same seed string should
  give the same rollout. That makes the divergence from Jackdaw directly measurable, rollout by
  rollout. That is the spike.
- rumbot-sim's `rng.rs` and its bit-level fixture method are the template for our own RNG port in
  any case.

## Plan

### Step 0: profile the real split (on the PC, under 1 h)

The container has no `runs/_loc` (it needs `Balatro.exe`), so the real `Policy`, `odds` and
`readiness` could not run here. On the PC, profile arm E and arm F of `scripts/policy_lab.py` on 3
seeds with cProfile. Record seconds per game for:
- `jokers.score`;
- `hands.classify`;
- the Jackdaw `step` inside rollouts;
- `Snapshot.copy`;
- the rest.

Run `profile_rollout_engine.py` there too, to compare with the numbers above. If the kernel is
much smaller than the ~70–80 s estimated above, revisit the plan.

### Stage A: scorer kernel in Rust (2–4 sessions, biggest gain per unit of work)

- **Make the rules data.** `jokers.parse` returns lambdas today. Make it return declarative rule
  specs: about 20 hand-rule kinds, 3 card-predicate kinds (suit-or-wild, rank set, all), held
  rules and retriggers. Keep a Python evaluator of the specs as the reference implementation and
  the fallback.
- **New crate `native/`** (PyO3, abi3 wheel, built with maturin), about 1.5–2.5k lines:
  - `classify`;
  - the subset loop of `evaluated_plays` (including the `allowed` filter);
  - `best_score`;
  - `score`.

  Floats are f64 in both languages. With the same order of operations and the same final
  `int(max(0, chips) * max(1, mult))`, the results are bit-identical.
- **Callers** pass a compact card array plus the specs once per call: `hands.best_score` and
  `play_candidates`, `readiness.power` and `play_mix`, `odds.forecast`, and `rollout.Policy`. The
  live bot on the real game uses the same path.
- **Identity checks:**
  - every subset score equal, Python vs Rust, on all hand states in recorded runs;
  - a randomised test over hands × joker sets;
  - lab arms E and F on the 28 main seeds giving identical per-seed results, as was done for v22.
- **Versioning:**
  - it is a new prompt version (hashed files change), with `VERSION_NOTE` "speed only";
  - add the Rust sources to the `PROMPT_VERSION` hash, or a Rust edit would change behaviour
    without changing the version.
- **Build:**
  - it is the first compiled dependency, which breaks "no other deps", so keep it optional: fall
    back to Python when the module is missing, and record in `summary` which backend ran;
  - on Windows it needs the Rust MSVC toolchain and VS Build Tools;
  - Python 3.14 needs a PyO3 release that supports 3.14 (check at build time).

### Spike: test an existing engine as the rollout engine (1–2 sessions)

1. Build rumbot-sim and Gambit and run their own benchmarks. Read their code before building:
   build scripts run code.
2. Write a minimal converter from a Jackdaw state for 5–10 `validate_rollout.py` states that use
   only common jokers.
3. Run the same seeds (`RO0..`) in both engines and count rollouts whose per-blind rows differ.

**Gate:**
- **≥ ~99% identical and the converter looks manageable:** adopt it for Stage B. That saves most
  of the porting; the cost is a new version and a benchmark.
- **Otherwise:** port Jackdaw ourselves, using their RNG code and fixtures as the reference.

### Stage B: rollout engine in Rust (own port: 15–30 sessions)

**Scope.** Only what `play_horizon` reaches from a shop state with the build frozen:
- run state, cards, card areas, deck draw and shuffle;
- hand evaluation and scoring;
- every joker's scoring and round hooks (any build can hold any joker);
- all bosses;
- select blind, end of round, cash-out and interest;
- hand levels and planet use;
- card creation by jokers and seals (Riff-raff, Cartomancer, DNA, Certificate, Marble, 8 Ball,
  Blue Seal, ...) with the pools behind it;
- passive voucher effects;
- the `sim_patches` fixes on those paths.

**Out of scope:**
- shop population (already off in rollouts);
- packs (rollouts skip them);
- tarot and spectral use;
- challenges;
- env, bridge, CLI and observation code.

**Size.** About 10–11k of Jackdaw's 17k engine lines, so 12–16k lines of Rust:

| module | lines |
|---|---:|
| `rng.py` | 522 |
| card, card area, state | ~1.5k |
| `hand_eval` | 538 |
| `scoring` | 946 |
| `jokers` | 2,663 |
| `blind` | 601 |
| round lifecycle and economy | ~570 |
| the play and round paths of `game.py` | ~1.3k |
| `card_factory` and `pools` | ~0.8k |
| planets and vouchers | ~0.3k |
| data loading | ~0.6k |

**Simplification.** Apply the option (buy, sell, redeem, or `rollout_items` pack and tarot picks)
in Python Jackdaw as today, then convert the resulting state to Rust once per option. The Rust
state is cloned per rollout. So the shop actions never need porting.

**Python boundary.** One call per option: `forecast_runs(state, policy_spec, seeds, bosses)`
returns the per-blind rows that `summarize` already consumes. `Policy` runs in Rust on the Stage A
kernel. Calling back into Python for each decision would cancel most of the gain.

**Incremental fallback.**
- Rust takes a state only if every joker, boss, seal and voucher in it is implemented; otherwise
  the forecast runs in Python.
- Implement jokers in order of how often they appear in recorded forecast states. A large share
  of forecasts then go native long before all 150 jokers are done.

**Identity checks.**
- RNG: a bit-level fixture from Jackdaw's oracle files (`rng_oracle_TESTSEED.json`, ...). Also
  check `truncate_13`'s `%.13f` rounding.
- A debug mode that logs each pseudorandom draw (stream key and value) in both engines. A single
  extra draw in one joker hook shifts everything after it, and this log finds the first divergence
  immediately.
- Forecast output identical on all `validate_rollout.py` states, and identical lab per-seed
  results, as in Stage A.

**Main risk:** the long tail of joker × boss × seal interactions, and the order of RNG calls inside
them. Jackdaw's own test suite (about 20k lines, with Lua oracles for scoring, hand evaluation, RNG
and the shop) can be mined for fixtures.

### Not planned: full port of Jackdaw

The live sim game, with its shop, packs, tags, consumables and the BalatroBot serialisation in
`sim_backend.py` and `sim_text.py`, spends about 1.5 s of engine time per game. Porting it would
roughly double Stage B's work for no measurable gain, and would put our 336-game parity result
(`docs/samples/sim-parity.json`) at risk.

## What was not verified

- **The per-game split:** taken from the v22 doc's single profiled game. It could not be
  re-measured here without the localization files.
- **The third-party engines:** their fidelity and speed claims are the authors'. Nothing was built
  or run, and their source was read only for layout and notes.
- **The ~20x factor:** comes from rumbot-sim's measurements on a different Python simulator. It is
  not a measurement of Jackdaw.
