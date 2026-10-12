# Surface state: one ledger, four projections

**Status: proposed contract and UX alternatives, not a shipped feature.** Source audit
baseline: `960d866b0beaca6970fbbb3f4580b0d6c6ed687a` (2026-10-12).
This document does not certify the parallel implementation branches. Its normative
“MUST” statements are acceptance criteria for that implementation.

## Observable promise

A user returning to a long-running task can answer, without sending “Status?”:
**what is running, where, what needs me, what actually finished, and what evidence
supports that claim?** A transport disconnect is not a failed job. A completed
assistant turn is not a completed engineering task. A lost observer is not proof
that an external command stopped. A successful build is not an installed release.

No surface should reconstruct those facts from prose, elapsed time, or a missing
child in a bounded list. Layout can differ; identities, lifecycle and evidence
must not. Preserve familiar chat, but stop making the transcript the state database.

## Research and limits

A bounded private-history survey covered notes dated October 5–11, approximately
the week preceding this design. `find_sessions` candidates were verified with
`read_session` for the runtime-factory request and stale-TUI incident. Daily memory
notes are retained reports, not rerun experiments. Search is neither a census nor
proof that an absent feature was unused. October 12's daily note had no matching
entries. Earlier decision-inbox and WoW preferences were explicitly checked as
historical context, not silently attributed to the current week.

Only generalized product requirements appear here. Private identifiers, machine
inventories, account records, and raw conversations are not publication material.
The private research locator list is in the task's ignored output, not this spec.

| Observed usage / evidence strength | Product consequence |
| --- | --- |
| Oct 5 durability report describes durable child topology and unsafe unknown effects; Oct 8–10 reports repeatedly distinguish lost observers from completed remote work. Retained reports, not independently rerun. | Preserve operation identities, reconcile instead of replaying, and expose recovery explicitly. |
| Oct 6 Hand menu work distinguishes registrations from physical computers, connection health from capture readiness, and local service from sign-in. Oct 7 request explicitly rejects per-workspace Hand identity. | One stable Hand identity with separate capabilities, service/auth health and dated observations; cwd is context, not a new Hand. |
| Oct 6 request for a memory file tree; repeated use of durable `/brain` reports to survive runtime loss. | Brain is a durable workspace/resource, not a compute host or a perpetual running job. Link artifacts and checkpoint receipts from status. |
| Oct 7 fresh-terminal measurements and Oct 8 startup investigations distinguish first frame, accepted turn, model response and tool time. | Show these phases separately; never hide admission delay inside a generic spinner or add overlapping spans. |
| Verified history turn reports stale running children after restart, with unknown agent IDs and no matching local processes. | Authoritative recovery and snapshot replacement must retire stale UI entries; lack of a list result alone cannot do so. |
| Verified factory request wants configuration once, then independent agents/harnesses. Oct 9–11 recovery reports distinguish child completion delivery from root task completion. | Runtime → agent/session → turn → run ownership, with explicit task-tree lineage and independently versioned completions. |
| Historical mobile preference: incoming signals and user thoughts converge into sourced decisions, including decisions with no preceding TODO. | Shared card envelope; decision authorization/version remains distinct from tool progress. Current approval code corroborates this separation. |
| Historical WoW preference: mobile-like streaming/live tool cards, parallel conversations, quest sidebar. | Same state semantics through a bandwidth-bounded companion projection; local quest organization is not native child ownership. |

## Current source map (facts, not proposed APIs)

* [TUI transcript model](../../bin/nanocodex/src/nanocodex2/tui/transcript/model.rs):
  `running_tools`, `finish_activity`, `background_shell_ids`,
  `settle_orphaned_tools`, `settle_unfinished_tools`. The existing fallback already
  uses **unknown**, not failed, and exempts retained process sessions. Move the
  authority for this decision upstream; do not regress this distinction.
* [Managed protocol](../../js/managed/src/protocol.ts): `ready` includes
  `active_turns`/`latest_event_cursor`; `ServerMessage` includes turn lifecycle,
  nested `AgentEvent`, optional cursor/time/turn fields. It is not yet the full
  resource/task snapshot proposed below.
* [Durable events](../../js/managed/src/durable-events.ts): string cursors,
  bounded history, `DurableEventTail.high_water_cursor`, keepalives, per-subscriber
  authorization and projection. Reuse these primitives rather than introduce a
  second uncorrelated transport log.
