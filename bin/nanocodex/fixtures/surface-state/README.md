# Producer recovery fixtures

These intentionally retained integration fixtures were extracted from the real
curl/workerd journey captured by protocol agent 2, not hand-invented wire messages.
Producer branch: `feat/surface-state-protocol`, commit
`b94dfa6f75d3a8b853d3bf9a3faab8a5f61880ce` (merged into this variant).

Source capture directory on the validation host:
`/srv/nanocodex/w/surface-state-protocol/output/surface-state-recovery/2026-10-12T20-53-32.081Z-e2b1dc65/curl/`.

- `open-snapshot.json`: exact body of `002-root-surface-during.response`.
- `recovery-events.json`: causal run/tool prefix, `runtime_epoch`, and two
  interruption receipts selected from `007-root-history-surface.response`.

All identifiers are from the synthetic producer journey. No headers or bearer
material are included. `mission_control_real_protocol_capture` deserializes the
published Rust types, hydrates the snapshot at cursor 10, applies later deltas,
and renders the full captured causal sequence using the production TUI harness.
This is a consumer replay of agent 2's live evidence, not a claim that agent 3
independently repeated the producer's workerd SIGKILL journey.
