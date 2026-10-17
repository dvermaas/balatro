//! Rust client for the TypeSafe API: `system_one`, as typesafe-sdk 0.7.4 sends it for the bot
//! (`TypeSafeClient(api_key=..., model=...)` and `client.system_one(state=..., questions=...)`).
//!
//! Equivalent to the Python SDK in what goes on the wire (method, URL, headers, JSON body), the
//! retry policy and backoff (`_core/retry.py`), the error classes and messages (`_core/errors.py`),
//! and the response decoding (`_core/response_types.py`). The HTTP layer is the `Transport` trait so
//! tests can replay recorded responses; `UreqTransport` is the real one (ureq, rustls).
//!
//! Questions are the SDK's `model_dump()` dicts (see `native/bot/src/iface.rs`). Answers come back as
//! `model_dump()` dicts per question name. Python's int dict keys (score legends and probabilities)
//! are JSON string keys here, as in any JSON dump of them.

use serde_json::{Map, Value};
use std::fmt;
use std::time::{Duration, Instant};

pub const SDK_VERSION: &str = "0.7.4";
const SDK_NAME: &str = "typesafe-sdk";
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
pub const DEFAULT_MODEL: &str = "jev-latest";
/// `constants.DEFAULT_TIMEOUT`, seconds per HTTP operation.
pub const DEFAULT_TIMEOUT: f64 = 10.0;
const SYSTEM_ONE_PATH: &str = "/v1/systemone";
const REQUEST_ID_HEADER: &str = "x-typesafe-request-id";
const RETRY_COUNT_HEADER: &str = "X-TypeSafe-Retry-Count";
const RUNTIME_HEADER: &str = "X-TypeSafe-Runtime";
const MAX_ERROR_BODY_LENGTH: usize = 200;
const ANSWER_TYPES: [&str; 3] = ["noul", "choice", "score"];

/// One HTTP attempt, as the SDK builds it: SDK headers only (the HTTP library adds Host etc.).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Seconds for each of connect, read and write (httpx's `Timeout(t)`).
    pub timeout: f64,
}

#[derive(Clone, Debug)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TransportError {
    /// httpx's TimeoutException.
    Timeout,
    /// Any other httpx RequestError (connect refused, reset, ...); the text is the library's message.
    Connection(String),
}

/// Sends one attempt. Implementations must not retry; the client does that.
pub trait Transport {
    fn send(&self, request: &Request) -> Result<RawResponse, TransportError>;
}

/// The real transport: ureq over rustls. Timeouts are per operation, like httpx's `Timeout(t)`.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new(timeout: f64) -> Self {
        let d = Duration::from_secs_f64(timeout);
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(d)
            .timeout_read(d)
            .timeout_write(d)
            .build();
        Self { agent }
    }
}

impl Transport for UreqTransport {
    fn send(&self, request: &Request) -> Result<RawResponse, TransportError> {
        use std::io::Read;
        let mut req = self.agent.request(&request.method, &request.url);
        for (name, value) in &request.headers {
            req = req.set(name, value);
        }
        let response = match req.send_bytes(&request.body) {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => {
                return Err(if is_timeout(&t) {
                    TransportError::Timeout
                } else {
                    TransportError::Connection(t.to_string())
                })
            }
        };
        let status = response.status();
        let headers = response
            .headers_names()
            .into_iter()
            .filter_map(|n| response.header(&n).map(|v| (n.clone(), v.to_string())))
            .collect();
        let mut body = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut body)
            .map_err(|e| TransportError::Connection(e.to_string()))?;
        Ok(RawResponse { status, headers, body })
    }
}

fn is_timeout(t: &ureq::Transport) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(t);
    while let Some(e) = source {
        if let Some(io) = e.downcast_ref::<std::io::Error>() {
            if matches!(io.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) {
                return true;
            }
        }
        source = e.source();
    }
    false
}

/// `_core/retry.py` RetryPolicy with the SDK defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff_initial: f64,
    pub backoff_max: f64,
    pub backoff_jitter: f64,
    pub respect_retry_after: bool,
    pub api_connection_error: bool,
    pub api_timeout_error: bool,
    /// Total budget in seconds per call; `None` disables it.
    pub timeout: Option<f64>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            backoff_initial: 0.5,
            backoff_max: 5.0,
            backoff_jitter: 0.25,
            respect_retry_after: true,
            api_connection_error: true,
            api_timeout_error: true,
            timeout: Some(30.0),
        }
    }
}

