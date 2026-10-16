"""Policy lab: compare shop valuation policies at scale on the Jackdaw sim, with no JEV calls.

Every arm plays the same fresh seeds with the same play/discard policy: rollout.Policy, the fast
code policy used inside the forecast, which keeps playing when no discards are left (The Water).
Only joker and voucher buying differs between arms.
  gain           v18-like: jokers.gain on recent plays with v18's thresholds (SURE_GAIN, $25 buffer,
                 early joker phase, strong vouchers from surplus, rerolls from surplus)
  forecast       v19 guard rails: rollout.forecast (bosses=2), buy the best option that beats skip by
                 > 2 SE in blinds cleared (sell-swaps of the 2 weakest jokers included), reroll when
                 nothing beats skip by 1 SE and money stays >= $25
  forecast-long  the same with a 3-boss horizon
  none           never buys jokers or vouchers (floor)
Shared by all arms: buy the shop planet for our most played hand when affordable (keeping $4) and
use planets at once; skip packs, tarots, spectrals and playing cards; never skip blinds.

  python scripts/policy_lab.py --seeds 50 --arms gain,forecast,forecast-long,none --name pilot
  python scripts/policy_lab.py --report pilot                     # tables from runs/_policy_lab/pilot
Results stream to runs/_policy_lab/<name>/results.jsonl (resumable: finished games are skipped).
"""
from __future__ import annotations

import argparse
import json
import math
import multiprocessing as mp
import os
import random
import statistics
import sys
import time
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "src"))
OUT = ROOT / "runs" / "_policy_lab"

FORECAST_N = 16
RECENT = 10


# ---------------------------------------------------------------------------
# Shop arms
# ---------------------------------------------------------------------------

def _d(x):
    return x if isinstance(x, dict) else {}


def _cards(pub, area):
    return _d(pub.get(area)).get("cards") or []


class Arm:
    name = "none"

    def __init__(self, cfg):
        self.cfg = cfg

    def decide(self, pub, game):
        return ("leave",)


class GainArm(Arm):
    """v18's code thresholds on jokers.gain (JEV's item scores replaced by the measured gain)."""
    name = "gain"

    def decide(self, pub, game):
        import decisions
        import jokers
        money = pub.get("money") or 0
        jl = _d(pub.get("jokers"))
        owned = _cards(pub, "jokers")
        full = len(owned) >= (jl.get("limit") or 5)
        ante = pub.get("ante_num") or 1
        early = ante <= decisions.EARLY_ANTE and len(owned) < decisions.EARLY_JOKERS
        best = None
        for i, c in enumerate(_cards(pub, "shop")):
            cost = _d(c.get("cost")).get("buy", 0)
            neg = _d(c.get("modifier")).get("edition") == "NEGATIVE"
            if c.get("set") != "JOKER" or cost > money or (full and not neg):
                continue
            g = jokers.gain(pub, game["recent"], add=c)
            if g is None:
                continue
            sure = g >= decisions.SURE_GAIN
            loose = g >= self.cfg["useful_gain"] and (money - cost >= decisions.INTEREST_BUFFER or early)
            early_any = early and g > 0
            if (sure or loose or early_any) and (best is None or g > best[0]):
                best = (g, ("buy", i))
        if full and best is None:
            for j, o in enumerate(owned):
                m = _d(o.get("modifier"))
                if m.get("eternal") or m.get("edition") == "NEGATIVE":
                    continue
                sell = _d(o.get("cost")).get("sell", 0)
                for i, c in enumerate(_cards(pub, "shop")):
                    cost = _d(c.get("cost")).get("buy", 0)
                    if c.get("set") != "JOKER" or cost > money + sell:
                        continue
                    g = jokers.gain(pub, game["recent"], add=c, remove=j)
                    if g is not None and g >= decisions.SURE_GAIN and (best is None or g > best[0]):
                        best = (g, ("replace", j, i))
        if best:
            return best[1]
        for i, c in enumerate(_cards(pub, "vouchers")):
            cost = _d(c.get("cost")).get("buy", 0)
            if decisions.VOUCHERS.get(c.get("label"), (None, False))[1] and money - cost >= decisions.INTEREST_BUFFER:
                return ("voucher", i)
        reroll = _d(pub.get("round")).get("reroll_cost") or 5
        if money - reroll >= decisions.INTEREST_BUFFER and game["rerolls"] < decisions.MAX_REROLLS:
            return ("reroll",)
        return ("leave",)


