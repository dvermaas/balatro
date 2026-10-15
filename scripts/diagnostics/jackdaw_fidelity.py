"""Jackdaw full-game fidelity audit (offline; research venv only).

Replays recorded real-game runs (runs/<id>/decisions.jsonl action events) through
Jackdaw's in-process SimBackend, which speaks the BalatroBot JSON-RPC, and diffs the
simulated gamestate against every recorded BalatroBot gamestate (states.jsonl.gz).

Recorded draw order / shop contents are used only for this offline comparison; nothing
here feeds a policy.

Usage (from the repo root):
  runs/_research/python/Scripts/python.exe scripts/diagnostics/jackdaw_fidelity.py \
      [--set bench30-a] [--runs N] [--out docs/samples/jackdaw-fidelity.json]
"""
from __future__ import annotations

import argparse
import collections
import glob
import gzip
import json
import os
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "runs", "_research", "jackdaw"))

from jackdaw.bridge.backend import RPCError, SimBackend  # noqa: E402

STATE_ALIASES = {"SMODS_BOOSTER_OPENED": "BOOSTER_OPENED"}


def real_profile_locked():
    """Center keys still locked on the real Balatro profile (meta.jkr, profile 1).

    Lua's get_current_pool drops `v.unlocked == false` centers (except rarity-4 jokers) to
    UNAVAILABLE, which shifts every pseudorandom_element pick. Jackdaw assumes a fully
    unlocked profile, so we inject the locked keys as banned keys (same UNAVAILABLE effect)."""
    import re
    import zlib
    from jackdaw.engine.profile import _get_locked_items
    from jackdaw.engine.data.prototypes import JOKERS
    raw = open(os.path.join(os.environ.get("APPDATA", ""), "Balatro", "1", "meta.jkr"), "rb").read()
    txt = zlib.decompress(raw, -15).decode("utf8", "replace")
    m = re.search(r'\["unlocked"\]=\{(.*?)\}', txt, re.S)
    unl = set(re.findall(r'\["([a-z]_[a-z0-9_]+)"\]=true', m.group(1))) if m else set()
    m = re.search(r'\["discovered"\]=\{(.*?)\}', txt, re.S)
    disc = set(re.findall(r'\["([a-z]+_[a-z0-9_]+)"\]=true', m.group(1))) if m else set()
    locked = sorted(k for k in _get_locked_items() if k not in unl
                    and not (k in JOKERS and getattr(JOKERS[k], "rarity", 0) == 4))
    return locked, sorted(disc)


_PATCHED = {"locked": None, "discovered": None}

# Profile drift: centers unlocked on the real profile during the recorded period. meta.jkr only
# shows today's profile, so runs recorded before the unlock must treat the key as locked.
# j_throwback: last run that never offered it 20261013-071851, first run offering it 20261013-191409
# (its unlock is "continue a run from the main menu", i.e. resume tooling).
PROFILE_HISTORY = {"j_throwback": "20261013-0719"}


def patch_profile(locked, discovered=None):
    from jackdaw.engine import pools
    if "orig" not in _PATCHED:
        _PATCHED["orig"] = pools.get_current_pool

        def wrapped(pool_type, rng, ante, **kw):
            lk = _PATCHED["locked"]
            if pool_type == "Tag" and _PATCHED["discovered"] is not None:
                # Lua: a Tag with `requires` is only in pool if that center is discovered
                kw["discovered"] = _PATCHED["discovered"]
            if lk and pool_type not in ("Tag", "Enhanced"):
                b = dict.fromkeys(kw.get("banned_keys") or (), True)
                b.update(dict.fromkeys(lk, True))
                kw["banned_keys"] = b
            return _PATCHED["orig"](pool_type, rng, ante, **kw)
        pools.get_current_pool = wrapped
    _PATCHED["locked"] = set(locked) if locked else None
    _PATCHED["discovered"] = set(discovered) if discovered is not None else None


def _d(x):
    return x if isinstance(x, dict) else {}


def _norm_state(s):
    return STATE_ALIASES.get(s, s)


def _ed(v):
    if not v:
        return None
    return str(v).upper()


def _packkind(k):
    # p_buffoon_normal_2 -> p_buffoon_normal: the art variant of the guaranteed first-shop
    # Buffoon pack comes from unseeded math.random in the real game
    return k.rsplit("_", 1)[0] if k and k[-1].isdigit() else k


