// SPDX-License-Identifier: Apache-2.0
//! Mission Control's deterministic event projection. Time is an explicit render input.
use super::transcript::TranscriptRecord;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(crate) struct SurfaceState {
    pub(crate) epoch: Option<String>,
    pub(crate) last_event: Option<u64>,
    pub(crate) connected: bool,
    entities: BTreeMap<String, Entity>,
    turn: Option<String>,
}
#[derive(Clone)]
struct Entity {
    scope: String,
    parent: String,
    family: String,
    model: String,
    state: String,
    step: String,
    at: u64,
    tool: bool,
    evidence: String,
    retained: bool,
}
impl SurfaceState {
    /// Atomically replaces one complete server snapshot, at its opaque cursor.
    /// Caller must replay surface deltas strictly after the returned cursor.
    pub(crate) fn snapshot(
        &mut self,
        snapshot: &nanocodex_managed::SurfaceSnapshot,
    ) -> Option<String> {
        if snapshot.protocol != 1 {
            return None;
        }
        let mut next = Self::default();
        next.epoch = Some(snapshot.epoch.to_string());
        for run in &snapshot.runs {
            let scope = run
                .agent_id
                .map_or_else(|| "root".into(), |id| id.to_string());
            next.entities.insert(
                scope.clone(),
                Entity {
                    scope,
                    parent: "root".into(),
                    family: "family ?".into(),
                    model: "model ?".into(),
                    state: "running".into(),
                    step: "snapshot run".into(),
                    at: run.started_at,
                    tool: false,
                    evidence: format!("snapshot @{}", snapshot.cursor),
                    retained: false,
                },
            );
        }
        for agent in &snapshot.agents {
            let scope = agent.agent_id.to_string();
            next.entities.insert(
                scope.clone(),
                Entity {
                    scope,
                    parent: agent
                        .parent_agent_id
                        .map_or_else(|| "root".into(), |id| id.to_string()),
                    family: "family ?".into(),
                    model: "model ?".into(),
                    state: agent.state.clone(),
                    step: agent.role.clone(),
                    at: agent.updated_at,
                    tool: false,
                    evidence: format!("snapshot @{}", snapshot.cursor),
                    retained: false,
                },
            );
        }
        for call in &snapshot.tool_calls {
            let scope = call
                .agent_id
                .map_or_else(|| "root".into(), |id| id.to_string());
            next.entities.insert(
                call_key(&scope, &call.call_id, u64::from(call.attempt)),
                Entity {
                    scope,
                    parent: "root".into(),
                    family: "family ?".into(),
                    model: "model ?".into(),
                    state: "running".into(),
                    step: call.tool.clone(),
                    at: call.started_at,
                    tool: true,
                    evidence: format!("snapshot @{}", snapshot.cursor),
                    retained: false,
                },
            );
        }
        // Snapshot generation is transport evidence, not entity progress.
        *self = next;
        Some(snapshot.cursor.clone())
    }
    pub(crate) fn observe(&mut self, record: &TranscriptRecord) {
        let kind = record.kind();
        let p = record.decode_payload::<Value>().unwrap_or(Value::Null);
        let at = record.recorded_at_unix_ms();
        self.last_event = Some(self.last_event.unwrap_or(0).max(at));
        let scope = record
            .managed_agent_id()
            .map_or_else(|| "root".into(), |id| id.to_string());
        if kind == "runtime_epoch" {
            for call in p["interrupted"]["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let owner = call["agent_id"]
                    .as_u64()
                    .map_or_else(|| "root".into(), |id| id.to_string());
                let key = call_key(
                    &owner,
                    call["call_id"].as_str().unwrap_or(""),
                    call["attempt"].as_u64().unwrap_or(1),
                );
                if let Some(entity) = self.entities.get_mut(&key)
                    && matches!(entity.state.as_str(), "running" | "retained")
                    && entity.at <= at
                {
                    entity.state = "unknown".into();
                    entity.retained = false;
                    entity.step = "runtime restarted; reconcile receipt".into();
                    entity.evidence = "runtime receipt · effect unknown".into();
                }
            }
            for run in p["interrupted"]["runs"].as_array().into_iter().flatten() {
                let owner = run["agent_id"]
                    .as_u64()
                    .map_or_else(|| "root".into(), |id| id.to_string());
                if let Some(entity) = self.entities.get_mut(&owner)
                    && matches!(entity.state.as_str(), "running" | "retained")
                    && entity.at <= at
                {
                    entity.state = "unknown".into();
                }
            }
            self.epoch = p.get("epoch").map(identity);
            return;
        }
        if kind == "runtime.epoch" {
            let epoch = p.get("epoch").map(identity);
            if epoch != self.epoch
                && (self.epoch.is_some() || p.get("previous_epoch").is_some_and(|v| !v.is_null()))
            {
                for entity in self
                    .entities
                    .values_mut()
                    .filter(|e| matches!(e.state.as_str(), "running" | "retained"))
                {
                    entity.state = "unknown".into();
                    entity.step = "runtime restarted; outcome unknown".into();
                }
            }
            self.epoch = epoch;
            return;
        }
        if kind == "run.started" && scope == "root" {
            let turn = record
                .managed_turn_id()
                .map(|id| id.to_string())
                .or_else(|| record.agent_request_id().map(|id| id.to_string()));
            if self.turn != turn {
                self.entities
                    .retain(|_, e| e.scope != "root" || e.retained || e.state == "unknown");
                self.turn = turn;
            }
        }
        if matches!(
            kind,
            "run.started" | "run.completed" | "run.failed" | "run.unknown"
        ) {
            for entity in self
                .entities
                .values_mut()
                .filter(|e| e.tool && !e.retained && e.scope == scope && e.state == "running")
            {
                entity.state = "unknown".into();
                entity.step = if kind == "run.started" {
                    "new run; no terminal receipt"
                } else {
                    "run ended; no terminal receipt"
                }
                .into();
            }
        }
        let tool = kind.starts_with("tool.");
        if !(tool || kind.starts_with("run.") || kind.starts_with("assistant.")) {
            return;
        }
        let key = if tool {
            call_key(
                &scope,
                p["call_id"].as_str().unwrap_or("unidentified"),
                p["attempt"].as_u64().unwrap_or(1),
            )
        } else {
            scope.clone()
        };
        let retained = kind == "tool.result"
            && p["outcome_unknown"] != true
            && p["status"] != "unknown"
            && p["status"] != "failed"
            && p["is_error"] != true
            && (p["structured_result"]["session_id"].is_number()
                || p["result"]
                    .as_str()
                    .is_some_and(|s| s.contains("Script running with cell ID")));
        if retained {
            for (id, child) in &mut self.entities {
                if id.starts_with(&format!("{key}/")) && child.state == "running" {
                    child.retained = true;
                    child.state = "retained".into();
                    child.evidence = "yielded · liveness unconfirmed".into();
                }
            }
        }
        let entity = self.entities.entry(key).or_insert_with(|| Entity {
            scope: scope.clone(),
            parent: "root".into(),
            family: "family ?".into(),
            model: "model ?".into(),
            state: "running".into(),
            step: String::new(),
            at,
            tool,
            evidence: "agent-reported".into(),
            retained: false,
        });
        if at < entity.at
            || (entity.tool
                && entity.state == "unknown"
                && entity.evidence == "runtime receipt · effect unknown")
        {
            return;
        }
        entity.at = at;
        if matches!(kind, "tool.result" | "tool.call") {
            entity.retained = retained;
        }
        for (field, dest) in [("family", &mut entity.family), ("model", &mut entity.model)] {
            if let Some(value) = p[field].as_str() {
                *dest = value.into();
            }
        }
        if let Some(parent) = p.get("parent_agent_id") {
            entity.parent = identity(parent);
        }
        entity.step = p["tool"]
            .as_str()
            .or_else(|| p["name"].as_str())
            .unwrap_or(kind)
            .into();
        if kind == "tool.result" {
            entity.evidence = if retained {
                "yield receipt · completion unconfirmed"
            } else if p["outcome_unknown"] == true {
                "runtime receipt · effect unknown"
            } else if p["structured_result"]["evidence"]["kind"] == "test_build" {
                "test-build · agent-reported"
            } else {
                "verified: receipt only"
            }
            .into();
        }
        entity.state = match kind {
            "run.started" | "tool.call" => "running",
            "run.completed" => "completed",
            "run.failed" => {
                if p["status"] == "cancelled" {
                    "cancelled"
                } else {
                    "failed"
                }
            }
            "run.unknown" | "tool.unknown" => "unknown",
            "tool.result" => {
                if retained {
                    "retained"
                } else if p["outcome_unknown"] == true || p["status"] == "unknown" {
                    "unknown"
                } else if p["is_error"] == true || p["status"] == "error" || p["status"] == "failed"
                {
                    "failed"
                } else {
                    "completed"
                }
            }
            _ => &entity.state,
        }
        .into();
    }
    pub(crate) fn subagent(&mut self, update: &nanocodex_subagents::AgentUpdate, at: u64) {
        use nanocodex_subagents::AgentUpdate;
        match update {
            AgentUpdate::Added(d) => {
                self.entities.entry(d.id.to_string()).or_insert(Entity {
                    scope: d.id.to_string(),
                    parent: d.parent.map_or_else(|| "root".into(), |p| p.to_string()),
                    family: "family ?".into(),
                    model: "model ?".into(),
                    state: "admitted".into(),
                    step: d.role.clone(),
                    at,
                    tool: false,
                    evidence: "agent-reported".into(),
                    retained: false,
                });
            }
            AgentUpdate::Event { id, event } => {
                let record = TranscriptRecord::from_agent(event.seq, at, event.clone());
                // Local events carry their child identity in the update envelope.
                if let Ok(mut value) = serde_json::to_value(record) {
                    value["agent"]["managed_agent_id"] =
                        serde_json::to_value(id).unwrap_or(Value::Null);
                    if let Ok(record) = serde_json::from_value(value) {
                        self.observe(&record);
                    }
                }
            }
            AgentUpdate::Status { id, status } | AgentUpdate::Completion { id, status, .. } => {
                if let Some(e) = self.entities.get_mut(&id.to_string()) {
                    let value = serde_json::to_value(status).unwrap_or(Value::Null);
                    e.state = value["state"].as_str().unwrap_or("unknown").into();
                    e.at = at;
                }
            }
            _ => {}
        }
    }
    pub(crate) fn footer(&self, now: u64, context: Option<u64>) -> String {
        let age = self.last_event.map_or_else(
            || "no signal".into(),
            |at| format!("{}s ago", now.saturating_sub(at) / 1000),
        );
        let percent = context.map_or_else(
            || "?".into(),
            |p| format!("{p}%{}", if p > 100 { " WARNING >100%" } else { "" }),
        );
        format!(
            "context {percent} | event {age} | epoch {} | stream {}",
            self.epoch
                .as_deref()
                .unwrap_or("unreported; recovery unobservable"),
            if self.connected {
                "observed (health unconfirmed)"
            } else {
                "unconfirmed/replay"
            }
        )
    }
    fn rows(&self) -> Vec<(&String, &Entity)> {
        let mut rows = self.entities.iter().collect::<Vec<_>>();
        rows.sort_by_key(|(id, e)| {
            (
                id.as_str() != "root",
                e.tool,
                !matches!(e.state.as_str(), "running" | "unknown"),
                e.scope.parse::<u64>().unwrap_or(u64::MAX),
                (*id).clone(),
            )
        });
        rows
    }
    pub(crate) fn row_count(&self) -> usize {
        self.entities.len()
    }
    pub(crate) fn row_scope(&self, selected: usize) -> Option<&str> {
        self.rows().get(selected).map(|(_, e)| e.scope.as_str())
    }
    pub(crate) fn panel(&self, now: u64, selected: usize, focused: bool, limit: usize) -> String {
        let running = self
            .entities
            .values()
            .filter(|e| !e.tool && e.state == "running")
            .count();
        let unknown = self
            .entities
            .values()
            .filter(|e| e.state == "unknown")
            .count();
        let retained = self.entities.values().filter(|e| e.retained).count();
        let completed = self
            .entities
            .values()
            .filter(|e| !e.tool && e.state == "completed")
            .count();
        let mut text = format!(
            "Status · legacy / best-effort\nDone: {completed} (not verified)\nIn progress: {running}\nBlocked/unknown: {unknown}\nRetained calls: {retained} (unconfirmed)\nNext: inspect task evidence\n\nTask tree · assignment states\n"
        );
        let first = selected.saturating_sub(limit.saturating_sub(1));
        text.push_str(&format!(
            "F6 tree · ↑↓/PgUp/PgDn · Enter\n{} shown / {} loaded · row {}\n",
            limit.min(self.entities.len()),
            self.entities.len(),
            selected.saturating_add(1).min(self.entities.len())
        ));
        for (index, (id, e)) in self.rows().into_iter().enumerate().skip(first).take(limit) {
            let age = now.saturating_sub(e.at) / 1000;
            let unresolved = self
                .entities
                .values()
                .filter(|call| call.tool && call.scope == e.scope && call.state == "unknown")
                .count();
            let stale = if !e.tool && unresolved > 0 {
                format!(" · {unresolved} unresolved calls")
            } else if e.retained {
                " · liveness unconfirmed".into()
            } else {
                String::new()
            };
            text.push_str(&format!(
                "{}{}{} [{}]{}\n  {} / {}\n  {} · {age}s\n  {}\n",
                if index == selected && focused {
                    "> "
                } else {
                    ""
                },
                if e.scope == "root" {
                    "".into()
                } else {
                    format!("{} → ", e.parent)
                },
                id,
                e.state,
                stale,
                e.family,
                e.model,
                e.step,
                e.evidence
            ));
        }
        text
    }
}

pub(crate) fn now_ms() -> u64 {
    #[cfg(feature = "tui-bench")]
    {
        1_750_000_100_000
    }
    #[cfg(not(feature = "tui-bench"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }
}

fn identity(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

fn call_key(scope: &str, call: &str, attempt: u64) -> String {
    if attempt == 1 {
        format!("{scope}/{call}")
    } else {
        format!("{scope}/a{attempt}/{call}")
    }
}
