//! Behaviour-cloning log: one record per rumbot action taken in a strategic phase (BlindSelect,
//! RoundEval, Shop, Pack), written as one gzip file per game at game end (tmp file + rename).
//!
//! File `<seed>.rlb.gz`, little endian inside the gzip stream:
//!
//! ```text
//! header  b"BRL1" u32 schema_version  u64 schema_hash  u32 obs_dim  u32 n_actions  u16 seed_len  seed
//!         u32 n_records
//! record  u32 step  i16 ante  u8 phase  u8 blind_index  u16 round  i32 money  u16 blinds_cleared
//!         u16 action_id (0xFFFF = unexpressible)  u8 action_type  i16 action_index  u8 n_cards  u8[n_cards]
//!         u8 flags  u16 nnz  u16[nnz] obs_index  f32[nnz] obs_value  u16 n_legal  u16[n_legal] legal_ids
//! footer  u8 won  i16 ante  u16 blinds  u16 round  u32 n_counters  { u16 len  name  u64 value }*
//! ```
//!
//! `step` counts every rumbot step of the game (Playing included). `phase` / `action_type` are the
//! `rumbot_sim::game::{Phase, ActionType}` enum positions. `blinds_cleared` is the blinds beaten before
//! this action (`round_number`: rounds started, all of them won while the run is alive outside Playing).
//! flags: 1 chosen id in the legal mask, 2 round trip gives the same type and item, 4 round trip gives
//! the identical action (slot too), 8 the action carries card targets, 16 the round trip kept them,
//! 32 the default target choice (no hint) equals them, 64 rumbot called the action legal.

use crate::obs::Encoder;
use crate::vocab::{legal_with_ids, strategic_phase, to_action_from, vocab_of};
use rumbot_sim::game::{Action, ActionType, GameState, Phase};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 4] = b"BRL1";

pub fn phase_code(p: Phase) -> u8 {
    match p {
        Phase::BlindSelect => 0,
        Phase::Playing => 1,
        Phase::RoundEval => 2,
        Phase::Shop => 3,
        Phase::Pack => 4,
        Phase::GameOver => 5,
        Phase::Won => 6,
    }
}

pub const ACTION_TYPES: [ActionType; 18] = [
    ActionType::CashOut,
    ActionType::SelectBlind,
    ActionType::SkipBlind,
    ActionType::Play,
    ActionType::Discard,
    ActionType::UseConsumable,
    ActionType::SellJoker,
    ActionType::SwapJokerLeft,
    ActionType::SellConsumable,
    ActionType::Buy,
    ActionType::BuyAndUse,
    ActionType::RerollBoss,
    ActionType::BuyVoucher,
    ActionType::Reroll,
    ActionType::BuyPack,
    ActionType::PickPack,
    ActionType::SkipPack,
    ActionType::LeaveShop,
];

pub fn action_type_code(t: ActionType) -> u8 {
    ACTION_TYPES.iter().position(|x| *x == t).unwrap() as u8
}

pub struct GameLog {
    seed: String,
    enc: Encoder,
    steps: u32,
    n_records: u32,
    buf: Vec<u8>,
    pub counters: BTreeMap<String, u64>,
    /// seconds spent in `on_step` (encoding, masks, round trip)
    pub secs: f64,
}

fn put_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn put_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

impl GameLog {
    pub fn new(seed: &str) -> Self {
        GameLog { seed: seed.to_string(), enc: Encoder::new(), steps: 0, n_records: 0, buf: Vec::with_capacity(1 << 16), counters: BTreeMap::new(), secs: 0.0 }
    }

    fn count(&mut self, k: &str) {
        *self.counters.entry(k.to_string()).or_insert(0) += 1;
    }

    pub fn records(&self) -> u32 {
        self.n_records
    }

