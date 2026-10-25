//! Lab experiments that are not production policy (scripts/policy_lab.py BOT_ARMS, in Rust). Read once from
//! `JEVLAB_EXP` (comma-separated flags); unset = production behaviour. Set only by jevlab runs; the defaults here
//! (VALUE_TAIL since v53) apply everywhere.
//!
//! Pair commitment (docs/lab-lessons.md "Open ideas"): the 5k-games teacher's highest hand at ante 6 is Pair
//! in 77% of its wins, with Pair jokers and Pair levels together. Flags:
//!   pair-planets  the shop planet rule always targets Pair; the pack Picker adds PAIR_PLANET to Mercury
//!   pair-jokers   Pair jokers clear the auto-buy bar PAIR_SE paired SEs earlier, and win early slot fills
//!   pair-keep     Pair jokers are never among the weakest jokers (swaps, Verdant Leaf sales)
//!   pair-play     a play containing a Pair within PAIR_PLAY of the best score is played instead
//!   free-discard  before a play, discard spare cards a held joker pays for (Castle's suit, Mail-In
//!                 Rebate's rank, Jacks, 3+ face cards, Trading Card's single card, Burnt Joker's hand,
//!                 Yorick); the forecast's rollouts do the same (simcore::free_discard)
//!   eye-plan      under The Eye the play's hand type is chosen by its value over the round (now + hands
//!                 left x expected best of a fresh deal without that type)
//!   late-floor=N  the shop's interest floor (loose buys, rerolls, strong vouchers) drops $N per ante after 4
//!   skip-tags=A+B replaces decisions::SKIP_TAGS, the tags a Small/Big blind is skipped for at ante <= SKIP_ANTE
//!                 ("Investment", "Negative", ...; skip-tags=none skips nothing)
//!   skip-min=A    skip-tags only from ante A
//!   skip-max=A    skip-tags up to ante A (default decisions::SKIP_ANTE)
//!   fc-bosses=B   jevlab's shop/item forecasts look B bosses ahead (DecisionCfg::lab: 3)
//!   fc-n=N        jevlab's forecasts play N rollouts per option (DecisionCfg::lab: 64)
//!   max-rerolls=N / swap-sells=N / synth=N / planet-deals=N  override decisions::MAX_REROLLS, SWAP_SELLS and
//!                 picker::SYNTH_HANDS, PLANET_DEALS (the pack Picker's sample sizes)
//!   sure-se=X     the forecasts' auto-buy bar in paired SEs over buying nothing (decisions::SURE_SE)
//!   rollout-nodiscard  forecast rollouts never discard (a probe: how much do their discards matter?)
//!   rollout-discard2=S  rollouts discard toward the keep set (old rule, flush draw, straight draw, pairs, best
//!                 play) with the best mean score over S order-free redraws (simcore::SMART_DISCARD)
//!   rollout-behind=K  forecast rollouts discard only while K x best play x hands left < chips needed (default 1)
//!   discard-key=K  with hands to spare, CodeJev's discards are ranked by K: pace (chance the next play makes the
//!                 pace) or low (20th-percentile next play), then the mean (default: the mean next play)
//!   clear-margin=M  a play counts as clearing the blind when its estimate >= M x chips needed (with jokers and an
//!                 exact estimate; default 1 since v56, was 1.1)
//!   clear-risky=M  the clear margin when the estimate is uncertain (Misprint, Bloodstone, an unmodelled scoring
//!                 joker, a Lucky card in hand; default decisions::play::CLEAR_MARGIN = 1.1)
//!   setup-play=M  out of discards with hands to spare, CodeJev picks the play with the best two-play average (avg)
//!                 or the best chance the two plays clear, then the average (clear); default: the best score now
//!   cj-fc-n=N     CodeJev's own item forecast (shop tarots, spectrals, packs it is asked about) plays N rollouts
//!                 (default codejev::FORECAST_N = 16)
//!   no-order-search  the v56 joker order (xMult jokers to the right, nothing around a copier) instead of v57's
//!                 search by sampled score (readiness::best_joker_order)
//!   card-order    before a play, the played cards are moved to the front of the hand in the order that scores
//!                 most (decisions::best_card_order)
//!   tarot-measure  targeted tarots pick their cards by measurement (readiness::measured_plan) instead of
//!                 tarots::plan's card-value rule
//!   fc-order      the shop forecast's rollouts hold each option's jokers in v57's searched order (a bought joker
//!                 was appended on the right)
//!   early-jokers=N / early-ante=A  the shop's early slot fill (any joker the forecast rates above zero) runs while
//!                 the bot has fewer than N jokers, up to ante A (decisions::EARLY_JOKERS 4, EARLY_ANTE 3)
//!   planet-floor=N  the shop's planet rule (top hand's planet, else a Celestial pack) buys while $N stays (default
//!                 decisions::PLANET_FLOOR = 4)
//!   toss-plain    discard plans throw plain cards before enhanced, editioned or sealed ones (rank order otherwise)
//!   no-scorer-fix  v57's scorer: jokers read at their shown value even when the game updates them before the
//!                 hand scores (Obelisk, Green Joker, Ride the Bus), and a rightmost Blueprint copying the leftmost joker
//!   extra-chips   cards carry their permanent bonus chips (Hiker) into the bot's scorer and the rollouts' choices
//!   splash        with Splash held, the scorer counts every played card (it scored only the poker hand's cards)
//!   baseball-fix  Baseball Card's X1.5 applies at each Uncommon joker's place, after its effect (was all at once)
//!   pack-forecast  a pack's jokers go through the shop forecast (as $0 shop jokers) instead of the Picker's gain
//!   no-rollout-shop  v59's frozen-build forecast rollouts (v60: rollouts shop, simcore::rollout_shop)
//!   rollout-shop=F  the rollouts' joker floor: they buy the priciest affordable joker while $F stays (default 10)
//!   rollout-shop-first  rollouts take the first affordable joker in the shop (in effect random), not the priciest
//!   rollout-free-first  rollouts buy a free (tag) joker before the priciest one
//!   rollout-shop-nopolicy  rollouts shop but do not rebuild their play policy after buying a joker
//!   skip-forecast=A  up to ante A, skip-or-play at a Small/Big blind is decided by a forecast from the blind select
//!                 with the real tags (forecast::skip_forecast), not the static tag list
//!   skip-forecast-add=A  the static skips stay; up to ante A the skip forecast may add a skip for another tag
//!   play-now-ratio=R  CodeJev plays now only if the best play's estimate >= R x the best discard's modeled next
//!                 play (default codejev::PLAY_NOW_RATIO = 0.6 since v55; was 1)
//!   play-now-spare=R  the play-now ratio while discards left >= hands left (spare discards)
//!   boss-reroll=A+B  at blind select, reroll the ante's boss when it is one of these ("Violet_Vessel+The_Wall")
//!                 with Retcon, or Director's Cut once per ante, and $10 (sim view only: BalatroBot has no endpoint)
//!   strong-vouchers  the production rule in the lab: decisions::VOUCHERS marked strong are bought from money
//!                 above the interest floor (DecisionCfg::lab turns it off)
//!   buy-vouchers=A+B more vouchers treated as strong ("Blank+Clearance_Sale"; _ for a space)
//!   voucher-floor=F a strong voucher is bought while $F stays (default: the interest floor)
//!   money-tail=W  forecast rollouts score blinds cleared + W * BLINDS_PER_DOLLAR[ante] * money at their end
//!                 (rollouts never shop, so money and prices were worth nothing to the forecast)
//!   value-tail=W  forecast rollouts score blinds cleared + W * P(win) * blinds left at their end (v53: default
//!                 VALUE_TAIL, also in production; value-tail=0 turns it off)
//!                 (value_model.rs, learned from 5,000 lab games)

