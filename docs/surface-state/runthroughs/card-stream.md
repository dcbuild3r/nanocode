# Runthrough: TUI variant B, card stream

Branch `feat/tui-card-stream`. One shared surface-state model feeds both the
terminal and the web:

- `crates/nanocodex-surface-state`: a pure reducer (`SurfaceEvent` to
  `SurfaceState`) and the card projection (`SurfaceState::project(now)`).
  Every label, badge and line is produced here. The shared `agent_events`
  adapter maps today's `run.*`/`tool.*` lifecycle pairs, durable turn
  terminals and the runtime epoch onto it.
- `js/nanocodex-surface-state`: the JavaScript mirror for web, with a
  DOM-free HTML renderer and stylesheet.
- `nanocodex-tui-render::card_stream`: the ratatui widget. In the TUI, **F7**
  swaps the transcript pane for the card stream (Esc or F7 returns, PgUp/PgDn
  scroll); the composer stays live.

Reducer guarantees: cursor fencing (replays are counted, never applied); sticky
terminal states; epoch settlement of in-flight turns/runs/agents/tools to
`unknown` ("runtime restarted"); snapshot authority (an open entity missing from
a reconnect snapshot becomes `unknown`, "closed while disconnected"); orphan
settlement (a tool call cannot outlive the turn, run, agent or Code Mode cell
that issued it). Liveness footer: epoch, cursor, open count, last-event and
heartbeat age, context percentage, restarts, ignored replays. Provenance badges:
`verified` (runtime receipt), `agent-reported`, `test-build`, and `unsourced`.

## Cross-surface parity

The six fixtures in `crates/nanocodex-surface-state/fixtures/*.json` are event
streams with clock steps; `*.cards.json` holds the golden projection. The Rust
test and the JavaScript test reduce the same events and must produce the same
golden cards, so the TUI and web cannot drift.

~~~sh
export CARGO_TARGET_DIR=/srv/nanocodex/w/surface-target-A
cargo test -p nanocodex-surface-state                         # 6 fixtures + wire round-trip
(cd js/nanocodex-surface-state && pnpm test)                  # 8 node tests + tsc type test
cargo test -p nanocodex-tui-render --test card_stream         # every fixture at 40/80/120 columns
cargo test -p nanocodex-bin --test nanocodex2_tui_lifecycle terminal_card_stream -- --nocapture
~~~

Mutation check: changing one badge string only in `index.mjs` made 2 parity
tests fail, so the comparison is effective.

## 1. Real TUI over a controlled managed service (PTY)

`terminal_card_stream_settles_tools_from_runtime_receipts` starts the built
`nanocodex` binary in a PTY against the test file's controlled managed service,
submits a prompt, streams `run.started` and two `tool.call`s (one completes),
presses F7, sends the protocol's restart receipt (an unknown-outcome
`tool.result` with `reason: runtime_restarted`), then the durable turn completion.
Frames are the `vt100` screen (160x32) printed by that run.

Tool running; the sibling completed with a verified receipt:
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpYp8hZW                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn gpt-card-stream ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release -p card-stream-journey ───────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
  │ exec_command in /srv/cards                                                                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell echo CARD_PROBE ────────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [verified]                                                                                                                                                 │
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

Restart receipt: the card settles to `unknown` instead of ticking:
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpYp8hZW                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn gpt-card-stream ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────── running 0s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release -p card-stream-journey ──────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ runtime restarted                                                                                                                                          │
  │ exec_command in /srv/cards                                                                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell echo CARD_PROBE ────────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [verified]                                                                                                                                                 │
  │ exec_command                                                                                                                                               │
  │ exit 0 · 10 B output                                                                                                                                       │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 6 · 1 open · last event 0s ago
╭─ 0%/272k Thinking… ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────── 0s  gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter steer · Tab queue · Esc Esc stop ────────────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

Durable turn completion:
~~~text
╭─ BRAIN Workspace ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── attached ─╮
│ /tmp/.tmpYp8hZW                                                                                                                                              │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭─ TURN Turn gpt-card-stream ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 0s ─╮
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell cargo build --release -p card-stream-journey ──────────────────────────────────────────────────────────────────────────────────────── unknown ─╮
  │ [outcome unknown]                                                                                                                                          │
  │ runtime restarted                                                                                                                                          │
  │ exec_command in /srv/cards                                                                                                                                 │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
  ╭─ TOOL Shell echo CARD_PROBE ────────────────────────────────────────────────────────────────────────────────────────────────────────────────── done in 0s ─╮
  │ [verified]                                                                                                                                                 │
  │ exec_command                                                                                                                                               │
  │ exit 0 · 10 B output                                                                                                                                       │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 7 · 0 open · last event 0s ago
