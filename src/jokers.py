"""Joker-aware hand scoring, parsed from each joker's live effect text.

BalatroBot gives every joker's current description, including "(Currently X1.4 Mult)" for scaling
jokers, so most effects can be read straight from the text. Scoring follows Balatro's order: each
scoring card (with retriggers) and its per-card joker effects, then cards held in hand, then each
joker left to right. Jokers whose scoring text we can't parse are reported as unmodelled, so Jev
knows where the estimate is blind. Random effects count at their average.

Part of the prompt version hash (see recorder.PROMPT_VERSION).
"""

import re

from hands import RANK_CHIPS, RANK_ORDER, _d
from strategy import CONTAINED_IN, HAND_NAMES

FACES = set("JQK")
WORD_RANK = {"Ace": "A", "King": "K", "Queen": "Q", "Jack": "J", "10": "T"} | {str(n): str(n) for n in range(2, 10)}
WORD_SUIT = {"Diamond": "D", "Heart": "H", "Club": "C", "Spade": "S"}
HAND_RE = "|".join(sorted(HAND_NAMES, key=len, reverse=True))
NUM = r"(\d+(?:\.\d+)?)"


def _parts(text):
    """'+20 Chips and +4 Mult' / 'X1.5 Mult' -> (chips, mult, xmult)."""
    c = sum(float(x) for x in re.findall(r"\+" + NUM + r" Chips", text))
    m = sum(float(x) for x in re.findall(r"\+" + NUM + r" Mult", text))
    x = 1.0
    for v in re.findall(r"X" + NUM + r" Mult", text):
        x *= float(v)
    return c, m, x


def _card_pred(subject):
    """Which scoring cards a per-card effect applies to, from e.g. 'face cards', 'Aces', '10 or 4'."""
    s = subject
    suit = next((v for k, v in WORD_SUIT.items() if k in s), None)
    if suit:
        return lambda i: i["suit"] == suit or i["enh"] == "WILD"
    if "face" in s:
        return lambda i: i["rank"] in FACES
    if "odd rank" in s:
        return lambda i: i["rank"] in set("A9753")
    if "even rank" in s:
        return lambda i: i["rank"] in set("T8642")
    ranks = {WORD_RANK[w] for w in re.findall(r"Ace|King|Queen|Jack|10|[2-9]", s)}
    if ranks:
        return lambda i: i["rank"] in ranks
    if "all" in s or "cards" in s:
        return lambda i: True
    return None


