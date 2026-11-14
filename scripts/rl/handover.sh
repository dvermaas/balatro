#!/bin/bash
# handover.sh BIN LOGROOT NAME@SET:WEIGHTS[:ANTES] ...
# Plays a policy on c2k (SET a: seeds 0-449, b: 450-899) with jevlab BIN (argmax, JEVLAB_RLORDER=dagger,
# JEVLAB_EXP=no-round-search), through scripts/lab/chunk.sh (run runs/_policy_lab/NAME, compared with rl-v77-nors),
# logging its decisions to LOGROOT/NAME (JEVLAB_RLLOG, for behaviour.py). ANTES (lo-hi, optional) is
# JEVLAB_RLANTES: the network decides outside rounds only in those antes, v77's handlers the rest.
# Milestone 7: handover.sh runs/_rl/bin/jevlab-rl10 runs/_rl/m7 rl-ho-p14@a:runs/_rl/ppo/rl-ppo-a/weights/it74.brlw:1-4
cd "$(dirname "$0")/../.."
BIN=$1 LOGROOT=$2; shift 2
for cfg in "$@"; do
  IFS=: read -r ns w antes <<< "$cfg"
  name=${ns%@*}; set=${ns##*@}; start=0; [ "$set" = "b" ] && start=450
  if [ -n "$antes" ]; then export JEVLAB_RLANTES=$antes; else unset JEVLAB_RLANTES; fi
  JEVLAB_RLPOLICY=$w JEVLAB_RLORDER=dagger JEVLAB_RLLOG=$LOGROOT/$name \
    scripts/lab/chunk.sh "$BIN" c2k $start 450 "$name" rl-v77-nors no-round-search
  rm -rf "runs/_policy_lab/$name-part"
done