/// `_core/errors.py`. `class` is the Python class name; `Display` is `str(exc)`.
#[derive(Clone, Debug)]
pub struct ApiError {
    pub class: &'static str,
    pub status: u16,
    pub body: Option<Value>,
    pub headers: Vec<(String, String)>,
    pub message: String,
    pub endpoint: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Error {
    /// TypeSafeError: a client-side check failed before anything was sent.
    Client(String),
    /// TypeSafeAPIError and its subclasses, including TypeSafeAPIResponseValidationError.
    Api(ApiError),
    /// TypeSafeAPITimeoutError, with the configured timeout.
    Timeout(f64),
    /// TypeSafeAPIConnectionError.
    Connection(String),
}

impl Error {
    pub fn class(&self) -> &'static str {
        match self {
            Error::Client(_) => "TypeSafeError",
            Error::Api(a) => a.class,
            Error::Timeout(_) => "TypeSafeAPITimeoutError",
            Error::Connection(_) => "TypeSafeAPIConnectionError",
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Api(a) => Some(a.status),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Client(m) => write!(f, "{m}"),
            Error::Timeout(t) => write!(f, "Request timed out (timeout={t:?})."),
            Error::Connection(m) => write!(f, "{m}"),
            Error::Api(a) => {
                let mut message = if a.message.is_empty() {
                    a.status.to_string()
                } else {
                    format!("{} {}", a.status, a.message)
                };
                if let Some(endpoint) = &a.endpoint {
                    message = format!("{endpoint}: {message}");
                }
                if let Some(id) = &a.request_id {
                    message.push_str(&format!(" (request_id={id})"));
                }
                write!(f, "{message}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// `api_error`: the class for a non-2xx status.
fn api_error(status: u16, body: Option<Value>, headers: Vec<(String, String)>, endpoint: String) -> ApiError {
    let class = match status {
        400 => "TypeSafeBadRequestError",
        401 => "TypeSafeAuthenticationError",
        403 => "TypeSafePermissionDeniedError",
        404 => "TypeSafeNotFoundError",
        422 => "TypeSafeUnprocessableEntityError",
        429 => "TypeSafeRateLimitError",
        s if s >= 500 => "TypeSafeInternalServerError",
        _ => "TypeSafeAPIError",
    };
    make_api_error(class, status, body, headers, Some(endpoint), None)
}

fn make_api_error(
    class: &'static str,
    status: u16,
    body: Option<Value>,
    headers: Vec<(String, String)>,
    endpoint: Option<String>,
    message: Option<String>,
) -> ApiError {
    let message = match message {
        Some(m) => m,
        None => match extract_message(&body) {
            Some(detail) if !detail.is_empty() => detail,
            _ => match &body {
                None => "status code (no body)".to_string(),
                Some(v) => {
                    let raw = match v {
                        Value::String(s) => s.clone(),
                        other => serde_json::to_string(other).unwrap_or_default(),
                    };
                    if raw.chars().count() > MAX_ERROR_BODY_LENGTH {
                        let cut: String = raw.chars().take(MAX_ERROR_BODY_LENGTH).collect();
                        format!("{cut}\u{2026}")
                    } else {
                        raw
                    }
                }
            },
        },
    };
    let request_id = header(&headers, REQUEST_ID_HEADER).map(str::to_string);
    ApiError { class, status, body, headers, message, endpoint, request_id }
}

/// `errors.extract_message`: the server's message from an error body, or None.
fn extract_message(body: &Option<Value>) -> Option<String> {
    match body {
        None => None,
        Some(Value::String(s)) => (!s.is_empty()).then(|| s.clone()),
        Some(Value::Object(m)) => {
            let error = m.get("error");
            let message = m.get("message");
            let detail = m.get("detail");
            if let Some(Value::String(s)) = error {
                return Some(s.clone());
            }
            if let Some(Value::Object(e)) = error {
                if let Some(Value::String(s)) = e.get("message") {
                    return Some(s.clone());
                }
            }
            if let Some(Value::String(s)) = message {
                return Some(s.clone());
            }
            if let Some(Value::String(s)) = detail {
                return Some(s.clone());
            }
            if let Some(Value::Object(d)) = detail {
                if let Some(Value::String(s)) = d.get("message") {
                    return Some(s.clone());
                }
            }
            if let Some(Value::Array(entries)) = detail {
                let mut parts = Vec::new();
                for entry in entries {
                    let Value::Object(e) = entry else { continue };
                    let Some(Value::String(msg)) = e.get("msg") else { continue };
                    let path = match e.get("loc") {
                        Some(Value::Array(loc)) => loc
                            .iter()
                            .filter(|item| !matches!(item, Value::String(s) if s == "body"))
                            .map(json_str)
                            .collect::<Vec<_>>()
                            .join("."),
                        _ => String::new(),
                    };
                    parts.push(if path.is_empty() { msg.clone() } else { format!("{path}: {msg}") });
                }
                let joined = parts.join("; ");
                return (!joined.is_empty()).then_some(joined);
            }
            None
        }
        Some(_) => None,
    }
}

/// Python `str()` of a JSON scalar, for error locations.
fn json_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A decoded `system_one` response. Answers are `model_dump()` dicts in the Python field order.
#[derive(Clone, Debug)]
pub struct Response {
    pub model: String,
    pub answers: Map<String, Value>,
    /// `usage.input_tokens`, None when the API omits it (the bot counts None as 0).
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

/// `errors.parse_retry_after`: milliseconds from `retry-after-ms` or `retry-after`, or None.
/// HTTP-date values of `retry-after` are not parsed (they count as unparseable).
pub fn parse_retry_after(headers: &[(String, String)]) -> Option<f64> {
    for (name, multiplier) in [("retry-after-ms", 1.0), ("retry-after", 1000.0)] {
        let Some(raw) = header(headers, name) else { continue };
        let text = raw.trim();
        let parsed = if text.is_empty() { Ok(0.0) } else { text.parse::<f64>() };
        match parsed {
            // The Python code tries an HTTP date here, then falls through to the next header.
            Err(_) => continue,
            Ok(value) => {
                if value.is_finite() {
                    if value >= 0.0 {
                        let delay = value * multiplier;
                        if delay.is_finite() {
                            return Some(delay);
                        }
                    } else if name == "retry-after" {
                        return None;
                    }
                }
            }
        }
    }
    None
}

/// `retry._backoff` with the jitter draw passed in (`random` is `random.random()`, in [0, 1)).
pub fn backoff(attempt: u32, initial: f64, maximum: f64, jitter: f64, random: f64) -> f64 {
    if initial == 0.0 || maximum == 0.0 {
        return 0.0;
    }
    let exponent = attempt as i32 - 1;
    let exponential = if (exponent as f64) >= maximum.log2() - initial.log2() {
        maximum
    } else {
        initial * 2f64.powi(exponent)
    };
    let delay = exponential * (1.0 - random * jitter);
    // Python round(delay, 3): correctly rounded, ties to even.
    let rounded: f64 = format!("{delay:.3}").parse().unwrap_or(delay);
    exponential.min(rounded)
}

fn random_unit() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    h.write_u64(nanos as u64);
    (h.finish() >> 11) as f64 / (1u64 << 53) as f64
}

fn real_sleep(seconds: f64) {
    if seconds > 0.0 {
        std::thread::sleep(Duration::from_secs_f64(seconds));
    }
}

pub struct TypeSafeClient<T: Transport> {
    api_key: String,
    base_url: String,
    model: String,
    timeout: f64,
    retry: RetryPolicy,
    transport: T,
    sleep: fn(f64),
    jitter: fn() -> f64,
}

impl TypeSafeClient<UreqTransport> {
    /// `TypeSafeClient(api_key=..., model=...)`: explicit values win, then the environment
    /// (`TYPESAFE_API_KEY`, `TYPESAFE_DEFAULT_MODEL`, `TYPESAFE_BASE_URL`), then the defaults.
    pub fn new(api_key: Option<&str>, model: Option<&str>) -> Result<Self, Error> {
        Self::with_transport(api_key, model, UreqTransport::new(DEFAULT_TIMEOUT))
    }
}

impl<T: Transport> TypeSafeClient<T> {
    pub fn with_transport(api_key: Option<&str>, model: Option<&str>, transport: T) -> Result<Self, Error> {
        let key = resolve_string(api_key, "TYPESAFE_API_KEY", "").trim().to_string();
        if key.is_empty() {
            return Err(Error::Client(
                "No API key was provided. Pass api_key or set the TYPESAFE_API_KEY environment variable.".into(),
            ));
        }
        if !key.is_ascii() || key.chars().any(|c| c.is_control() || c == ' ') {
            return Err(Error::Client(
                "API key must contain only printable ASCII characters without whitespace.".into(),
            ));
        }
        let base_url = resolve_string(None, "TYPESAFE_BASE_URL", DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_string();
        let model = resolve_string(model, "TYPESAFE_DEFAULT_MODEL", DEFAULT_MODEL);
        Ok(Self {
            api_key: key,
            base_url,
            model,
            timeout: DEFAULT_TIMEOUT,
            retry: RetryPolicy::default(),
            transport,
            sleep: real_sleep,
            jitter: random_unit,
        })
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Replace the wait between attempts (tests use a no-op).
    pub fn with_sleep(mut self, sleep: fn(f64)) -> Self {
        self.sleep = sleep;
        self
    }

    pub fn with_jitter(mut self, jitter: fn() -> f64) -> Self {
        self.jitter = jitter;
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// `client.system_one(state=state, questions=questions)`; `questions` are model_dump dicts.
    /// `state` is the JSON value the SDK would send (`Null` is the SDK's "State is required").
    pub fn system_one(&self, state: &Value, questions: &Map<String, Value>) -> Result<Response, Error> {
        if state.is_null() {
            return Err(Error::Client("State is required.".into()));
        }
        normalize_questions(questions)?;
        let mut body = Map::new();
        body.insert("state".into(), state.clone());
        body.insert("model".into(), Value::String(self.model.clone()));
        body.insert("questions".into(), Value::Object(questions.clone()));
        let content = serde_json::to_string(&Value::Object(body)).expect("JSON body");
        let runtime = format!(
            "rust/{} ({}; {})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let request = Request {
            method: "POST".into(),
            url: format!("{}{}", self.base_url, SYSTEM_ONE_PATH),
            headers: vec![
                ("Authorization".into(), format!("Bearer {}", self.api_key)),
                ("Accept".into(), "application/json".into()),
                ("User-Agent".into(), format!("{SDK_NAME}/{SDK_VERSION}")),
                ("X-TypeSafe-SDK".into(), format!("{SDK_NAME}/{SDK_VERSION}")),
                (RUNTIME_HEADER.into(), runtime),
                ("Content-Type".into(), "application/json".into()),
            ],
            body: content.into_bytes(),
            timeout: self.timeout,
        };
        self.execute(&request)
    }

    /// `transport.send` with tenacity's loop: `stop_after_attempt(max_retries + 1)`,
    /// `stop_before_delay(timeout)`, the retry predicate, then Retry-After or backoff.
    fn execute(&self, request: &Request) -> Result<Response, Error> {
        let start = Instant::now();
        let mut attempts: u32 = 0;
        loop {
            attempts += 1;
            let mut attempt_request = request.clone();
            if attempts > 1 {
                attempt_request.headers.push((RETRY_COUNT_HEADER.into(), (attempts - 1).to_string()));
            }
            let outcome = match self.transport.send(&attempt_request) {
                Err(TransportError::Timeout) => Err(Error::Timeout(self.timeout)),
                Err(TransportError::Connection(m)) => Err(Error::Connection(format!("Connection error: {m}"))),
                Ok(raw) => parse_system_one(raw, &request.method, &request.url),
            };
            let error = match outcome {
                Ok(response) => return Ok(response),
                Err(e) => e,
            };
            if !self.retryable(&error) {
                return Err(error);
            }
            let wait = self.wait(&error, attempts);
            let elapsed = start.elapsed().as_secs_f64();
            let over_budget = self.retry.timeout.is_some_and(|t| elapsed + wait >= t);
            if attempts > self.retry.max_retries || over_budget {
                return Err(error);
            }
            (self.sleep)(wait);
        }
    }

    fn retryable(&self, error: &Error) -> bool {
        match error {
            Error::Timeout(_) => self.retry.api_timeout_error,
            Error::Connection(_) => self.retry.api_connection_error,
            Error::Api(a) => a.status == 408 || a.status == 429 || (500..600).contains(&a.status),
            Error::Client(_) => false,
        }
    }

    fn wait(&self, error: &Error, attempt: u32) -> f64 {
        if self.retry.respect_retry_after {
            if let Error::Api(a) = error {
                if let Some(ms) = parse_retry_after(&a.headers) {
                    return ms / 1000.0;
                }
            }
        }
        backoff(
            attempt,
            self.retry.backoff_initial,
            self.retry.backoff_max,
            self.retry.backoff_jitter,
            (self.jitter)(),
        )
    }
}

fn resolve_string(value: Option<&str>, env: &str, default: &str) -> String {
    match value {
        Some(v) => v.to_string(),
        None => match std::env::var(env) {
            Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => default.to_string(),
        },
    }
}

/// `response_types._prepare_system_one_response` and `SystemOneResponse._decode`.
fn parse_system_one(raw: RawResponse, method: &str, url: &str) -> Result<Response, Error> {
    let endpoint = format!("{method} {url}");
    let decoded = deserialize(&raw.body);
    if !(200..300).contains(&raw.status) {
        return Err(Error::Api(api_error(raw.status, decoded, raw.headers, endpoint)));
    }
    let validation = |path: &str| {
        Error::Api(make_api_error(
            "TypeSafeAPIResponseValidationError",
            raw.status,
            decoded.clone(),
            raw.headers.clone(),
            Some(endpoint.clone()),
            Some(format!("Invalid response data at '{path}'.")),
        ))
    };
    let Some(Value::Object(mut body)) = decoded.clone() else {
        return Err(validation(""));
    };
    if let Some(Value::Object(answers)) = body.get_mut("answers") {
        let names: Vec<String> = answers.keys().cloned().collect();
        for name in names {
            let ty = match answers.get(&name) {
                Some(Value::Object(a)) => a.get("type").cloned(),
                _ => None,
            };
            match ty {
                Some(Value::String(t)) => {
                    if !ANSWER_TYPES.contains(&t.as_str()) {
                        // Forward compatibility: the SDK warns and drops answer types it does not know.
                        answers.shift_remove(&name);
                    }
                }
                _ => return Err(validation(&format!("answers.{name}.type"))),
            }
        }
    }
    let model = match body.get("model") {
        Some(Value::String(s)) => s.clone(),
        _ => return Err(validation("model")),
    };
    let (input_tokens, output_tokens) = match body.get("usage") {
        Some(Value::Object(u)) => (
            usage_int(u.get("input_tokens")).ok_or_else(|| validation("usage.input_tokens"))?,
            usage_int(u.get("output_tokens")).ok_or_else(|| validation("usage.output_tokens"))?,
        ),
        _ => return Err(validation("usage")),
    };
    let mut answers = Map::new();
    match body.get("answers") {
        None => {}
        Some(Value::Object(raw_answers)) => {
            for (name, raw_answer) in raw_answers {
                let answer = decode_answer(name, raw_answer).map_err(|path| validation(&path))?;
                answers.insert(name.clone(), answer);
            }
        }
        Some(_) => return Err(validation("answers")),
    }
    Ok(Response { model, answers, input_tokens, output_tokens })
}

/// Pydantic's strict `int | None` for a usage count. Outer None: wrong type.
fn usage_int(v: Option<&Value>) -> Option<Option<i64>> {
    match v {
        None | Some(Value::Null) => Some(None),
        Some(Value::Number(n)) => n.as_i64().map(Some),
        _ => None,
    }
}

/// Pydantic's strict `float`: JSON numbers, ints included.
fn strict_float(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64(),
        _ => None,
    }
}

/// A `dict[str, float]` (choice probabilities) or `dict[int, float]` (score probabilities; the keys
/// are JSON strings that must parse as ints) as ordered (key, value) pairs.
fn float_map(v: Option<&Value>, field: &str, name: &str, int_keys: bool) -> Result<Vec<(String, f64)>, String> {
    let Some(Value::Object(m)) = v else {
        return Err(format!("answers.{name}.{field}"));
    };
    let mut out = Vec::new();
    for (k, x) in m {
        let key = if int_keys {
            int_key(k).ok_or_else(|| format!("answers.{name}.{field}"))?.to_string()
        } else {
            k.clone()
        };
        let f = strict_float(Some(x)).ok_or_else(|| format!("answers.{name}.{field}.{k}"))?;
        out.push((key, f));
    }
    Ok(out)
}

/// One answer, as its `model_dump()` dict. Err carries the SDK's dotted field path.
fn decode_answer(name: &str, raw: &Value) -> Result<Value, String> {
    let Value::Object(a) = raw else {
        return Err(format!("answers.{name}.type"));
    };
    let field = |f: &str| format!("answers.{name}.{f}");
    match a.get("type").and_then(Value::as_str) {
        Some("noul") => {
            let noul = strict_float(a.get("noul")).ok_or_else(|| field("noul"))?;
            Ok(serde_json::json!({"type": "noul", "noul": noul}))
        }
        Some("choice") => {
            let choice = match a.get("choice") {
                Some(Value::String(s)) => s.clone(),
                _ => return Err(field("choice")),
            };
            let confidence = strict_float(a.get("confidence")).ok_or_else(|| field("confidence"))?;
            let probs = float_map(a.get("probabilities"), "probabilities", name, false)?;
            let mut out = Map::new();
            out.insert("type".into(), "choice".into());
            out.insert("choice".into(), Value::String(choice));
            out.insert("confidence".into(), confidence.into());
            out.insert("probabilities".into(), float_object(probs));
            Ok(Value::Object(out))
        }
        Some("score") => {
            let score = strict_float(a.get("score")).ok_or_else(|| field("score"))?;
            let confidence = strict_float(a.get("confidence")).ok_or_else(|| field("confidence"))?;
            let legend = match a.get("legend") {
                Some(Value::Object(l)) => {
                    let mut out = Map::new();
                    for (k, v) in l {
                        let key = int_key(k).ok_or_else(|| field("legend"))?;
                        match v {
                            Value::String(_) | Value::Object(_) | Value::Array(_) => {
                                out.insert(key.to_string(), v.clone());
                            }
                            _ => return Err(format!("answers.{name}.legend.{k}")),
                        }
                    }
                    out
                }
                _ => return Err(field("legend")),
            };
            let probs = float_map(a.get("probabilities"), "probabilities", name, true)?;
            let mut out = Map::new();
            out.insert("type".into(), "score".into());
            out.insert("score".into(), score.into());
            out.insert("confidence".into(), confidence.into());
            out.insert("legend".into(), Value::Object(legend));
            out.insert("probabilities".into(), float_object(probs));
            Ok(Value::Object(out))
        }
        _ => Err(format!("answers.{name}.type")),
    }
}

fn float_object(pairs: Vec<(String, f64)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, f)| (k, Value::from(f))).collect())
}

/// Python `int(key)` for a dict key of `dict[int, ...]`, restricted to plain decimal integers.
fn int_key(k: &str) -> Option<i64> {
    k.parse::<i64>().ok()
}

/// `json.deserialize`: None for an empty body, the JSON value, or the text if it is not JSON.
fn deserialize(bytes: &[u8]) -> Option<Value> {
    if bytes.is_empty() {
        return None;
    }
    match serde_json::from_slice(bytes) {
        Ok(Value::Null) => None,
        Ok(v) => Some(v),
        Err(_) => Some(Value::String(String::from_utf8_lossy(bytes).into_owned())),
    }
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Python truthiness of a JSON value.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// `questions.normalize_questions` on model_dump dicts.
fn normalize_questions(questions: &Map<String, Value>) -> Result<(), Error> {
    if questions.is_empty() {
        return Err(Error::Client("At least one question is required.".into()));
    }
    for (name, question) in questions {
        let not_dict = || {
            Error::Client(format!(
                r#"Question "{name}" must be a question object or a dictionary with a nonempty string "type"."#
            ))
        };
        let Value::Object(q) = question else { return Err(not_dict()) };
        let ty = match q.get("type") {
            Some(Value::String(t)) if !t.is_empty() => t.as_str(),
            _ => return Err(not_dict()),
        };
        if (ty == "choice" || ty == "score") && !q.contains_key("criteria") {
            return Err(Error::Client(format!(r#"Question "{name}" requires "criteria"."#)));
        }
        match ty {
            "score" if !truthy(q.get("criteria")) => {
                return Err(Error::Client(format!(
                    r#"Score question "{name}" has no criteria; at least one score is required."#
                )))
            }
            "choice" if !truthy(q.get("criteria")) => {
                return Err(Error::Client(format!(
                    r#"Choice question "{name}" has no criteria; at least one choice is required."#
                )))
            }
            "noul" => {
                let instructions = q.get("instructions").unwrap_or(&Value::Null);
                let criteria = q.get("criteria").unwrap_or(&Value::Null);
                if !criteria.is_null() && !criteria.is_object() {
                    return Err(Error::Client(format!(r#"Noul question "{name}" criteria must be an object."#)));
                }
                let described = matches!(criteria, Value::Object(c) if c.values().any(|v| !v.is_null()));
                if instructions.is_null() && !described {
                    return Err(Error::Client(format!(
                        r#"Noul question "{name}" has neither instructions nor criteria; at least one is required."#
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
