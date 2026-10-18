//! Port of src/prompts.py (every string JEV sees: byte-identical). See docs/rust-port-guide.md.
//!
//! Python implicit string concatenation becomes `concat!` over the same pieces. Dicts keep their
//! insertion order as `&[(key, value)]` slices; `pairs_json` gives the JSON object (order kept).

use serde_json::{Map, Value};

// prompts.py 8-81: the note of the last Python version (v32)
macro_rules! py_note {
    () => {
        concat!(
            "v32: no behaviour change. Merge of main's full Rust bot (native/bot, frozen at v27) into v31: ",
            "native/sim now builds on native/simcore, which changes the version hash. sim60-a code-only lab: ",
            "60/60 games identical to v31. Based on: ",
            "v31: the forecasts' auto-buy bar drops from 2 to 1 paired SE over buying nothing (decisions.SURE_SE, ",
            "rollout_items.best). Small sure gains (packs, swaps, planets) rarely cleared 2 SE at 16-24 rollouts. ",
            "Lab, code only, pooled 320 paired seeds: +0.23 +- 0.10 ante, wins 60 vs 50 (1.5 SE: +0.09; 0.5 SE: ",
            "+0.14 on 200). Verdant Leaf sales skip hand-size and life-saving jokers (Troubadour, Juggler, ...). ",
            "Based on: ",
            "v30: Verdant Leaf. The bot sells its weakest joker (smallest loss on recent hands) before playing ",
            "when Verdant Leaf debuffs the hand (decisions._leaf_sale), and the sim lifts Verdant Leaf when a ",
            "joker is sold (sim_patches.verdant_leaf_lifts_on_joker_sale; rumbot-sim already did). New sim ",
            "baseline. v28 lab: 13 of 200 games died to Verdant Leaf at ante 8. Based on: ",
            "v29: the shop offers Negative jokers and consumables with full slots (a Negative card brings its ",
            "own slot; rollout.options already allowed it, decisions.shop's filter dropped it). Based on: ",
            "v28: planets and joker text. (1) The shop buys the planet for the hand our deck scores most with ",
            "(readiness.play_mix) and the biggest Celestial pack while $4 stays (decisions._planet_rule); a ",
            "pack's planet is picked by rollout_items.planet_gain, which re-chooses the best play after the ",
            "level-up on deals of hand size + 3 (Rust kernel), instead of jokers.gain with fixed plays, which ",
            "spread levels over Pair, High Card and ties. v27 levelled only ~3 hands by ante 6. (2) Joker text ",
            "fixes, in Python and the Rust kernel: The Idol needs its rank and suit (was every card of the ",
            "suit), Bloodstone at its expected X, Driver's License's count is not +Mult, Seeing Double and ",
            "Baseball Card (rarity from Jackdaw's centers) are modelled. Based on: ",
            "v27: item-forecast rollouts keep their policy when jokers are only removed (Madness, Ceremonial ",
            "Dagger, Popcorn or Gros Michel running out): the removed jokers' entries are dropped instead of ",
            "rebuilding the policy from the jokers' current text. Rust does the same, so Rust gives up on 1% ",
            "of item rollouts instead of 19%. A small behaviour change in rollouts only: lab E+F on 28 seeds, ",
            "1 game of 28 changed per arm (-0.11 +- 0.11 ante, noise). 56/56 identical to JEV_NATIVE=0; ",
            "683 s -> 60 s. Based on: ",
            "v26: speed only, same decisions as v25 on the new sim baseline. Item rollouts run in Rust from ",
            "their first shop, including the bot's tarot/spectral use (use_items and tarots.plan ported, with ",
            "the tarot table and build suit passed in). Rust gives up only when the jokers change. The Fool's ",
            "last_tarot_planet is passed. The joker forecast's per-seed path now carries RNG streams and ",
            "compares RNG state, not pickled bytes. Sim fixes (new sim baseline, real-replay parity still ",
            "329/336): a debuffed joker loses its passive (Stuntman hand size); SMODS fills grown hand slots ",
            "at once; rumbot's Crimson Heart follows SMODS. Lab E+F on 28 seeds: 56/56 identical to ",
            "JEV_NATIVE=0, 677 s -> 70 s. Based on: ",
            "v25: speed only, same decisions as v24. The pack/tarot/spectral forecast's rollouts (rollout_items) ",
            "hand off to rumbot-sim at the first shop holding no consumables, with Jackdaw's pseudoseed streams ",
            "carried over (sim_bridge.run_strict, jev_sim.rollout_strict). Rust gives up (and Python carries on) ",
            "when the bot would use a tarot or the jokers change at a blind select (Madness, Riff-raff, Popcorn): ",
            "about 11% of handoffs. Lab arms E+F on 28 seeds: 56/56 identical to JEV_NATIVE=0, 670 s -> 83 s ",
            "(8.1x). Sim also fixed: Smeared Joker applies to every suit check. Based on: ",
            "v24 draft: speed only, same decisions as v23, in two parts. (1) jokers.score over every play subset (hands.evaluated_plays, best_score, best_play: odds forecasts, readiness, setup plays) and rollout.Policy's candidates run in a Rust kernel (native/, src/native.py); joker rules are data tuples instead of lambdas so both evaluate the same rule; every subset score identical (tests/test_native.py). (2) The shop forecast's rollouts (rollout.forecast) run on rumbot-sim in Rust (native/sim, src/sim_bridge.py): each option's Jackdaw state is converted once and rollout.Policy/play_horizon run in Rust, rows identical to the Python loop (150/150 jokers, 10,764 rollouts; 123/123 forecast options). Python stays the fallback (JEV_NATIVE=0 or modules not built). Pack/tarot forecasts (rollout_items) stay in Python. Based on: ",
            "v23: early joker slots are filled by code. Through ante 3 with fewer than 4 jokers, when no shop ",
            "option clears the 2 SE bar, code buys the joker the forecast rates best above buying nothing ",
            "(any positive margin). On the 4 seeds where JEV v20 beat the code-only bot most (XYK41G5M, ",
            "68RZS4XR, R8ZXJN6L, 54ATPSF9), the code-only bot reached ante 2 with 0-2 jokers and died, ",
            "while JEV filled slots and went to ante 6-9. Based on: ",
            "v22: speed only, same decisions as v21 (per-seed results of the lab's arms E and F identical on ",
            "28 seeds): rollout copies by unpickling one snapshot instead of deepcopy; cash-outs inside forecast ",
            "rollouts skip shop population; hands.classify memoised; tarots.auto_use builds the build plan only ",
            "when a targeted card needs it. Based on: ",
            "v21 draft: the policy lab's arm F in production (+0.44 +- 0.12 ante over v20's code path, 303 paired ",
            "seeds). Packs, shop tarots and shop spectrals get their own rollout forecast (rollout_items.py: packs ",
            "opened with sampled contents and the Picker rule, tarots used on tarots.plan targets) in what is left ",
            "of the 4 s shop budget; one that beats buying nothing by > 2 SE is bought by code after any joker ",
            "auto-buy, and JEV sees the same forecast fields on every pack/tarot/spectral. In an open pack the ",
            "Picker's card is taken by code when its value is >= 1.25x the runner-up's, otherwise JEV rates the ",
            "cards with a pick_value field. Forecast exceptions are logged with tracebacks to ",
            "runs/_forecast_errors.log; fixed a KeyError in the forecast state when Fortune Teller is owned. Based on: ",
            "v20 draft: the policy lab's arm C in production. Forecast horizon 3 bosses (n=24); a forecast ",
            "option that beats buying nothing by > 2 SE in blinds cleared is bought by code (recorded as an ",
            "'auto' shop event with its numbers), without a JEV question; JEV still rates close calls, packs, ",
            "tarots, spectrals and economy items with the forecast fields (survive_2_bosses and ",
            "blinds_cleared_next_6 renamed survive_horizon and blinds_cleared_horizon); reroll rule kept; rollout guard for ",
            "an empty deck and hand. Based on: ",
            "v19 draft: rollout forecast in the shop (rollout.py). Each joker, planet, voucher and swap is ",
            "simulated through the next 2 bosses with the build frozen (32 rollouts, common random numbers); ",
            "JEV sees next_boss_clear vs skip (paired difference and SE), survive_2_bosses, boss_margin and ",
            "blinds_cleared_next_6 instead of measured_gain/readiness_after. Code: sure buy at > 2 SE over ",
            "skip, hide items > 2 SE worse (unless economic), reroll when nothing beats skip by 1 SE, swaps ",
            "only sell the 2 weakest jokers; v18 path when the forecast fails. Packs/tarots unchanged."
        )
    };
}

