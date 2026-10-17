"""Static tables for native/bot/src/engine (card centers, hand examples, tags) from Jackdaw data.
Run from the main repo checkout (needs third_party/jackdaw and runs/_loc):
  PY=C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe
  $PY scripts/port/engine_tables.py <out.json>   (cwd = C:/Users/admin/Documents/Repos/balatro)"""
import json, sys
sys.path[:0] = ["src"]
import sim_backend  # sets up the jackdaw path
import sim_text
from jackdaw.engine.data.prototypes import TAGS, BLINDS
sim_backend.apply_patches()
sim_text.loc()
centers = {}
for k in list(sim_text.center.__globals__.get("_CENTERS") or {}) or []:
    pass
sim_text.center("c_base")
for k, v in sim_text._CENTERS.items():
    if k[:2] in ("j_", "c_", "v_", "p_"):
        centers[k] = {"name": v.get("name"), "effect": v.get("effect"), "set": v.get("set")}
out = {
    "hand_examples": {k: (v or {}).get("example") or [] for k, v in sim_text.game_hands().items()},
    "centers": centers,
    "enh_label": sim_text._ENH_LABEL,
    "tags": {k: getattr(t, "name", "") for k, t in TAGS.items()},
    "blinds": {k: b.name for k, b in BLINDS.items()},
}
json.dump(out, open(sys.argv[1], "w"), indent=0)
print(len(centers), len(out["tags"]), len(out["hand_examples"]))
