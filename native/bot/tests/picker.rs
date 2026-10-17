//! Identity test for rollout_items.py Picker / synth_plays: fixtures/picker.jsonl.gz (synth_plays, pick_pub,
//! values on trace gamestates) and every Picker.values / Picker.pick_pub "oracle" line of the traces.

use jevbot::picker::{pick_json, synth_plays, Picker, SYNTH_HANDS};
use jevbot::trace;
use serde_json::Value;
use std::collections::HashMap;

fn s(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}
fn list(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
fn vals(p: &mut Picker, a: &Value) -> Value {
    let (pack, hand, jk) = (list(&a["pack"]), list(&a["hand"]), list(&a["jokers"]));
    Value::Array(p.values(&pack, &hand, &a["money"], &jk, a["joker_limit"].as_i64().unwrap()).iter().map(|v| v.to_json()).collect())
}

#[test]
fn picker_identical() {
    let mut tally: HashMap<String, (usize, usize)> = HashMap::new();
    let mut bad: Vec<String> = Vec::new();
    let mut note = |f: &str, ok: bool, msg: &dyn Fn() -> String| {
        let t = tally.entry(f.to_string()).or_default();
        t.1 += 1;
        if ok { t.0 += 1 } else if bad.len() < 6 { bad.push(format!("{f}: {}", msg())) }
    };
    // fixtures
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("picker.jsonl.gz"));
    let mut st: HashMap<String, Vec<Value>> = HashMap::new();
    for r in &rows {
        let arm = r["arm"].as_str().unwrap().to_string();
        let states = st.entry(arm.clone()).or_insert_with(|| trace::states(&arm, None));
        let g = states[r["i"].as_u64().unwrap() as usize].clone();
        let f = r["fn"].as_str().unwrap();
        let got = match f {
            "synth_plays" => Value::Array(synth_plays(&g, SYNTH_HANDS, "SYNTH")),
            "pick_pub" => pick_json(Picker::new(&g).pick_pub(&g)),
            "values" => vals(&mut Picker::new(&g), r),
            _ => panic!("fn {f}"),
        };
        note(&format!("fixture {f}"), s(&got) == s(&r["ret"]), &|| format!("{} got {} want {}", r["i"], trace::short(&got, 200), trace::short(&r["ret"], 200)));
    }
    // oracle lines
    for arm in ["bot-forecast-items", "bot-forecast-long"] {
        for path in trace::trace_files(arm) {
            let mut pubs: Option<Value> = None;
            for r in trace::load_trace(&path) {
                match r["k"].as_str() {
                    Some("call") => pubs = r.get("ok").cloned(),
                    Some("oracle") => {
                        let f = r["fn"].as_str().unwrap_or("");
                        if f != "Picker.values" && f != "Picker.pick_pub" { continue; }
                        let g = pubs.clone().expect("oracle before call");
                        let got = if f == "Picker.values" { vals(&mut Picker::new(&g), &r["args"]) } else { pick_json(Picker::new(&g).pick_pub(&g)) };
                        note(f, s(&got) == s(&r["ret"]), &|| format!("got {} want {}", trace::short(&got, 200), trace::short(&r["ret"], 200)));
                    }
                    _ => {}
                }
            }
        }
    }
    let mut keys: Vec<_> = tally.keys().cloned().collect();
    keys.sort();
    for k in &keys { println!("{k}: {}/{} identical", tally[k].0, tally[k].1); }
    assert!(bad.is_empty(), "mismatches:\n{}", bad.join("\n"));
}