class ForecastArm(Arm):
    """v19's guard rails on rollout.forecast, without JEV."""
    name = "forecast"

    def decide(self, pub, game):
        import decisions
        import rollout
        money = pub.get("money") or 0
        jl = _d(pub.get("jokers"))
        full = len(_cards(pub, "jokers")) >= (jl.get("limit") or 5)
        weak = decisions._weakest_jokers(pub, game["recent"]) if full else set()
        keys = {o["key"] for o in rollout.options(pub) if o["supported"] and o.get("set") in ("JOKER", "VOUCHER")
                and (not o["key"].startswith("replace_") or int(o["key"].split("_")[1]) in weak)}
        reroll = _d(pub.get("round")).get("reroll_cost") or 5
        can_reroll = money - reroll >= self.cfg["reroll_floor"] and game["rerolls"] < self.cfg["max_rerolls"]
        if not keys:
            return ("reroll",) if can_reroll else ("leave",)
        t = time.time()
        try:
            fc = rollout.forecast(pub, n=self.cfg["n"], bosses=self.cfg["bosses"], keys=keys)
        except Exception:  # noqa: BLE001  like decisions.shop: a failed forecast buys nothing here
            game["forecast_errors"] += 1
            return ("leave",)
        game["forecast_s"] += time.time() - t
        game["forecasts"] += 1
        rows = [(k, r["vs_skip"]["blinds"]) for k, r in fc["options"].items() if k != "skip"]
        rows.sort(key=lambda kv: -kv[1][0])
        if rows and rows[0][1][0] > self.cfg["buy_se"] * max(rows[0][1][1], 1e-9) + self.cfg.get("buy_margin", 0):
            k = rows[0][0]
            p = k.split("_")
            if k.startswith("replace_"):
                return ("replace", int(p[1]), int(p[2]))
            return ("voucher", int(p[2])) if k.startswith("buy_voucher_") else ("buy", int(p[2]))
        if can_reroll and not any(d > self.cfg["reroll_se"] * max(se, 1e-9) for _, (d, se) in rows):
            return ("reroll",)
        return ("leave",)


ARMS = {
    "none": (Arm, {}),
    "gain": (GainArm, {"useful_gain": 0.15}),
    "forecast": (ForecastArm, {"n": FORECAST_N, "bosses": 2, "buy_se": 2, "reroll_se": 1, "reroll_floor": 25,
                               "max_rerolls": 2}),
    "forecast-long": (ForecastArm, {"n": FORECAST_N, "bosses": 3, "buy_se": 2, "reroll_se": 1, "reroll_floor": 25,
                                    "max_rerolls": 2}),
    # diagnosis variants
    "forecast-noreroll": (ForecastArm, {"n": FORECAST_N, "bosses": 2, "buy_se": 2, "reroll_se": 1, "reroll_floor": 25,
                                        "max_rerolls": 0}),
    "forecast-long-noreroll": (ForecastArm, {"n": FORECAST_N, "bosses": 3, "buy_se": 2, "reroll_se": 1,
                                             "reroll_floor": 25, "max_rerolls": 0}),
    "forecast-bank": (ForecastArm, {"n": FORECAST_N, "bosses": 2, "buy_se": 2, "reroll_se": 1, "reroll_floor": 25,
                                    "max_rerolls": 0, "buy_margin": 0.25}),
}


# ---------------------------------------------------------------------------
# One game
# ---------------------------------------------------------------------------

def _shared_planets(gs, pub):
    """Shared rule: buy the shop planet for our most played hand (keep $4), then use held planets."""
    import jokers
    import rollout
    from jackdaw.engine.actions import BuyCard
    from jackdaw.engine.game import IllegalActionError, step
    hands = _d(pub.get("hands"))
    played = max(hands.items(), key=lambda kv: (_d(kv[1]).get("played") or 0, _d(kv[1]).get("level") or 0),
                 default=(None, {}))
    if played[0] and (_d(played[1]).get("played") or 0) > 0:
        for i, c in enumerate(_cards(pub, "shop")):
            lv = jokers.planet_level(_d(c.get("value")).get("effect")) if c.get("set") == "PLANET" else None
            cost = _d(c.get("cost")).get("buy", 0)
            if lv and lv[0] == played[0] and (pub.get("money") or 0) - cost >= 4 \
                    and len(gs.get("consumables", [])) < gs.get("consumable_slots", 2):
                try:
                    step(gs, BuyCard(shop_index=i))
                    return True
                except IllegalActionError:
                    return False
    rollout.use_planets(gs)
    return False


