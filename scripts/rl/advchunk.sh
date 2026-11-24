#!/bin/bash
# advchunk.sh NAME [EVERY]: one ~2 h chunk of the late-game advisor training (train_rl.py --advise), resumable.
# Restores runs/_rl/ppo/NAME from models/rl/NAME after a container wipe (slim checkpoint: actor, critic, curve; fresh
# optimisers), trains for --hours 1.75, then copies the curve to models/rl/NAME every chunk and the slim checkpoint once it
# is EVERY iterations behind (default 6, ~6 h: a 10 MB file in git history), commits and pushes.
# First chunk (creates the run): see the NEW line below.
set -u
cd "$(dirname "$0")/../.."
NAME=$1 EVERY=${2:-6}
PY=$HOME/rlvenv/bin/python
D=runs/_rl/ppo/$NAME M=models/rl/$NAME
mkdir -p "$M"
if [ ! -f "$D/ckpt.pt" ] && [ -f "$M/ckpt.pt" ]; then mkdir -p "$D"; cp "$M/ckpt.pt" "$D/ckpt.pt"; echo "restored $D from $M"; fi
[ -x runs/_rl/bin/jevlab-adv ] || { echo "runs/_rl/bin/jevlab-adv missing: build native and copy target/release/jevlab there"; exit 1; }
if [ -f "$D/ckpt.pt" ]; then
  $PY scripts/rl/train_rl.py "$NAME" --hours 1.75
else  # NEW: v77 with the round search plays; the item network (PPO it40) may replace its actions in antes 5-8
  $PY scripts/rl/train_rl.py "$NAME" --init models/rl/ppo-item/init --init-ckpt models/rl/ppo-item/ckpt.pt \
    --bin runs/_rl/bin/jevlab-adv --exp "" --antes 5-8 --advise 6 --games 500 --warmup 2 --eval-every 10 --eval-n 200 --hours 1.75
fi
cp "$D/curve.jsonl" "$M/curve.jsonl"
N=$(wc -l < "$M/curve.jsonl")
$PY - "$D/ckpt.pt" "$M/ckpt.pt" "$EVERY" <<'PY'
import os, sys, torch
ck = torch.load(sys.argv[1], map_location="cpu", weights_only=False)
if os.path.exists(sys.argv[2]) and ck["iter"] - torch.load(sys.argv[2], map_location="cpu", weights_only=False)["iter"] < int(sys.argv[3]):
    sys.exit()
torch.save({k: ck[k] for k in ("cfg", "iter", "curve", "actor", "critic")}, sys.argv[2] + ".tmp")
os.replace(sys.argv[2] + ".tmp", sys.argv[2])
print("slim checkpoint saved at iteration", ck["iter"])
PY
git add "$M" && git commit -qm "RL $NAME: $N iterations

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01HHtFAA1KQbirJsqpAChdJM" && for i in 1 2 3 4; do git push -q && break; sleep $((i * 10)); done
tail -2 "$M/curve.jsonl" | cut -c1-600
