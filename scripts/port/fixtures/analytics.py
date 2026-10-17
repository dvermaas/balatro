"""Fixture for native/bot/src/analytics.rs: calls the Python analytics.py functions on recorded runs
(read only: nothing is written under runs/) and stores each return value.

Rows: {"fn", "args", "ret"}; fn is one of prompt_versions, run_backend, load_decisions, run_metrics,
all_runs, version_stats, benchmark_matrix. Paths in args are absolute (POSIX-style).
Run: C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/analytics.py
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402  (puts the frozen v27 src/ of the main port worktree on sys.path)
import analytics  # noqa: E402
import benchmark  # noqa: E402
import recorder  # noqa: E402

MAIN = Path("C:/Users/admin/Documents/Repos/balatro/runs")
SIM = MAIN / "sim"
N_REAL, N_SIM = 30, 10

rows = []


def add(fn, args, ret):
    rows.append({"fn": fn, "args": args, "ret": ret})


def dirs_with_summary(root):
    return sorted(p for p in root.glob("*/summary.json") if p.is_file())


def pick(paths, n):
    step = max(1, len(paths) // n)
    return paths[::step][:n]


def summaries(root):
    return {p.parent.name: json.loads(p.read_text(encoding="utf-8")) for p in dirs_with_summary(root)}


real_paths = dirs_with_summary(MAIN)
sim_paths = dirs_with_summary(SIM)
sample = pick(real_paths, N_REAL) + pick(sim_paths, N_SIM)

prompts_dir = recorder.PROMPTS_DIR
current = recorder.PROMPT_VERSION
add("prompt_versions", {"prompts_dir": prompts_dir.as_posix(), "current": current},
    analytics.prompt_versions())

for p in sample:
    d = p.parent
    add("run_backend", {"run": json.loads(p.read_text(encoding="utf-8"))},
        analytics.run_backend(json.loads(p.read_text(encoding="utf-8"))))
    add("run_metrics", {"run_dir": d.as_posix()}, analytics.run_metrics(d))
for p in sample[:5] + sample[-5:]:
    d = p.parent
    add("load_decisions", {"run_dir": d.as_posix()}, analytics.load_decisions(d))

for root, backend in ((MAIN, "balatrobot"), (SIM, "jackdaw")):
    add("all_runs", {"root": root.as_posix()}, analytics.all_runs(root))
    add("version_stats", {"backend": backend, "root": root.as_posix(), "prompts_dir": prompts_dir.as_posix(),
                          "current": current},
        analytics.version_stats(backend, root))

# Benchmark names present in the recorded summaries, plus the default and an unknown name.
names = set()
for root in (MAIN, SIM):
    for s in summaries(root).values():
        b = s.get("benchmark") or {}
        if b.get("name"):
            names.add(b["name"])
for name in sorted(names) + [None, "not-a-set"]:
    for root, backend in ((MAIN, "balatrobot"), (SIM, "jackdaw")):
        add("benchmark_matrix",
            {"name": name, "backend": backend, "root": root.as_posix(), "prompts_dir": prompts_dir.as_posix(),
             "current": current},
            analytics.benchmark_matrix(name, backend, root))

path = states.write_fixture("analytics", rows)
print(f"{len(rows)} rows, {len(names)} benchmark names ({sorted(names)}) -> {path}")
print("sets:", list(benchmark.SETS))
