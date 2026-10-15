"""Copy Balatro's English localization out of a local game install (for the sim backend).

The game's text is not ours to redistribute, so it is never committed. The sim backend loads it
at runtime from $BALATRO_LOC_DIR (default: runs/_loc/). On a cloud box, copy these files there.

  python scripts/extract_localization.py [--exe PATH_TO_Balatro.exe] [--smods PATH_TO_smods] [--out DIR]

Writes:
  <out>/en-us.lua        localization/en-us.lua from the game's love archive (Balatro.exe is a zip)
  <out>/game-hands.lua  the default poker-hand table from game.lua (BalatroBot reports its `example`s)
  <out>/smods-en-us.lua  Steamodded's localization/en-us.lua overlay (BalatroBot runs under SMODS,
                         which adds/overrides a few description strings), if found
"""
import argparse
import os
import shutil
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_EXE = r"C:\Program Files (x86)\Steam\steamapps\common\Balatro\Balatro.exe"
DEFAULT_SMODS = Path(os.environ.get("APPDATA", "")) / "Balatro" / "Mods" / "smods"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=DEFAULT_EXE, help="Balatro.exe (or the .love archive / macOS Resources/Balatro.love)")
    ap.add_argument("--smods", default=str(DEFAULT_SMODS), help="Steamodded mod folder")
    ap.add_argument("--out", default=os.environ.get("BALATRO_LOC_DIR") or str(ROOT / "runs" / "_loc"))
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(a.exe) as z:
        (out / "en-us.lua").write_bytes(z.read("localization/en-us.lua"))
        game = z.read("game.lua").decode("utf8")
    print("wrote", out / "en-us.lua")
    # G.GAME.hands defaults (order/level/example): BalatroBot reports each hand's `example`
    i = game.index("hands = {", game.index("function Game:init_game_object"))
    j, depth = i + len("hands = "), 0
    for j in range(j, len(game)):
        depth += {"{": 1, "}": -1}.get(game[j], 0)
        if depth == 0:
            break
    (out / "game-hands.lua").write_text("return " + game[i + len("hands = "): j + 1] + "\n", encoding="utf8")
    print("wrote", out / "game-hands.lua")
    smods = Path(a.smods) / "localization" / "en-us.lua"
    if smods.exists():
        shutil.copyfile(smods, out / "smods-en-us.lua")
        print("wrote", out / "smods-en-us.lua")
    else:
        print("no Steamodded localization found at", smods, "(optional)", file=sys.stderr)


if __name__ == "__main__":
    main()
