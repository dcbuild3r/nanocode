//! Bridge from today's agent event stream to [`SurfaceEvent`]s.
//!
//! Until the runtime publishes surface events natively, this adapter maps the
//! explicit lifecycle pairs already on the wire (`run.started` →
//! `run.completed`/`run.failed`, `tool.call` → `tool.result`) and the
//! runtime epoch marker into entity upserts. It never infers an outcome: an
//! entity stays running until its own terminal event or a newer runtime epoch
//! settles it to `unknown`. Living in the shared crate keeps every surface on
//! one mapping instead of one heuristic per surface.

use crate::model::{Body, Entity, Lifecycle, Provenance, SurfaceEvent};
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

/// Stateful mapping from agent events to surface events.
#[derive(Clone, Debug, Default)]
pub struct AgentEventAdapter {
    cursor: u64,
    epoch: u64,
    turns: u64,
    root_turn: Option<String>,
    entities: HashMap<String, Entity>,
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

fn arguments(payload: &Value) -> Value {
    match &payload["arguments"] {
        Value::String(text) => serde_json::from_str(text).unwrap_or(Value::String(text.clone())),
        other => other.clone(),
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
        }
    }

    fn scope_parent(&self, scope: EventScope<'_>) -> Option<String> {
        scope
            .agent_id
            .map_or_else(|| self.root_turn.clone(), |id| Some(format!("agent:{id}")))
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

    /// Map one agent event (`kind` plus decoded payload) to surface events.
    pub fn observe(
        &mut self,
        kind: &str,
        payload: &Value,
        scope: EventScope<'_>,
        at_ms: u64,
    ) -> Vec<SurfaceEvent> {
        match kind {
            "runtime.epoch" | "runtime_epoch" => {
                let Some(epoch) = payload["epoch"].as_u64() else {
                    return Vec::new();
                };
                if epoch <= self.epoch {
                    return Vec::new();
                }
                self.epoch = epoch;
                let reason = payload["reason"]
                    .as_str()
                    .map_or("runtime restarted", |reason| match reason {
                        "runtime_restarted" => "runtime restarted",
                        other => other,
                    })
                    .to_owned();
                vec![SurfaceEvent::RuntimeEpoch {
                    epoch,
                    cursor: self.next_cursor(),
                    at_ms,
                    reason,
                }]
            }
            "run.started" => {
                let entity = if let Some(agent) = scope.agent_id {
                    let id = format!("agent:{agent}");
                    let previous = self.entities.get(&id);
                    Entity {
                        parent_id: self.root_turn.clone(),
                        epoch: self.epoch,
                        state: Lifecycle::Running,
                        title: previous.map_or_else(
                            || format!("Agent {agent}"),
                            |entity| entity.title.clone(),
                        ),
                        subject: payload["model"].as_str().map(str::to_owned),
                        reason: None,
                        started_at_ms: Some(at_ms),
                        updated_at_ms: at_ms,
                        provenance: None,
                        body: previous.map_or(
                            Body::Agent {
                                role: None,
                                summary: None,
                                context_pct: None,
                            },
                            |entity| entity.body.clone(),
                        ),
                        id,
                    }
                } else {
                    self.turns += 1;
                    let id = scope.turn_id.map_or_else(
                        || format!("turn:{}", self.turns),
                        |turn| format!("turn:{turn}"),
                    );
                    self.root_turn = Some(id.clone());
                    Entity {
                        id,
                        parent_id: None,
                        epoch: self.epoch,
                        state: Lifecycle::Running,
                        title: "Turn".to_owned(),
                        subject: payload["model"].as_str().map(str::to_owned),
                        reason: None,
                        started_at_ms: Some(at_ms),
                        updated_at_ms: at_ms,
                        provenance: None,
                        body: Body::Turn { prompt: None },
                    }
                };
                vec![self.upsert(entity, at_ms)]
            }
            "run.completed" | "run.failed" => {
                let id = match scope.agent_id {
                    Some(agent) => format!("agent:{agent}"),
                    None => match &self.root_turn {
                        Some(turn) => turn.clone(),
                        None => return Vec::new(),
                    },
                };
                let Some(mut entity) = self.entities.get(&id).cloned() else {
                    return Vec::new();
                };
                entity.state = if kind == "run.completed" {
                    Lifecycle::Completed
                } else if payload["status"] == "cancelled" {
                    Lifecycle::Cancelled
                } else {
                    Lifecycle::Failed
                };
                entity.reason = payload["error"]
                    .as_str()
                    .map(|error| first_line(error, 160));
                entity.updated_at_ms = at_ms;
                entity.epoch = self.epoch.max(entity.epoch);
                vec![self.upsert(entity, at_ms)]
            }
            "tool.call" => {
                let Some(call_id) = payload["call_id"].as_str() else {
                    return Vec::new();
                };
                let tool = payload["tool"].as_str().unwrap_or("tool");
                let owner = scope
                    .agent_id
                    .map_or_else(|| "root".to_owned(), |id| id.to_string());
                let parent_id = call_id
                    .rfind("/code-")
                    .map(|index| format!("tool:{owner}:{}", &call_id[..index]))
                    .filter(|parent| self.entities.contains_key(parent))
                    .or_else(|| self.scope_parent(scope));
                let args = arguments(payload);
                let subject = args["cmd"]
                    .as_str()
                    .or_else(|| args["command"].as_str())
                    .or_else(|| args["code"].as_str())
                    .or_else(|| args["file_path"].as_str())
                    .or_else(|| args["path"].as_str())
                    .or_else(|| args["query"].as_str())
                    .map(|text| first_line(text, 120));
                let entity = Entity {
                    id: format!("tool:{owner}:{call_id}"),
                    parent_id,
                    epoch: self.epoch,
                    state: Lifecycle::Running,
                    title: friendly_tool(tool).to_owned(),
                    subject,
                    reason: None,
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
                let owner = scope
                    .agent_id
                    .map_or_else(|| "root".to_owned(), |id| id.to_string());
                let id = format!("tool:{owner}:{call_id}");
                let Some(mut entity) = self.entities.get(&id).cloned() else {
                    return Vec::new();
                };
                let structured = &payload["structured_result"];
                let unknown = payload["outcome_unknown"] == true
                    || structured["outcome_unknown"] == true
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
                    entity.reason = Some("runtime reported the outcome as unknown".to_owned());
                }
                // A tool.result is a runtime receipt for the call itself.
                entity.provenance = Some(Provenance::Verified);
                entity.updated_at_ms = at_ms;
                if let Body::ToolCall {
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
                vec![self.upsert(entity, at_ms)]
            }
            _ => Vec::new(),
        }
    }

    /// Record a subagent's role as its card title, when a surface learns it.
    pub fn agent_role(&mut self, agent_id: u64, role: &str, at_ms: u64) -> Option<SurfaceEvent> {
        let id = format!("agent:{agent_id}");
        let mut entity = self.entities.get(&id).cloned().unwrap_or_else(|| Entity {
            id: id.clone(),
            parent_id: self.root_turn.clone(),
            epoch: self.epoch,
            state: Lifecycle::Admitted,
            title: role.to_owned(),
            subject: None,
            reason: None,
            started_at_ms: None,
            updated_at_ms: at_ms,
            provenance: None,
            body: Body::Agent {
                role: None,
                summary: None,
                context_pct: None,
            },
        });
        if entity.title == role && self.entities.contains_key(&id) {
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