/// What changed in this version, newest first; each note ends "Based on: " + the one before.
pub const VERSION_NOTE: &str = concat!(
    "v45 (Rust only): blind skips. At antes 1-3 the bot skips a Small or Big blind whose tag is Investment, ",
    "Negative or Rare (a code rule; the lab's top-option JEV never skipped). Fresh 2,000 seeds vs v44: 428 vs 359 ",
    "wins (z = 4.46); ante-1 deaths 36 -> 80, fewer at every later ante. Skipping for 11 tags: 369 vs 359, mean ante ",
    "-0.72. Based on: ",
    "v44 (Rust only): Hieroglyph and Petroglyph are never bought. -1 ante makes the forecast's 3-boss horizon of ",
    "blinds cleared easier, but a win needs that ante again with a hand (a discard) fewer every round. 2,000 ",
    "seeds vs v43: 367 vs 347 wins (z = 1.77), +0.023 +- 0.018 ante; the 605 games that bought Hieroglyph won ",
    "135 instead of 115. Based on: ",
    "v43 (Rust only): Raised Fist. The scorer took the lowest held card's value with Stone cards as 0 (so any ",
    "held Stone zeroed it) and ignored Mime and the card's Red Seal; it now mirrors the game: the rightmost ",
    "lowest-ranked non-Stone card, nothing when it is debuffed, repeated per held retrigger and Red Seal. ",
    "Based on: ",
    "v42 (Rust only): Misprint. Its exported effect is only \"+\" (random dynatext), so the scorer counted it ",
    "as 0 and 37% of plays holding it scored over 1.5x their estimate; it now counts its average +11.5 Mult. ",
    "Based on: ",
    "v41 (Rust only): The Mouth's first play. It locks the round's hand type, so the bot values each type in ",
    "its plays as its best play now + (hands left - 1) x its expected best score in a fresh deal from the deck ",
    "(16 order-free samples) and leads with the best. 2,000 seeds vs v40: +0.019 +- 0.007 ante, 356 vs 346 ",
    "wins; Mouth deaths 87 -> 70. Based on: ",
    "v40 (Rust only): The Mouth. It scores only the round's first hand type, but every type played (even one ",
    "that scored 0) counts as played this round, and the bot allowed any played type: after one off-type play it ",
    "kept playing that type for 0 and called it a sure clear. The bot now remembers the first type; the ",
    "rollouts use the game's lock. Based on: ",
    "v39 (Rust only): The Serpent. It tops the hand up with exactly 3 cards after any play or discard, ",
    "whatever room there is, so until a play clears the bot (and the forecast's rollouts) discards the lowest ",
    "card outside the best play, one at a time: each discard grows the hand by 2. Based on: ",
    "v38 (Rust only): Verdant Leaf bait. In the shop before Verdant Leaf, with a joker slot free, the bot buys ",
    "the cheapest sellable joker and sells it when the boss starts, instead of selling its weakest real joker ",
    "(Leaf deaths at ante 8 were near misses: 95-98% of the target). Based on: ",
    "v37 (Rust only): joker text. A money joker's \"(Currently $4)\" (Cloud 9) was read as +4 Mult, and ",
    "Seltzer's \"for the next 10 hands\" as retrigger 10s; now money is not scored and Seltzer retriggers every ",
    "played card. Found by auditing 6,923 plays' estimates against their scores. Based on: ",
    "v36 (Rust only): Cerulean Bell. The game always adds the boss's forced card to a play (cutting it to 5 ",
    "cards); the bot did not know which card it was, so a planned Straight could lose a card and score 3% of ",
    "its estimate (27% of Cerulean Bell plays scored under 2/3 of the estimate; 103 of 5,000 lab games died to ",
    "it at ante 8). The view marks the card (state.forced) and every play the bot considers contains it. ",
    "Based on: ",
    "v35 (Rust only): no behaviour change. Lab-only experiment flags, off unless JEVLAB_EXP is set ",
    "(experiment.rs: Pair commitment, free discards for discard jokers in the bot and in simcore's rollouts; value_model.rs: a learned P(win) as the forecast rollouts' tail ",
    "value), and jevlab JEVLAB_SNAPS shop snapshots. Based on: ",
    "v34: speed only, same decisions as v33. Planet values score their deals once for every planet ",
    "(a level changes only its own hand's plays); best_score/best_play skip plays with a kicker when a ",
    "kicker cannot matter; rumbot formats log lines only when kept and uses bitsets for played hands; ",
    "the pack-pick view leaves out text the Picker does not read; forecasts build their start state once. ",
    "Lab E+F, 30 seeds: every game identical to v33 (call logs and decisions); CPU time on 16 seeds, ",
    "4 threads: 71.7 s -> 31.4 s (E), 90.6 s -> 37.6 s (F). Based on: ",
    "v33 (Rust only): speed only, same decisions as v32. The hand scorer kernel no longer allocates per ",
    "play subset and walks flat rule lists; mimalloc is the allocator; rumbot's hand tables are arrays and ",
    "its read-only joker scans no longer copy the row. The play-mix cache is per game (it was process-wide, ",
    "so games run on threads could depend on each other). Lab E+F, 30 seeds: every game identical (call ",
    "logs and decisions); 16 seeds, 4 threads: 37.0 s -> 20.6 s (E), 54.0 s -> 27.2 s (F). The Python code ",
    "is removed. Based on: ",
    py_note!(),
);

