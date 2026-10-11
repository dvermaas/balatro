"""Minimal JSON-RPC client for the BalatroBot mod (http://127.0.0.1:12346)."""

import itertools
import json
import urllib.request


class RPCError(Exception):
    def __init__(self, code, message, name):
        super().__init__(f"{name or code}: {message}")
        self.code = code
        self.name = name


class Balatro:
    def __init__(self, url="http://127.0.0.1:12346", timeout=60):
        self.url = url
        self.timeout = timeout
        self._ids = itertools.count(1)

    def call(self, method, params=None):
        body = {"jsonrpc": "2.0", "method": method, "id": next(self._ids)}
        if params is not None:
            body["params"] = params
        req = urllib.request.Request(
            self.url,
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=self.timeout) as r:
            resp = json.load(r)
        if "error" in resp:
            e = resp["error"]
            raise RPCError(e.get("code"), e.get("message"), (e.get("data") or {}).get("name"))
        return resp["result"]

    def state(self):
        return self.call("gamestate")
