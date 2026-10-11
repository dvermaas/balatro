"""Game loop: read BalatroBot state -> build candidate moves -> Jev picks one -> execute."""

import json
import os
import time
from collections import deque
from pathlib import Path

from typesafe_sdk import Choice, TypeSafeClient

from balatro import Balatro, RPCError
from hands import _d, describe, discard_candidates, play_candidates
from recorder import RunRecorder

ROOT = Path(__file__).parent
GAME_RULES = (
    "You are playing Balatro, a poker roguelike. Each round you must reach the blind's chip target "
    "before running out of hands. A played hand scores (chips) x (mult); jokers modify the score. "
    "Discards cost no hand but are limited. Money buys jokers, packs and vouchers in the shop; "
    "holding money earns interest ($1 per $5 held, up to $5)."
)
SHOP_ACTION_CAP = 8
TRANSITIONAL_WAIT = 0.25


def load_env(path=ROOT / ".env"):
    if path.exists():
        for line in path.read_text().splitlines():
            k, _, v = line.strip().partition("=")
            if k and not k.startswith("#"):
                os.environ.setdefault(k, v.strip().strip('"').strip("'"))


def area(gs, name):
    return _d(gs.get(name))


def cards_of(gs, name):
    c = area(gs, name).get("cards")
    return c if isinstance(c, list) else []


def item_desc(c):
    v = _d(c.get("value"))
    m = _d(c.get("modifier"))
    if c.get("set") in ("DEFAULT", "ENHANCED"):
        text = describe(c)
    else:
        text = c.get("label") or c.get("key")
        if v.get("effect"):
            text += f": {v['effect']}"
    if m.get("edition"):
        text += f" [{m['edition'].lower()}]"
    for flag in ("eternal", "rental"):
        if m.get(flag):
            text += f" [{flag}]"
    if m.get("perishable"):
        text += " [perishable]"
    return text


def current_blind(gs):
    for b in _d(gs.get("blinds")).values():
        if b.get("status") in ("CURRENT", "SELECT"):
            return b
    return {}


def summarize(gs):
    rnd = _d(gs.get("round"))
    blind = current_blind(gs)
    levels = {
        name: f"level {h.get('level')}: {h.get('chips')} chips x {h.get('mult')} mult"
        for name, h in sorted(_d(gs.get("hands")).items(), key=lambda kv: kv[1].get("order", 0))
    }
    jokers, cons = area(gs, "jokers"), area(gs, "consumables")
    return {
        "ante": gs.get("ante_num"),
        "round": gs.get("round_num"),
        "money": gs.get("money"),
        "blind": {
            "name": blind.get("name"),
            "effect": blind.get("effect") or "none",
            "chip_target": blind.get("score"),
        },
        "chips_scored_this_round": rnd.get("chips"),
        "hands_left": rnd.get("hands_left"),
        "discards_left": rnd.get("discards_left"),
        "jokers": [item_desc(c) for c in cards_of(gs, "jokers")],
        "joker_slots": f"{jokers.get('count', 0)}/{jokers.get('limit', 5)}",
        "consumables": [item_desc(c) for c in cards_of(gs, "consumables")],
        "consumable_slots": f"{cons.get('count', 0)}/{cons.get('limit', 2)}",
        "hand_levels": levels,
        "cards_in_hand": [describe(c) for c in cards_of(gs, "hand")],
    }


