# engine (phase 1) handoff

Branch port/engine. Files: native/bot/src/engine/{mod.rs,actions.rs,view.rs,tables.json}, native/bot/tests/engine_replay.rs,
scripts/port/engine_tables.py (makes tables.json from Jackdaw data: hand examples, joker center "effect", enh labels, tag names), `pub mod engine;` in lib.rs.

## Done
- `RumbotGame` implements `Game`. All methods the traces use are mapped: start, gamestate, menu, health, select, skip, play, discard,
  buy{card|voucher|pack}, sell{joker|consumable}, use{consumable,cards}, reroll, cash_out, next_round, pack{card,targets|skip}, rearrange{hand|jokers|consumables}.
  Error codes/messages follow Jackdaw's IllegalActionError strings and sim_backend._rearrange; phase-mismatch message text is a guess
  (traces contain no errors at all, so error behaviour is untested).
- Cerulean Bell forced card (_with_forced) ported. played_this_round counts and round hands_played are tracked in the wrapper (rumbot keeps only a set / run total).
- View: serialize/serialize_card/_area/_hands/_blinds ported with the same key order; shop-frozen areas; pack area. Compiles.
- Replay harness: `cargo test --test engine_replay -- --nocapture` (env ARMS, LIMIT, IGNORE=comma path substrings with [] for indices, STRICT=1).

## Replay results so far
0/60 games identical; 0 calls matched: every game diverges at call 0 (start): `.round.hands_left` python=0 rust=4
(Python reports 0 hands/discards outside a round; rumbot keeps allowance at 4/3). Fix: in view, report hands_left/discards_left/discards_used = 0
unless phase is Playing/GameOver (check what Python shows in ROUND_EVAL/SHOP). Then iterate with the harness (use IGNORE to peel classes).
Not yet seen beyond call 0.

## Expected divergence classes (unverified guesses, check)
- card ids: Jackdaw sort_id vs rumbot uid (counter start/offset); shop vouchers, packs and pack consumables have NO id in rumbot (emitted as 0).
- blind status in ROUND_EVAL/SHOP/pack (rule derived from trace stats; depends on when rumbot advances blind_index/ante); boss name via ante_boss (name vs key?).
- reroll_cost outside shop, round.chips after round, hand ordering after draw, Won phase mapping (no wins in arm F), joker buy cost for rental/edition.
- engine differences (Jackdaw vs rumbot): to classify once the view diffs are gone.

## Text fields (sim_text, phase 2; emitted as "")
- every `value.effect` (playing cards, jokers, consumables, vouchers, boosters, pack cards, shop cards): sim_text.card_effect
- `blinds.{small,big,boss}.effect`: sim_text.blind_effect (boss key needed: rumbot has boss name; map name->key via tables "blinds")
- `blinds.*.tag_effect`: sim_text.tag_effect (needs tag key from ante_tag_keys, per-tag vars: hands played, unused discards, skips)
- static tables already extracted (no localization needed): hand `example`, joker modifier.enhancement (center effect), enhancement labels, tag names.
- Card/consumable/voucher/pack labels use rumbot names (not sim_text); verify in replay.

## Next steps
1. Fix hands_left/discards zeros, rerun, peel divergences one class at a time; classify view bug vs engine difference.
2. Verify error behaviour against Jackdaw directly (small Python script), since traces have no errors.
3. Phase 2: port sim_text.
