//! Port of src/readiness.py. See docs/rust-port-guide.md.
//! Boss readiness: can our deck + jokers + hand levels beat the next blinds (sampled power vs pace).

use crate::hands::{Allowed, Info, Levels, ScorerArg};
use crate::{gs, hands, jokers};
use pycompat::statistics::{self, PyNum};
use pycompat::{pyround, Random};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// readiness.py 17
const BASE: [i64; 8] = [300, 800, 2000, 5000, 11000, 20000, 35000, 50000];

// readiness.py 18: BLIND_MULT
fn blind_mult(k: &str) -> f64 {
    match k {
        "small" => 1.0,
        "big" => 1.5,
        _ => 2.0,
    }
}

// readiness.py 23-24: str(c.get(k) or "") for rank, suit, enh, edition, seal, debuff
fn key(c: &Info) -> [String; 6] {
    [
        c.rank.clone().unwrap_or_default(),
        c.suit.clone().unwrap_or_default(),
        c.enh.clone().unwrap_or_default(),
        c.edition.clone().unwrap_or_default(),
        c.seal.clone().unwrap_or_default(),
        if c.debuff { "True".to_string() } else { String::new() },
    ]
}

// readiness.py 67-69: the hand's sort key (rank index first, stone cards unranked)
fn hand_sort_key(c: &Info) -> (i64, [String; 6]) {
    let first = match c.rank_truthy() {
        Some(r) if !c.enh_is("STONE") => -((hands::rank_index(r) as i64) + 2),
        _ => 0,
    };
    (first, key(c))
}

// readiness.py 40-41
pub fn base_amount(ante: i64) -> i64 {
    if ante <= 8 {
        BASE[(ante.max(1).min(8) - 1) as usize]
    } else {
        BASE[7] << (ante - 8)
    }
}

// readiness.py 44-57. A blind's `score` is used as is when truthy (int or float), else a float.
pub fn upcoming(g: &Value, ahead: usize) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::new();
    let order = ["small", "big", "boss"];
    let blinds = gs::get(g, "blinds");
    let ante_v = gs::get(g, "ante_num");
    let ante = if gs::truthy(ante_v) { gs::int(ante_v) } else { 1 };
    for k in order {
        let b = gs::get(blinds, k);
        if !gs::d(b).is_empty() {
            let status = gs::get(b, "status").as_str();
            if !matches!(status, Some("DEFEATED") | Some("SKIPPED")) {
                let name = gs::get(b, "name");
                let label_name = if gs::truthy(name) { gs::s(name).to_string() } else { k.to_string() };
                let score = gs::get(b, "score");
                let target = if gs::truthy(score) {
                    score.clone()
                } else {
                    json!(base_amount(ante) as f64 * blind_mult(k))
                };
                out.push((format!("ante {ante} {label_name}"), target));
            }
        }
    }
    let mut a = ante + 1;
    while out.len() < ahead {
        for k in order {
            out.push((format!("ante {a} {k} blind"), json!((base_amount(a) as f64 * blind_mult(k)) as i64)));
        }
        a += 1;
    }
    out.truncate(ahead);
    out
}

/// Python's `{"median", "p20", "samples"}`; the median is an int for odd n, a float for even n.
pub struct Power {
    pub median: PyNum,
    pub p20: i64,
    pub samples: usize,
}

impl Power {
    pub fn to_json(&self) -> Value {
        json!({"median": pynum_json(self.median), "p20": self.p20, "samples": self.samples})
    }
}

fn pynum_json(n: PyNum) -> Value {
    match n {
        PyNum::Int(i) => json!(i),
        PyNum::Float(f) => json!(f),
    }
}

// readiness.py 61: `_d(gs.get("hand")).get("limit") or 8`
fn hand_size(g: &Value) -> usize {
    let l = gs::get(gs::get(g, "hand"), "limit");
    if gs::truthy(l) {
        gs::int(l) as usize
    } else {
        8
    }
}

// readiness.py 58-60 and 107-108: the deck (draw pile, then hand), sorted by _key
fn sorted_deck(g: &Value) -> Vec<Info> {
    let mut deck: Vec<Info> = gs::cards_of(g, "cards").iter().map(hands::card_info).collect();
    deck.extend(gs::cards_of(g, "hand").iter().map(hands::card_info));
    deck.sort_by_cached_key(key);
    deck
}

// readiness.py 58-78 (js and levels at their defaults)
pub fn power(g: &Value, samples: usize) -> Option<Power> {
    let deck = sorted_deck(g);
    let size = hand_size(g);
    if deck.len() < size {
        return None;
    }
    let js = jokers::build(g, None);
    let levels: Levels = gs::area(g, "hands").clone();
    let scorer = jokers::Scorer::new(levels, js, jokers::Rnd { hands_left: 2.0, discards_left: 1.0 });
    let mut rng = Random::new_int(13013); // fixed local seed
    let mut scores: Vec<i64> = Vec::with_capacity(samples);
    for _ in 0..samples {
        let idx = rng.sample_indices(deck.len(), size);
        let mut hand: Vec<Info> = idx.iter().map(|&i| deck[i].clone()).collect();
        hand.sort_by_cached_key(hand_sort_key);
        scores.push(hands::best_score(&hand, ScorerArg::Jokers(&scorer), Allowed::None));
    }
    scores.sort();
    let n = scores.len();
    let median = statistics::median_int(&scores);
    let p20 = scores[(0.2 * (n as f64 - 1.0)) as usize];
    Some(Power { median, p20, samples: n })
}

