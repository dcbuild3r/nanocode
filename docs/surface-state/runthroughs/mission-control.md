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
agent-reported`, `receipt`, and `runtime receipt · effect unknown`.
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

Merged producer through `400830739` (PR #989); earlier live evidence below
identifies the exact producer revision it exercised.
The actual wire marker is `runtime_epoch {epoch, reason, interrupted}`, not the
proposed `runtime.epoch`. The TUI projects that exact message through both live
and retained-history adapters. Only listed interrupted calls/runs are retired;
known terminal/newer observations are preserved. Legacy dotted markers remain
supported but a first baseline marker does not imply a restart.

The ManagedClient now negotiates `/ws?surface=1`, receiving epoch markers
through the same retained-cursor websocket replay path as ordinary events. The
older snapshot-cursor SSE side task has been removed: a fresh snapshot cursor on
stateful reconnect could skip a recovery marker. Unsupported legacy servers
keep the legacy behavior and unconfirmed recovery label.

Tool cards in both projections now isolate `(agent_id, call_id, attempt)`;
missing attempt defaults to 1. The raw event journal keeps original wire IDs.
Snapshot tool calls and interrupted epoch tool references consume `attempt` too.
The `replay-attempts` capture demonstrates an unknown first attempt beside a
separate completed second attempt, rather than folding replay into the old card.

The typed snapshot adapter consumes `protocol`, `epoch`, `cursor`, `generated_at`,
`runs`, `agents`, and `tool_calls`; the consumer fixture hydrates cursor 10 then
applies later real deltas. **Full live panel snapshot replacement, process/turn
resource hydration, and snapshot/replay race equivalence are not shipped here.**
`ready.surface.runtime_generation` is now bound to retained process/cell observations
across reconnects; see the host-generation run below.
This is partial integration, not full `surface_state_v1` conformance.

Latest validation: 351 TUI tests passed (2 ignored), 63 managed-client tests
passed, native binary build passed. The host-generation revision also underwent
a fresh isolated real-workerd SIGKILL run, captured below.

`bin/nanocodex/fixtures/surface-state/README.md` records exact producer capture
provenance. The test replays actual curl output from agent 2's workerd recovery
journey, not invented wire data. The generated `real-protocol.txt` shows epoch 1
and two unknown interruption receipts. It is a replay of that evidence, not a
claim that this variant independently killed a deployed Worker.

## Remaining integration boundaries

Tool attempts are isolated; full Run/Agent revision reconciliation, evidence-backed Status revisions,
dependency-aware stale summaries, independently verified effects, native Hand
resource health, and complete snapshot/delta race tests remain incomplete. Legacy inferred uncertainty may be refined by later receipts. An explicit
producer `outcome_unknown` receipt or an exact epoch interruption stays terminal
for that attempt; a later
same-attempt success is ignored, while a new attempt gets its own card. The producer can still replay effects on restart (see
reviewer's upstream report); this TUI cannot make that execution policy safe.
Navigation currently follows row indices; live insertion can move the selected
entity. This branch is a reviewable alternative, not merge-ready certification.

## Retained-call follow-up after reviewer rounds 2/3

A yield is not evidence that a process remains alive. Retained cells/process
bindings now display `[retained] · liveness unconfirmed`, not `[running]`.
The header counts retained calls separately from running assignments. Their
last-event age remains an observation, not a live execution timer. A yielded
cell gets `yield receipt · completion unconfirmed`, never completion-verification
language. Only still-running children inherit a cell's retained observation;
completed children are not reopened.

Explicit epoch interruption/unknown receipts retire retained observations.
Completed assignments show an unresolved-call count when their background work
has an unknown receipt. With no epoch authority (for example legacy loopback
`/surface` 404), the footer says `recovery unobservable`. This cannot infer that
a process died merely because it vanished from a partial list or a socket broke;
without host-generation evidence, retained/unconfirmed is the honest state.
With a later observed generation or epoch, those bindings now settle unknown.

Reviewer agent 5 independently ran both variants on the protocol workerd stack
and reported epoch 1/2/3 arriving through Mission Control's SSE path. That is
reviewer evidence on PR #987, not a new live run by this agent. This follow-up
uses production-renderer fixture journeys for yielded children, late epoch
settlement and completed assignments with unresolved calls. The reviewer's
shared stack/session was not modified or killed.

## Captured terminal excerpts

Actual production-renderer captures follow. Right-pane crops plus footer and
unknown transcript lines are retained, blank rows omitted; selected-agent keeps
full-width nonblank rows. Full files are in ignored `output/mission-control/`.

### normal

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 0                  │
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│1 shown / 1 loaded · row 1          │
│root [running]                      │
│  codex / gpt-6-astra               │
│  run.started · 99s                 │
│  agent-reported                    │
└────────────────────────────────────┘
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### swarm

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Retained calls: 0 (unconfirmed)     │
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
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### restart

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 0                      │
│Blocked/unknown: 3                  │
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [unknown] · 1 unresolved calls │
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
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running] · 1 unresolved calls │
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
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running] · 1 unresolved calls │
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
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│2 shown / 2 loaded · row 1          │
│root [running] · 1 unresolved calls │
│  family ? / model ?                │
│  run.started · 99s                 │
│  agent-reported                    │
│root/fallback-shell [unknown]       │
│  family ? / model ?                │
│  new run; no terminal receipt · 99s│
│  agent-reported                    │
└────────────────────────────────────┘
    └── ? Shell   … · completion unknown · outcome unknown · tool call ended without a terminal result
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### context-overflow

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 1                  │
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running] · 1 unresolved calls │
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
│Retained calls: 0 (unconfirmed)     │
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
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### selected-agent

```text
╭────────────────────────────────── Agent 7 · claude-opus-4-6 · #7 ──────────────────────────────────╮┌ Mission Control ───────────────────┐
│                                                                                                    ││Status · legacy / best-effort       │
│                                                                                                    ││Done: 0 (not verified)              │
│                                                                                                    ││In progress: 14                     │
│                                                                                                    ││Blocked/unknown: 0                  │
│                                                                                                    ││Retained calls: 0 (unconfirmed)     │
│                                                                                                    ││Next: inspect task evidence         │
│                                                                                                    ││                                    │
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
│                                            ▓▓██  ██░░░░                                            ││  agent-reported                    │
│                                      ▒▒▓▓                ░░░░                                      ││root → 8 [running]                  │
│                                    ▒▒                        ░░                                    ││  codex / gpt-6-astra               │
│                                  ░░                            ░░                                  ││  run.started · 99s                 │
│                                  ░░         nanocodex          ░░                                  ││  agent-reported                    │
│                                  ░░                            ░░                                  ││root → 9 [running]                  │
│                                    ░░                        ░░                                    ││  codex / gpt-6-astra               │
│                                      ░░░░                ░░░░                                      ││  run.started · 99s                 │
│                                            ░░░░  ░░░░░░                                            ││  agent-reported                    │
│                                                                                                    ││root → 10 [running]                 │
│                                                                                                    ││  codex / gpt-6-astra               │
│                                                                                                    ││  run.started · 99s                 │
│                                                                                                    ││  agent-reported                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                                                                                                    ││                                    │
│                   pgup/pgdn scroll · ctrl+home/end · ctrl+o expand all · esc back                  ││                                    │
╰────────────────────────────────────────────────────────────────────────────────────────────────────╯└────────────────────────────────────┘
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### test-build

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Retained calls: 0 (unconfirmed)     │
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
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### verified-receipt

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Retained calls: 0 (unconfirmed)     │
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
│  receipt                           │
└────────────────────────────────────┘
    └── × Shell  $ echo fixture · completion unknown · fixture receipt · 0ms
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### epoch-baseline

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 2                      │
│Blocked/unknown: 0                  │
│Retained calls: 0 (unconfirmed)     │
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
│Retained calls: 0 (unconfirmed)     │
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
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### retained-process

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 0                  │
│Retained calls: 1 (unconfirmed)     │
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
│root/shell-1 [retained] · liveness  │
│unconfirmed                         │
│  family ? / model ?                │
│  exec_command · 99s                │
│  yield receipt · completion        │
│unconfirmed                         │
└────────────────────────────────────┘
    └── ◇ Shell  $ echo fixture · session 42 · completion unknown · 0ms
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### real-protocol

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 0                      │
│Blocked/unknown: 3                  │
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [unknown] · 2 unresolved calls │
│  family ? / gpt-6.1-sol            │
│  run.started · 0s                  │
│  agent-reported                    │
│root/surface-root-cell [unknown]    │
│  family ? / model ?                │
│  runtime restarted; reconcile      │
│receipt · 0s                        │
│  runtime receipt · effect unknown  │
│root/surface-root-cell/code-1       │
│[unknown]                           │
│  family ? / model ?                │
│  runtime restarted; reconcile      │
│receipt · 0s                        │
│  runtime receipt · effect unknown  │
└────────────────────────────────────┘
    └── ? Shell   … · completion unknown · outcome unknown · tool call ended without a terminal result
