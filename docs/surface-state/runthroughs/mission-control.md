# Mission Control TUI variant

Draft PR #987: legacy / best-effort task projection alongside the existing
transcript. The task pane labels assignment states, not permanent Agent lifetime.
Tool receipts do not count as completed tasks. Root is first, active/unknown
assignments precede terminal assignments, and tools follow assignments. Loaded
and visible row counts are explicit. Root-turn changes preserve children and
retained processes. A one-second UI deadline refreshes event age without input;
it is not a network heartbeat.

## Controls and evidence

F6 focuses the pane. Arrows, page keys and Home/End navigate; Enter opens the
selected agent's existing transcript viewer. Root returns focus to root chat.
Esc/F6 exits. Narrow terminals use a full-content focused task pane. Context is
first in the footer so an overflow warning is not hidden behind stream text.

Evidence labels are deliberately scoped: `agent-reported`, `test-build ·
agent-reported`, `verified: receipt only`, and `runtime receipt · effect unknown`.
No receipt proves deployment, installation or an external effect. Age alone
never marks legitimate slow tools failed, stale or no-signal. The stream says
`observed (health unconfirmed)` or `unconfirmed/replay`; neither claims a live
transport based on a past event. The host's heartbeat/disconnect integration
remains incomplete and is not concealed by a green connected label.

## Reviewer fixes and actual local runs

Reviewer #987 comment 6132410380 reproduced lifecycle/liveness bugs on the older
commit. This revision addresses:

- Idle age freezing: one-second render deadline; no keypress needed.
- False connected/offline claims: explicit unconfirmed health.
- Root-turn replacement dropping children: preserve independently owned children.
- First observed dotted epoch retiring live work: baseline unless a previous
  epoch/recovery transition is evidenced. JSON string epoch quotes are stripped.
- Open Code Mode cells and process sessions retired by turn fallback: retained
  process/cell children are exempt; yielded results are not completed tasks.
- Task counts including tools, root sorted behind all children, silent clipping:
  task-only done/running counts, root-first attention ordering, shown/loaded counts.

Commands run from this worktree on Linux:

```sh
CARGO_TARGET_DIR=/srv/nanocodex/w/surface-target-B \
  cargo test -p nanocodex-bin --lib --features tui-bench nanocodex2::tui -- --nocapture
CARGO_TARGET_DIR=/srv/nanocodex/w/surface-target-B \
  cargo build -p nanocodex-bin --bin nanocodex
bash /srv/nanocodex/w/review-scratch/dogfood.sh mc-fix3 \
  /srv/nanocodex/w/surface-target-B/debug/nanocodex 140 40
```

Final consumer suite: 351 passed, 0 failed, 2 ignored. The native build passed.
The live liveness run used the real CLI/loopback managed-server in tmux and the
reviewer's scripted provider, with one synthetic normal turn. After SIGKILL of
only that harness server, captures without keyboard input showed ages 25s, 28s,
31s. The TUI did not independently detect the dead socket; it correctly continued
to label transport health unconfirmed. Full live frames and logs are retained
in the swarm's ignored `output/mission-live/`. The task-owned tmux session was
closed after capture. This live binary preceded the protocol-adapter merge;
protocol integration is verified separately below, not implied by this run.

## Published protocol integration

Merged producer `b94dfa6f75d3a8b853d3bf9a3faab8a5f61880ce`.
The actual wire marker is `runtime_epoch {epoch, reason, interrupted}`, not the
proposed `runtime.epoch`. The TUI projects that exact message through both live
and retained-history adapters. Only listed interrupted calls/runs are retired;
known terminal/newer observations are preserved. Legacy dotted markers remain
supported but a first baseline marker does not imply a restart.

The client negotiates GET `/surface`, takes its opaque `cursor`, and opens the
published `surface_events` (`?surface=1`) stream after that cursor. Only epoch
markers are forwarded alongside the existing websocket data, avoiding duplicate
ordinary events. Unsupported legacy servers simply keep legacy behavior. The
task exits when its receiver closes. This side stream currently stops on an
unrecoverable stream error; it is not a substitute for lifecycle heartbeat health.

The typed snapshot adapter consumes `protocol`, `epoch`, `cursor`, `generated_at`,
`runs`, `agents`, and `tool_calls`; the consumer fixture hydrates cursor 10 then
applies later real deltas. **Full live panel snapshot replacement, process/turn
resource hydration, and snapshot/replay race equivalence are not shipped here.**
The live side stream uses the snapshot cursor, while the panel still uses existing
history and child events. This is partial integration, not `surface_state_v1`
conformance or an attempt/revision ledger.

`bin/nanocodex/fixtures/surface-state/README.md` records exact producer capture
provenance. The test replays actual curl output from agent 2's workerd recovery
journey, not invented wire data. The generated `real-protocol.txt` shows epoch 1
and two unknown interruption receipts. It is a replay of that evidence, not a
claim that this variant independently killed a deployed Worker.