pub const WALL_AHEAD: i64 = 2;
pub const MIX_EXTRA: usize = 3; // v15: deal hand size + 3 cards, roughly what one discard adds

// readiness.py 79-96
pub fn assess(g: &Value, hands_per_round: Option<i64>, samples: usize) -> Option<Value> {
    let p = power(g, samples)?;
    let hands = match hands_per_round {
        Some(h) if h != 0 => h,
        _ => 4,
    };
    let hf = hands as f64;
    let med = p.median.f64();
    let mut rows: Vec<Value> = Vec::new();
    let mut ratios: Vec<f64> = Vec::new();
    for (label, target) in upcoming(g, 3) {
        let t = gs::num(&target);
        let need = t / hf;
        let ratio = pyround(med / need, 2);
        ratios.push(ratio);
        rows.push(json!({"blind": label, "target": t.trunc() as i64, "ratio": ratio}));
    }
    // min(): the first minimal element
    let mut worst = ratios[0];
    for &r in &ratios[1..] {
        if r < worst {
            worst = r;
        }
    }
    // readiness.py 90-94: the wall, WALL_AHEAD antes on (capped at ante 8)
    let ante_v = gs::get(g, "ante_num");
    let ante = if gs::truthy(ante_v) { gs::int(ante_v) } else { 1 };
    let a = (ante + WALL_AHEAD).min(8);
    let target = base_amount(a) as f64 * blind_mult("boss");
    let wall = json!({"blind": format!("ante {a} boss"), "target": target.trunc() as i64,
                      "ratio": pyround(med / (target / hf), 2)});
    let typical = match p.median {
        PyNum::Int(i) => i,
        PyNum::Float(f) => f.trunc() as i64,
    };
    Some(json!({"typical_hand": typical, "weak_hand": p.p20, "hands": hands,
                "blinds": rows, "worst_ratio": worst, "wall": wall}))
}

// readiness.py 99-100: the process-wide cache (cleared past 256 entries)
fn mix_cache() -> &'static Mutex<HashMap<String, Value>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// readiness.py 103-150: {hand name: share of sampled best-play score}
pub fn play_mix(g: &Value, samples: usize) -> Value {
    // readiness.py 117-120: the signature (seed, ante, round, shop, joker labels, levels)
    let js_cards = gs::cards_of(g, "jokers");
    let levels: Levels = gs::area(g, "hands").clone();
    let mut lv: Vec<(&String, &Value)> = levels.iter().collect();
    lv.sort_by(|a, b| a.0.cmp(b.0));
    let sig = serde_json::to_string(&json!([
        gs::get(g, "seed"),
        gs::get(g, "ante_num"),
        gs::get(g, "round_num"),
        gs::get(g, "state").as_str() == Some("SHOP"),
        js_cards.iter().map(|c| gs::get(c, "label").clone()).collect::<Vec<Value>>(),
        lv.iter().map(|(k, v)| json!([k, gs::get(v, "level")])).collect::<Vec<Value>>(),
    ]))
    .expect("sig");
    if let Some(v) = mix_cache().lock().expect("mix cache").get(&sig) {
        return v.clone();
    }
    let deck = sorted_deck(g);
    let size = hand_size(g) + MIX_EXTRA;
    if deck.len() < size {
        return json!({});
    }
    let js = jokers::build(g, None);
    let scorer = jokers::Scorer::new(levels.clone(), js, jokers::Rnd { hands_left: 2.0, discards_left: 1.0 });
    let mut rng = Random::new_int(15015);
    let mut total: Vec<(String, i64)> = Vec::new(); // dict insertion order
    for _ in 0..samples {
        let idx = rng.sample_indices(deck.len(), size);
        let hand: Vec<Info> = idx.iter().map(|&i| deck[i].clone()).collect();
        let (score, name) = hands::best_play(&hand, &levels, ScorerArg::Jokers(&scorer), Allowed::None);
        if let Some(n) = name {
            match total.iter_mut().find(|(k, _)| k == n) {
                Some(e) => e.1 += score,
                None => total.push((n.to_string(), score)),
            }
        }
    }
    let sum: i64 = total.iter().map(|(_, v)| *v).sum();
    let norm = (if sum == 0 { 1 } else { sum }) as f64; // sum(...) or 1
    total.sort_by(|a, b| (-a.1).cmp(&(-b.1))); // stable, like sorted(key=-v)
    let mut mix = Map::new();
    for (k, v) in &total {
        mix.insert(k.clone(), json!(pyround(*v as f64 / norm, 3)));
    }
    let mix = Value::Object(mix);
    let mut cache = mix_cache().lock().expect("mix cache");
    if cache.len() > 256 {
        cache.clear();
    }
    cache.insert(sig, mix.clone());
    mix
}

// readiness.py 153-161. An early return gives `gain` back unchanged (an int stays an int).
pub fn lasting(gain: &Value, kind: Option<&str>, wall_ratio: &Value) -> Value {
    if !gs::truthy(gain) || matches!(kind, Some("xmult") | Some("scaling")) || !gs::truthy(wall_ratio) {
        return gain.clone();
    }
    let m = (1.0 / gs::num(wall_ratio)).max(1.0);
    json!(gs::num(gain) / m.powf(0.5))
}
