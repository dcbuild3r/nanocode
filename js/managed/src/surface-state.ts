import type { AgentEvent } from "nanocodex";
import type { DurableEvent } from "./durable-events";

/**
 * Result text for a call whose runtime was lost before it reported. The
 * recovered runtime cannot observe the old handler, so the outcome is unknown.
 */
export const INTERRUPTED_TOOL_CALL_ERROR =
  "The session runtime restarted before a terminal result for this tool call, so its outcome is unknown. Inspect external state before retrying any effect.";

/** Durable message type marking a new runtime epoch after recovery. */
export const RUNTIME_EPOCH_MESSAGE = "runtime_epoch";

/** Root events carry no agent_id. Child agent ids are positive integers. */
const ROOT_AGENT = -1;
const EPOCH_KEY = "nanocodex:runtime-epoch";

type EventMessage = Readonly<{ type: "event"; event: AgentEvent; agent_id?: number }>;

type OpenCallRow = {
  agent_id: number; call_id: string; tool: string; request_id: string; protocol_version: number;
  seq: number; turn_id: string | null; runtime_turn_id: string | null; model_call_index: number | null;
  cursor: number; started_at: number;
};
type OpenRunRow = { agent_id: number; turn_id: string | null; cursor: number; started_at: number };
type AgentRow = {
  agent_id: number; parent_agent_id: number | null; role: string; state: string; updated_at: number; epoch: number;
};

export type SurfaceToolCall = Readonly<{
  agent_id: number | null; call_id: string; tool: string; turn_id: string | null;
  parent_call_id: string | null; started_cursor: string; started_at: number;
}>;
export type SurfaceRun = Readonly<{
  agent_id: number | null; turn_id: string | null; state: "running"; started_cursor: string; started_at: number;
}>;
export type SurfaceAgent = Readonly<{
  agent_id: number; parent_agent_id: number | null; role: string; state: string; updated_at: number;
  /** Runtime epoch of the latest lifecycle report for this child. */
  reported_epoch: number;
}>;

export type RuntimeEpochMessage = Readonly<{
  type: typeof RUNTIME_EPOCH_MESSAGE;
  epoch: number;
  reason: "runtime_restarted";
  interrupted: Readonly<{
    tool_calls: readonly SurfaceToolCall[];
    runs: readonly Omit<SurfaceRun, "state">[];
    turns: readonly string[];
  }>;
}>;

const agentId = (stored: number): number | null => (stored === ROOT_AGENT ? null : stored);
const parentCallId = (callId: string): string | null => {
  const index = callId.lastIndexOf("/code-");
  return index > 0 ? callId.slice(0, index) : null;
};
const toolCall = (row: OpenCallRow): SurfaceToolCall => ({
  agent_id: agentId(row.agent_id), call_id: row.call_id, tool: row.tool, turn_id: row.turn_id,
  parent_call_id: parentCallId(row.call_id), started_cursor: String(row.cursor), started_at: row.started_at,
});

/**
 * Durable projection of the open lifecycle entities a client must render:
 * tool calls without a terminal result, runs without a terminal, and child
 * agents with their latest reported lifecycle state. It is maintained inside
 * every durable event append (same SQLite transaction), so a snapshot read in
 * one synchronous step is exactly consistent with the latest event cursor.
 *
 * Runtime recovery. A Session isolate that is lost (deploy, eviction,
 * SIGKILL) takes every running tool handler and run with it. The next cold
 * construction calls recover(): one runtime_epoch message, then one terminal
 * unknown-outcome tool.result for every call that was still open, root and
 * child alike. Clients therefore never render a call as running past the
 * restart. A recovered run that later replays the same call id (root Code
 * Mode recovery does) publishes a fresh tool.call and tool.result after the
 * epoch: a new attempt, not a second result for the interrupted one.
 *
 * Not covered: a runtime torn down and rebuilt inside one live isolate. Its
 * handlers report their own cancellation results.
 */
export class SurfaceState {
  readonly #storage: DurableObjectStorage;

