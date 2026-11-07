# Rust port: guide for porting agents

> **Historical**: the port is complete and the Python code was removed (git history up to `4628654`). Still useful:
> "Porting conventions" and "Reference data" (the references can no longer be regenerated) and "Port results".

Branch `rust-port` (worktree `C:/Users/admin/Documents/Repos/balatro-rust`), frozen at v27. Goal: the
whole bot in Rust, with **identical behaviour** to the v27 Python bot, checked seed by seed. Plan
and status: [rust-port-scope.md](rust-port-scope.md) and the README section "Rust port".

## Layout

- `native/pycompat`: CPython-exact float repr/format/round, `fsum`, `comb`, `statistics`,
  `random.Random`, `json.dumps`. Use it instead of hand-rolling any of these.
- `native/bot` (crate `jevbot`): one Rust module per Python module, same names:
  `hands.rs` ← `src/hands.py`, `decisions/shop.rs` ← part of `src/decisions.py`, and so on.
  - `gs.rs`: Python dict semantics over the gamestate (`d`, `get`, `truthy`, `num`, `int`,
    `cards_of`, `area`). The gamestate stays a `serde_json::Value`.
  - `iface.rs`: the `Game`, `Jev` and `Oracle` traits.
  - `trace.rs`: reference traces and fixture paths.
- `native/kernel` (`jev_kernel`): the existing hand scorer (`classify`, `Scorer`, `each_play`).
  `src/native.py` shows how the Python side encodes inputs for it; in Rust call it directly.
- `native/rumbot`: the game engine (rumbot-sim fork). `native/simcore`: the rollouts on it (the PyO3 `native/sim`
  crate of the Python era is gone).

## Reference data (not in git)

`runs/_port/` of a checkout (Rust tests read `$JEV_PORT_DATA`, else this repo's `runs/_port`, else the old port
worktree's; `native/bot/src/trace.rs`). It was regenerated from the Python bot after each Python behaviour change
(v31: 60 traces in ~3 min, every fixture in ~10 min on 4 cores); with the Python code gone it can no longer be:
`scripts/port/trace_game.py --set sim30-a --count 30 --arm <arm> --out runs/_port/traces/<arm>` for both
arms, then every `scripts/port/fixtures/*.py` (analytics, panel, typesafe, pycompat and recorder_list read
or write fixed Windows paths of the main machine).
- `traces/<arm>/<seed>.jsonl.gz`: 30 sim30-a games per arm, played by the Python bot (v27, then v31)
  (`bot-forecast-items` = arm F, `bot-forecast-long` = arm E). Format: docstring of
  `scripts/port/trace_game.py`. Every game call and result, every JEV request and answer, every
  bot event, and the forecast/Picker calls ("oracles") with their results.
- `fixtures/<name>.jsonl.gz`: unit fixtures, one file per module, made by
  `scripts/port/fixtures/<name>.py` (the porting agent writes it). Inputs come from the traces'
  gamestates: `scripts/port/states.py` in Python, `jevbot::trace::states` in Rust.

Python reference: the checkout's own `src/` (or `JEV_PORT_ROOT`'s): `import states` (from
`scripts/port/`) puts it on `sys.path`; set `JEV_PORT_ROOT` to a full checkout when your worktree has no
Jackdaw submodule or localization. Native modules are installed; they give the same results as pure Python
(`JEV_NATIVE=0` forces pure Python).

## Protocol for each task

1. **Own worktree and branch.** Never work in the main port worktree:
   ```
   git -C C:/Users/admin/Documents/Repos/balatro-rust worktree add C:/Users/admin/Documents/Repos/port-wt/<task> -b port/<task> rust-port
   ```
2. **Build/test** in your worktree: `export PATH="$HOME/.cargo/bin:$PATH"; cd native/bot; cargo test --test <name>`
   (or `cd native/pycompat`). Run cargo commands with a long timeout (first build ~1-2 min).
   Show only the tail of cargo output (`2>&1 | tail -30`); never dump full logs into your context.
3. **Only touch your files**: the Rust files and fixture script named in your brief, plus a test
   file `native/bot/tests/<name>.rs`. If shared code (`gs.rs`, `pycompat`, another module) lacks
   something, add a private helper in your own file with a `// SHARED?` comment and say so in your
   report. Do not edit other modules; if one you depend on is wrong, report it.
4. **Exactness is the bar.** A test passes only when every fixture row matches exactly: strings
   byte for byte, floats bit for bit (`to_bits()`), ints vs floats kept apart (Python `3` vs `3.0`
   print differently). Report "N/N rows identical". Do not loosen comparisons to make tests pass.
5. **Read narrowly.** Read the Python you port and the signatures you need (`grep -n "def \|^pub fn"`),
   not whole unrelated files. Your context budget is 100k tokens. A hook warns you at 90k: then
   stop, commit, and write `docs/port-handoff/<task>.md` (done, left, exact next step, failing rows).
6. **Finish**: `cargo test` green for your test, `git add` your files, `git commit` on your branch
   (no attribution lines). Final report, under 15 lines: branch, files, "N/N identical" per test,
   anything shared you needed, open problems.

## Porting conventions

**Structure.** Port line by line, keeping the Python function names (snake_case) and order, with
the Python line range in a comment above each function (`// decisions.py 616-825`). Same branches,
same early returns. Do not "improve" or reorder logic: a port that is cleaner but not identical is
wrong. Comments: keep the Python comments that explain *why*.

