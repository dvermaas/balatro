//! Port of src/recorder.py (RunRecorder, version_hash): one run dir per seed under the log root with
//! decisions.jsonl, states.jsonl.gz and summary.json. tests/recorder.rs replays the calls of
//! scripts/port/fixtures/recorder.py and compares every file byte for byte.
//!
//! Not ported: snapshot_prompts / prompt_versions (they import prompts.py and write runs/_prompts)
//! and list_runs (the panel reads the summaries). `scorer` and `rollouts` are native.backend() and
//! native.rollout_backend() in Python, so they are constructor arguments here.

use flate2::{Compression, GzBuilder};
use pycompat::json::dumps;
use pycompat::pyround;
use serde_json::{json, Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::gs::truthy;

/// Python text mode on Windows writes "\r\n" for every "\n" (decisions.jsonl, summary.json and the
/// lines inside states.jsonl.gz), so the port does the same.
#[cfg(windows)]
const NL: &str = "\r\n";
#[cfg(not(windows))]
const NL: &str = "\n";

/// The recorder's wall clock: time.time() and time.strftime("%Y%m%d-%H%M%S"). Injected so tests can fix it.
pub trait Clock {
    fn time(&self) -> f64;
    fn stamp(&self) -> String;
}

/// System clock: `time()` is epoch seconds; `stamp()` is local time like Python's
/// time.strftime("%Y%m%d-%H%M%S") (Windows GetLocalTime; other platforms fall back to UTC).
pub struct SystemClock;

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct SystemTime {
    year: u16,
    month: u16,
    dow: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    ms: u16,
}

#[cfg(windows)]
extern "system" {
    fn GetLocalTime(t: *mut SystemTime);
}

/// Local wall-clock "YYYYmmdd-HHMMSS" right now.
fn local_stamp(utc_secs: i64) -> String {
    #[cfg(windows)]
    {
        let _ = utc_secs;
        let mut t = SystemTime::default();
        unsafe { GetLocalTime(&mut t) };
        format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second)
    }
    #[cfg(not(windows))]
    {
        utc_stamp(utc_secs)
    }
}

/// "YYYYmmdd-HHMMSS" of an epoch second in UTC.
pub fn utc_stamp(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
    let rem = secs.rem_euclid(86400);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

impl SystemClock {
    /// The same instant as `stamp()` in UTC (tests compare the two to check the local offset).
    pub fn stamp_utc_for_test(&self) -> String {
        utc_stamp(self.time().floor() as i64)
    }
}

impl Clock for SystemClock {
    fn time(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
    fn stamp(&self) -> String {
        local_stamp(self.time().floor() as i64)
    }
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// SHA-1 (FIPS 180-4), hand-written so the crate needs no new dependency.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (hi, v) in h.iter_mut().zip([a, b, c, d, e]) {
            *hi = hi.wrapping_add(v);
        }
    }
    let mut out = [0u8; 20];
    for (i, hi) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&hi.to_be_bytes());
    }
    out
}

/// recorder.version_hash(files, root): sha1 of the files' exact bytes (missing files skipped), first
/// 8 hex chars. Python passes its own .py list and src/; the Rust bot will pass its own sources.
pub fn version_hash(files: &[&str], root: &Path) -> String {
    let mut buf = Vec::new();
    for f in files {
        let p = root.join(f);
        if p.exists() {
            buf.extend(fs::read(&p).expect("read hashed file"));
        }
    }
    sha1(&buf)[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// The Rust sources that define behaviour, sorted, relative to `native/`.
pub fn behaviour_sources(native: &Path) -> Vec<String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let path = e.path();
            let r = format!("{rel}/{name}");
            if path.is_dir() {
                walk(&path, &r, out);
            } else if name.ends_with(".rs") {
                out.push(r);
            }
        }
    }
    let mut out = Vec::new();
    for d in ["bot/src", "simcore/src", "kernel/src", "rumbot/src", "pycompat/src"] {
        walk(&native.join(d), d, &mut out);
    }
    out.sort();
    out
}