  constructor(storage: DurableObjectStorage) {
    this.#storage = storage;
    storage.sql.exec(`CREATE TABLE IF NOT EXISTS managed_surface_tool_calls (
      agent_id INTEGER NOT NULL, call_id TEXT NOT NULL, tool TEXT NOT NULL,
      request_id TEXT NOT NULL, protocol_version INTEGER NOT NULL, seq INTEGER NOT NULL,
      turn_id TEXT, runtime_turn_id TEXT, model_call_index INTEGER, cursor INTEGER NOT NULL,
      started_at INTEGER NOT NULL, PRIMARY KEY (agent_id, call_id)
    );
    CREATE TABLE IF NOT EXISTS managed_surface_runs (
      agent_id INTEGER PRIMARY KEY, turn_id TEXT, cursor INTEGER NOT NULL, started_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS managed_surface_agents (
      agent_id INTEGER PRIMARY KEY, parent_agent_id INTEGER, role TEXT NOT NULL,
      state TEXT NOT NULL, updated_at INTEGER NOT NULL, epoch INTEGER NOT NULL
    );`);
    // The previous child-only index is superseded; calls it still holds are
    // carried over so they are settled at the next recovery.
    const legacy = storage.sql.exec<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'managed_open_tool_calls'").toArray();
    if (legacy.length > 0) {
      storage.transactionSync(() => {
        storage.sql.exec(`INSERT OR IGNORE INTO managed_surface_tool_calls (agent_id, call_id, tool, request_id,
          protocol_version, seq, turn_id, runtime_turn_id, model_call_index, cursor, started_at)
          SELECT agent_id, call_id, tool, request_id, protocol_version, seq, turn_id, runtime_turn_id,
            model_call_index, cursor, 0 FROM managed_open_tool_calls`);
        storage.sql.exec("DROP TABLE managed_open_tool_calls");
      });
    }
  }

  /** Current runtime epoch; 0 until the first recovery with open work. */
  epoch(): number {
    return this.#storage.kv.get<number>(EPOCH_KEY) ?? 0;
  }

