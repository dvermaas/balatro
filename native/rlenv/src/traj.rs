//! Trajectory files from the fast environment: `<seed>.brt.gz`, the rlb layout (rlcore/src/log.rs) with
//! magic `BRT1` and four f32 after each record's legal ids, plus power snapshots in the footer:
//!
//! ```text
//! header  b"BRT1"  (then exactly the rlb header)
//! record  (the rlb record)  f32 logp  f32 value_win  f32 value_blinds  f32 reward
//! footer  (the rlb footer)  u16 n_power  { i16 ante  u16 n  f32[n] log10 power }*
//! ```
//! `reward` = blinds cleared between this decision and the next one (or the game's end); flags are
//! 1|2|4|64 (chosen by id, so always in the mask) plus 8 when the action carries card targets.

use crate::env::GameOut;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn write(dir: &Path, o: &GameOut) -> std::io::Result<(PathBuf, u64)> {
    std::fs::create_dir_all(dir)?;
    rlcore::log::write_schema(dir)?;
    let mut b: Vec<u8> = Vec::with_capacity(64 * 1024);
    b.extend_from_slice(b"BRT1");
    b.extend_from_slice(&rlcore::SCHEMA_VERSION.to_le_bytes());
    b.extend_from_slice(&rlcore::schema_hash().to_le_bytes());
    b.extend_from_slice(&(rlcore::obs_dim() as u32).to_le_bytes());
    b.extend_from_slice(&(rlcore::n_actions() as u32).to_le_bytes());
    b.extend_from_slice(&(o.seed.len() as u16).to_le_bytes());
    b.extend_from_slice(o.seed.as_bytes());
    b.extend_from_slice(&(o.records.len() as u32).to_le_bytes());
    for r in &o.records {
        b.extend_from_slice(&r.step.to_le_bytes());
        b.extend_from_slice(&r.ante.to_le_bytes());
        b.push(r.phase);
        b.push(r.blind_index);
        b.extend_from_slice(&r.round.to_le_bytes());
        b.extend_from_slice(&r.money.to_le_bytes());
        b.extend_from_slice(&r.blinds_cleared.to_le_bytes());
        b.extend_from_slice(&r.action_id.to_le_bytes());
        b.push(r.atype);
        b.extend_from_slice(&r.aindex.to_le_bytes());
        b.push(r.cards.len() as u8);
        b.extend_from_slice(&r.cards);
        b.push(1 | 2 | 4 | 64 | if r.cards.is_empty() { 0 } else { 8 });
        b.extend_from_slice(&(r.obs_idx.len() as u16).to_le_bytes());
        for i in &r.obs_idx {
            b.extend_from_slice(&i.to_le_bytes());
        }
        for v in &r.obs_val {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&(r.legal.len() as u16).to_le_bytes());
        for m in &r.legal {
            b.extend_from_slice(&m.to_le_bytes());
        }
        for f in [r.logp, r.value[0], r.value[1], r.reward] {
            b.extend_from_slice(&f.to_le_bytes());
        }
    }
    b.push(o.won as u8);
    b.extend_from_slice(&(o.ante as i16).to_le_bytes());
    b.extend_from_slice(&(o.blinds.max(0) as u16).to_le_bytes());
    b.extend_from_slice(&(o.round.max(0) as u16).to_le_bytes());
    let counters: Vec<(&str, u64)> = vec![
        ("decisions", o.decisions as u64),
        ("empty_hand", o.empty_hand as u64),
        ("reorders", o.reorders as u64),
        ("targets_fallback", o.targets_fallback as u64),
        ("targets_planned", o.targets_planned as u64),
        ("truncated", o.truncated as u64),
    ];
    b.extend_from_slice(&(counters.len() as u32).to_le_bytes());
    for (k, v) in counters {
        b.extend_from_slice(&(k.len() as u16).to_le_bytes());
        b.extend_from_slice(k.as_bytes());
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&(o.power.len() as u16).to_le_bytes());
    for (a, p) in &o.power {
        b.extend_from_slice(&(*a as i16).to_le_bytes());
        b.extend_from_slice(&(p.len() as u16).to_le_bytes());
        for x in p {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    let path = dir.join(format!("{}.brt.gz", o.seed));
    let tmp = dir.join(format!(".{}.brt.gz.tmp{}", o.seed, std::process::id()));
    {
        let f = std::fs::File::create(&tmp)?;
        let mut gz = flate2::write::GzEncoder::new(std::io::BufWriter::new(f), flate2::Compression::new(6));
        gz.write_all(&b)?;
        gz.finish()?.flush()?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok((path.clone(), std::fs::metadata(&path)?.len()))
}
