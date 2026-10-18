# Balatro coaching: actionable notes from the transcript

Source: `[English (auto-generated)] The GREATEST Balatro Player Coached Me. IT WAS INSANE. [DownSub.com].txt`, read in full (3,465 lines). References below are **one-based lines in that unmodified file**, including blank lines; there are no timestamps. The coach introduces himself as Dr. Spectre / Balatro University (L4–9). Speaker turns are not labeled, so “coach guidance” means advice supported by the coaching exchange, not a verbatim attributed quotation.

## Scope and how to use these notes

The session practices **Red Deck, White Stake, high scores within roughly six to eight antes**, with frequent discussion of a multiplayer mod. It does not establish a universal Gold Stake win policy. The coach explicitly distinguishes higher-stake obstacles from ordinary practice (L98–120), and later says White Stake's lower scoring requirements allow more reliance on planets and more slots for economy (L2311–2322). The run also receives an unusually favorable combination of jokers (L3285–3299).

For JEV, the most useful lesson is a decision process: **calculate enough scoring to survive, turn surplus safety into value, and spend that value on a coherent future build**. This is a synthesis of the transcript, not an instruction to change the bot or begin another benchmark. These notes do not modify source, prompts, settings, runs, or the game.

## 1. Early survival: establish a scoring floor

- **Ante 1, first blind:** the coach usually seeks a flush and aims to clear 300 in one hand; a weaker flush can be followed by a pair. Only the higher straights—ace-high, king-high, queen-high—are described as one-hand clears in the opening example. The goal is the extra dollar from finishing in fewer hands, not flushes for their own sake (L160–183). **Arithmetic correction:** the transcript's “at least 30” card-chip threshold (L168–169) would not clear 300 with an unmodified level-one flush. The project's recorded sample has 35 base chips and 4 Mult, so the cards need 40 chips: `(35 + 40) × 4 = 300`. This correction is checked against `docs/samples/sample_gamestate.json`, not attributed to the coach.
- **First boss without scoring support:** plan for approximately two strong five-card hands, such as flush + straight or flush + full house. The coach's rough starting estimates are three of a kind around 180 and two pair around 100–140; debuffed scoring cards lower these estimates. Follow the actual target and remaining hands rather than treating these estimates as constants (L314–349).
- **Before leaving a shop, explain how the next blind is beatable.** At the Ante 2 transition, an unsupported 800 target would require too many weak hands; Mystic Summit is bought for immediate scoring. If no suitable joker appeared, the fallback was a Celestial Pack seeking Jupiter or Saturn to improve the hands already used in Ante 1 (L393–428).
- **Do not pay for redundant safety by default.** Once Mystic Summit covers the next rounds, save rather than also buying planets “just in case.” Later, early-game Jupiter/Saturn insurance is dropped when three of a kind is stable; spend on relevant future hands instead (L423–430, L1559–1569).
- **Scoring purchases can have economic payback.** Scholar could save two or three hands, hence two or three dollars, but the coach revises that recommendation when reminded that relevant aces are debuffed. Check the upcoming boss before evaluating the purchase (L286–311).

**JEV application (derived):** evaluate candidate plays and shop exits against the blind target, boss restrictions, remaining hands, and a conservative score estimate. Recompute after changing a card, hand type, joker, or activation condition. Do not inherit “this wins” from a previous hand with different ranks (L484–499).

## 2. Economy: exploit surplus scoring, preserve the winning finish

### Early purchases and interest

The coach's first-shop ordering is money/value generation, then additive Mult, then other support such as chips. He would take Reserved Parking on White Stake; Mad Joker is the safer scoring alternative. Buying nothing and saving interest is also allowed. Tarot generation counts as value, not only cash (L184–213).

Treat **$25 as the ordinary max-interest reserve in this session**, not a prohibition on every purchase below it:

