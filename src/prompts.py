"""Everything Jev reads, in one place. Editing this file (or hands.py, strategy.py, decisions.py) creates
a new prompt version. VERSION_NOTE is shown on the Prompts page; update it to describe what changed.

v6 follows TypeSafe's guidance: narrow questions batched in one call, decision-specific state, guidance
in the questions instead of the state, structured option fields, and code combining the answers.
"""

VERSION_NOTE = (
    "v24 draft: speed only, same decisions as v23, in two parts. (1) jokers.score over every play subset (hands.evaluated_plays, best_score, best_play: odds forecasts, readiness, setup plays) and rollout.Policy's candidates run in a Rust kernel (native/, src/native.py); joker rules are data tuples instead of lambdas so both evaluate the same rule; every subset score identical (tests/test_native.py). (2) The shop forecast's rollouts (rollout.forecast) run on rumbot-sim in Rust (native/sim, src/sim_bridge.py): each option's Jackdaw state is converted once and rollout.Policy/play_horizon run in Rust, rows identical to the Python loop (150/150 jokers, 10,764 rollouts; 123/123 forecast options). Python stays the fallback (JEV_NATIVE=0 or modules not built). Pack/tarot forecasts (rollout_items) stay in Python. Based on: "
    "v23: early joker slots are filled by code. Through ante 3 with fewer than 4 jokers, when no shop "
    "option clears the 2 SE bar, code buys the joker the forecast rates best above buying nothing "
    "(any positive margin). On the 4 seeds where JEV v20 beat the code-only bot most (XYK41G5M, "
    "68RZS4XR, R8ZXJN6L, 54ATPSF9), the code-only bot reached ante 2 with 0-2 jokers and died, "
    "while JEV filled slots and went to ante 6-9. Based on: "
    "v22: speed only, same decisions as v21 (per-seed results of the lab's arms E and F identical on "
    "28 seeds): rollout copies by unpickling one snapshot instead of deepcopy; cash-outs inside forecast "
    "rollouts skip shop population; hands.classify memoised; tarots.auto_use builds the build plan only "
    "when a targeted card needs it. Based on: "
    "v21 draft: the policy lab's arm F in production (+0.44 +- 0.12 ante over v20's code path, 303 paired "
    "seeds). Packs, shop tarots and shop spectrals get their own rollout forecast (rollout_items.py: packs "
    "opened with sampled contents and the Picker rule, tarots used on tarots.plan targets) in what is left "
    "of the 4 s shop budget; one that beats buying nothing by > 2 SE is bought by code after any joker "
    "auto-buy, and JEV sees the same forecast fields on every pack/tarot/spectral. In an open pack the "
    "Picker's card is taken by code when its value is >= 1.25x the runner-up's, otherwise JEV rates the "
    "cards with a pick_value field. Forecast exceptions are logged with tracebacks to "
    "runs/_forecast_errors.log; fixed a KeyError in the forecast state when Fortune Teller is owned. Based on: "
    "v20 draft: the policy lab's arm C in production. Forecast horizon 3 bosses (n=24); a forecast "
    "option that beats buying nothing by > 2 SE in blinds cleared is bought by code (recorded as an "
    "'auto' shop event with its numbers), without a JEV question; JEV still rates close calls, packs, "
    "tarots, spectrals and economy items with the forecast fields (survive_2_bosses and "
    "blinds_cleared_next_6 renamed survive_horizon and blinds_cleared_horizon); reroll rule kept; rollout guard for "
    "an empty deck and hand. Based on: "
    "v19 draft: rollout forecast in the shop (rollout.py). Each joker, planet, voucher and swap is "
    "simulated through the next 2 bosses with the build frozen (32 rollouts, common random numbers); "
    "JEV sees next_boss_clear vs skip (paired difference and SE), survive_2_bosses, boss_margin and "
    "blinds_cleared_next_6 instead of measured_gain/readiness_after. Code: sure buy at > 2 SE over "
    "skip, hide items > 2 SE worse (unless economic), reroll when nothing beats skip by 1 SE, swaps "
    "only sell the 2 weakest jokers; v18 path when the forecast fails. Packs/tarots unchanged."
)

# Short factual brief, sent with every question as `game_rules`. Strategy lives in the questions.
GAME_RULES = (
    "Balatro: reach each blind's chip target before hands run out. A hand scores chips x mult; jokers add "
    "chips or mult, or multiply mult (xMult). Discards are free but limited. Interest: $1 per $5 held at "
    "round end, max $5 (at $25)."
)

