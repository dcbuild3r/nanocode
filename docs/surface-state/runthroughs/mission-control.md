# Mission Control TUI variant

The transcript remains the work surface. At 100 columns or wider, a pinned
38-column Mission Control panel shows the current root turn's Status and an
agent/tool tree. Narrow terminals retain the transcript and liveness footer.
The tree labels family/model as unknown unless event evidence supplies them;
completion is explicitly **not verification**. Child-parent arrows are sourced
from child descriptors where available. Rows show the latest observed step,
lifecycle, and event age. No signal after 60 seconds is an observation, not failure.

`SurfaceState` is a clock-free event reducer. Rendering receives time explicitly;
the `tui-bench` feature uses a fixed clock for fixture fingerprints. Root run
changes clear the prior turn's status; retries settle missing same-scope tool
receipts as unknown. `runtime.epoch` settles outstanding transcript tools and
surface entities. Explicit unknown tool/run terminals are also accepted by the
projection; ordinary tool results can subsequently resolve unknown outcomes.
Transport loss is labeled offline/replay, not success. Context above 100% is
shown without clamping and with an explicit warning.

## Reproduce the captured fixture journey

```sh
CARGO_TARGET_DIR=/srv/nanocodex/w/surface-target-B \
  cargo test -p nanocodex-bin --lib --features tui-bench \
  nanocodex2::tui -- --nocapture
```

`mission_control_journey` uses the same TestBackend / AppNode harness as
`benches/nanocodex2_tui.rs`, not a separate mock renderer. Each frame is 140×48.
It checks visible results, rejects projection errors, and renders twice to
assert stable fingerprints. Captured frames are generated in
`output/mission-control/` (ignored evidence, not committed):

- `normal.txt`: running root, family/model, pinned Status, elapsed/no-signal observation.
- `swarm.txt`: root + child 7, distinct families/models, running Shell.
- `restart.txt`: epoch 2, Shell stops spinning and says completion unknown.
- `reconnect.txt`: replay followed by new root run; prior unknown is preserved.
- `stale-summary.txt`: old completion labeled stale and not verified.
- `context-overflow.txt`: 108% context warning.
- `fallback.txt`: new same-scope run without an epoch settles orphan Shell unknown.

These are fixture-driven local renderer captures, **not** claims of a deployed
runtime restart, a live network reconnection, a shipped build, or completed work
verification. A missing upstream epoch event remains visibly unreported.

## Remaining integration boundaries

This variant consumes existing transcript and child updates. It does not add
snapshot/delta transport, heartbeat production, authoritative process-session
ownership, persisted evidence-backed plans, or task-tree keyboard navigation.
Status is derived event evidence, not an agent-authored verified/next plan.
The side pane has no independent scroll control yet; large trees clip to the
available height. Ages advance on normal TUI redraws, not a separate heartbeat.
Family is unknown when the runtime supplies no family field. The historical
transcript reducer still owns card layout and existing completion rules; the new
reducer owns the Mission Control projection, rather than replacing all history
logic in one change.

## Observed local verification (2026-10-12)

- `cargo check -p nanocodex-bin --lib --features tui-bench`: passed.
- Command above: **350 passed, 0 failed, 2 ignored**, including all seven
  Mission Control captured frames; final run finished in 1.23s after compilation.
- Inspected `restart.txt`: transcript visibly contains `completion unknown` and
  no live running timer for that Shell. Inspected `context-overflow.txt`: footer
  visibly contains `context 108% WARNING >100%`.
- `git diff --check`: passed. Existing compiler warnings remain unchanged.