- Count lost interest as part of a purchase's effective cost. Dropping from $27 to $24 for a $3 pack can make that pack effectively cost $4 for the next interest payout (L724–743).
- In-round money generation can replenish the reserve before interest is calculated, increasing the practical budget. Reserved Parking and Mail-In Rebate are the examples; their random returns should not be treated as guaranteed cash (L744–751, L1453–1455).
- Save **$25 plus voucher money**, especially before the shop after a boss (L755–761, L1596–1603).
- Clearance Sale can justify a short-term interest loss because later purchases repay it; Liquidation is bought before other purchases to benefit from the discount (L279–285, L2848–2856).

### Reserved Parking: safe farming pattern

1. Know a finishing hand wins; in the opening example, four queens are already available.
2. Discard unwanted cards to find more faces while retaining the finish.
3. Use surplus hands to play unwanted cards and receive held-face payouts.
4. Finish before exhausting the hands required to survive (L223–253).

Keep a running estimate of typical hand score and available tolerance. If the current full house already covers the remainder, a spare hand may earn Parking money first (L378–390). Smiley Face can fit this pattern: hold faces for payouts, then play them to finish (L704–720).

**Do not confuse a safe finish with a likely future draw.** The coach distinguishes retaining a known winning flush from the greedier option of throwing it away and needing another face card. The latter has a draw dependency (L449–466). A later near-mistake comes from chasing Mail-In Rebate when only one hand remains; stop farming and play the winning five of a kind (L2257–2267).

Every extra hand sacrifices a dollar of unused-hand payout in the session's accounting. Compare that cost with the incremental payout; an economy joker is not free money simply because it triggers (L295–301, L1741–1757).

### Mail-In Rebate: preserve discards for the paying rank

- When the target rank remains in the deck, spend surplus **hands as draw actions** to save discards for the target. This works only while a dependable finish remains (L1617–1628, L1943–1953).
- Count target cards remaining, draw-pile size, and cards reachable through remaining draws. The example has 19 cards left and three discards of five, reaching 15 of them; a spare play can improve coverage. This reasoning also applies to Faceless Joker (L1959–1982).
- Economy plans can conflict: Parking wants faces held, while Rebate wants target-rank discards. The coach eventually replaces Parking with Chaos the Clown because hand costs and Rebate's draw needs make Parking less attractive (L1730–1757).
- More economy jokers can be appropriate **when scoring is already sufficient**. Preserve a useful balance of chips and Mult when freeing a slot (L1524–1557).

## 3. Shops: budget for opportunities, then buy information

### Reroll decision rules

- Have a **shopping list** before rerolling: missing chips, missing Mult, economy, or useful tarot cards. If scoring and economy are covered and consumable slots are already full, a reroll may have little usable upside (L1832–1867).
- Reserve pack costs, the interest floor, and money to buy a hit. The coach suggests an additional roughly $5 purchase buffer. A reroll that leaves exactly $25 prevents purchasing the result without reducing interest (L1157–1177, L1221–1225).
- Spread rerolls between shops because successive rolls become more expensive (L1226–1229).
- Packs are limited per shop; rerolls are available in later shops. Do not spend their entire budget chasing shop results (L1173–1177).
- Chaos the Clown's free roll also preserves the lower reroll cost sequence: the transcript compares 5/6/7 with 0/5/6. Its savings grow with the number of rolls actually used; do not value it as a fixed $5 regardless of shopping behavior (L1730–1740).

### Pack order is conditional

The coach initially favors boosters before rerolls and Arcana before other packs because Hermit/Temperance can refund the cost (L565–571, L866–876). He then explains why **planned, affordable rerolls can come first**:

- Shop tarot cards can be carried into an Arcana Pack and combined with its choices—for example, apply Chariot, then Death the resulting steel card (L1097–1114, L1339–1347).
- An unwanted planet still present in the shop is described as blocking that planet from a Celestial Pack. Open the pack while useful blockers remain, rather than rerolling them away (L1098–1104, L2631–2644).
- Likewise, unwanted held consumables can be kept to block duplicates, then sold when the slot is needed (L1262–1268, L2852–2856).
- Open a Buffoon Pack before committing to an ordinary shop joker when the pack can supply more information. Misprint's broad applicability is normally attractive, but Zany Joker becomes more appealing when Wily Joker is also offered, creating a combined three-of-a-kind plan (L819–855).