def parse(effect):
    """Rule dict for one joker, or None when its text has no scoring effect we can read."""
    e = effect or ""
    rule = {"card": [], "retrigger": [], "held": [], "hand": None}
    if re.search(r"1 in \d+ chance", e) and not re.match(r"[+X]\d", e):
        return None  # chance-based side effects (money, tarots, destruction)

    # Retriggers
    m = re.match(r"Retrigger (.+?)(?: used in scoring)?(?: (\d+) additional times?)?$", e)
    if m:
        subj, times = m.group(1), int(m.group(2) or 1)
        if "first played card" in subj:
            rule["retrigger"].append(("first", times))
        elif "held in hand" in subj:
            rule["held_retrigger"] = times
        elif "final hand of round" in subj:
            rule["retrigger"].append(("final", times))
        else:
            pred = _card_pred(subj.replace("each played", "").replace("all played", ""))
            if pred:
                rule["retrigger"].append((pred, times))
        return rule

    # Per scoring card: "Played Aces give +20 Chips and +4 Mult when scored", "Each played 10 or 4 gives ..."
    m = re.match(r"(First played face card|(?:Each )?[Pp]layed (.+?)) (?:each )?gives? (.+?) when scored", e)
    if m and "chance" not in e:
        c, mu, x = _parts(m.group(3))
        if c or mu or x != 1:
            pred = "first_face" if m.group(1).startswith("First") else _card_pred(m.group(2))
            if pred:
                rule["card"].append((pred, c, mu, x))
                return rule

    # Held in hand: Baron, Shoot the Moon, Raised Fist
    m = re.match(r"Each (\w+) held in hand gives (.+)$", e)
    if m and m.group(1) in WORD_RANK:
        r = WORD_RANK[m.group(1)]
        c, mu, x = _parts(m.group(2))
        rule["held"].append((lambda i, r=r: i["rank"] == r, mu, x))
        return rule
    if e.startswith("Adds double the rank of lowest ranked card held in hand"):
        rule["hand"] = lambda ctx: (0, 2 * min([_nominal(i) for i in ctx["held"]] or [0]), 1)
        return rule

    cur = re.search(r"\(Currently ([+X]?)\$?" + NUM + r"(?: (Mult|Chips))?\)", e)
    unit_guess = (re.search(r"(Mult|Chips)", e) or [None, "Mult"])[1]

    # Conditional on the played hand
    m = re.match(r"([+X])" + NUM + r" (Mult|Chips) if played hand contains an? (" + HAND_RE + ")", e)
    if m:
        sign, val, unit, need = m.groups()
        eff = _effect(sign, float(val), unit)
        rule["hand"] = lambda ctx: eff if ctx["name"] in CONTAINED_IN.get(need, {need}) else (0, 0, 1)
        return rule
    m = re.match(r"([+X])" + NUM + r" (Mult|Chips) if played hand contains 3 or fewer cards", e)
    if m:
        eff = _effect(m.group(1), float(m.group(2)), m.group(3))
        rule["hand"] = lambda ctx: eff if ctx["n_played"] <= 3 else (0, 0, 1)
        return rule
    m = re.match(r"([+X])" + NUM + r" (Mult|Chips) on final hand of round", e)
    if m:
        eff = _effect(m.group(1), float(m.group(2)), m.group(3))
        rule["hand"] = lambda ctx: eff if ctx["hands_left"] == 1 else (0, 0, 1)
        return rule
    m = re.match(r"([+X])" + NUM + r" (Mult|Chips) when 0 discards remaining", e)
    if m:
        eff = _effect(m.group(1), float(m.group(2)), m.group(3))
        rule["hand"] = lambda ctx: eff if ctx["discards_left"] == 0 else (0, 0, 1)
        return rule
    m = re.match(r"X" + NUM + r" Mult every \d+ hands played", e)
    if m:
        x = float(m.group(1))
        rule["hand"] = lambda ctx: (0, 0, x) if re.search(r"Active|\b0 remaining", e) else (0, 0, 1)
        return rule
    if e.startswith("Adds the number of times poker hand has been played"):
        rule["hand"] = lambda ctx: (0, ctx["times_played"] + 1, 1)
        return rule
    m = re.match(r"\+" + NUM + r" Chips for each remaining discard", e)
    if m:
        v = float(m.group(1))
        rule["hand"] = lambda ctx: (v * ctx["discards_left"], 0, 1)
        return rule
    m = re.match(r"X" + NUM + r" Mult if all cards held in hand are (\w+?)s? or (\w+?)s?$", e)
    if m:
        x, suits = float(m.group(1)), {WORD_SUIT.get(m.group(2)), WORD_SUIT.get(m.group(3))}
        rule["hand"] = lambda ctx: (0, 0, x) if all(i["suit"] in suits for i in ctx["held"]) else (0, 0, 1)
        return rule
    m = re.match(r"X" + NUM + r" Mult if poker hand contains a Diamond card, Club card, Heart card, and Spade", e)
    if m:
        x = float(m.group(1))
        rule["hand"] = lambda ctx: (0, 0, x) if len(ctx["scoring_suits"]) + ctx["wild"] >= 4 else (0, 0, 1)
        return rule
    m = re.match(r"X" + NUM + r" Mult if played poker hand has already been played this round", e)
    if m:
        x = float(m.group(1))
        rule["hand"] = lambda ctx: (0, 0, x) if ctx["played_this_round"] else (0, 0, 1)
        return rule
    if e.startswith("+0-23 Mult"):  # Misprint: random, count the average
        rule["hand"] = lambda ctx: (0, 11.5, 1)
        return rule

    # Scaling jokers that also gain during this hand: count the gain when its condition holds
    if cur:
        sign, val, unit = cur.group(1) or "+", float(cur.group(2)), cur.group(3) or unit_guess
        base = _effect(sign, val, unit)
        g = re.search(r"[Gg]ains ([+X])" + NUM + r" (Mult|Chips) if played hand contains an? (" + HAND_RE + ")", e)
        if g:
            gain, need = float(g.group(2)), g.group(4)
            rule["hand"] = lambda ctx: _effect(sign, val + gain, unit) \
                if ctx["name"] in CONTAINED_IN.get(need, {need}) else base
        else:
            rule["hand"] = lambda ctx: base
        return rule

    # Plain values: "+4 Mult", "X1.44 Mult, loses ...", "+60 Chips -5 Chips for every hand played"
    m = re.match(r"([+X])" + NUM + r" (Mult|Chips)(.*)$", e)
    if m and not re.match(r"\s*(if|on|when|every|for each|per)\b", m.group(4)):
        eff = _effect(m.group(1), float(m.group(2)), m.group(3))
        rule["hand"] = lambda ctx: eff
        return rule
    return None


