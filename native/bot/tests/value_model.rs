//! value_model.rs against sklearn's predictions (VALUE_REF=<vmodel_ref.json from vexport.py>; skipped without it).
#[test]
fn p_win_matches_sklearn() {
    let Some(p) = std::env::var_os("VALUE_REF") else { return };
    let rows: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let mut worst: f64 = 0.0;
    for r in rows.as_array().unwrap() {
        let x: Vec<f64> = r["x"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        worst = worst.max((jevbot::value_model::p_win_x(&x, 5) - r["p"].as_f64().unwrap()).abs());
    }
    println!("{} rows, worst |rust - sklearn| = {worst:e}", rows.as_array().unwrap().len());
    assert!(worst < 1e-9);
}