context ? | event 0s ago | epoch 1 | stream unconfirmed/replay
```

### yielded-unconfirmed

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 0                      │
│Blocked/unknown: 0                  │
│Retained calls: 2 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [completed]                    │
│  family ? / model ?                │
│  run.completed · 99s               │
│  agent-reported                    │
│root/cell [retained] · liveness     │
│unconfirmed                         │
│  family ? / model ?                │
│  exec · 99s                        │
│  yield receipt · completion        │
│unconfirmed                         │
│root/cell/code-1 [retained] ·       │
│liveness unconfirmed                │
│  family ? / model ?                │
│  exec_command · 99s                │
│  yielded · liveness unconfirmed    │
└────────────────────────────────────┘
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

### completed-with-lost-call

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 0                      │
│Blocked/unknown: 1                  │
│Retained calls: 1 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [completed] · 1 unresolved     │
│calls                               │
│  family ? / model ?                │
│  run.completed · 99s               │
│  agent-reported                    │
│root/cell/code-1 [unknown]          │
│  family ? / model ?                │
│  runtime restarted; reconcile      │
│receipt · 99s                       │
│  runtime receipt · effect unknown  │
│root/cell [retained] · liveness     │
│unconfirmed                         │
│  family ? / model ?                │
│  exec · 99s                        │
│  yield receipt · completion        │
│unconfirmed                         │
└────────────────────────────────────┘
    └── ? Shell   … · completion unknown · outcome unknown · tool call ended without a terminal result
context ? | event 99s ago | epoch 1 | stream unconfirmed/replay
```