def play_game(task):
    """Play one seed with one arm to the end; returns a result dict (errors are returned, not raised)."""
    seed, arm_name = task
    t0 = time.time()
    try:
        if arm_name == "bot-forecast-long":
            return _play_bot(seed, arm_name, t0)
        return _play(seed, arm_name, t0)
    except Exception as e:  # noqa: BLE001
        return {"seed": seed, "arm": arm_name, "error": f"{type(e).__name__}: {e}",
                "trace": traceback.format_exc(limit=6), "seconds": round(time.time() - t0, 1)}


def _play(seed, arm_name, t0):
    import rollout
    import sim_backend
    from jackdaw.engine.actions import (BuyCard, CashOut, Discard, GamePhase, NextRound, PlayHand, RedeemVoucher,
                                        Reroll, SelectBlind, SellCard, SkipPack)
    from jackdaw.engine.game import IllegalActionError, step
    from jackdaw.engine.run_init import initialize_run
    cls, cfg = ARMS[arm_name]
    arm = cls(cfg)
    gs = initialize_run("b_red", 1, seed)
    gs["phase"], gs["blind_on_deck"] = GamePhase.BLIND_SELECT, "Small"
    game = {"recent": [], "rerolls": 0, "forecast_s": 0.0, "forecasts": 0, "forecast_errors": 0}
    log = {"buys": [], "rerolls": 0, "reroll_spent": 0, "spent": 0, "blinds": 0, "deaths": None}
    pol, pol_sig = None, None
    for _ in range(20000):
        ph = gs["phase"]
        if gs.get("won"):
            break
        if ph == GamePhase.BLIND_SELECT:
            step(gs, SelectBlind())
        elif ph == GamePhase.SELECTING_HAND:
            if not gs["hand"]:  # deck and hand exhausted: no legal play, the run is lost
                break
            sig = tuple((j.center_key, repr(sorted((j.ability or {}).items(), key=str))) for j in gs["jokers"])
            if pol is None or sig != pol_sig:
                jd = sim_backend._area(gs["jokers"], gs.get("joker_slots", 5), gs, "jokers")["cards"]
                pol, pol_sig = rollout.Policy(jd), sig
            kind, idx = pol.decide(gs)
            if kind == "play":
                cr = gs["current_round"]
                game["recent"] = (game["recent"] + [{
                    "infos": [rollout._info(c) for c in gs["hand"]], "combo": list(idx),
                    "rnd": {"hands_left": cr.get("hands_left"), "discards_left": cr.get("discards_left")}}])[-RECENT:]
            try:
                step(gs, (PlayHand if kind == "play" else Discard)(card_indices=tuple(idx)))
            except IllegalActionError:
                forced = [p for p, c in enumerate(gs["hand"]) if c.ability.get("forced_selection")]
                step(gs, PlayHand(card_indices=tuple(forced[:1] or [0])))
        elif ph == GamePhase.ROUND_EVAL:
            log["blinds"] += 1
            step(gs, CashOut())
            game["rerolls"] = 0
        elif ph == GamePhase.SHOP:
            for _a in range(8):
                pub = sim_backend.serialize(gs)
                if _shared_planets(gs, pub):
                    continue
                pub = sim_backend.serialize(gs)
                act = arm.decide(pub, game)
                money = gs["dollars"]
                try:
                    if act[0] == "leave":
                        break
                    if act[0] == "reroll":
                        step(gs, Reroll())
                        game["rerolls"] += 1
                        log["rerolls"] += 1
                        log["reroll_spent"] += money - gs["dollars"]
                    elif act[0] == "voucher":
                        label = _cards(pub, "vouchers")[act[1]].get("label")
                        step(gs, RedeemVoucher(card_index=act[1]))
                        log["buys"].append((pub.get("ante_num"), "voucher", label))
                    elif act[0] == "replace":
                        sold = _cards(pub, "jokers")[act[1]].get("label")
                        label = _cards(pub, "shop")[act[2]].get("label")
                        step(gs, SellCard(area="jokers", card_index=act[1]))
                        step(gs, BuyCard(shop_index=act[2]))
                        log["buys"].append((pub.get("ante_num"), "swap", f"{sold}->{label}"))
                    else:
                        label = _cards(pub, "shop")[act[1]].get("label")
                        step(gs, BuyCard(shop_index=act[1]))
                        log["buys"].append((pub.get("ante_num"), "joker", label))
                    log["spent"] += max(0, money - gs["dollars"])
                except IllegalActionError:
                    break
            rollout.use_planets(gs)
            step(gs, NextRound())
        elif ph == GamePhase.PACK_OPENING:
            step(gs, SkipPack())
        elif ph == GamePhase.GAME_OVER:
            break
        else:
            raise RuntimeError(f"phase {ph}")
    ante = gs["round_resets"]["ante"]
    return {"seed": seed, "arm": arm_name, "won": bool(gs.get("won")), "ante": 8 if gs.get("won") else ante,
            "blinds": log["blinds"], "round": gs.get("round"), "rerolls": log["rerolls"],
            "reroll_spent": log["reroll_spent"], "spent": log["spent"], "buys": log["buys"],
            "forecasts": game["forecasts"], "forecast_errors": game["forecast_errors"], "forecast_s": round(game["forecast_s"], 1),
            "jokers": [j.ability.get("name") for j in gs["jokers"]], "seconds": round(time.time() - t0, 1)}


