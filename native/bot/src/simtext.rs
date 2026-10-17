//! Port of src/sim_text.py lines 1-348: Lua table parser, localization loading, loc_parse_string,
//! number formatting and localize(). The game's text is loaded at runtime from $BALATRO_LOC_DIR
//! (default runs/_loc of the main port worktree); it is never committed.

use pycompat::fmt::fmt_g;
use pycompat::{fixed, pyfloat, pyround};
use regex::Regex;
use serde_json::{Map, Number, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const DEFAULT_LOC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../runs/_loc");

fn loc_dir() -> PathBuf {
    match std::env::var("BALATRO_LOC_DIR") {
        Ok(s) if !s.is_empty() => PathBuf::from(s),
        _ => PathBuf::from(DEFAULT_LOC_DIR),
    }
}

/// Path.read_text(encoding="utf8"): universal newlines.
fn read_text(p: &std::path::Path) -> String {
    let s = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn rx(p: &str) -> Regex {
    Regex::new(p).unwrap()
}

// sim_text.py 28-110: Lua table-literal parser

fn token_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        rx(r#"(?s)^(?:(?P<ws>\s+|--\[\[.*?\]\]|--[^\n]*)|(?P<str>"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\[\[.*?\]\])|(?P<num>-?0x[0-9a-fA-F]+|-?\d+\.?\d*(?:[eE][-+]?\d+)?)|(?P<name>[A-Za-z_][A-Za-z0-9_]*)|(?P<op>[{}\[\]=,;]))"#)
    })
}

// sim_text.py 40-45
pub fn _unescape(s: &str) -> String {
    if s.starts_with("[[") {
        return s[2..s.len() - 2].to_string();
    }
    let body = &s[1..s.len() - 1];
    static R: OnceLock<Regex> = OnceLock::new();
    let r = R.get_or_init(|| rx(r"(?s)\\(.)"));
    r.replace_all(body, |c: &regex::Captures| {
        let g = &c[1];
        match g {
            "n" => "\n".to_string(),
            "t" => "\t".to_string(),
            "\\" => "\\".to_string(),
            "\"" => "\"".to_string(),
            "'" => "'".to_string(),
            "r" => "\r".to_string(),
            "\n" => "\n".to_string(),
            o => o.to_string(),
        }
    })
    .into_owned()
}

fn key_string(k: &Value) -> String {
    match k {
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else {
                pyfloat(n.as_f64().unwrap())
            }
        }
        Value::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        Value::Null => "null".to_string(),
        _ => panic!("unhashable Lua key"),
    }
}

fn lua_num(v: &str) -> Value {
    if v.to_lowercase().contains('x') {
        let (neg, h) = match v.strip_prefix('-') {
            Some(h) => (true, h),
            None => (false, v),
        };
        let n = i64::from_str_radix(&h[2..], 16).expect("hex");
        Value::from(if neg { -n } else { n })
    } else if v.contains(|c| c == '.' || c == 'e' || c == 'E') {
        Value::from(v.parse::<f64>().unwrap())
    } else {
        Value::from(v.parse::<i64>().expect("int"))
    }
}

