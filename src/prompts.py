"""Everything Jev reads, in one place. Editing this file (or hands.py, strategy.py, decisions.py) creates
a new prompt version. VERSION_NOTE is shown on the Prompts page; update it to describe what changed.

v6 follows TypeSafe's guidance: narrow questions batched in one call, decision-specific state, guidance
in the questions instead of the state, structured option fields, and code combining the answers.
"""

VERSION_NOTE = (
    "Code measures, Jev weighs. Shop/pack items carry measured_gain: the score change the joker or planet "
    "gives on this run's last 10 hands (jokers.py). Through ante 3 joker slots fill before interest (buy a "
    "Useful joker regardless of the $25 buffer while under 4 jokers), and any joker measured at +50% is "
    "bought. Discard plans carry exact draw odds and the score if they hit (odds.py, order-free deck "
    "counts). Boss-aware: debuffed cards are tossed first and never chased; Psychic/Eye/Mouth filter "
    "plays; Flint halves estimates; on the last hand, discards are used before a play that can't clear "
    "(The Needle). Fixed suit order in deck counts. Builds on v8 (joker-aware estimates, pace gate)."
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
        "in the deck) and what the hand scores `if_hit`, jokers included. Weigh odds times payoff against "
        "what we need this round."
    ),
    "item_value": (
        "How much would taking the item in `item` (described in this question) strengthen this run? "
        "`measured_gain` is its measured effect on the scores of our recent hands, jokers included: trust it "
        "for the immediate effect and add what it cannot see (scaling over the run, money, risk, future "
        "hands). Consider `engine.missing`, `upcoming_boss` and what it costs."
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
