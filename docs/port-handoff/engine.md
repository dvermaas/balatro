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

## Replay results (round engine-r1)
Run with `IGNORE="].id"` (card ids are Jackdaw sort_ids vs rumbot uids: arbitrary labels, never used as handles by the traces; allow-listed via IGNORE, not in code).
Calls matched before first divergence, per game over the 60 recorded games (30 seeds x 2 arms):
- before: min 0 / median 0 / max 0 (all diverged at call 0 on `.round.hands_left`)
- after:  min 6 / median 22 / max 60 (1476 calls total; 0/60 games identical yet)
Fixes this round (all view/action mapping):
1. round.hands_left/discards_left/discards_used and round.chips: Python reports 0 until the first blind is selected, then keeps the last round's values through ROUND_EVAL/SHOP/BLIND_SELECT; rumbot resets them (eng.round_started/last_round/last_chips snapshot in actions.rs).
2. boss blind name: `ante_boss` holds a key, use `GameState::_pick_boss()`.
3. discard: rumbot's `is_legal` wants ascending card indices; sort for discards (play keeps scoring order).
4. consumable cards (shop/pack/area) carry `modifier.enhancement` from the center effect ("HAND UPGRADE", "SUIT CONVERSION", ...), like jokers.
Remaining first-divergence classes (60 games):
- 28 `.packs.cards[].key`: Jackdaw `p_buffoon_normal_1` vs rumbot `_2` (variant number; likely rumbot lacks variant info -> view bug or derive from slot/RNG; check shop pack variants in Jackdaw).
- 23 `.blinds.small.tag_name` + 2 `.blinds.big.tag_name`: different tags (e.g. seed 11BLBC1T call 60: python "Top-up Tag" rust "Economy Tag"; 61QF3Z7R call 42: "Orbital Tag" vs "Investment Tag"; AEQ18EUN call 40 big: "Uncommon Tag" vs "Skip Tag"). Probably engine difference in tag RNG/roll timing (not fixed, per task); verify whether rumbot rolls tags at a different point.
- 3 `.pack.limit`: python=2 rust=4 (seed 665WDGX8 call 28 `buy {"pack":1}`): pack choose/limit mapping for a Mega/other pack size, view bug probably.
- 2 `.hand.cards[].key` (68RZS4XR call 22 after `use {"consumable":0,...}`: python S_A, rust C_5): tarot effect/hand order, probably engine difference.
- 2 `.shop.cards[].label`: "Riff-raff" vs "Riff-Raff" (cosmetic naming; map via joker name table or allow-list `.label` case).

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
1. Fix pack key variant (28), pack.limit, Riff-raff label; then investigate tag_name (engine?) and the S_A/C_5 tarot case; keep peeling.
2. Verify error behaviour against Jackdaw directly (small Python script), since traces have no errors.
3. Phase 2: port sim_text.
