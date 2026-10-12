# Runthrough: TUI variant B, card stream

Branch `feat/tui-card-stream` (stacked on `feat/surface-state-protocol`). One shared
surface-state model feeds the terminal and the web:

- `crates/nanocodex-surface-state`: a pure reducer (`SurfaceEvent` to `SurfaceState`)
  and the card projection (`SurfaceState::project(now)`). Every label, badge and
  line is produced there. The shared `agent_events` adapter maps the managed wire
  (event envelopes, `run.*`/`tool.*` pairs, durable turn terminals, `runtime_epoch`,
  the `GET /v1/agents/:id/surface` snapshot) onto it.
- `js/nanocodex-surface-state`: the JavaScript mirror for web, with a DOM-free HTML
  renderer and stylesheet.
- `nanocodex-tui-render::card_stream`: the ratatui widget. In the TUI, **F7** swaps
  the transcript pane for the card stream (Esc or F7 returns, PgUp/PgDn page; the
  view follows the newest cards until you page up). The composer stays live.

Reducer guarantees:

- Cursor fencing: replays are counted and never applied.
- Heartbeats and connection changes are control frames: they never fence deltas.
- Terminal states are sticky.
- A newer epoch settles in-flight turns, runs, agents and tools to `unknown`
  ("runtime restarted"). A first-seen epoch is a baseline, not a restart.
- Snapshot authority: an open runtime entity missing from a reconnect snapshot
  becomes `unknown`.
- Orphan settlement: a tool call cannot outlive its owner.
- Attempts: a turn or agent resumed in a newer epoch shows `attempt N`. A tool call
  re-dispatched under the same call id becomes a separate attempt card, so
  "attempt 1 unknown / attempt 2 running" stays visible.

Liveness footer: disconnected time, epoch, cursor, open count, last-event and
heartbeat age, context %, restarts and ignored replays. The TUI redraws it every
second while the view is open.

Provenance badges:

- `verified`: checked evidence.
- `receipt`: the runtime observed the tool call return. It says nothing about the
  call's claim.
- `agent-reported`, `test-build` and `unsourced`.

## Cross-surface parity

The eight synthetic fixtures in `crates/nanocodex-surface-state/fixtures/*.json`
are event streams with clock steps, and `*.cards.json` holds the golden projection.
The Rust and JavaScript tests reduce the same events and must produce the same
golden cards.

~~~sh
export CARGO_TARGET_DIR=/srv/nanocodex/w/surface-target-A
cargo test -p nanocodex-surface-state                  # 8 fixtures + wire round-trip, 6 producer journeys
(cd js/nanocodex-surface-state && pnpm test)           # 10 node tests (same goldens) + tsc type test
cargo test -p nanocodex-tui-render --test card_stream  # every fixture at 40/80/120 columns
cargo test -p nanocodex-bin --test nanocodex2_tui_lifecycle terminal_card_stream -- --nocapture
~~~

## 1. Real TUI over a controlled managed service (PTY)

The journeys start the built `nanocodex` binary in a PTY against the test file's
controlled managed service, press F7, and print the `vt100` screen (160x32).

### 1a. Restart receipt settles a running tool

`terminal_card_stream_settles_tools_from_runtime_receipts`:

1. `input.accepted` (the prompt becomes the Turn subject), `run.started` and two
   `tool.call`s, one of which completes with a `receipt`.
2. The protocol's restart receipt: an unknown-outcome `tool.result` with
   `reason: runtime_restarted` and `epoch: 1`.
3. The durable turn completion.
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpRFMsxD                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn CARD_STREAM_JOURNEY ───────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release -p card-stream-journey ───────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
  │ exec_command in /srv/cards                                                                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell echo CARD_PROBE ────────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [receipt]                                                                                                                                                  │
  │ exec_command                                                                                                                                               │
  │ exit 0 · 10 B output                                                                                                                                       │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 5 · 2 open · last event 0s ago
╭─ 0%/272k Thinking… ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────── 0s  gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter steer · Tab queue · Esc Esc stop ────────────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

Restart receipt: the card settles to `unknown`, and the epoch named by the receipt advances the footer:
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpRFMsxD                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn CARD_STREAM_JOURNEY ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
│ [outcome unknown]                                                                                                                                            │
│ runtime restarted                                                                                                                                            │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release -p card-stream-journey ──────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ runtime restarted                                                                                                                                          │
  │ exec_command in /srv/cards                                                                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell echo CARD_PROBE ────────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [receipt]                                                                                                                                                  │
  │ exec_command                                                                                                                                               │
  │ exit 0 · 10 B output                                                                                                                                       │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 7 · 0 open · last event 0s ago · 1 restart(s)
╭─ 0%/272k Thinking… ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────── 0s  gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter steer · Tab queue · Esc Esc stop ────────────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

Durable turn completion (F7 then returns to the transcript, asserted):
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpRFMsxD                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn CARD_STREAM_JOURNEY ───────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 0s ─╮
│ [outcome reconciled]                                                                                                                                         │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release -p card-stream-journey ──────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ runtime restarted                                                                                                                                          │
  │ exec_command in /srv/cards                                                                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell echo CARD_PROBE ────────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [receipt]                                                                                                                                                  │
  │ exec_command                                                                                                                                               │
  │ exit 0 · 10 B output                                                                                                                                       │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 8 · 0 open · last event 0s ago · 1 restart(s)
╭─ 0%/272k ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter send · / actions · @ paths · @@ sessions ────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

### 1b. Dropped stream, then reconnect over the surface snapshot

`terminal_card_stream_adopts_the_surface_snapshot_on_reconnect`: a shell call is
running when the stream breaks.

- While disconnected, the footer says `disconnected Ns` and keeps aging on an idle
  terminal (asserted at 0s and 1s).
- The service's `GET /v1/agents/:id/surface` answers with a body captured from the
  real producer (`fixtures/managed/child-surface-during.json`), rebased onto this
  turn, clock-shifted to now, at epoch 1.
- On Enter-to-reconnect the TUI catches up history, then reads the snapshot
  (`ManagedClient::surface`) and applies it after the replayed records.