struct Toks(Vec<(&'static str, String)>);

impl Toks {
    fn text(&self, i: usize) -> &str {
        &self.0[i].1
    }
    fn value(&self, i: usize) -> (Value, usize) {
        let (kind, v) = (self.0[i].0, &self.0[i].1);
        match kind {
            "str" => (Value::String(_unescape(v)), i + 1),
            "num" => (lua_num(v), i + 1),
            "name" => (
                match v.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    "nil" => Value::Null,
                    o => Value::String(o.to_string()),
                },
                i + 1,
            ),
            _ if v == "{" => self.table(i + 1),
            _ => panic!("unexpected token {v:?}"),
        }
    }
    fn table(&self, mut i: usize) -> (Value, usize) {
        let mut arr: Vec<Value> = vec![];
        let mut d = Map::new();
        while self.text(i) != "}" {
            if self.text(i) == "[" {
                let (k, ni) = self.value(i + 1);
                i = ni + 2; // ] =
                let (v, ni) = self.value(i);
                i = ni;
                d.insert(key_string(&k), v);
            } else if self.0[i].0 == "name" && self.text(i + 1) == "=" {
                let k = self.text(i).to_string();
                let (v, ni) = self.value(i + 2);
                i = ni;
                d.insert(k, v);
            } else {
                let (v, ni) = self.value(i);
                i = ni;
                arr.push(v);
            }
            if self.text(i) == "," || self.text(i) == ";" {
                i += 1;
            }
        }
        if !d.is_empty() && !arr.is_empty() {
            for (n, v) in arr.into_iter().enumerate() {
                d.insert((n + 1).to_string(), v);
            }
            return (Value::Object(d), i + 1);
        }
        if !d.is_empty() || arr.is_empty() {
            (Value::Object(d), i + 1)
        } else {
            (Value::Array(arr), i + 1)
        }
    }
}

/// Parse `return { ... }` (constructors, strings, numbers, booleans) into dicts/lists.
pub fn parse_lua_table(text: &str) -> Value {
    let mut toks = vec![];
    let mut pos = 0;
    let re = token_re();
    while pos < text.len() {
        let rest = &text[pos..];
        let m = re
            .captures(rest)
            .unwrap_or_else(|| panic!("Lua parse error at {pos}: {:?}", rest.chars().take(40).collect::<String>()));
        pos += m.get(0).unwrap().end();
        for kind in ["ws", "str", "num", "name", "op"] {
            if let Some(g) = m.name(kind) {
                if kind != "ws" {
                    toks.push((kind, g.as_str().to_string()));
                }
                break;
            }
        }
    }
    let toks = Toks(toks);
    let mut i = 0;
    if !toks.0.is_empty() && toks.0[0].0 == "name" && toks.0[0].1 == "return" {
        i = 1;
    }
    toks.value(i).0
}

// sim_text.py 113-170: loading

static LOC: OnceLock<Value> = OnceLock::new();
static HANDS: OnceLock<Value> = OnceLock::new();

fn merge(dst: &mut Map<String, Value>, src: Map<String, Value>) {
    for (k, v) in src {
        match (v, dst.get_mut(&k)) {
            (Value::Object(sv), Some(Value::Object(dv))) => merge(dv, sv),
            (v, _) => {
                dst.insert(k, v);
            }
        }
    }
}

pub fn loc() -> &'static Value {
    LOC.get_or_init(|| {
        let dir = loc_dir();
        let f = dir.join("en-us.lua");
        if !f.exists() {
            panic!("Balatro localization not found at {}", f.display());
        }
        let mut data = parse_lua_table(&read_text(&f));
        let s = dir.join("smods-en-us.lua");
        if s.exists() {
            // Steamodded overlays its own strings (SMODS.handle_loc_file)
            if let (Value::Object(d), Value::Object(sv)) = (&mut data, parse_lua_table(&read_text(&s))) {
                merge(d, sv);
            }
        }
        // Steamodded's process_loc_text for The Wheel (game_objects/blind.lua): "#1# in #2# ..."
        if let Some(wheel) = data
            .get_mut("descriptions")
            .and_then(|x| x.get_mut("Blind"))
            .and_then(|x| x.get_mut("bl_wheel"))
            .and_then(|x| x.as_object_mut())
        {
            if let Some(Value::Array(text)) = wheel.get_mut("text") {
                if !text.is_empty() {
                    let t = text[0].as_str().unwrap().to_string();
                    let t = if t.starts_with("#1#") { t } else { format!("#1#{t}") };
                    text[0] = Value::String(t.replace(chr7(), "#2#"));
                }
            }
        }
        data
    })
}

fn chr7() -> char {
    char::from_digit(7, 10).unwrap()
}