### Earlier live idle/outage footer (no keyboard input)

```text
context 0% | event 25s ago | epoch unreported | stream observed (health unconfirmed)
context 0% | event 28s ago | epoch unreported | stream observed (health unconfirmed)
context 0% | event 31s ago | epoch unreported | stream observed (health unconfirmed)
```

### replay-attempts

Unknown attempt 1 remains unchanged after a late same-attempt success. Attempt 2
has a distinct completed card. Captured by the production TestBackend journey.

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 0 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 1                  │
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [running] · 1 unresolved calls │
│  family ? / model ?                │
│  run.started · 99s                 │
│  agent-reported                    │
│root/retry [unknown]                │
│  family ? / model ?                │
│  exec_command · 99s                │
│  runtime receipt · effect unknown  │
│root/a2/retry [completed]           │
│  family ? / model ?                │
│  exec_command · 99s                │
│  receipt                           │
└────────────────────────────────────┘
    ├── ? Shell  $ echo attempt1 · completion unknown · 0ms
context ? | event 99s ago | epoch unreported; recovery unobservable | stream unconfirmed/replay
```

## Host generation follow-up

`ready.surface` is decoded into a local `SurfaceReady` observation. It carries
the current durable cursor, does **not** advance that cursor, and is delivered
before reconnect replay. A first observation only establishes a baseline. A
later generation increase or known epoch increase invalidates retained process
and Code Mode bindings, including open nested children: `unknown`, reason
`host runtime lost`. Successful completed child calls are not reopened. This is
loss of the runtime binding, not proof the external OS process was killed.
Legacy servers without metadata remain explicitly unconfirmed.

The managed-client websocket integration test reconnects at the same cursor,
observes generation 1 then 2 with unchanged epoch 0, and asserts observations do
not consume/skip either following durable event. The production-renderer journey
captures the no-change first baseline and idle generation-only invalidation.

On 2026-10-12 this agent ran a separate instance of reviewer's real workerd
harness (producer checkout `20c486203`, port 43830, task-owned stack directory),
attached the newly built TUI, and submitted scripted `SLEEP 180`. Before owner
loss the Code Mode/shell bindings were retained. SIGKILL and same-port restart
at 21:49:19–21:49:24 yielded two unknown rows and zero retained calls, with the
transcript reporting `host runtime lost; process outcome unknown`. This was
real transport + persisted SQLite/R2 + scripted model, not a deployed account.
No reviewer-owned process was killed. The task-owned workerd and tmux sessions
were stopped after capturing. Full frames: swarm output/mission-generation-live.

### host-runtime-lost

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 1                      │
│Blocked/unknown: 1                  │
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [completed] · 1 unresolved     │
│calls                               │
│  codex / gpt-6-astra               │
│  run.completed · 99s               │
│  agent-reported                    │
│root → 7 [running]                  │
│  claude / claude-opus-4-6          │
│  run.started · 99s                 │
│  agent-reported                    │
│root/shell-1 [unknown]              │
│  family ? / model ?                │
│  host runtime lost · 99s           │
│  runtime receipt · effect unknown  │
└────────────────────────────────────┘
    └── ? Shell … · session 42 · completion unknown · host runtime lost; process outcome unknown · 0ms
context ? | event 99s ago | epoch 0 | stream unconfirmed/replay
```

### Live before/after owner loss