- The lost call settles to `unknown`. The child agent's last report is from epoch 0,
  so it is `unknown` too. The snapshot's open calls appear nested under their
  owners.
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpiNaPZW                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn CARD_RECONNECT_JOURNEY ────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo test -p lost-before-restart ──────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
  │ exec_command                                                                                                                                               │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 3 · 2 open · last event 0s ago
╭─ 0%/272k Running exec command… ─────────────────────────────────────────────────────────────────────────────────────────────────────── 0s  gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter steer · Tab queue · Esc Esc stop ────────────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

Stream dropped:
~~~text
╭─ BRAIN Workspace ─╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮──────── attached ─╮
│ /tmp/.tmpiNaPZW   │ Could not reconnect: managed request failed (503 Service Unavailable): unavailable: try again. Press Enter to retry. │                   │
╰───────────────────╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯───────────────────╯
╭─ TURN Turn CARD_RECONNECT_JOURNEY ────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 1s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo test -p lost-before-restart ──────────────────────────────────────────────────────────────────────────────────────────────── running 1s ─╮
  │ exec_command                                                                                                                                               │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

▲ disconnected 1s · epoch 0 · cursor 3 · 2 open · last event 1s ago
╭─ 0%/272k Connection lost · Enter to reconnect ──────────────────────────────────────────────────────────────────────────────────────── 1s  gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ send paused · keep editing ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

After reconnect and the surface snapshot:
~~~text
╰───────────────────────────────────────────────────────────────────────╭─────────────╮────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn CARD_RECONNECT_JOURNEY ────────────────────────────────────│ Reconnected │─────────────────────────────────────────────────────────── running 1s ─╮
│ [attempt 2]                                                           ╰─────────────╯                                                                        │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo test -p lost-before-restart ─────────────────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ runtime restarted while disconnected                                                                                                                       │
  │ exec_command                                                                                                                                               │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Surface child ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ last report predates the runtime restart                                                                                                                   │
  │ role: Surface child                                                                                                                                        │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 1s ─╮
    │ exec                                                                                                                                                     │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 1s ─╮
      │ exec_command                                                                                                                                           │
      ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 1s ─╮
  │ exec                                                                                                                                                       │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Wait for agent ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 1s ─╮
    │ wait_agent                                                                                                                                               │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
● epoch 1 · cursor 4 · 5 open · last event 1s ago · 1 restart(s)
╭─ 0%/272k Thinking… ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────── 1s  gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter steer · Tab queue · Esc Esc stop ────────────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

Turn completed:
~~~text
╭─ BRAIN Workspace ─────────────────────────────────────────────────────╭─────────────╮───────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpiNaPZW                                                       │ Reconnected │                                                                        │
╰───────────────────────────────────────────────────────────────────────╰─────────────╯────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn CARD_RECONNECT_JOURNEY ────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 1s ─╮
│ [attempt 2]                                                                                                                                                  │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo test -p lost-before-restart ─────────────────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ runtime restarted while disconnected                                                                                                                       │
  │ exec_command                                                                                                                                               │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Surface child ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ last report predates the runtime restart                                                                                                                   │
  │ role: Surface child                                                                                                                                        │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 1s ─╮
    │ exec                                                                                                                                                     │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 1s ─╮
      │ exec_command                                                                                                                                           │
      ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ owner ended without a tool result                                                                                                                          │
  │ exec                                                                                                                                                       │
● epoch 1 · cursor 5 · 2 open · last event 0s ago · 1 restart(s)
╭─ 0%/272k ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter send · / actions · @ paths · @@ sessions ────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

### 1c. First attach reads the snapshot

`terminal_card_stream_reads_the_surface_snapshot_on_first_attach`: the service already sits at epoch 1 with a child at work (the real `child-surface-moved-on.json` body, clock-shifted). Attach reads `/surface` after history, so the view opens at epoch 1 with the open entities and no spurious restart (asserted). End and Home jump to the newest and oldest cards.
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpvPetqd                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ AGENT Surface child ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
│ role: Surface child                                                                                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
  │ exec                                                                                                                                                       │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
    │ exec_command                                                                                                                                             │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 2 · 3 open · last event 0s ago
╭─ 0%/272k ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter send · / actions · @ paths · @@ sessions ────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

### 1d. A live runtime epoch re-reads the snapshot

`terminal_card_stream_rereads_the_surface_after_a_live_runtime_epoch`: a shell call is running. The service then emits `runtime_epoch` on the WebSocket (the protocol branch opts the socket into `surface=1`). The card stream settles in-flight work to `unknown · runtime restarted` and re-reads `/surface` in the background, so the service's open child work appears with no reconnect (asserted).
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpqKzY9U                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
│ [outcome unknown]                                                                                                                                            │
│ runtime restarted                                                                                                                                            │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo test -p lost-to-epoch ───────────────────────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ runtime restarted                                                                                                                                          │
  │ exec_command                                                                                                                                               │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Surface child ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
  │ role: Surface child                                                                                                                                        │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
    │ exec                                                                                                                                                     │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
      │ exec_command                                                                                                                                           │
      ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 5 · 3 open · last event 0s ago · 1 restart(s)
╭─ 0%/272k Running exec command… ─────────────────────────────────────────────────────────────────────────────────────────────────────── 0s  gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter steer · Tab queue · Esc Esc stop ────────────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

## 2. Fixture journeys in a real terminal (tmux)

`output/runthrough/tmux-fixtures.sh` (ignored) runs the `nanocodex-tui-render` example
`card_stream live <fixture>` in a 100x48 tmux session, presses `n` per step and saves
`tmux capture-pane -p`. The first line is the example's chrome.

### Normal turn
~~~text
normal-turn · step 1/2: tool running   [n]ext [j/k] scroll [q]uit
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 14s ago                                   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ BRAIN Brain durable workspace ─────────────────────────────────────────────────────── attached ─╮
│ /brain                                                                                           │
│ 42 memory note(s) · 3 uncommitted file(s)                                                        │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Run the surface-state tests ──────────────────────────────────────────── running 14s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo test -p nanocodex-surface-state ─────────────────────────────── running 12s ─╮
  │ exec_command in /srv/repo on paradigm                                                          │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 3 · 2 open · last event 12s ago · heartbeat 1s ago · context 18%