/// Default G.GAME.hands table (for `example`), from game-hands.lua.
pub fn game_hands() -> &'static Value {
    HANDS.get_or_init(|| {
        let f = loc_dir().join("game-hands.lua");
        if f.exists() { parse_lua_table(&read_text(&f)) } else { Value::Object(Map::new()) }
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Str(String),
    Ref(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub strings: Vec<Seg>,
    pub control: Vec<(String, String)>,
}

impl Part {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert(
            "strings".into(),
            Value::Array(
                self.strings
                    .iter()
                    .map(|s| match s {
                        Seg::Str(s) => Value::String(s.clone()),
                        Seg::Ref(r) => Value::Array(vec![Value::String(r.clone())]),
                    })
                    .collect(),
            ),
        );
        let mut c = Map::new();
        for (k, v) in &self.control {
            c.insert(k.clone(), Value::String(v.clone()));
        }
        m.insert("control".into(), Value::Object(c));
        Value::Object(m)
    }
}

fn py_isspace(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

fn ctl_get<'a>(c: &'a [(String, String)], k: &str) -> Option<&'a String> {
    c.iter().find(|(a, _)| a == k).map(|(_, v)| v)
}

/// Port of misc_functions.lua loc_parse_string. An empty line parses to None.
pub fn parse_line(line: &str) -> Option<Vec<Part>> {
    if line.is_empty() {
        return None;
    }
    let chars: Vec<char> = line.chars().collect();
    let mut parts: Vec<Part> = vec![];
    let mut control: Vec<(String, String)> = vec![];
    let mut strs: Vec<Seg> = vec![];
    let mut buf: Option<String> = None;
    let (mut i, n) = (0usize, chars.len());
    while i < n {
        if chars[i] == '{' {
            if let Some(b) = buf.take() {
                strs.push(Seg::Str(b));
            }
            if !strs.is_empty() || ctl_get(&control, "element").is_some() {
                parts.push(Part { strings: std::mem::take(&mut strs), control: std::mem::take(&mut control) });
            }
            strs = vec![];
            control = vec![];
            let j = (i..n).find(|&j| chars[j] == '}').expect("substring not found");
            let inner: String = chars[i + 1..j].iter().collect();
            for item in inner.split(',') {
                if let Some((k, v)) = item.split_once(':') {
                    if !k.is_empty() {
                        if let Some(e) = control.iter_mut().find(|(a, _)| a == k) {
                            e.1 = v.to_string();
                        } else {
                            control.push((k.to_string(), v.to_string()));
                        }
                    }
                }
            }
            i = j + 1;
            continue;
        }
        if chars[i] == '#' {
            let Some(j) = (i + 1..n).find(|&j| chars[j] == '#') else { break };
            if let Some(b) = buf.take() {
                strs.push(Seg::Str(b));
            }
            let r: String = chars[i + 1..j].iter().collect();
            if !r.is_empty() {
                strs.push(Seg::Ref(r));
            }
            i = j + 1;
            continue;
        }
        let drop = ctl_get(&control, "X").is_some() && py_isspace(chars[i]);
        let mut b = buf.take().unwrap_or_default();
        if !drop {
            b.push(chars[i]);
        }
        buf = Some(b);
        i += 1;
    }
    if let Some(b) = buf {
        strs.push(Seg::Str(b));
    }
    if !strs.is_empty() || ctl_get(&control, "element").is_some() {
        parts.push(Part { strings: strs, control });
    }
    Some(parts)
}

fn falsy(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}

/// text_parsed for a descriptions entry (multi-box text = list of lists: only box 1 is 'main').
pub fn _text_parsed(entry: &Value) -> Vec<Vec<Part>> {
    let text = entry.get("text").cloned().unwrap_or(Value::Null);
    let mut items: Vec<Value> = if falsy(&text) {
        vec![]
    } else if let Value::String(_) = text {
        vec![text]
    } else if let Value::Array(a) = text {
        a
    } else {
        panic!("unsupported text");
    };
    if let Some(Value::Array(first)) = items.first() {
        items = first.clone();
    }
    items
        .iter()
        .filter_map(|t| match t {
            Value::String(s) => parse_line(s),
            o if falsy(o) => None,
            _ => panic!("non-string text line"),
        })
        .collect()
}

// sim_text.py 176-270: number formatting

fn lua_tostring_num(n: &Number) -> String {
    if let Some(i) = n.as_i64() {
        // float(v).is_integer() and abs(v) < 1e15
        if (i as f64).abs() < 1e15 { i.to_string() } else { fmt_g(i as f64, 14) }
    } else {
        lua_tostring_f64(n.as_f64().unwrap())
    }
}

