#!/bin/bash
# flipq.sh BIN FIRST_SALT LAST_SALT [N]: paired-decision data for the learned shop correction (docs/lab-lessons.md).
# For each salt, plays every seed of c2k and b2k with JEVLAB_EXP="flip-rand=N,flip-salt=S" (one forecast-backed shop
# decision per game taken the other way) into runs/_policy_lab/flip-SET-sS, one 450-seed chunk at a time (chunk.sh,
# resumable: rerun the same command after a restart or a tool time limit; finished games of a killed chunk are kept). The unflipped partner of each game is SET-v77.
set -u
R="$(cd "$(dirname "$0")/../.." && pwd)"
BIN=$1 A=$2 B=$3 N=${4:-60}
for S in $(seq "$A" "$B"); do
  for SET in c2k b2k; do
    for START in 0 450 900 1350 1800; do
      echo "$(date -u +%FT%TZ) salt $S $SET $START"
      # a chunk killed mid-run (time limit, restart) left finished games in NAME-part: keep them before chunk.sh clears it
      P="$R/runs/_policy_lab/flip-$SET-s$S"
      if [ -f "$P-part/results.jsonl" ] && [ -f "$P/results.jsonl" ]; then
        python3 - "$P" <<'PY'
import json, sys
p = sys.argv[1]; done = {json.loads(l)["seed"] for l in open(p + "/results.jsonl") if l.strip()}
new = [l for l in open(p + "-part/results.jsonl") if l.strip() and "won" in json.loads(l) and json.loads(l)["seed"] not in done]
open(p + "/results.jsonl", "a").writelines(l if l.endswith("\n") else l + "\n" for l in new)
PY
      fi
      "$R/scripts/lab/chunk.sh" "$BIN" "$SET" "$START" 450 "flip-$SET-s$S" "$SET-v77" "flip-rand=$N,flip-salt=$S" | tail -1
    done
  done
done
echo "$(date -u +%FT%TZ) done"