def card_sig(c, playing=True):
    m = _d(c.get("modifier"))
    sig = [c.get("key"), _ed(m.get("edition")), _ed(m.get("seal"))]
    if playing:
        sig.append(_ed(m.get("enhancement")))
    return tuple(sig)


def area(gs, name):
    return _d(gs.get(name)).get("cards") or []


def snapshot(gs):
    """Order-sensitive, comparison-ready projection of a BalatroBot gamestate."""
    rnd = _d(gs.get("round"))
    blinds = _d(gs.get("blinds"))
    snap = {
        "state": _norm_state(gs.get("state")),
        "ante": gs.get("ante_num"),
        "round_num": gs.get("round_num"),
        "money": gs.get("money"),
        # hands/discards are only meaningful inside a round (the real game shows 4/4 at blind select)
        "round": (rnd.get("hands_left"), rnd.get("discards_left"), rnd.get("chips"))
        if gs.get("state") == "SELECTING_HAND" else None,
        "reroll_cost": rnd.get("reroll_cost"),
        "blinds": {k: (_d(blinds.get(k)).get("name"), _d(blinds.get(k)).get("tag_name") or None,
                       _d(blinds.get(k)).get("score")) for k in ("small", "big", "boss")},
        "hand": [card_sig(c) for c in area(gs, "hand")],
        "deck": [card_sig(c) for c in area(gs, "cards")],
        "jokers": [card_sig(c, False) for c in area(gs, "jokers")],
        "consumables": [c.get("key") for c in area(gs, "consumables")],
        "hand_levels": {k: (_d(v).get("level"), _d(v).get("chips"), _d(v).get("mult"), _d(v).get("played"))
                        for k, v in _d(gs.get("hands")).items()},
        "used_vouchers": sorted(_d(gs.get("used_vouchers")).keys()) if isinstance(gs.get("used_vouchers"), dict)
        else sorted(gs.get("used_vouchers") or []),
    }
    st = snap["state"]
    if st == "SHOP":
        for name in ("shop", "vouchers", "packs"):
            snap[name] = [(_packkind(c.get("key")) if name == "packs" else c.get("key"), _d(c.get("cost")).get("buy"), _ed(_d(c.get("modifier")).get("edition")))
                          for c in area(gs, name)]
    if st == "BOOSTER_OPENED":
        snap["pack"] = [card_sig(c, c.get("set") in ("DEFAULT", "ENHANCED")) for c in area(gs, "pack")]
    return snap


# Fields whose divergence changes what a policy sees / can do (vs cosmetic).
CRITICAL = ("state", "ante", "money", "round", "hand_multiset", "deck_multiset", "jokers", "consumables",
            "shop", "vouchers", "packs", "pack", "blinds", "hand_levels")


def diff(rec, sim):
    out = {}
    for k, v in rec.items():
        if k in ("hand", "deck"):
            if v != sim.get(k):
                sv = sim.get(k) or []
                if k == "hand":
                    out[k + "_order"] = {"rec": [c[0] for c in v], "sim": [c[0] for c in sv]}
                else:  # draw pile: report the first differing position counted from the top (end)
                    i = next((i for i in range(1, min(len(v), len(sv)) + 1) if v[-i] != sv[-i]), None)
                    out[k + "_order"] = {"first_diff_from_top": i, "rec_len": len(v), "sim_len": len(sv)}
            if collections.Counter(v) != collections.Counter(sim.get(k) or []):
                out[k + "_multiset"] = {"rec_only": sorted(map(str, (collections.Counter(v) - collections.Counter(sim.get(k) or [])).elements()))[:8],
                                        "sim_only": sorted(map(str, (collections.Counter(sim.get(k) or []) - collections.Counter(v)).elements()))[:8]}
            continue
        if k == "hand_levels":
            # Jackdaw omits the three secret hands until unlocked; compare the hands it reports
            bad = {h: (lv, sim[k][h]) for h, lv in v.items() if h in sim.get(k, {}) and sim[k][h] != lv}
            if bad:
                out[k] = bad
            continue
        if v != sim.get(k):
            out[k] = {"rec": v, "sim": sim.get(k)}
    return out


def remap_cards(idx, rec_hand, sim_hand):
    """Map recorded hand indices onto the simulated hand by card identity."""
    used, res = set(), []
    if any(i >= len(rec_hand) for i in idx):
        return None  # recorded hand snapshot is stale (no state captured before this action)
    for i in idx:
        sig = rec_hand[i]
        j = next((j for j, s in enumerate(sim_hand) if s == sig and j not in used), None)
        if j is None:
            return None
        used.add(j)
        res.append(j)
    return res


