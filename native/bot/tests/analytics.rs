//! Replays the calls of scripts/port/fixtures/analytics.py (the Python analytics.py on recorded runs)
//! and compares every return value with the Python one: same keys in the same order, same int/float
//! types, same rounding. Compared as canonical JSON text, so key order and 2 vs 2.0 both count.

use jevbot::analytics;
use jevbot::trace;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

fn text(args: &Value, k: &str) -> String {
    args[k].as_str().expect("string arg").to_string()
}

fn call(f: &str, a: &Value) -> Value {
    match f {
        "prompt_versions" => Value::Array(analytics::prompt_versions(Path::new(&text(a, "prompts_dir")), &text(a, "current"))),
        "run_backend" => Value::String(analytics::run_backend(&a["run"])),
        "load_decisions" => Value::Array(analytics::load_decisions(Path::new(&text(a, "run_dir")))),
        "run_metrics" => analytics::run_metrics(Path::new(&text(a, "run_dir"))),
        "all_runs" => Value::Array(analytics::all_runs(Path::new(&text(a, "root")))),
        "version_stats" => Value::Array(analytics::version_stats(
            &text(a, "backend"),
            Path::new(&text(a, "root")),
            Path::new(&text(a, "prompts_dir")),
            &text(a, "current"),
        )),
        "benchmark_matrix" => analytics::benchmark_matrix(
            a["name"].as_str(),
            &text(a, "backend"),
            Path::new(&text(a, "root")),
            Path::new(&text(a, "prompts_dir")),
            &text(a, "current"),
        ),
        other => panic!("unknown fn {other}"),
    }
}

#[test]
fn analytics_identical() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("analytics.jsonl.gz"));
    let mut per: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut first_bad: Option<String> = None;
    for row in &rows {
        let f = row["fn"].as_str().expect("fn").to_string();
        let got = serde_json::to_string(&call(&f, &row["args"])).expect("json");
        let want = serde_json::to_string(&row["ret"]).expect("json");
        let e = per.entry(f.clone()).or_insert((0, 0));
        e.1 += 1;
        if got == want {
            e.0 += 1;
        } else if first_bad.is_none() {
            let at = got.bytes().zip(want.bytes()).position(|(x, y)| x != y).unwrap_or(got.len().min(want.len()));
            let lo = at.saturating_sub(120);
            first_bad = Some(format!(
                "{f} {}\n rust:   ...{}\n python: ...{}",
                row["args"],
                &got[lo..(at + 120).min(got.len())],
                &want[lo..(at + 120).min(want.len())]
            ));
        }
    }
    let mut total_ok = 0;
    let mut total = 0;
    for (f, (ok, n)) in &per {
        println!("{f}: {ok}/{n} identical");
        total_ok += ok;
        total += n;
    }
    if let Some(msg) = &first_bad {
        println!("first mismatch: {msg}");
    }
    println!("{total_ok}/{total} identical");
    assert_eq!(total_ok, total);
}
