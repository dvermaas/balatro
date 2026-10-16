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
    # Development pilot fixed before v16: persistent weak seeds, a regression, two strong controls.
    # Selected from bench30-a history; never present this as a held-out win-rate evaluation.
    "weak7-a": ["75WVKF42", "DHPB8Q1L", "PX8GK4E6", "JV8CL6BQ", "2RGEX6EN", "2CM51JLD", "BAPQJMY5"],
    # Fresh seeds drawn 2026-10-16 for the all-unlocked profile and the Jackdaw sim (sim30-a);
    # never used for development before its first baseline. Not comparable with bench30-a (locked pool).
    "sim30-a": ["TU9KANLF", "MGBZKPLC", "YZAI217Z", "2YLF77B5", "1YJ4DQ2R",
                  "XFIKZ36U", "665WDGX8", "UDBU5JA1", "11BLBC1T", "6CLKMFPY",
                  "1SHJTVG7", "EV2CS4TX", "Z9DXS1JX", "6L3CIXAF", "R8ZXJN6L",
                  "MZ5KZ3JH", "AEQ18EUN", "HXGTTFCH", "3XFPWBMH", "61QF3Z7R",
                  "SXYS1LUB", "QNCJLKQ3", "X4JSXYEF", "KGA553W7", "XT9RAZDH",
                  "4R5NYC8E", "68RZS4XR", "F1PE5WEY", "SKP9DVE3", "WKPGXL14"],
    # sim30-a + 30 more fresh seeds drawn 2026-10-16 (first 30 identical, so sim30-a results pair).
    "sim60-a": ["GY2PWF7D", "Z88P7IDQ", "BIA2QXW3", "8Y1ZLY8X", "57R27KQ2",
                  "GMJWI7Q1", "D172P6BH", "DUSJQ1TN", "MHVZ1JEC", "K792ZEJU",
                  "A7MZDVCC", "Z3GELG7B", "DRMQY3JS", "IDXTTNVK", "PCL2K7FC",
                  "XYK41G5M", "JU76QQT6", "USJXYQLN", "2HKM8YZ8", "BAF8SB49",
                  "6MV2BE26", "54ATPSF9", "NSMRHDU9", "7NULGXWV", "3F5ILH3G",
                  "D54JXDEK", "GTYIS5F4", "RS718JG4", "RHPJ6FZH", "QQ7KIM4A"],
}
SETS["sim60-a"] = SETS["sim30-a"] + SETS["sim60-a"]
NAME = "bench30-a"  # the set the "Benchmark" button runs
SEEDS = SETS[NAME]
