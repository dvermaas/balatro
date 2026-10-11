"""Fixed benchmarks: every prompt version plays the same seeds, so results compare like for like.

Never edit a set's seeds; add a new set instead. bench30-a starts with bench10-a's 10 seeds.
"""

SETS = {
    "bench10-a": ["7X7WSDCX", "6CAXN9I6", "BAPQJMY5", "TBIR2XMU", "PX8GK4E6",
                  "WWYTV5PP", "DHPB8Q1L", "XUTRVN1Y", "2RGEX6EN", "V98SL5G2"],
    "bench30-a": ["7X7WSDCX", "6CAXN9I6", "BAPQJMY5", "TBIR2XMU", "PX8GK4E6",
                  "WWYTV5PP", "DHPB8Q1L", "XUTRVN1Y", "2RGEX6EN", "V98SL5G2",
                  "WMAZ7HWJ", "W4YA1M4U", "U8AC5647", "8XNG984V", "SDNUURBN",
                  "A7YBUVWD", "X62IZYJ4", "JRE681Y8", "75WVKF42", "I9Q96AKF",
                  "LRFULSLB", "8GWNFCSN", "MW6PBTW4", "5V1F9XQ2", "2S38V5NR",
                  "2CM51JLD", "UR3ZTT24", "C3XPB74B", "JV8CL6BQ", "57AXFNDP"],
    # BalatroBench's setup (Red deck, White stake, like ours): 5 seeds x 3 runs, for comparison with
    # their LLM leaderboard (balatrobench.com, v1.0.8, final round reached). "SEED#n" = repeat n.
    "balatrobench-15": [f"{s}#{n}" for s in ("AAAAAAA", "BBBBBBB", "CCCCCCC", "DDDDDDD", "EEEEEEE")
                        for n in (1, 2, 3)],
}
NAME = "bench30-a"  # the set the "Benchmark" button runs
SEEDS = SETS[NAME]
