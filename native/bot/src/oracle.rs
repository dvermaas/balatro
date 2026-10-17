//! `RustOracle`: the Oracle served by the Rust forecasts (forecast.rs, picker.rs) instead of a
//! recording. `CheckedOracle` is the identity-test wrapper: it serves the recorded call (checking
//! its args) and panics if the computed result differs from the recorded one.

use crate::forecast;
use crate::iface::Oracle;
use crate::picker::{pick_json, Picker};
use serde_json::Value;

pub struct RustOracle;

fn list(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}

impl Oracle for RustOracle {
    fn forecast(&mut self, pub_gs: &Value, n: i64, bosses: i64, keys: &[String], budget_s: Option<f64>) -> Value {
        crate::prof::time("oracle.forecast", || forecast::forecast(pub_gs, n as usize, bosses, Some(keys), budget_s))
    }
    fn items_forecast(&mut self, pub_gs: &Value, n: i64, bosses: i64, keys: &[String], budget_s: Option<f64>) -> Value {
        crate::prof::time("oracle.items_forecast", || crate::forecast_items::items_forecast(pub_gs, n as usize, bosses, Some(keys), budget_s))
    }
    fn picker_values(&mut self, pub_gs: &Value, a: &Value) -> Value {
        crate::prof::time("oracle.picker_values", || {
        let (pack, hand, jk) = (list(&a["pack"]), list(&a["hand"]), list(&a["jokers"]));
        let mut p = Picker::new(pub_gs);
        let v = p.values(&pack, &hand, &a["money"], &jk, a["joker_limit"].as_i64().unwrap_or(5));
        Value::Array(v.iter().map(|v| v.to_json()).collect())
        })
    }
    fn pick_pub(&mut self, pub_gs: &Value) -> Value {
        crate::prof::time("oracle.pick_pub", || pick_json(Picker::new(pub_gs).pick_pub(pub_gs)))
    }
}

/// Drop `seconds` and `_runs` (timing and raw rollouts) at the top level.
fn core(v: &Value) -> String {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("seconds");
        o.remove("_runs");
    }
    serde_json::to_string(&v).unwrap()
}

/// Serves `rec`'s recorded values, computing each call with `RustOracle` and checking equality.
pub struct CheckedOracle {
    pub rec: Box<dyn Oracle>,
    pub checked: usize,
}

impl CheckedOracle {
    fn cmp(&mut self, what: &str, got: Value, want: Value) -> Value {
        let (g, w) = (core(&got), core(&want));
        if g != w {
            panic!("computed {what} differs from recorded:\n  rust:   {}\n  python: {}", crate::trace::short(&serde_json::from_str(&g).unwrap(), 700), crate::trace::short(&serde_json::from_str(&w).unwrap(), 700));
        }
        self.checked += 1;
        want
    }
}

impl Oracle for CheckedOracle {
    fn forecast(&mut self, p: &Value, n: i64, b: i64, k: &[String], bs: Option<f64>) -> Value {
        let want = self.rec.forecast(p, n, b, k, bs);
        let got = RustOracle.forecast(p, n, b, k, bs);
        self.cmp("forecast", got, want)
    }
    fn items_forecast(&mut self, p: &Value, n: i64, b: i64, k: &[String], bs: Option<f64>) -> Value {
        let want = self.rec.items_forecast(p, n, b, k, bs);
        // a time budget makes the rollout count machine-dependent: replay the recorded count without one
        let m = want["rollouts"].as_i64().filter(|_| bs.is_some()).unwrap_or(n);
        let got = RustOracle.items_forecast(p, m, b, k, None);
        self.cmp("items_forecast", got, want)
    }
    fn picker_values(&mut self, p: &Value, a: &Value) -> Value {
        let want = self.rec.picker_values(p, a);
        let got = RustOracle.picker_values(p, a);
        self.cmp("Picker.values", got, want)
    }
    fn pick_pub(&mut self, p: &Value) -> Value {
        let want = self.rec.pick_pub(p);
        let got = RustOracle.pick_pub(p);
        self.cmp("Picker.pick_pub", got, want)
    }
}
