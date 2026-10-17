# Forecast port plan (rollout.py + rollout_items.py -> Rust)

Scope: `rollout.forecast(pub,n,bosses,keys,budget_s)`, `rollout_items.forecast(...)`, `Picker.values`, `Picker.pick_pub`.
Oracles: trace lines `{"k":"oracle","fn","args","ret"}` (scripts/port/trace_game.py). `args.pub` is the public gamestate, so a
Rust forecast can be replayed from the trace alone. `budget_s` is wall-clock: see risk 6.

## 1. Data flow today

### rollout.forecast (shop options, n=40, bosses=2)
1. `build_state(pub)` (Python/Jackdaw): `initialize_run(deck, stake, "ROLLOUT")`, vouchers applied (`apply_voucher`), deck = cards+hand as a
   canonically SORTED multiset (never draw order), jokers via `joker_card` (running state parsed out of the effect text by `_joker_dynamic`),
   consumables, limits, money, round, hand levels, idol/mail/ancient/castle targets, ante/boss/blind states, `shop_cards`/`shop_vouchers`
   from pub, `_rollout_no_shop=True`, `shop_boosters=[]`.
2. `options(pub)`: pure function of pub (skip, buy_card_i, buy_voucher_i, replace_j_i = sell j + buy i). `supported` = JOKER/PLANET/DEFAULT/
   ENHANCED (+vouchers, replaces); the rest are listed in `unsupported`. The `keys` filter always keeps `skip`.
3. `Policy(option_jokers(pub,opt))`: the text-scorer jokers after the option (pure pub math).
4. `Snapshot(base)` pickles the state; `run_options`:
   - Rust path (`sim_bridge.available()`): per option, copy snapshot, set `rng=PseudoRandom("RO000000")`, run `apply_option` in Jackdaw
     (`step(BuyCard/SellCard/RedeemVoucher)`). If the RNG state is unchanged (almost always), `sim_bridge.describe(gs)` -> JSON ->
     `jev_sim.rollouts(desc, policy_jokers, seeds=[RO000000..], bosses)`. In Rust `build(desc)` makes the rumbot `GameState`; per seed
     `g.rng = RunRng::new(seed)`, then `play_horizon`. The option's effect (cost, joker added, voucher effects, planet used) is computed by
     JACKDAW; rumbot only sees the post-option state.
   - If the option drew RNG (e.g. selling Invisible Joker): per seed, `rng=PseudoRandom(seed)`, apply in Jackdaw, `describe(gs, rng=True)`
     carries the Jackdaw pseudoseed pools (`rng_pools`) into `RunRng.pools`, so the stream continues exactly.
   - Chunks of 8 rollouts when `budget_s` is set (stop after >=8 once the wall clock passed), else all n at once.
5. `summarize(runs,bosses)` per option, plus `vs_skip` via `paired` (mean/SE of paired differences, `round(x,3)`). The result has `seconds`
   (wall time) and `_runs`; compare everything except those.
6. The `forecast` wrapper saves/restores `card_mod._sort_id_counter` (Jackdaw-only; irrelevant in Rust).

### rollout_items.forecast (packs, shop tarots/spectrals, n=16, bosses=3)
1. `build_state` = rollout.build_state + `shop_boosters` from pub["packs"]. `Picker(pub)` reads pub; `synth_plays` deals k hands from the deck
   multiset with `random.Random("SYNTH")` (Python MT19937, a pure function of pub).
2. `options(pub)`: skip, buy tarot/spectral (`usable`), buy pack i (cost <= money).
3. Per option and per k: `snap.copy()`, `rng=PseudoRandom("RO{k}")`, then in JACKDAW `OpenBooster` (draws pack contents from the pseudoseed
   streams), `open_and_pick` (`Picker.pick` over `_area_cards` views -> `PickPackCard`/`SkipPack`, max 6 picks). Pack generation, edition/
   enhancement rolls, tarot targets and effects all run in Jackdaw RNG.
