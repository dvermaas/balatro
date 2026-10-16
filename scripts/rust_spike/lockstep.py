"""Spike: play the same seeds with the same actions in Jackdaw (+ our sim_patches), rumbot-sim and
Gambit, and report the first step where each Rust engine's state differs from Jackdaw's.

Policy (computed from Jackdaw's state, sent to all three by card identity / slot index):
select every blind; play a flush if 5+ of a suit, else the biggest rank groups (<=5 cards), else
the 5 highest cards; never discard; in the shop buy the first joker slot when affordable and a
joker slot is free; never buy packs; cash out; stop at game over / win / 400 steps.
"""
import json, math, os, re, subprocess, sys
from collections import Counter
SPIKE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(SPIKE))
sys.path.insert(0, ROOT + "/src")
import sim_backend; sim_backend.apply_patches()  # noqa: E401
from jackdaw.engine.run_init import initialize_run
from jackdaw.engine.game import step, IllegalActionError
from jackdaw.engine.actions import GamePhase, SelectBlind, PlayHand, CashOut, BuyCard, Discard, SkipBlind, RedeemVoucher, OpenBooster, PickPackCard, NextRound, SkipPack

CENTERS = json.load(open(ROOT + "/third_party/jackdaw/jackdaw/engine/data/centers.json"))
import unicodedata


def _nm(x):
    return "".join(ch for ch in unicodedata.normalize("NFKD", x).lower() if ch.isalnum())


NAME2KEY = {_nm(v["name"]): k for k, v in CENTERS.items() if v.get("name")}
RV = {r: i for i, r in enumerate("23456789TJQKA")}
DRIVERS = {"rumbot": SPIKE + "/rumbot_drv/target/release/rumbot_drv",
           "gambit": SPIKE + "/gambit_drv/target/release/gambit_drv"}


def norm_key(x):
    if _nm(x) in NAME2KEY and not re.match(r"[a-z]+_", x):
        x = NAME2KEY[_nm(x)]
    m = re.match(r"p_(\w+?)_(normal|jumbo|mega)_\d+$", x)
    return f"{m.group(1)}_{m.group(2)}" if m else x


def ident(c):  # playing cards by rank+suit, everything else by center key
    return cid(c) if c.ability.get("set") in ("Default", "Enhanced") else c.center_key


def cid(card):  # "H_A" -> "AH"
    s, r = card.card_key.split("_")
    return ("T" if r == "10" else r[0]) + s


PH = {GamePhase.BLIND_SELECT: "BLIND", GamePhase.SELECTING_HAND: "PLAY", GamePhase.ROUND_EVAL: "EVAL",
      GamePhase.SHOP: "SHOP", GamePhase.PACK_OPENING: "PACK", GamePhase.GAME_OVER: "OVER"}


class Jackdaw:
    def new(self, seed):
        self.gs = initialize_run("b_red", 1, seed)
        self.total = 0
        return self.dump("")

    def cmd(self, c, arg=None):
        gs = self.gs
        before = gs.get("chips", 0)
        try:
            if c == "select":
                step(gs, SelectBlind())
            elif c == "play":
                step(gs, PlayHand(card_indices=tuple(arg)))
                after = gs.get("chips", 0)
                self.total += after - before if after >= before else math.nan
            elif c == "discard":
                step(gs, Discard(card_indices=tuple(arg)))
            elif c == "cash":
                step(gs, CashOut())
            elif c == "buy":
                step(gs, BuyCard(shop_index=arg))
            elif c == "leave":
                step(gs, NextRound())
            elif c == "skippack":
                step(gs, SkipPack())
            elif c == "skip":
                step(gs, SkipBlind())
            elif c == "voucher":
                step(gs, RedeemVoucher(card_index=arg))
            elif c == "pack":
                step(gs, OpenBooster(card_index=arg))
            elif c == "pick":
                step(gs, PickPackCard(card_index=arg))
            err = ""
        except IllegalActionError as e:
            err = str(e)
        return self.dump(err)

    def dump(self, err):
        gs = self.gs
        ph = "WON" if gs.get("won") and gs["phase"] in (GamePhase.SHOP, GamePhase.ROUND_EVAL) and False else PH.get(gs["phase"], str(gs["phase"]))
        shop = gs["phase"] == GamePhase.SHOP
        rr = gs["round_resets"]
        return {"phase": ph, "ante": rr["ante"], "blind": {"Small": 0, "Big": 1, "Boss": 2}.get(gs.get("blind_on_deck"), -1),
                "boss": rr["blind_choices"]["Boss"], "money": gs["dollars"], "total": self.total,
                "deck_n": len(gs.get("deck", [])), "hand": [cid(c) for c in gs.get("hand", [])],
                "jokers": [c.center_key for c in gs.get("jokers", [])],
                "slots": [ident(c) for c in gs.get("shop_cards", [])] if shop else [],
                "vouchers": [c.center_key for c in gs.get("shop_vouchers", [])] if shop else [],
                "packs": [c.center_key for c in gs.get("shop_boosters", [])] if shop else [],
                "tags": [rr.get("blind_tags", {}).get("Small"), rr.get("blind_tags", {}).get("Big")],
                "pack": [ident(c) for c in gs.get("pack_cards", [])] if gs["phase"] == GamePhase.PACK_OPENING else [],
                "_pack_sets": [c.ability.get("set") for c in gs.get("pack_cards", [])] if gs["phase"] == GamePhase.PACK_OPENING else [],
                "_vcost": [c.cost for c in gs.get("shop_vouchers", [])] if shop else [],
                "_pcost": [c.cost for c in gs.get("shop_boosters", [])] if shop else [],
                "err": err, "_costs": [c.cost for c in gs.get("shop_cards", [])] if shop else [],
                "_sets": [c.ability.get("set") for c in gs.get("shop_cards", [])] if shop else [],
                "_jslots": gs.get("joker_slots", 5)}


