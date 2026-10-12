//! Bridge from the managed event stream to [`SurfaceEvent`]s.
//!
//! The runtime does not yet publish surface events natively, so this adapter
//! maps what the wire already carries explicitly: lifecycle pairs
//! (`run.started` to `run.completed`/`run.failed`, `tool.call` to `tool.result`),
//! durable turn terminals, the `runtime_epoch` marker and the
//! `GET /v1/agents/:id/surface` snapshot of open entities. It never infers an
//! outcome: an entity stays running until its own terminal event, a newer
//! runtime epoch, or a snapshot that no longer lists it. Living in the shared
//! crate keeps every surface on one mapping instead of one heuristic each.

use crate::model::{Body, Entity, Lifecycle, Provenance, SurfaceEvent};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// Where an agent event came from.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventScope<'a> {
    /// Child agent identity; `None` for the root agent.
    pub agent_id: Option<u64>,
    /// Managed turn identity, when known.
    pub turn_id: Option<&'a str>,
}

/// `GET /v1/agents/:id/surface`: every open lifecycle entity, exact at
/// `cursor`. Mirrors the managed service's `SurfaceSnapshot` wire shape.
#[derive(Clone, Debug, Deserialize)]
pub struct ManagedSnapshot {
    pub epoch: u64,
    /// Cold constructions of the session runtime, idle restarts included. A
    /// change means retained processes belong to a lost runtime.
    #[serde(default)]
    pub runtime_generation: u64,
    pub cursor: String,
    pub generated_at: u64,
    #[serde(default)]
    pub turns: Vec<ManagedTurn>,
    #[serde(default)]
    pub runs: Vec<ManagedRun>,
    #[serde(default)]
    pub agents: Vec<ManagedAgent>,
    #[serde(default)]
    pub tool_calls: Vec<ManagedToolCall>,
    #[serde(default)]
    pub process_sessions: Vec<ManagedProcessSession>,
}

/// A non-terminal managed turn (`accepted` or `cancelling`).
#[derive(Clone, Debug, Deserialize)]
pub struct ManagedTurn {
    pub id: String,
    pub state: String,
    pub created_at: f64,
    pub updated_at: f64,
}

/// A run without a terminal event.
#[derive(Clone, Debug, Deserialize)]
pub struct ManagedRun {
    pub agent_id: Option<u64>,
    pub turn_id: Option<String>,
    pub started_at: u64,
}

/// A child agent's latest reported lifecycle state.
#[derive(Clone, Debug, Deserialize)]
pub struct ManagedAgent {
    pub agent_id: u64,
    pub parent_agent_id: Option<u64>,
    pub role: String,
    pub state: String,
    pub updated_at: u64,
    pub reported_epoch: u64,
}

/// A tool call without a terminal result.
#[derive(Clone, Debug, Deserialize)]
pub struct ManagedToolCall {
    pub agent_id: Option<u64>,
    pub call_id: String,
    pub tool: String,
    pub turn_id: Option<String>,
    pub parent_call_id: Option<String>,
    pub started_at: u64,
    /// Producer attempt (1 first; replays after a runtime epoch count up).
    #[serde(default)]
    pub attempt: Option<u64>,
}

/// A durable Hand process session that has not finished.
#[derive(Clone, Debug, Deserialize)]
pub struct ManagedProcessSession {
    pub id: u64,
    pub machine_id: Option<String>,
}

/// Stateful mapping from managed events to surface events.
#[derive(Clone, Debug, Default)]
pub struct AgentEventAdapter {
    cursor: u64,
    epoch: u64,
    /// Last seen `runtime_generation` from a snapshot.
    generation: Option<u64>,
    /// Process session id to the tool card that started it.
    process_parents: HashMap<u64, String>,
    turns: u64,
    root_turn: Option<String>,
    entities: HashMap<String, Entity>,
    /// First prompt line per card id (`turn:<managed id>` or `agent:<id>`).
    prompts: HashMap<String, String>,
    /// Current card id and attempt number per tool call identity. The
    /// runtime re-dispatches an interrupted call under the same call id, so a
    /// call seen again after it ended (or in a newer epoch) is a new attempt.
    attempts: HashMap<String, (String, u32)>,
    /// A root prompt accepted before its turn identity was known.
    pending_prompt: Option<String>,
    /// `spawn_agent` task per spawn call card, until its receipt names the agent.
    spawn_tasks: HashMap<String, String>,
    /// Agents whose card subject is their assigned task.
    tasked: std::collections::HashSet<String>,
}

