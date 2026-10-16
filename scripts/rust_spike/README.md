# Rust engine spike (lockstep vs Jackdaw)

Plays the same seeds with the same actions in Jackdaw (with `src/sim_patches.py`), rumbot-sim and
Gambit, and reports each Rust engine's first divergence from Jackdaw. Results and conclusions are in
[docs/rust-port-scope.md](../../docs/rust-port-scope.md).

```
cd scripts/rust_spike
git clone https://github.com/mmank/rumbot-sim ext/rumbot-sim && git -C ext/rumbot-sim checkout 888c5165de58d69bd48d9ddbfbd3209f2b064820
git clone https://github.com/PrimalMentat15/Gambit ext/Gambit && git -C ext/Gambit checkout 8dc1716a9f7d75c25069beaed8944f8f84db8763
(cd rumbot_drv && cargo build --release) && (cd gambit_drv && cargo build --release)
cd ../..
python scripts/rust_spike/lockstep.py 300 docs/samples/rust-spike-lockstep.json   # ~20 s
python scripts/rust_spike/classify.py docs/samples/rust-spike-lockstep.json       # causes
python scripts/rust_spike/trace.py GVUPXANG 9                                      # last states before a divergence
rumbot_drv/target/release/greedy 5000                                              # rumbot speed, greedy policy
```

`ext/` and the Cargo build output are git-ignored. Neither engine has a build script; rumbot-sim has
no dependencies, and Gambit's core builds without its optional `serde`.
