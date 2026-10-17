"""Fixture for native/bot/src/balatro.rs: Python's BalatroBot client (src/balatro.py) against a local
mock server. Each row is one call: the request body the server received (raw text, as json.dumps wrote
it), the canned response text the mock returned, and what Balatro.call() gave back (the result as
JSON, or the str() of the exception it raised). The Rust test replays the same calls against its own
mock with the same canned responses and compares the bodies (canonical compact JSON, key order kept)
and the results/messages. No real game is contacted: the server binds 127.0.0.1 on an ephemeral port.

Writes runs/_port/fixtures/balatro.jsonl.gz.
"""

import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import balatro  # noqa: E402
import states  # noqa: E402

# (case name, [(method, params_or_None)], canned response per call). A canned response is the raw
# body text the mock sends back; the request id in it is ignored by the client.
CASES = [
    ("ok_gamestate", [("gamestate", None)],
     ['{"jsonrpc":"2.0","id":1,"result":{"state":"MENU","ante_num":1,"money":4}}']),
    ("ok_start_params", [("start", {"deck": "RED", "stake": "WHITE", "seed": "TU9KANLF"})],
     ['{"jsonrpc":"2.0","id":1,"result":{"ok":true}}']),
    ("ok_empty_table", [("health", None)],
     ['{"jsonrpc":"2.0","id":1,"result":[]}']),
    ("ok_then_error_ids", [("gamestate", None), ("cash_out", None), ("gamestate", None)],
     ['{"jsonrpc":"2.0","id":1,"result":{"state":"ROUND_EVAL"}}',
      '{"jsonrpc":"2.0","id":2,"error":{"code":-32001,"message":"Not allowed in this state","data":{"name":"BAD_STATE"}}}',
      '{"jsonrpc":"2.0","id":3,"result":{"state":"ROUND_EVAL"}}']),
    ("err_named", [("select", None)],
     ['{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"Invalid params","data":{"name":"INVALID_PARAMS"}}}']),
    ("err_no_data", [("play", {"cards": [0]})],
     ['{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"Something broke"}}']),
    ("err_null_data", [("reroll", None)],
     ['{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"No money","data":null}}']),
    ("err_empty_name_uses_code", [("buy", {"card": 2})],
     ['{"jsonrpc":"2.0","id":1,"error":{"code":-32003,"message":"Nope","data":{"name":""}}}']),
    ("err_no_message", [("pack", None)],
     ['{"jsonrpc":"2.0","id":1,"error":{"code":-32004,"data":{"name":"PACK"}}}']),
    ("err_no_code_no_name", [("skip", None)],
     ['{"jsonrpc":"2.0","id":1,"error":{"message":"bare"}}']),
]


def make_handler(seen, responses):
    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            n = int(self.headers.get("Content-Length", "0"))
            seen.append({"body": self.rfile.read(n).decode("utf-8"),
                         "content_type": self.headers.get("Content-Type")})
            out = responses.pop(0).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(out)))
            self.end_headers()
            self.wfile.write(out)

        def log_message(self, *a):
            pass

    return Handler


def run_case(name, calls, canned):
    seen, responses = [], list(canned)
    server = ThreadingHTTPServer(("127.0.0.1", 0), make_handler(seen, responses))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    client = balatro.Balatro(url=f"http://127.0.0.1:{server.server_address[1]}")
    out = []
    try:
        for method, params in calls:
            try:
                res = {"result": client.call(method, params)}
            except balatro.RPCError as e:
                res = {"rpc_error": {"code": e.code, "name": e.name, "str": str(e)}}
            out.append({"method": method, "params": params, **res})
    finally:
        server.shutdown()
        server.server_close()
    return {"case": name, "requests": seen, "canned": canned, "calls": out}


def main():
    rows = [run_case(n, c, r) for n, c, r in CASES]
    states.write_fixture("balatro", rows)
    print(f"{len(rows)} cases -> runs/_port/fixtures/balatro.jsonl.gz")


if __name__ == "__main__":
    main()