* [Subagent model](../../crates/nanocodex-subagents/src/model.rs),
  [durability](../../crates/nanocodex-subagents/src/durable.rs),
  [task tree](../../crates/nanocodex-subagents/src/task_tree.rs): agent status and
  completion delivery are separate from an individual tool card.
* [Shared web thread](../../js/account/src/SharedThreadView.tsx): cursor-sorted
  history and fetch streaming with reconnect cursor preservation. Add state
  hydration here without breaking its read/write-share authorization distinction.
* [Apple transcript](../../apple/InboxCore/Sources/InboxCore/RealtimeTranscript.swift),
  [incremental preparation](../../apple/InboxCore/Sources/InboxCore/TranscriptPreparation.swift),
  [managed client](../../apple/InboxCore/Sources/InboxCore/ManagedClient.swift):
  keep incremental text rendering independent from lifecycle facts.
* [Decision preparation](../../apple/InboxCore/Sources/InboxCore/DecisionPreparation.swift)
  requires fresh detail and exact prepared-version matching for approval;
  [TODO backend](../../js/managed/src/todo-inbox.ts) persists versioned decisions
  and operation-keyed responses. Unified appearance must not bypass these rules.
* [WoW contract](../../examples/wow/docs/backend-contract.md),
  [durable client](../../examples/wow/durable_client.py),
  [addon client](../../examples/wow/addon/Nanocodex/Client.lua): existing history
  cursors, explicit steer/cancel receipts and local-only organization. Native child
  creation is explicitly unsupported in that contract; do not draw fake children.

## Canonical entities and states

Execution states: `admitted → running → completed | failed | cancelled | unknown`.
Immediate rejection/terminal receipt may skip running. Unknown is terminal **for
an execution attempt**, not proof of failure or permission to retry an effect.
A later authoritative receipt is a versioned reconciliation of that attempt;
resumed execution gets a new attempt, never an invisible terminal→running reversal.

| Entity / identity | Owner and lifetime | State / required facts |
| --- | --- | --- |
| Runtime / runtime_id + epoch | Session execution authority; epoch changes on recovery, not client reconnect | initializing, ready, recovering, unavailable; fenced generation, reason, previous epoch |
| Agent / durable agent_id | Runtime; owns session and explicit parent relationship | available, active, interrupted, closed; current attempt, harness/model, completion revision. `completed` describes an assignment/result, not destruction of a reusable agent. |
| Turn / turn_id | Agent; one admitted user or delegated request | execution states; admission receipt, cancellation_requested separately, terminal cursor; task outcome distinct |
| Run / run_id + attempt_id | Turn; concrete execution attempt | execution states; epoch, model phase, last progress, parent run if nested |
| ToolCall / call_id + attempt_id | Run or Code Mode parent call | execution states; logical operation_id, dispatch/effect uncertainty, result/evidence ref, owning Hand if any |
| ProcessSession / process_id + runtime binding | Execution host, not polling turn | execution states; originating call, host generation, output cursor, exit receipt; polls are separate calls |
| Hand / stable machine/connection identity | Account registry; cross-turn | online, offline, unknown; observation timestamp, service/auth/capture capabilities independently, stale resource samples. No terminal lifecycle for permanent hardware. |
| Brain / workspace_id | Account/session durable workspace | available, unavailable, unknown; namespace, checkpoint/revision and observed time; no “running memory” badge without an actual operation |
| Card / card_id + revision | View over entity or versioned decision | kind: tool, status, decision, artifact; source references; decisions keep preparation/approval/execution states separate |
| Status / root turn + status revision | Root task projection | done/verified, in progress, blocked, next; generated_at, through_cursor, dependency revisions, evidence refs |

IDs are opaque strings in the wire contract. Parentage is explicit and scoped to
an authorized task tree. Session forks are lineage edges, not ownership edges.
A child may outlive the root's current turn when its ownership policy permits it.
Closing a root cancels descendants only according to the existing explicit policy.
A turn terminal MUST NOT imply all processes or background children terminated.

## Protocol diff: proposed `surface_state_v1`