def _effect(sign, val, unit):
    if sign == "X":
        return 0, 0, val
    return (val, 0, 1) if unit == "Chips" else (0, val, 1)


def _nominal(i):
    if i["enh"] == "STONE" or not i["rank"]:
        return 0
    return RANK_CHIPS[i["rank"]]


def entry(c):
    eff = _d(c.get("value")).get("effect") or ""
    return {"label": c.get("label"), "rule": parse(eff), "edition": _d(c.get("modifier")).get("edition"),
            "scoring": bool(re.search(r"Mult|Chips|Retrigger", eff))}


def build(gs, cards=None):
    """Parse the run's jokers once per decision: [{label, rule or None, edition, scoring}]."""
    out = [entry(c) for c in (_d(gs.get("jokers")).get("cards") or [] if cards is None else cards)]
    # Blueprint copies the joker to its right, Brainstorm the leftmost.
    for n, j in enumerate(out):
        if j["label"] in ("Blueprint", "Brainstorm"):
            src = out[n + 1] if j["label"] == "Blueprint" and n + 1 < len(out) else (out[0] if n else None)
            if src and src["label"] not in ("Blueprint", "Brainstorm"):
                j["rule"], j["scoring"] = src["rule"], src["scoring"]
    return out


def unmodelled(jokers):
    return [j["label"] for j in jokers if j["rule"] is None and j["scoring"]]


def after_discard(gs, discarded, drawn):
    """Forecast known discard-sensitive scoring values without mutating the live snapshot.

    Banner/Mystic Summit use the decremented round counter in score(). Other event-driven
    effects remain approximations, just as in the normal scorer.
    """
    cards = []
    for card in _d(gs.get("jokers")).get("cards") or []:
        effect = _d(card.get("value")).get("effect") or ""
        label = card.get("label")
        if label == "Blue Joker":
            remaining = max(0, len(_d(gs.get("cards")).get("cards") or []) - drawn)
            effect = f"+{2 * remaining} Chips"
        elif label == "Ramen":
            match = re.match(r"X" + NUM + r" Mult", effect)
            if match:
                effect = f"X{max(1, float(match.group(1)) - 0.01 * len(discarded)):.8f} Mult"
        elif label == "Green Joker":
            match = re.search(r"Currently \+" + NUM + r" Mult", effect)
            if match:
                effect = f"+{max(0, float(match.group(1)) - 1)} Mult"
        cards.append({**card, "value": {**_d(card.get("value")), "effect": effect}})
    return build(gs, cards)


