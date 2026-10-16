"""Build/load an isolated two-rule scorer correction without editing src/.

The exact unified diff is the review artifact. String anchors fail closed if the
baseline changes. Production imports never see this module or its candidate.
"""
import difflib
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parents[2]
FILES = ("prompts.py", "hands.py", "strategy.py", "decisions.py", "jokers.py", "odds.py", "build.py", "tarots.py", "readiness.py")
V18_JOKERS = "26e3df41ddd4"  # sha1 prefix of src/jokers.py at v18 d22adb59
sys.path.insert(0, str(ROOT / "src"))


def _v18_jokers():
    """src/jokers.py at v18 (d22adb59): the working copy while it is unchanged, else from git history
    (v23 rewrote the rules as data for the native kernel)."""
    path = ROOT / "src" / "jokers.py"
    if hashlib.sha1(path.read_bytes()).hexdigest()[:12] == V18_JOKERS:
        return path.read_bytes()
    revs = subprocess.run(["git", "log", "--format=%H", "-n", "50", "--", "src/jokers.py"], cwd=ROOT,
                          capture_output=True, text=True).stdout.split()
    for rev in revs:
        blob = subprocess.run(["git", "show", f"{rev}:src/jokers.py"], cwd=ROOT, capture_output=True).stdout
        if hashlib.sha1(blob).hexdigest()[:12] == V18_JOKERS:
            return blob
    raise RuntimeError("Expected the frozen v18 jokers.py baseline (not in the working copy or git history)")


def candidate_text():
    # The candidate patches jokers.py only: pin that file to its v18 (d22adb59) bytes, so later prompt
    # versions keep the diagnostic usable.
    original = _v18_jokers().decode("utf-8")
    replacements = [
        ('return lambda i: i["rank"] in FACES',
         'return lambda i: i.get("is_face", i["rank"] in FACES)'),
        ('rule["hand"] = lambda ctx: (0, 2 * min([_nominal(i) for i in ctx["held"]] or [0]), 1)',
         'rule["raised_fist"] = True  # Individual held-card effect, including retriggers.'),
        ('return {"label": c.get("label"), "rule": parse(eff), "edition": _d(c.get("modifier")).get("edition"),',
         'return {"label": c.get("label"), "rule": parse(eff), "edition": _d(c.get("modifier")).get("edition"),\n'
         '            "all_face": c.get("label") == "Pareidolia" and not _d(c.get("state")).get("debuff"),'),
        ('    played = [infos[i] for i in combo]\n    held = [infos[i] for i in range(len(infos)) if i not in combo]',
         '    all_face = any(j.get("all_face") for j in jokers)\n'
         '    infos = [{**i, "is_face": all_face or i["rank"] in FACES} for i in infos]\n'
         '    played = [infos[i] for i in combo]\n'
         '    held = [infos[i] for i in range(len(infos)) if i not in combo]'),
        ('played[p]["rank"] in FACES and not played[p]["debuff"]',
         'played[p]["is_face"] and not played[p]["debuff"]'),
        ('    for c in held:\n        if c["debuff"]:',
         '    # The last lowest-ranked non-Stone card triggers Raised Fist. A debuffed\n'
         '    # lowest card blocks it; a higher live card cannot replace that card.\n'
         '    ranked = [n for n, c in enumerate(held) if c["rank"] and c["enh"] != "STONE"]\n'
         '    lowest = min(ranked, key=lambda n: (RANK_ORDER.index(held[n]["rank"]), -n)) if ranked else None\n'
         '    for held_index, c in enumerate(held):\n        if c["debuff"]:'),
        ('            for r in rules:\n                for pred, mm, xx in r["held"]:',
         '            for r in rules:\n'
         '                if r.get("raised_fist") and held_index == lowest:\n'
         '                    mult += 2 * _nominal(c)\n'
         '                for pred, mm, xx in r["held"]:'),
    ]
    revised = original
    for before, after in replacements:
        if revised.count(before) != 1:
            raise RuntimeError(f"Patch anchor changed or ambiguous: {before[:80]}")
        revised = revised.replace(before, after)
    return original, revised


def candidate_module():
    _, text = candidate_text()
    module = ModuleType("diagnostic_jokers_candidate")
    module.__file__ = str(ROOT / "runs/_experiments/mechanics-candidate/jokers.py")
    exec(compile(text, module.__file__, "exec"), module.__dict__)
    return module


def write_artifacts():
    old, new = candidate_text()
    diff = "".join(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                     fromfile="a/src/jokers.py", tofile="b/src/jokers.py"))
    (ROOT / "docs/samples/mechanics-candidate.patch").write_text(diff, encoding="utf-8")
    folder = ROOT / "runs/_experiments/mechanics-candidate"
    folder.mkdir(exist_ok=True, parents=True)
    (folder / "jokers.py").write_text(new, encoding="utf-8")
    print(json.dumps({"baseline": "d22adb59", "candidate_sha256": hashlib.sha256(new.encode()).hexdigest(),
                      "patch": "docs/samples/mechanics-candidate.patch", "production_changed": False}))


if __name__ == "__main__":
    write_artifacts()
