"""Everything Jev reads, in one place. Editing this file (or hands.py, strategy.py, decisions.py) creates
a new prompt version. VERSION_NOTE is shown on the Prompts page; update it to describe what changed.

v6 follows TypeSafe's guidance: narrow questions batched in one call, decision-specific state, guidance
in the questions instead of the state, structured option fields, and code combining the answers.
"""

VERSION_NOTE = (
    "v6 + fixes: buy/take thresholds moved to mid-scale (v6 bought once in 14 shops); never swap out a "
    "Negative joker (frees no slot); shop sees the upcoming boss; scaling xMult jokers tagged as scaling, "
    "Obelisk tagged with its reset condition; empty fields dropped. Base: "
    "Restructured per TypeSafe guidance. Hand: three questions in one call (play now? / best play / "
    "best discard) combined in code; plays and discards are structured objects; state is the round, "
    "cards, jokers, relevant hand levels and the remaining deck. Shop and packs: a 0-4 Score per item "
    "in one call; code buys the best item above a threshold (stricter when it dips under the $25 "
    "interest buffer), rerolls only from surplus, else leaves. Strategy text moved out of the state."
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
        "Which play will score the most chips this turn once the jokers in `jokers` apply? Estimates "
        "exclude jokers; a joker that boosts a hand type can make a lower estimate the better play."
    ),
    "best_discard": (
        "If we discard, which discard gives the best chance of a strong next hand, given `cards_in_hand` "
        "and the cards left in `deck_remaining`?"
    ),
    "item_value": (
        "How much would taking the item in `item` (described in this question) strengthen this run? A strong run combines a chips source, +Mult and "
        "xMult jokers (xMult and jokers that scale over the run matter most), jokers that boost the hands "
        "in `main_hands`, and levelled-up main hands. Consider `engine.missing` and what it costs."
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
