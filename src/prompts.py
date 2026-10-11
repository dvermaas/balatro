"""Everything Jev reads, in one place. Editing this file (or hands.py, strategy.py, decisions.py) creates
a new prompt version. VERSION_NOTE is shown on the Prompts page; update it to describe what changed.

v6 follows TypeSafe's guidance: narrow questions batched in one call, decision-specific state, guidance
in the questions instead of the state, structured option fields, and code combining the answers.
"""

VERSION_NOTE = (
    "From the BalatroBench dissection (top LLMs: ~4 jokers by ante 2, 11-18 consumables and 2-3 vouchers a "
    "run, one committed hand type). Committed build (build.py: target hand + flush suit) is in every state; "
    "discard plans say whether they fit it. Tarots and spectrals get used: tarots.py knows each card's "
    "mechanics and picks targets for the build (pack picks pass targets; held ones auto-use, targeted ones "
    "while a hand is showing); Jev sees what each would do. Vouchers carry their run-long worth, and strong "
    "ones (Grabber, Wasteful, Paint Brush, Clearance Sale, Overstock, Antimatter) are bought when affordable. "
    "Early rerolls down to $5 while a joker slot is empty. Discard budget: near pace with no spare discards, "
    "dig only when a plan's odds x payoff clearly beats playing. Builds on v9."
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
        "better one? Compare its estimate with `round.chips_needed_per_remaining_hand`, and weigh hands left, "
        "discards left and how likely `deck_remaining` is to improve the hand."
    ),
    "best_play": (
        "Which play is best this turn? `estimated_score` already includes our jokers (except any in "
        "`jokers_not_in_estimates`), so it is the real expected score. Prefer the higher score unless a "
        "lower one keeps stronger cards or avoids resetting a scaling joker."
    ),
    "best_discard": (
        "If we discard, which plan is best? Each plan gives `chance_to_hit` (exact odds from the cards left "
        "in the deck) and what the hand scores `if_hit`, jokers included, and whether it fits `our_build`. "
        "Weigh odds times payoff against what we need this round; prefer plans that grow our build."
    ),
    "item_value": (
        "How much would taking the item in `item` (described in this question) strengthen this run? "
        "`measured_gain` is its measured effect on the scores of our recent hands, jokers included: trust it "
        "for the immediate effect and add what it cannot see (scaling over the run, money, risk, future "
        "hands). `use` says what a tarot or spectral would do for us; `worth` says what a voucher gives for "
        "the rest of the run. Favour items that fit `our_build`. Consider `engine.missing`, `upcoming_boss` "
        "and what it costs."
    ),
}

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