    /// Called with the state before `a` is stepped.
    pub fn on_step(&mut self, g: &GameState, a: &Action) {
        let step = self.steps;
        self.steps += 1;
        if !strategic_phase(g.phase) {
            if !matches!(a.r#type, ActionType::Play | ActionType::Discard) {
                self.count(&format!("in_round:{}", a.r#type.as_str()));
            }
            return;
        }
        let t0 = std::time::Instant::now();
        let (idx, val) = self.enc.encode(g);
        let legal = legal_with_ids(g);
        let mut mask: Vec<u16> = legal.iter().map(|(v, _)| *v as u16).collect();
        mask.sort_unstable();
        mask.dedup();
        let mut flags = 0u8;
        if g.is_legal(a) {
            flags |= 64;
        } else {
            self.count("action_illegal");
        }
        if !a.cards.is_empty() {
            flags |= 8;
            self.count("targets_given");
        }
        let id = match vocab_of(g, a) {
            Ok(v) => {
                if mask.binary_search(&(v as u16)).is_ok() {
                    flags |= 1;
                } else {
                    self.count("not_in_mask");
                }
                let cands: Vec<Action> = legal.iter().filter(|(x, _)| *x == v).map(|(_, a)| a.clone()).collect();
                match to_action_from(g, &cands, Some(&a.cards)) {
                    Some(b) => {
                        if b.r#type == a.r#type && vocab_of(g, &b) == Ok(v) {
                            flags |= 2;
                        } else {
                            self.count("roundtrip_identity_mismatch");
                        }
                        if b == *a {
                            flags |= 4;
                        } else if b.index != a.index {
                            self.count("roundtrip_other_slot");
                        }
                        if !a.cards.is_empty() {
                            if b.cards == a.cards {
                                flags |= 16;
                            } else {
                                self.count("roundtrip_targets_lost");
                            }
                            if to_action_from(g, &cands, None).is_some_and(|d| d.cards == a.cards) {
                                flags |= 32;
                                self.count("targets_default_match");
                            }
                        }
                    }
                    None => self.count("roundtrip_none"),
                }
                v as u16
            }
            Err(r) => {
                self.count("unexpressible");
                self.count(&format!("unexpressible:{}", r.as_str()));
                0xFFFF
            }
        };
        self.count(&format!("records:{}", g.phase.as_str()));
        self.count(&format!("type:{}", a.r#type.as_str()));
        let b = &mut self.buf;
        put_u32(b, step);
        b.extend_from_slice(&(g.ante as i16).to_le_bytes());
        b.push(phase_code(g.phase));
        b.push(g.blind_index.clamp(0, 255) as u8);
        put_u16(b, g.round_number.max(0) as u16);
        b.extend_from_slice(&g.money.to_le_bytes());
        put_u16(b, g.round_number.max(0) as u16);
        put_u16(b, id);
        b.push(action_type_code(a.r#type));
        b.extend_from_slice(&(a.index as i16).to_le_bytes());
        b.push(a.cards.len() as u8);
        b.extend(a.cards.iter().map(|&c| c as u8));
        b.push(flags);
        put_u16(b, idx.len() as u16);
        for i in &idx {
            put_u16(b, *i);
        }
        for v in &val {
            b.extend_from_slice(&v.to_le_bytes());
        }
        put_u16(b, mask.len() as u16);
        for m in &mask {
            put_u16(b, *m);
        }
        self.n_records += 1;
        self.secs += t0.elapsed().as_secs_f64();
    }

    /// Write `<dir>/<seed>.rlb.gz` (atomically) with the game's result; returns (path, bytes written).
    pub fn write(&self, dir: &Path, won: bool, ante: i32, blinds: i32, round: i32) -> std::io::Result<(PathBuf, u64)> {
        std::fs::create_dir_all(dir)?;
        write_schema(dir)?;
        let mut out = Vec::with_capacity(self.buf.len() + 256);
        out.extend_from_slice(MAGIC);
        put_u32(&mut out, crate::SCHEMA_VERSION);
        out.extend_from_slice(&crate::schema_hash().to_le_bytes());
        put_u32(&mut out, crate::obs_dim() as u32);
        put_u32(&mut out, crate::n_actions() as u32);
        put_u16(&mut out, self.seed.len() as u16);
        out.extend_from_slice(self.seed.as_bytes());
        put_u32(&mut out, self.n_records);
        out.extend_from_slice(&self.buf);
        out.push(won as u8);
        out.extend_from_slice(&(ante as i16).to_le_bytes());
        put_u16(&mut out, blinds.max(0) as u16);
        put_u16(&mut out, round.max(0) as u16);
        let mut counters = self.counters.clone();
        counters.insert("steps".into(), self.steps as u64);
        counters.insert("log_us".into(), (self.secs * 1e6) as u64);
        put_u32(&mut out, counters.len() as u32);
        for (k, v) in &counters {
            put_u16(&mut out, k.len() as u16);
            out.extend_from_slice(k.as_bytes());
            out.extend_from_slice(&v.to_le_bytes());
        }
        let path = dir.join(format!("{}.rlb.gz", self.seed));
        let tmp = dir.join(format!(".{}.rlb.gz.tmp{}", self.seed, std::process::id()));
        {
            let f = std::fs::File::create(&tmp)?;
            let mut gz = flate2::write::GzEncoder::new(std::io::BufWriter::new(f), flate2::Compression::new(6));
            gz.write_all(&out)?;
            gz.finish()?.flush()?;
        }
        std::fs::rename(&tmp, &path)?;
        let n = std::fs::metadata(&path)?.len();
        Ok((path, n))
    }
}

/// `<dir>/schema.json`, written once (same content from every game of a schema version).
pub fn write_schema(dir: &Path) -> std::io::Result<()> {
    let p = dir.join("schema.json");
    let s = crate::schema_json();
    if std::fs::read_to_string(&p).is_ok_and(|x| x == s) {
        return Ok(());
    }
    let tmp = dir.join(format!(".schema.json.tmp{}.{:?}", std::process::id(), std::thread::current().id()));
    std::fs::write(&tmp, &s)?;
    std::fs::rename(&tmp, &p)
}
