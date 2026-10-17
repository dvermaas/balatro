"""Fixture for native/typesafe: the typesafe-sdk 0.7.4 client, run with no network.

Each row is one system_one call. The SDK's own httpx2 transport is replaced by a scripted
BaseTransport (the lowest layer the SDK sends through), which records every attempt's method, URL,
SDK headers and body, and answers from a scripted server. Rows:
  {"fn": "system_one", "name", "args": {"state", "questions"}, "server": [step...],
   "requests": [{"method", "url", "headers", "body"}...], "ret": {...} | "err": {...}}
  {"fn": "backoff", "args": {"attempt", "initial", "maximum", "jitter"}, "ret": float}
Steps are {"status", "headers", "body"} or {"exc": "timeout" | "connect"}. Run with the main repo's venv:
  C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe scripts/port/fixtures/typesafe.py
"""

import json
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402  (puts the v27 sources on sys.path)

import httpx2  # noqa: E402
from typesafe_sdk import Choice, Noul, Score, TypeSafeClient  # noqa: E402
from typesafe_sdk._core.retry import _backoff  # noqa: E402

SDK_HEADERS = ("Authorization", "Accept", "User-Agent", "X-TypeSafe-SDK", "Content-Type", "X-TypeSafe-Retry-Count")
KEY = "test-key"
MODEL = "jev-latest"
sleeps = []
time.sleep = lambda s: sleeps.append(s)  # the SDK's retries would otherwise wait for real


class Scripted(httpx2.BaseTransport):
    def __init__(self, steps):
        self.steps = list(steps)
        self.requests = []

    def handle_request(self, request):
        self.requests.append({
            "method": request.method,
            "url": str(request.url),
            "headers": {n: request.headers[n] for n in SDK_HEADERS if n in request.headers},
            "body": request.content.decode("utf-8"),
        })
        step = self.steps.pop(0) if self.steps else {"status": 200, "headers": {}, "body": "{}"}
        if step.get("exc") == "timeout":
            raise httpx2.ReadTimeout("timed out", request=request)
        if step.get("exc") == "connect":
            raise httpx2.ConnectError("connection refused", request=request)
        return httpx2.Response(step["status"], headers=step.get("headers", {}),
                               content=step["body"].encode("utf-8"), request=request)


def build_questions(dumps):
    cls = {"choice": Choice, "noul": Noul, "score": Score}
    out = {}
    for name, q in dumps.items():
        obj = cls[q["type"]](**q)
        assert obj.model_dump() == q, (name, obj.model_dump(), q)  # the dumps are the wire form
        out[name] = obj
    return out


def ok_body(answers, tokens):
    return json.dumps({"model": MODEL, "answers": answers,
                       "usage": {"input_tokens": tokens[0], "output_tokens": tokens[1]}})


def run(name, state, dumps, steps):
    tr = Scripted(steps)
    client = TypeSafeClient(api_key=KEY, model=MODEL, transport=tr)
    row = {"fn": "system_one", "name": name, "args": {"state": state, "questions": dumps}}
    try:
        resp = client.system_one(state=state, questions=build_questions(dumps))
        row["ret"] = {"model": resp.model, "input_tokens": resp.usage.input_tokens,
                      "output_tokens": resp.usage.output_tokens,
                      "answers": {k: a.model_dump() for k, a in resp.answers.items()}}
    except Exception as e:  # noqa: BLE001 - the error mapping is what is under test
        row["err"] = {"class": type(e).__name__, "status": getattr(e, "status", None), "str": str(e)}
    row["server"] = steps
    row["requests"] = tr.requests
    client.close()
    # JSON round trip, as the fixture file stores it (int dict keys become strings)
    return json.loads(json.dumps(row, ensure_ascii=False))


