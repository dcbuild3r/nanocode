// Cross-surface contract: the JavaScript reducer and projection must produce
// exactly the golden cards the Rust crate produces for the shared fixtures,
// so the web and TUI surfaces render identical state from identical events.

import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";
import { SurfaceState } from "../index.mjs";
import { renderCardStreamHtml } from "../html.mjs";

const fixtures = new URL("../../../crates/nanocodex-surface-state/fixtures/", import.meta.url);
const names = readdirSync(fixtures)
  .filter((file) => file.endsWith(".json") && !file.endsWith(".cards.json"))
  .map((file) => file.slice(0, -".json".length))
  .sort();

const read = (file) => JSON.parse(readFileSync(new URL(file, fixtures), "utf8"));
const plain = (value) => JSON.parse(JSON.stringify(value));

test("covers every required journey", () => {
  for (const journey of ["normal-turn", "subagent-swarm", "restart-mid-tool", "reconnect", "stale-summary"]) {
    assert.ok(names.includes(journey), journey);
  }
});

for (const name of names) {
  test(`${name} matches the Rust golden projection`, () => {
    const fixture = read(`${name}.json`);
    const golden = read(`${name}.cards.json`);
    const state = new SurfaceState();
    const frames = fixture.steps.map((step) => {
      for (const event of step.events) state.apply(event);
      return { label: step.label, projection: plain(state.project(step.now_ms)) };
    });
    assert.deepStrictEqual(frames, golden);
    for (const frame of frames) {
      const html = renderCardStreamHtml(frame.projection);
      for (const card of frame.projection.cards) {
        assert.ok(html.includes(`data-card-id="${card.id}"`), `${name}: ${card.id} rendered`);
      }
    }
  });
}

test("unknown events fail loudly instead of being guessed", () => {
  const state = new SurfaceState();
  state.apply({ type: "heartbeat", cursor: 1, at_ms: 0 });
  assert.throws(() => state.apply({ type: "mystery", cursor: 2, at_ms: 0 }), TypeError);
});