4. The horizon: `sim_bridge.run_strict(gs, policy, bosses, items)` = `describe(gs, rng=True, items)` -> `jev_sim.rollout_strict` with the Jackdaw
   streams. Rust returns None (STRICT_ABORT) when the bot would use a tarot at a blind select or jokers change in a way Rust cannot follow;
   then Python continues the rollout (`use_items`, `_refresh`, policy cache keyed by `_jsig`). v27 made this ~1%.
5. summarize/paired as above (no `_runs`; adds `errors`).

### What a bit-exact Rust forecast must reproduce
- Seeds `RO{k:06d}` -> rumbot `RunRng::new(seed)` is the same stream family as Jackdaw `PseudoRandom(seed)` (the existing 56/56 identity
  checks). The starting point is the post-option state, so Rust must apply options itself and (items) open packs with the same draws in the
  same order as Jackdaw's `OpenBooster` + `PickPackCard`.
- Row semantics `{ante,type,boss,target,chips,cleared}` and the horizon rule already live in lib.rs `play_horizon` (reuse).
- summarize/paired float math = Python `statistics.mean/stdev` and `round(x,3)` (pycompat helpers). Dict key order: decide in task 2 whether the
  oracle compare is order-sensitive.

## 2. All-Rust design

Shared crate `native/simcore` (non-PyO3) holds what lib.rs has besides glue: `Policy`, `candidates/straight/info`, `discard`, `plan`/`use_items`/
`use_planets`, `refresh_policy`, `play_horizon(_traced)`, `Row`, and `build(desc)` split into `desc -> GameState`. `native/sim` keeps the
`#[pyfunction]` wrappers and depends on simcore; `native/bot` (jevbot) depends on simcore too. Moving the code is a speed-only prompt version
(files under native/ are hashed): one commit, gated by the 56/56 identity check (CLAUDE.md), before anything builds on it.

New Rust in jevbot (`forecast.rs`, `forecast_items.rs`): `state_from_pub(&Gs) -> GameState` replaces build_state + describe in one step.

| Jackdaw-side step today | Rust replacement |
|---|---|
| `initialize_run` + vouchers | `GameState::new("JEVROLLOUT", deck, stake)` + `voucher_by_key` (already in `build`) |
| deck multiset sort, `playing_card` | sort pub cards with the same key; build rumbot cards (rank/suit/enh/seal/edition; `sim_bridge._playing`) |
| `joker_card`/`_joker_dynamic` (regex on effect text) | regex on pub text straight to rumbot `JokerInstance.counter/secondary/hands_at_create/named_hand/extra_sell_value`, composing the text->ability and `sim_bridge_jokers` ability->counter tables per joker (skips the Jackdaw ability hop; test per joker) |
| hand levels, idol/mail/ancient/castle, boss, blind states, usage counters | direct field sets as in `build`, from pub fields (planets_used -> first N planets as `unique_planets`) |
| `apply_option` (BuyCard/SellCard/RedeemVoucher in Jackdaw `step`) | engine/actions.rs buy/sell/voucher on the built state. Needs: buy joker (add_to_deck effects, cost, slots, negative), buy planet (used at once), buy playing card, redeem voucher effects, sell (extra_sell_value; Invisible Joker draws RNG) |
| `Snapshot` pickle | `GameState::deep_clone()` |
| RNG-drawing option: describe(rng=True) carry | always set `g.rng=RunRng::new(seed)` BEFORE applying the option; no carry code. Optionally reuse one applied state per option when no draw happened |
| `OpenBooster` + `open_and_pick` | rumbot pack open (engine `pack`) producing view dicts (engine/view.rs); Picker.pick ported against them. BIGGEST GAP: pack content must draw like Jackdaw. engine.md lists open diffs (`p_buffoon_normal_1` vs `_2`, `pack.limit`) |
| items `play_horizon` Python fallback (`use_items`, `_refresh`) | Rust `play_horizon_traced(items=Some)` already uses items; make STRICT_ABORT cases native or fail loudly |
| `Policy(option_jokers)` | jevbot has the text-scorer (jokers.rs, strategy.rs); `option_jokers` is pure pub math |
| summarize/paired/forecast assembly | plain f64 + pycompat round/mean/stdev, output `serde_json::Value` |

