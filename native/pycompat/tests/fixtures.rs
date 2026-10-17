//! Checks every row of runs/_port/fixtures/pycompat.jsonl (made by scripts/port/fixtures/pycompat.py).

use pycompat::statistics::PyNum;
use pycompat::*;
use serde_json::Value;
use std::collections::BTreeMap;

fn fixture_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("PYCOMPAT_FIXTURE") {
        return p.into();
    }
    "C:/Users/admin/Documents/Repos/balatro-rust/runs/_port/fixtures/pycompat.jsonl".into()
}

fn f(h: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(h.as_str().unwrap(), 16).unwrap())
}
fn hexs(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}
fn fl(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(f).collect()
}
fn il(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
}
fn num(n: PyNum) -> Value {
    match n {
        PyNum::Int(i) => serde_json::json!(["int", i]),
        PyNum::Float(x) => serde_json::json!(["float", hexs(x)]),
    }
}
fn fnum(x: f64) -> Value {
    serde_json::json!(["float", hexs(x)])
}
fn mk(seed: &Value) -> Random {
    match seed {
        Value::String(s) => Random::new_str(s),
        v => Random::new_int(v.as_i64().unwrap()),
    }
}

fn run(fnname: &str, a: &[Value]) -> Value {
    match fnname {
        "pyfloat" => Value::String(pyfloat(f(&a[0]))),
        "fixed" => Value::String(fixed(f(&a[0]), a[1].as_u64().unwrap() as usize)),
        "signed" => Value::String(signed(f(&a[0]), a[1].as_u64().unwrap() as usize)),
        "pyround" => Value::String(hexs(pyround(f(&a[0]), a[1].as_i64().unwrap() as i32))),
        "pyround_int" => serde_json::json!(pyround_int(f(&a[0]))),
        "fsum" => Value::String(hexs(fsum(&fl(&a[0])))),
        "comb" => Value::String(comb(a[0].as_u64().unwrap(), a[1].as_u64().unwrap()).to_string()),
        "random" => {
            let mut r = mk(&a[0]);
            Value::Array((0..a[1].as_u64().unwrap()).map(|_| Value::String(hexs(r.random()))).collect())
        }
        "getrandbits" => {
            let mut r = mk(&a[0]);
            Value::Array(a[1].as_array().unwrap().iter()
                .map(|k| Value::String(r.getrandbits(k.as_u64().unwrap() as u32).to_string())).collect())
        }
        "randbelow" => {
            let mut r = mk(&a[0]);
            Value::Array(a[1].as_array().unwrap().iter()
                .map(|n| Value::String(r.randbelow(n.as_u64().unwrap()).to_string())).collect())
        }
        "randrange" => {
            let mut r = mk(&a[0]);
            Value::Array(a[1].as_array().unwrap().iter()
                .map(|p| serde_json::json!(r.randrange(p[0].as_i64().unwrap(), p[1].as_i64().unwrap()))).collect())
        }
        "choice" => {
            let mut r = mk(&a[0]);
            Value::Array(a[1].as_array().unwrap().iter()
                .map(|n| serde_json::json!(r.choice_index(n.as_u64().unwrap() as usize))).collect())
        }
        "shuffle" => {
            let mut r = mk(&a[0]);
            Value::Array(a[1].as_array().unwrap().iter().map(|n| {
                let mut x: Vec<usize> = (0..n.as_u64().unwrap() as usize).collect();
                r.shuffle(&mut x);
                serde_json::json!(x)
            }).collect())
        }
        "sample" => {
            let mut r = mk(&a[0]);
            Value::Array(a[1].as_array().unwrap().iter().map(|p| {
                serde_json::json!(r.sample_indices(p[0].as_u64().unwrap() as usize, p[1].as_u64().unwrap() as usize))
            }).collect())
        }
        "sample_stream" => {
            let mut r = mk(&a[0]);
            let (n, k, reps) = (a[1].as_u64().unwrap() as usize, a[2].as_u64().unwrap() as usize, a[3].as_u64().unwrap());
            Value::Array((0..reps).map(|_| serde_json::json!(r.sample_indices(n, k))).collect())
        }
        "mean_int" => num(statistics::mean_int(&il(&a[0]))),
        "mean_f64" => fnum(statistics::mean_f64(&fl(&a[0]))),
        "stdev_int" => fnum(statistics::stdev_int(&il(&a[0]))),
        "stdev_f64" => fnum(statistics::stdev_f64(&fl(&a[0]))),
        "median_int" => num(statistics::median_int(&il(&a[0]))),
        "median_f64" => fnum(statistics::median_f64(&fl(&a[0]))),
        "json_dumps" => Value::String(json::dumps(&a[0])),
        other => panic!("unknown fn {other}"),
    }
}

#[test]
fn all_rows_identical() {
    let text = std::fs::read_to_string(fixture_path()).expect("fixture missing: run scripts/port/fixtures/pycompat.py");
    let mut tot: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut bad: Vec<String> = vec![];
    for line in text.lines().filter(|l| !l.is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let name = row["fn"].as_str().unwrap().to_string();
        let args = row["args"].as_array().unwrap();
        let got = run(&name, args);
        let e = tot.entry(name.clone()).or_default();
        e.1 += 1;
        if serde_json::to_string(&got).unwrap() == serde_json::to_string(&row["ret"]).unwrap() {
            e.0 += 1;
        } else if bad.iter().filter(|b| b.starts_with(&name)).count() < 3 {
            let s = |v: &Value| { let t = v.to_string(); t.chars().take(160).collect::<String>() };
            bad.push(format!("{name} args={} want={} got={}", s(&row["args"]), s(&row["ret"]), s(&got)));
        }
    }
    let mut ok = true;
    for (k, (a, b)) in &tot {
        println!("{k}: {a}/{b} identical");
        ok &= a == b;
    }
    for b in &bad {
        println!("MISMATCH {b}");
    }
    assert!(ok, "some rows differ");
}