/// The Rust bot's prompt version: `version_hash` over `behaviour_sources` (sha1 of their exact bytes).
pub fn bot_version(native: &Path) -> String {
    let files = behaviour_sources(native);
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    version_hash(&refs, native)
}

/// The `native/` directory of this checkout (compile-time path; override with JEV_NATIVE_DIR).
pub fn native_dir() -> PathBuf {
    std::env::var_os("JEV_NATIVE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
}

/// `recorder.snapshot_prompts()`: save the prompt text under `version` in `prompts_dir`
/// (index.json order + `<version>.json`), once per version.
pub fn snapshot_prompts(prompts_dir: &Path, version: &str, created: f64) {
    fs::create_dir_all(prompts_dir).expect("create prompts dir");
    let index_path = prompts_dir.join("index.json");
    let mut index: Value = if index_path.exists() {
        serde_json::from_str(&fs::read_to_string(&index_path).expect("read index.json")).expect("index json")
    } else {
        json!({"order": []})
    };
    let order = index["order"].as_array_mut().expect("order");
    if order.iter().any(|v| v == version) {
        return;
    }
    order.push(json!(version));
    let mut out = String::new();
    dumps_indent(&index, 0, &mut out);
    fs::write(&index_path, text_out(&out)).expect("write index.json");
    let kv = |a: &[(&str, &str)]| Value::Object(a.iter().map(|(k, v)| (k.to_string(), json!(v))).collect());
    let snap = json!({"version": version, "created": created, "note": crate::prompts::VERSION_NOTE,
        "rules": crate::prompts::GAME_RULES, "instructions": kv(crate::prompts::INSTRUCTIONS),
        "options": kv(crate::prompts::OPTIONS)});
    let mut out = String::new();
    dumps_indent(&snap, 0, &mut out);
    fs::write(prompts_dir.join(format!("{version}.json")), text_out(&out)).expect("write snapshot");
}

/// json.dumps(v, indent=2). pycompat's dumps has no indent option, so the containers are laid out
/// here and every scalar goes through pycompat::json::dumps. Empty containers stay on one line.
fn dumps_indent(v: &Value, level: usize, out: &mut String) {
    let pad = |n: usize| " ".repeat(2 * n);
    match v {
        Value::Object(m) if !m.is_empty() => {
            out.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&pad(level + 1));
                out.push_str(&dumps(&Value::String(k.clone())));
                out.push_str(": ");
                dumps_indent(x, level + 1, out);
            }
            out.push('\n');
            out.push_str(&pad(level));
            out.push('}');
        }
        Value::Array(a) if !a.is_empty() => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&pad(level + 1));
                dumps_indent(x, level + 1, out);
            }
            out.push('\n');
            out.push_str(&pad(level));
            out.push(']');
        }
        _ => out.push_str(&dumps(v)),
    }
}

/// Text-mode newline translation (see NL).
fn text_out(s: &str) -> String {
    s.replace('\n', NL)
}

/// d.get(k) with None for a missing key.
fn get(v: &Value, k: &str) -> Value {
    v.get(k).cloned().unwrap_or(Value::Null)
}

fn as_count(v: Option<&Value>) -> i64 {
    v.and_then(Value::as_i64).unwrap_or(0)
}

fn bump(r: &mut Map<String, Value>, key: &str, n: i64) {
    let c = as_count(r.get(key));
    r.insert(key.to_string(), json!(c + n));
}

/// Python `a or b` on gamestate values.
fn or(a: Value, b: Value) -> Value {
    if truthy(&a) {
        a
    } else {
        b
    }
}

fn append_text(path: &Path, text: &str) {
    let mut f = OpenOptions::new().create(true).append(true).open(path).expect("open decisions.jsonl");
    f.write_all(text.as_bytes()).expect("write decisions.jsonl");
}

/// One gzip member per call, as Python's gzip.open(path, "at") writes: header mtime from the clock,
/// FNAME "states.jsonl" (the basename minus .gz), compresslevel 9.
fn append_gz(path: &Path, text: &str, mtime: u32) {
    let f = OpenOptions::new().create(true).append(true).open(path).expect("open states.jsonl.gz");
    let mut gz = GzBuilder::new().filename("states.jsonl").mtime(mtime).write(f, Compression::new(9));
    gz.write_all(text.as_bytes()).expect("write states.jsonl.gz");
    gz.finish().expect("finish states.jsonl.gz");
}