~~~
~~~text
normal-turn · step 2/2: turn completed   [n]ext [j/k] scroll [q]uit
╭─ STATUS Status Run the surface-state tests ─────────────────────────────────────────────── done ─╮
│ Done                                                                                             │
│   [verified] surface-state tests pass                                                            │
│     evidence: t1.c1 exit 0                                                                       │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 40s ago                                   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ BRAIN Brain durable workspace ─────────────────────────────────────────────────────── attached ─╮
│ /brain                                                                                           │
│ 42 memory note(s) · 3 uncommitted file(s)                                                        │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Run the surface-state tests ──────────────────────────────────────────── done in 38s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo test -p nanocodex-surface-state ─────────────────────────────── done in 29s ─╮
  │ [receipt]                                                                                      │
  │ exec_command in /srv/repo on paradigm                                                          │
  │ exit 0 · 17 KiB output                                                                         │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 7 · 0 open · last event 2s ago · heartbeat 27s ago · context 18%
~~~

### Subagent swarm: pinned Status, Hands (online, offline/stale), Brain, Decision, task tree
~~~text
subagent-swarm · step 1/1: swarm in flight   [n]ext [j/k] scroll [q]uit
╭─ STATUS Status Redesign surface state ───────────────────────────────────────────────── blocked ─╮
│ Done                                                                                             │
│   [verified] surface-state crate builds                                                          │
│     evidence: cargo build exit 0                                                                 │
│   [agent-reported] variant A review complete                                                     │
│     evidence: a3 summary                                                                         │
│ In progress                                                                                      │
│   [test-build] card-stream renderer tests                                                        │
│     evidence: a2.c1 running                                                                      │
│ Blocked                                                                                          │
│   [unsourced] studio-mac offline for Apple build                                                 │
│ Next                                                                                             │
│   [unsourced] decide merge order                                                                 │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 10s ago                                   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ HAND studio-mac ────────────────────────────────────────────────────────────────────── offline ─╮
│ [offline] [stale resources]                                                                      │
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 1h 10m ago                                │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ BRAIN Brain durable workspace ─────────────────────────────────────────────────────── attached ─╮
│ /brain                                                                                           │
│ 42 memory note(s) · 3 uncommitted file(s)                                                        │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Redesign surface state ────────────────────────────────────────────── running 10m 0s ─╮
│ [quiet 1m 30s]                                                                                   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT protocol-runtime ────────────────────────────────────────────────────── running 9m 55s ─╮
  │ [quiet 3m 0s]                                                                                  │
  │ role: runtime events                                                                           │
  │ Epoch event and snapshot route implemented; tests next                                         │
  │ summary (agent-reported) current at cursor 15                                                  │
  │ context 64%                                                                                    │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT tui-card-stream ─────────────────────────────────────────────────────── running 9m 54s ─╮
  │ [quiet 5m 0s]                                                                                  │
  │ role: card stream TUI                                                                          │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell cargo test -p nanocodex-tui-render ────────────────────────────── running 5m 0s ─╮
    │ [quiet 5m 0s]                                                                                │
    │ exec_command in /srv/repo on paradigm                                                        │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT dogfood-reviewer ────────────────────────────────────────────────────── done in 8m 13s ─╮
● epoch 1 · cursor 18 · 4 open · last event 1m 20s ago · heartbeat 2s ago · context 71%
~~~

### Runtime restart mid-tool

Before: the shell call has been silent for over an hour; the card says so (`quiet`) and the footer warns.
~~~text
restart-mid-tool · step 1/3: before restart: tool silent for an hour   [n]ext [j/k] scroll [q]uit
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 1m 21s ago                                │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Ship the release ──────────────────────────────────────────────────── running 1h 11m ─╮
│ [quiet 1h 11m]                                                                                   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release ──────────────────────────────────────────── running 1h 11m ─╮
  │ [quiet 1h 11m]                                                                                 │
  │ exec_command in /srv/repo on paradigm                                                          │
  │ 0 B output                                                                                     │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT release-notes ───────────────────────────────────────────────────────── running 1h 11m ─╮
  │ [quiet 1h 11m]                                                                                 │
  │ role: docs                                                                                     │
  │ Drafting release notes                                                                         │
  │ summary (agent-reported) current at cursor 3                                                   │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

▲ epoch 1 · cursor 4 · 3 open · last event 1h 11m ago
~~~

`runtime_epoch` 2 settles every in-flight entity of epoch 1:
~~~text
restart-mid-tool · step 2/3: runtime epoch 2: in-flight work settles   [n]ext [j/k] scroll [q]uit
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 1m 23s ago                                │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Ship the release ─────────────────────────────────────────────────────────── unknown ─╮
│ [outcome unknown]                                                                                │
│ runtime restarted                                                                                │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release ─────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                              │
  │ runtime restarted                                                                              │
  │ exec_command in /srv/repo on paradigm                                                          │
  │ 0 B output                                                                                     │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT release-notes ──────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown] [summary stale]                                                              │
  │ runtime restarted                                                                              │
  │ role: docs                                                                                     │
  │ Drafting release notes                                                                         │
  │ summary (agent-reported) from cursor 3; 1 newer change(s) through cursor 5                     │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 2 · cursor 5 · 0 open · last event 1s ago · 1 restart(s)
~~~

After: a new turn runs and the subagent's durable completion reconciles its `unknown`:
~~~text
restart-mid-tool · step 3/3: after restart: new turn and reconciled completion   [n]ext [j/k] scroll
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 2m 0s ago                                 │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Ship the release ─────────────────────────────────────────────────────────── unknown ─╮
│ [outcome unknown]                                                                                │
│ runtime restarted                                                                                │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release ─────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                              │
  │ runtime restarted                                                                              │
  │ exec_command in /srv/repo on paradigm                                                          │
  │ 0 B output                                                                                     │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT release-notes ───────────────────────────────────────────────────────── done in 1h 11m ─╮
  │ [outcome reconciled]                                                                           │
  │ role: docs                                                                                     │
  │ Release notes drafted in docs/RELEASE.md                                                       │
  │ summary (agent-reported) current at cursor 7                                                   │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Status? ──────────────────────────────────────────────────────────────── running 30s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 2 · cursor 7 · 1 open · last event 20s ago · heartbeat 2s ago · context 40% · 1 restart(s)
~~~

### Reconnect

