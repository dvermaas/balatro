# jevbot handoff: the `JevBot` interface for decisions porters

Files: `native/bot/src/jev_bot.rs` (bot), `codejev.rs` (JEV stand-in), `replay.rs` (trace harness),
`tests/bot_replay.rs`. Handlers are fn stubs with `todo!()` bodies that you replace, signature fixed:

    decisions::blind_select(bot: &mut JevBot, gs: &Value) -> Result<(), RpcError>   // decisions/mod.rs
    decisions::play::selecting_hand / shop::shop / booster::booster  (same signature; mod.rs re-exports
    `pub use play::selecting_hand; pub use shop::shop; pub use booster::booster;` - keep those lines)

`gs` is the state `step()` fetched (a clone of `bot.last_state` at that time).

## Types (jev_bot.rs)
- `Action = (String, Option<Value>)`: `(method, params)`; `None` = Python None. `action_value(&a)` -> `["m", params|null]`.
- `Options = Vec<(String, (String, Action))>`: Python's `{key: (desc, (method, params))}`, **insertion ordered**
  (the order JEV and CodeJev see). Build entries with `opt(key, desc, method, params)`.
- `Stop {Rpc, Runtime(String), BenchmarkComplete}`: what `step()` returns on Python exceptions.

## Fields (Python name -> type)
`jev: Box<dyn Jev>`, `game: Box<dyn Game>`, `oracle: SharedOracle` (= `Rc<RefCell<dyn Oracle>>`, use
`bot.oracle.borrow_mut().forecast(&gs, n, bosses, &keys, budget_s)`; `rollout.forecast`, `items_forecast`,
`picker_values`, `pick_pub`), `clock: Box<dyn BotClock>` (time/perf/sleep; zero and no sleep in replay),
`recorder: RunRecorder`, `backend: String`, `error_backoff: f64`, `deck/stake/model: String`,
`auto_new_run`, `events: VecDeque<Value>` (cap 400), `_event_id: i64`, `stats: Stats {decisions,
input_tokens, output_tokens, jev_ms, runs, wins, best_ante, errors}`, `last_state: Option<Value>`,
`_last_pack_cost: Value` (init 0), `_round_blind: Value` (Null = None), `_round_had_bones`, `_auto_failed:
HashSet<String>`, `_finished_seed: Value`, `bench_*` (parked), and the decisions.py / tarots.py ones:
`_shop_key: Value` (None=Null, compare with `==`), `_shop_actions: i64`, `_failed: HashSet<String>` (holds
`json.dumps(action)`: use `pycompat::json::dumps`), `_rerolls: i64`, `_recent: Vec<Value>`, `_recent_seed: Value`
(Python `getattr(bot,"_recent_seed",None)` = Null initially), `_forecast_cache` / `_item_forecast_cache:
Option<(Value /*sig*/, Value /*forecast*/)>` (Python `getattr(..., None)`). `hasattr(bot,"recorder")` is always true.
Constants: `SHOP_ACTION_CAP`, `TRANSITIONAL_WAIT`.

## Methods
- `emit(&mut self, kind: &str, data: Value)`: `data` is a JSON **object in Python kwarg order** (`json!({..})`).
  id/t/kind are prepended. Events are compared with the trace "event" lines (keys t, ms, *_seconds, *_s ignored).
- `act(&mut self, method: &str, params: Option<&Value>) -> Result<Value, RpcError>`: emits action/outcome,
  updates `last_state`. Propagate with `?` exactly where Python lets `RPCError` escape; catch with `match`
  where Python has `except RPCError` (e.g. `bot._failed.add(..)` in `_shop_act`).
- `ask(&mut self, qid, gs, instructions: &str, options: &Options) -> Action`: one Choice (single option =
  "forced", no JEV). Use `let (m, p) = bot.ask(..); bot.act(&m, p.as_ref())?;`.
- `query(&mut self, gs, state: &Value, questions: &Map<String, Value>) -> (Map<String, Value>, Value)`:
  returns `(answers, meta)`; answers are the SDK dumps (`answers["best_play"]["choice"]`, `["probabilities"]`,
  `["score"]`, `["noul"]`, `["confidence"]`); `meta` goes to `record_decision(question, gs, &choice, &action,
  &options, &probs, &confidence, &meta, display)` (all `&Value` except question/display, which are `&str`;
  `action` is the JSON you want in the event, e.g. `action_value(&a)` or a list of them).
- `build_state(gs) -> Value` (`{"rules","game"}`), `summarize`, `item_desc(c, Option<&[String]>)`,
  `current_blind(gs) -> Value` (`{}` if none), free functions. `area` / `cards_of` are in `gs.rs`.
- `step()`, `on_menu`, `on_game_over`, `on_round_eval`, `_finish_run`: ported (benchmark queue in
  `on_menu` is `todo!()`; the lab plays one seed).

## Question builders (build `Map<String, Value>` of qid -> question; key order = Python dict order)
- `choice_q(instructions: &str, &Options)` / `choice_q_map(instructions, Map criteria)`
- `score_q(instructions: Value /*str or dict*/, criteria: Value /*ITEM_VALUE_LEVELS as json list*/)`
- `noul_q(instructions: &str, criteria: Value /*{"true": .., "false": ..}*/)`
Shapes match the traces ("type", "instructions", "criteria").

## Conventions
- JEV transport errors panic (Python has no handler either). `RpcError` is the only recoverable error; its
  `Display` is `message`, which must equal Python `str(e)` (`"name: message"`; replay traces already store it so).
- Oracle args: pass `keys` sorted; `budget_s` `None` when Python passes none (TraceOracle compares only recorded args).
- Python `time.strftime`/`time.time()` in decisions: use `bot.clock.time()` (0.0 in replay; only in unrecorded debug logs).

## Replay
`cargo test --test bot_replay -- --nocapture` (about 15 s): replays 60 games and prints
`bot replay: N/60 games identical; records matched min/median/max a/b/c` plus where each stopped.
`JEV_REPLAY_ASSERT=1` makes it fail unless 60/60. Records = game calls + JEV requests + oracle calls + events.
`JEV_REPLAY_SKIP=1`: a step whose handler panics with todo!() is rewound and its recorded records skipped (counted per handler in the summary); `JEV_REPLAY_GAMES=<n>` replays only the first n games.
Other tests: `build_state_matches_asks` (693/693 ask states), `codejev_matches_answers` (2593/2593, arm E;
arm F is exercised by the full replay because its oracle calls interleave with decisions').
Current state: all 60 stop at `decisions::blind_select todo!()` after 4 matched records (start + first gamestate).
`iface.rs` gained `Jev::observe(&mut self, last_state)` (default no-op) and `SharedOracle`.
