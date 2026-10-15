# Lessons from BALATRO'S ULTIMATE GUIDE

Reviewed 2026-10-14, before making another strategy change. Read the complete
newest transcript, `[English (auto-generated)] BALATRO'S ULTIMATE GUIDE
[DownSub.com].txt`, and compared it with v17 source, its completed bench30-a
recordings, the previous coaching notes, and installed game Lua mechanics.
Source remains **v17 `11845ce8`**. This review makes no gameplay changes.

## Main finding: we are declining useful money

The tutorial's strongest contribution to our next experiment is its emphasis
on economy buying future improvements (transcript lines 108–121, 155–190).
That is already familiar advice, but checking it against our recordings found
a much more specific problem than merely needing a stronger economy prompt.

Across the **30 full v17 benchmark games**, these cards appeared as eligible
options in the following recorded decisions:

| Card | Shop decisions | Pack decisions | Times selected |
| --- | ---: | ---: | ---: |
| The Hermit | 26 | 8 | 0 |
| Temperance | 7 | 8 | 0 |

These are **49 decision exposures, not 49 unique cards**: an offer can remain
available across several actions in one shop or pack. There were also no
recorded automatic uses of either card. Of the 33 shop exposures, **28 would
produce positive immediate net cash** from buying and using the offered card,
using recorded prices and installed payout rules. Do not sum those repeated
offers as missed income.

Examples from the full benchmark:

| Seed, event | Offer and available cash | Recorded decision | Alternative cash outcome |
| --- | --- | --- | --- |
| XUTRVN1Y, 6544 | $3 Hermit, holding $21 | Leave; Hermit rated 0.96/4 | Buy for $3, use at $18: finish with $36, net +$15 |
| WMAZ7HWJ, 7839 | $3 Hermit, holding $26 | Reroll; Hermit rated 0.88/4 | Buy and use: $43, net +$17, before further shopping |
| 7X7WSDCX, 3448 | $3 Temperance paying $9 | Reroll; rated 0.92/4 | Net +$6 before further shopping |
| 7X7WSDCX, 3589 | Free Hermit paying $20 in an open pack | Take Magician; Hermit rated 1.24/4 | $20 cash instead of the chosen enhancement |

The shop cases are particularly useful: gaining cash can precede another
purchase or reroll. Pack picks have a real opportunity cost, so passing cash
for a survival upgrade can be correct. Neither these alternatives nor the
tutorial establish that taking the money would have won the runs.

There is also a concrete information mismatch. `decisions.shop` supplies
`money_after` and interest **after purchase only**, while the payout lives in
separate prose. It calls `tarots.note` with pre-purchase money. For the $21
Hermit example, JEV sees $18 after buying but a claimed $20 payout; the actual
payout would be $18. The larger behavioral problem remains that it declines
the positive cash opportunity even with this optimistic payout description.

**Recommended first experiment:** represent buying and using a cash tarot as
one financial comparison: cost, payout after cost, net profit, final money and
interest. Let JEV compare that option with continuing the shop, and evaluate
paid-pack alternatives separately. Isolate this from broader joker or play
changes. Replay the recorded refusals first, including low-cash cases where
the purchase is unprofitable; then use repeated weak-seed and control trials.
Retain v15 as the performance reference. Do not jump straight to another full
benchmark on the strength of this finding.

All 49 records, exact run/event IDs, calculations and limitations are saved in
[the economy audit](samples/ultimate-guide-economy-audit.json). No JEV requests,
game actions or draw-order inspection were needed for this review.

## Other useful lessons and their status

### Build scoring that actually activates

The guide's chips / additive Mult / multiplicative Mult framework (lines
24–105) is useful, especially its warning against an expensive early
multiplier without its supporting conditions. Our v16/v17 prompt already
puts basic Mult and temporary support ahead of inactive multipliers. The
earlier coaching tutorial recommended early additive Mult before chips;
this guide presents chips first. Neither order should become an unconditional
rule: compare the marginal improvement to the current build.

We have a relevant observed failure: v15 `75WVKF42` bought Baseball Card
without Uncommon support and lost The Water. V16 stopped that purchase but
still lost, so avoiding the bad purchase alone is not a demonstrated cure.
See [the v16 investigation](v16-outlier-pilot.md).

`strategy.engine` still classifies effect text into roles without evaluating
activation or contributions from hand levels. A dormant Mystic Summit counts
as having Mult; a sufficiently levelled hand can supply chips without a chip
joker. Use effective score contributions when deciding what is missing or
replaceable. The tutorial's late-game replacement of a chip joker with xMult
is a conditional example, not a reason to remove every chip joker late.

### Measure combinations and round resources