Connected, then three minutes without events (the liveness footer warns):
~~~text
reconnect · step 1/4: connected   [n]ext [j/k] scroll [q]uit
╭─ TURN Turn Index the docs ───────────────────────────────────────────────────────── running 20s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell rg -n surface docs ────────────────────────────────────────────────── running 15s ─╮
  │ exec_command in /srv/repo on paradigm                                                          │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ─────────────────────────────────────────────────────────── running 12s ─╮
  │ exec_command in /srv/repo on paradigm                                                          │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 10 · 3 open · last event 10s ago · heartbeat 1s ago
~~~
~~~text
reconnect · step 2/4: disconnected for 3 minutes   [n]ext [j/k] scroll [q]uit
╭─ TURN Turn Index the docs ────────────────────────────────────────────────────── running 3m 20s ─╮
│ [quiet 3m 10s]                                                                                   │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell rg -n surface docs ─────────────────────────────────────────────── running 3m 15s ─╮
  │ [quiet 3m 10s]                                                                                 │
  │ exec_command in /srv/repo on paradigm                                                          │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ──────────────────────────────────────────────────────── running 3m 12s ─╮
  │ [quiet 3m 10s]                                                                                 │
  │ exec_command in /srv/repo on paradigm                                                          │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

▲ epoch 1 · cursor 10 · 3 open · last event 3m 10s ago · heartbeat 3m 1s ago
~~~

The reconnect snapshot lists only open entities; the missing call becomes `unknown`:
~~~text
reconnect · step 3/4: reconnected: snapshot of open entities   [n]ext [j/k] scroll [q]uit
╭─ TURN Turn Index the docs ────────────────────────────────────────────────────── running 3m 22s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell rg -n surface docs ────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                              │
  │ closed while disconnected                                                                      │
  │ exec_command in /srv/repo on paradigm                                                          │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ──────────────────────────────────────────────────────── running 3m 14s ─╮
  │ exec_command in /srv/repo on paradigm                                                          │
  │ 2 KiB output                                                                                   │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 30 · 2 open · last event 1s ago · heartbeat 3m 3s ago
~~~

Deltas after the snapshot: a stale replay (cursor 9) and a duplicate (cursor 31) are ignored; the real result reconciles the card:
~~~text
reconnect · step 4/4: deltas after snapshot: replays ignored, outcome reconciled   [n]ext [j/k] scro
╭─ TURN Turn Index the docs ────────────────────────────────────────────────────── running 3m 25s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell rg -n surface docs ────────────────────────────────────────────────── done in 55s ─╮
  │ [receipt] [outcome reconciled]                                                                 │
  │ exec_command in /srv/repo on paradigm                                                          │
  │ exit 0 · 512 B output                                                                          │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ──────────────────────────────────────────────────────── running 3m 17s ─╮
  │ exec_command in /srv/repo on paradigm                                                          │
  │ 4 KiB output                                                                                   │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 32 · 2 open · last event 2s ago · heartbeat 3m 6s ago · 2 replay(s) ignored
~~~

### Stale-summary detection

The agent summary predates three newer changes in its subtree, the Hand's resources are 12 minutes old, and context is at 93%. The second frame shows them refreshed:
~~~text
stale-summary · step 1/2: summary and resources stale   [n]ext [j/k] scroll [q]uit
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ [stale resources]                                                                                │
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 12m 0s ago                                │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Fix the flaky test ────────────────────────────────────────────────── running 15m 0s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT fixer ──────────────────────────────────────────────────────────────── running 14m 50s ─╮
  │ [summary stale]                                                                                │
  │ role: tests                                                                                    │
  │ Running cargo test; 2 failures to fix                                                          │
  │ summary (agent-reported) from cursor 5; 3 newer change(s) through cursor 8                     │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell cargo test -p nanocodex-managed ──────────────────────────────── done in 1m 20s ─╮
    │ [receipt]                                                                                    │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 101 · 40 KiB output                                                                     │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell apply_patch crates/nanocodex-managed/src/lib.rs ──────────────── done in 1m 40s ─╮
    │ [receipt]                                                                                    │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 0                                                                                       │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell cargo test -p nanocodex-managed ──────────────────────────────── done in 7m 10s ─╮
    │ [receipt]                                                                                    │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 0 · 38 KiB output                                                                       │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯

▲ epoch 1 · cursor 8 · 2 open · last event 20s ago · heartbeat 1s ago · context 93%
~~~
~~~text
stale-summary · step 2/2: summary refreshed   [n]ext [j/k] scroll [q]uit
╭─ STATUS Status Fix the flaky test ────────────────────────────────────────────────── next steps ─╮
│ Done                                                                                             │
│   [verified] cargo test -p nanocodex-managed passes                                              │
│     evidence: a1.c3 exit 0                                                                       │
│   [agent-reported] root cause: nondeterministic ordering                                         │
│     evidence: a1 summary                                                                         │
│ Next                                                                                             │
│   [unsourced] push branch and open draft PR                                                      │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ linux x86_64                                                                                     │
│ CPU 21% · RAM 68 GB free · disk 120 GB free · observed 5s ago                                    │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Fix the flaky test ────────────────────────────────────────────────── running 16m 0s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT fixer ──────────────────────────────────────────────────────────────── done in 15m 40s ─╮
  │ role: tests                                                                                    │
  │ Fixed flaky ordering; cargo test passes                                                        │
  │ summary (agent-reported) current at cursor 10                                                  │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell cargo test -p nanocodex-managed ──────────────────────────────── done in 1m 20s ─╮
    │ [receipt]                                                                                    │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 101 · 40 KiB output                                                                     │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell apply_patch crates/nanocodex-managed/src/lib.rs ──────────────── done in 1m 40s ─╮
    │ [receipt]                                                                                    │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 0                                                                                       │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell cargo test -p nanocodex-managed ──────────────────────────────── done in 7m 10s ─╮
    │ [receipt]                                                                                    │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 0 · 38 KiB output                                                                       │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯

▲ epoch 1 · cursor 12 · 1 open · last event 2s ago · heartbeat 1m 1s ago · context 93%
~~~

### Orphaned tool calls