**The gamestate** is `&Value`. `_d(x)` → `gs::d(x)`, `x.get("k")` → `gs::get(x, "k")`,
`x or 0` → `gs::num(..)` / `gs::int(..)`, truthiness → `gs::truthy`. Build dicts that leave the
module (JEV state, options, event data) as `serde_json::Value` with `json!` or `Map`, inserting keys
in **exactly the Python insertion order** (`serde_json` has `preserve_order` on).

**Python behaviours that silently break exactness:**
- **int vs float.** `a / b` is always float; `//` floors, also for negatives (`-7 // 2 == -4`;
  for ints with a positive divisor that is `a.div_euclid(b)`); `%` takes the divisor's sign
  (`rem_euclid` for positive divisors); `int * float` is float; `round(x)` returns an int,
  `round(x, n)` a float. Track which one every printed or JSON value is.
- **Formatting.** `f"{x}"` of a float → `pycompat::pyfloat`; `:.2f` → `pycompat::fixed(x, 2)`;
  `:+.2f` → `pycompat::signed`; `round(x, n)` → `pycompat::pyround`, `round(x)` →
  `pycompat::pyround_int`. Rust's `{}` / `{:.2}` are NOT the same. `f"{None}"` is "None",
  `f"{True}"` is "True". `str(list)` / `repr` of lists and dicts use Python repr (`['a', 1]`).
- **Rounding** is half-to-even on the exact binary value (`round(2.5) == 2`, `round(0.125, 2) == 0.12`).
- **Sums.** `sum(xs)` adds left to right starting at int 0 (sum of ints is an int); `math.fsum` is
  exact (`pycompat::fsum`). `statistics.mean/stdev/median` are exact and type-preserving: use
  `pycompat::statistics`. Never reorder a float sum or product.
- **min/max** return the *first* extreme element; Rust's `Iterator::max_by` returns the *last*.
  Use a manual loop or `min_by`/`max_by` with care. `max(xs, key=f, default=d)` likewise.
- **Sorting** is stable in both; `sorted(key=lambda: (a, -b))` → `sort_by` comparing tuples in the
  same order. Python compares floats with `<`; NaN never appears in practice. `reverse=True` is a
  stable reverse sort (equal elements keep their order), NOT `sort` then `reverse()`.
- **Dicts** iterate in insertion order; `dict.update`/re-assigning an existing key keeps its
  position. Use `serde_json::Map` or `indexmap`-style ordered maps, never `HashMap`, for anything
  whose order can reach output or decisions. **Sets of strings** iterate in random order in
  Python: if output depends on set order, report it (it is a Python bug); membership is fine.
- **Strings.** `str.title()`, `.capitalize()`, `.split()` (no arg splits on runs of whitespace),
  `.strip()`, slicing by code points (not bytes), `in` is substring. Port exactly.
- **Regex.** Python `re` → `regex` crate; lookaround or backreferences → `fancy_regex`.
  `re.match` anchors at the start only; `re.fullmatch` at both ends; `re.search` anywhere.
  Python `\d`, `\w` are Unicode-aware like `regex`'s defaults. Compile once (`OnceLock`).
- **Random.** `random.Random(seed)` → `pycompat::Random` (same seed type: int or str).
  `rng.sample(pop, k)` → `sample_indices(len, k)` mapped. Never use `rand`.
- **Exceptions.** `try/except` around engine or game calls becomes `Result`; keep the same fallback
  path. Errors the Python code would raise and nobody catches → `panic!` with a clear message.

**Floats** in `jev_kernel` are already identical to Python (same operation order); keep the order
of operations when you combine its results.

## Testing pattern

1. `scripts/port/fixtures/<name>.py`: imports the Python module, runs the functions you port over
   inputs from `states.states(...)` (and any derived inputs, e.g. each hand's candidates), and
   writes rows `{"fn": ..., "args": ..., "ret": ...}` with `states.write_fixture("<name>", rows)`.
   Cover every public function and the branches that matter; a few thousand rows is plenty.
   Encode tuples as lists, sets as sorted lists. Keep floats as floats.
2. `native/bot/tests/<name>.rs`: reads the fixture with `jevbot::trace::read_jsonl`, calls the Rust
   function, compares `serde_json::to_string` of result and expectation (this checks key order and
   int/float types too). Collect mismatches; print the first 5 compactly (`trace::short`); assert
   none. Print "N/N identical".

## Port results (historical, from the README)

Tests: `cd native && cargo test --release`. Most compare against fixtures and traces
recorded from the Python bot in `runs/_port` (not in git; `$JEV_PORT_DATA` overrides the path). They
stay valid without the Python code, but can no longer be regenerated, so a behaviour change makes
them stale.

At Python v31 (references regenerated from v32's Python, see "Reference data" above):

- Bot replay: 60/60 games identical; CodeJev 2535/2535 requests; every decision fixture identical (jokers,
  picker, readiness, scorer, hands, odds, dec_shop_a, fc_stats). Forecasts: items 2913/2916, packs
  2223/2224 (`fc_items` is slow, ~2-9 min); all four misses are one Jackdaw bug (an Arcana pack's tarot
  duplicating one we hold; [followups](port-handoff/followups.md)).
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
