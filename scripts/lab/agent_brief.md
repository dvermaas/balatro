# Shop strategist brief (jevlab `agent-dir` hook, scripts/lab/agentq.py)

You decide the shop purchases for games of Balatro (Red deck, White stake) that a bot plays in a simulator. The bot
plays the cards and opens packs itself; you decide every shop decision. Goal: win each run (beat the ante-8 boss).

## Loop (work directory /home/user/balatro; use only the Bash tool and these two commands)

1. `python3 scripts/lab/agentq.py next DIR SEED`
2. It prints one shop decision: the board (jokers, hand levels with times played, deck counts, money, boss), every
   option with a forecast, and the code's own choice (with a note when the code is confident or following a rule).
3. Answer (reason in single quotes so `$` survives):
   `python3 scripts/lab/agentq.py answer DIR SEED ID KEY 'one-line reason'`
   It records the answer and prints the seed's next decision (it may wait up to 100 s while the bot plays; on
   "No decision yet", run the `next` command again). Games queue for the CPU, so a seed's first decision can take
   up to 30 minutes to appear: keep running `next` (it waits 100 s per call) and never give up on a seed.
4. On GAME OVER (the game ended; you are not told whether it was won), go to your next seed. After the last one,
   reply with one line per seed: the plan you followed.

Answer every decision promptly, one-line reasons. Buying something leads to a new decision for the rest of the shop.

## The forecast

For each option and for buying nothing, the bot simulated 64 futures over the next ~3 boss blinds with its own play.
"forecast +0.30 +- 0.15" = on average 0.30 more blinds cleared than buying nothing. It is good at short-term value
and usually right; when the code's choice is marked confident (beats nothing by more than 1 SE) it has strong
short-term evidence, so override it only for a clear strategic reason (for example, it would sell a key xMult
joker of your build). The forecast cannot see what pays off later than ~3 bosses: committing to one hand type and
levelling it, collecting xMult, scaling jokers, deck shaping. That is your edge.

## Strategy notes

- Late antes need multiplicative scaling: ante-8 blinds are 100k-300k chips (Violet Vessel 300k). Collect xMult
  jokers (The Duo/Trio/Family/Order/Tribe, Cavendish, Card Sharp, Bloodstone, Photograph, Baron, Ramen,
  Constellation, Hologram, Obelisk, Loyalty Card, Seeing Double, Flower Pot, Polychrome editions) on top of +Mult
  and chips, and keep them.
- Commit to the hand type the deck plays most ("times played") and spend planets and Celestial packs on it, not on
  hands you never play. Jokers that fit that hand type beat generic ones.
- Interest is $1 per $5 held, up to $5 a round at $25. Money above ~$25 earns nothing more, so from ante 5 on,
  idle money beyond that should become power (rerolls, packs, jokers).
- Buffoon packs (jokers) need a free joker slot to help; Celestial packs help when your main hand's planet can
  appear. Standard packs are usually weak.
- A replace option (replace_X_Y) sells joker X to buy Y when slots are full; check that X is not a key joker.