## Remaining integration boundaries

No canonical attempt/revision reconciliation, evidence-backed Status revisions,
dependency-aware stale summaries, independently verified effects, native Hand
resource health, or complete snapshot/delta race test. Legacy later receipts may
refine unknown in place. The producer can still replay effects on restart (see
reviewer's upstream report); this TUI cannot make that execution policy safe.
Navigation currently follows row indices; live insertion can move the selected
entity. This branch is a reviewable alternative, not merge-ready certification.

## Captured terminal excerpts

Actual deterministic production-renderer captures follow. These are right-pane
crops plus footer and unknown transcript lines, with blank rows omitted. The
selected-agent frame preserves full-width nonblank rows. Complete frames are
ignored under `output/mission-control/`. The harness is shared with
`benches/nanocodex2_tui.rs`; it rejects rendering errors and checks stable frames.

### normal

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 0                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│1 shown / 1 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
└────────────────────────────────────┘
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### swarm

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 7 [running]                  │
│  claude / claude-opus-4-6          │
│  run.started · 99s                 │
│  agent-reported                    │
│root/shell-1 [running]              │
│  family ? / model ?                │
│  exec_command · 99s                │
│  agent-reported                    │
└────────────────────────────────────┘
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### restart

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 0                      │
│Blocked/unknown: 3                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [unknown]                      │
│  codex / gpt-6-astra               │
│  runtime restarted; outcome unknown│
│· 99s                               │
│  agent-reported                    │
│root → 7 [unknown]                  │
│  claude / claude-opus-4-6          │
│  runtime restarted; outcome unknown│
│· 99s                               │
│  agent-reported                    │
│root/shell-1 [unknown]              │
│  family ? / model ?                │
│  runtime restarted; outcome unknown│
│· 99s                               │
│  agent-reported                    │
└────────────────────────────────────┘
    └── ? Shell   … · completion unknown · outcome unknown · tool call ended without a terminal result
context ? | event 99s ago | epoch 2 | stream unconfirmed/replay
```

### reconnect

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 2                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 7 [unknown]                  │
│  claude / claude-opus-4-6          │
│  runtime restarted; outcome unknown│
│· 99s                               │
│  agent-reported                    │
│root/shell-1 [unknown]              │
│  family ? / model ?                │
│  runtime restarted; outcome unknown│
│· 99s                               │
│  agent-reported                    │
└────────────────────────────────────┘
    └── ? Shell   … · completion unknown · outcome unknown · tool call ended without a terminal result
context ? | event 99s ago | epoch 2 | stream unconfirmed/replay
```

### stale-summary

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 1                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 7 [completed]                │
│  claude / claude-opus-4-6          │
│  run.completed · 99s               │
│  agent-reported                    │
│root/shell-1 [unknown]              │
│  family ? / model ?                │
│  runtime restarted; outcome unknown│
│· 99s                               │
│  agent-reported                    │
└────────────────────────────────────┘
    └── ? Shell   … · completion unknown · outcome unknown · tool call ended without a terminal result
context ? | event 99s ago | epoch 2 | stream unconfirmed/replay
```

### fallback

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 1                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│2 shown / 2 loaded · row 1          │
│root [running]                      │
│  family ? / model ?                │
│  run.started · 99s                 │
│  agent-reported                    │
│root/fallback-shell [unknown]       │
│  family ? / model ?                │
│  new run; no terminal receipt · 99s│
│  agent-reported                    │
└────────────────────────────────────┘
    └── ? Shell   … · completion unknown · outcome unknown · tool call ended without a terminal result
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### context-overflow

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 1                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 7 [completed]                │
│  claude / claude-opus-4-6          │
│  run.completed · 99s               │
│  agent-reported                    │
│root/shell-1 [unknown]              │
│  family ? / model ?                │
│  runtime restarted; outcome unknown│
│· 99s                               │
│  agent-reported                    │
└────────────────────────────────────┘
    └── ? Shell   … · completion unknown · outcome unknown · tool call ended without a terminal result
context 108% WARNING >100% | event 99s ago | epoch 2 | stream unconfirmed/replay
```

### tree-scrolled

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 14                     │
│Blocked/unknown: 0                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│5 shown / 15 loaded · row 15        │
│root → 16 [running]                 │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 17 [running]                 │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 18 [running]                 │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 19 [running]                 │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│> root/shell-1 [running]            │
│  family ? / model ?                │
│  exec_command · 99s                │
│  agent-reported                    │
└────────────────────────────────────┘
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### selected-agent

```text
╭────────────────────────────────── Agent 7 · claude-opus-4-6 · #7 ──────────────────────────────────╮┌ Mission Control ───────────────────┐
│                                                                                                    ││Status · legacy / best-effort       │
│                                                                                                    ││Done: 0 (not verified)              │
│                                                                                                    ││In progress: 14                     │
│                                                                                                    ││Blocked/unknown: 0                  │
│                                                                                                    ││Next: inspect task evidence         │
│                                                                                                    ││Task tree · assignment states       │
│                                                                                                    ││F6 tree · ↑↓/PgUp/PgDn · Enter      │
│                                                                                                    ││5 shown / 15 loaded · row 2         │
│                                                                                                    ││root [running]                      │
│                                                                                                    ││  codex / gpt-6-astra               │
│                                                                                                    ││  run.started · 99s                 │
│                                                                                                    ││  agent-reported                    │
│                                                                                                    ││root → 7 [running]                  │
│                                                                                                    ││  claude / claude-opus-4-6          │
│                                                                                                    ││  run.started · 99s                 │
│                                                                                                    ││  agent-reported                    │
│                                            ▓▓██  ██░░░░                                            ││root → 8 [running]                  │
│                                      ▒▒▓▓                ░░░░                                      ││  codex / gpt-6-astra               │
│                                    ▒▒                        ░░                                    ││  run.started · 99s                 │
│                                  ░░                            ░░                                  ││  agent-reported                    │
│                                  ░░         nanocodex          ░░                                  ││root → 9 [running]                  │
│                                  ░░                            ░░                                  ││  codex / gpt-6-astra               │
│                                    ░░                        ░░                                    ││  run.started · 99s                 │
│                                      ░░░░                ░░░░                                      ││  agent-reported                    │
│                                            ░░░░  ░░░░░░                                            ││root → 10 [running]                 │
│                                                                                                    ││  codex / gpt-6-astra               │
│                                                                                                    ││  run.started · 99s                 │
│                                                                                                    ││  agent-reported                    │
│                   pgup/pgdn scroll · ctrl+home/end · ctrl+o expand all · esc back                  ││                                    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯└────────────────────────────────────┘
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### test-build

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 7 [running]                  │
│  claude / claude-opus-4-6          │
│  run.started · 99s                 │
│  agent-reported                    │
│root/shell-1 [completed]            │
│  family ? / model ?                │
│  exec_command · 99s                │
│  test-build · agent-reported       │
└────────────────────────────────────┘
    └── × Shell  $ echo fixture · completion unknown · 0ms
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### verified-receipt

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 7 [running]                  │
│  claude / claude-opus-4-6          │
│  run.started · 99s                 │
│  agent-reported                    │
│root/shell-1 [completed]            │
│  family ? / model ?                │
│  exec_command · 99s                │
│  verified: receipt only            │
└────────────────────────────────────┘
    └── × Shell  $ echo fixture · completion unknown · fixture receipt · 0ms
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### epoch-baseline

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 7 [running]                  │
│  claude / claude-opus-4-6          │
│  run.started · 99s                 │
│  agent-reported                    │
│root/shell-1 [running]              │
│  family ? / model ?                │
│  exec_command · 99s                │
│  agent-reported                    │
└────────────────────────────────────┘
context ? | event 99s ago | epoch 7 | stream unconfirmed/replay
```

### children-survive

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│2 shown / 2 loaded · row 1          │
│root [running]                      │
│  family ? / model ?                │
│  run.started · 99s                 │
│  agent-reported                    │
│root → 7 [running]                  │
│  claude / claude-opus-4-6          │
│  run.started · 99s                 │
│  agent-reported                    │
└────────────────────────────────────┘
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### retained-process

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 0                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [completed]                    │
│  codex / gpt-6-astra               │
│  run.completed · 99s               │
│  agent-reported                    │
│root → 7 [running]                  │
│  claude / claude-opus-4-6          │
│  run.started · 99s                 │
│  agent-reported                    │
│root/shell-1 [running] · retained   │
│process                             │
│  family ? / model ?                │
│  exec_command · 99s                │
│  verified: receipt only            │
└────────────────────────────────────┘
    └── ◇ Shell  $ echo fixture · session 42 · completion unknown · 0ms
context ? | event 99s ago | epoch unreported | stream unconfirmed/replay
```

### real-protocol

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 0                      │
│Blocked/unknown: 3                  │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [unknown]                      │
│  family ? / gpt-6.1-sol            │
│  run.started · 0s                  │
│  agent-reported                    │
│root/surface-root-cell [unknown]    │
│  family ? / model ?                │
│  exec · 0s                         │
│  runtime receipt · effect unknown  │
│root/surface-root-cell/code-1       │
│[unknown]                           │
│  family ? / model ?                │
│  exec_command · 0s                 │
│  runtime receipt · effect unknown  │
└────────────────────────────────────┘
context ? | event 0s ago | epoch 1 | stream unconfirmed/replay
```

### Live idle/outage footer (no keyboard input)

```text
context 0% | event 25s ago | epoch unreported | stream observed (health unconfirmed)
context 0% | event 28s ago | epoch unreported | stream observed (health unconfirmed)
context 0% | event 31s ago | epoch unreported | stream observed (health unconfirmed)
```
