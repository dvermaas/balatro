#!/usr/bin/env python3
"""dash_data.py OUT_DIR: the three data files of the progress dashboard's "Reinforcement learning" section.

    rl_tests.json  each step's result on c2k 0-449 (runs/_policy_lab/rl-*), in the order tried, plus v77
    rl_curve.json  held-out argmax wins at the checkpoints of the self-play runs (runs/_rl/ppo/*/curve.jsonl)
    rl_early.json  per iteration of the run in progress: wins among its sampled training games

Upload each as an asset of the dashboard and point the dataset of the same name at it. Add a step to STEPS, a run to
CURVES or EARLY when there is one.
"""
import json
import math
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
LAB = os.path.join(ROOT, "runs/_policy_lab")
PPO = os.path.join(ROOT, "runs/_rl/ppo")

# key, label, who, lab run, note
STEPS = [
    ("copy", "Copy of v77's decisions", "Policy alone", "rl-clone4w5-nors", "Network trained only to imitate v77; the starting point for learning from play."),
    ("ppo_flat", "First network, learning from play", "Policy alone", "rl-ppoa61-nors", "46,400 games of self-play (best checkpoint)."),
    ("dagger", "Taught v77's late-game choices", "Policy alone", "rl-dg-v1a", "Agreement with v77 rose, wins fell."),
    ("item_copy", "New network that scores jokers, copy only", "Policy alone", "rl-it-mix", "Picks v77's joker far more often, wins no more."),
    ("ppo_item", "New network, learning from play", "Policy alone", "rl-ppoitem40-nors", "29,600 games of self-play (best checkpoint)."),
    ("hybrid", "Policy for antes 1–4, v77 after", "Policy early, v77 late", "rl-ho40-p14", "The policy's early game is level with v77's."),
    ("early", "Antes 1–4 trained further, v77 after", "Policy early, v77 late", "rl-ho-e2it9-p14", "10,000 more games on the early game only. Over 900 seeds it wins exactly as many as before the training (530 against 530)."),
    ("v77_nors", "v77 without round search", "v77", "rl-v77-nors", "Same settings as the policies above."),
    ("hybrid_rs", "Policy for antes 1–4 inside the full bot", "Policy early, v77 late", "rl-ho40-p14-rs", "With round search, so compare with v77 (full). Over 900 seeds: 544 against v77's 561, within the noise, at 35% less time per game."),
    ("v77", "v77 (full)", "v77", "c2k-v77", "With round search."),
]
CURVES = [("rl-ppo-a", "First network"), ("rl-ppo-item", "New network (scores jokers)")]
EARLY = [("rl-ppo-early", "Small steps"), ("rl-ppo-early2", "3× larger steps")]


def lab_rows(name, limit=450):
    with open(os.path.join(LAB, name, "results.jsonl")) as f:
        return [json.loads(l) for l in f if l.strip()][:limit]


def main(out):
    os.makedirs(out, exist_ok=True)
    tests = []
    for i, (key, step, who, run, note) in enumerate(STEPS):
        r = lab_rows(run)
        n, w = len(r), sum(1 for x in r if x.get("won"))
        lost = [x["ante"] for x in r if not x.get("won")]
        tests.append(dict(order=i + 1, key=key, step=step, who=who, games=n, wins=w, win_rate=round(w / n, 4),
                          mean_ante=round(sum(x["ante"] for x in r) / n, 3), died_ante_1_4=sum(a <= 4 for a in lost),
                          died_ante_5_7=sum(5 <= a <= 7 for a in lost), died_ante_8=sum(a >= 8 for a in lost), note=note, run=run))
    curve = []
    for run, label in CURVES:
        games = 0
        for l in open(os.path.join(PPO, run, "curve.jsonl")):
            r = json.loads(l)
            if r["phase"] == "ppo":
                games += r["sampled_games"]
            if "eval_win" in r:
                curve.append(dict(network=label, run=run, games_trained=games, heldout_games=r["eval_games"],
                                  heldout_wins=round(r["eval_win"] * r["eval_games"]), heldout_win_rate=round(r["eval_win"], 4),
                                  heldout_mean_ante=round(r["eval_ante"], 3)))
    early, it = [], 0
    for run, label in EARLY:
        p = os.path.join(PPO, run, "curve.jsonl")
        for l in open(p) if os.path.exists(p) else []:
            r = json.loads(l)
            it += 1
            w, n = r["sampled_win"], r["sampled_games"]
            early.append(dict(iteration=it, run=run, step_size=label, phase=r["phase"], games=n, sampled_wins=round(w * n),
                              sampled_win_rate=round(w, 4), se=round(math.sqrt(w * (1 - w) / n), 4), mean_ante=round(r["sampled_ante"], 3)))
    for name, rows in (("rl_tests", tests), ("rl_curve", curve), ("rl_early", early)):
        with open(os.path.join(out, name + ".json"), "w") as f:
            json.dump(rows, f, indent=1)
        print(name, len(rows))


if __name__ == "__main__":
    main(sys.argv[1])