The guide's Burglar / Mystic Summit / Ramen example (lines 320–332) illustrates
why item value depends on the resulting lineup and play policy. Burglar
removes discards and adds hands; this enables Summit and avoids Ramen's
discard-related deterioration. It can also make drawing difficult hands
harder. Our own v16 Burglar run died early, so “always take Burglar” is not
supported.

There is a confirmed approximation in our shop readiness:

- `readiness.power` evaluates every sample with two hands left and **one
  discard left**; `play_mix` uses the same fixed counters.
- `readiness.assess` defaults to **four hands per round**, and the shop calls
  it without an override for Burglar or other hand-count effects.
- `_gain` warns JEV that extra hands and generated jokers are unmeasured, but
  does not quantify them. Historical-hand scoring also does not simulate
  the new round resources after acquiring a joker.

A small offline illustration with two plain Aces, a level-one Pair, Burglar
and Mystic Summit gives **64** with the readiness assumption of one discard,
versus **544** with zero discards. The underlying hand scorer understands
Summit; the hypothetical shop context is wrong for this lineup. This is a
synthetic example, not a lineup observed in the 30-game benchmark.

Boss effects deserve the same treatment. Shop questions already include the
upcoming boss, but the generic readiness samples do not apply its restrictions
or scoring changes. Correcting these estimates is a separate candidate
experiment, not something to bundle into the cash-tarot trial.

### Time consumables deliberately

The guide suggests waiting for a better Hermit payout when safe and buying
intended jokers before using Temperance (lines 226–233). Current `tarots.auto_use`
uses Hermit as soon as cash is at least $4, and Temperance without checking
whether buying another intended joker first would improve its payout. Offline
checks also show it proposes using Temperance with no jokers, for $0.

This is a real code limitation, but **not an observed explanation for v17's
losses**, since that full benchmark never selected these cards. Taking the
cards appropriately comes before optimizing their timing. Waiting must also
account for immediate survival, available purchases, interest, and consumable
slot pressure; “always wait for $20” would be another poor blanket rule.

### Keep the deck aligned with a reachable hand

The guide links planets and tarot edits to the hand the deck is building
toward (lines 125–154). We already choose a build from sampled scoring hands
and measure planet gains. Reimposing a rigid flush plan would undo the v15
correction to overcommitting to flushes that the bot rarely produced.

Tarot targeting remains coarse: `build.card_value` mostly uses rank, suit fit
and a generic enhancement bonus. It does not fully understand each joker
combination. The guide's examples support evaluating compatible deck edits,
but they do not establish that chasing a named multi-joker combo is better
than using what the shop actually supplies.

## Useful details to retain, without importing blanket rules

- Interest thresholds are already explicit in shop options. Spending $14
  down to $10 preserves the same interest tier, but still costs $4 of liquidity.
  The normal interest cap is $5 at $25, not the guide's approximate $20 reserve.
- A spare choice in a Mega Buffoon Pack may have resale value (lines 217–222).
  Our paid-pack wording acknowledges temporary sellable jokers, but the
  available actions do not plan an explicit take/sell/take sequence. Worth
  measuring later; lower priority than the recorded cash-tarot refusals.
- The first-blind one-hand clear earns extra remaining-hand money (lines
  191–216). Current hand logic already compares scoring and discard forecasts;
  this is reinforcement, not a new feature by itself.
- The guide's face-card engine warning about The Plant is a reason to check
  boss compatibility, not to force a face-card build.
- Treat named synergies as conditional opportunities: Hologram with card
  generators, sell-value support with Swashbuckler, and retriggers with
  scoring effects. Do not reserve slots for several unowned pieces.
- Gold Stake rental/perishable advice has limited direct relevance to the
  current Red Deck / White Stake benchmark. Selling every other joker before
  Hex/Ankh is not an automatic benefit; sacrificed scoring and future value
  must be included.

## Evidence and limitations

The new source is an automatically generated transcript with corrupted names,
no timestamps and no visuals. “Mold” is Mult, “anti” is ante, and other names
were normalized only where context is clear. Its assertions that economy is
almost always best or that a recipe guarantees victory are tutorial
simplifications, not evaluation results.

Mechanics relevant to the numeric checks were read from the installed game's
`%APPDATA%/Balatro/Mods/lovely/dump`: `card.lua` contains the Hermit payout,
Temperance sell-value update, Burglar round effects and Mystic Summit scoring;
`game.lua` contains their configured amounts. No RNG state or future cards
were used. This review modifies documentation only, and source hashing still
returns `11845ce8`.

The most actionable new lesson is **to stop treating immediate cash profit as
a vague low-value alternative to scoring**. Conditional scoring estimates
and consumable timing are separate follow-ups. None is yet a demonstrated
win-rate improvement.
