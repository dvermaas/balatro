//! PyO3 module `jev_native`: the scorer kernel (../kernel) for src/native.py.
//! Part of the prompt version hash (see recorder.PROMPT_VERSION).

use jev_kernel::{cards_of, classify, each_play, CardIn, RuleIn, HAND_NAMES};
use pyo3::prelude::*;

#[pyclass(module = "jev_native")]
struct Scorer(jev_kernel::Scorer);

#[pymethods]
impl Scorer {
    #[new]
    fn new(levels: Vec<(f64, f64, f64, bool)>, jokers: Vec<(u8, Option<RuleIn>)>, hands_left: f64, discards_left: f64) -> PyResult<Self> {
        jev_kernel::Scorer::new(levels, jokers, hands_left, discards_left)
            .map(Scorer)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }
}

#[pyfunction]
fn evaluate(s: PyRef<'_, Scorer>, cards: Vec<CardIn>, allow: (u8, u32, bool)) -> Vec<(f64, usize, Vec<usize>, Vec<usize>)> {
    let cards = cards_of(&cards);
    let mut out = Vec::new();
    each_play(&s.0, &cards, allow, |score, name, combo, sc| {
        out.push((score, name, combo.to_vec(), sc.iter().map(|&p| combo[p]).collect()));
    });
    out
}

#[pyfunction]
fn best_score(s: PyRef<'_, Scorer>, cards: Vec<CardIn>, allow: (u8, u32, bool)) -> f64 {
    let cards = cards_of(&cards);
    let mut best: Option<f64> = None;
    each_play(&s.0, &cards, allow, |score, _, _, _| {
        if best.map_or(true, |b| score > b) {
            best = Some(score);
        }
    });
    best.unwrap_or(0.0)
}

/// max((score, name)) like Python tuples: a tie on score goes to the larger name string.
#[pyfunction]
fn best_play(s: PyRef<'_, Scorer>, cards: Vec<CardIn>, allow: (u8, u32, bool)) -> (f64, i64) {
    let cards = cards_of(&cards);
    let mut best: Option<(f64, usize)> = None;
    each_play(&s.0, &cards, allow, |score, name, _, _| {
        let better = match best {
            None => true,
            Some((bs, bn)) => score > bs || (score == bs && HAND_NAMES[name] > HAND_NAMES[bn]),
        };
        if better {
            best = Some((score, name));
        }
    });
    best.map_or((0.0, -1), |(s, n)| (s, n as i64))
}

/// One score (rollout.Policy, tarots): jokers.score(infos, combo, name, scoring).
#[pyfunction]
fn score(s: PyRef<'_, Scorer>, cards: Vec<CardIn>, combo: Vec<usize>, name: usize, scoring: Vec<usize>) -> f64 {
    s.0.score(&cards_of(&cards), &combo, name, &scoring)
}

/// jokers.score for several plays of one hand: [(combo, hand index, scoring positions into combo)].
#[pyfunction]
fn score_many(s: PyRef<'_, Scorer>, cards: Vec<CardIn>, plays: Vec<(Vec<usize>, usize, Vec<usize>)>) -> Vec<f64> {
    let cards = cards_of(&cards);
    plays.iter().map(|(combo, name, sc)| s.0.score(&cards, combo, *name, sc)).collect()
}

/// hands._classify on encoded cards: (hand index, scoring positions).
#[pyfunction(name = "classify")]
fn classify_py(cards: Vec<CardIn>) -> (usize, Vec<usize>) {
    classify(&cards_of(&cards))
}

#[pymodule]
fn jev_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Scorer>()?;
    m.add_function(wrap_pyfunction!(evaluate, m)?)?;
    m.add_function(wrap_pyfunction!(best_score, m)?)?;
    m.add_function(wrap_pyfunction!(best_play, m)?)?;
    m.add_function(wrap_pyfunction!(score, m)?)?;
    m.add_function(wrap_pyfunction!(score_many, m)?)?;
    m.add_function(wrap_pyfunction!(classify_py, m)?)?;
    Ok(())
}
