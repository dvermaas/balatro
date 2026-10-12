"""Everything Jev reads, in one place. Editing this file (or hands.py, strategy.py, decisions.py) creates
a new prompt version. VERSION_NOTE is shown on the Prompts page; update it to describe what changed.

v6 follows TypeSafe's guidance: narrow questions batched in one call, decision-specific state, guidance
in the questions instead of the state, structured option fields, and code combining the answers.
"""

VERSION_NOTE = (
    "v13: readiness-driven shop on top of v12's hand play. readiness.py deals 24 random hands from our "
    "current deck and scores the best play in each (jokers and hand levels included), then compares the "
    "typical hand with the per-hand pace of the next 3 blinds. Behind (readiness < 0.6): invest, i.e. buy "
    "Useful items down to $5, reroll down to $10, a joker swap measured at +50% is a sure buy, no vouchers. "
    "Ahead (>= 1.2): bank, only clear upgrades. Between: v11/v12 rules. The shop state carries the "
    "readiness per blind and a plan line, and jokers/planets show readiness before -> after."
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
    "best_discard": (
        "If we discard, which plan best helps us survive this round? Options are ordered by measured "
        "survival value. Each models our best next play after the draw, including misses and cards held "
        "for joker effects. Prefer better average score and downside; with one hand left, prefer the "
        "chance to clear. These are sampled scoring estimates, so account for unmodeled joker effects. "
        "Use build fit only to break close choices, never to justify a clearly worse survival outlook."
    ),
    "item_value": (
        "How much would taking the item in `item` (described in this question) strengthen this run? "
        "`measured_gain` is its measured effect on the scores of our recent hands, jokers included: trust it "
        "for the immediate effect and add what it cannot see (scaling over the run, money, risk, future "
        "hands). `use` says what a tarot or spectral would do for us; `worth` says what a voucher gives for "
        "the rest of the run. Favour items that fit `our_build`. Consider `engine.missing`, `upcoming_boss` "
        "and what it costs. `readiness` compares our typical hand with what the next blinds need per hand "
        "and `plan` says whether to spend on power or save; `readiness_after` is the measured lift."
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