Budget: take `budget_s`/a max-chunks argument as input; replay traces with `n = ret.rollouts` (cut-offs are 8-aligned) so tests are deterministic.

## 3. Risks and isolated tests

1. **Pack-opening RNG/content parity (items).** rumbot pack generation vs Jackdaw `OpenBooster` (card count, rarity, tarot targets, editions,
   Soul/Black Hole, pack variant/limit). Test: python script emits fixtures (seed string, pack key, ante, pre-state hash -> pack cards) from ~200
   sampled traced calls; Rust must match; then per-option `buy_pack_i` rows and `taken` for k=0..15. Picker.values/pick_pub oracle lines need no engine.
2. **Joker running state from pub text.** Old chain: text -> Jackdaw ability -> rumbot counter. Rounding or parse gaps (Ride the Bus, Obelisk,
   Green Joker, Hologram/Constellation xMult floats, Campfire, Loyalty countdown, Yorick, To Do hand, Ceremonial Dagger, Hiker chips) change rows.
   Test per joker key: dump `(counter, secondary, hands_at_create, named_hand, extra_sell_value)` via the Python chain over every joker dict in the
   traces (style of scripts/diagnostics/rumbot_joker_parity.py); Rust must equal.
3. **Option side effects computed by Jackdaw.** Voucher effects (Overstock, Hieroglyph, Astronomer...), buy-time rolls (Invisible, Riff-raff, Burglar),
   edition sell values, replace ordering (sell credit before buy), planet use on purchase, negative slots. Test per option key over all recorded
   calls: post-option summary (money, jokers+counters, deck sequence, levels, vouchers, RNG-pool hash) from Python `apply_option`+`describe` fixtures
   vs Rust, with seeds RO000000..RO000003.
4. **Deck order / card identity.** Rollout draws shuffle the vector in the order `describe` emitted (canonical sort, then hand, discard, played; bought
   playing cards appended). Test: dump (rank,suit,enh,seal,edition) sequences of `describe` for ~100 calls; compare with Rust deck. Check rumbot
   for uid-dependent tie-breaks (grep sorts on uid); engine.md says uids differ from sort_ids.
5. **Float and stats.** mean/stdev(sample)/round(x,3), result key order. Test on 1k random vectors vs Python fixtures.
6. **Wall-clock budget.** Non-deterministic cut-off; see above.
7. **STRICT_ABORT path.** Python fallback games not flagged in traces. Test: lab arms with JEV_NATIVE=0 vs new on 28 seeds (ask the user first).

## 4. Task split (dependency order)

Fixtures: small Python dump scripts in `scripts/port/`, files under `native/bot/tests/fixtures/forecast/`.

1. **simcore extraction (Sonnet; mechanical, ~0 new Py).** Move non-PyO3 lib.rs code to `native/simcore`. Done: both crates build; rollout parity
   script and the 56/56 identity check (ask the user) unchanged.
2. **Stats and assembly (Haiku; ~100 Py: summarize, paired, m_*, both `options`, `option_jokers`, forecast result assembly).** Done: with recorded
   per-option rows as input, Rust `ret.options`/`vs_skip` equal recorded exactly; `options(pub)` keys/desc/cost/unsupported equal on all calls.
3. **Picker (Sonnet; ~130 Py: Picker.*, synth_plays, usable, card/consumable values; add MT19937 `sample` if pycompat lacks it).** Done: every
   Picker.values and pick_pub oracle line equal (floats via repr).
4. **state_from_pub, non-joker part (Sonnet; ~200 Py: build_state minus jokers, playing_card, item_card, describe field mapping).** Done: field-by-field
   equality with Python `describe(build_state(pub))` (excluding jokers/rng) for every forecast pub.
5. **Joker running state (Sonnet; ~250 Py: joker_card, _joker_dynamic, joker_fields composed).** Done: per-joker fixtures equal for all keys in
   traces + edge text variants; jokers part of state_from_pub equal on all forecast pubs.