The turn completes while a Code Mode cell and its nested call never reported results. A late "running" replay for the nested call is rejected:
~~~text
orphaned-tool · step 1/2: code mode cell with nested call running   [n]ext [j/k] scroll [q]uit
╭─ TURN Turn Summarize the logs ───────────────────────────────────────────────────── running 20s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode const logs = await tools.read_file({path: 'app.log'}) ─────────── running 18s ─╮
  │ exec                                                                                           │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL read_file app.log ─────────────────────────────────────────────────────── running 17s ─╮
    │ read_file                                                                                    │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell wc -l app.log ──────────────────────────────────────────────────────── done in 1s ─╮
  │ [receipt]                                                                                      │
  │ exec_command                                                                                   │
  │ exit 0                                                                                         │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 4 · 3 open · last event 16s ago
~~~
~~~text
orphaned-tool · step 2/2: turn completes without the tool results   [n]ext [j/k] scroll [q]uit
╭─ TURN Turn Summarize the logs ───────────────────────────────────────────────────── done in 30s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode const logs = await tools.read_file({path: 'app.log'}) ─────────────── unknown ─╮
  │ [outcome unknown]                                                                              │
  │ owner ended without a tool result                                                              │
  │ exec                                                                                           │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL read_file app.log ─────────────────────────────────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                            │
    │ owner ended without a tool result                                                            │
    │ read_file                                                                                    │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell wc -l app.log ──────────────────────────────────────────────────────── done in 1s ─╮
  │ [receipt]                                                                                      │
  │ exec_command                                                                                   │
  │ exit 0                                                                                         │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 6 · 0 open · last event 1s ago
~~~

### Control frames and attempts

The five frames show:

1. A first-seen epoch 7 is a baseline, so no restart is counted.
2. A heartbeat at cursor 40 does not drop a delayed completion at cursor 5.
3. The dropped stream shows in the footer.
4. After epoch 8 the turn resumes as attempt 2.
5. A yielded Code Mode cell leaves its nested shell detached (still running), not unknown, and a Hand observation does not refresh the last-event age.
~~~text
control-frames · step 1/5: first observed epoch 7 is a baseline, not a restart   [n]ext [j/k] scroll
╭─ TURN Turn Run the suite ─────────────────────────────────────────────────────────── running 3s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ──────────────────────────────────────────────────────────── running 2s ─╮
  │ exec_command                                                                                   │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 7 · cursor 4 · 2 open · last event 2s ago · heartbeat 4s ago · context 12%
~~~
~~~text
control-frames · step 2/5: heartbeat high-water does not drop a delayed completion   [n]ext [j/k] sc
╭─ TURN Turn Run the suite ────────────────────────────────────────────────────────── running 10s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ──────────────────────────────────────────────────────────── done in 8s ─╮
  │ [receipt]                                                                                      │
  │ exec_command                                                                                   │
  │ exit 0                                                                                         │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 7 · cursor 5 · 1 open · last event 1s ago · heartbeat 2s ago · context 12%
~~~
~~~text
control-frames · step 3/5: stream dropped   [n]ext [j/k] scroll [q]uit
╭─ TURN Turn Run the suite ────────────────────────────────────────────────────────── running 43s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ──────────────────────────────────────────────────────────── done in 8s ─╮
  │ [receipt]                                                                                      │
  │ exec_command                                                                                   │
  │ exit 0                                                                                         │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

▲ disconnected 30s · epoch 7 · cursor 5 · 1 open · last event 34s ago · heartbeat 35s ago · context…
~~~
~~~text
control-frames · step 4/5: reconnected after a restart: the turn resumes as attempt 2   [n]ext [j/k]
╭─ TURN Turn Run the suite ────────────────────────────────────────────────────────── running 48s ─╮
│ [attempt 2]                                                                                      │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ──────────────────────────────────────────────────────────── done in 8s ─╮
  │ [receipt]                                                                                      │
  │ exec_command                                                                                   │
  │ exit 0                                                                                         │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 8 · cursor 7 · 1 open · last event 1s ago · heartbeat 40s ago · context 12% · 1 restart(s)
~~~
~~~text
control-frames · step 5/5: cell yields: nested call detached; hand observation is not an event   [n]
╭─ HAND paradigm ───────────────────────────────────────────────────────────────────────── online ─╮
│ CPU 9% · observed 1s ago                                                                         │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn Run the suite ─────────────────────────────────────────────────────── running 1m 18s ─╮
│ [attempt 2]                                                                                      │
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm test ──────────────────────────────────────────────────────────── done in 8s ─╮
  │ [receipt]                                                                                      │
  │ exec_command                                                                                   │
  │ exit 0                                                                                         │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode await tools.exec_command({cmd: 'sleep 600'}) ───────────────────── done in 6s ─╮
  │ [receipt]                                                                                      │
  │ exec                                                                                           │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell sleep 600 ───────────────────────────────────────────────────────── running 27s ─╮
    │ [detached]                                                                                   │
    │ exec_command                                                                                 │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 8 · cursor 11 · 2 open · last event 22s ago · heartbeat 1m 10s ago · context 12% · 1 restar…
~~~

### Producer streams and snapshot authority

A child stream with its own cursor space (cursor 3 after the main stream reached 40) is applied, not dropped as a replay; its duplicate is ignored. The turn's end settles a yielded cell's nested call locally; a later producer snapshot that still lists the call reopens it as running and detached:
~~~text
streams · step 1/3: main stream at cursor 40, child stream at cursor 3   [n]ext [j/k] scroll [q]uit
╭─ TURN Turn Serve and test ────────────────────────────────────────────────────────── running 6s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode await tools.exec_command({cmd: 'pnpm dev'}) ────────────────────── running 5s ─╮
  │ exec                                                                                           │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell pnpm dev ─────────────────────────────────────────────────────────── running 4s ─╮
    │ exec_command                                                                                 │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Worker 1 ────────────────────────────────────────────────────────────────── running 3s ─╮
  │ role: Worker 1                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 40 · 4 open · last event 3s ago · 1 replay(s) ignored
