//! Item identity tables: every joker, consumable, voucher, booster kind, tag and blind gets a fixed
//! 0-based id, frozen from the engine's own key lists (`rumbot_sim::vocabulary`, sorted the way the
//! engine sorts `G.P_CENTERS`). Ids depend only on those frozen lists, never on rumbot's registries, so
//! an id means the same item in the simulator and in the real game.

use rumbot_sim::cards::{Edition, Enhancement, Rank, Seal, Suit};
use rumbot_sim::shop::{PackKind, PackSpec};
use rumbot_sim::vocabulary::{BLIND_LIST, KEY_LIST, TAG_LIST};
use std::collections::HashMap;
use std::sync::OnceLock;

pub const N_JOKERS: usize = 150;
pub const N_CONSUMABLES: usize = 52; // 22 Tarot + 12 Planet + 18 Spectral
pub const N_VOUCHERS: usize = 32;
pub const N_PACK_KINDS: usize = 15; // 5 kinds x 3 sizes
pub const N_TAGS: usize = 24;
pub const N_BLINDS: usize = 30;

pub struct Ids {
    pub joker_keys: Vec<&'static str>,
    pub consumable_keys: Vec<&'static str>,
    pub voucher_keys: Vec<&'static str>,
    pub tag_keys: Vec<&'static str>,
    pub blind_keys: Vec<&'static str>,
    joker_by_key: HashMap<&'static str, usize>,
    consumable_by_key: HashMap<&'static str, usize>,
    voucher_by_key: HashMap<&'static str, usize>,
    tag_by_key: HashMap<&'static str, usize>,
    blind_by_key: HashMap<&'static str, usize>,
    /// rumbot joker / consumable *names* -> id (rumbot instances carry names, not keys)
    joker_by_name: HashMap<&'static str, usize>,
    consumable_by_name: HashMap<&'static str, usize>,
}

static IDS: OnceLock<Ids> = OnceLock::new();

pub fn ids() -> &'static Ids {
    IDS.get_or_init(build)
}

fn build() -> Ids {
    let (mut jk, mut ck, mut vk) = (Vec::new(), Vec::new(), Vec::new());
    for entry in KEY_LIST.split(',') {
        let (key, set) = entry.split_once('|').unwrap_or((entry, ""));
        match set {
            "Joker" => jk.push(key),
            "Tarot" | "Planet" | "Spectral" => ck.push(key),
            "Voucher" => vk.push(key),
            _ => {}
        }
    }
    let tk: Vec<&'static str> = TAG_LIST.split(',').filter(|s| !s.is_empty()).collect();
    let bk: Vec<&'static str> = BLIND_LIST.split(',').filter(|s| !s.is_empty()).collect();
    assert_eq!((jk.len(), ck.len(), vk.len(), tk.len(), bk.len()), (N_JOKERS, N_CONSUMABLES, N_VOUCHERS, N_TAGS, N_BLINDS));
    let idx = |v: &Vec<&'static str>| v.iter().enumerate().map(|(i, k)| (*k, i)).collect::<HashMap<_, _>>();
    let mut joker_by_name = HashMap::new();
    for (i, k) in jk.iter().enumerate() {
        if let Some(n) = rumbot_sim::shop_pool::name_by_joker_key(k) {
            joker_by_name.insert(n, i);
        }
    }
    let mut consumable_by_name = HashMap::new();
    for (i, k) in ck.iter().enumerate() {
        if let Some(n) = rumbot_sim::shop_pool::name_by_consumable_key(k) {
            consumable_by_name.insert(n, i);
        }
    }
    Ids {
        joker_by_key: idx(&jk),
        consumable_by_key: idx(&ck),
        voucher_by_key: idx(&vk),
        tag_by_key: idx(&tk),
        blind_by_key: idx(&bk),
        joker_keys: jk,
        consumable_keys: ck,
        voucher_keys: vk,
        tag_keys: tk,
        blind_keys: bk,
        joker_by_name,
        consumable_by_name,
    }
}

impl Ids {
    /// A rumbot joker name ("Riff-Raff") -> joker id.
    pub fn joker(&self, name: &str) -> Option<usize> {
        self.joker_by_name.get(name).copied().or_else(|| {
            rumbot_sim::shop_pool::key_by_joker_name(name).and_then(|k| self.joker_by_key.get(k).copied())
        })
    }
    /// A rumbot consumable name ("The Fool") -> consumable id.
    pub fn consumable(&self, name: &str) -> Option<usize> {
        self.consumable_by_name.get(name).copied().or_else(|| {
            rumbot_sim::shop_pool::key_by_consumable_name(name).and_then(|k| self.consumable_by_key.get(k).copied())
        })
    }
    pub fn voucher(&self, key: &str) -> Option<usize> {
        self.voucher_by_key.get(key).copied()
    }
    pub fn tag(&self, key: &str) -> Option<usize> {
        self.tag_by_key.get(key).copied()
    }
    pub fn blind(&self, key: &str) -> Option<usize> {
        self.blind_by_key.get(key).copied()
    }
}

pub const PACK_KINDS: [PackKind; 5] = [PackKind::Arcana, PackKind::Celestial, PackKind::Standard, PackKind::Buffoon, PackKind::Spectral];
pub const PACK_SIZES: [&str; 3] = ["normal", "jumbo", "mega"];

pub fn pack_kind_index(k: PackKind) -> usize {
    PACK_KINDS.iter().position(|x| *x == k).unwrap()
}
pub fn pack_size_index(size: &str) -> usize {
    PACK_SIZES.iter().position(|x| *x == size).unwrap_or(0)
}
/// A booster's identity: (kind, size). The four art variants of a key (`p_arcana_normal_1..4`) are one item.
pub fn pack_id(p: &PackSpec) -> usize {
    pack_kind_index(p.kind) * 3 + pack_size_index(p.size)
}
pub fn pack_name(i: usize) -> String {
    format!("{}_{}", PACK_KINDS[i / 3].as_str(), PACK_SIZES[i % 3])
}

/// Engine order (`bot_api.lua` ids, as `rumbot_sim::state` uses): ranks Two..Ace, suits S H C D.
pub fn rank_index(r: Rank) -> usize {
    r.value() as usize - 2
}
pub fn suit_index(s: Suit) -> usize {
    rumbot_sim::state::suit_id(s) as usize - 1
}
/// 0 = none, foil, holo, polychrome, negative.
pub fn edition_index(e: Edition) -> usize {
    rumbot_sim::state::edition_id(e) as usize
}
/// 0 = none, gold, red, blue, purple.
pub fn seal_index(s: Seal) -> usize {
    rumbot_sim::state::seal_id(s) as usize
}
/// 0 = none, bonus, mult, wild, glass, steel, stone, gold, lucky.
pub fn enhancement_index(e: Enhancement) -> usize {
    rumbot_sim::state::enhancement_id(rumbot_sim::state::enhancement_key(e)) as usize
}
pub const RANK_NAMES: [&str; 13] = ["2", "3", "4", "5", "6", "7", "8", "9", "T", "J", "Q", "K", "A"];
pub const SUIT_NAMES: [&str; 4] = ["S", "H", "C", "D"];
pub const EDITION_NAMES: [&str; 5] = ["none", "foil", "holo", "polychrome", "negative"];
pub const SEAL_NAMES: [&str; 5] = ["none", "gold", "red", "blue", "purple"];
pub const ENH_NAMES: [&str; 9] = ["none", "bonus", "mult", "wild", "glass", "steel", "stone", "gold", "lucky"];
