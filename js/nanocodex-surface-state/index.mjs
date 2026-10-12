// JavaScript mirror of crates/nanocodex-surface-state.
//
// A pure reducer over runtime lifecycle events and the card projection every
// surface renders. Keep this file behaviorally identical to the Rust crate:
// test/parity.test.mjs checks both against the same fixture goldens.

export const STALE_RESOURCES_MS = 5 * 60 * 1000;
export const QUIET_MS = 60 * 1000;

const SETTLES_ON_EPOCH = new Set(["agent", "turn", "tool_call", "process_session", "run"]);
const ENVIRONMENT = new Set(["status", "hand", "brain"]);
const SECTIONS = [
  ["done", "Done", "ok"],
  ["in_progress", "In progress", "live"],
  ["blocked", "Blocked", "error"],
  ["next", "Next", "text"],
];
const PROVENANCE_LABEL = { verified: "verified", agent_reported: "agent-reported", test_build: "test-build" };
const PROVENANCE_TONE = { verified: "ok", agent_reported: "info", test_build: "warn" };

export const isTerminal = (state) => state !== "admitted" && state !== "running";
export const isDefinitive = (state) =>
  state === "completed" || state === "failed" || state === "cancelled";

/** Compact elapsed time: `42s`, `4m 12s`, `71h 3m`. */
export function formatDuration(ms) {
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.floor(bytes / 1024)} KiB`;
  return `${Math.floor(bytes / (1024 * 1024))} MiB`;
}

const sub = (a, b) => Math.max(0, a - b);
const line = (text, tone) => ({ text, tone });
const badge = (text, tone) => ({ text, tone });

export class SurfaceState {
  epoch = 0;
  cursor = 0;
  lastEventAtMs = undefined;
  lastHeartbeatAtMs = undefined;
  contextPct = undefined;
  /** @type {Map<string, any>} */
  entities = new Map();
  nextSeq = 0;
  replaysIgnored = 0;
  regressionsIgnored = 0;
  restarts = [];

  entity(id) {
    return this.entities.get(id)?.entity;
  }

  ordered() {
    return [...this.entities.values()].sort((a, b) => a.seq - b.seq);
  }

  apply(event) {
    if (event.type === "snapshot") {
      this.#applySnapshot(event);
      return;
    }
    if (event.cursor <= this.cursor) {
      this.replaysIgnored += 1;
      return;
    }
    switch (event.type) {
      case "upsert":
        this.#upsert(event.entity, event.cursor, event.at_ms);
        this.#advance(event.cursor, event.at_ms);
        break;
      case "runtime_epoch":
        this.#settleEpoch(event.epoch, event.cursor, event.at_ms, event.reason);
        this.#advance(event.cursor, event.at_ms);
        break;
      case "heartbeat":
        this.lastHeartbeatAtMs = event.at_ms;
        if (event.context_pct !== undefined) this.contextPct = event.context_pct;
        this.cursor = event.cursor;
        break;
      default:
        throw new TypeError(`unknown surface event type: ${event.type}`);
    }
  }

  #advance(cursor, atMs) {
    this.cursor = cursor;
    this.lastEventAtMs = atMs;
  }

  #applySnapshot({ epoch, cursor, at_ms: atMs, entities }) {
    if (cursor < this.cursor) {
      this.replaysIgnored += 1;
      return;
    }
    if (this.cursor > 0 && epoch > this.epoch) {
      this.#settleEpoch(epoch, cursor, atMs, "runtime restarted while disconnected");
    }
    const open = new Set(entities.map((entity) => entity.id));
    for (const tracked of this.entities.values()) {
      const entity = tracked.entity;
      if (!isTerminal(entity.state) && !open.has(entity.id)) {
        entity.state = "unknown";
        entity.reason = "closed while disconnected";
        entity.updated_at_ms = atMs;
        tracked.settledLocally = true;
        tracked.lastCursor = cursor;
        tracked.lastEventAtMs = atMs;
      }
    }
    for (const entity of entities) this.#upsert(entity, cursor, atMs);
    this.epoch = Math.max(this.epoch, epoch);
    this.#advance(cursor, atMs);
  }

  #upsert(entity, cursor, atMs) {
    const tracked = this.entities.get(entity.id);
    if (!tracked) {
      this.entities.set(entity.id, {
        entity: structuredClone(entity),
        seq: this.nextSeq,
        lastCursor: cursor,
        lastEventAtMs: atMs,
        settledLocally: false,
        reconciled: false,
      });
      this.nextSeq += 1;
      return;
    }
    const previous = tracked.entity.state;
    const next = entity.state;
    const resumedInNewEpoch = entity.epoch > tracked.entity.epoch;
    if (isTerminal(previous) && !isTerminal(next) && !resumedInNewEpoch) {
      this.regressionsIgnored += 1;
      return;
    }
    if (isDefinitive(previous) && next === "unknown") {
      this.regressionsIgnored += 1;
      return;
    }
    if (tracked.settledLocally && isDefinitive(next)) tracked.reconciled = true;
    if (!isTerminal(next) || isDefinitive(next)) tracked.settledLocally = false;
    tracked.entity = structuredClone(entity);
    tracked.lastCursor = cursor;
    tracked.lastEventAtMs = atMs;
    if (isTerminal(next) && !isTerminal(previous)) this.#settleOrphanedToolCalls(entity.id, cursor, atMs);
  }

  // A tool call cannot outlive the turn, run, agent or Code Mode cell that
  // issued it: when the owner ends first, still-open tool calls settle to
  // unknown (mirrors the Rust reducer).
  #settleOrphanedToolCalls(ownerId, cursor, atMs) {
    const kind = this.entities.get(ownerId)?.entity.kind;
    if (!SETTLES_ON_EPOCH.has(kind) || kind === "process_session") return;
    const owners = [ownerId];
    while (owners.length > 0) {
      const owner = owners.pop();
      for (const tracked of this.entities.values()) {
        const entity = tracked.entity;
        if (entity.kind !== "tool_call" || entity.parent_id !== owner) continue;
        owners.push(entity.id);
        if (isTerminal(entity.state)) continue;
        entity.state = "unknown";
        entity.reason = "owner ended without a tool result";
        entity.updated_at_ms = atMs;
        tracked.settledLocally = true;
        tracked.lastCursor = cursor;
        tracked.lastEventAtMs = atMs;
      }
    }
  }

  #settleEpoch(epoch, cursor, atMs, reason) {
    if (epoch <= this.epoch) {
      this.replaysIgnored += 1;
      return;
    }
    let settled = 0;
    for (const tracked of this.entities.values()) {
      const entity = tracked.entity;
      if (SETTLES_ON_EPOCH.has(entity.kind) && !isTerminal(entity.state) && entity.epoch < epoch) {
        entity.state = "unknown";
        entity.reason = reason;
        entity.updated_at_ms = atMs;
        tracked.settledLocally = true;
        tracked.lastCursor = cursor;
        tracked.lastEventAtMs = atMs;
        settled += 1;
      }
    }
    this.restarts.push({ epoch, at_ms: atMs, reason, settled });
    this.epoch = epoch;
  }

  /**
   * Project the ordered card stream: pinned Status cards, then Hand and Brain
   * cards, then the task tree depth-first.
   */
  project(nowMs) {
    const ordered = this.ordered();
    const children = new Map();
    const roots = [];
    for (const tracked of ordered) {
      if (ENVIRONMENT.has(tracked.entity.kind)) continue;
      const parentId = tracked.entity.parent_id;
      const parent = parentId === undefined ? undefined : this.entities.get(parentId);
      if (parent && !ENVIRONMENT.has(parent.entity.kind)) {
        if (!children.has(parentId)) children.set(parentId, []);
        children.get(parentId).push(tracked);
      } else {
        roots.push(tracked);
      }
    }
    const cards = [];
    for (const kind of ["status", "hand", "brain"]) {
      for (const tracked of ordered) {
        if (tracked.entity.kind === kind) cards.push(this.#card(tracked, 0, nowMs, children));
      }
    }
    const visited = new Set();
    const stack = [...roots].reverse().map((root) => [root, 0]);
    while (stack.length > 0) {
      const [tracked, depth] = stack.pop();
      if (visited.has(tracked.entity.id)) continue;
      visited.add(tracked.entity.id);
      cards.push(this.#card(tracked, depth, nowMs, children));
      const kids = children.get(tracked.entity.id);
      if (kids) for (const kid of [...kids].reverse()) stack.push([kid, depth + 1]);
    }
    return { now_ms: nowMs, cards, liveness: this.#liveness(nowMs) };
  }

  #liveness(nowMs) {
    let open = 0;
    for (const { entity } of this.entities.values()) {
      if (SETTLES_ON_EPOCH.has(entity.kind) && !isTerminal(entity.state)) open += 1;
    }
    const eventAge = this.lastEventAtMs === undefined ? undefined : sub(nowMs, this.lastEventAtMs);
    const heartbeatAge =
      this.lastHeartbeatAtMs === undefined ? undefined : sub(nowMs, this.lastHeartbeatAtMs);
    const parts = [`epoch ${this.epoch}`, `cursor ${this.cursor}`, `${open} open`];
    if (eventAge !== undefined) parts.push(`last event ${formatDuration(eventAge)} ago`);
    if (heartbeatAge !== undefined) parts.push(`heartbeat ${formatDuration(heartbeatAge)} ago`);
    if (this.contextPct !== undefined) parts.push(`context ${this.contextPct}%`);
    if (this.restarts.length > 0) parts.push(`${this.restarts.length} restart(s)`);
    if (this.replaysIgnored > 0) parts.push(`${this.replaysIgnored} replay(s) ignored`);
    const quiet = (age) => age === undefined || age > QUIET_MS;
    const stalled = open > 0 && quiet(eventAge) && quiet(heartbeatAge);
    const liveness = {
      epoch: this.epoch,
      cursor: this.cursor,
      open,
      restarts: this.restarts.length,
      replays_ignored: this.replaysIgnored,
      regressions_ignored: this.regressionsIgnored,
      label: parts.join(" · "),
      tone: stalled || (this.contextPct !== undefined && this.contextPct >= 90) ? "warn" : "ok",
    };
    if (this.contextPct !== undefined) liveness.context_pct = this.contextPct;
    return liveness;
  }

  #walk(root, children, visit) {
    const visited = new Set();
    const stack = [root];
    while (stack.length > 0) {
      const tracked = stack.pop();
      if (visited.has(tracked.entity.id)) continue;
      visited.add(tracked.entity.id);
      visit(tracked);
      const kids = children.get(tracked.entity.id);
      if (kids) stack.push(...kids);
    }
  }

  #card(tracked, depth, nowMs, children) {
    const entity = tracked.entity;
    const kind = entity.kind;
    const badges = [];
    const lines = [];
    let stateLabel;
    let tone;
    switch (entity.state) {
      case "admitted":
        [stateLabel, tone] = ["queued", "info"];
        break;
      case "running":
        stateLabel =
          entity.started_at_ms === undefined
            ? "running"
            : `running ${formatDuration(sub(nowMs, entity.started_at_ms))}`;
        tone = "live";
        break;
      case "completed":
        stateLabel =
          entity.started_at_ms === undefined
            ? "done"
            : `done in ${formatDuration(sub(entity.updated_at_ms, entity.started_at_ms))}`;
        tone = "ok";
        break;
      case "failed":
        [stateLabel, tone] = ["failed", "error"];
        break;
      case "cancelled":
        [stateLabel, tone] = ["cancelled", "muted"];
        break;
      default:
        [stateLabel, tone] = ["unknown", "warn"];
    }

    if (entity.provenance !== undefined) {
      badges.push(badge(PROVENANCE_LABEL[entity.provenance], PROVENANCE_TONE[entity.provenance]));
    }
    if (entity.state === "unknown" && tracked.settledLocally) badges.push(badge("outcome unknown", "warn"));
    if (tracked.reconciled) badges.push(badge("outcome reconciled", "info"));
    let latestEvent = 0;
    this.#walk(tracked, children, (node) => {
      latestEvent = Math.max(latestEvent, node.lastEventAtMs);
    });
    const quietFor = sub(nowMs, latestEvent);
    if (!isTerminal(entity.state) && SETTLES_ON_EPOCH.has(kind) && quietFor > QUIET_MS) {
      badges.push(badge(`quiet ${formatDuration(quietFor)}`, "warn"));
    }
    if (entity.reason !== undefined) {
      const reasonTone =
        entity.state === "failed" ? "error" : entity.state === "unknown" ? "warn" : "muted";
      lines.push(line(entity.reason, reasonTone));
    }

    switch (kind) {
      case "agent": {
        if (entity.role !== undefined) lines.push(line(`role: ${entity.role}`, "muted"));
        const summary = entity.summary;
        if (summary !== undefined) {
          lines.push(line(summary.text, "text"));
          let newer = 0;
          let latest = 0;
          this.#walk(tracked, children, (node) => {
            if (node.lastCursor > summary.at_cursor) {
              newer += 1;
              latest = Math.max(latest, node.lastCursor);
            }
          });
          const provenance = PROVENANCE_LABEL[summary.provenance];
          if (newer > 0) {
            badges.push(badge("summary stale", "warn"));
            lines.push(
              line(
                `summary (${provenance}) from cursor ${summary.at_cursor}; ${newer} newer change(s) through cursor ${latest}`,
                "warn",
              ),
            );
          } else {
            lines.push(line(`summary (${provenance}) current at cursor ${summary.at_cursor}`, "muted"));
          }
        }
        if (entity.context_pct !== undefined) {
          lines.push(line(`context ${entity.context_pct}%`, entity.context_pct >= 90 ? "warn" : "muted"));
        }
        break;
      }
      case "turn":
        if (entity.prompt !== undefined) lines.push(line(`> ${entity.prompt}`, "text"));
        break;
      case "tool_call": {
        const place = [entity.tool];
        if (entity.cwd !== undefined) place.push(`in ${entity.cwd}`);
        if (entity.hand_id !== undefined) place.push(`on ${entity.hand_id}`);
        lines.push(line(place.join(" "), "muted"));
        const result = [];
        if (entity.exit_code !== undefined) result.push(`exit ${entity.exit_code}`);
        if (entity.output_bytes !== undefined) result.push(`${formatBytes(entity.output_bytes)} output`);
        if (result.length > 0) {
          const failed = entity.exit_code !== undefined && entity.exit_code !== 0;
          lines.push(line(result.join(" · "), failed ? "error" : "muted"));
        }
        break;
      }
      case "process_session":
        if (entity.hand_id !== undefined) lines.push(line(`on ${entity.hand_id}`, "muted"));
        if (entity.exit_code !== undefined) {
          lines.push(line(`exit ${entity.exit_code}`, entity.exit_code === 0 ? "muted" : "error"));
        }
        break;
      case "run":
        break;
      case "hand": {
        if (entity.online) {
          [stateLabel, tone] = ["online", "ok"];
        } else {
          [stateLabel, tone] = ["offline", "muted"];
          badges.push(badge("offline", "muted"));
        }
        if (entity.os !== undefined) lines.push(line(entity.os, "muted"));
        const resources = entity.resources;
        if (resources !== undefined) {
          const parts = [];
          if (resources.cpu_pct !== undefined) parts.push(`CPU ${resources.cpu_pct}%`);
          if (resources.ram_free_gb !== undefined) parts.push(`RAM ${resources.ram_free_gb} GB free`);
          if (resources.disk_free_gb !== undefined) parts.push(`disk ${resources.disk_free_gb} GB free`);
          const age = sub(nowMs, resources.observed_at_ms);
          parts.push(`observed ${formatDuration(age)} ago`);
          const stale = age > STALE_RESOURCES_MS;
          if (stale) {
            badges.push(badge("stale resources", "warn"));
            if (entity.online) tone = "warn";
          }
          lines.push(line(parts.join(" · "), stale ? "warn" : "muted"));
        } else {
          lines.push(line("resources unknown", "muted"));
        }
        break;
      }
      case "brain": {
        if (entity.state === "running") [stateLabel, tone] = ["attached", "ok"];
        lines.push(line(entity.workspace, "text"));
        const parts = [];
        if (entity.memory_notes !== undefined) parts.push(`${entity.memory_notes} memory note(s)`);
        if (entity.dirty_files !== undefined) parts.push(`${entity.dirty_files} uncommitted file(s)`);
        if (parts.length > 0) lines.push(line(parts.join(" · "), "muted"));
        break;
      }
      case "decision": {
        if (entity.state === "admitted" || entity.state === "running") {
          [stateLabel, tone] = ["awaiting decision", "info"];
        } else if (entity.state === "completed") {
          stateLabel = "decided";
        }
        lines.push(line(entity.question, "text"));
        if (entity.recommendation !== undefined) {
          lines.push(line(`recommended: ${entity.recommendation}`, "info"));
        }
        const options = entity.options ?? [];
        if (options.length > 0) lines.push(line(`options: ${options.join(" / ")}`, "muted"));
        if (entity.chosen !== undefined) lines.push(line(`chosen: ${entity.chosen} (intent recorded)`, "ok"));
        break;
      }
      case "status": {
        const items = entity.items ?? [];
        const has = (section) => items.some((item) => item.section === section);
        if (has("blocked")) [stateLabel, tone] = ["blocked", "error"];
        else if (has("in_progress")) [stateLabel, tone] = ["in progress", "live"];
        else if (has("next")) [stateLabel, tone] = ["next steps", "info"];
        else [stateLabel, tone] = ["done", "ok"];
        for (const [section, label, sectionTone] of SECTIONS) {
          const sectionItems = items.filter((item) => item.section === section);
          if (sectionItems.length === 0) continue;
          lines.push(line(label, sectionTone));
          for (const item of sectionItems) {
            const provenance =
              item.provenance === undefined ? "unsourced" : PROVENANCE_LABEL[item.provenance];
            const itemTone = item.provenance === undefined ? "muted" : PROVENANCE_TONE[item.provenance];
            lines.push(line(`  [${provenance}] ${item.text}`, itemTone));
            if (item.evidence !== undefined) lines.push(line(`    evidence: ${item.evidence}`, "muted"));
          }
        }
        break;
      }
      default:
        throw new TypeError(`unknown entity kind: ${kind}`);
    }

    const card = {
      id: entity.id,
      kind,
      depth,
      pinned: kind === "status",
      title: entity.title,
      state: entity.state,
      state_label: stateLabel,
      tone,
      badges,
      lines,
    };
    if (entity.subject !== undefined) card.subject = entity.subject;
    return card;
  }
}

/** Reduce a complete event list and project it at `nowMs`. */
export function projectEvents(events, nowMs) {
  const state = new SurfaceState();
  for (const event of events) state.apply(event);
  return state.project(nowMs);
}
