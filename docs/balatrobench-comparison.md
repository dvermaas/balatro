# BalatroBench comparison (2026-10-11)

Source: balatrobench.com leaderboard v1.0.8 (BalatroLLM "default" strategy), public per-request
data on its CDN (`gamestate.md`, `tool_call.json`, `memory.md`, `reasoning.md`, `strategy.md` per
request). Same game setup as ours: Red deck, White stake, seeds AAAAAAA..EEEEEEE, 3 runs each
(our `balatrobench-5` benchmark set). Numbers below were computed by crawling every request of
the top three models.

## Leaderboard (final round reached; 24 = beat ante 8)

| Model | Avg round | Wins | s/call | tokens in/call | $/run |
|---|---|---|---|---|---|
| gemini-3-pro-preview | 19.3 ± 6.8 | 9/15 | 34 | 14.6k | 11.16 |
| gpt-5.2 | 18.4 ± 5.1 | 3/16 | 29 | 14.1k | 4.36 |
| gemini-3-flash-preview | 16.7 ± 7.4 | 5/15 | 17 | 14.5k | 2.15 |
| claude-opus-4.5 | 13.9 ± 6.5 | 2/15 | 48 | 16.5k | 14.86 |
| claude-sonnet-4.5 | 11.9 ± 6.7 | 2/15 | 54 | 16.7k | 8.88 |
| gpt-oss-120b | 6.3 ± 2.9 | 0/15 | 2.8 | 13.6k | 0.27 |
| claude-haiku-4.5 | 5.1 ± 3.4 | 0/15 | 41 | 16.8k | 1.19 |

JEV (v8): ~7.3 blinds cleared on bench30-a (≈ round 8), 0.2 s/call, ~1.6k tokens in/call.

## Behaviour profile of the top three

| | Gemini 3 Pro | GPT-5.2 | Gemini 3 Flash | JEV v8 |
|---|---|---|---|---|
| Jokers at end of ante 1 / 2 / 3 | 2.3 / 4.2 / 5.0 | 2.2 / 4.3 / 4.9 | 2.5 / 3.8 / 5.0 | 0.8 / 1.9 / 3.1 |
| Money in shop, ante 1 / 2 / 3 | $9 / $17 / $33 | $7 / $6 / $4 | $5 / $10 / $22 | $8 / $22 / $31 |
| Joker buys per run | 18.8 | 18.3 | 10.1 | 1.8 |
| Packs bought per run | 28.7 | 17.4 | 25.5 | 3.6 |
| Rerolls per run | 20.5 | 4.5 | 8.5 | ~1.5 |
| Jokers sold per run | 12.1 | 9.1 | 3.5 | rare (swaps only) |
| Consumables used per run | 17.7 | 13.0 | 11.3 | planets only |
| Vouchers per run | 3.1 | 3.1 | 1.9 | 0 |
| Discard share of moves | 0.44 | 0.30 | 0.43 | ~0.39 |
| Blind skips | 0 | 0 | 0 | 0 |

## Qualitative patterns (from `memory.md` decision logs)

- Commit to one hand type (Flush, or Three of a Kind with Zany Joker) and level it hard: Celestial
  packs are bought to hunt that hand's planet; jokers are judged by fit with the build.
- Tarots are used actively, with chosen target cards (often right before the winning hand, so the
  enhanced cards return to the deck), and Arcana packs are bought for money tarots (Hermit, Temperance).
- Boss play-around: e.g. under The Hook, discard to dig instead of playing a hand that can't win.
- Spend early, bank later: ante 1-2 money goes into jokers/packs; from ante 3 they hold $25+.
  GPT-5.2 barely holds money at all and is still #2.

Their strategy guide (`strategy.md`, ~42 KB, sent with every request) covers rules, scoring order,
hand-type focus (Flush easiest), ante-phase priorities, tarot priority (suit changers for flushes,
enhancements, Hanged Man thinning, Hermit), a "never sell-buy cycle" economy rule, boss adaptation and
common synergies.