use std::sync::OnceLock;

pub const PAIR_PLANET: f64 = 0.5;
pub const PAIR_SE: f64 = 1.0;
pub const PAIR_PLAY: f64 = 0.85;
/// v53: the forecasts' value tail by default (production and lab); `value-tail=0` turns it off.
pub const VALUE_TAIL: f64 = 0.5;
/// Jokers that pay for playing Pairs (or for repeating one hand, which Pair commitment does).
pub const PAIR_JOKERS: &[&str] = &["Jolly Joker", "Sly Joker", "The Duo", "Half Joker", "Supernova", "Card Sharp", "Spare Trousers", "Mad Joker", "Clever Joker"];

#[derive(Debug, Default)]
pub struct Exp {
    pub pair_planets: bool,
    pub pair_jokers: bool,
    pub pair_keep: bool,
    pub pair_play: bool,
    pub value_tail: f64,
    /// `power-bonus=W`: each forecast option scores + W x (log10 p75 power of its state - the skip option's), in blinds
    /// (value_model::power_log on the option applied, before any rollout; ~1 blind per 10x power mid-game by the power model)
    pub power_bonus: f64,
    /// `power-bonus-ante=A`: the power bonus only from ante A (default every ante)
    pub power_bonus_ante: i64,
    /// v61: through ante 6, a shop joker's forecast delta gains B x (its value - the sold joker's) blinds (B = JOKER_PRIOR;
    /// lab `joker-prior=B`, `no-joker-prior`), values from data/joker_values.json (lab: JOKER_PRIOR=<json> instead)
    pub joker_prior: f64,
    /// lab `joker-prior-from=A`: the joker prior only from ante A (v61: every ante through 6)
    pub joker_prior_from: i64,
    /// lab `joker-prior-to=A`: the joker prior through ante A (v61: 6)
    pub joker_prior_to: i64,
    /// lab `joker-prior-early-to=A`: the JOKER_PRIOR_EARLY table through ante A (default 3)
    pub joker_prior_early_to: i64,
    /// v62: the Picker's joker value (score gain, $1 = 0.01) gains P x the joker's learned value (floor 0.01); P = PACK_PRIOR
    /// (lab `pack-prior=P`, `no-pack-prior`)
    pub pack_prior: f64,
    /// `prior-sell`: swap sell candidates add the held joker with the lowest JOKER_PRIOR value
    pub prior_sell: bool,
    /// `prior-safe=K`: the joker prior x min(1, p_survive(option) / p_survive(skip))^K (horizon survival from the forecast)
    pub prior_safe: f64,
    /// `rollout-shop-value`: rollouts buy the joker with the highest learned value (simcore::ROLLOUT_SHOP_VALUE)
    pub rollout_shop_value: bool,
    /// `rollout-last-discard`: rollouts may discard before a last hand that cannot clear (simcore::LAST_DISCARD)
    pub rollout_last_discard: bool,
    pub free_discard: bool,
    pub money_tail: f64,
    pub eye_plan: bool,
    pub late_floor: i64,
    pub skip_tags: Vec<String>,
    pub skip_min: i64,
    pub skip_max: Option<i64>,
    pub fc_bosses: Option<i64>,
    pub fc_n: Option<i64>,
    pub max_rerolls: Option<i64>,
    pub swap_sells: Option<usize>,
    pub synth: Option<usize>,
    pub planet_deals: Option<usize>,
    pub sure_se: Option<f64>,
    pub boss_reroll: Vec<String>,
    pub rollout_nodiscard: bool,
    pub rollout_discard2: usize,
    pub play_now_ratio: Option<f64>,
    pub play_now_spare: Option<f64>,
    pub rollout_behind: Option<f64>,
    pub discard_key: Option<String>,
    pub clear_margin: Option<f64>,
    pub clear_risky: Option<f64>,
    pub setup_play: Option<String>,
    pub cj_fc_n: Option<i64>,
    pub no_order_search: bool,
    pub card_order: bool,
    pub tarot_measure: bool,
    pub fc_order: bool,
    pub early_jokers: Option<i64>,
    pub early_ante: Option<i64>,
    pub planet_floor: Option<i64>,
    pub toss_plain: bool,
    pub no_scorer_fix: bool,
    pub extra_chips: bool,
    pub splash: bool,
    pub baseball_fix: bool,
    pub pack_forecast: bool,
    pub rollout_shop: Option<i32>,
    pub rollout_shop_policy: bool,
    pub rollout_shop_first: bool,
    pub skip_forecast: Option<i64>,
    pub skip_forecast_add: Option<i64>,
    pub rollout_free_first: bool,
    pub strong_vouchers: bool,
    pub buy_vouchers: Vec<String>,
    pub voucher_floor: Option<i64>,
}