**Important transcript claim, not independently verified here:** adding a card from a Standard Pack in the current shop will not make that new card appear in the current Arcana Pack's presented hand. Do not buy Standard first solely to immediately Death its new card in Arcana (L572–598).

The “buy both packs whenever above $25” remark is an aggressive value-building heuristic from this run, not an unconditional bot rule. The surrounding discussion also requires voucher/purchase buffers and allows taking no card from a purchased pack.

## 4. Jokers: marginal value and build compatibility

### Balance chips and Mult

The coach's pacing heuristic is **one or two additive-Mult sources in Antes 1–2, then a chip source in Antes 2–3**. The first +15 Mult dramatically improves a base-4-Mult flush; another similarly sized additive-Mult joker gives a smaller relative improvement. Chips then improve the other factor of the score (L1038–1079).

Compare a joker's **relative contribution to the current hand**, not just its printed number. When the hand already supplies about 200 chips, Wily's +100 is roughly ×1.5. At that point it may be replaceable while keeping economy. A later example estimates losing it reduces a 56,000 hand to roughly two-thirds of that, still sufficient (L2286–2306, L2428–2444). Replaceable does not mean worthless: the coach still prefers Wily to an unidentified random joker (L2336–2340).

### Follow the build's actual actions

- Joker Stencil is temporary if filling slots will weaken it. Buy it when immediate points are necessary; in this safe single-player situation packs have priority (L525–557).
- The Tribe is rejected because committing to flushes would require holding nonfaces and interfere with Parking. The Trio could fit the existing three-of-a-kind plan and replace support without changing the draw strategy (L1178–1218).
- Mystic Summit can be deliberately enabled/disabled through discards, useful when excessive score would prematurely end value farming. Once that concern fades, flexible always-on support can replace restrictive support (L1119–1150).
- Bootstraps reduces freedom to spend because spending also reduces its Mult. Still, buying immediate safety may allow greedier Rebate farming. Later the coach considers a lower-scoring Baron replacement because it frees the cash to be spent (L1992–2025, L2130–2138).
- Seltzer provides burst scoring and synergizes with Smiley Face. If one hand finishes each round, it can cover roughly three antes. Its value is especially high for a near-term multiplayer confrontation, but it need not be bought when existing score suffices (L2142–2184).
- Campfire is described as strong for Gold Stake wins but unsuitable as the sustainable route to this run's huge later scores (L2228–2236).

These are contextual evaluations, not a static joker tier list.

## 5. Deck building: improve the best cards and access to them

### Additions and removals

- **Do not take a Standard Pack card just because it is offered.** Prefer value generators (blue, purple, gold seals; sometimes gold cards) or a specific build ingredient. A plain extra four is unnecessary when one three of a kind is already accessible from the starter deck; it dilutes access to better cards (L687–700, L889–918).
- Early speculative additions have more time to develop. A low-rank red seal could become glass in a mixed full house; a ten is close to becoming a face through Strength. Similar cards may be too slow late in the run (L652–682, L694–700, L1434–1439, L1586–1593). The coach explicitly accepts that an early speculation may fail to work out (L1606–1613).
- Face cards have many relevant synergies in this chosen route. Put an early gold enhancement on a face; a king held for Parking is especially suitable (L633–651). This does **not** make faces correct for every possible build.
- The coach removes aces early in this face-focused plan: their small chip advantage matters mainly early, they are far from faces through Strength, and face synergies dominate the intended route (L1761–1809). Do not generalize this into ignoring an actual ace synergy.
- Even an additional face or steel king can be wrong if it dilutes the strongest existing cards. Later, with hand size nine, the question becomes the **best eight kings to hold**, especially red seals, rather than whether another steel king beats an average card (L1810–1822, L3143–3154).

