#!/bin/bash
# gen_bc.sh BIN START COUNT [OUT]
# Behaviour-cloning data from v77's own games: plays seeds START..START+COUNT-1 (0-based) of the training seed
# set docs/samples/cloud-lab/seeds-e16k.txt (disjoint from the b2k / c2k test sets, which must never be used for
# training) with jevlab binary BIN (arm F, no JEVLAB_EXP, 4 jobs) and JEVLAB_RLLOG, writing
#   OUT/games/<seed>.rlb.gz   one log per game (native/rlcore/src/log.rs; read with scripts/rl/read_rlb.py)
#   OUT/games/schema.json     the schema the logs were written with
#   OUT/results.jsonl         jevlab's row per game (won, ante, blinds, ..., rl_records, rl_bytes, rl_s)
# OUT defaults to runs/_rl/bc-v77. Resumable: a seed with a finished row and its log is skipped, so a killed run
# can simply be rerun (rows are appended as games finish). OUT/.gen pins the binary: another BIN is refused.
# ~20 CPU-s a game: ~500 seeds per 45 min on 4 cores. Copy BIN out of native/target first. Never delete
# /tmp/plab_* while it runs.
set -u
R="$(cd "$(dirname "$0")/../.." && pwd)"
BIN=$1 START=$2 COUNT=$3 OUT=${4:-$R/runs/_rl/bc-v77}
LIST="$R/docs/samples/cloud-lab/seeds-e16k.txt"
case "$(cd "$(dirname "$BIN")" && pwd)" in */native/target/*) echo "gen_bc.sh: copy BIN out of native/target first" >&2; exit 1;; esac
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
mkdir -p "$OUT/games"; OUT="$(cd "$OUT" && pwd)"
TAG="$(basename "$BIN") $(sha1sum "$BIN" | cut -c1-12)"
if [ -s "$OUT/results.jsonl" ] && [ "$(cat "$OUT/.gen" 2>/dev/null)" != "$TAG" ]; then
  echo "gen_bc.sh: $OUT already holds games from another binary ($(cat "$OUT/.gen" 2>/dev/null || echo 'no tag'))" >&2; exit 1
fi
touch "$OUT/results.jsonl"; echo "$TAG" > "$OUT/.gen"
SEEDS=$(python3 - "$OUT" "$LIST" "$START" "$COUNT" <<'PY'
import json, os, sys
out, lst, a, n = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
done = set()
for l in open(os.path.join(out, "results.jsonl")):
    if l.strip():
        r = json.loads(l)
        if "won" in r and os.path.exists(os.path.join(out, "games", r["seed"] + ".rlb.gz")):
            done.add(r["seed"])
print(",".join(s for s in open(lst).read().split()[a:a + n] if s not in done))
PY
)
if [ -n "$SEEDS" ]; then
  echo "gen_bc.sh: $(echo "$SEEDS" | tr ',' '\n' | wc -l) games to play -> $OUT" >&2
  cd "$R" && JEVLAB_RLLOG="$OUT/games" "$BIN" --seeds "$SEEDS" --arm F --jobs 4 --out "$(dirname "$OUT")" --name "$(basename "$OUT")" > /dev/null 2> "$OUT/last-run.log"
fi
python3 - "$OUT" "$LIST" "$START" "$COUNT" <<'PY'
import json, os, sys
out, lst, a, n = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
want = set(open(lst).read().split()[a:a + n])
rows = {}
for l in open(os.path.join(out, "results.jsonl")):
    if l.strip():
        r = json.loads(l)
        if "won" in r:
            rows[r["seed"]] = r
got = [rows[s] for s in want if s in rows and os.path.exists(os.path.join(out, "games", s + ".rlb.gz"))]
errs = sum(1 for l in open(os.path.join(out, "results.jsonl")) if l.strip() and "won" not in json.loads(l))
print(f"seeds {a}..{a + n - 1}: {len(got)}/{n} games logged, won {sum(r['won'] for r in got)}, "
      f"records {sum(r.get('rl_records', 0) for r in got)}, unexpressible {sum(r.get('rl_unexpressible', 0) for r in got)}; "
      f"error rows in results.jsonl (retried on rerun): {errs}")
PY