/// Future blinds cleared per $1 held at the first shop of each ante (index = ante; 5,000 lab games, ridge
/// regression with build controls, docs/lab-lessons.md).
pub const BLINDS_PER_DOLLAR: [f64; 10] = [0.0, 0.105, 0.105, 0.071, 0.040, 0.041, 0.034, 0.023, 0.012, 0.0];

pub fn get() -> &'static Exp {
    static E: OnceLock<Exp> = OnceLock::new();
    E.get_or_init(|| {
        let s = std::env::var("JEVLAB_EXP").unwrap_or_default();
        let has = |f: &str| s.split(',').any(|x| x.trim() == f || (x.trim() == "pair" && f.starts_with("pair-")));
        let num = |f: &str| s.split(',').find_map(|x| x.trim().strip_prefix(f)?.strip_prefix('=')?.parse::<f64>().ok()).unwrap_or(0.0);
        let e = Exp { pair_planets: has("pair-planets"), pair_jokers: has("pair-jokers"), pair_keep: has("pair-keep"), pair_play: has("pair-play"),
              value_tail: s.split(',').find_map(|x| x.trim().strip_prefix("value-tail=")?.parse().ok()).unwrap_or(VALUE_TAIL), free_discard: has("free-discard"), money_tail: num("money-tail"), power_bonus: num("power-bonus"), power_bonus_ante: num("power-bonus-ante") as i64, joker_prior: s.split(',').find_map(|x| x.trim().strip_prefix("joker-prior=")?.parse().ok()).unwrap_or(if has("no-joker-prior") { 0.0 } else { JOKER_PRIOR }), pack_prior: s.split(',').find_map(|x| x.trim().strip_prefix("pack-prior=")?.parse().ok()).unwrap_or(if has("no-pack-prior") { 0.0 } else { PACK_PRIOR }), joker_prior_from: num("joker-prior-from") as i64, joker_prior_to: s.split(',').find_map(|x| x.trim().strip_prefix("joker-prior-to=")?.parse().ok()).unwrap_or(6), joker_prior_early_to: s.split(',').find_map(|x| x.trim().strip_prefix("joker-prior-early-to=")?.parse().ok()).unwrap_or(3), prior_sell: has("prior-sell"), prior_safe: num("prior-safe"), rollout_shop_value: has("rollout-shop-value"), rollout_last_discard: has("rollout-last-discard"), eye_plan: has("eye-plan"),
              late_floor: num("late-floor") as i64,
              skip_tags: s.split(',').find_map(|x| x.trim().strip_prefix("skip-tags=")).map_or(vec![], |v| v.split('+').map(|t| format!("{t} Tag")).collect()),
              skip_min: num("skip-min") as i64,
              skip_max: s.split(',').find_map(|x| x.trim().strip_prefix("skip-max=")?.parse().ok()),
              fc_bosses: s.split(',').find_map(|x| x.trim().strip_prefix("fc-bosses=")?.parse().ok()),
              fc_n: s.split(',').find_map(|x| x.trim().strip_prefix("fc-n=")?.parse().ok()),
              max_rerolls: s.split(',').find_map(|x| x.trim().strip_prefix("max-rerolls=")?.parse().ok()),
              swap_sells: s.split(',').find_map(|x| x.trim().strip_prefix("swap-sells=")?.parse().ok()),
              synth: s.split(',').find_map(|x| x.trim().strip_prefix("synth=")?.parse().ok()),
              planet_deals: s.split(',').find_map(|x| x.trim().strip_prefix("planet-deals=")?.parse().ok()),
              sure_se: s.split(',').find_map(|x| x.trim().strip_prefix("sure-se=")?.parse().ok()),
              rollout_nodiscard: has("rollout-nodiscard"),
              rollout_discard2: num("rollout-discard2") as usize,
              play_now_ratio: s.split(',').find_map(|x| x.trim().strip_prefix("play-now-ratio=")?.parse().ok()),
              play_now_spare: s.split(',').find_map(|x| x.trim().strip_prefix("play-now-spare=")?.parse().ok()),
              rollout_behind: s.split(',').find_map(|x| x.trim().strip_prefix("rollout-behind=")?.parse().ok()),
              discard_key: s.split(',').find_map(|x| x.trim().strip_prefix("discard-key=")).map(String::from),
              clear_margin: s.split(',').find_map(|x| x.trim().strip_prefix("clear-margin=")?.parse().ok()),
              clear_risky: s.split(',').find_map(|x| x.trim().strip_prefix("clear-risky=")?.parse().ok()),
              setup_play: s.split(',').find_map(|x| x.trim().strip_prefix("setup-play=")).map(String::from),
              cj_fc_n: s.split(',').find_map(|x| x.trim().strip_prefix("cj-fc-n=")?.parse().ok()),
              no_order_search: has("no-order-search"),
              card_order: has("card-order"),
              tarot_measure: has("tarot-measure"),
              fc_order: has("fc-order"),
              early_jokers: s.split(',').find_map(|x| x.trim().strip_prefix("early-jokers=")?.parse().ok()),
              early_ante: s.split(',').find_map(|x| x.trim().strip_prefix("early-ante=")?.parse().ok()),
              planet_floor: s.split(',').find_map(|x| x.trim().strip_prefix("planet-floor=")?.parse().ok()),
              toss_plain: has("toss-plain"),
              no_scorer_fix: has("no-scorer-fix"),
              extra_chips: has("extra-chips"),
              splash: has("splash"),
              baseball_fix: has("baseball-fix"),
              pack_forecast: has("pack-forecast"),
              // v60: shopping rollouts with policy rebuilds by default; `no-rollout-shop` freezes the build again,
              // `rollout-shop-nopolicy` keeps the shopping without the rebuild
              rollout_shop_policy: !has("no-rollout-shop") && !has("rollout-shop-nopolicy"),
              rollout_shop_first: has("rollout-shop-first"),
              skip_forecast: s.split(',').find_map(|x| x.trim().strip_prefix("skip-forecast=")?.parse().ok()),
              skip_forecast_add: s.split(',').find_map(|x| x.trim().strip_prefix("skip-forecast-add=")?.parse().ok()),
              rollout_free_first: has("rollout-free-first"),
              rollout_shop: if has("no-rollout-shop") { None } else { Some(s.split(',').find_map(|x| x.trim().strip_prefix("rollout-shop=")?.parse().ok()).unwrap_or(10)) },
              boss_reroll: s.split(',').find_map(|x| x.trim().strip_prefix("boss-reroll=")).map_or(vec![], |v| v.split('+').map(|t| t.replace('_', " ")).collect()),
              strong_vouchers: has("strong-vouchers"),
              buy_vouchers: s.split(',').find_map(|x| x.trim().strip_prefix("buy-vouchers=")).map_or(vec![], |v| v.split('+').map(|t| t.replace('_', " ")).collect()),
              voucher_floor: s.split(',').find_map(|x| x.trim().strip_prefix("voucher-floor=")?.parse().ok()) };
        simcore::FREE_DISCARD.store(e.free_discard, std::sync::atomic::Ordering::Relaxed);
        simcore::NO_DISCARD.store(e.rollout_nodiscard, std::sync::atomic::Ordering::Relaxed);
        simcore::SMART_DISCARD.store(e.rollout_discard2, std::sync::atomic::Ordering::Relaxed);
        simcore::EXTRA_CHIPS.store(e.extra_chips, std::sync::atomic::Ordering::Relaxed);
        simcore::ROLLOUT_SHOP.store(e.rollout_shop.is_some(), std::sync::atomic::Ordering::Relaxed);
        simcore::ROLLOUT_SHOP_FIRST.store(e.rollout_shop_first, std::sync::atomic::Ordering::Relaxed);
        simcore::ROLLOUT_SHOP_VALUE.store(e.rollout_shop_value, std::sync::atomic::Ordering::Relaxed);
        simcore::LAST_DISCARD.store(e.rollout_last_discard, std::sync::atomic::Ordering::Relaxed);
        let _ = simcore::ROLLOUT_VALUE.set(joker_value);
        simcore::FREE_FIRST.store(e.rollout_free_first, std::sync::atomic::Ordering::Relaxed);
        if let Some(f) = e.rollout_shop {
            simcore::ROLLOUT_SHOP_FLOOR.store(f, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(k) = e.rollout_behind {
            simcore::ROLLOUT_BEHIND.store(k.to_bits(), std::sync::atomic::Ordering::Relaxed);
        }
        e
    })
}

pub fn is_pair_joker(label: Option<&str>) -> bool {
    label.is_some_and(|l| PAIR_JOKERS.contains(&l))
}

/// v61: the joker prior's weight, in blinds per unit of learned value.
pub const JOKER_PRIOR: f64 = 1.0;
/// v62: pack picks value a joker at its score gain + PACK_PRIOR x its learned value (lab `pack-prior=P`, `no-pack-prior`)
pub const PACK_PRIOR: f64 = 1.0;

/// v61: a joker's learned value (data/joker_values.json: the logit of winning when held at ante 5's first shop, from
/// 2,000 v60 lab games, docs/samples/cloud-lab/joker-study/jval.py; clipped to [-1, 1.5]); 0 for a joker it does not
/// list. Lab: JOKER_PRIOR=<file> loads another table.
pub fn joker_value(key: &str) -> f64 {
    static T: std::sync::OnceLock<std::collections::HashMap<String, f64>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        let text = match std::env::var("JOKER_PRIOR") {
            Ok(p) => std::fs::read_to_string(&p).expect("JOKER_PRIOR file"),
            Err(_) => include_str!("../data/joker_values.json").to_string(),
        };
        serde_json::from_str(&text).expect("joker values json")
    });
    t.get(key).cloned().unwrap_or(0.0)
}