pub fn lua_tostring_f64(v: f64) -> String {
    if v.is_infinite() {
        return (if v > 0.0 { "inf" } else { "-inf" }).into();
    }
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        return (v as i64).to_string();
    }
    fmt_g(v, 14)
}

pub fn lua_tostring(v: &Value) -> String {
    match v {
        Value::Bool(b) => (if *b { "true" } else { "false" }).into(),
        Value::Null => "nil".into(),
        Value::Number(n) => lua_tostring_num(n),
        Value::String(s) => s.clone(),
        _ => panic!("lua_tostring of container"),
    }
}

/// number_format on a finite number; `e_switch_point` None or 0 means the default 1e11.
pub fn number_format_f(num: f64, e_switch_point: Option<f64>) -> String {
    let sign = if num >= 0.0 { "" } else { "-" };
    let num = num.abs();
    let thr = match e_switch_point {
        Some(e) if e != 0.0 => e,
        _ => 100000000000.0,
    };
    if num >= thr {
        let x: f64 = fmt_g(num, 4).parse().unwrap();
        let fac = x.log10().floor();
        assert!(fac.is_finite(), "math.floor of non-finite");
        let mut fac_i = fac as i32;
        let p = if fac_i >= 0 { format!("1e{fac_i}").parse::<f64>().unwrap() } else { 10f64.powf(fac_i as f64) };
        let mut mantissa = pyround(x / p, 3);
        if mantissa >= 10.0 {
            mantissa /= 10.0;
            fac_i += 1;
        }
        let prec = if fac_i >= 100 { 1 } else if fac_i >= 10 { 2 } else { 3 };
        return format!("{}{}e{}", sign, fixed(mantissa, prec), fac_i);
    }
    let formatted;
    if num != num.floor() && num < 100.0 {
        let mut f = fixed(num, if num >= 10.0 { 1 } else { 2 });
        if f.ends_with('0') {
            static R: OnceLock<Regex> = OnceLock::new();
            f = R.get_or_init(|| rx(r"\.?0+$")).replace(&f, "").into_owned();
        }
        if num < 0.01 {
            return lua_tostring_f64(num);
        }
        formatted = f;
    } else {
        formatted = fixed(num, 0);
    }
    let rev: String = formatted.chars().rev().collect();
    static R3: OnceLock<Regex> = OnceLock::new();
    let rev = R3.get_or_init(|| rx(r"(\d\d\d)")).replace_all(&rev, "${1},").into_owned();
    let rev = rev.strip_suffix(',').unwrap_or(&rev).to_string();
    format!("{}{}", sign, rev.chars().rev().collect::<String>())
}

/// number_format(num, e_switch_point): non-numbers are returned unchanged.
pub fn number_format(num: &Value, e_switch_point: Option<f64>) -> Value {
    match num {
        Value::Number(n) => Value::String(number_format_f(n.as_f64().unwrap(), e_switch_point)),
        o => o.clone(),
    }
}

pub fn format_ui_value(v: &Value) -> String {
    if !matches!(v, Value::Number(_)) {
        return lua_tostring(v);
    }
    match number_format(v, Some(1000000.0)) {
        Value::String(s) => s,
        _ => unreachable!(),
    }
}

/// SMODS.signed
pub fn signed(v: &Value) -> String {
    if v.is_null() {
        return "+0".into();
    }
    if v.as_f64().expect("number") >= 0.0 { format!("+{}", lua_tostring(v)) } else { lua_tostring(v) }
}

/// SMODS.signed_dollars
pub fn signed_dollars(v: &Value) -> String {
    if v.is_null() {
        return "$0".into();
    }
    let a = if let Some(i) = v.as_i64() { Value::from(i.abs()) } else { Value::from(v.as_f64().unwrap().abs()) };
    format!("{}${}", if v.as_f64().unwrap() < 0.0 { "-" } else { "" }, lua_tostring(&a))
}

// sim_text.py 273-348: localize()