~~~
~~~text
streams · step 2/3: cell yields, then the turn ends: the nested call is settled locally   [n]ext [j/
╭─ TURN Turn Serve and test ───────────────────────────────────────────────────────── done in 11s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode await tools.exec_command({cmd: 'pnpm dev'}) ────────────────────── done in 7s ─╮
  │ [receipt]                                                                                      │
  │ exec                                                                                           │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell pnpm dev ────────────────────────────────────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                            │
    │ owner ended without a tool result                                                            │
    │ exec_command                                                                                 │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Worker 1 ────────────────────────────────────────────────────────────────── running 9s ─╮
  │ role: Worker 1                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 42 · 1 open · last event 1s ago · 1 replay(s) ignored
~~~
~~~text
streams · step 3/3: producer snapshot still lists the call: reopened and detached   [n]ext [j/k] scr
╭─ TURN Turn Serve and test ───────────────────────────────────────────────────────── done in 11s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode await tools.exec_command({cmd: 'pnpm dev'}) ────────────────────── done in 7s ─╮
  │ [receipt]                                                                                      │
  │ exec                                                                                           │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell pnpm dev ────────────────────────────────────────────────────────── running 13s ─╮
    │ [detached]                                                                                   │
    │ exec_command                                                                                 │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Worker 1 ───────────────────────────────────────────────────────────────── running 12s ─╮
  │ role: Worker 1                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 43 · 2 open · last event 1s ago · 1 replay(s) ignored
~~~

## 3. Web render

`node js/nanocodex-surface-state/scripts/render-fixtures.mjs <out> --chrome <chrome-headless-shell>`
reduces every fixture step with the JavaScript mirror. For each step it writes
standalone HTML (`renderCardStreamHtml` plus `styles.css`) and a headless-Chromium
PNG: 22 of each in this run, kept in ignored `output/surface-state-web/` rather
than committed. The parity test also asserts that every projected card id appears
in the rendered HTML.

## 4. Real producer output through the shared adapter

`crates/nanocodex-surface-state/tests/managed_producer.rs` reduces history pages and
`GET /surface` bodies captured by the managed surface-state curl recovery journey
(protocol branch, run `2026-10-12T20-59-29.567Z-675b6ed4`). The captures are copied
into `fixtures/managed/` with their goldens. Frames below are
`card_stream cards <golden> --width 110 --height 34`.

### Root reconnect across a runtime restart

History up to cursor 10, then the real post-restart snapshot (epoch 1, cursor 13,
no open runs or calls), then only the deltas after the snapshot cursor. The
runtime re-dispatches the cell under the same call ids, and that renders as
separate attempt cards:

frame 1: connected: cell and nested shell running (110x34)
~~~text
╭─ TURN Turn SURFACE_ROOT: apply S1 once. ────────────────────────────────────────────────────── running 0s ─╮
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"curl -s -X POST https://effects.exampl…  running 0s ─╮
  │ exec                                                                                                     │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell curl -s -X POST https://effects.example/effect/S1 ──────────────────────────── running 0s ─╮
    │ exec_command                                                                                           │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 3 · 3 open · last event 0s ago
~~~

frame 2: reconnect: surface snapshot at epoch 1 (110x34)
~~~text
╭─ TURN Turn SURFACE_ROOT: apply S1 once. ────────────────────────────────────────────────────── running 1s ─╮
│ [attempt 2]                                                                                                │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"curl -s -X POST https://effects.example/e…  unknown ─╮
  │ [outcome unknown]                                                                                        │
  │ runtime restarted while disconnected                                                                     │
  │ exec                                                                                                     │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell curl -s -X POST https://effects.example/effect/S1 ─────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime restarted while disconnected                                                                   │
    │ exec_command                                                                                           │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 4 · 1 open · last event 0s ago · 1 restart(s)
~~~

frame 3: deltas after the snapshot cursor (110x34)
~~~text
╭─ TURN Turn SURFACE_ROOT: apply S1 once. ────────────────────────────────────────────────────── done in 2s ─╮
│ [attempt 2]                                                                                                │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"curl -s -X POST https://effects.example/e…  unknown ─╮
  │ [outcome unknown]                                                                                        │
  │ runtime restarted while disconnected                                                                     │
  │ exec                                                                                                     │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell curl -s -X POST https://effects.example/effect/S1 ─────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime restarted while disconnected                                                                   │
    │ exec_command                                                                                           │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"curl -s -X POST https://effects.example/ef…  failed ─╮
  │ [receipt]                                                                                                │
  │ attempt 2: re-dispatched under the same call id                                                          │
  │ exec                                                                                                     │
  │ 358 B output                                                                                             │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell curl -s -X POST https://effects.example/effect/S1 ─────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime reported the outcome as unknown                                                                │
    │ exec_command                                                                                           │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 1 · cursor 10 · 0 open · last event 0s ago · 1 restart(s)
~~~

The recovered attempt's nested effect reports `outcome: unknown` (`CODE_MODE_CALL_INTERRUPTED`), so it stays `unknown`. The cell attempt itself has a failed receipt.

### Child swarm from snapshots alone

A surface attaching mid-swarm builds the tree from the snapshot. The post-restart snapshot then closes the lost calls and shows the child's new work:

frame 1: attach: snapshot during the swarm (110x34)
~~~text
╭─ TURN Turn ─────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Surface child ─────────────────────────────────────────────────────────────────────── running 0s ─╮
  │ role: Surface child                                                                                      │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode ────────────────────────────────────────────────────────────────────────── running 0s ─╮
    │ exec                                                                                                   │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell ──────────────────────────────────────────────────────────────────────────── running 0s ─╮
      │ exec_command                                                                                         │
      ╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode ──────────────────────────────────────────────────────────────────────────── running 0s ─╮
  │ exec                                                                                                     │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Wait for agent ───────────────────────────────────────────────────────────────────── running 0s ─╮
    │ wait_agent                                                                                             │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 1 · 6 open · last event 0s ago
~~~

frame 2: snapshot after restart: child moved on (110x34)
~~~text
╭─ TURN Turn ────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
│ [outcome unknown]                                                                                          │
│ runtime restarted while disconnected                                                                       │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Surface child ─────────────────────────────────────────────────────────────────────── running 0s ─╮
  │ [attempt 2]                                                                                              │
  │ role: Surface child                                                                                      │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode ───────────────────────────────────────────────────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime restarted while disconnected                                                                   │
    │ exec                                                                                                   │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell ─────────────────────────────────────────────────────────────────────────────── unknown ─╮
      │ [outcome unknown]                                                                                    │
      │ runtime restarted while disconnected                                                                 │
      │ exec_command                                                                                         │
      ╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode ────────────────────────────────────────────────────────────────────────── running 0s ─╮
    │ exec                                                                                                   │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell ──────────────────────────────────────────────────────────────────────────── running 0s ─╮
      │ exec_command                                                                                         │
      ╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode ─────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                        │
  │ runtime restarted while disconnected                                                                     │
  │ exec                                                                                                     │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Wait for agent ──────────────────────────────────────────────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime restarted while disconnected                                                                   │
    │ wait_agent                                                                                             │
● epoch 1 · cursor 2 · 3 open · last event 0s ago · 1 restart(s)
~~~

### Full captured child history (no snapshot)

The child's card takes its role (`Surface child`) from the real `spawn_agent` receipt:

frame 1: full child history (110x34)
~~~text
╭─ TURN Turn SURFACE_SPAWN: delegate S2 to one child. ────────────────────────────────────────── done in 2s ─╮
│ [attempt 2]                                                                                                │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode text(await tools.spawn_agent({"role":"Surface child","task":"SURFACE_CHILD…  done in 0s ─╮
  │ [receipt]                                                                                                │
  │ exec                                                                                                     │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Spawn agent ──────────────────────────────────────────────────────────────────────── done in 0s ─╮
    │ [receipt]                                                                                              │
    │ spawn_agent                                                                                            │
    │ 66 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Surface child SURFACE_CHILD_TASK: apply effect S2 once, then submit_result. ───────── running 0s ─╮
  │ [attempt 2]                                                                                              │
  │ role: Surface child                                                                                      │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"curl -s -X POST https://effects.example…  unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime restarted                                                                                      │
    │ exec                                                                                                   │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell curl -s -X POST https://effects.example/effect/S2 ───────────────────────────── unknown ─╮
      │ [outcome unknown]                                                                                    │
      │ runtime restarted                                                                                    │
      │ exec_command                                                                                         │
      ╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"curl -s -X POST https://effects.exam…  done in 0s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell curl -s -X POST https://effects.example/effect/S3 ────────────────────────── done in 0s ─╮
      │ [receipt]                                                                                            │
      │ exec_command                                                                                         │
● epoch 1 · cursor 32 · 1 open · last event 0s ago · 1 restart(s)
~~~

### Real hosted-stack swarm: 17 children, six restarts

The history and `/surface` body were captured from the reviewer's real Managed API stack (protocol workerd harness, scripted model) and trimmed to lifecycle events in `fixtures/managed/swarm-*.json`. Children whose runs ended without an accepted result are reported `failed` by `wait_agent` (cursor 332). Their cards say failed, titled by role, with the task as subject. Children with no reported outcome yet show `unknown · run ended; assignment outcome not reported yet` instead of done. Excerpt of the history replay:
~~~text
  ╭─ AGENT Worker 1 CHILD 1 40: run your shell then submit_result. ───────────────────────────────── failed ─╮
  │ subagent turn ended without a valid submit_result call Inspect child evidence before assigning recovery. │
  │ A plain final answer is not an accepted result; do not…                                                  │
  │ role: Worker 1                                                                                           │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"echo child 1; sleep 40; echo child …  done in 11s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    │ 98 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell echo child 1; sleep 40; echo child 1 done ───────────────────────────────────── unknown ─╮
      │ [outcome unknown]                                                                                    │
      │ runtime restarted                                                                                    │
      │ exec_command                                                                                         │
      ╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.submit_result({output:"child 1 finished"})); ─────────── done in 11s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    │ 98 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Worker 2 CHILD 2 43: run your shell then submit_result. ───────────────────────────────── failed ─╮
  │ subagent turn ended without a valid submit_result call Inspect child evidence before assigning recovery. │
  │ A plain final answer is not an accepted result; do not…                                                  │
  │ role: Worker 2                                                                                           │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"echo child 2; sleep 43; echo child …  done in 11s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    │ 98 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell echo child 2; sleep 43; echo child 2 done ───────────────────────────────────── unknown ─╮
      │ [outcome unknown]                                                                                    │
      │ runtime restarted                                                                                    │
      │ exec_command                                                                                         │
      ╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.submit_result({output:"child 2 finished"})); ─────────── done in 11s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    │ 99 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
~~~

After the service snapshot (all 17 `agents[].state = failed`):
~~~text
  ╭─ AGENT Worker 1 CHILD 1 60: run your shell then submit_result. ───────────────────────────────── failed ─╮
  │ role: Worker 1                                                                                           │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"echo child 1; sleep 60; echo child …  done in 11s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    │ 98 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell echo child 1; sleep 60; echo child 1 done ───────────────────────────────────── unknown ─╮
      │ [outcome unknown]                                                                                    │
      │ runtime restarted                                                                                    │
      │ exec_command                                                                                         │
      ╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.submit_result({output:"child 1 finished"})); ─────────── done in 11s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    │ 99 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ AGENT Worker 2 CHILD 2 63: run your shell then submit_result. ───────────────────────────────── failed ─╮
  │ role: Worker 2                                                                                           │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"echo child 2; sleep 63; echo child …  done in 11s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    │ 98 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
      ╭─ TOOL Shell echo child 2; sleep 63; echo child 2 done ───────────────────────────────────── unknown ─╮
      │ [outcome unknown]                                                                                    │
      │ runtime restarted                                                                                    │
      │ exec_command                                                                                         │
      ╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Code Mode text(await tools.submit_result({output:"child 2 finished"})); ─────────── done in 11s ─╮
    │ [receipt]                                                                                              │
    │ exec                                                                                                   │
    │ 99 B output                                                                                            │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
~~~
~~~text
● epoch 6 · cursor 266 · 0 open · last event 0s ago · 6 restart(s)
~~~

### Producer attempt ids (real hosted stack, attempt-aware protocol)

The reviewer's capture after the protocol added `payload.attempt` (`fixtures/managed/attempts-*.json`). The shell killed mid-run keeps attempt 1 as `unknown · runtime restarted`, and the producer's `attempt: 2` replay is its own card. The epoch-2 snapshot reports the workers' state from epoch 1, so they read unknown:
~~~text
╭─ TURN Turn SLEEP 605 ───────────────────────────────────────────────────────────────────────── done in 5s ─╮
│ [attempt 2]                                                                                                │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"echo starting; sleep 605; echo finished",…  unknown ─╮
  │ [outcome unknown]                                                                                        │
  │ runtime restarted                                                                                        │
  │ exec                                                                                                     │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell echo starting; sleep 605; echo finished ───────────────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime restarted                                                                                      │
    │ exec_command                                                                                           │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Code Mode text(await tools.exec_command({cmd:"echo starting; sleep 605; echo finished", …  failed ─╮
  │ [receipt]                                                                                                │
  │ attempt 2: re-dispatched under the same call id                                                          │
  │ exec                                                                                                     │
  │ 296 B output                                                                                             │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell echo starting; sleep 605; echo finished ───────────────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime reported the outcome as unknown                                                                │
    │ exec_command                                                                                           │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn SWARM 4 30 ──────────────────────────────────────────────────────────────────────── done in 5s ─╮
~~~

### Process sessions and runtime generation

A shell whose call returns while its process keeps running reports `session_id`. The process becomes its own card under the call, and a snapshot listing it in `process_sessions` keeps it open. A snapshot with a newer `runtime_generation` makes it unknown (the retained process may belong to the lost runtime), and a later exit code settles it. This journey uses the documented wire shape; no capture with a live process session exists yet:

frame 1: call returned; process keeps running (110x34)
~~~text
╭─ TURN Turn start the dev server ────────────────────────────────────────────────────────────── running 1s ─╮
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm dev ─────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [receipt]                                                                                                │
  │ exec_command                                                                                             │
  │ 5 B output                                                                                               │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ PROC Process 7 pnpm dev ───────────────────────────────────────────────────────────────── running 1s ─╮
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 4 · 2 open · last event 1s ago
~~~

frame 2: snapshot lists the process (generation 3) (110x34)
~~~text
╭─ TURN Turn start the dev server ────────────────────────────────────────────────────────────── running 3s ─╮
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm dev ─────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [receipt]                                                                                                │
  │ exec_command                                                                                             │
  │ 5 B output                                                                                               │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ PROC Process 7 pnpm dev ──────────────────────────────────────────────────────────────────── running ─╮
    │ on hand-a                                                                                              │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 5 · 2 open · last event 0s ago
~~~

frame 3: runtime generation 4: retained process unverified (110x34)
~~~text
╭─ TURN Turn start the dev server ────────────────────────────────────────────────────────────── running 9s ─╮
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm dev ─────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [receipt]                                                                                                │
  │ exec_command                                                                                             │
  │ 5 B output                                                                                               │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ PROC Process 7 pnpm dev ──────────────────────────────────────────────────────────────────── unknown ─╮
    │ [outcome unknown]                                                                                      │
    │ runtime replaced (generation 4); process may belong to the lost runtime                                │
    │ on hand-a                                                                                              │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 6 · 1 open · last event 0s ago
~~~

frame 4: write_stdin reports exit 0 (110x34)
~~~text
╭─ TURN Turn start the dev server ───────────────────────────────────────────────────────────── running 10s ─╮
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell pnpm dev ─────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [receipt]                                                                                                │
  │ exec_command                                                                                             │
  │ 5 B output                                                                                               │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ PROC Process 7 pnpm dev ───────────────────────────────────────────────────────────────── done in 0s ─╮
    │ [receipt]                                                                                              │
    │ exit 0                                                                                                 │
    ╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Process input ──────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [receipt]                                                                                                │
  │ write_stdin                                                                                              │
  │ exit 0                                                                                                   │
  ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 9 · 1 open · last event 0s ago
~~~

## Adopting the shared state from Mission Control (variant A)

`tui/card_stream.rs` owns the TUI's single `CardStreamView`: one shared `SurfaceState`
plus the shared `AgentEventAdapter`. It is fed by `observe(&TranscriptRecord)`,
`observe_subagent(&AgentUpdate)`, `apply_snapshot(&SurfaceSnapshot)` and
`connection(bool)`, and read through `projection(now_ms)`.

Variant A's `surface.rs` maps onto it one to one:

| Variant A `surface.rs` | Shared `CardStreamView` |
|---|---|
| `SurfaceState::snapshot` | `apply_snapshot` |
| `observe` / `subagent` | `observe` / `observe_subagent` |
| `footer` | `projection.liveness.label` |
| `row_count` / `row_scope` | `projection.cards` (ids `turn:<id>`, `agent:<n>`, `tool:<owner>:<call>[#attempt]`) |
| `panel` | renders those cards: pinned Status first, then the task tree by `depth` |

## Known gaps

- Hand, Decision and Status cards are modeled, reduced, projected and rendered
  (fixtures above), but the managed stream carries no Hand resources, decisions
  or Status items yet. The live TUI shows Turn, Agent, Tool and Workspace cards.
- The TUI reads the snapshot at attach, at Resume, on Enter-to-reconnect, and
  after a live `runtime_epoch`. A silent transport reconnect inside the
  WebSocket driver resumes from its cursor; if a restart happened in the gap, the
  replay carries the `runtime_epoch` that triggers the re-read. An idle restart
  (new `runtime_generation`, no epoch) is only seen at the next snapshot read,
  because the driver does not surface the `ready.surface` generation.
- The snapshot lists open entities only, so work that finished during a gap is
  known only from the replayed history. The TUI replays the gap before applying
  the snapshot.
- A child whose run ends with no lifecycle report reads `unknown` until a
  `wait_agent`/`list_agents` receipt or the snapshot reports its state.
- The reducer fences each producer stream separately (`stream` on deltas). The
  TUI adapter still numbers events locally, because one managed history shares
  one cursor space.
- A yielded cell's nested call shows `detached`. If its turn ends first, it
  settles locally to `unknown` until a producer snapshot lists it again, which
  reopens it. Process sessions are linked to the call that reported
  `session_id`; no captured producer output with a live process session exists
  yet.
- Resumed turns and agents keep one card with an `attempt N` badge (same id,
  newer epoch). Re-dispatched tool calls get separate attempt cards.
- Tool calls first seen in a snapshot have no subject line, because snapshots
  carry no arguments.
- F7 state does not persist across re-attach.
- The JavaScript package mirrors the reducer and projection; the managed adapter
  is Rust-only, and no web app mounts the card stream yet.
