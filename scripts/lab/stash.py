#!/usr/bin/env python3
"""stash.py: keep lab rows in git, so a wiped cloud container does not lose finished lab runs.

    stash.py save NAME [NAME ...] [--snaps]   runs/_policy_lab/NAME/results.jsonl -> docs/samples/cloud-lab/rows/NAME.jsonl.gz
    stash.py restore [NAME ...]               the reverse (all stashed runs when no NAME); never overwrites more rows
    stash.py list                             stashed runs and their row counts

Rows are saved without their per-round `snaps` unless --snaps is given (9x smaller; cmp.py, decomp.py and the
win/ante comparisons do not need them). The run's .chunk tag (binary and JEVLAB_EXP) is kept beside the rows, so
chunk.sh can extend a restored run with the same binary name.
"""
import gzip
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
LAB = os.path.join(ROOT, "runs/_policy_lab")
STASH = os.path.join(ROOT, "docs/samples/cloud-lab/rows")


def save(names, snaps):
    os.makedirs(STASH, exist_ok=True)
    for n in names:
        d = os.path.join(LAB, n)
        rows = [json.loads(l) for l in open(os.path.join(d, "results.jsonl")) if l.strip()]
        if not snaps:
            rows = [{k: v for k, v in r.items() if k != "snaps"} for r in rows]
        with gzip.open(os.path.join(STASH, n + ".jsonl.gz"), "wt") as f:
            for r in rows:
                f.write(json.dumps(r) + "\n")
        tag = open(os.path.join(d, ".chunk")).read() if os.path.exists(os.path.join(d, ".chunk")) else ""
        with open(os.path.join(STASH, n + ".chunk"), "w") as f:
            f.write(tag)
        print(n, len(rows), "rows" + (" with snaps" if snaps else ""))


def restore(names):
    names = names or sorted(f[:-9] for f in os.listdir(STASH) if f.endswith(".jsonl.gz"))
    for n in names:
        with gzip.open(os.path.join(STASH, n + ".jsonl.gz"), "rt") as f:
            rows = [l for l in f if l.strip()]
        d = os.path.join(LAB, n)
        p = os.path.join(d, "results.jsonl")
        have = sum(1 for l in open(p) if l.strip()) if os.path.exists(p) else 0
        if have >= len(rows):
            print(n, f"kept ({have} rows here, {len(rows)} stashed)")
            continue
        os.makedirs(d, exist_ok=True)
        with open(p, "w") as f:
            f.writelines(rows)
        tag = os.path.join(STASH, n + ".chunk")
        if os.path.exists(tag):
            with open(os.path.join(d, ".chunk"), "w") as f:
                f.write(open(tag).read())
        print(n, len(rows), "rows restored")


def main(a):
    if not a or a[0] not in ("save", "restore", "list"):
        sys.exit(__doc__)
    if a[0] == "save":
        save([x for x in a[1:] if x != "--snaps"], "--snaps" in a)
    elif a[0] == "restore":
        restore(a[1:])
    else:
        for f in sorted(os.listdir(STASH)) if os.path.isdir(STASH) else []:
            if f.endswith(".jsonl.gz"):
                with gzip.open(os.path.join(STASH, f), "rt") as g:
                    print(f[:-9], sum(1 for _ in g))


if __name__ == "__main__":
    main(sys.argv[1:])