6. **Option application (Sonnet; ~150 Py: apply_option buy/sell/voucher/planet, RNG-first order).** Done: post-option fixtures equal (risk 3).
7. **rollout.forecast end to end (Sonnet; ~100 Py glue: run loop, common random numbers, chunked budget).** Done: every rollout.forecast oracle
   line reproduces `ret` (minus `seconds`, `_runs`) on all 60 recorded games; mismatches classified by option type.
8. **Pack opening + open_and_pick (Sonnet; ~150 Py; may need engine fixes from engine.md).** Done: pack-content fixtures (risk 1) and per-option
   `taken` lists equal.
9. **rollout_items.forecast end to end (Sonnet; ~150 Py: options, shop_boosters, items horizon, policy cache).** Done: all rollout_items.forecast
   oracle lines equal, zero STRICT_ABORT.
10. **Wiring (Haiku; ~50 Py).** jev_bot calls the Rust forecasts via iface; full-game trace replay shows all oracle calls matching; no `jackdaw`/
    `jev_sim` on the bot path.

Haiku: 2 and 10 (mechanical, tight fixtures). Sonnet: the rest (RNG/float parity judgement).

### Task 8 status (fc-packs)

`forecast_items.rs` (buy_pack/buy_card, ported Picker over the engine views) is identical to Python on
`fc_packs.py`: 2552/2552 pack contents, 2552/2552 picks, 2552/2552 lite post-option states (money, joker and
consumable names, card count, slots, all RNG pool values). Jackdaw quirks mirrored via GameState flags set only
by `apply_item_option`: `pack_ignores_played` (OpenBooster reads an unsynced `played_hand_types`/`used_jokers`, so
Planet X/Ceres/Eris and owned centres are not filtered and the Soul/Black Hole rolls never skip),
`fool_always_usable` (PickPackCard skips can_use: Emperor/High Priestess/Fool with no room create nothing; The Fool
with no last card draws from `Tarot_Planet`+`fool`). Rumbot bug fixed: a Spectral Soul hit no longer skips the Black
Hole roll (common_events.lua:2088-2100). NOT done: the lite comparison is not yet the full
`sim_bridge.describe(gs, rng=True)`; that needs a GameState -> StateDesc inverse (none exists in simcore).

### Task fc-items2 status
- `RustOracle::items_forecast` now calls `forecast_items::items_forecast`; `CheckedOracle` checks it against the recording (rollout count taken from the recording when a time budget was used). `JEV_REPLAY_ARM=E|F` selects one arm in `bot_replay`.
- Arm F computed-oracle replay (`JEV_REPLAY_ORACLE=rust JEV_REPLAY_ARM=F`, ~3.6 min): 18/30 identical; the 12 others stop at an items_forecast rollout-level mismatch (the known 10 classes). Arm E not rerun (no items forecast in E).
- Debug aids: `FC_ONLY=<idx>` (fc_items: one call, prints first differing option), `FC_ROWS=1` (forecast_items prints per-rollout rows on stderr).
- Finding for MZ5KZ3JH idx 2391 (`skip`, k=3, Brainstorm/Banner/Bootstraps/Supernova/Drunkard): Python's row is Jackdaw's, not rumbot's. Rust aborts (`refresh_policy`, Brainstorm in policy, Amber Acorn reshuffle at ante 8 boss), so `run_strict` returns None and Python replays the WHOLE horizon on Jackdaw. Rust (rumbot semantics) and Jackdaw agree through ante 7 Big (65752) but differ on the first hand of the ante 7 boss (18473 Jackdaw vs 19227 rumbot, Pair of Aces); Jackdaw shows money $55 at that hand vs $52 at the shop before it, so suspect a money/Bootstraps (or reward) difference between engines. Rollouts where Python's Rust path aborts are Jackdaw results; to match them exactly the Rust side needs the Jackdaw behaviour for that case (or an engine fix verified in the Lua dump). Not yet fixed. The Arcana/Spectral/Celestial classes were not examined.
