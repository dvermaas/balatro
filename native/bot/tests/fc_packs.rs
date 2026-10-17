//! Identity check for src/forecast_items.rs against scripts/port/fixtures/fc_packs.py: opened pack contents,
//! picks taken and the post-option state (money, jokers, consumables, card count, slots, RNG pools).

use jevbot::forecast_items::{apply_item_option, describe_lite};
use jevbot::forecast_stats::items_options;
use jevbot::picker::Picker;
use jevbot::trace::{fixtures_dir, read_jsonl};
use serde_json::Value;

fn num_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).map_or(false, |w| num_eq(v, w))),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(v, w)| num_eq(v, w)),
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

#[test]
fn fc_packs_identical() {
    let rows = read_jsonl(&fixtures_dir().join("fc_packs.jsonl.gz"));
    let mut n = [0usize; 3];
    let mut ok = [0usize; 3];
    let mut firsts: Vec<String> = vec![];
    let mut last_pub: Option<Value> = None;
    let mut picker_pub = Value::Null;
    let _ = &mut last_pub;
    let mut owned: Option<Picker> = None;
    let _ = &mut picker_pub;
    // Picker borrows the pub: keep one boxed pub per distinct pub (rows of a pub are contiguous)
    let mut pubs: Vec<Box<Value>> = vec![];
    for r in &rows {
        if pubs.last().map_or(true, |p| **p != r["pub"]) {
            pubs.push(Box::new(r["pub"].clone()));
            owned = None;
        }
        let p: &Value = unsafe { &*(&**pubs.last().unwrap() as *const Value) };
        if owned.is_none() {
            owned = Some(Picker::new(p));
        }
        let picker = owned.as_mut().unwrap();
        let key = r["key"].as_str().unwrap();
        let opts = items_options(p);
        let opt = opts.iter().find(|o| o["key"] == r["key"]).expect("option");
        let seed = r["seed"].as_str().unwrap();
        let got = apply_item_option(p, opt, seed, picker);
        let tag = format!("{key} k={}", r["k"]);
        let res: [Result<(), String>; 3] = match (got, r.get("error")) {
            (Err(e), None) => { let m = format!("{tag}: rust error {e}"); [Err(m.clone()), Err(m.clone()), Err(m)] }
            (Ok(_), Some(err)) => { let m = format!("{tag}: python refused ({err}), rust applied"); [Err(m.clone()), Err(m.clone()), Err(m)] }
            (Err(_), Some(_)) => [Ok(()), Ok(()), Ok(())],
            (Ok(a), None) => {
                let pack = Value::Array(a.pack.clone());
                let taken = Value::Array(a.taken.clone());
                let lite = describe_lite(&a.g);
                [
                    if num_eq(&pack, &r["pack"]) { Ok(()) } else { Err(format!("{tag}: pack rust {pack} python {}", r["pack"])) },
                    if num_eq(&taken, &r["taken"]) { Ok(()) } else { Err(format!("{tag}: taken rust {taken} python {} view {}", r["taken"], a.view0)) },
                    if num_eq(&lite, &r["lite"]) { Ok(()) } else {
                        let (x, y) = (lite.as_object().unwrap(), r["lite"].as_object().unwrap());
                        let k = x.keys().find(|k| !num_eq(&x[*k], &y[*k])).cloned().unwrap_or_default();
                        let (a, b) = (x[&k].to_string(), y[&k].to_string());
                        let cut = |s: String| if s.len() > 300 { s[..300].to_string() } else { s };
                        Err(format!("{tag}: lite.{k} rust {} python {}", cut(a), cut(b)))
                    },
                ]
            }
        };
        for (i, x) in res.into_iter().enumerate() {
            n[i] += 1;
            match x {
                Ok(()) => ok[i] += 1,
                Err(m) => if firsts.len() < 8 {
            }
        }
    }
    for (i, name) in ["pack contents", "picks taken", "post-option state"].iter().enumerate() {
        println!("fc_packs {name}: {}/{} identical", ok[i], n[i]);
    }
    for f in &firsts { println!("  {f}"); }
    assert_eq!(n[0] - ok[0] + n[1] - ok[1] + n[2] - ok[2], 0);
}