INSTRUCTIONS = {
    "blind": "Should we play or skip the upcoming blind?",
    "play_now": (
        "Should we play our strongest hand (`strongest_play`) now instead of discarding first to build a "
        "better one? `best_draw_by_measurement` compares the best draw plan with playing now: its average "
        "includes misses, and poor_draw_next_play shows downside. Weigh the measured improvement and "
        "chance of meeting pace against spending one of our remaining discards. Preserve discards when "
        "we are on pace and the improvement is small. Survival this round comes before build preference."
    ),
    "best_play": (
        "Which play is best this turn? `estimated_score` already includes our jokers (except any in "
        "`jokers_not_in_estimates`), but are approximate. Prefer the higher score unless a "
        "lower one keeps stronger cards or avoids resetting a scaling joker."
    ),
    "setup_play": (
        " When `next_hand_outlook` "
        "is present, no discards remain but another hand does: compare the supplied two-play totals "
        "and rankings, not just immediate score. A setup play spends a hand to preserve a promising draw. "
        "Weigh poor-draw totals and chance_two_plays_clear as well as the average; a high average alone "
        "does not guarantee survival. The forecast assumes our best modeled next play and includes "
        "misses; it does not plan the whole round. Choose for survival rather than build preference."
    ),
    "best_discard": (
        "If we discard, which plan best helps us survive this round? Options are ordered by measured "
        "survival value. Each models our best next play after the draw, including misses and cards held "
        "for joker effects. Prefer better average score and downside; with one hand left, prefer the "
        "chance to clear. These are sampled scoring estimates, so account for unmodeled joker effects. "
        "Use build fit only to break close choices, never to justify a clearly worse survival outlook."
    ),
    "item_value": (
        "How much would taking the item in `item` (described in this question) strengthen this run? "
        "`next_boss_clear`, `survive_horizon`, `boss_margin` and `blinds_cleared_horizon` simulate the "
        "next blinds with this item and the build otherwise frozen (see `forecast`): trust them for scoring "
        "power and add what they cannot see (money, later purchases). `measured_gain`, when shown instead, "
        "is its measured effect on the scores of our recent hands, jokers included: trust it "
        "for the immediate effect and add what it cannot see (scaling over the run, money, risk, future "
        "hands). `use` says what a tarot or spectral would do for us; `worth` says what a voucher gives for "
        "the rest of the run. Favour items that fit `our_build`. Consider `engine.missing`, `upcoming_boss` "
        "and what it costs. `readiness` compares our typical hand with what the next blinds need per hand "
        "and `plan` says whether to spend on power or save. "
        "With little scoring support, prefer reliable Mult and useful temporary jokers over an xMult "
        "effect whose activation conditions are not met. `known_effect` restores missing card text. "
        "For a free pack pick, follow `pack_selection`; do not charge its pack price again."
    ),
}

CASH_ITEM_QUESTION = 'How valuable is taking this cash tarot in this situation? Use `cash_outcome`, which already subtracts the price before calculating the payout. In a shop, buying and using it can fund another purchase in the SAME shop; compare net profit, not just the temporary purchase cost. A substantial immediate net cash profit is a strong economy improvement even though it adds no chips directly and fits any hand type. A loss or zero profit is not an immediate cash upgrade. In a paid pack, the money competes with the other free picks: prefer immediate scoring if needed to survive, otherwise consider how the cash funds future upgrades.'

CASH_STEP_QUESTION = 'Which action should we take before continuing this shop? Each cash-tarot option buys and immediately uses that card, then returns to this SAME shop with the stated net cash profit. Other items remain available; no blind is played in between. Prefer useful immediate profit that funds our next purchases. Choose continue_shopping if there is a concrete reason to pass these cash transactions. The numeric cash outcomes already include the purchase price.'

ITEM_VALUE_LEVELS = [
    "Hurts the run or is wasted (off-plan, useless now, or not worth the money)",
    "Marginal: small or situational benefit",
    "Useful: a solid upgrade for this build",
    "Strong: clearly improves scoring or economy for this run",
    "Core: a key engine piece, e.g. an xMult or scaling joker that fits our main hands",
]

PLAY_NOW_CRITERIA = {
    "true": "Play the strongest hand now",
    "false": "Discard first to build a stronger hand",
}

OPTIONS = {
    "blind_play": "Play the {name} (need {score} chips) and earn its cash reward",
    "blind_skip": (
        "Skip the {name} to gain the {tag_name}: {tag_effect}. "
        "Skipping forgoes the blind's money and shop visit."
    ),
    "discard_plan": "{label}",
}
