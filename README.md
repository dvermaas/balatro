# JEV × Balatro

A stress test for **JEV**, TypeSafe's typed "System One" model: a bot that plays
[Balatro](https://store.steampowered.com/app/2379780/Balatro/) by reading the live game state,
asking JEV to pick the next move, executing it, and repeating. Every decision is recorded so
prompt changes can be compared run by run.

```
Balatro (+ BalatroBot mod)  ──JSON-RPC──▶  jev_bot.py  ──typed Choice──▶  JEV API
        ▲                                     │
        └────────────── action ◀──────────────┘          panel.py: control panel + run analytics
```

**How decisions work.** Code enumerates the sensible legal moves (best play per hand type with a
score estimate, a few discard strategies, shop buys, pack picks) and JEV answers one `Choice`
question over them, returning a probability for every option. Code owns the rules and execution;
JEV supplies the judgment.

## Setup (one-time)

Already done on this machine; listed here so it can be redone.

1. **Lovely injector** (v0.10+): put `winmm.dll` next to `Balatro.exe`
   (`C:\Program Files (x86)\Steam\steamapps\common\Balatro`). Delete it to go back to vanilla.
2. **Mods**, in `%APPDATA%\Balatro\Mods\`:
   - `smods/`: [Steamodded](https://github.com/Steamodded/smods)
   - `balatrobot/`: [BalatroBot](https://github.com/coder/balatrobot) (`balatrobot.json`, `balatrobot.lua`, `src/lua`)
   - `jev_tweaks/`: copy of [`mods/jev_tweaks`](mods/jev_tweaks) from this repo
3. **Local BalatroBot patch.** In `Mods/balatrobot/src/lua/endpoints/start.lua`, guard the
   `viewed_back` line; otherwise starting a new run after a game over crashes:
   ```lua
   if G.GAME.viewed_back then G.GAME.viewed_back:change_to(deck_data) end
   ```
   Re-apply it after updating BalatroBot.
4. **Python env**: `uv venv .venv` then `uv pip install --python .venv typesafe-sdk`.
5. **`.env`**: `JEV_API_KEY=...` (a TypeSafe API key; the org needs credits on the API).

## Running

1. Close Balatro, then start it with **[`scripts/launch_fast.cmd`](scripts/launch_fast.cmd)**: BalatroBot fast
   mode (10× game speed) with audio on. Launching from Steam also works, but at normal speed
   and muted.
2. Start the control panel and open http://127.0.0.1:8765:
   ```
   .venv\Scripts\python src\panel.py
   ```
3. Press **Run** (or Space). It starts paused.

### Control panel

- **Navbar:** Run/Pause, Step (one decision), `⋯` for delay, auto-start next run, abandon run.
  The pill shows whether the bot is running, which prompt version is live, how many runs it has,
  decisions per minute and average run time. Keys: `Space` run/pause, `S` step, `1/2/3` tabs.
- **Live:** current blind, chip progress, hands/discards, money, hand, jokers, and a feed
  of decisions with JEV's probability for each option (click to see all options).
- **Runs:** every recorded run, filterable by prompt version. Click one for a timeline grouped
  by ante/round, including the exact state sent to JEV for each decision.
- **Prompts:** one card per prompt version with stats (ante reached, wins, confidence per
  decision type, pack take rate, wasted discards, jokers held) and ▲/▼ against the previous
  version, plus the full prompt text with changes highlighted.

## Tuning prompts

All text JEV reads lives in **[`src/prompts.py`](src/prompts.py)**: game rules, the question per decision
type, and the option templates. Always-right moves and the context JEV gets live in
**[`src/strategy.py`](src/strategy.py)**: auto-using planets, keeping xMult jokers rightmost, and tagging
jokers by role and by synergy with the run's main hands. The version is a hash of `prompts.py` +
`hands.py` + `strategy.py` (+ `decisions.py` from v6 and `jokers.py` from v8, where the per-decision questions and buy/skip
thresholds live), so **any edit creates a new version** (v1, v2, …) with its own stats.
Update `VERSION_NOTE` to say what changed and restart the panel.

### Benchmark

To compare versions cleanly, open the **Benchmark** tab and press **Benchmark current version**.
The bot plays the same 10 fixed seeds ([`src/benchmark.py`](src/benchmark.py)) and stops when done. The tab
shows a summary per version and a seed × version grid of the ante reached. Seeded runs don't
trigger unlocks.

## Recorded data

```
runs/
  <started>-<seed>/
    summary.json       # result, ante/round, decisions, tokens, confidence, prompt_version
    decisions.jsonl    # every JEV call: state sent, instructions, options, choice, probabilities,
                       # confidence, latency, tokens; plus actions, errors and run end
  _prompts/
    index.json         # version order (v1, v2, …)
    <version>.json     # snapshot of the prompt text for that version
```

## Layout

```
src/                    the bot and control panel (run: python src/panel.py)
  balatro.py            JSON-RPC client for BalatroBot (127.0.0.1:12346)
  decisions.py          v6+ handlers: batched typed questions per decision, thresholds in code
  jokers.py             joker-aware hand scoring parsed from live joker text (v8+)
  hands.py              hand evaluation, score estimates, play and discard candidates
  prompts.py            everything JEV reads (rules, questions, option templates)
  strategy.py           code-level policy: auto planets, joker ordering, joker tags, interest
  jev_bot.py            the game loop: one handler per game state, asks JEV, executes
  recorder.py           per-run recording and prompt-version snapshots/ids
  analytics.py          per-run, per-version and benchmark metrics
  benchmark.py          the fixed 10-seed benchmark set
  panel.py, panel.html  control panel server and single-page UI
  legacy/               byte-exact rebuild of v1, runnable with `python src/panel.py --legacy v1`
mods/jev_tweaks/        companion Balatro mod: skips blocking "unlocked" popups
scripts/launch_fast.cmd starts Balatro in fast mode with audio
docs/                   TypeSafe skill notes, sample game states
runs/                   recordings (git-ignored except runs/_prompts version snapshots)
```

## Known issues

- **Something blocks the game** (a popup, a menu): Balatro stops answering, and after 60 s the
  bot pauses with a hint in the feed. Clear it in the game and press Run. Unlock popups are
  handled by `jev_tweaks`.
- **Consumables** in hand (tarots) are never used, and tarots that need targets from a pack get a
  best-effort target (first two cards).
- **Score estimates ignore jokers**; JEV is told this and sees the jokers' effects.
- The JEV API returns **HTTP 402** when the TypeSafe org has no usable API credits. The free
  monthly credit alone did not work for us; a paid top-up did.