def main():
    rows = []
    # Real JEV traffic: the first two JEV calls of every game, both arms, answered with the recorded answers.
    n_ok = 0
    types = {}
    for arm in ("bot-forecast-items", "bot-forecast-long"):
        for path in states.trace_files(arm):
            seen = 0
            for line in states.read_jsonl(path):
                if line.get("k") != "jev" or seen >= 2:
                    continue
                seen += 1
                for q in line["questions"].values():
                    types[q["type"]] = types.get(q["type"], 0) + 1
                steps = [{"status": 200, "headers": {"x-typesafe-request-id": f"req_{n_ok}"},
                          "body": ok_body(line["answers"], (100 + n_ok, 5 + n_ok))}]
                rows.append(run(f"{arm}/{path.stem.split('.')[0]}#{seen}", line["state"], line["questions"], steps))
                n_ok += 1
    print("jev calls", n_ok, "question types", types)

    # Error and retry cases, on the first recorded JEV call.
    first = next(l for l in states.read_jsonl(states.trace_files("bot-forecast-items")[0]) if l.get("k") == "jev")
    st, dumps = first["state"], first["questions"]
    ok = {"status": 200, "headers": {}, "body": ok_body(first["answers"], (7, 3))}
    rid = {"x-typesafe-request-id": "req_err"}
    cases = [
        ("402 no credits", [{"status": 402, "headers": rid, "body": '{"error":{"message":"Insufficient credits"}}'}]),
        ("422 detail list", [{"status": 422, "headers": {}, "body": json.dumps({"detail": [
            {"loc": ["body", "questions", "tone", "choice", "criteria"], "msg": "Field required", "type": "missing"}]})}]),
        ("500 then ok, Retry-After 1s", [{"status": 500, "headers": {"retry-after": "1"}, "body": '{"message":"boom"}'}, ok]),
        ("429 x3 (retry-after-ms 0)", [{"status": 429, "headers": {"retry-after-ms": "0"}, "body": "slow down"}] * 3),
        ("503 x3, no body", [{"status": 503, "headers": {}, "body": ""}] * 3),
        ("timeout x3", [{"exc": "timeout"}] * 3),
        ("connect error then ok", [{"exc": "connect"}, ok]),
        ("200 not JSON", [{"status": 200, "headers": rid, "body": "<html>oops</html>"}]),
        ("200 without usage", [{"status": 200, "headers": {}, "body": json.dumps({"model": MODEL, "answers": {}})}]),
        ("200 answer missing confidence", [{"status": 200, "headers": {}, "body": json.dumps({
            "model": MODEL, "usage": {"input_tokens": 1, "output_tokens": 1},
            "answers": {"tone": {"type": "choice", "choice": "calm", "probabilities": {"calm": 1}}}})}]),
        ("200 answer without type", [{"status": 200, "headers": {}, "body": json.dumps({
            "model": MODEL, "usage": {"input_tokens": 1, "output_tokens": 1}, "answers": {"tone": {"choice": "calm"}}})}]),
        ("200 unknown answer type dropped", [{"status": 200, "headers": {}, "body": json.dumps({
            "model": MODEL, "usage": {"input_tokens": 1, "output_tokens": 1},
            "answers": {**first["answers"], "order": {"type": "ranking", "ranks": ["a", "b"]}}})}]),
    ]
    for name, steps in cases:
        rows.append(run("error: " + name, st, dumps, steps))

    # Backoff with jitter 0 (the random jitter cannot match across languages).
    for attempt in range(1, 8):
        rows.append({"fn": "backoff", "args": {"attempt": attempt, "initial": 0.5, "maximum": 5.0, "jitter": 0.0},
                     "ret": _backoff(attempt, 0.5, 5.0, 0.0)})
    rows.append({"fn": "backoff", "args": {"attempt": 3, "initial": 0.0, "maximum": 5.0, "jitter": 0.0},
                 "ret": _backoff(3, 0.0, 5.0, 0.0)})

    print("rows", len(rows), "sleeps the SDK asked for", sleeps)
    print("wrote", states.write_fixture("typesafe", rows))


if __name__ == "__main__":
    main()
