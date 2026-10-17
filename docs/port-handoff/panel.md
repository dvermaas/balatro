# panel handoff

`native/bot/src/panel.rs` ports `src/panel.py`; `jevbot panel` is the CLI.

Run (sim, no JEV tokens, no game):
`cargo run --release --bin jevbot -- panel --backend rumbot --code-jev --port 8765`
then open http://127.0.0.1:8765 (serves `<root>/src/panel.html`, re-read per request; `--root` defaults to the repo root
two levels above the crate). Real game: `--backend balatrobot`; TypeSafe JEV: omit `--code-jev` (key from `<root>/.env`).
Runs are read from `<root>/runs` (rumbot: `<root>/runs/sim`, prompts from `<root>/runs/_prompts`).

Design: JevBot is not Send, so a worker thread builds and owns it and publishes a snapshot after each step; HTTP threads
(hand-written HTTP/1.1, thread per connection) read the snapshot and queue abandon/settings/cancel for the worker.
Same API and JSON as Python: GET /api/live /api/runs /api/runs/<id> /api/prompts /api/benchmark; POST run/pause/step/abandon/settings.
Errors: JEV failure (a panic "JEV call failed: ..." from `JevBot::query`) -> "Jev API: <msg>" (carries the 402 text); "timed out" /
connect errors -> Python's Balatro hints; then the bot pauses. Tests: `cargo test --test panel` (needs
`scripts/port/fixtures/panel.py` run once).

Not ported: POST /api/benchmark answers 500 (JevBot's benchmark queue / start_benchmark is parked); `--legacy` (the v1 bot
is Python-only: `jevbot panel --legacy` prints "the v1 bot is Python-only: run src/panel.py --legacy" and exits 2); the traceback
in error events; `current_run` is read from the run's summary.json (recorder.run is private); version id is "rust" (no
prompt hash), so Python-recorded runs don't count in `version.runs`; abandon errors surface as error events, not HTTP 500.