# ---------------------------------------------------------------------------
# Arm E: the bot's real code path (JevBot + decisions.py) with a code stand-in for JEV
# ---------------------------------------------------------------------------

def _code_jev(bot_ref):
    """JEV replaced by 'take the top-ranked option': Choice -> the first option (the bot lists its best
    measured option first; cash tarot steps -> continue shopping), play_now -> play when the strongest
    play clears or beats the best measured discard, item scores -> 0 except the shared planet rule
    (the most played hand's planet, keeping $4), so shop buying is the forecast's code path alone."""
    sys.path.insert(0, str(ROOT / "scripts"))
    from sim_run import FirstOptionJev
    from typesafe_sdk import SystemOneResponse

    class CodeJev(FirstOptionJev):
        model = "code-top-option"

        def system_one(self, state, questions):
            answers = {}
            for qid, q in questions.items():
                if q.type == "choice":
                    keys = list(q.criteria)
                    c = "continue_shopping" if "continue_shopping" in keys else keys[0]
                    answers[qid] = {"type": "choice", "choice": c, "confidence": 1.0,
                                    "probabilities": {k: float(k == c) for k in keys}}
                elif q.type == "noul":
                    sp = _d(state.get("strongest_play"))
                    bd = _d(state.get("best_draw_by_measurement"))
                    play = sp.get("clears_blind_alone") or "modeled_average_next_play" not in bd or                         (sp.get("estimated_score") or 0) >= (bd.get("modeled_average_next_play") or 0)
                    answers[qid] = {"type": "noul", "noul": 1.0 if play else 0.0}
                else:
                    answers[qid] = {"type": "score", "score": self.item_score(q), "confidence": 1.0, "legend": {},
                                    "probabilities": {}}
            return SystemOneResponse.model_validate({"model": self.model, "answers": answers,
                                                     "usage": {"input_tokens": 0, "output_tokens": 0}})

        def item_score(self, q):
            item = _d(_d(q.instructions).get("item")) if isinstance(q.instructions, dict) else {}
            gs = bot_ref[0].last_state or {}
            hands = _d(gs.get("hands"))
            top = max(hands.items(), key=lambda kv: (_d(kv[1]).get("played") or 0, _d(kv[1]).get("level") or 0),
                      default=(None, {}))
            if item.get("kind") == "planet" and top[0] and (_d(top[1]).get("played") or 0) > 0                     and f"Level up {top[0]} " in (item.get("what") or "") and (item.get("money_after") or 0) >= 4:
                return 4.0
            return 0.0

    return CodeJev()