def score(infos, combo, name, scoring, hand_levels, jokers, rnd):
    """Expected score of playing hand positions `combo` (scoring = indices into combo)."""
    h = hand_levels.get(name, {})
    chips, mult = float(h.get("chips", 0)), float(h.get("mult", 1))
    played = [infos[i] for i in combo]
    held = [infos[i] for i in range(len(infos)) if i not in combo]
    sc = sorted(scoring)
    hands_left = rnd.get("hands_left") or 1
    rules = [j["rule"] for j in jokers if j["rule"]]

    first_face = next((p for p in sc if played[p]["rank"] in FACES and not played[p]["debuff"]), None)
    for k, p in enumerate(sc):
        c = played[p]
        if c["debuff"]:
            continue
        reps = 1 + (c["seal"] == "RED")
        for r in rules:
            for pred, times in r["retrigger"]:
                if (pred == "first" and k == 0) or (pred == "final" and hands_left == 1) or (
                        callable(pred) and pred(c)):
                    reps += times
        for _ in range(reps):
            chips += (50 if c["enh"] == "STONE" or not c["rank"] else RANK_CHIPS[c["rank"]])
            chips += {"BONUS": 30}.get(c["enh"], 0) + {"FOIL": 50}.get(c["edition"], 0)
            mult += {"MULT": 4}.get(c["enh"], 0) + {"HOLO": 10}.get(c["edition"], 0)
            mult *= {"GLASS": 2}.get(c["enh"], 1) * {"POLYCHROME": 1.5}.get(c["edition"], 1)
            for r in rules:
                for pred, cc, mm, xx in r["card"]:
                    if (pred == "first_face" and p == first_face) or (callable(pred) and pred(c)):
                        chips, mult = chips + cc, (mult + mm) * xx

    held_reps = 1 + sum(r.get("held_retrigger", 0) for r in rules)
    for c in held:
        if c["debuff"]:
            continue
        for _ in range(held_reps + (c["seal"] == "RED")):
            if c["enh"] == "STEEL":
                mult *= 1.5
            for r in rules:
                for pred, mm, xx in r["held"]:
                    if pred(c):
                        mult = (mult + mm) * xx

    ctx = {"name": name, "n_played": len(played), "held": held, "hands_left": hands_left,
           "discards_left": rnd.get("discards_left") or 0, "times_played": h.get("played", 0),
           "played_this_round": h.get("played_this_round", 0),
           "scoring_suits": {played[p]["suit"] for p in sc if played[p]["enh"] != "WILD"},
           "wild": sum(played[p]["enh"] == "WILD" for p in sc)}
    for j in jokers:
        ed = j["edition"]
        chips += 50 if ed == "FOIL" else 0
        mult += 10 if ed == "HOLO" else 0
        if j["rule"] and j["rule"]["hand"]:
            cc, mm, xx = j["rule"]["hand"](ctx)
            chips, mult = chips + cc, (mult + mm) * xx
        mult *= 1.5 if ed == "POLYCHROME" else 1
    return int(max(0, chips) * max(1, mult))


# ---- measured value of a shop item (v9) ---------------------------------------

def sample_plays(gs, recent):
    """Hands to measure items on: this run's recent plays, else the example hands of our likely hands."""
    from hands import classify
    out = []
    for r in recent:
        played = [r["infos"][i] for i in r["combo"]]
        name, sc = classify(played)
        out.append((r["infos"], r["combo"], name, sc, r["rnd"]))
    if len(out) >= 4:  # fewer recent hands are too noisy alone: mix in typical ones
        return out
    hands = _d(gs.get("hands"))
    names = sorted(hands, key=lambda n: (-(hands[n].get("played") or 0), -(hands[n].get("level") or 1)))[:2] or ["Pair"]
    for n in names:
        ex = hands.get(n, {}).get("example") or []
        infos = [{"rank": k.split("_")[1], "suit": k[0], "enh": None, "edition": None, "seal": None, "debuff": False}
                 for k, _ in ex]
        if infos:
            name, sc = classify(infos)
            out.append((infos, list(range(len(infos))), name, sc, {"hands_left": 2, "discards_left": 1}))
    return out


def gain(gs, recent, add=None, remove=None, level_up=None):
    """Score ratio - 1 over sample hands after adding a joker card, removing joker index `remove`,
    or levelling a hand (level_up=(hand, chips, mult)). None when the joker's effect can't be modelled."""
    owned = list(_d(gs.get("jokers")).get("cards") or [])
    if add is not None and entry(add)["rule"] is None:
        return None if entry(add)["scoring"] else 0.0
    base_js = build(gs, owned)
    new_cards = [c for n, c in enumerate(owned) if n != remove] + ([add] if add is not None else [])
    new_js = build(gs, new_cards)
    levels = _d(gs.get("hands"))
    new_levels = levels
    if level_up:
        h, c, m = level_up
        new_levels = {**levels, h: {**levels.get(h, {}), "chips": levels.get(h, {}).get("chips", 0) + c,
                                    "mult": levels.get(h, {}).get("mult", 1) + m}}
    before = after = 0
    for infos, combo, name, sc, rnd in sample_plays(gs, recent):
        before += score(infos, combo, name, sc, levels, base_js, rnd)
        after += score(infos, combo, name, sc, new_levels, new_js, rnd)
    return after / before - 1 if before else None


def planet_level(effect):
    m = re.search(r"Level up (" + HAND_RE + r") \+(\d+) Mult and \+(\d+) chips", effect or "")
    return (m.group(1), int(m.group(3)), int(m.group(2))) if m else None
