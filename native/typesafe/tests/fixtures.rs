//! Replays runs/_port/fixtures/typesafe.jsonl.gz (made by scripts/port/fixtures/typesafe.py from the
//! typesafe-sdk 0.7.4 client with a scripted transport). Each system_one row: the request every
//! attempt sends (method, URL, SDK headers, body string) and the returned answers or the error
//! (class, status, str). Each backoff row: the SDK's wait with jitter 0.

use flate2::read::GzDecoder;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use typesafe::{backoff, Request, RawResponse, Transport, TransportError, TypeSafeClient};

const FIXTURE: &str = "C:/Users/admin/Documents/Repos/balatro-rust/runs/_port/fixtures/typesafe.jsonl.gz";
const VOLATILE_HEADER: &str = "x-typesafe-runtime";

/// Serves the recorded server steps in order and keeps every attempt it was sent.
struct Scripted {
    steps: RefCell<Vec<Value>>,
    seen: RefCell<Vec<Request>>,
}

impl Transport for &Scripted {
    fn send(&self, request: &Request) -> Result<RawResponse, TransportError> {
        self.seen.borrow_mut().push(request.clone());
        let step = if self.steps.borrow().is_empty() {
            json!({"status": 200, "headers": {}, "body": "{}"})
        } else {
            self.steps.borrow_mut().remove(0)
        };
        match step.get("exc").and_then(Value::as_str) {
            Some("timeout") => return Err(TransportError::Timeout),
            Some("connect") => return Err(TransportError::Connection("connection refused".into())),
            _ => {}
        }
        let headers = step["headers"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect();
        Ok(RawResponse {
            status: step["status"].as_u64().unwrap() as u16,
            headers,
            body: step["body"].as_str().unwrap().as_bytes().to_vec(),
        })
    }
}

fn no_sleep(_: f64) {}
fn no_jitter() -> f64 {
    0.0
}

/// A request as the fixture stores it: volatile runtime header dropped, headers sorted by name.
fn request_json(method: &str, url: &str, headers: &[(String, String)], body: &str) -> String {
    let headers: BTreeMap<String, String> = headers
        .iter()
        .filter(|(n, _)| n.to_ascii_lowercase() != VOLATILE_HEADER)
        .cloned()
        .collect();
    json!({"method": method, "url": url, "headers": headers, "body": body}).to_string()
}

fn check_system_one(row: &Value) -> Result<(), String> {
    let scripted = Scripted {
        steps: RefCell::new(row["server"].as_array().unwrap().clone()),
        seen: RefCell::new(Vec::new()),
    };
    let client = TypeSafeClient::with_transport(Some("test-key"), Some("jev-latest"), &scripted)
        .map_err(|e| e.to_string())?
        .with_sleep(no_sleep)
        .with_jitter(no_jitter);
    let state = &row["args"]["state"];
    let questions = row["args"]["questions"].as_object().unwrap();
    let result = client.system_one(state, questions);

    let got_requests: Vec<String> = scripted
        .seen
        .borrow()
        .iter()
        .map(|r| request_json(&r.method, &r.url, &r.headers, std::str::from_utf8(&r.body).unwrap()))
        .collect();
    let want_requests: Vec<String> = row["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let headers: Vec<(String, String)> = r["headers"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                .collect();
            request_json(r["method"].as_str().unwrap(), r["url"].as_str().unwrap(), &headers, r["body"].as_str().unwrap())
        })
        .collect();
    if got_requests != want_requests {
        return Err(format!(
            "requests differ ({} vs {} attempts)\n  got:  {}\n  want: {}",
            got_requests.len(),
            want_requests.len(),
            got_requests.first().map_or("-", String::as_str),
            want_requests.first().map_or("-", String::as_str)
        ));
    }

    match (result, row.get("ret"), row.get("err")) {
        (Ok(resp), Some(ret), _) => {
            let got = json!({
                "model": resp.model,
                "input_tokens": resp.input_tokens,
                "output_tokens": resp.output_tokens,
                "answers": Value::Object(resp.answers),
            });
            if got.to_string() != ret.to_string() {
                return Err(format!("answers differ\n  got:  {got}\n  want: {ret}"));
            }
            Ok(())
        }
        (Err(e), _, Some(err)) => {
            let got = json!({"class": e.class(), "status": e.status(), "str": e.to_string()});
            if got.to_string() != err.to_string() {
                return Err(format!("error differs\n  got:  {got}\n  want: {err}"));
            }
            Ok(())
        }
        (Ok(resp), _, _) => Err(format!("expected an error, got answers {:?}", resp.answers.keys())),
        (Err(e), _, _) => Err(format!("unexpected error {e}")),
    }
}

fn check_backoff(row: &Value) -> Result<(), String> {
    let a = &row["args"];
    let got = backoff(
        a["attempt"].as_u64().unwrap() as u32,
        a["initial"].as_f64().unwrap(),
        a["maximum"].as_f64().unwrap(),
        a["jitter"].as_f64().unwrap(),
        0.0,
    );
    let want = row["ret"].as_f64().unwrap();
    if got.to_bits() != want.to_bits() {
        return Err(format!("backoff {a} gave {got:?}, want {want:?}"));
    }
    Ok(())
}

#[test]
fn replays_fixture() {
    let file = File::open(FIXTURE).expect("fixture (run scripts/port/fixtures/typesafe.py)");
    let reader = BufReader::new(GzDecoder::new(file));
    let (mut total, mut same) = (0, 0);
    let mut mismatches = Vec::new();
    for line in reader.lines() {
        let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
        total += 1;
        let result = match row["fn"].as_str() {
            Some("system_one") => check_system_one(&row),
            Some("backoff") => check_backoff(&row),
            other => Err(format!("unknown fn {other:?}")),
        };
        match result {
            Ok(()) => same += 1,
            Err(msg) => {
                if mismatches.len() < 5 {
                    mismatches.push(format!("row {total} ({}): {msg}", row["name"].as_str().unwrap_or("-")));
                }
            }
        }
    }
    println!("{same}/{total} identical");
    assert!(mismatches.is_empty(), "\n{}", mismatches.join("\n"));
}