### Hanged Man, Death, Strength, and suit changes

- Remove cards for consistency. Removing two of the same unwanted rank is marginally preferred in the repeated-rank build; suit matters mainly on the cards that will form the eventual scoring hand. Do not refuse useful removal while waiting for a perfect pair when draw resources are limited (L1876–1903, L1927–1941, L2341–2352).
- Removing two held cards before playing five lets the next draw refill seven slots, improving immediate card coverage (L1904–1908).
- The coach says Rebate chooses its rank through a random card in the deck, so eliminating ranks also changes Rebate's future target distribution (L1920–1927). This is a transcript claim; no external game-rule verification was performed.
- Death should usually copy the **best card in the deck**; patience is allowed. Avoid consuming it on an ordinary jack when a red-seal target can be found (L2920–2926).
- Strength can consolidate a rank as well as increase it: a jack to queen adds a fifth queen; tens to jacks create new faces. Choose based on the future hand, not simply the largest starting rank (L1232–1257).
- Apply suit changes before Death when that yields an extra card of the desired suit. Spades are favored in this run because of its current concentration, not because of an inherent suit advantage (L1383–1401).

## 6. Scaling: distinguish finishing Ante 8 from huge scores

The coach favors **repeatable multiplicative effects plus retriggers** for huge scores: glass, Photograph, Hanging Chad, red seals, and copy jokers. A single ×3 rare may finish an eight-ante run without creating a huge-score engine. Rare-tag skips are therefore weak searches for the particular high-score rares wanted here (L129–158).

The intended played-card route evolves toward full house, four of a kind, five of a kind, and potentially flush house/flush five. Repeated-rank hands can play multiple copies of the same glass card. Four of a kind's planet scaling can eventually carry survival while joker slots generate value (L273–278, L1283–1305, L1349–1355).

- Make glass early enough to copy it repeatedly; a safe extra queen and a glass seed are different investments, and glass is the high-score pivot (L2033–2053).
- One useful red seal can be enhanced and copied; it is harder to find than a glass enhancement. Extra redundant seals/cards are not automatically needed when enough draws already reach the deck (L2356–2407).
- Play five of a kind once to unlock its planet in the session's account. In the example, sacrificing $3 from a held gold card buys that unlock; afterward the same sacrifice is unnecessary (L2093–2110, L2215–2219).
- Upgrade the hand you intend to use. Five of a kind provides room for another glass card, but four of a kind can be easier immediately. Later a Baron/Mime high-card pivot makes further five-of-a-kind planets less relevant (L2242–2247, L2522–2529, L3110–3113).
- Baron/Mime can be enormous when assembled, but Baron is described as slow for a six/eight-ante score race, and finding that exact pair is less flexible than glass retriggers. The fortunate route taken in the video is not a promise that it can be forced (L2114–2128, L2829–2847).

## 7. Scoring order, held cards, and copy-joker choices

### Order matters more than labels

Glass and steel multipliers occur before ordinary end-of-hand additive-Mult jokers in the discussion. Thus steel does not multiply the later Gros Michel/Mystic Summit addition. Smiley Face adds Mult during played-card scoring, before steel, so it can improve steel's contribution (L1460–1480).

Steel can help, but do not sacrifice hand consistency or survival merely to hold it: the coach compares its ×1.5 with glass's ×2 and available played-card retriggers (L1306–1333, L2195–2201). Photograph can replace Smiley Face because early retrigger access gives it a better growth route, even though later Smiley additions are not multiplied by Photograph's earlier activation (L2497–2514).

### Re-evaluate what to play versus hold

With Baron, holding kings may beat playing them in five of a kind. Compare the complete score, including base chips/Mult and held multipliers; do not always choose the highest-ranked poker hand. The transcript contrasts a full house/other hand that preserves kings with a five-of-a-kind line that spends them (L2445–2490).

A later red-seal king held with Baron/Mime/copies receives enough ×1.5 activations to beat playing it as glass. A stone addition improves the chip-poor high-card hand substantially; later, another strong held king can be worth more than retaining that stone card (L3061–3095, L3224–3233). These are state-dependent comparisons.

