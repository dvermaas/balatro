"""Game loop: read BalatroBot state -> build candidate moves -> Jev picks one -> execute."""

import json
import os
import time
from collections import deque
from pathlib import Path

from typesafe_sdk import Choice, TypeSafeClient

import benchmark
import strategy
from balatro import Balatro, RPCError
from hands import _d, describe, discard_candidates, play_candidates
from prompts import GAME_RULES, INSTRUCTIONS, OPTIONS
from recorder import RunRecorder

ROOT = Path(__file__).resolve().parent.parent  # repo root: .env and runs/
SHOP_ACTION_CAP = 8
TRANSITIONAL_WAIT = 0.25


class BenchmarkComplete(Exception):
    """Raised at the menu after the last benchmark run so the controller can stop."""


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


def item_desc(c, mains=None):
    """Card text for Jev. Jokers get role/synergy tags when the run's main hands are given."""
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
    if mains is not None and c.get("set") == "JOKER":
        text += f" [{'; '.join(strategy.joker_tags(v.get('effect'), mains))}]"
    elif mains and c.get("set") == "PLANET":
        hit = [h for h in mains if f"Level up {h}" in (v.get("effect") or "")]
        text += f" [levels {hit[0]}, a main hand]" if hit else " [levels a hand we rarely play]"
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
    need = max(0, (blind.get("score") or 0) - (rnd.get("chips") or 0))
    hands_left = rnd.get("hands_left") or 0
    mains = strategy.main_hands(gs)
    return {
        "ante": gs.get("ante_num"),
        "round": gs.get("round_num"),
        "money": gs.get("money"),
        "interest_per_round": strategy.interest(gs.get("money")),
        "engine": strategy.engine([_d(c.get("value")).get("effect") for c in cards_of(gs, "jokers")]),
        "main_hands": mains or "none yet",
        "blind": {
            "name": blind.get("name"),
            "effect": blind.get("effect") or "none",
            "chip_target": blind.get("score"),
        },
        "chips_scored_this_round": rnd.get("chips"),
        "chips_still_needed": need,
        "chips_needed_per_remaining_hand": round(need / hands_left) if hands_left else None,
        "hands_left": rnd.get("hands_left"),
        "discards_left": rnd.get("discards_left"),
        "jokers": [item_desc(c, mains) for c in cards_of(gs, "jokers")],
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
        self.events = deque(maxlen=400)
        self._event_id = 0
        self._last_pack_cost = 0
        self.bench_queue, self.bench_current, self.bench_done_pending = [], None, False
        self._auto_failed = set()
        self.stats = {"decisions": 0, "input_tokens": 0, "output_tokens": 0, "jev_ms": 0,
                      "runs": 0, "wins": 0, "best_ante": 0, "errors": 0}
        self.last_state = None
        self._shop_key, self._shop_actions, self._failed = None, 0, set()

    # ---- plumbing -------------------------------------------------------

    def emit(self, kind, **data):
        self._event_id += 1
        ev = {"id": self._event_id, "t": time.time(), "kind": kind, **data}
        self.events.append(ev)
        self.recorder.write(ev)

    def act(self, method, params=None):
        self.emit("action", method=method, params=params)
        return self.game.call(method, params)

    auto_actions = True  # legacy versions predate strategy.auto_action

    def build_state(self, gs):
        return {"rules": GAME_RULES, "game": summarize(gs)}

    def ask(self, qid, gs, instructions, options):
        """options: {key: (description, (method, params))}. Returns the chosen (method, params)."""
        if len(options) == 1:
            (k, (desc, action)), = options.items()
            self.emit("forced", question=qid, choice=k, desc=desc)
            return action
        state = self.build_state(gs)
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
        self.recorder.ensure(gs, self.deck, self.stake, self.model, self.bench_current)
        self.recorder.progress(gs)
        handler = getattr(self, f"on_{(st or '').lower()}", None)
        if handler is None:
            time.sleep(TRANSITIONAL_WAIT)  # animation / transitional state
            return False
        try:
            auto = strategy.auto_action(gs) if self.auto_actions else None
            auto_key = auto and json.dumps([auto[0], auto[1], gs.get("round_num"), st])
            if auto and auto_key not in self._auto_failed:
                method, params, reason = auto
                self.emit("auto", msg=reason)
                try:
                    self.act(method, params)
                except RPCError:
                    self._auto_failed.add(auto_key)
                    raise
            else:
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

    def start_benchmark(self):
        self.bench_queue = list(enumerate(benchmark.SEEDS))
        self.emit("info", msg=f"Benchmark {benchmark.NAME}: {len(self.bench_queue)} seeded runs queued")

    def on_menu(self, gs):
        params = {"deck": self.deck, "stake": self.stake}
        self.bench_current = None
        if self.bench_queue:
            idx, seed = self.bench_queue.pop(0)
            params["seed"] = seed
            self.bench_current = {"name": benchmark.NAME, "index": idx, "of": len(benchmark.SEEDS)}
            self.emit("info", msg=f"Benchmark run {idx + 1}/{len(benchmark.SEEDS)} (seed {seed})")
        elif self.bench_done_pending:
            self.bench_done_pending = False
            self.emit("info", msg=f"Benchmark {benchmark.NAME} complete")
            raise BenchmarkComplete()
        elif not self.auto_new_run:
            time.sleep(0.5)
            return
        else:
            self.emit("info", msg=f"Starting new run ({self.deck} deck, {self.stake} stake)")
        self.bench_done_pending = bool(self.bench_current) and not self.bench_queue
        self.act("start", params)

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
        fields = {k: b.get(k) for k in ("name", "score", "tag_name", "tag_effect")}
        opts = {
            "play": (OPTIONS["blind_play"].format(**fields), ("select", None)),
            "skip": (OPTIONS["blind_skip"].format(**fields), ("skip", None)),
        }
        self.act(*self.ask("blind", gs, INSTRUCTIONS["blind"], opts))

    def on_selecting_hand(self, gs):
        rnd = _d(gs.get("round"))
        need = max(0, (current_blind(gs).get("score") or 0) - (rnd.get("chips") or 0))
        hands_left = rnd.get("hands_left") or 1
        pace = max(1, round(need / hands_left))
        hand = cards_of(gs, "hand")
        opts = {}
        for n, p in enumerate(play_candidates(hand, _d(gs.get("hands"))), 1):
            if p["estimate"] >= need:
                verdict = OPTIONS["verdict_clears"].format(need=need)
            else:
                verdict = OPTIONS["verdict_pace"].format(
                    pct=round(100 * p["estimate"] / pace), pace=pace, need=need, hands=hands_left)
            kicked = OPTIONS["play_kicked"].format(cards=", ".join(p["kicked"])) if p["kicked"] else ""
            desc = OPTIONS["play"].format(hand=p["hand"], scoring=", ".join(p["scoring"]), kicked=kicked,
                                          estimate=p["estimate"], verdict=verdict)
            opts[f"play_{n}"] = (desc, ("play", {"cards": p["cards"]}))
        if (rnd.get("discards_left") or 0) > 0:
            for n, d in enumerate(discard_candidates(hand), 1):
                desc = OPTIONS["discard"].format(label=d["label"], cards=", ".join(d["tossed"]))
                opts[f"discard_{n}"] = (desc, ("discard", {"cards": d["cards"]}))
        self.act(*self.ask("move", gs, INSTRUCTIONS["move"], opts))

    def on_shop(self, gs):
        key = (gs.get("ante_num"), gs.get("round_num"))
        if key != self._shop_key:
            self._shop_key, self._shop_actions, self._failed = key, 0, set()
        money = gs.get("money") or 0
        jok, cons = area(gs, "jokers"), area(gs, "consumables")
        jokers_full = jok.get("count", 0) >= jok.get("limit", 5)
        cons_full = cons.get("count", 0) >= cons.get("limit", 2)
        mains = strategy.main_hands(gs)

        def after(cost):
            left = money - cost
            before_i, after_i = strategy.interest(money), strategy.interest(left)
            note = f"leaves ${left}"
            if after_i != before_i:
                note += f", interest drops ${before_i} -> ${after_i}/round"
            return note

        opts = {"leave": (OPTIONS["shop_leave"].format(interest=strategy.interest(money)), ("next_round", None))}
        if self._shop_actions < SHOP_ACTION_CAP:
            for i, c in enumerate(cards_of(gs, "shop")):
                cost = _d(c.get("cost")).get("buy", 0)
                if cost > money or (c.get("set") == "JOKER" and jokers_full) or (
                        c.get("set") in ("TAROT", "PLANET", "SPECTRAL") and cons_full):
                    continue
                opts[f"buy_card_{i}"] = (OPTIONS["shop_buy_card"].format(
                    item=item_desc(c, mains), cost=cost, after=after(cost)), ("buy", {"card": i}))
            for i, c in enumerate(cards_of(gs, "vouchers")):
                cost = _d(c.get("cost")).get("buy", 0)
                if cost <= money:
                    opts[f"buy_voucher_{i}"] = (OPTIONS["shop_buy_voucher"].format(
                        item=item_desc(c), cost=cost, after=after(cost)), ("buy", {"voucher": i}))
            for i, c in enumerate(cards_of(gs, "packs")):
                cost = _d(c.get("cost")).get("buy", 0)
                if cost <= money:
                    opts[f"buy_pack_{i}"] = (OPTIONS["shop_buy_pack"].format(
                        item=item_desc(c), cost=cost, after=after(cost)), ("buy", {"pack": i}))
            reroll = _d(gs.get("round")).get("reroll_cost") or 5
            if money - reroll >= 25:  # reroll only from surplus above the interest buffer
                opts["reroll"] = (OPTIONS["shop_reroll"].format(cost=reroll, after=after(reroll)), ("reroll", None))
            if jokers_full:
                # One combined option per (owned joker, shop joker): Jev never chained a sell and a buy.
                for j, owned in enumerate(cards_of(gs, "jokers")):
                    if _d(owned.get("modifier")).get("eternal"):
                        continue
                    sell = _d(owned.get("cost")).get("sell", 0)
                    for i, c in enumerate(cards_of(gs, "shop")):
                        cost = _d(c.get("cost")).get("buy", 0)
                        if c.get("set") == "JOKER" and cost <= money + sell:
                            opts[f"replace_{j}_{i}"] = (OPTIONS["shop_replace"].format(
                                old=item_desc(owned, mains), sell=sell, new=item_desc(c, mains), cost=cost,
                                after=after(cost - sell)), ("replace", {"joker": j, "card": i}))
        opts = {k: v for k, v in opts.items() if json.dumps(v[1]) not in self._failed}
        shop = [f"${_d(c.get('cost')).get('buy', 0)} {item_desc(c)}"
                for name in ("shop", "vouchers", "packs") for c in cards_of(gs, name)]
        self.emit("info", msg=f"Shop (${money}): " + "; ".join(shop))
        action = self.ask("shop", gs, INSTRUCTIONS["shop"], opts)
        self._shop_actions += 1
        if action[0] == "buy" and "pack" in (action[1] or {}):
            self._last_pack_cost = _d(cards_of(gs, "packs")[action[1]["pack"]].get("cost")).get("buy", 0)
        try:
            if action[0] == "replace":
                self.act("sell", {"joker": action[1]["joker"]})
                self.act("buy", {"card": action[1]["card"]})
            else:
                self.act(*action)
        except RPCError:
            self._failed.add(json.dumps(action))
            raise

    def on_smods_booster_opened(self, gs):
        jok = area(gs, "jokers")
        jokers_full = jok.get("count", 0) >= jok.get("limit", 5)
        opts = {}
        for i, c in enumerate(cards_of(gs, "pack")):
            if c.get("set") == "JOKER" and jokers_full:
                continue
            opts[f"take_{i}"] = (OPTIONS["pack_take"].format(item=item_desc(c, strategy.main_hands(gs))),
                                 ("pack", {"card": i}))
        opts["skip"] = (OPTIONS["pack_skip"].format(paid=self._last_pack_cost), ("pack", {"skip": True}))
        method, params = self.ask("pack", gs, INSTRUCTIONS["pack"], opts)
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
