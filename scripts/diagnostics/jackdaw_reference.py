"""Strict, one-play adapter for a pinned Jackdaw checkout, never the live policy.

The adapter consumes ONLY allowlisted public observations. It never starts a
simulated seeded run, predicts a real draw, or loads a game save. Unsupported
state raises explicitly rather than silently using prototype/default values.
Each evaluation reconstructs mutable engine objects; callers remain unchanged.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHECKOUT = ROOT / "runs/_research/jackdaw"
REVISION = "e66de78855df84755d3af7d9a016129f3449f878"
sys.path.insert(0, str(CHECKOUT))
from jackdaw.engine.blind import Blind
from jackdaw.engine.card import Card
from jackdaw.engine.data.prototypes import BLINDS
from jackdaw.engine.hand_eval import evaluate_hand
from jackdaw.engine.hand_levels import HandLevels
from jackdaw.engine.rng import PseudoRandom
from jackdaw.engine.scoring import score_hand

SUITS = {"H": "Hearts", "D": "Diamonds", "S": "Spades", "C": "Clubs"}
RANKS = {"A": "Ace", "K": "King", "Q": "Queen", "J": "Jack", "T": "10"}
ENHANCEMENTS = {None: "c_base", **{k.upper(): "m_" + k for k in
    ("bonus", "mult", "wild", "glass", "steel", "stone", "gold", "lucky")}}
STATIC = set("""j_joker j_greedy_joker j_lusty_joker j_wrathful_joker j_gluttenous_joker
 j_jolly j_zany j_mad j_crazy j_droll j_sly j_wily j_clever j_devious j_crafty
 j_half j_banner j_mystic_summit j_dusk j_raised_fist j_fibonacci j_scary_face
 j_abstract j_even_steven j_odd_todd j_scholar j_supernova j_blackboard j_baron
 j_photograph j_acrobat j_sock_and_buskin j_hanging_chad j_smiley j_walkie_talkie
 j_shoot_the_moon j_stuntman j_duo j_trio j_family j_order j_tribe j_arrowhead
 j_onyx_agate j_pareidolia j_smeared j_four_fingers j_shortcut j_card_sharp
 j_gros_michel j_cavendish j_burglar j_riff_raff j_golden j_showman j_juggler
 j_drunkard j_troubadour j_todo_list j_mime j_splash""".split())
DYNAMIC = {"j_blue_joker", "j_bull", "j_ice_cream", "j_popcorn", "j_ramen",
           "j_fortune_teller", "j_throwback", "j_ancient", "j_red_card"}


class Unsupported(ValueError):
    """Observation lacks information required for an honest deterministic score."""


def verify_checkout():
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=CHECKOUT, text=True).strip()
    if revision != REVISION:
        raise RuntimeError(f"Jackdaw revision changed: {revision}")
    dirty = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=CHECKOUT, text=True)
    if dirty.strip():
        raise RuntimeError("Jackdaw tracked source is modified")
    return revision


def number(pattern, text, description):
    found = re.search(pattern, text)
    if not found:
        raise Unsupported(f"Missing public {description}")
    return float(found[1])


def playing_card(src):
    value, modifier, state = src["value"], src["modifier"], src["state"]
    if state.get("hidden") or state.get("face_down") or state.get("facing") == "back":
        raise Unsupported("Face-down card identity must not be used")
    rank, suit = value.get("rank"), value.get("suit")
    if rank not in set("23456789TJQKA") or suit not in SUITS:
        raise Unsupported("Unknown visible card identity")
    enhancement = modifier.get("enhancement")
    if enhancement not in ENHANCEMENTS:
        raise Unsupported(f"Enhancement {enhancement}")
    c = Card()
    c.set_base(f"{suit}_{rank}", SUITS[suit], RANKS.get(rank, rank))
    c.set_ability(ENHANCEMENTS[enhancement])
    # Public extra chips includes enhancement bonus and accumulated Hiker chips.
    extra = re.search(r"\+(\d+) extra chips", value.get("effect", ""), re.I)
    if extra:
        c.ability["perma_bonus"] = int(extra[1]) - (30 if enhancement == "BONUS" else 0)
    apply_modifiers(c, modifier, state)
    return c


def apply_modifiers(card, modifier, state):
    edition = modifier.get("edition")
    if edition:
        if edition not in {"FOIL", "HOLO", "POLYCHROME", "NEGATIVE"}:
            raise Unsupported(f"Edition {edition}")
        card.set_edition({edition.lower(): True})
    if modifier.get("seal"):
        if modifier["seal"] not in {"RED", "BLUE", "GOLD", "PURPLE"}:
            raise Unsupported(f"Seal {modifier['seal']}")
        card.seal = modifier["seal"].title()
    card.debuff = bool(state.get("debuff"))


def make_jokers(obs, game):
    out = []
    for src in obs["jokers"]["cards"]:
        key, text = src["key"], src["value"].get("effect", "")
        if key not in STATIC | DYNAMIC:
            raise Unsupported(f"Joker state not adapted: {key}")
        c = Card()
        c.set_ability(key)
        apply_modifiers(c, src["modifier"], src["state"])
        if not c.debuff:
            if key == "j_blue_joker":
                game["deck_cards_remaining"] = number(r"Currently \+(\d+) Chips", text, "Blue Joker value") / 2
            elif key == "j_bull":
                displayed = number(r"Currently \+(\d+) Chips", text, "Bull value")
                if displayed != 2 * max(0, game["money"]):
                    raise Unsupported("Bull value and visible money disagree")
            elif key == "j_ice_cream":
                c.ability["extra"]["chips"] = number(r"^\+(\d+) Chips", text, "Ice Cream value")
            elif key in {"j_popcorn", "j_red_card"}:
                pattern = r"^\+(\d+) Mult" if key == "j_popcorn" else r"Currently \+(\d+) Mult"
                c.ability["mult"] = number(pattern, text, "current Mult")
            elif key == "j_ramen":
                c.ability["x_mult"] = number(r"^X([\d.]+) Mult", text, "Ramen value")
            elif key == "j_fortune_teller":
                game["consumable_usage_tarot"] = number(r"Currently \+(\d+)", text, "tarot count")
            elif key == "j_throwback":
                game["skips"] = (number(r"Currently X([\d.]+) Mult", text, "Throwback value") - 1) / .25
            elif key == "j_ancient":
                match = re.search(r"with (Heart|Diamond|Club|Spade) suit", text)
                if not match:
                    raise Unsupported("Ancient Joker public suit missing")
                game["ancient_suit"] = match[1] + "s"
        out.append(c)
    return out


def evaluate(obs, indices):
    """One ordered play; only the immediate score is deterministic, not its future."""
    if not 1 <= len(indices) <= 5 or len(set(indices)) != len(indices):
        raise ValueError("Play must contain 1-5 distinct indices")
    if any(i < 0 or i >= len(obs["hand"]["cards"]) for i in indices):
        raise ValueError("Card index out of range")
    if obs.get("deck") not in (None, "RED"):
        raise Unsupported(f"Deck not adapted: {obs.get('deck')}")
    current = next((b for b in obs["blinds"].values() if b["status"] == "CURRENT"), None)
    if not current:
        raise Unsupported("No current blind")
    name = current["name"]
    # Those pre-score events need information/transitions absent from this adapter.
    if name in {"The Hook", "The Ox", "Amber Acorn", "Crimson Heart", "Cerulean Bell"}:
        raise Unsupported(f"Pre-score transition not adapted: {name}")
    key = next((k for k, p in BLINDS.items() if p.name == name), None)
    if key is None:
        raise Unsupported(f"Unknown blind: {name}")
    blind = Blind.create(key, ante=obs["ante_num"])
    blind.chips = current["score"]
    levels = HandLevels()
    for hand, data in obs["hands"].items():
        dest = levels.get_state(hand)
        for attr in ("level", "chips", "mult", "played", "played_this_round"):
            setattr(dest, attr, data[attr])
    blind.hands_used = {h: True for h, v in obs["hands"].items() if v["played_this_round"] > 0}
    if name == "The Mouth":
        if len(blind.hands_used) > 1:
            raise Unsupported("Ambiguous Mouth locked hand")
        blind.only_hand = next(iter(blind.hands_used), None)
    rnd = obs["round"]
    game = {"money": obs.get("money", 0), "hands_left": rnd["hands_left"] - 1,
            "current_round_hands_played": rnd["hands_played"], "discards_left": rnd["discards_left"],
            "discards_used": rnd["discards_used"], "joker_slots": obs["jokers"].get("limit", 5)}
    if name == "The Tooth":
        game["money"] -= len(indices)
    js = make_jokers(obs, game)
    cs = [playing_card(c) for c in obs["hand"]["cards"]]
    played, held = [cs[i] for i in indices], [c for i, c in enumerate(cs) if i not in indices]
    scoring = evaluate_hand(played, jokers=js).scoring_cards
    if any(c.center_key == "m_lucky" and not c.debuff for c in scoring):
        raise Unsupported("Scoring Lucky card: random outcome is not a deterministic oracle")
    result = score_hand(played, held, js, levels, blind, PseudoRandom("DIAGNOSTIC-INDEPENDENT"), game_state=game)
    return {"total": result.total, "chips": result.chips, "mult": result.mult,
            "hand": result.hand_type, "debuffed": result.debuffed}
