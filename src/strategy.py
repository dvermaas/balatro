"""Code-level strategy: things that are always right, so Jev doesn't spend a decision on them,
plus joker tags that give Jev the build context community guides say matters.

Part of the prompt version hash (see recorder.PROMPT_VERSION).
"""

import re

from hands import _d

HAND_NAMES = ["Flush Five", "Flush House", "Five of a Kind", "Straight Flush", "Four of a Kind", "Full House",
              "Flush", "Straight", "Three of a Kind", "Two Pair", "Pair", "High Card"]
# "played hand contains a X" also fires for bigger hands that include X.
CONTAINED_IN = {
    "Pair": {"Pair", "Two Pair", "Three of a Kind", "Full House", "Four of a Kind", "Five of a Kind",
             "Flush House", "Flush Five"},
    "Two Pair": {"Two Pair", "Full House", "Flush House"},
    "Three of a Kind": {"Three of a Kind", "Full House", "Four of a Kind", "Five of a Kind", "Flush House",
                        "Flush Five"},
    "Straight": {"Straight", "Straight Flush"},
    "Flush": {"Flush", "Straight Flush", "Flush House", "Flush Five"},
    "Four of a Kind": {"Four of a Kind", "Five of a Kind", "Flush Five"},
}
COPIERS =("Blueprint", "Brainstorm")  # these copy a neighbour, so never reorder around them


def joker_kind(effect):
    """Coarse joker role from its effect text: xmult, scaling, mult, chips, economy or utility."""
    e = effect or ""
    if re.search(r"\bX\d", e):
        return "xmult"
    if re.search(r"\bgains\b|permanently|per hand played", e, re.I) and "$" not in e:
        return "scaling"
    if re.search(r"\+\d+ Mult|to Mult|Mult for each", e):
        return "mult"
    if re.search(r"\+\d+ Chips", e):
        return "chips"
    if re.search(r"\$\d|Earn \$|sell value", e):
        return "economy"
    return "utility"


def main_hands(gs, n=2):
    """The hands the run is built around: levelled first, then strongest, then most played.

    Frequency alone locked v3/v4 into Pair builds (early rounds mostly deal pairs, which then got
    favoured by tags and planets). High Card and Pair only count once they have been levelled up.
    """
    picks = []
    for name, h in _d(gs.get("hands")).items():
        level, played = h.get("level", 1) or 1, h.get("played", 0) or 0
        # v15: a level alone no longer makes a main hand (one Jupiter made Flush "main", which then drew more
        # Jupiters while we kept playing pairs); we must actually be playing it.
        if name not in HAND_NAMES or played < 2:
            continue
        if name in ("High Card", "Pair") and level < 2:
            continue
        picks.append((-level, HAND_NAMES.index(name), -played, name))
    return [p[-1] for p in sorted(picks)[:n]]


def engine(jokers_effects):
    """What the joker lineup covers, and what a scoring engine still lacks."""
    kinds = [joker_kind(e) for e in jokers_effects]
    has = {k: kinds.count(k) for k in ("chips", "mult", "xmult", "scaling", "economy", "utility") if k in kinds}
    missing = []
    if not has.get("mult") and not has.get("scaling"):
        missing.append("a +Mult source for a reliable scoring base")
    if not has.get("chips"):
        missing.append("a chips source")
    if not has.get("xmult"):
        missing.append("xMult that activates with our current jokers and hands")
    return {"has": has or "nothing yet", "missing": missing or "nothing major: upgrade weak or off-plan jokers"}


def joker_tags(effect, mains):
    kind = joker_kind(effect)
    tags = [{"xmult": "xMult", "scaling": "scales over the run", "mult": "+Mult", "chips": "+Chips",
             "economy": "economy", "utility": "utility"}[kind]]
    if kind == "xmult" and re.search(r"\bgains\b", effect or ""):
        tags[0] = "xMult that scales over the run"
    if "without playing your most played poker hand" in (effect or "") and mains:
        tags.append(f"resets whenever we play our most played hand (likely {mains[0]})")
    if (re.search(r"destroy (a random )?Joker|destroy Joker|this card is destroyed|is destroyed", effect or "", re.I)
            and "1 in 1000" not in (effect or "")):  # Cavendish: negligible risk
        tags.append("risky: destroys jokers or itself")
    m = re.search(r"contains an? (" + "|".join(HAND_NAMES) + ")", effect or "")
    if m and mains:
        hit = [h for h in mains if h in CONTAINED_IN.get(m.group(1), {m.group(1)})]
        tags.append(f"boosts {' and '.join(hit)}, our main hand{'s' if len(hit) > 1 else ''}" if hit
                    else f"only helps hands containing a {m.group(1)}, which we rarely play")
    return tags


def interest(money):
    return max(0, min(5, (money or 0) // 5))


def _cards(gs, area):
    c = _d(gs.get(area)).get("cards")
    return c if isinstance(c, list) else []


def auto_action(gs):
    """An always-good action to take before asking Jev, as (method, params, reason), or None."""
    state = gs.get("state")
    if state in ("SHOP", "BLIND_SELECT", "SELECTING_HAND"):
        for i, c in enumerate(_cards(gs, "consumables")):
            if c.get("set") == "PLANET":
                return "use", {"consumable": i}, f"auto-use planet {c.get('label')}"
    if state in ("SHOP", "SELECTING_HAND"):
        jokers = _cards(gs, "jokers")
        if any(j.get("label") in COPIERS for j in jokers):
            return None
        kinds = [joker_kind(_d(j.get("value")).get("effect")) == "xmult" for j in jokers]
        order = [i for i, x in enumerate(kinds) if not x] + [i for i, x in enumerate(kinds) if x]
        if order != list(range(len(jokers))):
            return "rearrange", {"jokers": order}, "move xMult jokers to the right so they apply last"
    return None