class JevBot:
    def __init__(self, deck="RED", stake="WHITE", model="jev-latest", log_dir=ROOT / "runs"):
        load_env()
        key = os.environ.get("JEV_API_KEY") or os.environ.get("TYPESAFE_API_KEY")
        if not key:
            raise RuntimeError("JEV_API_KEY missing from .env")
        self.jev = TypeSafeClient(api_key=key, model=model)
        self.game = Balatro()
        self.deck, self.stake, self.model = deck, stake, model
        self.recorder = RunRecorder(log_dir)
        self.auto_new_run = True
        self.events = deque(maxlen=300)
        self.stats = {"decisions": 0, "input_tokens": 0, "output_tokens": 0, "jev_ms": 0,
                      "runs": 0, "wins": 0, "best_ante": 0, "errors": 0}
        self.last_state = None
        self._shop_key, self._shop_actions, self._failed = None, 0, set()

    # ---- plumbing -------------------------------------------------------

    def emit(self, kind, **data):
        ev = {"t": time.time(), "kind": kind, **data}
        self.events.append(ev)
        self.recorder.write(ev)

    def act(self, method, params=None):
        self.emit("action", method=method, params=params)
        return self.game.call(method, params)

    def ask(self, qid, gs, instructions, options):
        """options: {key: (description, (method, params))}. Returns the chosen (method, params)."""
        if len(options) == 1:
            (k, (desc, action)), = options.items()
            self.emit("forced", question=qid, choice=k, desc=desc)
            return action
        state = {"rules": GAME_RULES, "game": summarize(gs)}
        t0 = time.perf_counter()
        resp = self.jev.system_one(
            state=state,
            questions={qid: Choice(instructions=instructions, criteria={k: v[0] for k, v in options.items()})},
        )
        ms = int((time.perf_counter() - t0) * 1000)
        ans = resp.choices[qid]
        self.stats["decisions"] += 1
        self.stats["jev_ms"] += ms
        tin = (resp.usage.input_tokens or 0) if resp.usage else 0
        tout = (resp.usage.output_tokens or 0) if resp.usage else 0
        self.stats["input_tokens"] += tin
        self.stats["output_tokens"] += tout
        self.emit(
            "decision", question=qid, game_state=gs.get("state"), ante=gs.get("ante_num"),
            round=gs.get("round_num"), choice=ans.choice, action=options[ans.choice][1],
            confidence=ans.confidence, probs=dict(ans.probabilities),
            options={k: v[0] for k, v in options.items()}, instructions=instructions, state_sent=state,
            model=resp.model, ms=ms, input_tokens=tin, output_tokens=tout,
        )
        return options[ans.choice][1]

    # ---- one step -------------------------------------------------------

    def step(self):
        gs = self.game.state()
        self.last_state = gs
        st = gs.get("state")
        self.recorder.ensure(gs, self.deck, self.stake, self.model)
        self.recorder.progress(gs)
        handler = getattr(self, f"on_{(st or '').lower()}", None)
        if handler is None:
            time.sleep(TRANSITIONAL_WAIT)  # animation / transitional state
            return False
        try:
            handler(gs)
            self._error_streak = 0
        except RPCError as e:
            self.stats["errors"] += 1
            self._error_streak = getattr(self, "_error_streak", 0) + 1
            self.emit("error", msg=f"{st}: {e}")
            if self._error_streak >= 5:
                self._error_streak = 0
                raise RuntimeError(f"5 game errors in a row in {st}; pausing") from e
            time.sleep(0.5)
        return True

    def on_menu(self, gs):
        if not self.auto_new_run:
            time.sleep(0.5)
            return
        self.emit("info", msg=f"Starting new run ({self.deck} deck, {self.stake} stake)")
        self.act("start", {"deck": self.deck, "stake": self.stake})

    def on_game_over(self, gs):
        self.stats["runs"] += 1
        self.stats["wins"] += bool(gs.get("won"))
        self.stats["best_ante"] = max(self.stats["best_ante"], gs.get("ante_num") or 0)
        self.emit("run_end", won=bool(gs.get("won")), ante=gs.get("ante_num"), round=gs.get("round_num"))
        self.recorder.finish("won" if gs.get("won") else "lost", gs)
        self.act("menu")

    def on_round_eval(self, gs):
        self.act("cash_out")

    def on_blind_select(self, gs):
        b = current_blind(gs)
        if b.get("type") == "BOSS":
            self.act("select")
            return
        opts = {
            "play": (f"Play the {b.get('name')} (need {b.get('score')} chips) and earn its cash reward", ("select", None)),
            "skip": (f"Skip the {b.get('name')} to gain the {b.get('tag_name')}: {b.get('tag_effect')}. "
                     "Skipping forgoes the blind's money and shop visit.", ("skip", None)),
        }
        self.act(*self.ask("blind", gs, "Should we play or skip the upcoming blind?", opts))

    def on_selecting_hand(self, gs):
        rnd = _d(gs.get("round"))
        target = current_blind(gs).get("score") or 0
        need = max(0, target - (rnd.get("chips") or 0))
        hand = cards_of(gs, "hand")
        opts = {}
        for n, p in enumerate(play_candidates(hand, _d(gs.get("hands"))), 1):
            desc = f"Play {p['hand']} scoring {', '.join(p['scoring'])}"
            if p["kicked"]:
                desc += f" (also plays away {', '.join(p['kicked'])})"
            desc += f". Estimated {p['estimate']} chips before jokers; {need} still needed."
            opts[f"play_{n}"] = (desc, ("play", {"cards": p["cards"]}))
        if (rnd.get("discards_left") or 0) > 0:
            for n, d in enumerate(discard_candidates(hand), 1):
                desc = f"Discard: {d['label']}. Throws away {', '.join(d['tossed'])} and draws replacements."
                opts[f"discard_{n}"] = (desc, ("discard", {"cards": d["cards"]}))
        self.act(*self.ask(
            "move", gs,
            "Choose the best next move this round. Weigh playing now against discarding to build a stronger "
            "hand, given the chips still needed, hands left, discards left, and how the jokers boost each hand type.",
            opts,
        ))

    def on_shop(self, gs):
        key = (gs.get("ante_num"), gs.get("round_num"))
        if key != self._shop_key:
            self._shop_key, self._shop_actions, self._failed = key, 0, set()
        money = gs.get("money") or 0
        jok, cons = area(gs, "jokers"), area(gs, "consumables")
        jokers_full = jok.get("count", 0) >= jok.get("limit", 5)
        cons_full = cons.get("count", 0) >= cons.get("limit", 2)
        opts = {"leave": ("Leave the shop and go to the next blind, keeping the money", ("next_round", None))}
        if self._shop_actions < SHOP_ACTION_CAP:
            for i, c in enumerate(cards_of(gs, "shop")):
                cost = _d(c.get("cost")).get("buy", 0)
                if cost > money or (c.get("set") == "JOKER" and jokers_full) or (
                        c.get("set") in ("TAROT", "PLANET", "SPECTRAL") and cons_full):
                    continue
                opts[f"buy_card_{i}"] = (f"Buy {item_desc(c)} for ${cost}", ("buy", {"card": i}))
            for i, c in enumerate(cards_of(gs, "vouchers")):
                cost = _d(c.get("cost")).get("buy", 0)
                if cost <= money:
                    opts[f"buy_voucher_{i}"] = (f"Buy voucher {item_desc(c)} for ${cost}", ("buy", {"voucher": i}))
            for i, c in enumerate(cards_of(gs, "packs")):
                cost = _d(c.get("cost")).get("buy", 0)
                if cost <= money:
                    opts[f"buy_pack_{i}"] = (f"Buy and open {item_desc(c)} for ${cost}", ("buy", {"pack": i}))
            reroll = _d(gs.get("round")).get("reroll_cost") or 5
            if reroll <= money:
                opts["reroll"] = (f"Reroll the shop for ${reroll} to see new cards", ("reroll", None))
            if jokers_full:
                for i, c in enumerate(cards_of(gs, "jokers")):
                    sell = _d(c.get("cost")).get("sell", 0)
                    opts[f"sell_joker_{i}"] = (f"Sell joker {item_desc(c)} for ${sell} to free a slot",
                                               ("sell", {"joker": i}))
            for i, c in enumerate(cards_of(gs, "consumables")):
                if c.get("set") == "PLANET":
                    opts[f"use_{i}"] = (f"Use {item_desc(c)} now", ("use", {"consumable": i}))
        opts = {k: v for k, v in opts.items() if json.dumps(v[1]) not in self._failed}
        shop = [f"${_d(c.get('cost')).get('buy', 0)} {item_desc(c)}"
                for name in ("shop", "vouchers", "packs") for c in cards_of(gs, name)]
        self.emit("info", msg=f"Shop (${money}): " + "; ".join(shop))
        action = self.ask("shop", gs, "We are in the shop. What is the single best next shop action to "
                                      "maximise the chance of winning the run?", opts)
        self._shop_actions += 1
        try:
            self.act(*action)
        except RPCError:
            self._failed.add(json.dumps(action))
            raise

    def on_smods_booster_opened(self, gs):
        opts = {"skip": ("Skip the rest of this pack", ("pack", {"skip": True}))}
        for i, c in enumerate(cards_of(gs, "pack")):
            opts[f"take_{i}"] = (f"Take {item_desc(c)}", ("pack", {"card": i}))
        method, params = self.ask("pack", gs, "A booster pack is open. Which card should we take, if any?", opts)
        try:
            self.act(method, params)
        except RPCError as e:
            if params.get("skip"):
                raise
            # Tarot/Spectral that needs hand targets: aim at the first two cards, else give up on the pack.
            self.emit("info", msg=f"Pack pick needs targets ({e}); retrying with targets")
            try:
                self.act("pack", {**params, "targets": [0, 1]})
            except RPCError:
                try:
                    self.act("pack", {**params, "targets": [0]})
                except RPCError:
                    self.act("pack", {"skip": True})
