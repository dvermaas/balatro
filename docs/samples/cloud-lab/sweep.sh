#!/bin/bash
# sweep.sh SET "EXP1" "EXP2" ...: plays every JEVLAB_EXP arm on seeds-SET.txt (b2k or c2k) and compares each with
# SET-v45-base.jsonl.gz. Run from the repo root after `cd native && cargo build --release`.
set -e
D=docs/samples/cloud-lab; set_=$1; shift
for e in "$@"; do
  n=sweep-$set_-$(echo "$e" | tr -c 'A-Za-z0-9\n' '_')
  rm -rf runs/_policy_lab/$n
  JEVLAB_EXP="$e" native/target/release/jevlab --seeds $(cat $D/seeds-$set_.txt) --arm F --jobs $(nproc) --name $n > /dev/null 2>&1
  echo "== $e"; python3 $D/cmpbase.py $D/$set_-v45-base.jsonl.gz runs/_policy_lab/$n/results.jsonl
done