Add an advertised capability; keep the existing stream and text events. Producers
persist entity revisions and lifecycle events in the same durability boundary as
execution admission/receipts. Do not implement correctness only in a TUI reducer.
The following names are proposed, not claims about existing routes or SDK exports.

```text
Existing ready(active_turns, latest_event_cursor)
  + surface_state_version: 1, runtime_id, epoch, snapshot_available
Existing turn_*/event messages
  + canonical entity references (without changing legacy payload meaning)
New durable events
  runtime.epoch {previous_epoch, epoch, reason, recovery_batch_id}
  surface.entity {entity_id, kind, revision, attempt_id?, state, owner, facts}
  surface.reconciled {entity_id, attempt_id, revision, receipt, supersedes_cursor}
  surface.removed {entity_id, revision, reason}  // deletion, not completion
  surface.status {root_turn_id, revision, through_cursor, dependencies, sections}
New control frames (not evidence of tool progress)
  surface.heartbeat {epoch, server_time, high_water_cursor}
  surface.resync_required {reason, snapshot_available}
```

Envelope: `{schema_version, stream_id, cursor, epoch, emitted_at, type, payload}`.
Cursor is an opaque decimal string (never a JS number); entity revision and attempt
identity provide dedupe independent of transport duplication. Cursor order is
local to its stream; never compare cursors from separate agents numerically.
For a tree assembled from multiple streams, publish a watermark vector plus
per-source freshness; do not pretend a cross-service snapshot is transactional.

### Terminal guarantee and recovery fence

1. Persist admission before acknowledging it; retain enough identity to settle
   every admitted execution attempt, including one with no dispatched tool yet.
2. Fence old producers when acquiring a new epoch. Recover a receipt already
   committed before emitting an unknown; known completed work must stay completed.
3. For every abandoned in-flight attempt lacking a terminal receipt, persist
   terminal `unknown` with reason `runtime_restarted` and effect certainty
   `unconfirmed`. Include tools nested in a Code Mode cell. No blanket failed.
4. Durable logical turns/children may resume under new attempts. A detached host
   process can still exist: retain its original host/process identity and show
   “outcome unknown; reconcile on original Hand.” Do not send it elsewhere.
5. Recovery batches may be bounded, but advertise `recovering` and withhold the
   authoritative ready snapshot until the batch fence and all settlements commit.
   A crash during recovery resumes the same settlement idempotently.
6. Late receipts must be authenticated to the original operation/host binding;
   only `surface.reconciled` can refine unknown. No second completion toast,
   no duplicate child result delivery, no replay of an irreversible effect.
7. During total service loss, no terminal-delivery deadline can be guaranteed.
   Clients display stale/disconnected observations until the authority recovers.
   Heartbeat loss never fabricates a terminal event.

Cancellation acceptance is `cancellation_requested=true`; only a terminal receipt
renders cancelled. Closing a stream is not cancellation. Terminal persistence,
transport delivery, user notification and user acknowledgment are distinct facts.

### Snapshot plus delta, without the reconnect race

Proposed authenticated snapshot read returns `{snapshot_id, schema_version,
stream_id, epoch, through_cursor, entities, tombstones, status, completeness}`.
It includes all open entities, selected terminal card summaries, and resource
observations; full historical text stays in the paginated event archive.

* Capture entities and high-water cursor at one consistent read boundary. For a
  paginated snapshot pin every page to snapshot_id and the same watermark.
* Apply only after the complete scoped snapshot arrives; buffer later deltas.
  Replace that scope, not the entire account. Do not interpret an omitted item
  in a partial page as deleted. Preserve local focus, drafts and read position.
* Subscribe/replay strictly **after** through_cursor, dedupe by stream/cursor and
  entity revision, then apply buffered events. Existing replay must retain events
  spanning the fetch/subscribe interval; otherwise demand a fresh snapshot.
* Event gaps/expired cursors/schema incompatibility yield resync_required, not a
  silent jump to latest. Numeric gaps alone do not prove loss in filtered streams.
* Reject obsolete revisions/epochs; preserve valid late reconciliation receipts
  via the current authority rather than directly accepting an old producer.
* Completion/decision-notification identity is durable. Replay updates a card in
  place, never appends an apparent new task completion.

Reducers must produce equivalent semantic state for full replay and snapshot +
remaining deltas. Transport-only heartbeats update connection health, not last
entity progress. A slow tool can legitimately have old progress and fresh transport.