/// Lab (`JOKER_PRIOR_EARLY=<json>`, through ante `joker-prior-early-to=A`, default 3): the joker prior's value in early
/// shops from a table measured at an earlier ante (economy jokers pay off as money, which the ante-5 model holds fixed);
/// falls back to joker_value for jokers it does not list.
pub fn joker_value_at(key: &str, ante: i64) -> f64 {
    static T: std::sync::OnceLock<std::collections::HashMap<String, f64>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| match std::env::var("JOKER_PRIOR_EARLY") {
        Ok(p) => serde_json::from_str(&std::fs::read_to_string(&p).expect("JOKER_PRIOR_EARLY file")).expect("JOKER_PRIOR_EARLY json"),
        Err(_) => Default::default(),
    });
    if ante <= get().joker_prior_early_to {
        if let Some(v) = t.get(key) {
            return *v;
        }
    }
    joker_value(key)
}

/// Lab (`JOKER_PAIRS=<json {"j_a|j_b": v}>`, keys sorted): the pair term of two held jokers (0 when unlisted or no file).
pub fn joker_pair(a: &str, b: &str) -> f64 {
    static T: std::sync::OnceLock<std::collections::HashMap<String, f64>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| match std::env::var("JOKER_PAIRS") {
        Ok(p) => serde_json::from_str(&std::fs::read_to_string(&p).expect("JOKER_PAIRS file")).expect("JOKER_PAIRS json"),
        Err(_) => Default::default(),
    });
    if t.is_empty() {
        return 0.0;
    }
    let k = if a <= b { format!("{a}|{b}") } else { format!("{b}|{a}") };
    t.get(&k).cloned().unwrap_or(0.0)
}