pub struct RunRecorder {
    root: PathBuf,
    backend: String,
    version: String,
    scorer: String,
    rollouts: String,
    clock: Box<dyn Clock>,
    run: Option<Map<String, Value>>,
    dir: Option<PathBuf>,
    conf_sum: f64,
}

/// Python `RunRecorder.list_runs(limit)` on `root`: the summary.json of each run dir, newest first (dir names
/// start with the start time). Like Python's `sorted(glob("*/summary.json"), reverse=True)[:limit or None]`,
/// the limit applies before unreadable summaries are skipped; `limit` 0 means no limit. Windows paths
/// compare case-insensitively, so names are lowercased for the sort.
pub fn list_runs(root: &Path, limit: usize) -> Vec<Value> {
    let mut dirs: Vec<PathBuf> = match fs::read_dir(root) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("summary.json").is_file()).collect(),
        Err(_) => return vec![],
    };
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default());
    dirs.reverse();
    if limit > 0 {
        dirs.truncate(limit);
    }
    dirs.iter()
        .filter_map(|d| {
            let text = fs::read_to_string(d.join("summary.json")).ok()?;
            serde_json::from_str::<Value>(&text).ok()
        })
        .collect()
}

impl RunRecorder {
    /// The runs root this recorder writes under.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The prompt version it stamps on runs.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// `RunRecorder.list_runs(limit)` for this recorder's root (see the free `list_runs`).
    pub fn list_runs(&self, limit: usize) -> Vec<Value> {
        list_runs(&self.root, limit)
    }

    /// The current run directory (None before the first `ensure`).
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// RunRecorder(root, backend). `version` is the bot's PROMPT_VERSION.
    pub fn new(
        root: impl Into<PathBuf>, backend: &str, version: &str, scorer: &str, rollouts: &str,
        clock: Box<dyn Clock>,
    ) -> Self {
        let root = root.into();
        fs::create_dir_all(&root).expect("create log root");
        RunRecorder {
            root,
            backend: backend.to_string(),
            version: version.to_string(),
            scorer: scorer.to_string(),
            rollouts: rollouts.to_string(),
            clock,
            run: None,
            dir: None,
            conf_sum: 0.0,
        }
    }

    /// ensure(gs, deck, stake, model, bench): start a run dir for a new seed (finishing the old one).
    pub fn ensure(&mut self, gs: &Value, deck: &Value, stake: &Value, model: &str, bench: &Value) {
        let seed = get(gs, "seed");
        if !truthy(&seed) || get(gs, "state").as_str() == Some("MENU") {
            return;
        }
        if self.run.as_ref().map_or(false, |r| r.get("seed") == Some(&seed)) {
            return;
        }
        if self.run.is_some() {
            self.finish("abandoned", Some(gs));
        }
        let started = self.clock.stamp();
        let seed_s = match &seed {
            Value::String(s) => s.clone(),
            other => dumps(other),
        };
        let mut dir = self.root.join(format!("{started}-{seed_s}"));
        let mut n = 1;
        while dir.exists() {
            // parallel sim workers can start the same seed in the same second
            n += 1;
            dir = self.root.join(format!("{started}-{seed_s}-{n}"));
        }
        fs::create_dir(&dir).expect("create run dir");
        let mut run = Map::new();
        run.insert("run_id".into(), json!(dir.file_name().unwrap().to_string_lossy()));
        run.insert("seed".into(), seed);
        run.insert("deck".into(), or(get(gs, "deck"), deck.clone()));
        run.insert("stake".into(), or(get(gs, "stake"), stake.clone()));
        run.insert("model".into(), json!(model));
        run.insert("prompt_version".into(), json!(self.version));
        run.insert("benchmark".into(), bench.clone());
        run.insert("backend".into(), json!(self.backend));
        run.insert("scorer".into(), json!(self.scorer));
        run.insert("rollouts".into(), json!(self.rollouts));
        run.insert("started".into(), json!(self.clock.time()));
        run.insert("ended".into(), Value::Null);
        run.insert("result".into(), json!("in_progress"));
        run.insert("ante".into(), get(gs, "ante_num"));
        run.insert("round".into(), get(gs, "round_num"));
        run.insert("money".into(), get(gs, "money"));
        for k in ["decisions", "forced", "errors", "input_tokens", "output_tokens", "jev_ms"] {
            run.insert(k.into(), json!(0));
        }
        run.insert("avg_confidence".into(), Value::Null);
        run.insert("low_confidence_decisions".into(), json!(0));
        run.insert("by_question".into(), json!({}));
        self.dir = Some(dir);
        self.run = Some(run);
        self.conf_sum = 0.0;
        self.save();
    }