## Evidence, status and safety

Every claim stores source kind (`runtime_receipt`, `agent_report`, `test_build`,
`deployment_receipt`, `installation_receipt`), source ref, observed time, subject
revision/commit, and verification scope. “Verified” requires inspectable evidence
for that claim; it is not an agent-selectable green label. Test-build is an evidence
scope, not a lower-confidence synonym for installed. Link a build to its source
SHA and an installation to the exact target and build. No implied CI/deploy result.

Status summaries carry the entity revisions they summarize. Mark “summary behind
live state” when one changes; keep the last summary visible with its timestamp.
Freshness is dependency-aware, not “some token arrived recently.” Generate prose
on meaningful transitions, not every heartbeat. Immediate badges remain pure
reducers; summary generation failure cannot block truthful lifecycle display.

UI timers show observed active time or time since last evidence; terminal unknown
stops a running animation. Missing timing is unknown, not zero. Show context usage
as measured with source/time, including values over 100%; distinguish compaction
from inactivity and avoid fabricating provider capacity.

Authorization is applied before snapshot/delta projection, including child edges,
Hand details, resource names, artifacts and decision bodies. Share readers must not
learn private topology from hidden counts. Scope/auth-generation changes invalidate
caches. Cards never carry credentials, provider tokens or private-input values.
Approval is exact-version, server-revalidated and operation-keyed; reconnect does
not resubmit. A stale decision can be viewed but not approved without refresh.

## Surface projections

| Surface | Navigation and primary state | Platform-specific constraint |
| --- | --- | --- |
| TUI | Task tree or card stream beside chat; pinned root Status; small Brain/Hand strip with age/epoch/context | Keyboard-first expand/filter; don't scroll readers on deltas. Legacy inference remains only behind explicit legacy capability and is labeled best-effort. |
| Web | Responsive task/detail layout using same entity IDs; reconnect banner and replay watermark | Background tab sleep is normal. Shared-thread projection remains server-authorized; read-only viewers get no action controls. |
| Apple | Needs me → Active → Activity; native cards and focused task drill-down; memory/Hand details as separate destinations | Push is a wakeup hint, not state authority. On foreground fetch snapshot before actions. Preserve drafts and scroll anchors; never equate notification delivery with phone display. |
| WoW | Quest sidebar, current quest panel, compact state badges and streamed active-thread text | Companion translates bounded deltas, prioritizes terminal/decision/recovery frames over token updates; ACK is transport delivery, not task success. Local project membership stays explicitly local. |

## Three UX alternatives

All sketches below are **illustrative, not captured terminal frames**. Their
underlying facts and action contracts are identical; choose layouts independently
of the lifecycle rollout.

### A — Mission Control: task tree as the home screen

```text
TASKS                         CURRENT TASK / evidence
▾ Parser rollout  needs me    Done       implementation [agent report]
  ✓ Research                  Verified   build @abc [test-build]
  ? Migration / Hand north    In progress review, receipt 8s ago
  ● Review                    Blocked    migration outcome unknown
  ◇ Approve rollout           Next       reconcile original operation
Brain saved · Hand north stale · epoch 9 · stream connected
```

Tree groups descendants by ownership, with optional execution-location columns.
Selecting a node filters cards without losing the root status. Expand unknown
opens the last receipt and safe reconciliation action, not Retry. Keyboard actions
must name their scope: steer this turn versus cancel this subtree. Wide screens
show tree/detail/chat; narrow screens use a node breadcrumb and one pane.

Best for coordinating swarms and locating blockers. Risk: large trees hide the
work product. Mitigation: collapse terminal siblings, pin evidence and decisions,
show bounded-page counts honestly (“12 loaded”, never “12 total” without proof).

### B — Card stream: one vocabulary with mobile

```text
PINNED  Parser rollout · summary behind live state · Refresh
NEEDS ME  Approve release v4 [view exact diff] [review]
UNKNOWN   Migration · runtime restarted · reconcile original operation
ACTIVE    Reviewer · last evidence 8s ago [open task]
EVIDENCE  Build passed @abc · test-build, not installed [receipt]
```

