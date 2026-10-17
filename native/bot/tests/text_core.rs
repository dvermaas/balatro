//! simtext.rs against runs/_port/fixtures/text_core.jsonl.gz (scripts/port/fixtures/text_core.py).
//! Rows compare as serialized JSON (key order, int/float types).

use jevbot::{simtext as t, trace};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

fn s(v: &Value) -> &str {
    v.as_str().expect("str")
}

fn parts_json(lines: &[Vec<t::Part>]) -> Value {
    Value::Array(lines.iter().map(|l| Value::Array(l.iter().map(|p| p.to_json()).collect())).collect())
}

fn strs(v: Vec<String>) -> Value {
    json!(v)
}

fn run(f: &str, a: &[Value]) -> Value {
    let vars = |i: usize| -> Vec<Value> { a[i].as_array().expect("vars").clone() };
    let ef = |v: &Value| v.as_f64();
    match f {
        "parse_lua_table_file" => {
            let p = Path::new("C:/Users/admin/Documents/Repos/balatro-rust/runs/_loc").join(s(&a[0]));
            t::parse_lua_table(&std::fs::read_to_string(p).unwrap().replace("\r\n", "\n"))
        }
        "parse_lua_table" => t::parse_lua_table(s(&a[0])),
        "loc" => t::loc().clone(),
        "game_hands" => t::game_hands().clone(),
        "parse_line" => match t::parse_line(s(&a[0])) {
            None => Value::Null,
            Some(p) => Value::Array(p.iter().map(|x| x.to_json()).collect()),
        },
        "_text_parsed" => parts_json(&t::_text_parsed(&t::loc()["descriptions"][s(&a[0])][s(&a[1])])),
        "desc" => match t::desc(s(&a[0]), s(&a[1])) {
            None => Value::Null,
            Some(l) => json!(l.iter().map(|x| x.len()).collect::<Vec<_>>()),
        },
        "localize_desc" => strs(t::localize_desc(s(&a[0]), s(&a[1]), &vars(2))),
        "raw_lines" => match t::desc(s(&a[0]), s(&a[1])) {
            Some(l) if !l.is_empty() => strs(t::raw_lines(l, &vars(2))),
            _ => json!([]),
        },
        "localize_other" => strs(t::localize_other(s(&a[0]), &vars(1))),
        "name_text" => t::name_text(s(&a[0]), s(&a[1])),
        "misc" => t::misc(s(&a[0]), s(&a[1])),
        "dictionary" => t::dictionary(s(&a[0])),
        "variable" => json!(t::variable(s(&a[0]), &vars(1))),
        "poker_hand" => t::poker_hand(s(&a[0])),
        "lua_tostring" => json!(t::lua_tostring(&a[0])),
        "format_ui_value" => json!(t::format_ui_value(&a[0])),
        "signed" => json!(t::signed(&a[0])),
        "signed_dollars" => json!(t::signed_dollars(&a[0])),
        "number_format" => t::number_format(&a[0], ef(&a[1])),
        _ => panic!("unknown fn {f}"),
    }
}

#[test]
fn text_core() {
    let rows = trace::read_jsonl(&trace::fixtures_dir().join("text_core.jsonl.gz"));
    let mut stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad: Vec<String> = vec![];
    for r in &rows {
        let f = s(&r["fn"]);
        let a = r["args"].as_array().unwrap();
        let got = std::panic::catch_unwind(|| run(f, a));
        let e = stats.entry(f.to_string()).or_default();
        e.1 += 1;
        let ok = match &got {
            Ok(g) => serde_json::to_string(g).unwrap() == serde_json::to_string(&r["ret"]).unwrap(),
            Err(_) => false,
        };
        if ok {
            e.0 += 1;
        } else if bad.len() < 5 {
            bad.push(format!(
                "{f} {} -> got {} want {}",
                trace::short(&r["args"], 100),
                got.map(|g| trace::short(&g, 100)).unwrap_or("PANIC".into()),
                trace::short(&r["ret"], 100)
            ));
        }
    }
    let mut all = true;
    for (f, (ok, n)) in &stats {
        println!("{f}: {ok}/{n} identical");
        all &= ok == n;
    }
    for b in &bad {
        println!("MISMATCH {b}");
    }
    assert!(all);
}
