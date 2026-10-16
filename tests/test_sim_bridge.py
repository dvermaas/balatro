"""v24: the shop forecast's rollouts on rumbot-sim (jev_sim, src/sim_bridge.py) equal the Python ones.

A few lockstep shop states x every forecast option (buy, voucher, sell-and-buy, an Invisible Joker
sell): rollout.run_options must give identical rows with and without jev_sim. The full checks are
scripts/diagnostics/rumbot_{joker,rollout,forecast}_parity.py. Skips when jev_sim isn't built.
Run: .venv/Scripts/python -m unittest discover -s tests -p test_sim_bridge.py -v
"""
import os
import sys
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "scripts" / "diagnostics"))

try:
    import jev_sim  # noqa: F401
    import rumbot_forecast_parity as fp
    import rumbot_rollout_parity as rp
    import rollout
    import sim_bridge
    from jackdaw.engine.card_factory import create_joker
    HAVE = True
except ImportError:
    HAVE = False


@unittest.skipUnless(HAVE and os.environ.get("JEV_NATIVE") != "0", "jev_sim or Jackdaw not available")
class RumbotRolloutsMatchJackdaw(unittest.TestCase):
    def test_run_options_identical(self):
        states = rp.shop_states("TSB00001")[:2] + rp.shop_states("TSB00002")[:2]
        self.assertTrue(states)
        texts = [{"label": "J", "value": {"effect": e}, "modifier": {}} for e in rp.EFFECTS[:3]]
        checked = 0
        for n, gs in enumerate(states):
            gs["_rollout_no_shop"] = True
            gs["dollars"] = max(gs.get("dollars", 0), 30)
            if n == 0:
                j = create_joker("j_invisible")
                j.ability["invis_rounds"] = 3
                j.add_to_deck(gs)
                gs["jokers"].append(j)
            snap = rollout.Snapshot(gs)
            out = {}
            for native in (False, True):
                opts = fp.options(gs)
                pols = {o["key"]: rollout.Policy(texts) for o in opts}
                avail = sim_bridge.available
                sim_bridge.available = (lambda: True) if native else (lambda: False)
                try:
                    out[native] = rollout.run_options(snap, opts, pols, 6, 2, time.time(), None)
                finally:
                    sim_bridge.available = avail
            self.assertEqual(out[False], out[True])
            checked += len(out[True][0])
        self.assertGreater(checked, 8)


if __name__ == "__main__":
    unittest.main()
