# nanocodex-surface-state

The shared surface-state model every Nanocodex surface renders. Surfaces do
not reconstruct lifecycle from transcript events; they reduce explicit
[`SurfaceEvent`]s into a [`SurfaceState`] and draw its [`Projection`] of cards.

- **Events** (`type`-tagged JSON): `snapshot` (open entities at a cursor),
  `upsert` (one entity), `runtime_epoch` (restart marker), `heartbeat`.
- **Entities** (`kind`-tagged): agent, turn, tool_call, process_session, run,
  hand, brain, decision, status. Lifecycle is
  `admitted | running | completed | failed | cancelled | unknown`; claims carry
  `verified | agent_reported | test_build` provenance.
- **Reducer guarantees**: cursor fencing of replays, sticky terminal states,
  epoch settlement of in-flight work to `unknown`, snapshot authority on
  reconnect, and settlement of tool calls orphaned by their owner.
- **Projection**: ordered cards (pinned Status, Hands, Brain, then the task
  tree), each with a state label, tone, badges and lines, plus a liveness
  footer. All text comes from integers and the caller's clock.
- `agent_events`: maps today's `run.*`/`tool.*` events, durable managed turn
  terminals and runtime epochs to surface events.

`js/nanocodex-surface-state` mirrors the reducer and projection for web;
`fixtures/*.json` with golden `*.cards.json` are checked by both. Regenerate the
goldens after an intentional projection change with
`UPDATE_SURFACE_GOLDEN=1 cargo test -p nanocodex-surface-state`, then run the
JavaScript tests. `nanocodex-tui-render::card_stream` renders the projection in
the terminal (F7 in the TUI).