/// A producer attempt card id: attempt 1 is the bare call id.
fn attempt_card(base: &str, attempt: u64) -> String {
    if attempt <= 1 {
        base.to_owned()
    } else {
        format!("{base}#{attempt}")
    }
}

/// `structured_result` as an object (some producers send it as JSON text).
fn structured_value(value: &Value) -> Value {
    value
        .as_str()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| value.clone())
}

/// A tool result's machine-readable body: the structured result, or the JSON
/// text a provider returned.
fn result_value(payload: &Value) -> Value {
    let structured = &payload["structured_result"];
    let value = if structured.is_null() {
        &payload["result"]
    } else {
        structured
    };
    value
        .as_str()
        .or_else(|| value["text"].as_str())
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| value.clone())
}

fn friendly_tool(tool: &str) -> &str {
    match tool {
        "exec_command" | "shell" | "Bash" => "Shell",
        "exec" => "Code Mode",
        "write_stdin" => "Process input",
        "apply_patch" | "Edit" | "Write" => "Edit files",
        "spawn_agent" => "Spawn agent",
        "send_agent_message" => "Message agent",
        "wait_agent" => "Wait for agent",
        other => other,
    }
}

fn first_line(text: &str, limit: usize) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.chars().count() > limit {
        let mut out: String = line.chars().take(limit.saturating_sub(1)).collect();
        out.push('…');
        out
    } else {
        line.to_owned()
    }
}

/// Prompt text from a string or a list of content parts (web/websocket
/// prompts arrive as `[{"type":"text","text":...}]`).
fn input_text(input: &Value) -> Option<String> {
    match input {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let text: Vec<&str> = parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .filter(|text| !text.trim_start().starts_with("<current_request_context>"))
                .collect();
            (!text.is_empty()).then(|| {
                text.join(
                    "
",
                )
            })
        }
        Value::Object(_) => input["text"].as_str().map(str::to_owned),
        _ => None,
    }
}

/// The user's prompt without the service-appended request context.
fn prompt_line(input: &str) -> Option<String> {
    let prompt = input
        .split("<current_request_context>")
        .next()
        .unwrap_or(input);
    let line = first_line(prompt, 120);
    (!line.is_empty()).then_some(line)
}

fn arguments(payload: &Value) -> Value {
    match &payload["arguments"] {
        Value::String(text) => serde_json::from_str(text).unwrap_or(Value::String(text.clone())),
        other => other.clone(),
    }
}

fn owner_key(agent_id: Option<u64>) -> String {
    agent_id.map_or_else(|| "root".to_owned(), |id| id.to_string())
}

fn agent_lifecycle(state: &str) -> Lifecycle {
    match state {
        "pending" => Lifecycle::Admitted,
        "running" | "closing" => Lifecycle::Running,
        "completed" | "closed" => Lifecycle::Completed,
        "failed" => Lifecycle::Failed,
        "cancelled" => Lifecycle::Cancelled,
        _ => Lifecycle::Unknown,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "service timestamps are non-negative Unix milliseconds"
)]
const fn millis(value: f64) -> u64 {
    value as u64
}

const fn agent_body() -> Body {
    Body::Agent {
        role: None,
        summary: None,
        context_pct: None,
    }
}

