"""Freeze a public-information development set, without reading draw-pile contents.

Run from the repository root: .venv/Scripts/python scripts/diagnostics/positions.py
Selection uses stable hashes within predeclared strata, not outcomes. The two
known failures are explicitly labelled regression cases, not a random sample.
"""
import gzip
import hashlib
import json
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VERSION = "11845ce8"
OUTPUT = ROOT / "docs/samples/diagnostic-positions-v1.json"
KNOWN = {("20261014-194148-JV8CL6BQ", 4157): "pareidolia_interaction",
         ("20261014-141234-WMAZ7HWJ", 8112): "raised_fist_debuff"}


def mapping(value):
    return value if isinstance(value, dict) else {}


def public_card(card):
    """Explicit allowlist: no opaque IDs or internal per-card RNG/state."""
    fields = {k: card[k] for k in ("key", "label", "set") if k in card}
    for key, allowed in {
        "value": ("rank", "suit", "effect"),
        "modifier": ("enhancement", "edition", "seal", "eternal", "perishable", "rental"),
        "state": ("debuff", "hidden", "face_down", "facing"),
        "cost": ("buy", "sell"),
    }.items():
        fields[key] = {k: v for k, v in mapping(card.get(key)).items() if k in allowed}
    return fields


def public_observation(gs):
    """Do not touch gs['cards']; that contains ordered future draws."""
    out = {k: gs[k] for k in ("ante_num", "round_num", "money", "state", "deck", "stake") if k in gs}
    out["round"] = {k: v for k, v in mapping(gs.get("round")).items()
                    if k in ("hands_left", "discards_left", "hands_played", "discards_used", "chips", "reroll_cost")}
    for area in ("hand", "jokers", "shop", "consumables", "packs", "vouchers"):
        src = mapping(gs.get(area))
        out[area] = {k: src[k] for k in ("count", "limit", "highlighted_limit") if k in src}
        out[area]["cards"] = [public_card(c) for c in src.get("cards", [])]
    out["hands"] = {name: {k: v for k, v in h.items()
                            if k in ("level", "chips", "mult", "played", "played_this_round")}
                    for name, h in mapping(gs.get("hands")).items()}
    out["blinds"] = {name: {k: v for k, v in b.items()
                             if k in ("name", "status", "type", "score", "effect")}
                     for name, b in mapping(gs.get("blinds")).items()}
    return out


def collect_candidates():
    candidates = []
    for path in sorted((ROOT / "runs").glob("*/summary.json")):
        summary = json.loads(path.read_text(encoding="utf-8"))
        if (summary.get("prompt_version") != VERSION or summary.get("result") not in ("won", "lost")
                or (summary.get("benchmark") or {}).get("name") != "bench30-a"):
            continue
        pending = None
        for line in path.with_name("decisions.jsonl").read_text(encoding="utf-8").splitlines():
            event = json.loads(line)
            if event["kind"] in ("decision", "forced"):
                pending = None
                if event["kind"] != "decision":
                    continue
                row = {"source": {"run": path.parent.name, "event": event["id"], "version": VERSION},
                       "recorded": {k: event[k] for k in ("question", "ante", "round", "choice", "action",
                                    "state_sent", "questions", "answers", "options", "model") if k in event}}
                if event["question"] == "shop":
                    row["stratum"] = "shop"
                    candidates.append(row)
                elif event["question"] == "move" and (event.get("action") or [None])[0] == "play":
                    row["stratum"] = ("early" if event["ante"] <= 2 else "boss"
                        if event.get("state_sent", {}).get("round", {}).get("blind", "").startswith("The ")
                        else "later")
                    pending = row
            elif event["kind"] == "outcome" and event.get("method") == "play" and pending:
                pending["actual"] = {"chips_delta": event.get("chips_delta"), "after_state": event.get("state")}
                candidates.append(pending)
                pending = None
    return candidates


def freeze():
    if OUTPUT.exists():
        raise SystemExit(f"Already frozen: {OUTPUT}. Use the existing set; never silently reselect it.")
    candidates = collect_candidates()
    selected, used = [], set()
    for row in candidates:
        key = row["source"]["run"], row["source"]["event"]
        if key in KNOWN:
            row["stratum"] = "known_regression"
            row["regression"] = KNOWN[key]
            selected.append(row)
            used.add(key)
    assert len(selected) == 2
    for stratum, count in {"early": 8, "boss": 10, "later": 10, "shop": 10}.items():
        runs = set()
        pool = sorted((r for r in candidates if r["stratum"] == stratum), key=lambda r:
            hashlib.sha256(f"diagnostic-v1:{r['source']['run']}:{r['source']['event']}".encode()).hexdigest())
        chosen = []
        for row in pool:
            key = row["source"]["run"], row["source"]["event"]
            if key in used or key[0] in runs:
                continue
            chosen.append(row)
            used.add(key)
            runs.add(key[0])
            if len(chosen) == count:
                break
        assert len(chosen) == count, (stratum, len(chosen))
        selected.extend(chosen)
    by_run = defaultdict(dict)
    for row in selected:
        by_run[row["source"]["run"]][row["source"]["event"]] = row
    for run, wanted in by_run.items():
        with gzip.open(ROOT / "runs" / run / "states.jsonl.gz", "rt", encoding="utf-8") as source:
            for line in source:
                envelope = json.loads(line)
                if envelope["id"] in wanted:
                    # Immediately discard everything except the public allowlist.
                    wanted[envelope["id"]]["observation"] = public_observation(envelope["gs"])
        assert all("observation" in row for row in wanted.values()), run
    for i, row in enumerate(selected, 1):
        row["case_id"] = f"D{i:02}"
    doc = {"schema": 1, "purpose": "Fixed development diagnostics, not an unbiased win-rate evaluation",
           "selection": "Two known regressions; then SHA256(diagnostic-v1:run:event), one run per stratum, independent of outcomes",
           "privacy": "Public allowlisted observation only; no seed, RNG, draw-pile order or future shops supplied to reference engine",
           "strata": dict(Counter(r["stratum"] for r in selected)), "positions": selected}
    OUTPUT.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"path": str(OUTPUT), "positions": len(selected), "runs": len(by_run), "strata": doc["strata"]}))


if __name__ == "__main__":
    freeze()
