#!/bin/bash
# gen_dagger.sh BIN WEIGHTS OUT START COUNT K [K ...]
# DAgger labels: the network WEIGHTS (argmax, JEVLAB_RLORDER=dagger, JEVLAB_EXP=no-round-search) decides outside
# rounds in antes 1..K (JEVLAB_RLANTES=1-K) and v77's handlers from ante K+1 on, so v77 labels the late states the
# network's own early game reaches. For each K in turn, plays the next COUNT seeds of the training set
# docs/samples/cloud-lab/seeds-e16k.txt, starting at START (0-based): K1 gets START.., K2 START+COUNT.., ...
# (never b2k / c2k, and keep clear of e16k 15000-15299, PPO's validation). Writes
#   OUT/kK/games/<seed>.rlb.gz   one log per game (records with ante <= K are the network's, the rest v77's;
#                                v77's sale-then-buy swaps are one replace_joker record, as in bc-v77-s2)
#   OUT/kK/results.jsonl         jevlab's rows
# Needs jevlab-rl11 or newer (rl10 logs v77's swaps as a lone sale plus a buy). Resumable per K: seeds with a row
# and a log are skipped. Removes the jevlab's own /tmp/plab_*_<pid> dirs once it has exited.
set -u
R="$(cd "$(dirname "$0")/../.." && pwd)"
BIN=$1 W=$2 OUT=$3 START=$4 COUNT=$5; shift 5
LIST="$R/docs/samples/cloud-lab/seeds-e16k.txt"
case "$(cd "$(dirname "$BIN")" && pwd)" in */native/target/*) echo "gen_dagger.sh: copy BIN out of native/target first" >&2; exit 1;; esac
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
W="$(cd "$(dirname "$W")" && pwd)/$(basename "$W")"
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
s=$START
for K in "$@"; do
  D="$OUT/k$K"; mkdir -p "$D/games"; touch "$D/results.jsonl"
  echo "$(basename "$BIN") $(sha1sum "$BIN" | cut -c1-12) $W 1-$K e16k $s+$COUNT" > "$D/.gen"
  SEEDS=$(python3 - "$D" "$LIST" "$s" "$COUNT" <<'PY'
import json, os, sys
d, lst, a, n = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
assert a + n <= 15000, "e16k 15000-15299 is PPO's validation range"
done = {r["seed"] for r in map(json.loads, filter(str.strip, open(os.path.join(d, "results.jsonl"))))
        if "won" in r and os.path.exists(os.path.join(d, "games", r["seed"] + ".rlb.gz"))}
print(",".join(x for x in open(lst).read().split()[a:a + n] if x not in done))
PY
)
  if [ -n "$SEEDS" ]; then
    cd "$R"
    JEVLAB_RLANTES=1-$K JEVLAB_RLPOLICY="$W" JEVLAB_RLORDER=dagger JEVLAB_EXP=no-round-search JEVLAB_RMDIRS=1 \
      JEVLAB_RLLOG="$D/games" "$BIN" --seeds "$SEEDS" --arm F --jobs 4 --out "$OUT" --name "k$K-part" > /dev/null 2> "$D/last-run.log" &
    pid=$!; wait $pid
    rm -rf /tmp/plab_*_"$pid"
    python3 -c "import json,sys; [print(l, end='') for l in open(sys.argv[1]) if l.strip() and 'won' in json.loads(l)]" \
      "$OUT/k$K-part/results.jsonl" >> "$D/results.jsonl"
    rm -rf "$OUT/k$K-part"
  fi
  python3 - "$D" <<'PY'
import json, os, sys
rows = [json.loads(l) for l in open(os.path.join(sys.argv[1], "results.jsonl")) if l.strip()]
print(f"{sys.argv[1]}: {len(rows)} games, won {sum(r['won'] for r in rows)}, mean ante {sum(r['ante'] for r in rows) / max(1, len(rows)):.2f}, "
      f"records {sum(r.get('rl_records', 0) for r in rows)}, network decisions {sum(r.get('rl_decisions', 0) for r in rows)}, "
      f"v77 states {sum(r.get('rl_v77_states', 0) for r in rows)}")
PY
  s=$((s + COUNT))
done
