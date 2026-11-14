#!/usr/bin/env python3
"""Train an item-level policy network (nets.ItemNet, attention) from scratch on logged labels; export it for jevlab.

    /home/user/rlvenv/bin/python scripts/rl/train_item.py --out runs/_rl/item/clone --labels clone
    /home/user/rlvenv/bin/python scripts/rl/train_item.py --out runs/_rl/item/mix --labels mix

--labels clone  every v77 record: runs/_rl/bc-v77-s2/games (all antes) and the v77 side (ante > K) of
                runs/_rl/dagger/r0, r1 (milestone 9's screen data)
--labels mix    the item analogue of DAgger v1a: iteration 74's own records at antes 1-4 (the network side of
                dagger/r0, r1) and v77's at antes 5+ (the v77 side of r0, r1 and bc-v77-s2 antes 5+)
Split by game as screen_items.py: sha1(seed) % 10 == 0 test, == 1 validation (early stopping on its cross-entropy),
the rest training. Prints and writes OUT/report.json with screen_items.py's metrics on the test games; writes
OUT/model.pt ({"state", "hidden", "schema_hash", "kind": "item"}; train_rl.py --init reads it) and OUT/weights.brlw
(BRLW format 2, native/rlcore/src/net.rs).
"""
import argparse
import hashlib
import json
import os
import sys

import numpy as np
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import nets as N  # noqa: E402
import read_rlb as R  # noqa: E402
import screen_items as S  # noqa: E402
import train_bc as B  # noqa: E402
import train_dagger as D  # noqa: E402


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--labels", choices=["clone", "mix"], required=True)
    ap.add_argument("--epochs", type=int, default=15)
    ap.add_argument("--patience", type=int, default=2)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--bs", type=int, default=512)
    a = ap.parse_args()
    torch.set_num_threads(4)
    os.makedirs(a.out, exist_ok=True)
    dd = ["runs/_rl/dagger/r0", "runs/_rl/dagger/r1"]
    if a.labels == "clone":
        ds, use = S.load(dd, ["runs/_rl/bc-v77-s2/games"])
    else:
        ds, role = D.load_sources(dd, ["runs/_rl/bc-v77-s2/games"])
        use = role > 0
    fams = np.append(B.fam_of(ds["schema"]["vocab"]["names"]), "?")
    data = B.Data(ds)
    h = np.array([int(hashlib.sha1(s.encode()).hexdigest(), 16) % 10 for s in ds["game_seed"]])
    ok = use & (ds["action"] != R.UNEXPRESSIBLE)
    g = ds["game"]
    tr, va, te = (np.where(ok & m)[0] for m in (h[g] >= 2, h[g] == 1, h[g] == 0))
    print(f"{len(ds['game_seed'])} games; records train {len(tr)} val {len(va)} test {len(te)}", flush=True)
    model = N.ItemNet(ds["obs_dim"], ds["n_actions"], N.item_layout(ds["schema"]), attention=True)
    vce = S.train(model, data, tr, va, a.lr, a.bs, a.epochs, a.patience, 0, lambda s: print(s, flush=True))
    lg, ce = S.predict(model, data, te)
    m = S.metrics(lg, ds, te, fams)
    late = ds["ante"][te] >= 5
    m["late"] = S.metrics(lg[late], ds, te[late], fams)
    m.update(val_ce=vce, test_ce=float(ce.mean()), labels=a.labels)
    print(json.dumps(m))
    torch.save({"state": model.state_dict(), "hidden": model.hidden, "schema_hash": ds["schema_hash"], "kind": "item"}, os.path.join(a.out, "model.pt"))
    N.export_weights(model, os.path.join(a.out, "weights.brlw"), ds["schema_hash"], ds["obs_dim"], ds["n_actions"])
    json.dump(m, open(os.path.join(a.out, "report.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