/// The v32 note, as prompts.py had it (the Python fixtures compare it).
pub const PY_NOTE: &str = py_note!();

// Short factual brief, sent with every question as `game_rules`. Strategy lives in the questions.
// prompts.py 47-51
pub const GAME_RULES: &str = concat!(
    "Balatro: reach each blind's chip target before hands run out. A hand scores chips x mult; jokers add ",
    "chips or mult, or multiply mult (xMult). Discards are free but limited. Interest: $1 per $5 held at ",
    "round end, max $5 (at $25)."
);

// prompts.py 53-94
pub const INSTRUCTIONS: &[(&str, &str)] = &[
    ("blind", "Should we play or skip the upcoming blind?"),
    (
        "play_now",
        concat!(
            "Should we play our strongest hand (`strongest_play`) now instead of discarding first to build a ",
            "better one? `best_draw_by_measurement` compares the best draw plan with playing now: its average ",
            "includes misses, and poor_draw_next_play shows downside. Weigh the measured improvement and ",
            "chance of meeting pace against spending one of our remaining discards. Preserve discards when ",
            "we are on pace and the improvement is small. Survival this round comes before build preference."
        ),
    ),
    (
        "best_play",
        concat!(
            "Which play is best this turn? `estimated_score` already includes our jokers (except any in ",
            "`jokers_not_in_estimates`), but are approximate. Prefer the higher score unless a ",
            "lower one keeps stronger cards or avoids resetting a scaling joker."
        ),
    ),
    (
        "setup_play",
        concat!(
            " When `next_hand_outlook` ",
            "is present, no discards remain but another hand does: compare the supplied two-play totals ",
            "and rankings, not just immediate score. A setup play spends a hand to preserve a promising draw. ",
            "Weigh poor-draw totals and chance_two_plays_clear as well as the average; a high average alone ",
            "does not guarantee survival. The forecast assumes our best modeled next play and includes ",
            "misses; it does not plan the whole round. Choose for survival rather than build preference."
        ),
    ),
    (
        "best_discard",
        concat!(
            "If we discard, which plan best helps us survive this round? Options are ordered by measured ",
            "survival value. Each models our best next play after the draw, including misses and cards held ",
            "for joker effects. Prefer better average score and downside; with one hand left, prefer the ",
            "chance to clear. These are sampled scoring estimates, so account for unmodeled joker effects. ",
            "Use build fit only to break close choices, never to justify a clearly worse survival outlook."
        ),
    ),
    (
        "item_value",
        concat!(
            "How much would taking the item in `item` (described in this question) strengthen this run? ",
            "`next_boss_clear`, `survive_horizon`, `boss_margin` and `blinds_cleared_horizon` simulate the ",
            "next blinds with this item and the build otherwise frozen (see `forecast`): trust them for scoring ",
            "power and add what they cannot see (money, later purchases). `measured_gain`, when shown instead, ",
            "is its measured effect on the scores of our recent hands, jokers included: trust it ",
            "for the immediate effect and add what it cannot see (scaling over the run, money, risk, future ",
            "hands). `use` says what a tarot or spectral would do for us; `worth` says what a voucher gives for ",
            "the rest of the run. Favour items that fit `our_build`. Consider `engine.missing`, `upcoming_boss` ",
            "and what it costs. `readiness` compares our typical hand with what the next blinds need per hand ",
            "and `plan` says whether to spend on power or save. ",
            "With little scoring support, prefer reliable Mult and useful temporary jokers over an xMult ",
            "effect whose activation conditions are not met. `known_effect` restores missing card text. ",
            "For a free pack pick, follow `pack_selection`; do not charge its pack price again."
        ),
    ),
];

