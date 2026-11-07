# Docs index

## Current

| Doc | What |
|---|---|
| [lab-lessons.md](lab-lessons.md) | Every policy-lab experiment and what it taught, v27 to now, with the method; start here |
| [samples/cloud-versions.json](samples/cloud-versions.json), [samples/cloud-experiments.json](samples/cloud-experiments.json) | The numbers: one row per version and seed set, one per experiment |
| [samples/cloud-lab/](samples/cloud-lab/README.md) | The fixed seed sets, kept games, learned tables and analysis scripts |
| [real-game-sync.md](real-game-sync.md) | How the simulator (rumbot) is checked against the real game; runs in [samples/real-sync/](samples/real-sync/) |
| [port-handoff/followups.md](port-handoff/followups.md) | Known small gaps left from the Rust port |
| [../native/rumbot/VENDORED.md](../native/rumbot/VENDORED.md) | Every local change to the simulator, citing the game's Lua |
| [rust-port-guide.md](rust-port-guide.md) | Porting conventions, the reference data behind the identity tests, the port's results |

## Reference (not version-bound)

- [coaching-tips-distilled.md](coaching-tips-distilled.md), [ultimate-guide-lessons.md](ultimate-guide-lessons.md): strategy notes from
  expert play.
- [balatrobench-comparison.md](balatrobench-comparison.md): how other LLM Balatro bots score (2026-10-11).
- [typesafe-skill.md](typesafe-skill.md): TypeSafe's agent skill for the JEV API.

## Historical (each says so at the top)

- JEV-bot pilots on the real game, Python era (v12-v18): [v12-experiment.md](v12-experiment.md),
  [v13-concept.md](v13-concept.md), [v15-benchmark-resume.md](v15-benchmark-resume.md), [v16-outlier-pilot.md](v16-outlier-pilot.md),
  [v17-setup-pilot.md](v17-setup-pilot.md), [v18-draft.md](v18-draft.md), [loss-audit.md](loss-audit.md),
  [methodology-audit.md](methodology-audit.md), [mechanics-milestone.md](mechanics-milestone.md).
- The Python lab and its Jackdaw simulator: [policy-lab.md](policy-lab.md), [rollout-forecast.md](rollout-forecast.md),
  [sim-backend.md](sim-backend.md), [jackdaw-fidelity.md](jackdaw-fidelity.md), [issue2-comment.md](issue2-comment.md).
- The Rust port: [rust-port-scope.md](rust-port-scope.md) and the task handoffs in [port-handoff/](port-handoff/).
