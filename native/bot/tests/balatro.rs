//! `balatro.rs` against `scripts/port/fixtures/balatro.py` (runs/_port/fixtures/balatro.jsonl.gz):
//! each recorded Python call is replayed against a local mock server that returns the same canned
//! bodies. Compared: the request body the server received (canonical compact JSON, key order kept)
//! and the result (JSON text) or the RPCError (message, name, code).

use jevbot::balatro::Balatro;
use jevbot::iface::Game;
use jevbot::trace::{fixtures_dir, read_jsonl};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Serves one canned HTTP/1.1 response per connection, in order, and records each request body.
fn mock(canned: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        for out in canned {
            let (mut sock, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut buf = vec![0u8; len];
            reader.read_exact(&mut buf).unwrap();
            seen2.lock().unwrap().push(String::from_utf8(buf).unwrap());
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                out.len(),
                out
            );
            sock.write_all(resp.as_bytes()).unwrap();
        }
    });
    (url, seen)
}

fn canon(text: &str) -> String {
    serde_json::to_string(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
}

#[test]
fn matches_python_client_on_fixture() {
    let rows = read_jsonl(&fixtures_dir().join("balatro.jsonl.gz"));
    assert!(!rows.is_empty(), "fixture rows missing: runs/_port/fixtures/balatro.jsonl.gz (recorded from the Python v27 bot; see trace::data_dir)");
    let (mut n, mut same, mut bad) = (0usize, 0usize, Vec::new());
    for row in &rows {
        let case = row["case"].as_str().unwrap();
        let canned: Vec<String> = row["canned"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect();
        let (url, seen) = mock(canned);
        let mut client = Balatro::with_url(&url, Duration::from_secs(60));
        for (i, c) in row["calls"].as_array().unwrap().iter().enumerate() {
            n += 1;
            let params = if c["params"].is_null() { None } else { Some(&c["params"]) };
            let got = client.call(c["method"].as_str().unwrap(), params);
            let py_body = row["requests"][i]["body"].as_str().unwrap();
            let rs_body = seen.lock().unwrap().get(i).cloned().unwrap_or_default();
            let mut ok = canon(py_body) == canon(&rs_body);
            if !ok {
                bad.push(format!("{case}#{i} body: py {} rs {}", py_body, rs_body));
            }
            if let Some(want) = c.get("result") {
                match &got {
                    Ok(v) => ok &= v.to_string() == want.to_string(),
                    Err(_) => ok = false,
                }
            } else {
                let w = &c["rpc_error"];
                match &got {
                    Err(e) => {
                        ok &= e.message == w["str"].as_str().unwrap();
                        ok &= e.name.as_deref() == w["name"].as_str();
                        if !w["code"].is_null() {
                            ok &= Some(e.code) == w["code"].as_i64();
                        }
                    }
                    Ok(_) => ok = false,
                }
            }
            if ok {
                same += 1;
            } else {
                bad.push(format!("{case}#{i}: py {} rs {:?}", c, got));
            }
        }
        assert_eq!(seen.lock().unwrap().len(), row["requests"].as_array().unwrap().len(), "{case}: request count");
    }
    for b in bad.iter().take(5) {
        eprintln!("{b}");
    }
    eprintln!("balatro fixture: {same}/{n} calls identical (bodies, results, RPCError messages)");
    assert_eq!(same, n);
}

#[test]
fn defaults_match_python_client() {
    let c = Balatro::new();
    assert_eq!(c.url, "http://127.0.0.1:12346");
    assert_eq!(c.timeout, Duration::from_secs(60));
    assert_eq!(c.backend(), "balatrobot");
}

#[test]
fn ids_count_from_one_per_client() {
    let mut c = Balatro::with_url("http://127.0.0.1:9", Duration::from_secs(1));
    assert_eq!(c.request_body("health", None), r#"{"jsonrpc":"2.0","method":"health","id":1}"#);
    assert_eq!(c.request_body("start", Some(&serde_json::json!({"deck":"RED"}))), r#"{"jsonrpc":"2.0","method":"start","id":2,"params":{"deck":"RED"}}"#);
}

#[test]
fn transport_failure_is_not_an_rpc_error() {
    // Python: urlopen raises URLError here, which step() does not catch: the port panics too.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut c = Balatro::with_url(&format!("http://127.0.0.1:{port}"), Duration::from_secs(2));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| c.call("gamestate", None)));
    assert!(r.is_err());
}
