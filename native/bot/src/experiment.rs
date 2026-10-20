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
              value_tail: s.split(',').find_map(|x| x.trim().strip_prefix("value-tail=")?.parse().ok()).unwrap_or(VALUE_TAIL), free_discard: has("free-discard"), money_tail: num("money-tail"), eye_plan: has("eye-plan"),
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
              boss_reroll: s.split(',').find_map(|x| x.trim().strip_prefix("boss-reroll=")).map_or(vec![], |v| v.split('+').map(|t| t.replace('_', " ")).collect()),
              strong_vouchers: has("strong-vouchers"),
              buy_vouchers: s.split(',').find_map(|x| x.trim().strip_prefix("buy-vouchers=")).map_or(vec![], |v| v.split('+').map(|t| t.replace('_', " ")).collect()),
              voucher_floor: s.split(',').find_map(|x| x.trim().strip_prefix("voucher-floor=")?.parse().ok()) };
        simcore::FREE_DISCARD.store(e.free_discard, std::sync::atomic::Ordering::Relaxed);
        simcore::NO_DISCARD.store(e.rollout_nodiscard, std::sync::atomic::Ordering::Relaxed);
        simcore::SMART_DISCARD.store(e.rollout_discard2, std::sync::atomic::Ordering::Relaxed);
        e
    })
}

pub fn is_pair_joker(label: Option<&str>) -> bool {
    label.is_some_and(|l| PAIR_JOKERS.contains(&l))
}
