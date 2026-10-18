# Cloud lab results (2026-10-16/17)

Per-game rows (`scripts/policy_lab.py` result format, gzipped) of every policy-lab run from the cloud
session that took the Python bot from v27 to v31/v32. Decision logs are not included. Write-ups:
[docs/policy-lab.md](../../policy-lab.md) (from "Mid-game scaling experiments") and
[docs/lab-lessons.md](../../lab-lessons.md). One row per experiment: [../cloud-experiments.json](../cloud-experiments.json);
per version: [../cloud-versions.json](../cloud-versions.json).

Seed sets (code only, arm F `bot-forecast-items` unless an experiment arm is named):

| file suffix | seeds | command |
|---|---|---|
| `-sim60`, `v27-cloud` | sim60-a | `--set sim60-a` |
| `-fresh60`, `fresh60` | 60 generated | `--seeds 60` |
| `-wins200`, `v28-wins` | 200 generated | `--seeds 200 --seed-offset 60` |
| `v31-train400` | 400 generated | `--seeds 400 --seed-offset 260` |

Versions in `prompt_version`: 79a88610 v27, cb57eae7 v28, a0adcc12 v29, 9ecfe423 v30, e56f8685 v31,
3ba3b355 the merged tree before its v32 note (identical to v31), 5d1e91ea v28 + the Rust smart-discard
experiment (`docs/samples/smart-discard.patch`). Experiment arms are defined in `BOT_ARMS` in
`scripts/policy_lab.py`; any row can be reproduced by re-running its arm on its seed set at its version.
`v1000-snaps` and `v4k-snaps` are 1,000 and 4,000 fresh Rust-lab games with a `snaps` list (one per shop visit; `vcurve.py`).
`free-discard` holds the free-discard arm (sim60-a + 200 fresh seeds; baselines in `value-tail`).
`value-tail` holds the learned tail-value arms (sim60-a and 200 fresh seeds; lab-lessons.md).
`pair-sim60` holds the Rust Pair-commitment arms (`rust-F` baseline, `rust-pair-*`; lab-lessons.md).
`rust-v32-sim60` is the Rust bot (`jevlab --set sim60-a --arm F`) after the v28-v31 port (`prompt_version` "rust").
Some files also hold a copy of the baseline arm's rows they were compared with. `scale-sim60` was
stopped early (27 games).

Compare two runs paired by seed, e.g.:

```python
import gzip, json, statistics
load = lambda f, arm: {r["seed"]: r for r in map(json.loads, gzip.open(f, "rt")) if r["arm"] == arm}
a = load("v31-wins200.jsonl.gz", "bot-forecast-items"); b = load("se-wins200.jsonl.gz", "bot-se05")
d = [b[k]["ante"] - a[k]["ante"] for k in b if k in a]
print(statistics.mean(d), statistics.stdev(d) / len(d) ** 0.5)
```
