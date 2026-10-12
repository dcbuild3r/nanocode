import { SurfaceState, projectEvents, type Projection, type SurfaceEvent } from "../index.mjs";
import { renderCardStreamHtml } from "../html.mjs";

const events: SurfaceEvent[] = [
  { type: "runtime_epoch", epoch: 2, cursor: 1, at_ms: 0, reason: "runtime restarted" },
  {
    type: "upsert",
    cursor: 2,
    at_ms: 0,
    entity: { id: "h", kind: "hand", epoch: 2, state: "running", title: "Hand", updated_at_ms: 0, online: true },
  },
];
const state = new SurfaceState();
for (const event of events) state.apply(event);
const projection: Projection = state.project(1);
const html: string = renderCardStreamHtml(projectEvents(events, 1));
void projection;
void html;

// @ts-expect-error tool calls require the tool name.
const invalid: SurfaceEvent = { type: "upsert", cursor: 3, at_ms: 0, entity: { id: "t", kind: "tool_call", epoch: 1, state: "running", title: "Shell", updated_at_ms: 0 } };
void invalid;
