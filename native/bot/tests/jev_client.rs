//! `jev_client.rs`: the TypeSafe adapter (token mapping, errors) through a fake transport, and the
//! `.env` parsing / setdefault rules of `jev_bot.load_env`, on a temp file (never the real .env).

use jevbot::iface::Jev;
use jevbot::jev_client::{load_env, parse_env_file, TypeSafeJev};
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::rc::Rc;
use typesafe::{Request, RawResponse, Transport, TransportError};

struct Fake {
    status: u16,
    body: String,
    seen: Rc<RefCell<Vec<Request>>>,
}

impl Transport for Fake {
    fn send(&self, request: &Request) -> Result<RawResponse, TransportError> {
        self.seen.borrow_mut().push(request.clone());
        Ok(RawResponse { status: self.status, headers: vec![], body: self.body.clone().into_bytes() })
    }
}

fn questions() -> Map<String, Value> {
    let mut q = Map::new();
    q.insert("best_play".into(), json!({"type": "choice", "instructions": "pick", "criteria": {"a": "x"}}));
    q
}

#[test]
fn maps_model_and_tokens() {
    let seen = Rc::new(RefCell::new(vec![]));
    let body = json!({"model": "jev-latest", "answers": {}, "usage": {"input_tokens": 12, "output_tokens": 3}});
    let fake = Fake { status: 200, body: body.to_string(), seen: seen.clone() };
    let mut jev = TypeSafeJev::with_transport("test-key", "jev-latest", fake).unwrap();
    assert_eq!(jev.model(), "jev-latest");
    let r = jev.system_one(&json!({"state": "MENU"}), &questions()).unwrap();
    assert_eq!((r.model.as_str(), r.input_tokens, r.output_tokens), ("jev-latest", 12, 3));
    assert!(r.answers.is_empty());
    assert_eq!(seen.borrow().len(), 1);
}

#[test]
fn missing_or_null_usage_counts_are_zero() {
    let seen = Rc::new(RefCell::new(vec![]));
    let body = json!({"model": "jev-latest", "answers": {}, "usage": {"input_tokens": null, "output_tokens": null}});
    let mut jev = TypeSafeJev::with_transport("k", "jev-latest", Fake { status: 200, body: body.to_string(), seen }).unwrap();
    let r = jev.system_one(&json!({"state": "MENU"}), &questions()).unwrap();
    assert_eq!((r.input_tokens, r.output_tokens), (0, 0));
}

#[test]
fn api_error_is_a_string() {
    let seen = Rc::new(RefCell::new(vec![]));
    let fake = Fake { status: 402, body: r#"{"detail":"no credits"}"#.into(), seen };
    let mut jev = TypeSafeJev::with_transport("k", "jev-latest", fake).unwrap();
    let e = jev.system_one(&json!({"state": "MENU"}), &questions()).unwrap_err();
    assert!(!e.is_empty());
    eprintln!("402 error text: {e}");
}

#[test]
fn env_file_rules() {
    let text = "# comment\nJEV_API_KEY = \"abc=def\" \n\nTYPESAFE_API_KEY='x'\nNOEQUALS\n#X=1\n";
    assert_eq!(
        parse_env_file(text),
        vec![
            // Python partitions the stripped line, so spaces before "=" stay in the key.
            ("JEV_API_KEY ".to_string(), "abc=def".to_string()),
            ("TYPESAFE_API_KEY".to_string(), "x".to_string()),
            ("NOEQUALS".to_string(), String::new()),
        ]
    );
}

#[test]
fn load_env_is_setdefault_and_skips_missing_file() {
    let key = format!("JEVBOT_TEST_ENV_{}", std::process::id());
    let dir = std::env::temp_dir().join(format!("jevbot_env_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("env.txt");
    std::fs::write(&path, format!("{key}=from_file\n")).unwrap();
    load_env(&path).unwrap();
    assert_eq!(std::env::var(&key).unwrap(), "from_file");
    std::fs::write(&path, format!("{key}=other\n")).unwrap();
    load_env(&path).unwrap();
    assert_eq!(std::env::var(&key).unwrap(), "from_file", "existing variables win");
    load_env(&dir.join("missing.env")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
