# rumbot-sim

Balatro as a Rust library: the crate `rumbot_sim`, a module-for-module
translation of the pure-Python simulator in
[jimbot-sim](https://github.com/mmank/jimbot-sim), checked against it (and,
for face-down cards and recorded human games, against the game itself) by
differential fixtures. Deterministic given a seed, no I/O, no dependencies;
the run is a phase-based state machine:

```rust
let mut game = rumbot_sim::game::GameState::new("SEED0000", "Red Deck", 1);
while !game.is_over() {
    game.step(&game.legal_actions()[0]);
}
```

Upstream, its user is [rumbot](https://github.com/mmank/rumbot), the handcrafted policy that plays it. **In this repo**
it is jevbot's simulator: the shop forecasts' rollouts (`native/simcore`), whole sim games (`engine::RumbotGame`) and
the policy lab (`jevlab`). This copy keeps only the library: upstream's tests, bins (`sim_bench`) and build profiles
were left out. Local changes, each citing the game's Lua, are in [VENDORED.md](VENDORED.md).

- [PORTING.md](PORTING.md) -- upstream's design notes: the frozen interface (shared `Rc<RefCell<_>>`
  handles, `fn`-pointer hooks, the module map), how the differential fixtures
  work, and the bugs each one caught.
