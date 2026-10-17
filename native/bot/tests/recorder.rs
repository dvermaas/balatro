//! Replays the calls of scripts/port/fixtures/recorder.py (the Python RunRecorder with a fixed clock)
//! into a temp dir and compares every produced file with the Python output.
//! Plain files must match byte for byte. states.jsonl.gz: the gzip header (mtime, FNAME, OS, XFL)
//! must match byte for byte and the decoded content must match; the deflate stream itself is not
//! compared, because flate2 (miniz_oxide) and CPython's zlib emit different bytes for the same input.

use flate2::read::MultiGzDecoder;
use jevbot::recorder::{self, Clock, RunRecorder};
use jevbot::trace;
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

struct FixedClock {
    t: f64,
    stamp: String,
}

impl Clock for FixedClock {
    fn time(&self) -> f64 {
        self.t
    }
    fn stamp(&self) -> String {
        self.stamp.clone()
    }
}

/// Every file under dir as (path relative to base with '/', bytes), sorted by name.
fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let p = entry.expect("entry").path();
        if p.is_dir() {
            walk(&p, base, out);
        } else {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, fs::read(&p).expect("read file")));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
}

fn scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("recorder_test_{}_{name}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).expect("scratch dir");
    p
}

/// Expected bytes of one fixture file entry: {"text": ...} or {"hex": ...}.
fn expected_bytes(v: &Value) -> Vec<u8> {
    if let Some(t) = v.get("text") {
        return t.as_str().expect("text").as_bytes().to_vec();
    }
    let h = v["hex"].as_str().expect("hex");
    (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect()
}

fn gunzip(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    MultiGzDecoder::new(b).read_to_end(&mut out).expect("gunzip");
    out
}

/// Header of a gzip member written with FNAME "states.jsonl": 10 fixed bytes + name + NUL.
fn same_file(name: &str, got: &[u8], want: &[u8]) -> bool {
    if !name.ends_with(".gz") {
        return got == want;
    }
    let header = 10 + "states.jsonl".len() + 1;
    if got.len() < header || want.len() < header || got[..header] != want[..header] {
        return false;
    }
    gunzip(got) == gunzip(want)
}

fn replay_trace(row: &Value, root: &Path) -> Vec<(String, Vec<u8>)> {
    let init = &row["init"];
    let clock = FixedClock { t: init["time"].as_f64().unwrap(), stamp: init["stamp"].as_str().unwrap().into() };
    let mut rec = RunRecorder::new(
        root,
        init["backend"].as_str().unwrap(),
        init["version"].as_str().unwrap(),
        init["scorer"].as_str().unwrap(),
        init["rollouts"].as_str().unwrap(),
        Box::new(clock),
    );
    for op in row["ops"].as_array().unwrap() {
        match op["op"].as_str().unwrap() {
            "ensure" => rec.ensure(&op["gs"], &op["deck"], &op["stake"], op["model"].as_str().unwrap(), &op["bench"]),
            "write_raw" => rec.write_raw(op["id"].as_i64().unwrap(), &op["gs"]),
            "write" => rec.write(&op["ev"]),
            "progress" => rec.progress(&op["gs"]),
            "finish" => {
                let gs = if op["gs"].is_null() { None } else { Some(&op["gs"]) };
                rec.finish(op["result"].as_str().unwrap(), gs)
            }
            other => panic!("unknown op {other}"),
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn recorder_matches_python_byte_for_byte() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("recorder.jsonl.gz"));
    let mut identical = 0;
    let mut mismatches: Vec<String> = Vec::new();
    for row in &rows {
        match row["fn"].as_str().unwrap() {
            "version_hash" => {
                let files: Vec<&str> = row["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
                let got = recorder::version_hash(&files, Path::new(row["root"].as_str().unwrap()));
                if got == row["ret"].as_str().unwrap() {
                    identical += 1;
                } else {
                    mismatches.push(format!("version_hash got {got} want {}", row["ret"]));
                }
            }
            "trace" => {
                let root = scratch(row["trace"].as_str().unwrap());
                let got = replay_trace(row, &root);
                let _ = fs::remove_dir_all(&root);
                let mut want: Vec<(String, Vec<u8>)> =
                    row["files"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), expected_bytes(v))).collect();
                want.sort_by(|a, b| a.0.cmp(&b.0));
                let same = got.len() == want.len()
                    && got.iter().zip(want.iter()).all(|((gn, gb), (wn, wb))| gn == wn && same_file(gn, gb, wb));
                if same {
                    identical += 1;
                } else {
                    let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
                    let wnames: Vec<&str> = want.iter().map(|(n, _)| n.as_str()).collect();
                    mismatches.push(format!("trace {}: files {names:?} vs {wnames:?}; {:?}", row["trace"],
                        got.iter().zip(want.iter()).filter(|((gn, gb), (wn, wb))| !(gn == wn && same_file(gn, gb, wb)))
                            .map(|((gn, _), _)| gn.clone()).collect::<Vec<_>>()));
                }
            }
            other => panic!("unknown fixture fn {other}"),
        }
    }
    for m in mismatches.iter().take(5) {
        eprintln!("MISMATCH {m}");
    }
    println!("{identical}/{} identical", rows.len());
    assert!(mismatches.is_empty(), "{} rows differ", mismatches.len());
}
