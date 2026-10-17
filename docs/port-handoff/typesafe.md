# Handoff: typesafe (Rust TypeSafe client)

## Done
- `native/typesafe` (crate `typesafe`): `TypeSafeClient::new` / `with_transport`, `system_one(&Value, &Map)`, `Transport` trait (injectable), `UreqTransport` (ureq 2, rustls), retry loop (max 2 retries, backoff, Retry-After, 30 s budget), error classes and `Display` matching the SDK.
- `scripts/port/fixtures/typesafe.py`: runs the real SDK 0.7.4 through its httpx2 transport hook, with a scripted server. Wrote `runs/_port/fixtures/typesafe.jsonl.gz`: 120 real JEV calls (both arms), 11 error/retry cases, 8 backoff rows.
- `native/typesafe/tests/fixtures.rs`: **140/140 identical** (requests byte-equal on body, method, URL, SDK headers; answers and errors equal by JSON string).

## Left / caveats
- Not reproduced exactly:
  - Backoff jitter is random in both SDK and Rust; only jitter=0 is compared.
  - `Retry-After` HTTP-date values are not parsed.
  - Connection error text comes from ureq, not httpx.
  - Timeout detection in `UreqTransport` is untested offline.
  - Legend value type errors give an approximate field path.
  - Python `int` dict keys become JSON string keys.
- Rust sends `X-TypeSafe-Runtime: rust/...` (volatile, excluded from compare).
- Not committed yet: `native/typesafe/` and `scripts/port/fixtures/typesafe.py` are untracked in branch `port/typesafe`.

## Next step
1. `git add native/typesafe scripts/port/fixtures/typesafe.py docs/port-handoff/typesafe.md` and commit on `port/typesafe`.
2. The jevbot adapter maps `Option` tokens to 0 and `Error` to the bot's error string.