// prompts.py 96
pub const CASH_ITEM_QUESTION: &str = "How valuable is taking this cash tarot in this situation? Use `cash_outcome`, which already subtracts the price before calculating the payout. In a shop, buying and using it can fund another purchase in the SAME shop; compare net profit, not just the temporary purchase cost. A substantial immediate net cash profit is a strong economy improvement even though it adds no chips directly and fits any hand type. A loss or zero profit is not an immediate cash upgrade. In a paid pack, the money competes with the other free picks: prefer immediate scoring if needed to survive, otherwise consider how the cash funds future upgrades.";

// prompts.py 98
pub const CASH_STEP_QUESTION: &str = "Which action should we take before continuing this shop? Each cash-tarot option buys and immediately uses that card, then returns to this SAME shop with the stated net cash profit. Other items remain available; no blind is played in between. Prefer useful immediate profit that funds our next purchases. Choose continue_shopping if there is a concrete reason to pass these cash transactions. The numeric cash outcomes already include the purchase price.";

// prompts.py 100-106
pub const ITEM_VALUE_LEVELS: &[&str] = &[
    "Hurts the run or is wasted (off-plan, useless now, or not worth the money)",
    "Marginal: small or situational benefit",
    "Useful: a solid upgrade for this build",
    "Strong: clearly improves scoring or economy for this run",
    "Core: a key engine piece, e.g. an xMult or scaling joker that fits our main hands",
];

// prompts.py 108-111
pub const PLAY_NOW_CRITERIA: &[(&str, &str)] = &[
    ("true", "Play the strongest hand now"),
    ("false", "Discard first to build a stronger hand"),
];

// prompts.py 113-123
pub const OPTIONS: &[(&str, &str)] = &[
    ("blind_play", "Play the {name} (need {score} chips) and earn its cash reward"),
    (
        "blind_skip",
        concat!(
            "Skip the {name} to gain the {tag_name}: {tag_effect}. ",
            "Skipping forgoes the blind's money and shop visit."
        ),
    ),
    ("discard_plan", "{label}"),
];

/// A `&[(key, value)]` table as a JSON object, keys in table order.
pub fn pairs_json(pairs: &[(&str, &str)]) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), Value::String(v.to_string()));
    }
    Value::Object(m)
}
