#!/bin/bash
# e16k_driver.sh: v61 on the 16,000 e16k seeds in 200-seed chunks. Resumable: on start it salvages the finished games
# of a chunk killed by a restart, and done seeds are skipped. Merged rows: runs/_policy_lab/e16k-v61/results.jsonl.
S=/tmp/claude-0/-home-user-balatro/67ba2d27-f901-5d1c-93fc-3995ad80f262/scratchpad; L=/home/user/balatro/runs/_policy_lab
BIN=$S/jevlab-v61; NAME=e16k-v61
mkdir -p $L/$NAME; touch $L/$NAME/results.jsonl
cd /home/user/balatro
if [ -s $L/$NAME-part/results.jsonl ]; then
  python3 -c "
import json
done={json.loads(l)['seed'] for l in open('$L/$NAME/results.jsonl')}
with open('$L/$NAME/results.jsonl','a') as f:
    for l in open('$L/$NAME-part/results.jsonl'):
        l=l.strip()
        if l and json.loads(l)['seed'] not in done: f.write(l+'\n')"
  echo "$(date -u +%H:%M) salvaged part: $(wc -l < $L/$NAME/results.jsonl) games" >> $S/e16k_driver.log
fi
rm -rf $L/$NAME-part
for START in $(seq 0 200 15800); do
  SEEDS=$(python3 -c "
import json
done={json.loads(l)['seed'] for l in open('$L/$NAME/results.jsonl')}
s=open('$S/e16k.lines').read().split()[$START:$START+200]
print(','.join(x for x in s if x not in done))")
  [ -z "$SEEDS" ] && continue
  rm -rf $L/$NAME-part
  echo "$(date -u +%H:%M) chunk $START" >> $S/e16k_driver.log
  JEVLAB_SNAPS=1 $BIN --seeds $SEEDS --arm F --jobs 4 --name $NAME-part > /dev/null 2>&1
  cat $L/$NAME-part/results.jsonl >> $L/$NAME/results.jsonl
  rm -rf $L/$NAME-part
  echo "$(date -u +%H:%M) done $START: $(wc -l < $L/$NAME/results.jsonl) games" >> $S/e16k_driver.log
done
echo "$(date -u +%H:%M) ALL DONE" >> $S/e16k_driver.log
