#!/bin/bash
# chunk.sh BIN SET START COUNT NAME BASE [EXP]
# Plays seeds START..START+COUNT-1 (0-based) of a seed set with jevlab binary BIN (arm F, JEVLAB_EXP=EXP), appends
# the rows to runs/_policy_lab/NAME/results.jsonl and compares NAME with BASE on the seeds both hold (cmp.py).
# SET is b2k or c2k (docs/samples/cloud-lab/seeds-SET.txt, the two fixed 2,000-seed sets) or a path to a seed list.
# Resumable: seeds already in NAME are skipped, so a killed chunk can simply be rerun. A run dir belongs to one
# experiment: NAME/.chunk holds "BIN-basename EXP", and a different binary or EXP is refused.
# Keep a chunk under ~55 min (4 cores, ~22 s a game: ~450 seeds). Copy BIN out of native/target first, so a rebuild
# mid-run cannot change it. Never delete /tmp/plab_* while a chunk runs: jevlab keeps each game's run dir there.
set -u
R="$(cd "$(dirname "$0")/../.." && pwd)"; L="$R/runs/_policy_lab"
BIN=$1 SET=$2 START=$3 COUNT=$4 NAME=$5 BASE=$6 EXP=${7:-}
[ -f "$SET" ] && LIST="$SET" || LIST="$R/docs/samples/cloud-lab/seeds-$SET.txt"
TAG="$(basename "$BIN") $EXP"
if [ -s "$L/$NAME/results.jsonl" ] && [ "$(cat "$L/$NAME/.chunk" 2>/dev/null)" != "$TAG" ]; then
  echo "chunk.sh: $NAME already holds results from another run ($(cat "$L/$NAME/.chunk" 2>/dev/null || echo 'no tag'))" >&2; exit 1
fi
mkdir -p "$L/$NAME"; touch "$L/$NAME/results.jsonl"; echo "$TAG" > "$L/$NAME/.chunk"
SEEDS=$(python3 - "$L/$NAME/results.jsonl" "$LIST" "$START" "$COUNT" <<'PY'
import json, sys
res, lst, a, n = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
done = {json.loads(l)["seed"] for l in open(res) if l.strip()}
print(",".join(s for s in open(lst).read().split()[a:a + n] if s not in done))
PY
)
if [ -n "$SEEDS" ]; then
  cd "$R" && rm -rf "${L:?}/${NAME:?}-part"
  JEVLAB_SNAPS=1 JEVLAB_EXP="$EXP" "$BIN" --seeds "$SEEDS" --arm F --jobs 4 --name "$NAME-part" > /dev/null 2>&1
  # keep only finished games (a row without "won" is a game error)
  python3 -c "import json,sys; [print(l, end='') for l in open(sys.argv[1]) if 'won' in json.loads(l)]" \
    "$L/$NAME-part/results.jsonl" >> "$L/$NAME/results.jsonl"
fi
python3 "$R/scripts/lab/cmp.py" "$BASE" "$NAME"