╭─ 0%/272k ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── gpt-6-astra  low ─╮
│                                                                                                                                                              │
│                                                                                                                                                              │
│                                                                                                                                                              │
╰─ Enter send · / actions · @ paths · @@ sessions ────────────────────────────────────────────────────────────────────────────────────────────────── ◉ dev  ~ ─╯
~~~

F7 then returns to the transcript (asserted: card text disappears, `Enter send` remains).

## 2. Fixture journeys in a real terminal (tmux)

`output/runthrough/tmux-fixtures.sh` (ignored) runs the
`nanocodex-tui-render` example `card_stream live <fixture>` in a 100x48 tmux
session, presses `n` per step and saves `tmux capture-pane -p`. The first line
is the example's chrome.

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

● epoch 1 · cursor 4 · 2 open · last event 12s ago · heartbeat 1s ago · context 18%
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
  │ [verified]                                                                                     │
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
● epoch 1 · cursor 19 · 4 open · last event 1m 20s ago · heartbeat 2s ago · context 71%
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

● epoch 2 · cursor 8 · 1 open · last event 20s ago · heartbeat 2s ago · context 40% · 1 restart(s)
~~~

### Reconnect

Connected, then three minutes without events (liveness turns to warn):
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

● epoch 1 · cursor 11 · 3 open · last event 10s ago · heartbeat 1s ago
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

▲ epoch 1 · cursor 11 · 3 open · last event 3m 10s ago · heartbeat 3m 1s ago
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
  │ [verified] [outcome reconciled]                                                                │
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

The agent summary predates three newer changes in its subtree; Hand resources are 12 minutes old; context is 93%. Then both are refreshed:
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
    │ [verified]                                                                                   │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 101 · 40 KiB output                                                                     │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell apply_patch crates/nanocodex-managed/src/lib.rs ──────────────── done in 1m 40s ─╮
    │ [verified]                                                                                   │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 0                                                                                       │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell cargo test -p nanocodex-managed ──────────────────────────────── done in 7m 10s ─╮
    │ [verified]                                                                                   │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 0 · 38 KiB output                                                                       │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯

▲ epoch 1 · cursor 9 · 2 open · last event 20s ago · heartbeat 1s ago · context 93%
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
    │ [verified]                                                                                   │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 101 · 40 KiB output                                                                     │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell apply_patch crates/nanocodex-managed/src/lib.rs ──────────────── done in 1m 40s ─╮
    │ [verified]                                                                                   │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 0                                                                                       │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯
    ╭─ TOOL Shell cargo test -p nanocodex-managed ──────────────────────────────── done in 7m 10s ─╮
    │ [verified]                                                                                   │
    │ exec_command in /srv/repo on paradigm                                                        │
    │ exit 0 · 38 KiB output                                                                       │
    ╰──────────────────────────────────────────────────────────────────────────────────────────────╯

▲ epoch 1 · cursor 12 · 1 open · last event 2s ago · heartbeat 1m 1s ago · context 93%
~~~

### Orphaned tool calls

The turn completes while a Code Mode cell and its nested call never reported; a late "running" replay for the nested call is rejected:
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
  │ [verified]                                                                                     │
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
  │ [verified]                                                                                     │
  │ exec_command                                                                                   │
  │ exit 0                                                                                         │
  ╰────────────────────────────────────────────────────────────────────────────────────────────────╯

● epoch 0 · cursor 6 · 0 open · last event 1s ago
~~~

## 3. Web render

`node js/nanocodex-surface-state/scripts/render-fixtures.mjs <out> --chrome <chrome-headless-shell>`
reduces every fixture step with the JavaScript mirror and writes standalone HTML
(`renderCardStreamHtml` plus `styles.css`) and a headless-Chromium PNG per step:
14 of each in this run, kept in ignored `output/surface-state-web/` rather than
committed. The parity test also asserts every projected card id appears in the
rendered HTML.

## Known gaps

- Hand, Decision and Status cards are modeled, reduced, projected and rendered
  (fixtures above), but today's agent event stream carries no Hand resources,
  decisions or Status items, so the live TUI shows Turn, Agent, Tool and
  Workspace cards until the runtime publishes those entities.
- The adapter maps per-event lifecycle. Reconnect still replays history; the
  protocol's surface snapshot route is not consumed yet.
- The JavaScript package mirrors the reducer and projection; the agent-event
  adapter is Rust-only so far, and no web app mounts the card stream yet.