    /// write_raw(event_id, gs): full gamestate per Jev decision, one gzip member per call.
    pub fn write_raw(&self, event_id: i64, gs: &Value) {
        let Some(dir) = self.dir.as_ref().filter(|_| self.run.is_some()) else { return };
        let line = dumps(&json!({"id": event_id, "gs": gs})) + "\n";
        append_gz(&dir.join("states.jsonl.gz"), &text_out(&line), self.clock.time() as u32);
    }

    /// write(ev): append the event and update the run's counters.
    pub fn write(&mut self, ev: &Value) {
        let Some(dir) = self.dir.as_ref().filter(|_| self.run.is_some()) else { return };
        append_text(&dir.join("decisions.jsonl"), &text_out(&(dumps(ev) + "\n")));
        let kind = get(ev, "kind");
        match kind.as_str() {
            Some("decision") => {
                let conf = if truthy(&get(ev, "confidence")) { get(ev, "confidence").as_f64().unwrap_or(0.0) } else { 0.0 };
                self.conf_sum += conf;
                let conf_sum = self.conf_sum;
                let r = self.run.as_mut().unwrap();
                bump(r, "decisions", 1);
                bump(r, "jev_ms", as_count(ev.get("ms")));
                bump(r, "input_tokens", as_count(ev.get("input_tokens")));
                bump(r, "output_tokens", as_count(ev.get("output_tokens")));
                let n = as_count(r.get("decisions")) as f64;
                r.insert("avg_confidence".into(), json!(pyround(conf_sum / n, 3)));
                if conf < 0.5 {
                    bump(r, "low_confidence_decisions", 1);
                }
                let q = match get(ev, "question") {
                    Value::String(s) => s,
                    other => dumps(&other),
                };
                let bq = r.get_mut("by_question").unwrap().as_object_mut().unwrap();
                let c = as_count(bq.get(&q));
                bq.insert(q, json!(c + 1));
                self.save();
            }
            Some("forced") => bump(self.run.as_mut().unwrap(), "forced", 1),
            Some("error") => {
                bump(self.run.as_mut().unwrap(), "errors", 1);
                self.save();
            }
            _ => {}
        }
    }

    /// progress(gs): refresh ante / round / money while the run's seed is current.
    pub fn progress(&mut self, gs: &Value) {
        let Some(r) = self.run.as_mut() else { return };
        if r.get("seed") == Some(&get(gs, "seed")) {
            r.insert("ante".into(), get(gs, "ante_num"));
            r.insert("round".into(), get(gs, "round_num"));
            r.insert("money".into(), get(gs, "money"));
        }
    }

    /// finish(result, gs): close the run and drop the current run.
    pub fn finish(&mut self, result: &str, gs: Option<&Value>) {
        if self.run.is_none() {
            return;
        }
        if let Some(g) = gs.filter(|g| truthy(g)) {
            self.progress(g);
        }
        let r = self.run.as_mut().unwrap();
        r.insert("result".into(), json!(result));
        r.insert("ended".into(), json!(self.clock.time()));
        self.save();
        self.run = None;
    }

    fn save(&self) {
        let mut s = String::new();
        dumps_indent(&Value::Object(self.run.clone().unwrap()), 0, &mut s);
        fs::write(self.dir.as_ref().unwrap().join("summary.json"), text_out(&s)).expect("write summary.json");
    }
}
