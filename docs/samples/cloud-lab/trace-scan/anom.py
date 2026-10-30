"""anom.py: per traced game, the losing round's last play (discards left) and zero-chip plays."""
import json, glob, collections, sys
zero = collections.Counter(); last = []
for f in glob.glob(sys.argv[1] + '/*/decisions.jsonl'):
    L = [json.loads(l) for l in open(f)]
    outs = [r for r in L if r.get('kind') == 'outcome']
    for r in outs:
        if r['method'] == 'play' and r.get('chips_delta', 1) == 0:
            zero[(r['ante'])] += 1
    end = [r for r in L if r.get('kind') == 'run_end']
    if end and not end[-1].get('won', False):
        plays = [r for r in outs if r['method'] == 'play']
        if plays:
            p = plays[-1]
            last.append((f.split('/')[2], p['ante'], p['hands_left'], p['discards_left'], p['chips']))
print('zero-chip plays by ante', sorted(zero.items()))
print('losses', len(last), 'with discards left at the final play', sum(1 for x in last if x[3] > 0))
for x in last:
    if x[3] > 0:
        print(x)
