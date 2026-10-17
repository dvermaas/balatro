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

## Replay results (round engine-r2)
Calls matched before first divergence per game (60 games, IGNORE="].id"): before min 6 / median 22 / max 60 (1476 calls); after min 14 / median 230 / max 429 (12632 calls); 38/60 games identical.
Fixes (all view/action, no engine change):
1. tags/boss: Jackdaw rolls the next ante's tags and boss when the boss falls (state ROUND_EVAL); rumbot rolls at cash_out. view.rs blinds() rolls on a `deep_clone` to preview them (keyed streams, so identical to the real roll; proved by 0 tag/boss diffs afterwards). Not an engine difference.
2. pack.limit: Jackdaw gs["pack_size"] is a constant 2 (run_init), view now emits 2.
3. debuff: Jackdaw clears card debuffs when the blind ends; view emits debuff only while a blind is active.
4. round.reroll_cost: Python keeps the shop's last cost after leaving the shop and resets to 5-discounts at round start (phase Playing); `eng.last_reroll`.
5. first-shop Buffoon pack: Jackdaw shop.py:180 always deals p_buffoon_normal_1 in the first shop's first slot (rumbot rolls math.random(1,2) like the game). REAL ENGINE DIFFERENCE (Jackdaw simplification, harmless: _1/_2 identical packs); view maps slot 0 of the first shop to _1 until it is bought (`eng.first_shop`, `first_pack_gone`).
6. labels: "Riff-Raff" -> "Riff-raff", "Séance" -> "Seance" (card_json).
Remaining classes (22 games): 6 `.round.discards_left` (python +1 after buying a joker in the shop, e.g. 1SHJTVG7@56 buy card 0: py 4 rust 3; Python updates round snapshot on joker buy, likely Drunkard); 6 `.cards.cards[].key` + 5 `.hand.cards[].key` (card identity after tarot use/pack pick, e.g. 68RZS4XR@22 use consumable 0 cards [7,6,5]: py S_A rust C_5; 6CLKMFPY@88 pack card 0: py H_K rust H_7: likely engine/RNG difference in tarot effect or pick, not yet classified); 2 `.round.chips` (KGA553W7@375 py 12444 rust 13542); 2 reroll_cost (UDBU5JA1@143 select: py 0 rust 5, free reroll/Chaos Clown); 1 action error (11BLBC1T@121 pack card 3 targets [3,0]: rust refuses).

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

## Replay results (round engine-r3)
Run with IGNORE="].id". Before: 38/60 games identical (12632 calls matched before first divergence). After: 48/60 identical (15119 of 15131 replayed calls).
Fixes (view/action mapping, no engine change):
1. discards_left snapshot outside Playing: Jackdaw card.py:762-775/835-844 (card.lua:591 ease_discard) adds/removes d_size (Drunkard 1, Merry Andy 3) to current_round.discards_left on buy/sell; actions.rs `act` wrapper mirrors the delta into eng.last_round.1 (only when neither phase is Playing).
2. Chaos the Clown: at round start (Playing) reroll_cost is 0 (free reroll), view.rs.
3. Consumable `use` targets and `pack` targets are hand positions, order-free in Jackdaw (consumables.py _death uses max index); rumbot is_legal only accepts ascending subsets and applies effects in given order, so targets are sorted ascending. This also fixed all 11 "wrong card key after tarot use / rejected pack targets" cases (68RZS4XR@22, 11BLBC1T@121, MZ5KZ3JH@251, ...).
Remaining (12 games, all genuine engine differences or unclassified):
- ENGINE DIFF, Jackdaw wrong, rumbot right (7 games; 6CLKMFPY@88, F1PE5WEY@42, QNCJLKQ3@28, 1YJ4DQ2R@330, SXYS1LUB@52, MZ5KZ3JH@80): `.cards.cards[0].key`. A playing card taken from a Standard pack goes to the draw pile: Jackdaw appends it at the END (next draw), the game inserts at index 1 = bottom (CardArea:emplace, cardarea.lua:49-51 `self.config.type == 'deck'` -> table.insert(cards,1,card); button_callbacks.lua:2267 G.deck:emplace). Rumbot does the bottom insert (draws via pop from the end). Visible until the next blind shuffle, and changes the shuffle input.
- ENGINE DIFF, Jackdaw wrong, rumbot right (2 games; 1SHJTVG7@143 items+long): round.chips py 3834 rust 4792 (= 142 * 27 vs 142 * 27 * 1.25). Marble Joker's Stone card must fire playing_card_added so Hologram gains X0.25 (card.lua:2996 playing_card_joker_effects({card})); Jackdaw jokers.py:1997 _marble does not fire it.
- ENGINE DIFF, Jackdaw right, rumbot wrong (1 game; R8ZXJN6L@342 bot-forecast-long): jokers.count py 4 rust 5 on the losing play (GAME_OVER). Popcorn (mult 4, extra 4) is eaten by the end_of_round context even when game_over is true (card.lua:3349, no game_over guard; state_events.lua:107 calls end_of_round before the game_over check); rumbot skips end_of_round joker effects on a loss. Harmless for play (game ends) but visible in the view.
- UNCLASSIFIED (KGA553W7@375, items+long): round.chips py 12444 rust 13542 on a Two Pair play under Crimson Heart (python had Abstract Joker debuffed going in, view matched before the play; jokers Abstract, Swashbuckler, Runner, Bootstraps, Blackboard; $61). Rust scores ~1098 more; needs a per-joker score trace (candidates: Swashbuckler/Abstract counting a debuffed joker, Bootstraps rounding, Blackboard held-card check).

## Replay results (round engine-r4)
49/60 games identical (15122 of 15133 calls). Fixes in native/rumbot (logged in VENDORED.md):
1. Popcorn (and every end_of_round joker) now runs on the losing play too: `_lose_round` runs the round_end hooks when Mr. Bones does not save (state_events.lua:107, card.lua:3283-3349, no game_over guard). R8ZXJN6L now identical.
2. KGA553W7@375: no rumbot change. Per-joker trace (tests/trace_one.rs, env SEED/UPTO/ARM) gives 122 chips x 111 (rust) vs 122 x 102 (python): Swashbuckler 11 vs 8. card.lua:4609-4617 sums sell_cost of all other jokers without a debuff test, so rumbot is right; Jackdaw jokers.py `_swashbuckler` skips debuffed jokers (ENGINE DIFF, Jackdaw wrong; 1SHJTVG7@143 Marble/Hologram is the other round.chips class).
Remaining 11 games: 7 draw-pile insertion (Jackdaw), 4 round.chips (2 Jackdaw Marble/Hologram, 2 Jackdaw Swashbuckler-debuff).
Guard: re-recorded 6 sim30-a games with the rebuilt jev-sim; identical to runs/_port/traces/bot-forecast-items ignoring timing keys, except the final record's prompt_version (hash includes rumbot). Forecasts unchanged.

