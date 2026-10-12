// Types mirror crates/nanocodex-surface-state (serde snake_case wire shape).

export type Lifecycle = "admitted" | "running" | "completed" | "failed" | "cancelled" | "unknown";
export type Provenance = "verified" | "receipt" | "agent_reported" | "test_build";
export type EntityKind =
  | "agent"
  | "turn"
  | "tool_call"
  | "process_session"
  | "run"
  | "hand"
  | "brain"
  | "decision"
  | "status";
export type Tone = "text" | "muted" | "info" | "live" | "ok" | "warn" | "error";
export type StatusSection = "done" | "in_progress" | "blocked" | "next";

export interface Summary {
  text: string;
  at_cursor: number;
  provenance: Provenance;
}

export interface Resources {
  observed_at_ms: number;
  cpu_pct?: number;
  ram_free_gb?: number;
  disk_free_gb?: number;
}

export interface StatusItem {
  section: StatusSection;
  text: string;
  provenance?: Provenance;
  evidence?: string;
}

interface EntityCommon {
  id: string;
  parent_id?: string;
  epoch: number;
  state: Lifecycle;
  title: string;
  subject?: string;
  reason?: string;
  started_at_ms?: number;
  updated_at_ms: number;
  provenance?: Provenance;
}

export type Entity = EntityCommon &
  (
    | { kind: "agent"; role?: string; summary?: Summary; context_pct?: number }
    | { kind: "turn"; prompt?: string }
    | {
        kind: "tool_call";
        tool: string;
        cwd?: string;
        hand_id?: string;
        exit_code?: number;
        output_bytes?: number;
      }
    | { kind: "process_session"; hand_id?: string; exit_code?: number }
    | { kind: "run" }
    | { kind: "hand"; online: boolean; os?: string; resources?: Resources }
    | { kind: "brain"; workspace: string; memory_notes?: number; dirty_files?: number }
    | {
        kind: "decision";
        question: string;
        recommendation?: string;
        options?: string[];
        chosen?: string;
      }
    | { kind: "status"; items: StatusItem[] }
  );

export type SurfaceEvent =
  | { type: "snapshot"; epoch: number; cursor: number; at_ms: number; entities: Entity[] }
  | { type: "upsert"; cursor: number; at_ms: number; entity: Entity }
  | { type: "runtime_epoch"; epoch: number; cursor: number; at_ms: number; reason: string }
  | { type: "heartbeat"; cursor: number; at_ms: number; context_pct?: number }
  | { type: "connection"; connected: boolean; at_ms: number };

export interface Badge {
  text: string;
  tone: Tone;
}

export interface CardLine {
  text: string;
  tone: Tone;
}

export interface Card {
  id: string;
  kind: EntityKind;
  depth: number;
  pinned: boolean;
  title: string;
  subject?: string;
  state: Lifecycle;
  state_label: string;
  tone: Tone;
  badges: Badge[];
  lines: CardLine[];
}

export interface Liveness {
  epoch: number;
  cursor: number;
  open: number;
  restarts: number;
  replays_ignored: number;
  regressions_ignored: number;
  context_pct?: number;
  label: string;
  tone: Tone;
}

export interface Projection {
  now_ms: number;
  cards: Card[];
  liveness: Liveness;
}

export interface EpochMark {
  epoch: number;
  at_ms: number;
  reason: string;
  settled: number;
}

export declare const STALE_RESOURCES_MS: number;
export declare const QUIET_MS: number;
export declare function isTerminal(state: Lifecycle): boolean;
export declare function isDefinitive(state: Lifecycle): boolean;
export declare function formatDuration(ms: number): string;

export declare class SurfaceState {
  readonly epoch: number;
  readonly cursor: number;
  readonly restarts: readonly EpochMark[];
  entity(id: string): Entity | undefined;
  /** Apply one event; replays and lifecycle regressions are counted and ignored. */
  apply(event: SurfaceEvent): void;
  project(nowMs: number): Projection;
}

export declare function projectEvents(events: readonly SurfaceEvent[], nowMs: number): Projection;