One stable card per entity/decision version, shared across TUI and mobile. Tabs
Needs me / Active / Activity are queries, not separate state stores. Tool cards
collapse into their parent operation; status and evidence remain accessible.
Completion updates the existing card. Reading a card pins its position while
unseen updates accumulate in a quiet counter; no live-feed scroll jump.

Best for phone-to-terminal continuity. Risk: a busy stream loses parent context.
Mitigation: persistent root badge, lineage breadcrumb and “show siblings”; keep
attention priority separate from chronological audit order. Tool completion can
never become a decision approval merely because they share a card component.

### C — Outcome Atlas: a navigable map of claims and uncertainty

```text
OUTCOME: release ready?
source @abc ── build passed ── review pending ── deploy not observed
                  │                   │
              receipt              reviewer
                  └── migration ? ────┘
Compare with last visit: +1 verified claim · 1 lost observer · 1 decision
```

Start with the requested outcome, not the agent or transcript. Each claim is a
node backed by evidence; edges express explicit prerequisites, never inferred
causation. An uncertainty frontier highlights missing or stale evidence. Select a
node to see its owning task and receipts. “Since I left” compares two durable
watermarks and explains state changes—including corrections and uncertainty.
A time scrubber is read-only; historical approvals cannot be executed.

Best for multi-day tasks with repeated “patched versus shipped?” confusion.
Risk: an attractive map invents dependencies or certainty. Mitigation: agent-
proposed dependencies are labeled as reports until accepted as task metadata;
keep an accessible linear claim list and exact source links. Start with a small
hand-authored outcome checklist rather than automatic semantic extraction.

Recommendation: land the shared contract first; use A for desktop orchestration,
B as the mobile/default compact projection, and prototype C behind read-only
outcome exploration. Do not fork the state machine for the three variants.

## Acceptance journeys and rollout gates

These are required future runs, **not tests executed by this documentation PR**.
Use synthetic identities and real public HTTP/CLI boundaries. Keep raw traces and
captured frames in ignored output or CI artifacts, never private histories in Git.

| Journey | Required observation / evidence |
| --- | --- |
| Normal turn with shell | Admission → run → tool → terminal; one card; terminal cursor/exit receipt; capture actual TUI/web render. |
| Mixed-harness swarm | Explicit parent links; child completion once across replay; root turn completion does not close retained children. |
| Restart mid-tool | Kill runtime after dispatch, recover epoch; abandoned attempt unknown, no ticking old card; original host command not replayed. |
| Crash between receipt and broadcast | Reconnect recovers completed result, never changes it to unknown; completion notification not duplicated. |
| Detached process and poll | Parent/poll terminal leaves process open; host generation loss produces unknown, not false exit; reconcile original binding. |
| Snapshot/reconnect race | Inject events between snapshot and subscribe and during paging; final state equals full replay; drafts/focus preserved. |
| Filtered/expired history | Empty visible page still advances valid watermark; genuine retention loss requests new snapshot; no fabricated terminal state. |
| Stale summary and deployment | Update dependency after summary; badge stale; build-only receipt never says installed; late reconciliation visibly corrects old unknown. |
| Mobile decision revision | Edit decision on another client; old approval rejected, refresh required; exact-version approval creates one operation. |
| Authorization and sharing | Foreign/deleted child and revoked share cannot leak snapshot, delta, cached cards or topology counts. |
| WoW companion loss | Drop/reorder/duplicate frames; converge via snapshot; ACK alone never completes task; no pretend native-child capability. |

Roll out in producer → shared contract/reducer → consumer order. Dual-write canonical
facts and legacy events initially; negotiate capability, measure disagreements,
then remove client orphan inference only for v1 sessions. Keep a labeled legacy
fallback for older servers; do not mix inferred and authoritative states silently.
Test old/new client combinations and unsupported schemas. Native host capabilities
may lag managed ones: advertise actual terminal/snapshot coverage, not blanket
parity. Resource health and task lifecycle can ship independently but share IDs.

Open implementation decisions: ledger retention and snapshot limits; ownership of
tree aggregation; durable notification acknowledgment semantics; heartbeat/staleness
thresholds; compatibility of each runtime's attempt IDs. Choose thresholds with
measured transport behavior. This spec intentionally promises no unmeasured latency,
exactly-once external effects, production deployment, or installed-surface parity.