def _play_bot(seed, arm_name, t0):
    import shutil
    import tempfile
    import decisions
    from jev_bot import BenchmarkComplete, JevBot
    from sim_backend import SimBalatro
    decisions.FORECAST_N, decisions.FORECAST_BUDGET, decisions.FORECAST_BOSSES = FORECAST_N, None, 3  # = arm C
    decisions.VOUCHERS = {k: (v[0], False) for k, v in decisions.VOUCHERS.items()}  # no v18 strong-voucher rule
    tmp = tempfile.mkdtemp(prefix="plab_")
    try:
        ref = [None]
        jev = _code_jev(ref)
        bot = JevBot(deck="RED", stake="WHITE", game=SimBalatro(), jev=jev, log_dir=Path(tmp))
        ref[0] = bot
        bot.model = jev.model
        bot.act("start", {"seed": seed, "deck": "RED", "stake": "WHITE"})
        engine_won = False
        for _ in range(20000):
            try:
                bot.step()
            except BenchmarkComplete:
                break
            except RuntimeError as e:
                # jev_bot's victory guard pauses for review when it can't verify the final boss (e.g. after
                # Hieroglyph moved the ante counter); the engine's own won flag is the ground truth here.
                if "Victory reported" in str(e) and bot.game.gs.get("won"):
                    engine_won = True
                    break
                raise
            if bot.stats["runs"] >= 1 and (bot.last_state or {}).get("state") == "MENU":
                break
        r = bot.recorder
        summ = json.loads((r.dir / "summary.json").read_text(encoding="utf-8"))
        ev = [json.loads(l) for l in open(r.dir / "decisions.jsonl", encoding="utf-8")]
        blinds = sum(1 for e in ev if e["kind"] == "round_end" and (e.get("scored") or 0) >= (e.get("target") or 1))
        rerolls = sum(1 for e in ev if e["kind"] == "action" and e.get("method") == "reroll")
        buys = [(e.get("ante"), "auto", e.get("desc")) for e in ev if e["kind"] == "auto" and e.get("question") == "shop"]
        buys += [(e.get("ante"), "jev", e.get("choice")) for e in ev if e["kind"] == "decision"
                 and e.get("question") == "shop" and str(e.get("choice")).startswith(("buy", "replace"))]
        fs = [e.get("forecast_seconds") or 0 for e in ev if e.get("forecast_seconds")]
        won = summ.get("result") == "won" or engine_won
        return {"seed": seed, "arm": arm_name, "won": won, "ante": 8 if won else summ.get("ante"), "blinds": blinds,
                "round": summ.get("round"), "rerolls": rerolls, "reroll_spent": 0, "spent": 0,
                "buys": [[b[0], "swap" if "replace" in str(b[2]) else "joker", b[2]] for b in buys],
                "forecasts": len(fs), "forecast_errors": 0, "forecast_s": round(sum(fs), 1),
                "game_errors": bot.stats["errors"], "seconds": round(time.time() - t0, 1)}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# ---------------------------------------------------------------------------
# Runner and report
# ---------------------------------------------------------------------------

def seeds(n, salt=20261016):
    import sim_backend
    rng = random.Random(salt)
    return [sim_backend.random_seed(rng) for _ in range(n)]


def load(name):
    p = OUT / name / "results.jsonl"
    rows = [json.loads(l) for l in open(p, encoding="utf-8")] if p.exists() else []
    return [r for r in rows if not r.get("error")], [r for r in rows if r.get("error")]


def paired(rows, a, b, key="ante"):
    by = {}
    for r in rows:
        by.setdefault(r["seed"], {})[r["arm"]] = r
    d = [float(v[a][key]) - float(v[b][key]) for v in by.values() if a in v and b in v]
    if len(d) < 2:
        return None
    return {"n": len(d), "diff": round(statistics.mean(d), 3), "se": round(statistics.stdev(d) / math.sqrt(len(d)), 3),
            "sd": round(statistics.stdev(d), 3)}


