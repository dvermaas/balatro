# Checking the lab's strategy against the real game (no JEV)

The lab (`jevlab`) plays every game on rumbot, the simulator. The skip-for-tag rules (v45-v48) and the other
lab gains only hold if rumbot plays a seed the way the real game does. This checks that, seed by seed, with no
JEV and no credits.

`jevbot play --lab` plays exactly as `jevlab --arm F` does: the code stand-in for JEV (CodeJev with item picks)
and the lab's forecasts (64 rollouts since v52, no time budget), so decisions do not depend on machine speed. The same seed
played on the real game and on rumbot should then go step for step the same.

## Steps

1. `git checkout main && git pull`, then `cd native && cargo build --release`.
2. The game: close Balatro, run `scripts/launch_fast.cmd`, check `curl -X POST 127.0.0.1:12346 -d
   '{"jsonrpc":"2.0","method":"health","id":1}'`. The profile must have every item unlocked (the sim assumes it).
3. For each seed (any of `docs/samples/cloud-lab/seeds-c2k.txt`, e.g. `RZSTXX2S`):
   ```
   native/target/release/jevbot play --backend balatrobot --lab --seed RZSTXX2S
   native/target/release/jevbot play --backend rumbot     --lab --seed RZSTXX2S
   python docs/samples/cloud-lab/sync_check.py runs/<real run dir> runs/sim/<sim run dir>
   ```
   Run dirs are named `<date>-<time>-<SEED>`. A real game takes a few minutes (the forecasts are the slow part).
4. Read the output:
   - `states: identical` and `actions: identical`: the seed plays the same in both. That is the goal.
   - `states: first difference at state N`: the simulator and the game disagree there, before the bot did
     anything different. Note the seed, the state number and the fields shown; that is a rumbot bug to fix
     (`native/rumbot`, `VENDORED.md`, citing the Lua in `Mods\lovely\dump`).
   - `actions: first difference` with no state difference before it: the bot decided differently from the same
     state, which should not happen (a nondeterminism bug).
   - The per-run list shows each blind skip with its tag, and every free ($0) shop item. After a Rare, Negative,
     Polychrome, Holographic or Foil skip the next shop should hold a free joker (an edition one for the edition
     tags); after an Investment skip the next boss cash-out pays $25 more. The bot may still not buy a free joker
     it rates as useless (the forecast decides).

A handful of seeds (5-10) that reach antes 3-4 is enough to see the tags pay out; any state difference is worth
reporting with its seed.