### Copy scoring effects or retriggers?

The coach's shortcut is to **balance multiplicative scoring effects and activation counts**, including the original activation and red seal. Compare candidate copies; “always copy Mime” or “always copy Chad” is unsupported (L2938–2998).

Concrete end-of-run example: first-card glass + Photograph supplies two ×2 effects; original + red seal + Chad supplies four activations. Blueprint on Photograph gives four activations × three ×2 effects; Blueprint on Chad gives six activations × two ×2 effects. Both yield twelve ×2 applications on that card in the coach's final correction (L3338–3397). The shortcut is approximate when cards have different enhancements or other effects; evaluate the whole hand.

The transcript also uses Blueprint on Rebate during discards and on scoring support at the finish (L2440–2444), and moves it from Baron for scoring to Mime for gold-card payouts after submitting the hand (L3015–3024). **The latter is a timing-sensitive manual technique, not an established capability of this bot's action interface.** Verify supported sequencing before translating it into automation.

## 8. Hands, discards, consumables, and bosses

- Build one target hand at a time: keep the cards needed for it and discard the rest instead of preserving several speculative plans. Play an already useful full house rather than discarding it merely because the original plan was a flush (L338–365).
- Discard as many unwanted cards as possible **subject to retaining necessary cards**. “Discard maximum” does not authorize discarding the faces being collected or the known finish (L996–1011).
- Use draw coverage to decide when ordinary redundant kings can be discarded for better red-seal/glass kings. Count what remains; do not use the draw pile's actual order (L2381–2399, L3224–3239).
- With facedown cards, sorting and known deck composition can narrow identities. The coach says Parking still pays on facedown faces. Playing hidden cards reveals information that discarding them may not record in the deck preview; decide whether that information is worth the cost (L928–968). The long hidden-card discussion includes corrected guesses, so it should not become a rigid inference rule.
- A suit-changing tarot can reveal facedown cards and provide a rescue flush. Keep useful consumables for emergency survival, not only eventual deck edits (L1500–1504, L1629–1683).
- Preserve the **last-used consumable** when planning a Fool. Using Hierophant after Death changes what Fool copies; opening Celestial first can likewise overwrite a valuable target. Arcana first is preferable when trying to recover Death/Hanged Man via Fool (L1507–1519, L1574–1582, L2026–2031).
- In this mature high-score route, Fool's premium targets are Hanged Man, Death, and Justice. The coach values deterministic deck shaping above another planet, Hermit, or two random Emperor tarots; early Devil can be different (L2275–2284, L2670–2695).
- When a strong hand would end the round before drawing the ideal cards, use a low-scoring throwaway hand **earlier**, before assembling the multiplier engine. The coach identifies this as a missed extra draw opportunity (L3266–3276, L3318–3321).
- Check boss effects before choosing the scoring showcase: a random-joker-disable boss prompts maximizing score in the preceding round instead (L3204–3209).

## 9. Skips and multiplayer caveats

The coach's simplified skip heuristic is **Investment Tag in Ante 1**, which provides $25; he explicitly acknowledges more nuance and says this is his approximately 99% default, not an exhaustive theorem (L2055–2074). Do not skip routinely for a random rare, or for delayed voucher benefits when the horizon is short (L129–158, L2055–2066).

Multiplayer changes the objective: enough score to beat a normal blind may lose to an opponent, and early PvP rounds can punish an economy-only plan before it pays off (L57–77, L531–557). The transcript also discusses using all hands quickly to cut short an opponent's farming after accepting a loss (L512–523). That is specific to the multiplayer mod's round behavior; it should not enter the single-player JEV policy.

## 10. Suggested decision checklist for future JEV work

The following is an **implementation-oriented synthesis**, not a claim that the coach prescribed this exact checklist or that the current code lacks these checks:

