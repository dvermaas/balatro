"""Game loop: read BalatroBot state -> build candidate moves -> Jev picks one -> execute."""

import json
import os
import time
from collections import deque
from pathlib import Path

from typesafe_sdk import Choice, TypeSafeClient

import benchmark
import strategy
import tarots
from balatro import Balatro, RPCError
from hands import _d, describe, discard_candidates, play_candidates
from prompts import GAME_RULES
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


def make_game(backend=None):
    """The game client: BalatroBot over HTTP (default) or the headless Jackdaw sim
    (JEV_BACKEND=sim, see src/sim_backend.py). Both speak the same call()/state() interface."""
    backend = (backend or os.environ.get("JEV_BACKEND") or "balatrobot").lower()
    if backend in ("sim", "jackdaw"):
        from sim_backend import SimBalatro
        return SimBalatro()
    if backend in ("balatrobot", "real", "game"):
        return Balatro()
    raise ValueError(f"unknown JEV_BACKEND {backend!r} (use balatrobot or sim)")


class JevBot:
    def __init__(self, deck="RED", stake="WHITE", model="jev-latest", log_dir=None, game=None, jev=None):
        load_env()
        if jev is None:
            key = os.environ.get("JEV_API_KEY") or os.environ.get("TYPESAFE_API_KEY")
            if not key:
                raise RuntimeError("JEV_API_KEY missing from .env")
            jev = TypeSafeClient(api_key=key, model=model)
        self.jev = jev
        self.game = game if game is not None else make_game()
        self.backend = getattr(self.game, "backend", "balatrobot")
        self.error_backoff = 0.5 if self.backend == "balatrobot" else 0.0
        if log_dir is None:  # sim runs live apart so real-game analyses and benchmarks never see them
            log_dir = ROOT / "runs" / ("sim" if self.backend != "balatrobot" else "")
        self.deck, self.stake, self.model = deck, stake, model
        self.recorder = RunRecorder(log_dir, backend=self.backend)
        self.auto_new_run = True
        self.events = deque(maxlen=400)
        self._event_id = 0
        self._last_pack_cost = 0
        self.bench_queue, self.bench_current, self.bench_done_pending = [], None, False
        self.bench_name = benchmark.NAME
        self._auto_failed = set()
        self._round_blind = None
        self._finished_seed = None  # retry menu cleanup without recording/playing a finished run again
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
        before = self.last_state or {}
        result = self.game.call(method, params)
        if isinstance(result, dict) and "state" in result:
            snap = self._snapshot(result, before)
            if method != "play":
                snap.pop("chips_delta", None)  # only a played hand scores chips
            self.emit("outcome", method=method, **snap)
            self.last_state = result
        return result

    @staticmethod
    def _snapshot(gs, before):
        """Compact after-action facts: lets analytics measure real hand scores, spending and progress."""
        rnd, prev = _d(gs.get("round")), _d(before.get("round"))
        out = {
            "state": gs.get("state"), "ante": gs.get("ante_num"), "round": gs.get("round_num"),
            "money": gs.get("money"), "money_delta": (gs.get("money") or 0) - (before.get("money") or 0),
            "chips": rnd.get("chips"), "hands_left": rnd.get("hands_left"), "discards_left": rnd.get("discards_left"),
            "jokers": [c.get("label") for c in cards_of(gs, "jokers")],
        }
        if before.get("round_num") == gs.get("round_num") and prev.get("chips") is not None:
            out["chips_delta"] = (rnd.get("chips") or 0) - (prev.get("chips") or 0)
        return out

    auto_actions = True  # legacy versions predate strategy.auto_action

    def query(self, gs, state, questions):
        """One Jev call with several typed questions (v6+). Returns (answers, meta) for record_decision()."""
        self.recorder.write_raw(self._event_id + 1, gs)
        t0 = time.perf_counter()
        resp = self.jev.system_one(state=state, questions=questions)
        ms = int((time.perf_counter() - t0) * 1000)
        tin = (resp.usage.input_tokens or 0) if resp.usage else 0
        tout = (resp.usage.output_tokens or 0) if resp.usage else 0
        self.stats["decisions"] += 1
        self.stats["jev_ms"] += ms
        self.stats["input_tokens"] += tin
        self.stats["output_tokens"] += tout
        meta = {"state_sent": state, "ms": ms, "input_tokens": tin, "output_tokens": tout, "model": resp.model,
                "questions": {k: q.model_dump() for k, q in questions.items()},
                "answers": {k: a.model_dump() for k, a in resp.answers.items()}}
        return resp.answers, meta

    def record_decision(self, question, gs, choice, action, options, probs, confidence, meta, display="prob"):
        """Log a combined decision in the same shape as ask(), so the panel and analytics read it alike."""
        self.emit("decision", question=question, game_state=gs.get("state"), ante=gs.get("ante_num"),
                  round=gs.get("round_num"), choice=choice, action=action, confidence=confidence,
                  probs=probs, options=options, display=display, **meta)

    def on_blind_select(self, gs):
        import decisions
        decisions.blind_select(self, gs)

    def on_selecting_hand(self, gs):
        import decisions
        decisions.selecting_hand(self, gs)

    def on_shop(self, gs):
        import decisions
        decisions.shop(self, gs)

    def on_smods_booster_opened(self, gs):
        import decisions
        decisions.booster(self, gs)

    def build_state(self, gs):
        return {"rules": GAME_RULES, "game": summarize(gs)}

    def ask(self, qid, gs, instructions, options):
        """options: {key: (description, (method, params))}. Returns the chosen (method, params)."""
        if len(options) == 1:
            (k, (desc, action)), = options.items()
            self.emit("forced", question=qid, choice=k, desc=desc)
            return action
        state = self.build_state(gs)
        self.recorder.write_raw(self._event_id + 1, gs)  # id the decision event below will get
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
        if self._finished_seed:
            if st == "MENU":
                self._finished_seed = None
            elif gs.get("seed") == self._finished_seed:
                self.act("menu")
                return True
            else:
                raise RuntimeError("Unexpected game while returning a finished run to the menu")
        self.recorder.ensure(gs, self.deck, self.stake, self.model, self.bench_current)
        if st == "SELECTING_HAND":
            self._round_blind = current_blind(gs)
            self._round_had_bones = any(c.get("label") == "Mr. Bones" for c in cards_of(gs, "jokers"))
        self.recorder.progress(gs)
        handler = getattr(self, f"on_{(st or '').lower()}", None)
        if handler is None:
            time.sleep(TRANSITIONAL_WAIT)  # animation / transitional state
            return False
        try:
            auto = (strategy.auto_action(gs) or tarots.auto_use(gs)) if self.auto_actions else None
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
            time.sleep(getattr(self, "error_backoff", 0.5))
        return True

    def start_benchmark(self, name=None):
        self.bench_name = name if name in benchmark.SETS else benchmark.NAME
        self.bench_queue = list(enumerate(benchmark.SETS[self.bench_name]))
        self.emit("info", msg=f"Benchmark {self.bench_name}: {len(self.bench_queue)} seeded runs queued")

    def on_menu(self, gs):
        params = {"deck": self.deck, "stake": self.stake}
        self.bench_current = None
        if self.bench_queue and self._clearly_worse():
            self.emit("info", msg=f"Benchmark stopped early: {self._clearly_worse()}")
            self.bench_queue, self.bench_done_pending = [], False
            raise BenchmarkComplete()
        if self.bench_queue:
            idx, seed = self.bench_queue.pop(0)
            params["seed"] = seed.split("#")[0]  # "SEED#n" labels a repeat of SEED
            seeds = benchmark.SETS[self.bench_name]
            self.bench_current = {"name": self.bench_name, "index": idx, "of": len(seeds)}
            self.emit("info", msg=f"Benchmark run {idx + 1}/{len(seeds)} (seed {seed})")
        elif self.bench_done_pending:
            self.bench_done_pending = False
            self.emit("info", msg=f"Benchmark {self.bench_name} complete")
            raise BenchmarkComplete()
        elif not self.auto_new_run:
            time.sleep(0.5)
            return
        else:
            self.emit("info", msg=f"Starting new run ({self.deck} deck, {self.stake} stake)")
        self.bench_done_pending = bool(self.bench_current) and not self.bench_queue
        self.act("start", params)

    EARLY_STOP_AFTER = 10
    FUTILITY_AFTER = 15  # by half-way, trailing by more than 1 SE means it is very unlikely to end up ahead

    def _clearly_worse(self):
        """After 10 benchmark seeds, compare seed by seed with the best finished version: stop if the
        current version trails by more than 2 standard errors (v6 was obvious after 6 seeds)."""
        import statistics
        import analytics
        m = analytics.benchmark_matrix(self.bench_name, backend=getattr(self, "backend", "balatrobot"),
                                       root=getattr(self.recorder, "root", None))
        me = next((v for v in m["versions"] if v["version"] == self.recorder.version), None)
        done = [v for v in m["versions"] if v is not me and v["done"] == v["of"]]
        if not me or me["done"] < self.EARLY_STOP_AFTER or not done:
            return None
        base = max(done, key=lambda v: v["avg_progress"] or 0)
        d = [me["cells"][s]["progress"] - base["cells"][s]["progress"] for s in me["cells"]
             if s in base["cells"] and me["cells"][s]["progress"] is not None
             and base["cells"][s]["progress"] is not None]
        if len(d) < self.EARLY_STOP_AFTER:
            return None
        mean, se = statistics.mean(d), statistics.stdev(d) / len(d) ** 0.5
        if mean < -2 * se or (len(d) >= self.FUTILITY_AFTER and mean < -se):
            return f"{me['label']} trails {base['label']} by {abs(mean):.2f} ± {se:.2f} blinds over {len(d)} seeds"
        return None

    def _finish_run(self, gs, won):
        if self._finished_seed != gs.get("seed"):
            self._finished_seed = gs.get("seed")
            self.stats["runs"] += 1
            self.stats["wins"] += won
            self.stats["best_ante"] = max(self.stats["best_ante"], gs.get("ante_num") or 0)
            self.emit("run_end", won=won, reported_won=bool(gs.get("won")),
                      ante=gs.get("ante_num"), round=gs.get("round_num"))
            self.recorder.finish("won" if won else "lost", gs)
        # menu clears the victory overlay and queued events. Do not cash out,
        # buy from a shop, or play endless rounds after a benchmark victory.
        self.act("menu")

    def on_game_over(self, gs):
        # The installed game sets won=True even on an ante-8 boss LOSS. Only
        # a prior advance past ante 8 proves an old endless-mode run was won.
        # New runs finish at the successful ROUND_EVAL below.
        won = bool(gs.get("won")) and (gs.get("ante_num") or 0) > 8
        self._finish_run(gs, won)

    def on_round_eval(self, gs):
        rnd, blind = _d(gs.get("round")), self._round_blind or {}  # already marked defeated here
        self.emit("round_end", ante=gs.get("ante_num"), round=gs.get("round_num"), blind=blind.get("name"),
                  target=blind.get("score"), scored=rnd.get("chips"), hands_left=rnd.get("hands_left"),
                  discards_left=rnd.get("discards_left"), money=gs.get("money"),
                  jokers=[c.get("label") for c in cards_of(gs, "jokers")])
        if gs.get("won") and (gs.get("ante_num") or 0) > 8:
            target = blind.get("score") or 0
            # Mr. Bones saving the ante-8 boss is a win in the real game: end_round sets game_won
            # whenever that round ends without a game over (state_events.lua). The save consumes him.
            bones_save = getattr(self, "_round_had_bones", False) and not any(
                c.get("label") == "Mr. Bones" for c in cards_of(gs, "jokers"))
            if blind.get("type") != "BOSS" or target <= 0 or ((rnd.get("chips") or 0) < target and not bones_save):
                raise RuntimeError("Victory reported without a verified final-boss clear; pausing for review")
            # Preserve the actual winning state even though there is no next Jev question.
            self.recorder.write_raw(self._event_id + 1, gs)
            self._finish_run(gs, True)
            return
        self.act("cash_out")