  /** Called for every appended durable event, inside its append transaction. */
  observe(event: DurableEvent<{ type: string }>): void {
    const message = event.message as Partial<EventMessage>;
    if (message.type !== "event" || !message.event) return;
    const agent = Number.isSafeInteger(message.agent_id) ? message.agent_id! : ROOT_AGENT;
    const { type, payload } = message.event;
    const now = event.created_at ?? Date.now();
    if (type === "run.started") {
      this.#storage.sql.exec(
        "INSERT OR REPLACE INTO managed_surface_runs (agent_id, turn_id, cursor, started_at) VALUES (?, ?, ?, ?)",
        agent, event.turn_id, Number(event.cursor), now);
      return;
    }
    if (type === "run.completed" || type === "run.failed") {
      this.#storage.sql.exec("DELETE FROM managed_surface_runs WHERE agent_id = ?", agent);
      return;
    }
    if (type !== "tool.call" && type !== "tool.result") return;
    const callId = payload?.call_id;
    if (typeof callId !== "string" || !callId) return;
    if (type === "tool.result") {
      this.#storage.sql.exec("DELETE FROM managed_surface_tool_calls WHERE agent_id = ? AND call_id = ?", agent, callId);
      return;
    }
    const { protocol_version: version, request_id: requestId, seq } = message.event;
    this.#storage.sql.exec(
      `INSERT OR REPLACE INTO managed_surface_tool_calls (agent_id, call_id, tool, request_id,
        protocol_version, seq, turn_id, runtime_turn_id, model_call_index, cursor, started_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      agent, callId, typeof payload.tool === "string" ? payload.tool : "unknown",
      typeof requestId === "string" ? requestId : "",
      Number.isSafeInteger(version) ? version : 1, Number.isSafeInteger(seq) ? seq : 0,
      event.turn_id, typeof payload.turn_id === "string" ? payload.turn_id : null,
      Number.isSafeInteger(payload.model_call_index) ? payload.model_call_index as number : null,
      Number(event.cursor), now,
    );
  }

  /**
   * Records a child agent lifecycle report. A repeated bind (every runtime
   * rebuild restores and rebinds the same children) keeps the last reported
   * state rather than regressing it to pending.
   */
  observeAgent(report: { agentId: string; parentAgentId: string | null; role: string; state: string; bind?: boolean }): void {
    const id = Number(report.agentId);
    if (!Number.isSafeInteger(id)) return;
    if (report.bind && this.#storage.sql.exec(
      "SELECT 1 FROM managed_surface_agents WHERE agent_id = ? AND state != 'closed'", id).toArray().length > 0) return;
    const parent = report.parentAgentId === null ? null : Number(report.parentAgentId);
    this.#storage.sql.exec(
      `INSERT OR REPLACE INTO managed_surface_agents (agent_id, parent_agent_id, role, state, updated_at, epoch)
       VALUES (?, ?, ?, ?, ?, ?)`,
      id, parent !== null && Number.isSafeInteger(parent) ? parent : null, report.role.slice(0, 256),
      report.state, Date.now(), this.epoch());
  }

  /** An explicit close released the child. */
  closeAgent(agentId: string): void {
    const id = Number(agentId);
    if (!Number.isSafeInteger(id)) return;
    this.#storage.sql.exec("UPDATE managed_surface_agents SET state = 'closed', updated_at = ?, epoch = ? WHERE agent_id = ?",
      Date.now(), this.epoch(), id);
  }

  toolCalls(): SurfaceToolCall[] {
    return this.#storage.sql.exec<OpenCallRow>(
      "SELECT * FROM managed_surface_tool_calls ORDER BY cursor").toArray().map(toolCall);
  }

  runs(): SurfaceRun[] {
    return this.#storage.sql.exec<OpenRunRow>("SELECT * FROM managed_surface_runs ORDER BY cursor").toArray()
      .map(row => ({ agent_id: agentId(row.agent_id), turn_id: row.turn_id, state: "running" as const,
        started_cursor: String(row.cursor), started_at: row.started_at }));
  }

  agents(): SurfaceAgent[] {
    return this.#storage.sql.exec<AgentRow>("SELECT * FROM managed_surface_agents ORDER BY agent_id").toArray()
      .map(row => ({ agent_id: row.agent_id, parent_agent_id: row.parent_agent_id, role: row.role,
        state: row.state, updated_at: row.updated_at, reported_epoch: row.epoch }));
  }

  /**
   * Messages that close every lifecycle entity a lost runtime left open: the
   * epoch marker, then one unknown-outcome result per open call, newest first
   * so nested Code Mode calls settle before their cell. Returns nothing when no
   * call, run or turn was open (an idle eviction is not a visible restart).
   * The caller appends them in the same transaction; appending the results
   * removes the calls, and the open runs are closed here.
   */
  recover(activeTurns: readonly string[]): { message: RuntimeEpochMessage | EventMessage; turnId: string | null }[] {
    const calls = this.#storage.sql.exec<OpenCallRow>(
      "SELECT * FROM managed_surface_tool_calls ORDER BY cursor DESC").toArray();
    const runs = this.runs();
    if (calls.length === 0 && runs.length === 0 && activeTurns.length === 0) return [];
    const epoch = this.epoch() + 1;
    this.#storage.kv.put(EPOCH_KEY, epoch);
    this.#storage.sql.exec("DELETE FROM managed_surface_runs");
    const now = Date.now();
    const marker: RuntimeEpochMessage = {
      type: RUNTIME_EPOCH_MESSAGE, epoch, reason: "runtime_restarted",
      interrupted: {
        tool_calls: [...calls].reverse().map(toolCall),
        runs: runs.map(({ state: _state, ...run }) => run),
        turns: [...activeTurns],
      },
    };
    return [{ message: marker, turnId: null }, ...calls.map(call => ({
      turnId: call.turn_id,
      message: {
        type: "event" as const,
        event: {
          protocol_version: call.protocol_version,
          request_id: call.request_id,
          seq: call.seq,
          type: "tool.result",
          payload: {
            call_id: call.call_id,
            tool: call.tool,
            status: "failed",
            duration_ns: call.started_at > 0 ? Math.max(0, now - call.started_at) * 1_000_000 : 0,
            started_after_ns: null,
            result: INTERRUPTED_TOOL_CALL_ERROR,
            outcome_unknown: true,
            structured_result: {
              code: "TOOL_CALL_INTERRUPTED", error: INTERRUPTED_TOOL_CALL_ERROR, outcome: "unknown",
              reason: "runtime_restarted", epoch,
            },
            ...(call.runtime_turn_id === null ? {} : { turn_id: call.runtime_turn_id }),
            ...(call.model_call_index === null ? {} : { model_call_index: call.model_call_index }),
          },
        } as unknown as AgentEvent,
        ...(call.agent_id === ROOT_AGENT ? {} : { agent_id: call.agent_id }),
      },
    }))];
  }

  clear(): void {
    this.#storage.sql.exec("DELETE FROM managed_surface_tool_calls");
    this.#storage.sql.exec("DELETE FROM managed_surface_runs");
    this.#storage.sql.exec("DELETE FROM managed_surface_agents");
    this.#storage.kv.delete(EPOCH_KEY);
  }
}

/**
 * Whether a durable message is visible to a reader that did not opt into the
 * surface-state protocol (surface=1). Strict legacy decoders reject unknown
 * message types; the unknown-outcome tool results stay visible to everyone.
 */
export function legacyVisible(message: { type: string }): boolean {
  return message.type !== RUNTIME_EPOCH_MESSAGE;
}