def apply_joker_perm(sim, perm):
    """Real BalatroBot accepts any permutation; Jackdaw's bridge only does one adjacent swap.
    Realise the permutation as a sequence of adjacent swaps."""
    cur = list(range(len(perm)))
    target = list(perm)
    for i in range(len(target)):
        j = cur.index(target[i])
        while j > i:
            p = list(range(len(perm)))
            p[j - 1], p[j] = p[j], p[j - 1]
            sim.handle("rearrange", {"jokers": p})
            cur[j - 1], cur[j] = cur[j], cur[j - 1]
            j -= 1


def replay(run_dir, remap=True):
    summ = json.load(open(os.path.join(run_dir, "summary.json")))
    events = sorted((json.loads(l) for l in open(os.path.join(run_dir, "decisions.jsonl"))), key=lambda e: e["id"])
    states = {}
    with gzip.open(os.path.join(run_dir, "states.jsonl.gz"), "rt") as f:
        for l in f:
            s = json.loads(l)
            states[s["id"]] = s["gs"]
    # Did the real game accept each action? (an outcome event follows before the next action)
    real_ok = {}
    for i, e in enumerate(events):
        if e.get("kind") == "action":
            ok = False
            for f in events[i + 1:]:
                if f.get("kind") in ("action", "decision", "forced"):
                    break
                if f.get("kind") == "outcome" and f.get("method") == e["method"]:
                    ok = True
                    break
            real_ok[e["id"]] = ok
    base_locked = _PATCHED["locked"]
    if base_locked is not None:
        _PATCHED["locked"] = base_locked | {k for k, t in PROFILE_HISTORY.items() if summ["run_id"] < t}
    try:
        return _replay(run_dir, summ, events, states, real_ok, remap)
    finally:
        _PATCHED["locked"] = base_locked


def _replay(run_dir, summ, events, states, real_ok, remap):
    sim = SimBackend()
    sim_gs = sim.handle("start", {"seed": summ["seed"], "deck": summ.get("deck", "RED"), "stake": summ.get("stake", "WHITE")})
    res = {"run_id": summ["run_id"], "seed": summ["seed"], "result": summ.get("result"), "rec_ante": summ.get("ante"),
           "rec_round": summ.get("round"), "states_recorded": len(states), "states_compared": 0,
           "states_exact": 0, "first": {}, "first_any": None, "first_critical": None, "action_error": None,
           "actions_applied": 0, "remapped_actions": 0, "rejections_matched": 0, "sim_accepted_real_rejected": []}
    last_rec = None
    counts = collections.Counter()
    for e in events:
        if e["id"] in states:
            rec = snapshot(states[e["id"]])
            last_rec = rec
            sim_gs = sim.handle("gamestate", {})
            d = diff(rec, snapshot(sim_gs))
            res["states_compared"] += 1
            if not d:
                res["states_exact"] += 1
            where = {"id": e["id"], "ante": rec["ante"], "round": rec["round_num"], "state": rec["state"]}
            for k, v in d.items():
                counts[k] += 1
                if k not in res["first"]:
                    res["first"][k] = dict(where, detail=v)
            if d and res["first_any"] is None:
                res["first_any"] = dict(where, fields=sorted(d))
            crit = [k for k in d if k in CRITICAL or k.endswith("_multiset")]
            if crit and res["first_critical"] is None:
                res["first_critical"] = dict(where, fields=sorted(crit), detail={k: d[k] for k in crit})
        if e.get("kind") != "action":
            continue
        method, params = e["method"], dict(e.get("params") or {})
        if method == "start":
            continue
        try:
            sim_now = snapshot(sim.handle("gamestate", {}))
            fresh = last_rec and last_rec["state"] == sim_now["state"] and last_rec["round_num"] == sim_now["round_num"]
            if remap and fresh and "cards" in params and method in ("play", "discard", "use"):
                m = remap_cards(params["cards"], last_rec["hand"], sim_now["hand"])
                if m is not None and m != params["cards"]:
                    params["cards"] = m
                    res["remapped_actions"] += 1
            if remap and fresh and "targets" in params and last_rec.get("hand"):
                m = remap_cards(params["targets"], last_rec["hand"], sim_now["hand"])
                if m is not None and m != params["targets"]:
                    params["targets"] = m
                    res["remapped_actions"] += 1
            if not real_ok.get(e["id"], True):
                # the real game rejected this call; the sim should too
                try:
                    sim.handle(method, params)
                    res["sim_accepted_real_rejected"].append({"id": e["id"], "method": method, "params": e.get("params")})
                except RPCError:
                    res["rejections_matched"] += 1
                continue
            if method == "rearrange" and "jokers" in params:
                apply_joker_perm(sim, params["jokers"])
            else:
                sim.handle(method, params)
            res["actions_applied"] += 1
        except (RPCError, Exception) as exc:  # noqa: BLE001
            import traceback
            res["action_error_tb"] = traceback.format_exc(limit=-3)[-600:]
            res["action_error"] = {"id": e["id"], "method": method, "params": e.get("params"),
                                   "error": f"{type(exc).__name__}: {exc}"[:300],
                                   "ante": last_rec and last_rec["ante"], "round": last_rec and last_rec["round_num"]}
            break
    res["diff_counts"] = dict(counts)
    fin = snapshot(sim.handle("gamestate", {}))
    res["sim_end"] = {"state": fin["state"], "ante": fin["ante"], "round": fin["round_num"], "money": fin["money"]}
    return res


