import sys, json
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent / "fixtures"))
import states, rollout, rollout_items as ri
from jackdaw.engine.rng import PseudoRandom
from jackdaw.engine.actions import OpenBooster
from jackdaw.engine.game import step
rows = states.read_jsonl(states.DATA / "fixtures" / "fc_packs.jsonl.gz")
for r in rows:
    if r["key"] == "buy_pack_0" and r["k"] == 0 and r["taken"] == ["j_brainstorm"]:
        gs = rollout.Snapshot(ri.build_state(r["pub"])).copy(); gs["rng"] = PseudoRandom(r["seed"])
        step(gs, OpenBooster(card_index=0)); pk = ri._area_cards(gs, "pack_cards"); pc = ri.Picker(r["pub"])
        print(json.dumps(pk)[:1500]); print("money", gs["dollars"], "jok", [j.get("key") for j in ri._area_cards(gs, "jokers")], gs.get("joker_slots"))
        print([v for v in pc.values(pk, ri._area_cards(gs, "hand"), gs["dollars"], ri._area_cards(gs, "jokers"), gs.get("joker_slots", 5))])
        break
