# Runthrough: surface-state protocol (runtime epochs and open-entity snapshots)

Branch `feat/surface-state-protocol`. Server: `js/managed` Durable Object
(`src/surface-state.ts`, `src/index.ts`). Client: `crates/nanocodex-managed`
(`ManagedClient::surface`, `ManagedClient::surface_events`,
`ManagedEventData::RuntimeEpoch`).

Every frame below was captured from a real run of
`node --test js/managed/test/surface-state-recovery-journey.test.mjs`: curl against
the normal ingress, Managed API and Egress workers on persisted workerd SQLite/R2,
with owner loss as SIGKILL of the whole workerd process group while an external
effect is held open. Only the model provider and the effect endpoint are stubbed.

## 1. Normal turn: what is open while a shell call runs

`GET /v1/agents/:id/surface` while the root Code Mode cell waits on effect S1
(IDs shortened):

```
{"epoch":0,"cursor":"10","turns":[{"id":"4d83ebe3","state":"accepted"}],
 "runs":[{"agent_id":null,"turn_id":"4d83ebe3"}],
 "tool_calls":[{"agent_id":null,"call_id":"surface-root-cell","tool":"exec","parent_call_id":null},
               {"agent_id":null,"call_id":"surface-root-cell/code-1","tool":"exec_command","parent_call_id":"surface-root-cell"}],
 "agents":[],"process_sessions":[]}
```

`cursor` equals the history `latest_cursor` read immediately afterwards (asserted).

## 2. Runtime restart mid-tool: the card settles to unknown

SIGKILL while S1 is held, restart, then durable history with `surface=1`
(cursor, type, event, agent, call, status, code):

```
1   agent_created
2   turn_accepted
4   event  run.started   root
9   event  tool.call     root  surface-root-cell
10  event  tool.call     root  surface-root-cell/code-1
11  runtime_epoch                                                     epoch=1 interrupted=2 calls
12  event  tool.result   root  surface-root-cell/code-1  failed  TOOL_CALL_INTERRUPTED
13  event  tool.result   root  surface-root-cell         failed  TOOL_CALL_INTERRUPTED
15  event  run.started   root
18  event  tool.call     root  surface-root-cell                       <- replayed attempt
19  event  tool.call     root  surface-root-cell/code-1
20  event  tool.result   root  surface-root-cell/code-1  failed  CODE_MODE_CALL_INTERRUPTED
21  event  tool.result   root  surface-root-cell         failed
28  event  run.completed root
29  turn_completed
```

The epoch and settlements are appended in the cold Session constructor, before
recovery replays anything. Each settled result carries
`outcome_unknown: true`, a `duration_ns`, and `structured_result`
`{code:"TOOL_CALL_INTERRUPTED",outcome:"unknown",reason:"runtime_restarted",epoch:1}`.
The effect S1 was dispatched exactly once.

The epoch frame (as delivered to a `surface=1` reader):

```
{"cursor":"11","turn_id":null,"type":"runtime_epoch","epoch":1,"reason":"runtime_restarted",
 "interrupted":{"tool_calls":[{"agent_id":null,"call_id":"surface-root-cell","tool":"exec","started_cursor":"9",...},
                              {"agent_id":null,"call_id":"surface-root-cell/code-1","tool":"exec_command","parent_call_id":"surface-root-cell","started_cursor":"10",...}],
                "runs":[{"agent_id":null,"turn_id":"4d83ebe3-...","started_cursor":"4",...}],
                "turns":["4d83ebe3-..."]}}
```

## 3. Reconnect

* `GET /events?cursor=0` (legacy reader): 0 `runtime_epoch` frames, both
  unknown-outcome results present. `GET /events?cursor=0&surface=1`: exactly one
  `runtime_epoch` frame.
* After the turn: `/surface` returns `{"epoch":1,"cursor":"29","turns":[],"runs":[],"tool_calls":[]}`;
  `/events/history?after=29&surface=1` is empty; after a follow-up turn, the delta
  from cursor 29 contains exactly the new turn.
* An idle SIGKILL/restart afterwards publishes nothing: epoch stays 1, one
  `runtime_epoch` row in history.

## 4. Subagent swarm: child card stuck while the child moved on

Root spawns a child; the child's shell call (S2) and the root's `wait_agent`
are running. Snapshot before the loss:

```
{"epoch":0,
 "tool_calls":[{"agent_id":1,"call_id":"surface-child-cell","tool":"exec"},
               {"agent_id":1,"call_id":"surface-child-cell/code-1","tool":"exec_command"},
               {"agent_id":null,"call_id":"surface-wait","tool":"exec"},
               {"agent_id":null,"call_id":"surface-wait/code-1","tool":"wait_agent"}],
 "agents":[{"agent_id":1,"role":"Surface child","state":"running","reported_epoch":0}],
 "runs":[{"agent_id":null},{"agent_id":1}]}
```

After SIGKILL and restart the restored child does new work (S3, held). Snapshot
at that moment:

```
{"epoch":1,
 "tool_calls":[{"agent_id":1,"call_id":"surface-child-moved-on","tool":"exec"},
               {"agent_id":1,"call_id":"surface-child-moved-on/code-1","tool":"exec_command"}],
 "agents":[{"agent_id":1,"role":"Surface child","state":"running","reported_epoch":1}],
 "runs":[{"agent_id":1}]}
```

Open calls computed from legacy history at the same moment:

| build | open calls while the child runs S3 | open calls after the root turn completed |
| --- | --- | --- |
| origin/master 960d866b0 | `1:surface-child-cell`, `1:surface-child-cell/code-1`, `1:surface-child-moved-on`, `1:surface-child-moved-on/code-1` | `1:surface-child-cell`, `1:surface-child-cell/code-1` (stuck forever) |
| this branch | `1:surface-child-moved-on`, `1:surface-child-moved-on/code-1` | none |

(Same journey script, `protocol-baseline-journey.test.mjs`, run in a detached
origin/master worktree and in this branch.)

## 5. Stale-summary detection

`agents[].reported_epoch` records the epoch of each child's latest lifecycle
report. A surface whose snapshot `epoch` is greater than an agent's
`reported_epoch` is showing a status that predates the latest restart and must
render it as stale (agent-reported, unverified) until a new report arrives.
