# Sim backend (Jackdaw)

JevBot can play full games against [Jackdaw](https://github.com/TylerFlar/jackdaw-balatro), a
Python re-implementation of Balatro, instead of the real game: headless, parallel, Linux-friendly.
The policy code is unchanged (no hashed file is edited), so prompt versions are the same as on the
real game; the only cost of a sim game is its JEV calls.

## Pieces

| File | Role |
|---|---|
| `third_party/jackdaw` | git submodule pinned at `e66de78855df84755d3af7d9a016129f3449f878`, kept pristine |
| `src/sim_backend.py` | `SimBalatro`: same `call(method, params)` interface and errors as `balatro.Balatro`, gamestate serialised exactly like BalatroBot (`[]` for empty tables, hidden deck, `Base Card` labels, stale shop areas, ...). Profile: everything unlocked and discovered |
| `src/sim_text.py` | card/blind/tag effect text, rendered from the game's own localization the way BalatroBot/SMODS do |
| `src/sim_patches.py` | fixes to Jackdaw applied at import (monkeypatches; upstream untouched) |
| `scripts/extract_localization.py` | pulls `en-us.lua` (+ hand examples, SMODS overrides) out of `Balatro.exe`; game assets are never committed |
| `scripts/sim_run.py` | headless CLI, N seeds in parallel processes |
| `scripts/diagnostics/sim_parity.py` | replays recorded real runs through the sim and diffs every state |
| `tests/test_sim_backend.py` | unit + smoke tests (skip without localization/submodule) |

Patches in `sim_patches.py`: Familiar/Grim/Incantation/Cryptid used from a pack put their cards in
the pack hand; BalatroBot pack-pick legality (incl. Judgement/Soul/Wraith overflow, Fool with
nothing to copy); consumable editions actually applied (the audit's undiagnosed Flush+Photograph+
Abstract 41-vs-31 and Flush+Madness on The Fish +50 cases were missing holo/foil); SMODS xMult
arithmetic `mult + mult*(x-1)` (Ramen rounding); `pairs(G.GAME.hands)` order for To Do List and
secret-hand visibility; Orbital tag per-ante choices; The Ox most-played hand; Amber Acorn
shuffles; debuffed Purple Seal inert; Tarot/Planet pool softlock; Crimson Heart selection. Joker
`rearrange` is a full permutation like BalatroBot's `endpoints/rearrange.lua`.

## Setup

Local (Windows, repo `.venv`, Python 3.14): nothing to install; Jackdaw is imported from the
submodule via `sys.path`.

```
git submodule update --init third_party/jackdaw
.venv/Scripts/python scripts/extract_localization.py          # -> runs/_loc (needs Balatro.exe)
```

Linux / cloud (Python 3.12+): Jackdaw needs no third-party packages; `--policy jev` needs
`typesafe-sdk` and `JEV_API_KEY` (environment or `.env`). Copy the three files from `runs/_loc`
(or run the extractor anywhere that has `Balatro.exe` and the SMODS checkout) and point at them:

```
git clone --recurse-submodules <repo> && cd <repo>
python3 -m venv .venv && .venv/bin/pip install typesafe-sdk
export BALATRO_LOC_DIR=/data/balatro_loc      # en-us.lua, game-hands.lua, smods-en-us.lua
export JACKDAW_PATH=/opt/jackdaw              # optional, default third_party/jackdaw
.venv/bin/python scripts/sim_run.py --set bench30-a --jobs 16
```

## Running

```
python scripts/sim_run.py --seed 7X7WSDCX                    # one JEV game
python scripts/sim_run.py --seed A,B,C --jobs 3              # parallel processes
python scripts/sim_run.py --set bench30-a --jobs 8           # a benchmark set (summary.benchmark)
python scripts/sim_run.py --seed A --policy greedy           # no JEV: option 1 everywhere
python scripts/sim_run.py --seed A --policy random           # no JEV: best play, random shop/packs
JEV_BACKEND=sim python src/panel.py                          # the panel driving the sim
```

Each game prints a JSON line (result, ante, decisions, tokens, JEV seconds, errors) and a final
summary line; exit status 1 if any game errored. Runs are recorded normally but under `runs/sim/`
(JEV) or `runs/sim_smoke/` (no-JEV policies), with `summary.backend = "jackdaw"`. Analytics and
the benchmark matrix filter by backend (`analytics.benchmark_matrix(backend=...)`), and real-game
views only count `balatrobot` runs, so sim and real results never share a benchmark cell.

## What is validated

- Replay parity (`sim_parity.py --set bench30-a`, 336 recorded real games, historical profile with
  the 58 then-locked items): see the numbers in `docs/samples/sim-parity.json`. At the time of
  writing 329/336 replay to the end with every state identical (the earlier fidelity audit: 304),
  25,304/25,454 compared states identical, 0 actions rejected by only one side (61 real rejections
  reproduced), 0 engine exceptions.
- Effect text (bench30-a): 1,493,414 card/blind/tag text instances compared against the recorded
  BalatroBot states, 1,493,407 exact (1092/1098 distinct key+text pairs; blinds, tags, tarots,
  planets, spectrals, vouchers, boosters and enhanced cards 100%). The residue is replay card-identity
  artefacts (duplicate-key Base Cards) and a Loyalty Card counter edge case.
- Smoke games with `--policy greedy/random`: 11 seeds through the full JevBot code path (blinds,
  bosses, shops, packs, consumables, rearrange, game over), 0 game or engine errors, ~1.5 s per
  game in parallel. Bot-side forecasting (odds/readiness), not the sim, dominates step time.

## Known gaps

- The Fool used with nothing to copy, from a pack, yields the right set but possibly the wrong card
  (exact `Tarot_Planet` pool/key not reproduced).
- Secret hands' position in `pairs(G.GAME.hands)` is unobserved (appended last): only matters for
  To Do List / Orbital once a secret hand has been played.
- Swashbuckler: Jackdaw excludes debuffed jokers from the sell-value sum when scoring (Lua includes).
- `sort_id` provenance can differ for created cards, which can change Amber Acorn outcomes.
- Ante 9+ (post-win endless) blinds untested; the bot stops at the win.
- Run profile is all-unlocked; historical real runs had 58 locked items, so the sim pools match
  today's real profile, not old recordings (the parity script patches the profile back).
