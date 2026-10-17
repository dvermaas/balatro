# Rust port: guide for porting agents

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
- `native/rumbot`: the game engine (rumbot-sim fork). `native/sim`: today's PyO3 rollouts on it.

## Reference data (not in git)

`runs/_port/` of a checkout (Rust tests read `JEV_PORT_DATA`, default
`C:/Users/admin/Documents/Repos/balatro-rust/runs/_port`). Regenerate it from the current Python bot after
any Python behaviour change (v31: 60 traces in ~3 min, every fixture in ~10 min on 4 cores):
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