def rng_parity(run_dir):
    """Initial state parity: deck order, blinds/tags, first shop (if reached without divergence)."""
    summ = json.load(open(os.path.join(run_dir, "summary.json")))
    with gzip.open(os.path.join(run_dir, "states.jsonl.gz"), "rt") as f:
        first = json.loads(f.readline())["gs"]
    sim = SimBackend()
    g = sim.handle("start", {"seed": summ["seed"], "deck": "RED", "stake": "WHITE"})
    r, s = snapshot(first), snapshot(g)
    return {"seed": summ["seed"], "deck_order_equal": r["deck"] == s["deck"],
            "blinds_equal": r["blinds"] == s["blinds"], "blinds_rec": r["blinds"], "blinds_sim": s["blinds"]}


def coverage(run_dirs):
    """Which content Jackdaw has code for, and which of it our recorded runs touched.

    'implemented' = has a registered handler (jokers/consumables) or is referenced by key in engine
    code outside data/ (bosses/tags/vouchers/enhancements/editions/seals). Keys with no code may
    still be correct when the effect is pure data (e.g. voucher config, base jokers)."""
    import re
    from jackdaw.engine import jokers as jk, consumables as cons
    from jackdaw.engine.data.prototypes import BLINDS, CENTER_POOLS, JOKERS, TAGS
    eng = os.path.join(ROOT, "runs", "_research", "jackdaw", "jackdaw", "engine")
    src = ""
    for fn in glob.glob(os.path.join(eng, "*.py")):
        src += open(fn, encoding="utf8").read()
    from jackdaw.engine.data.prototypes import _load_json
    centers = _load_json("centers.json")

    def mentioned(k):
        names = [k]
        nm = (centers.get(k) or {}).get("name") or getattr(BLINDS.get(k), "name", None)
        if nm:
            names.append(nm)
        return any(re.search(r"['\"]" + re.escape(n) + r"['\"]", src) for n in names)
    jreg = set(jk._REGISTRY) | set(getattr(jk, "_DOLLAR_REGISTRY", {}))
    creg = set(cons._CONSUMABLE_REGISTRY)
    cats = {
        "jokers": {k: (k in jreg or mentioned(k)) for k in JOKERS},
        "tarots": {k: k in creg for k in CENTER_POOLS.get("Tarot", [])},
        "planets": {k: k in creg for k in CENTER_POOLS.get("Planet", [])},
        "spectrals": {k: k in creg for k in CENTER_POOLS.get("Spectral", [])},
        "vouchers": {k: mentioned(k) for k in CENTER_POOLS.get("Voucher", [])},
        "bosses": {k: mentioned(k) for k, b in BLINDS.items() if k not in ("bl_small", "bl_big")},
        "tags": {k: mentioned(k) for k in TAGS},
        "enhancements": {k: mentioned(k) for k in CENTER_POOLS.get("Enhanced", [])},
    }
    name2boss = {b.name: k for k, b in BLINDS.items()} if hasattr(next(iter(BLINDS.values())), "name") else {}
    name2tag = {t.name: k for k, t in TAGS.items()} if hasattr(next(iter(TAGS.values())), "name") else {}
    seen = collections.defaultdict(collections.Counter)
    for d in run_dirs:
        with gzip.open(os.path.join(d, "states.jsonl.gz"), "rt") as f:
            for l in f:
                gs = json.loads(l)["gs"]
                for a in ("jokers", "consumables", "shop", "vouchers", "pack"):
                    for c in area(gs, a):
                        k = c.get("key") or ""
                        cat = {"j": "jokers", "v": "vouchers"}.get(k[:1])
                        if k.startswith("c_"):
                            cat = next((n for n in ("tarots", "planets", "spectrals") if k in cats[n]), None)
                        if cat:
                            seen[cat][k] += 1
                        m = _d(c.get("modifier"))
                        if c.get("set") in ("DEFAULT", "ENHANCED") and m.get("enhancement"):
                            seen["enhancements"]["m_" + str(m["enhancement"]).lower()] += 1
                for c in area(gs, "hand") + area(gs, "cards"):
                    m = _d(c.get("modifier"))
                    if m.get("enhancement"):
                        seen["enhancements"]["m_" + str(m["enhancement"]).lower()] += 1
                for b in _d(gs.get("blinds")).values():
                    b = _d(b)
                    if b.get("type") == "BOSS" and b.get("name") in name2boss:
                        seen["bosses"][name2boss[b["name"]]] += 1
                    if b.get("tag_name") in name2tag:
                        seen["tags"][name2tag[b["tag_name"]]] += 1
    out = {}
    for cat, impl in cats.items():
        missing = sorted(k for k, ok in impl.items() if not ok)
        out[cat] = {"total": len(impl), "implemented": len(impl) - len(missing), "no_code_found": missing,
                    "seen_in_runs": len(seen[cat]),
                    "seen_without_code": sorted(k for k in seen[cat] if k in impl and not impl[k])}
    return out


