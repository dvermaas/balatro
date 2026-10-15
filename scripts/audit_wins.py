"""Read-only audit of recorded wins against scored rounds and public game states.

The installed game can set won=True on an ante-8 boss loss. Never treat that
terminal flag alone as victory evidence. This audit is for the standard ante-8
win condition used by our benchmarks; ambiguous evidence stays unverified.
"""
import gzip
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def classify(seed, events, states):
    public = [s for s in states if s.get("seed") == seed]
    clear = next((e for e in events if e.get("kind") == "round_end"
                  and (e.get("ante") or 0) >= 9 and (e.get("target") or 0) > 0
                  and (e.get("scored") or 0) >= e["target"]), None)
    continued = any(s.get("won") is True and s.get("state") not in ("MENU", "GAME_OVER")
                    and (s.get("ante_num") or 0) >= 9 for s in public)
    if clear and continued:
        return {"verdict": "verified_win", "round_end_event": clear["id"],
                "blind": clear.get("blind"), "scored": clear["scored"], "target": clear["target"]}
    last = public[-1] if public else {}
    play = next((e for e in reversed(events) if e.get("kind") == "outcome"
                 and e.get("method") == "play"), {})
    blinds = last.get("blinds") or {}
    boss = blinds.get("boss") or {} if isinstance(blinds, dict) else {}
    target = boss.get("score") or 0
    if (not continued and last.get("won") is False and last.get("state") == "SELECTING_HAND"
            and play.get("state") == "GAME_OVER" and (play.get("ante") or 0) >= 8
            and last.get("ante_num") == play.get("ante")
            and last.get("round_num") == play.get("round") and boss.get("status") == "CURRENT"
            and target > 0 and play.get("chips") is not None and play["chips"] < target):
        return {"verdict": "false_positive", "last_play_event": play["id"],
                "blind": boss.get("name"), "scored": play["chips"], "target": target,
                "reason": "Lost the final boss below target; won=False in the last preterminal state"}
    return {"verdict": "unverified", "reason": "Insufficient explicit evidence; requires manual review"}


def audit_run(folder):
    events = [json.loads(l) for l in (folder / "decisions.jsonl").read_text(encoding="utf-8").splitlines()]
    summary = json.loads((folder / "summary.json").read_text(encoding="utf-8"))
    states = []
    with gzip.open(folder / "states.jsonl.gz", "rt", encoding="utf-8") as f:
        for line in f:
            gs = json.loads(line)["gs"]
            # Retain only outcome evidence; never inspect or use draw-pile order.
            states.append({k: gs.get(k) for k in ("seed", "state", "won", "ante_num", "round_num", "blinds")})
    return {"run_id": summary["run_id"], "seed": summary["seed"], "version": summary["prompt_version"],
            "recorded_result": summary["result"], **classify(summary["seed"], events, states)}


def audit_all(root=ROOT / "runs"):
    result = []
    for path in sorted(root.glob("*/summary.json")):
        row = json.loads(path.read_text(encoding="utf-8"))
        if row["result"] == "won" or row.get("win_audit"):
            result.append(audit_run(path.parent))
    return result


if __name__ == "__main__":
    print(json.dumps(audit_all(), indent=2))