impl AgentEventAdapter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    const fn next_cursor(&mut self) -> u64 {
        self.cursor += 1;
        self.cursor
    }

    fn upsert(&mut self, entity: Entity, at_ms: u64) -> SurfaceEvent {
        self.entities.insert(entity.id.clone(), entity.clone());
        SurfaceEvent::Upsert {
            cursor: self.next_cursor(),
            at_ms,
            entity,
            stream: None,
        }
    }

    fn turn_key(&mut self, turn_id: Option<&str>) -> String {
        turn_id.map_or_else(
            || {
                self.turns += 1;
                format!("turn:{}", self.turns)
            },
            |turn| format!("turn:{turn}"),
        )
    }

    fn scope_parent(&self, scope: EventScope<'_>) -> Option<String> {
        scope.agent_id.map_or_else(
            || {
                scope
                    .turn_id
                    .map(|turn| format!("turn:{turn}"))
                    .filter(|turn| self.entities.contains_key(turn))
                    .or_else(|| self.root_turn.clone())
            },
            |id| Some(format!("agent:{id}")),
        )
    }

    /// The card id of the latest attempt of tool call `base`.
    fn tool_card(&self, base: &str) -> String {
        self.attempts
            .get(base)
            .map_or_else(|| base.to_owned(), |(id, _)| id.clone())
    }

    fn subject_for(&self, id: &str) -> Option<String> {
        self.prompts.get(id).cloned().or_else(|| {
            self.entities
                .get(id)
                .and_then(|entity| entity.subject.clone())
        })
    }

    /// A workspace (Brain) card for the surface's configured workspace.
    pub fn workspace(&mut self, workspace: &str, at_ms: u64) -> SurfaceEvent {
        let entity = Entity {
            id: "brain".to_owned(),
            parent_id: None,
            epoch: self.epoch,
            state: Lifecycle::Running,
            title: "Workspace".to_owned(),
            subject: None,
            reason: None,
            started_at_ms: None,
            updated_at_ms: at_ms,
            provenance: None,
            body: Body::Brain {
                workspace: workspace.to_owned(),
                memory_notes: None,
                dirty_files: None,
            },
        };
        self.upsert(entity, at_ms)
    }

    /// Map one managed event envelope (the JSON of a history page entry or an
    /// SSE `data:` frame) to surface events.
    pub fn observe_managed(&mut self, event: &Value) -> Vec<SurfaceEvent> {
        let at_ms = event["created_at"].as_u64().unwrap_or(0);
        let turn_id = event["turn_id"].as_str();
        match event["type"].as_str() {
            Some("event") => {
                let nested = &event["event"];
                let Some(kind) = nested["type"].as_str() else {
                    return Vec::new();
                };
                let scope = EventScope {
                    agent_id: event["agent_id"].as_u64(),
                    turn_id,
                };
                self.observe(kind, &nested["payload"], scope, at_ms)
            }
            Some("runtime_epoch") => {
                self.observe("runtime_epoch", event, EventScope::default(), at_ms)
            }
            Some("turn_accepted") => {
                if let (Some(turn), Some(prompt)) = (
                    turn_id,
                    input_text(&event["input"]).as_deref().and_then(prompt_line),
                ) {
                    self.prompts.insert(format!("turn:{turn}"), prompt);
                }
                Vec::new()
            }
            Some("turn_completed") => self.finish_turn(turn_id, Lifecycle::Completed, None, at_ms),
            Some("turn_failed") => {
                let error = event["error"].as_str().map(|error| first_line(error, 160));
                self.finish_turn(turn_id, Lifecycle::Failed, error, at_ms)
            }
            Some("turn_cancelled") => self.finish_turn(turn_id, Lifecycle::Cancelled, None, at_ms),
            _ => Vec::new(),
        }
    }

    fn finish_turn(
        &mut self,
        turn_id: Option<&str>,
        state: Lifecycle,
        reason: Option<String>,
        at_ms: u64,
    ) -> Vec<SurfaceEvent> {
        let id = match turn_id {
            Some(turn) => format!("turn:{turn}"),
            None => match &self.root_turn {
                Some(turn) => turn.clone(),
                None => return Vec::new(),
            },
        };
        let Some(mut entity) = self.entities.get(&id).cloned() else {
            return Vec::new();
        };
        if entity.state.is_definitive() {
            return Vec::new();
        }
        entity.state = state;
        entity.reason = reason;
        entity.updated_at_ms = at_ms;
        entity.epoch = self.epoch.max(entity.epoch);
        vec![self.upsert(entity, at_ms)]
    }

    /// Map one agent event (`kind` plus decoded payload) to surface events.
    #[allow(clippy::too_many_lines, reason = "one arm per wire event kind")]
    pub fn observe(
        &mut self,
        kind: &str,
        payload: &Value,
        scope: EventScope<'_>,
        at_ms: u64,
    ) -> Vec<SurfaceEvent> {
        match kind {
            "input.accepted" => {
                let Some(prompt) = input_text(&payload["input"])
                    .as_deref()
                    .and_then(prompt_line)
                else {
                    return Vec::new();
                };
                let id = match (scope.agent_id, scope.turn_id) {
                    (Some(agent), _) => format!("agent:{agent}"),
                    (None, Some(turn)) => format!("turn:{turn}"),
                    (None, None) => {
                        self.pending_prompt = Some(prompt);
                        return Vec::new();
                    }
                };
                // A delegated child's first input is runtime boilerplate; its
                // assigned task (from the spawn) is the better subject.
                if self.tasked.contains(&id) {
                    return Vec::new();
                }
                self.prompts.insert(id.clone(), prompt.clone());
                match self.entities.get(&id).cloned() {
                    Some(mut entity) if entity.subject.as_deref() != Some(prompt.as_str()) => {
                        entity.subject = Some(prompt);
                        entity.updated_at_ms = at_ms;
                        vec![self.upsert(entity, at_ms)]
                    }
                    _ => Vec::new(),
                }
            }
            "runtime.epoch" | "runtime_epoch" => {
                let Some(epoch) = payload["epoch"].as_u64() else {
                    return Vec::new();
                };
                if epoch <= self.epoch {
                    return Vec::new();
                }
                self.epoch = epoch;
                let reason = match payload["reason"].as_str() {
                    None | Some("runtime_restarted") => "runtime restarted",
                    Some(other) => other,
                }
                .to_owned();
                vec![SurfaceEvent::RuntimeEpoch {
                    epoch,
                    cursor: self.next_cursor(),
                    at_ms,
                    reason,
                    stream: None,
                }]
            }
            "run.started" => {
                let entity = if let Some(agent) = scope.agent_id {
                    let id = format!("agent:{agent}");
                    let previous = self.entities.get(&id);
                    Entity {
                        parent_id: previous
                            .and_then(|entity| entity.parent_id.clone())
                            .or_else(|| {
                                self.scope_parent(EventScope {
                                    agent_id: None,
                                    ..scope
                                })
                            }),
                        epoch: self.epoch,
                        state: Lifecycle::Running,
                        title: previous.map_or_else(
                            || format!("Agent {agent}"),
                            |entity| entity.title.clone(),
                        ),
                        subject: self.subject_for(&id),
                        reason: None,
                        started_at_ms: Some(at_ms),
                        updated_at_ms: at_ms,
                        provenance: None,
                        body: previous.map_or_else(agent_body, |entity| entity.body.clone()),
                        id,
                    }
                } else {
                    let id = self.turn_key(scope.turn_id);
                    self.root_turn = Some(id.clone());
                    Entity {
                        parent_id: None,
                        epoch: self.epoch,
                        state: Lifecycle::Running,
                        title: "Turn".to_owned(),
                        subject: self.subject_for(&id).or_else(|| self.pending_prompt.take()),
                        reason: None,
                        started_at_ms: self
                            .entities
                            .get(&id)
                            .and_then(|entity| entity.started_at_ms)
                            .or(Some(at_ms)),
                        updated_at_ms: at_ms,
                        provenance: None,
                        body: Body::Turn { prompt: None },
                        id,
                    }
                };
                vec![self.upsert(entity, at_ms)]
            }
            // Durable managed turn terminals as projected by the TUI history,
            // delivered even when the run's own terminal event was lost.
            "managed.final_message" | "managed.turn_stopped" => {
                let turn = payload["turn_id"].as_str().or(scope.turn_id);
                let (state, reason) = match (kind, payload["error"].as_str()) {
                    ("managed.final_message", _) => (Lifecycle::Completed, None),
                    (_, Some(error)) => (Lifecycle::Failed, Some(first_line(error, 160))),
                    (_, None) => (Lifecycle::Cancelled, None),
                };
                let turn = turn.filter(|turn| self.entities.contains_key(&format!("turn:{turn}")));
                self.finish_turn(turn, state, reason, at_ms)
            }
            "run.completed" | "run.failed" => {
                let id = match scope.agent_id {
                    Some(agent) => format!("agent:{agent}"),
                    None => match scope
                        .turn_id
                        .map(|turn| format!("turn:{turn}"))
                        .filter(|turn| self.entities.contains_key(turn))
                        .or_else(|| self.root_turn.clone())
                    {
                        Some(turn) => turn,
                        None => return Vec::new(),
                    },
                };
                let Some(mut entity) = self.entities.get(&id).cloned() else {
                    return Vec::new();
                };
                let child = scope.agent_id.is_some();
                entity.state = if kind == "run.completed" && child {
                    // A child's run ending is not its assignment outcome; that
                    // arrives as a lifecycle report (wait/list/close_agent,
                    // `/surface` agents[].state).
                    Lifecycle::Unknown
                } else if kind == "run.completed" {
                    Lifecycle::Completed
                } else if payload["status"] == "cancelled" {
                    Lifecycle::Cancelled
                } else {
                    Lifecycle::Failed
                };
                entity.reason = payload["error"]
                    .as_str()
                    .map(|error| first_line(error, 160))
                    .or_else(|| {
                        (kind == "run.completed" && child)
                            .then(|| "run ended; assignment outcome not reported yet".to_owned())
                    });
                entity.updated_at_ms = at_ms;
                entity.epoch = self.epoch.max(entity.epoch);
                vec![self.upsert(entity, at_ms)]
            }
            "tool.call" => {
                let Some(call_id) = payload["call_id"].as_str() else {
                    return Vec::new();
                };
                let tool = payload["tool"].as_str().unwrap_or("tool");
                let owner = owner_key(scope.agent_id);
                let parent_id = call_id
                    .rfind("/code-")
                    .map(|index| self.tool_card(&format!("tool:{owner}:{}", &call_id[..index])))
                    .filter(|parent| self.entities.contains_key(parent))
                    .or_else(|| self.scope_parent(scope));
                let base = format!("tool:{owner}:{call_id}");
                let producer_attempt = payload["attempt"].as_u64();
                let (id, attempt) = match (producer_attempt, self.attempts.get(&base)) {
                    (Some(attempt), _) => (
                        attempt_card(&base, attempt),
                        u32::try_from(attempt).unwrap_or(u32::MAX),
                    ),
                    (None, current) => match current {
                        Some((current, attempt))
                            if self.entities.get(current).is_some_and(|entity| {
                                entity.state.is_terminal() || entity.epoch < self.epoch
                            }) =>
                        {
                            (format!("{base}#{}", attempt + 1), attempt + 1)
                        }
                        Some((current, attempt)) => (current.clone(), *attempt),
                        None => (base.clone(), 1),
                    },
                };
                self.attempts.insert(base, (id.clone(), attempt));
                if tool == "spawn_agent"
                    && let Some(task) = arguments(payload)["task"]
                        .as_str()
                        .map(|task| first_line(task, 120))
                {
                    self.spawn_tasks.insert(id.clone(), task);
                }
                let args = arguments(payload);
                let subject = args["cmd"]
                    .as_str()
                    .or_else(|| args["command"].as_str())
                    .or_else(|| args["code"].as_str())
                    .or_else(|| args["file_path"].as_str())
                    .or_else(|| args["path"].as_str())
                    .or_else(|| args["query"].as_str())
                    // Freeform tools (Code Mode) carry their source as a string.
                    .or_else(|| args.as_str())
                    .map(|text| first_line(text, 120));
                let entity = Entity {
                    id,
                    parent_id,
                    epoch: self.epoch,
                    state: Lifecycle::Running,
                    title: friendly_tool(tool).to_owned(),
                    subject,
                    reason: (attempt > 1).then(|| {
                        format!("attempt {attempt}: re-dispatched under the same call id")
                    }),
                    started_at_ms: Some(at_ms),
                    updated_at_ms: at_ms,
                    provenance: None,
                    body: Body::ToolCall {
                        tool: tool.to_owned(),
                        cwd: args["workdir"].as_str().map(str::to_owned),
                        hand_id: None,
                        exit_code: None,
                        output_bytes: None,
                    },
                };
                vec![self.upsert(entity, at_ms)]
            }
            "tool.result" => {
                let Some(call_id) = payload["call_id"].as_str() else {
                    return Vec::new();
                };
                let base = format!("tool:{}:{call_id}", owner_key(scope.agent_id));
                let structured_owned = structured_value(&payload["structured_result"]);
                let structured = &structured_owned;
                let id = payload["attempt"]
                    .as_u64()
                    .or_else(|| structured["attempt"].as_u64())
                    .map_or_else(
                        || self.tool_card(&base),
                        |attempt| attempt_card(&base, attempt),
                    );
                let Some(mut entity) = self.entities.get(&id).cloned() else {
                    return Vec::new();
                };
                let mut events = Vec::new();
                // The restart receipt names the epoch that settled the call;
                // surfaces whose stream omits `runtime_epoch` learn it here.
                if structured["reason"] == "runtime_restarted"
                    && let Some(epoch) = structured["epoch"].as_u64()
                {
                    events.extend(self.observe(
                        "runtime_epoch",
                        &serde_json::json!({"epoch": epoch, "reason": "runtime_restarted"}),
                        EventScope::default(),
                        at_ms,
                    ));
                }
                let unknown = payload["outcome_unknown"] == true
                    || structured["outcome_unknown"] == true
                    || structured["outcome"] == "unknown"
                    || payload["status"] == "unknown";
                entity.state = if unknown {
                    Lifecycle::Unknown
                } else {
                    match payload["status"].as_str() {
                        Some("failed") => Lifecycle::Failed,
                        Some("cancelled") => Lifecycle::Cancelled,
                        _ => Lifecycle::Completed,
                    }
                };
                if unknown {
                    entity.reason = Some(if structured["reason"] == "runtime_restarted" {
                        "runtime restarted".to_owned()
                    } else {
                        "runtime reported the outcome as unknown".to_owned()
                    });
                }
                // A definitive tool.result is a receipt that the call returned;
                // it verifies no claim. An unknown-outcome result is neither.
                if !unknown {
                    entity.provenance = Some(Provenance::Receipt);
                }
                entity.updated_at_ms = at_ms;
                if !unknown
                    && let Body::ToolCall {
                        exit_code,
                        output_bytes,
                        ..
                    } = &mut entity.body
                {
                    *exit_code = structured["exit_code"].as_i64();
                    *output_bytes = structured["output"]
                        .as_str()
                        .or_else(|| payload["result"].as_str())
                        .map(|text| text.len() as u64);
                }
                let task = self.spawn_tasks.remove(&entity.id);
                // A shell that keeps running past its call returns a process
                // session; that process becomes its own card under the call.
                let process = (!unknown)
                    .then(|| structured["session_id"].as_u64())
                    .flatten()
                    .map(|session| (session, structured["exit_code"].as_i64()));
                let call_card = entity.id.clone();
                let command = entity.subject.clone();
                events.push(self.upsert(entity, at_ms));
                if let Some((session, exit_code)) = process {
                    events.extend(
                        self.process_report(session, exit_code, &call_card, command, at_ms),
                    );
                }
                let reporting = matches!(
                    payload["tool"].as_str(),
                    Some(
                        "spawn_agent"
                            | "wait_agent"
                            | "list_agents"
                            | "close_agent"
                            | "interrupt_agent"
                    )
                );
                if reporting && !unknown {
                    let report = result_value(payload);
                    match report["agents"].as_array() {
                        Some(agents) => {
                            for agent in agents {
                                events.extend(self.agent_report(agent, None, at_ms));
                            }
                        }
                        None => events.extend(self.agent_report(&report, task, at_ms)),
                    }
                }
                events
            }
            _ => Vec::new(),
        }
    }

    /// Map a surface snapshot (`GET /v1/agents/:id/surface`) to one authoritative
    /// [`SurfaceEvent::Snapshot`]. Apply it after the history it supersedes:
    /// entities it no longer lists are no longer open.
    pub fn snapshot(&mut self, snapshot: &ManagedSnapshot) -> SurfaceEvent {
        let at_ms = snapshot.generated_at;
        self.epoch = self.epoch.max(snapshot.epoch);
        let epoch = snapshot.epoch;
        let mut entities: Vec<Entity> = Vec::new();
        let root_runs: HashMap<&str, u64> = snapshot
            .runs
            .iter()
            .filter(|run| run.agent_id.is_none())
            .filter_map(|run| Some((run.turn_id.as_deref()?, run.started_at)))
            .collect();
        for turn in &snapshot.turns {
            let id = format!("turn:{}", turn.id);
            self.root_turn = Some(id.clone());
            entities.push(Entity {
                subject: self.subject_for(&id),
                parent_id: None,
                epoch,
                state: Lifecycle::Running,
                title: "Turn".to_owned(),
                reason: (turn.state == "cancelling").then(|| "cancelling".to_owned()),
                started_at_ms: Some(
                    root_runs
                        .get(turn.id.as_str())
                        .copied()
                        .unwrap_or_else(|| millis(turn.created_at)),
                ),
                updated_at_ms: millis(turn.updated_at),
                provenance: None,
                body: Body::Turn { prompt: None },
                id,
            });
        }
        let turn_parent = |turn: Option<&str>, entities: &[Entity]| {
            turn.map(|turn| format!("turn:{turn}"))
                .filter(|id| entities.iter().any(|entity| &entity.id == id))
        };
        let child_runs: HashMap<u64, &ManagedRun> = snapshot
            .runs
            .iter()
            .filter_map(|run| Some((run.agent_id?, run)))
            .collect();
        for agent in &snapshot.agents {
            let id = format!("agent:{}", agent.agent_id);
            let run = child_runs.get(&agent.agent_id);
            let mut state = agent_lifecycle(&agent.state);
            let mut reason = None;
            if !state.is_terminal() && agent.reported_epoch < snapshot.epoch {
                state = Lifecycle::Unknown;
                reason = Some("last report predates the runtime restart".to_owned());
            }
            let parent_id = agent
                .parent_agent_id
                .map(|parent| format!("agent:{parent}"))
                .or_else(|| turn_parent(run.and_then(|run| run.turn_id.as_deref()), &entities))
                .or_else(|| self.root_turn.clone());
            entities.push(Entity {
                subject: self.subject_for(&id),
                parent_id,
                epoch: agent.reported_epoch,
                state,
                title: agent.role.clone(),
                reason,
                started_at_ms: run.map(|run| run.started_at),
                updated_at_ms: agent.updated_at,
                provenance: None,
                body: Body::Agent {
                    role: Some(agent.role.clone()),
                    summary: None,
                    context_pct: None,
                },
                id,
            });
        }
        for (agent_id, run) in &child_runs {
            let id = format!("agent:{agent_id}");
            if entities.iter().any(|entity| entity.id == id) {
                continue;
            }
            entities.push(Entity {
                title: self.entities.get(&id).map_or_else(
                    || format!("Agent {agent_id}"),
                    |entity| entity.title.clone(),
                ),
                subject: self.subject_for(&id),
                parent_id: turn_parent(run.turn_id.as_deref(), &entities)
                    .or_else(|| self.root_turn.clone()),
                epoch,
                state: Lifecycle::Running,
                reason: None,
                started_at_ms: Some(run.started_at),
                updated_at_ms: run.started_at,
                provenance: None,
                body: agent_body(),
                id,
            });
        }
        for call in &snapshot.tool_calls {
            let owner = owner_key(call.agent_id);
            let base = format!("tool:{owner}:{}", call.call_id);
            let id = call.attempt.map_or_else(
                || self.tool_card(&base),
                |attempt| attempt_card(&base, attempt),
            );
            if let Some(attempt) = call.attempt {
                self.attempts.insert(
                    base,
                    (id.clone(), u32::try_from(attempt).unwrap_or(u32::MAX)),
                );
            }
            let previous = self.entities.get(&id);
            let parent_id = call
                .parent_call_id
                .as_ref()
                .map(|parent| self.tool_card(&format!("tool:{owner}:{parent}")))
                .or_else(|| call.agent_id.map(|agent| format!("agent:{agent}")))
                .or_else(|| turn_parent(call.turn_id.as_deref(), &entities))
                .or_else(|| self.root_turn.clone());
            entities.push(Entity {
                parent_id,
                epoch,
                state: Lifecycle::Running,
                title: friendly_tool(&call.tool).to_owned(),
                subject: previous.and_then(|entity| entity.subject.clone()),
                reason: None,
                started_at_ms: Some(call.started_at),
                updated_at_ms: call.started_at,
                provenance: None,
                body: previous.map_or_else(
                    || Body::ToolCall {
                        tool: call.tool.clone(),
                        cwd: None,
                        hand_id: None,
                        exit_code: None,
                        output_bytes: None,
                    },
                    |entity| entity.body.clone(),
                ),
                id,
            });
        }
        // A new runtime generation means retained processes belong to a lost
        // runtime: they are listed, but nothing proves they still run.
        let replaced = self
            .generation
            .is_some_and(|known| snapshot.runtime_generation > known);
        self.generation = Some(snapshot.runtime_generation);
        for process in &snapshot.process_sessions {
            let id = format!("process:{}", process.id);
            let previous = self.entities.get(&id);
            entities.push(Entity {
                subject: previous.and_then(|entity| entity.subject.clone()),
                parent_id: self
                    .process_parents
                    .get(&process.id)
                    .cloned()
                    .or_else(|| previous.and_then(|entity| entity.parent_id.clone())),
                epoch,
                state: if replaced {
                    Lifecycle::Unknown
                } else {
                    Lifecycle::Running
                },
                title: format!("Process {}", process.id),
                reason: replaced.then(|| {
                    format!(
                        "runtime replaced (generation {}); process may belong to the lost runtime",
                        snapshot.runtime_generation
                    )
                }),
                started_at_ms: None,
                updated_at_ms: at_ms,
                provenance: None,
                body: Body::ProcessSession {
                    hand_id: process.machine_id.clone(),
                    exit_code: None,
                },
                id,
            });
        }
        for entity in &entities {
            self.entities.insert(entity.id.clone(), entity.clone());
        }
        SurfaceEvent::Snapshot {
            epoch,
            cursor: self.next_cursor(),
            at_ms,
            entities,
        }
    }

    /// A process session reported by a shell call result: running while no exit
    /// code is known, then completed or failed with it.
    fn process_report(
        &mut self,
        session: u64,
        exit_code: Option<i64>,
        call_card: &str,
        command: Option<String>,
        at_ms: u64,
    ) -> Option<SurfaceEvent> {
        let id = format!("process:{session}");
        let parent = self
            .process_parents
            .entry(session)
            .or_insert_with(|| call_card.to_owned())
            .clone();
        let previous = self.entities.get(&id);
        let state = match exit_code {
            None => Lifecycle::Running,
            Some(0) => Lifecycle::Completed,
            Some(_) => Lifecycle::Failed,
        };
        let entity = Entity {
            subject: previous
                .and_then(|entity| entity.subject.clone())
                .or(command),
            parent_id: Some(parent),
            epoch: self.epoch,
            state,
            title: format!("Process {session}"),
            reason: None,
            started_at_ms: previous
                .and_then(|entity| entity.started_at_ms)
                .or(Some(at_ms)),
            updated_at_ms: at_ms,
            provenance: exit_code.map(|_| Provenance::Receipt),
            body: Body::ProcessSession {
                hand_id: None,
                exit_code,
            },
            id,
        };
        if previous.is_some_and(|previous| previous.state == entity.state) && exit_code.is_none() {
            return None;
        }
        Some(self.upsert(entity, at_ms))
    }

    /// Apply one subagent lifecycle report (`{agent_id, role, status: {state,
    /// error}}`) from a spawn/wait/list/close receipt: role as title, task as
    /// subject, and the reported assignment state.
    fn agent_report(
        &mut self,
        report: &Value,
        task: Option<String>,
        at_ms: u64,
    ) -> Vec<SurfaceEvent> {
        let Some(agent) = report["agent_id"].as_u64() else {
            return Vec::new();
        };
        let id = format!("agent:{agent}");
        let mut events = Vec::new();
        if let Some(role) = report["role"].as_str() {
            events.extend(self.agent_role(agent, role, at_ms));
        }
        let Some(mut entity) = self.entities.get(&id).cloned() else {
            return events;
        };
        let mut changed = false;
        let task = task.or_else(|| report["task"].as_str().map(|task| first_line(task, 120)));
        if let Some(task) = task.filter(|task| entity.subject.as_deref() != Some(task.as_str())) {
            self.prompts.insert(id.clone(), task.clone());
            self.prompts.insert(id.clone(), task.clone());
            self.tasked.insert(id);
            entity.subject = Some(task);
            changed = true;
        }
        let status = &report["status"];
        if let Some(state) = status["state"]
            .as_str()
            .or_else(|| report["state"].as_str())
        {
            let next = agent_lifecycle(state);
            if next != entity.state {
                entity.state = next;
                entity.reason = status["error"]
                    .as_str()
                    .or_else(|| report["error"].as_str())
                    .map(|error| first_line(error, 160));
                entity.epoch = self.epoch.max(entity.epoch);
                changed = true;
            }
        }
        if changed {
            entity.updated_at_ms = at_ms;
            events.push(self.upsert(entity, at_ms));
        }
        events
    }

    /// Record a subagent's role as its card title, when a surface learns it.
    pub fn agent_role(&mut self, agent_id: u64, role: &str, at_ms: u64) -> Option<SurfaceEvent> {
        let id = format!("agent:{agent_id}");
        let known = self.entities.contains_key(&id);
        let mut entity = self.entities.get(&id).cloned().unwrap_or_else(|| Entity {
            id: id.clone(),
            parent_id: self.root_turn.clone(),
            epoch: self.epoch,
            state: Lifecycle::Admitted,
            title: role.to_owned(),
            subject: self.prompts.get(&id).cloned(),
            reason: None,
            started_at_ms: None,
            updated_at_ms: at_ms,
            provenance: None,
            body: agent_body(),
        });
        if known && entity.title == role {
            return None;
        }
        entity.title = role.to_owned();
        if let Body::Agent { role: slot, .. } = &mut entity.body {
            *slot = Some(role.to_owned());
        }
        entity.updated_at_ms = at_ms;
        Some(self.upsert(entity, at_ms))
    }
}