1. **Survival:** identify a reliable finish and score margin after the actual boss effect. Reserve the necessary hands/cards.
2. **Value action:** compare finishing now with incremental cash, consumables, or deck improvements from another play/discard, including lost hand payout and draw risk.
3. **Draw objective:** name the cards being sought; preserve the paying resource (e.g., Rebate discards) and estimate reachable cards using unordered counts.
4. **Shop budget:** interest reserve + packs + near-term voucher + affordable-hit buffer; adjust cautiously for in-round income.
5. **Shopping list:** buy the missing role or compatible upgrade. Avoid rerolling without a usable target/slot.
6. **Deck edit:** enhance/copy the best relevant card, remove unwanted ranks, and reject additions that dilute access to the intended final hand.
7. **Scaling/pivot:** compare complete played-hand and held-card engines; retire support only when the remaining score is sufficient.
8. **Ordering:** check consumable history, pack blockers, first scoring card, copy target, and activation sequence before acting.

The coach's closing priorities are a clear endgame direction for every tarot edit and maximizing round value over as many rounds as possible. Money increases access to jokers and tarot cards through rerolls, enabling both selection and deck shaping (L3431–3458).

## Transcription and evidence limitations

- Names are frequently corrupted: “reserve parking” means Reserved Parking; “WY” means Wily Joker; “mail/melee rebate” means Mail-In Rebate; “banana” in the +15-Mult discussion means Gros Michel; “Chad” means Hanging Chad; “mine” means Mime; “molt/malt” means Mult; “anti” means ante. Names have been normalized where context is clear.
- The transcript has no speaker labels, timestamps, card images, or complete shop states. Unnamed “this joker,” “this voucher,” and selected cards cannot always be identified. These notes avoid guessing their identities.
- Several calculations and hidden-card deductions are corrected during the conversation. For example, the opening high-card estimate changes with rank (L484–499), the five-of-a-kind arithmetic is inconsistent mid-explanation (L2082–2091), and the final Blueprint comparison ends in a tie after an initial preference (L3342–3389). Use the corrected principle and actual state, not every spoken intermediate number.
- L473 says to throw away faces while surrounding advice says to hold faces for Parking; it is treated as a transcription/speech inconsistency, not a new rule.
- Pack generation/blocking, Rebate target selection, and post-submit Blueprint movement are reported as **claims made in the session**, without external verification. In particular, the copy-joker arithmetic is a shortcut, not a complete scoring simulator specification.
- “NaN”/extreme endless scoring is discussed as a different horizon. Strong late-run outcomes and the reported four-billion played-card demonstration (L3399–3402) illustrate the favorable run, not typical expected results.

## "Balatro has a MASSIVE balance problem" (4,500-hour player, 2026-10-18 upload)

What it says about winning, with how our lab data lines up (docs/lab-lessons.md):

- Pairs win by consistency: about four pairs a round against two flushes, so once a few generic chip and mult
  jokers are in, the round total favours pairs. Chips are the enabler ("100 times nothing is nothing"). Our
  data agrees: Pair-leaning decks win ~30%, Flush decks ~11%; the 5k-games teacher levels Pair in 77% of wins.
  But forcing every planet and joker toward Pair lost in our lab: consistency within a round is the lesson, not
  committing every resource.
- Repeatable hands feed Supernova, Green Joker and Ride the Bus; Green Joker discourages discards. Green Joker
  and Burglar are among the jokers most linked to our wins.
- Money generators (Delayed Gratification, Faceless, Mail-In Rebate, gold cards, Certificate, Hologram) suit the
  pair player, who needs no specific suit or rank and keeps more hand space. Our forecast cannot value money
  (rollouts never shop); a money tail in the forecast did not help.
- Pair players can ignore most bosses (face-down and debuffed cards, The Water, The Mouth). Our Mouth fixes
  (v40, v41) came from valuing a hand type over the whole round, which is the same idea.
- Planets: ~one Celestial pack every two shops, ~1/3 chance to find a given planet: about four levels a run.
  (Our v28 planet rule reaches ~9 levels by ante 6.)