def report(name, pairs=None, only=None):
    rows, errors = load(name)
    if only:
        rows = [r for r in rows if r["arm"] in only]
    arms = sorted({r["arm"] for r in rows})
    seeds_all = {}
    for r in rows:
        seeds_all.setdefault(r["seed"], set()).add(r["arm"])
    complete = {s for s, a in seeds_all.items() if a >= set(arms)}
    rows = [r for r in rows if r["seed"] in complete]
    out = {"name": name, "seeds_complete": len(complete), "errors": len(errors), "arms": {}}
    for a in arms:
        rs = [r for r in rows if r["arm"] == a]
        if not rs:
            continue
        deaths = {}
        for r in rs:
            k = "won" if r["won"] else str(r["ante"])
            deaths[k] = deaths.get(k, 0) + 1
        out["arms"][a] = {
            "games": len(rs), "mean_ante": round(statistics.mean(r["ante"] for r in rs), 3),
            "se_ante": round(statistics.stdev(r["ante"] for r in rs) / math.sqrt(len(rs)), 3) if len(rs) > 1 else None,
            "win_rate": round(sum(r["won"] for r in rs) / len(rs), 3),
            "mean_blinds": round(statistics.mean(r["blinds"] for r in rs), 2),
            "death_ante": dict(sorted(deaths.items())),
            "mean_rerolls": round(statistics.mean(r["rerolls"] for r in rs), 2),
            "mean_reroll_spent": round(statistics.mean(r["reroll_spent"] for r in rs), 1),
            "mean_spent": round(statistics.mean(r["spent"] for r in rs), 1),
            "mean_buys": round(statistics.mean(len(r["buys"]) for r in rs), 2),
            "mean_swaps": round(statistics.mean(sum(b[1] == "swap" for b in r["buys"]) for r in rs), 2),
            "mean_vouchers": round(statistics.mean(sum(b[1] == "voucher" for b in r["buys"]) for r in rs), 2),
            "first_buy_ante_mean": round(statistics.mean([r["buys"][0][0] for r in rs if r["buys"] and r["buys"][0][0]] or [0]), 2),
            "mean_game_seconds": round(statistics.mean(r["seconds"] for r in rs), 1),
            "mean_forecasts": round(statistics.mean(r["forecasts"] for r in rs), 1),
            "forecast_seconds_per_call": round(sum(r["forecast_s"] for r in rs) / max(1, sum(r["forecasts"] for r in rs)), 2),
        }
    pairs = pairs or [(x, y) for x, y in (("forecast", "gain"), ("forecast-long", "gain"), ("forecast", "none"),
                                          ("forecast-long", "forecast"), ("gain", "none")) if x in arms and y in arms]
    pairs += [(x, "forecast-long") for x in arms if x.startswith("forecast-long-") and "forecast-long" in arms]
    pairs += [(x, "gain") for x in arms if x.startswith("forecast-long-") and "gain" in arms]
    out["paired"] = {f"{x} - {y}": {"ante": paired(rows, x, y), "blinds": paired(rows, x, y, "blinds"),
                                    "won": paired(rows, x, y, "won")} for x, y in pairs}
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seeds", type=int, default=50)
    ap.add_argument("--seed-offset", type=int, default=0, help="skip the first k generated seeds")
    ap.add_argument("--arms", default="gain,forecast,forecast-long,none")
    ap.add_argument("--name", default="pilot")
    ap.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) - 1))
    ap.add_argument("--report", help="print the report of a run name and exit (restricted to --arms if given)")
    a = ap.parse_args()
    if a.report:
        print(json.dumps(report(a.report, only=set(a.arms.split(",")) if "--arms" in sys.argv else None), indent=1))
        return 0
    d = OUT / a.name
    d.mkdir(parents=True, exist_ok=True)
    done = {(r["seed"], r["arm"]) for r in load(a.name)[0]}
    arms = a.arms.split(",")
    ss = seeds(a.seed_offset + a.seeds)[a.seed_offset:]
    # slow arms first so the pool drains evenly
    order = sorted(arms, key=lambda x: (not x.startswith("forecast"), x))
    tasks = [(s, arm) for arm in order for s in ss if (s, arm) not in done]
    print(f"{len(tasks)} games ({len(done)} done), {a.jobs} workers", flush=True)
    t0 = time.time()
    with open(d / "results.jsonl", "a", encoding="utf-8") as f, mp.get_context("spawn").Pool(a.jobs) as pool:
        for i, r in enumerate(pool.imap_unordered(play_game, tasks), 1):
            f.write(json.dumps(r) + "\n")
            f.flush()
            if i % 25 == 0 or r.get("error"):
                print(json.dumps({"done": i, "of": len(tasks), "elapsed": round(time.time() - t0),
                                  **({"error": r["error"], "seed": r["seed"], "arm": r["arm"]} if r.get("error") else {})}),
                      flush=True)
    print(json.dumps(report(a.name), indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