def speed(n_games=20, seed_prefix="SPD"):
    """Greedy-heuristic policy via the RPC layer: flush or biggest rank group, discard singletons,
    buy the first affordable joker, skip packs, never skip blinds."""
    import random
    rnd = random.Random(0)
    t0, steps, antes = time.perf_counter(), 0, []
    for g in range(n_games):
        sim = SimBackend()
        gs = sim.handle("start", {"seed": f"{seed_prefix}{g:05d}", "deck": "RED", "stake": "WHITE"})
        for _ in range(2000):
            st = gs["state"]
            try:
                if st == "BLIND_SELECT":
                    gs = sim.handle("select", {})
                elif st == "SELECTING_HAND":
                    cards = gs["hand"]["cards"]
                    order = "23456789TJQKA"
                    rk = [order.index(c["value"]["rank"]) if c["value"].get("rank") in order else 0 for c in cards]
                    su = [c["value"].get("suit") for c in cards]
                    suit, cnt = collections.Counter(su).most_common(1)[0]
                    if cnt >= 5:  # flush
                        play = [i for i in range(len(cards)) if su[i] == suit][:5]
                    else:  # biggest rank groups, then high cards
                        rc = collections.Counter(rk)
                        play = sorted(range(len(cards)), key=lambda i: (-rc[rk[i]], -rk[i]))[:5]
                    singles = [i for i in range(len(cards)) if collections.Counter(rk)[rk[i]] == 1 and i not in play[:2]]
                    if gs["round"]["discards_left"] > 0 and cnt < 5 and len(singles) >= 3 and rnd.random() < 0.5:
                        gs = sim.handle("discard", {"cards": sorted(singles, key=lambda i: rk[i])[:5]})
                    else:
                        gs = sim.handle("play", {"cards": sorted(play)})
                elif st == "ROUND_EVAL":
                    gs = sim.handle("cash_out", {})
                elif st == "SHOP":
                    shop = gs["shop"]["cards"]
                    aff = [i for i, c in enumerate(shop) if c["cost"]["buy"] <= gs["money"] and c.get("set") == "JOKER"
                           and gs["jokers"]["count"] < gs["jokers"]["limit"]]
                    gs = sim.handle("buy", {"card": aff[0]}) if aff else sim.handle("next_round", {})
                elif st == "BOOSTER_OPENED":
                    gs = sim.handle("pack", {"skip": True})
                else:
                    break
            except RPCError:
                gs = sim.handle("next_round", {}) if st == "SHOP" else sim.handle("gamestate", {})
                if gs["state"] == st and st != "SHOP":
                    break
            steps += 1
        antes.append(gs["ante_num"])
    dt = time.perf_counter() - t0
    return {"games": n_games, "seconds": round(dt, 2), "steps": steps, "steps_per_sec": round(steps / dt, 1),
            "games_per_sec": round(n_games / dt, 2), "mean_final_ante": round(sum(antes) / len(antes), 2),
            "note": "includes RPC-style serialisation of the full gamestate after every step"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default="bench30-a")
    ap.add_argument("--runs", type=int, default=0, help="limit number of runs (0 = all in set)")
    ap.add_argument("--run", action="append", help="specific run dir(s)")
    ap.add_argument("--speed", type=int, default=20)
    ap.add_argument("--out")
    ap.add_argument("--no-profile", action="store_true", help="Jackdaw default: fully unlocked profile")
    a = ap.parse_args()
    locked, disc = ([], None) if a.no_profile else real_profile_locked()
    patch_profile(locked, disc)
    print(f"profile: {len(locked)} locked keys injected, {len(disc or [])} discovered")
    if a.run:
        dirs = a.run
    else:
        dirs = []
        for f in sorted(glob.glob(os.path.join(ROOT, "runs", "2026*", "summary.json"))):
            s = json.load(open(f))
            if _d(s.get("benchmark")).get("name") == a.set and os.path.exists(os.path.join(os.path.dirname(f), "states.jsonl.gz")):
                dirs.append(os.path.dirname(f))
        if a.runs:
            dirs = dirs[: a.runs]
    rng = [rng_parity(d) for d in dirs]
    reps = []
    for d in dirs:
        r = replay(d)
        reps.append(r)
        fc = r["first_critical"]
        print(f"{r['run_id']} {r['result']} a{r['rec_ante']}r{r['rec_round']}: compared {r['states_compared']}/{r['states_recorded']}"
              f" exact {r['states_exact']} | first critical: {fc and (fc['ante'], fc['round'], fc['state'], fc['fields'])}"
              f" | action error: {r['action_error'] and (r['action_error']['method'], r['action_error']['error'][:90])}")
    out = {"jackdaw_commit": "e66de78855df84755d3af7d9a016129f3449f878", "set": a.set,
           "profile_locked_keys": locked, "profile_discovered_count": len(disc or []), "rng_parity": rng, "replays": reps}
    first_shop = []
    for r, d in zip(reps, dirs):
        fs = r["first"].get("shop") or r["first"].get("vouchers") or r["first"].get("packs")
        first_shop.append(not (fs and fs["ante"] == 1 and fs["round"] == 1))
    crit_causes = collections.Counter(tuple(r["first_critical"]["fields"]) if r["first_critical"] else None for r in reps)
    out["summary"] = {
        "runs": len(reps),
        "initial_deck_order_equal": sum(x["deck_order_equal"] for x in rng),
        "initial_blinds_and_tags_equal": sum(x["blinds_equal"] for x in rng),
        "first_shop_equal": sum(first_shop),
        "runs_without_critical_divergence": sum(r["first_critical"] is None for r in reps),
        "runs_replayed_to_end_cleanly": sum(r["first_critical"] is None and r["action_error"] is None for r in reps),
        "states_compared": sum(r["states_compared"] for r in reps),
        "states_exact": sum(r["states_exact"] for r in reps),
        "states_recorded": sum(r["states_recorded"] for r in reps),
        "actions_applied": sum(r["actions_applied"] for r in reps),
        "real_rejections_matched": sum(r["rejections_matched"] for r in reps),
        "sim_accepted_real_rejected": sum(len(r["sim_accepted_real_rejected"]) for r in reps),
        "first_critical_fields": {str(k): v for k, v in crit_causes.items()},
        "action_errors": dict(collections.Counter(f"{r['action_error']['method']}: {r['action_error']['error'][:60]}"
                                                  for r in reps if r["action_error"])),
    }
    print(json.dumps(out["summary"], indent=1))
    out["coverage"] = coverage(dirs)
    for k, v in out["coverage"].items():
        print("coverage", k, {x: v[x] for x in ("total", "implemented", "seen_in_runs")}, "missing:", v["no_code_found"][:12])
    if a.speed:
        out["speed"] = speed(a.speed)
        print("speed", out["speed"])
    if a.out:
        with open(a.out, "w", newline="\n") as f:
            json.dump(out, f, indent=1, default=str)


if __name__ == "__main__":
    main()