class Proc:
    def __init__(self, path):
        self.p = subprocess.Popen([path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)

    def send(self, line):
        self.p.stdin.write(line + "\n")
        self.p.stdin.flush()
        return json.loads(self.p.stdout.readline())

    def new(self, seed):
        self.last_total = 0
        return self.send("new " + seed)

    def cmd(self, c, arg=None, ids=None):
        if c in ("play", "discard"):  # map card identities to this engine's hand positions
            hand, used, idx = list(self.state["hand"]), set(), []
            for i in ids:
                j = next(k for k, h in enumerate(hand) if h == i and k not in used)
                used.add(j)
                idx.append(j)
            return self.send(c + " " + ",".join(map(str, sorted(idx))))
        return self.send(c if arg is None else f"{c} {arg}")


def choose_play(hand):
    suits = Counter(c[1] for c in hand)
    s, n = suits.most_common(1)[0]
    if n >= 5:
        return sorted((c for c in hand if c[1] == s), key=lambda c: -RV[c[0]])[:5]
    ranks = Counter(c[0] for c in hand)
    groups = sorted((r for r, k in ranks.items() if k >= 2), key=lambda r: (-ranks[r], -RV[r]))
    out = []
    for r in groups:
        g = [c for c in hand if c[0] == r][:4]
        if len(out) + len(g) <= 5:
            out += g
    if out:
        return out
    return sorted(hand, key=lambda c: -RV[c[0]])[:5]


def choose_discard(hand):
    """Dig when the best play is high card or one pair: keep the main suit if 3+ of it, else pairs."""
    ranks = Counter(c[0] for c in hand)
    suits = Counter(c[1] for c in hand)
    s, n = suits.most_common(1)[0]
    if n >= 5 or max(ranks.values()) >= 3 or sum(1 for k in ranks.values() if k >= 2) >= 2:
        return None
    keep = [c for c in hand if c[1] == s] if n >= 3 else [c for c in hand if ranks[c[0]] >= 2]
    toss = sorted((c for c in hand if c not in keep), key=lambda c: RV[c[0]])[:5]
    return toss or None


PHASE_FIELDS = {"BLIND": ["ante", "blind", "boss", "tags", "money", "jokers"],
                "PLAY": ["ante", "blind", "hand", "deck_n", "money", "jokers", "score"],
                "EVAL": ["score"], "SHOP": ["money", "jokers", "slots", "vouchers", "packs"],
                "OVER": ["score"], "PACK": ["pack", "money", "jokers"]}

FIELDS = ["pack", "phase", "ante", "blind", "boss", "money", "deck_n", "hand", "jokers", "slots", "vouchers", "packs", "tags", "score", "err"]


def view(st, prev_total, played):
    v = {k: st.get(k) for k in FIELDS if k not in ("score",)}
    v["hand"] = sorted(v["hand"] or [])
    for k in ("jokers", "slots", "vouchers", "packs", "tags", "pack"):
        v[k] = [norm_key(x) for x in (v[k] or [])]
    v["err"] = bool(v["err"])
    v["score"] = (st["total"] - prev_total) if played else None
    if isinstance(v["score"], float) and v["score"] == int(v["score"]):
        v["score"] = int(v["score"])
    return v


def run_seed(seed, procs, max_steps=400):
    jd = Jackdaw()
    states = {"jackdaw": jd.new(seed)}
    for name, p in procs.items():
        p.state = p.new(seed)
        states[name] = p.state
    totals = {k: 0 for k in states}
    first_diff = {name: None for name in procs}
    steps, last_cmd = 0, "new"
    shop_flags = {}
    played = False
    while steps < max_steps:
        views = {k: view(s, totals[k], played) for k, s in states.items()}
        for k, s in states.items():
            totals[k] = s["total"]
        for name in procs:
            if first_diff[name] is None:
                ph0 = views["jackdaw"]["phase"]
                fs = ["phase", "err"] + (PHASE_FIELDS.get(ph0, FIELDS) if views[name]["phase"] == ph0 else [])
                diffs = [f for f in fs if views[name][f] != views["jackdaw"][f]]
                if diffs:
                    first_diff[name] = {"step": steps, "after": last_cmd, "ante": views["jackdaw"]["ante"],
                                        "fields": {f: (views["jackdaw"][f], views[name][f]) for f in diffs}}
        if all(v is not None for v in first_diff.values()):
            break
        s = states["jackdaw"]
        ph = s["phase"]
        if ph in ("OVER", "WON") or jd.gs.get("won") or s["err"]:
            break
        played = False
        if ph == "BLIND" and s["blind"] == 0 and s["ante"] % 2 == 0 and WIDE:
            c, arg, ids = "skip", None, None
        elif ph == "BLIND":
            c, arg, ids = "select", None, None
        elif ph == "PLAY" and jd.gs["current_round"].get("discards_left", 0) > 0 and (toss := choose_discard(s["hand"])):
            ids = toss
            jidx, used = [], set()
            for i in ids:
                j = next(k for k, h in enumerate(s["hand"]) if h == i and k not in used)
                used.add(j); jidx.append(j)
            c, arg = "discard", sorted(jidx)
        elif ph == "PLAY":
            ids = choose_play(s["hand"])
            jidx, used = [], set()
            for i in ids:
                j = next(k for k, h in enumerate(s["hand"]) if h == i and k not in used)
                used.add(j); jidx.append(j)
            c, arg, played = "play", sorted(jidx), True
        elif ph == "EVAL":
            c, arg, ids = "cash", None, None
        elif ph == "SHOP":
            buy = next((i for i, (st, cost) in enumerate(zip(s["_sets"], s["_costs"]))
                        if st == "Joker" and cost <= s["money"] and len(s["jokers"]) < s["_jslots"]), None)
            c, arg, ids = ("buy", buy, None) if buy is not None else ("leave", None, None)
            if buy is None and WIDE:
                if s["vouchers"] and s["_vcost"][0] <= s["money"] and not shop_flags.get("v"):
                    c, arg = "voucher", 0
                    shop_flags["v"] = True
                elif s["packs"] and s["_pcost"][0] <= s["money"] and not shop_flags.get("p"):
                    c, arg = "pack", 0
                    shop_flags["p"] = True
            if c == "leave":
                shop_flags.clear()
        elif ph == "PACK":
            pick = next((i for i, st in enumerate(s["_pack_sets"]) if st in ("Planet", "Default", "Enhanced")
                         or (st == "Joker" and len(s["jokers"]) < s["_jslots"])), None)
            c, arg, ids = ("pick", pick, None) if pick is not None and WIDE else ("skippack", None, None)
        else:
            break
        states["jackdaw"] = jd.cmd(c, arg)
        for name, p in procs.items():
            if first_diff[name] is None:
                p.state = p.cmd(c, arg, ids) if c in ("play", "discard") else p.cmd(c, arg)
                states[name] = p.state
            else:
                states[name] = states["jackdaw"]  # frozen: already diverged
        last_cmd = c
        steps += 1
    return {"seed": seed, "steps": steps, "final_ante": states["jackdaw"]["ante"],
            "final_phase": states["jackdaw"]["phase"], "first_diff": first_diff}


WIDE = True

if __name__ == "__main__":
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 20  # usage: lockstep.py [n_seeds] [out.json]
    import random
    rnd = random.Random(20261016)
    chars = "123456789ABCDEFGHIJKLMNPQRSTUVWXYZ"
    seeds = ["".join(rnd.choice(chars) for _ in range(8)) for _ in range(n)]
    procs = {k: Proc(v) for k, v in DRIVERS.items()}
    out = []
    for sd in seeds:
        r = run_seed(sd, procs)
        out.append(r)
        print(json.dumps(r), flush=True)
    if len(sys.argv) > 2:
        json.dump(out, open(sys.argv[2], "w"), indent=1)
