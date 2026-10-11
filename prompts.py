"""Everything Jev reads, in one place. Editing this file (or hands.py) creates a new prompt version.

VERSION_NOTE is shown on the Prompts page; update it to describe what changed.
"""

VERSION_NOTE = (
    "v4 + main hands chosen by level and strength instead of play count (High Card/Pair only once "
    "levelled), and the hand-play instruction no longer pushes toward main hands, as in v2. v3/v4 "
    "had locked into Pair builds: fewer discards, more weak plays. | v4 base: v3 + engine focus and reroll discipline. Reroll is only offered when $25+ remains afterwards "
    "(v3 rerolled 31x and died broke). State gets an `engine` summary of what the joker lineup covers "
    "and is missing; shop asks what completes the engine. Jokers that destroy jokers are tagged risky. "
    "Money rule: spend only what is above $25. | v3 base: Build strategy from community guides + v2 data. Code now auto-uses planet cards and keeps xMult "
    "jokers rightmost. Jokers are tagged (xMult / scales / +Mult / +Chips / economy) and by synergy "
    "with the run's main hands; state includes main hands and interest. Rules stress xMult and scaling "
    "jokers, jokers over packs, keeping ~$25 for interest and spending the surplus. Full joker slots "
    "get combined 'replace X with Y' options."
)

GAME_RULES = (
    "You are playing Balatro, a poker roguelike. Each round you must reach the blind's chip target "
    "before running out of hands. A played hand scores (chips) x (mult); jokers modify the score. "
    "Discards cost no hand but are limited, and any discards left when the round ends are simply wasted. "
    "Blind targets grow fast, so the run is won by building a strong joker lineup, not by single hands. "
    "Winning builds focus on one or two poker hands (`game.main_hands`), level those up with planets, "
    "and stack jokers that boost them: a source of chips, some +Mult, and above all xMult jokers, which "
    "multiply the whole score. Jokers that scale (grow over the run) are worth more than fixed bonuses. "
    "Jokers whose bonus needs a hand we rarely play add little. Early in a run a good joker beats a "
    "booster pack. Interest pays $1 per $5 held at the end of each round, capped at $5 (reached at $25): "
    "keep about $25 once affordable and only spend money above that; never dip into the buffer to reroll. "
    "Think of the jokers as an engine: chips x mult x xMult. `game.engine` shows what ours covers and "
    "what it is missing; filling the biggest gap is usually the best purchase."
)

INSTRUCTIONS = {
    "blind": "Should we play or skip the upcoming blind?",
    "move": (
        "Choose the best next move this round. Compare each play's estimate with "
        "`game.chips_needed_per_remaining_hand`. If the best play falls well short of that pace and "
        "discards remain, discarding to build a stronger hand is usually better than spending a hand; "
        "if a play clears the blind, take it. Account for how the jokers boost each hand type."
    ),
    "shop": (
        "We are in the shop. Which single action best completes our scoring engine (see `game.engine.missing`)? "
        "Prefer xMult and 'scales over the run' jokers and jokers that boost our main hands; avoid jokers tagged "
        "risky unless they are clearly worth it. With full joker slots, replacing a weak or off-plan joker with "
        "a clearly better one is often right. Leaving with money is fine: it earns interest."
    ),
    "pack": (
        "A booster pack we already paid for is open. Taking a card costs nothing extra. Which card "
        "helps this run the most? Skip only if every card would actively hurt the run."
    ),
}

OPTIONS = {
    "blind_play": "Play the {name} (need {score} chips) and earn its cash reward",
    "blind_skip": (
        "Skip the {name} to gain the {tag_name}: {tag_effect}. "
        "Skipping forgoes the blind's money and shop visit."
    ),
    "play": "Play {hand} scoring {scoring}{kicked}. Estimated {estimate} chips before jokers: {verdict}.",
    "play_kicked": " (also plays away {cards})",
    "verdict_clears": "clears the blind ({need} still needed)",
    "verdict_pace": "{pct}% of the {pace}/hand pace needed ({need} still needed over {hands} hands)",
    "discard": "Discard: {label}. Throws away {cards} and draws replacements.",
    "shop_leave": "Leave the shop and go to the next blind, keeping the money (earning ${interest} interest)",
    "shop_buy_card": "Buy {item} for ${cost} ({after})",
    "shop_buy_voucher": "Buy voucher {item} for ${cost} ({after})",
    "shop_buy_pack": "Buy and open {item} for ${cost} ({after})",
    "shop_reroll": "Reroll the shop for ${cost} to see new cards ({after})",
    "shop_replace": "Replace joker {old} (sell for ${sell}) with {new} (costs ${cost}; {after})",
    "pack_take": "Take {item}",
    "pack_skip": "Skip taking anything; the ${paid} spent on this pack is wasted",
}