before

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 0                      │
│Blocked/unknown: 0                  │
│Retained calls: 2 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [completed]                    │
│  family ? / gpt-6.1-sol            │
│  run.completed · 12s               │
│  agent-reported                    │
│root/sleep-49b8d2 [retained] ·      │
│liveness unconfirmed                │
│  family ? / model ?                │
│  exec · 12s                        │
│  yield receipt · completion        │
│unconfirmed                         │
│root/sleep-49b8d2/code-1 [retained] │
│· liveness unconfirmed              │
│  family ? / model ?                │
│  exec_command · 23s                │
│  yielded · liveness unconfirmed    │
└────────────────────────────────────┘
    └── ◌ Shell  $ echo starting; sleep 180; echo finished · 23.5s
context 0% | event 12s ago | epoch 0 | stream observed (health unconfirmed)
```

after

```text
┌ Mission Control ───────────────────┐
│Status · legacy / best-effort       │
│Done: 1 (not verified)              │
│In progress: 0                      │
│Blocked/unknown: 2                  │
│Retained calls: 0 (unconfirmed)     │
│Next: inspect task evidence         │
│Task tree · assignment states       │
│F6 tree · ↑↓/PgUp/PgDn · Enter      │
│3 shown / 3 loaded · row 1          │
│root [completed] · 2 unresolved     │
│calls                               │
│  family ? / gpt-6.1-sol            │
│  run.completed · 21s               │
│  agent-reported                    │
│root/sleep-49b8d2 [unknown]         │
│  family ? / model ?                │
│  host runtime lost · 21s           │
│  runtime receipt · effect unknown  │
│root/sleep-49b8d2/code-1 [unknown]  │
│  family ? / model ?                │
│  host runtime lost · 32s           │
│  runtime receipt · effect unknown  │
└────────────────────────────────────┘
    └── ? Shell  $ echo starting;  … · completion unknown · host runtime lost; process outcome unknown
context 0% | event 4s ago | epoch 1 | stream observed (health unconfirmed)
```

## Shared reducer adoption assessment (#988)

Recommendation: adopt `nanocodex-surface-state` as the single lifecycle reducer
under this layout, not merge Card Stream's layout. Inspected its serde-only
crate at `4f467534d`: `AgentEventAdapter` handles managed history/snapshots,
`SurfaceState::apply` fences deltas and settles epochs, and `Projection.cards`
provides stable IDs, depth, pinned status, badges and semantic tone. These map
naturally to the current side pane; keep F6/tree scrolling/transcript focus as
presentation-only state and change selection from index to stable card ID.

Before replacing the current reducer:
1. Carry producer-supplied attempt IDs through the adapter/snapshot, rather than
   deriving attempts from arrival order. Preserve authoritative terminal unknown
   per attempt; allow refinement only for explicitly inferred uncertainty.
2. Add the generation observation as transport metadata (no manufactured durable
   cursor), and bind retained process/cell entities. Do not equate generation
   with epoch or a socket timeout with process death.
3. Route a watermark-consistent live snapshot + deltas into one reducer; keep
   partial snapshots from retiring unrelated entities. Separate durable producer
   cursor from local adapter bookkeeping and preserve replay order.
4. Feed shared cards/liveness into Mission Control and remove its duplicate
   BTreeMap lifecycle state. Keep historical transcript formatting, but drive
   lifecycle presentation from the same entity IDs rather than a second truth.
5. Gate the swap on common fixtures for idle generation change, same-call retry,
   cross-agent ID collision, unknown terminals, reconnect gaps and 30-child
   navigation. Repeat this live retained-process owner-loss run against both
   layouts; compare projections, not screenshot cosmetics.

This is an evaluated adoption plan, not a claim the dependency was integrated.
The inspected adapter snapshot shape omitted generation and explicit attempt,
and its reconciliation policy needs alignment with the terminal-per-attempt
contract. Wholesale swapping it now would regress fixes demonstrated here.

## Protocol round 3 synchronization

Merged #989 through `400830739`: loopback `/surface`, runtime epochs, explicit
attempts and runtime generation now match the consumer contract; the loopback
server opts into interruption rather than re-execution of a lost owner's tool
effect. This worktree reran 351 TUI tests (2 ignored), 63 managed-client tests,
and the durability test
`a_tool_effect_started_by_a_lost_owner_is_interrupted_not_re_executed` (1 passed).
The native binary build was rerun. This does not claim a new full loopback
SIGKILL journey; agent 2's independent comparison is in the protocol runthrough.

The new `ManagedEventStream::last_heartbeat()` exposes SSE heartbeat metadata.
Mission Control currently uses the websocket path, so its footer deliberately
still says health unconfirmed; merging this API does not falsely make that label
connected or demonstrate heartbeat consumption. Shared-reducer adoption should
wire real transport observations, not infer heartbeat from UI redraws.
