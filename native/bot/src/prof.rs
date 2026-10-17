//! Coarse profiling timers (JEV_PERF=1): accumulated ns and call counts per label.
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

static ACC: Mutex<BTreeMap<&'static str, (u64, u64)>> = Mutex::new(BTreeMap::new());

pub fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("JEV_PERF").is_ok())
}

pub fn time<T>(label: &'static str, f: impl FnOnce() -> T) -> T {
    if !on() {
        return f();
    }
    let t = Instant::now();
    let r = f();
    let ns = t.elapsed().as_nanos() as u64;
    let mut a = ACC.lock().unwrap();
    let e = a.entry(label).or_insert((0, 0));
    e.0 += ns;
    e.1 += 1;
    r
}

pub fn dump() {
    for (k, (ns, n)) in ACC.lock().unwrap().iter() {
        eprintln!("PROF {k:28} {:9.3} s  {n:8} calls  {:9.1} us/call", *ns as f64 / 1e9, *ns as f64 / 1e3 / *n as f64);
    }
}