fn _var(vars_: &[Value], sub: &str) -> String {
    let v = match sub.trim().parse::<i64>() {
        Ok(n) => {
            let idx = n - 1;
            if idx < vars_.len() as i64 {
                let k = if idx < 0 { idx + vars_.len() as i64 } else { idx };
                assert!(k >= 0, "list index out of range");
                vars_[k as usize].clone()
            } else {
                Value::Null
            }
        }
        Err(_) => Value::Null,
    };
    format_ui_value(&v)
}

fn join_part(part: &Part, vars_: &[Value]) -> String {
    part.strings
        .iter()
        .map(|x| match x {
            Seg::Str(s) => s.clone(),
            Seg::Ref(r) => _var(vars_, r),
        })
        .collect()
}

/// Text BalatroBot extracts from localize'd UI lines: every part's assembled string, markup removed.
pub fn box_lines(lines: &[Vec<Part>], vars_: &[Value]) -> Vec<String> {
    lines.iter().map(|line| line.iter().map(|p| join_part(p, vars_)).collect::<String>()).collect()
}

/// localize{type='raw_descriptions'} (used for tags): every part, markup removed.
pub fn raw_lines(lines: &[Vec<Part>], vars_: &[Value]) -> Vec<String> {
    box_lines(lines, vars_)
}

type Parsed = Option<&'static Vec<Vec<Part>>>;
static PARSED: OnceLock<Mutex<HashMap<(String, String), Parsed>>> = OnceLock::new();

pub fn desc(set_: &str, key: &str) -> Parsed {
    let mut m = PARSED.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    let k = (set_.to_string(), key.to_string());
    if let Some(p) = m.get(&k) {
        return *p;
    }
    let entry = loc()["descriptions"].get(set_).and_then(|s| s.get(key));
    let r: Parsed = match entry {
        Some(e) if e.is_object() => Some(Box::leak(Box::new(_text_parsed(e)))),
        _ => None,
    };
    m.insert(k, r);
    r
}

pub fn localize_desc(set_: &str, key: &str, vars_: &[Value]) -> Vec<String> {
    match desc(set_, key) {
        Some(lines) if !lines.is_empty() => box_lines(lines, vars_),
        _ => vec![],
    }
}

pub fn localize_other(key: &str, vars_: &[Value]) -> Vec<String> {
    localize_desc("Other", key, vars_)
}

pub fn misc(cat: &str, key: &str) -> Value {
    loc()["misc"]
        .get(cat)
        .filter(|c| !falsy(c))
        .and_then(|c| c.as_object())
        .and_then(|c| c.get(key))
        .cloned()
        .unwrap_or_else(|| Value::String("ERROR".into()))
}

pub fn dictionary(key: &str) -> Value {
    loc()["misc"]["dictionary"].get(key).cloned().unwrap_or_else(|| Value::String("ERROR".into()))
}

pub fn variable(key: &str, vars_: &[Value]) -> String {
    let Some(t) = loc()["misc"]["v_dictionary"].get(key) else { return "ERROR".into() };
    // a list value goes through parse_line element-wise: the strings concatenate into one text node
    let parts = match t {
        Value::Array(a) if !a.is_empty() => {
            let j: String = a.iter().map(|x| x.as_str().expect("v_dictionary string")).collect();
            vec![Part { strings: vec![Seg::Str(j)], control: vec![] }]
        }
        Value::Array(_) => vec![],
        _ => parse_line(t.as_str().expect("v_dictionary string")).unwrap_or_default(),
    };
    let s: String = parts.iter().map(|p| join_part(p, vars_)).collect();
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| rx(r"\{.-\}")).replace_all(&s, "").into_owned()
}

pub fn name_text(set_: &str, key: &str) -> Value {
    let e = loc()["descriptions"].get(set_).and_then(|s| s.get(key));
    match e {
        Some(e) if !falsy(e) => match e.get("name") {
            Some(Value::Array(a)) => {
                Value::String(a.iter().map(|x| x.as_str().expect("name str")).collect::<Vec<_>>().join(" "))
            }
            Some(n) => n.clone(),
            None => Value::Null,
        },
        _ => Value::String("ERROR".into()),
    }
}

pub fn poker_hand(h: &str) -> Value {
    misc("poker_hands", h)
}
