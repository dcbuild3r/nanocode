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
}
impl SurfaceState {
    pub(crate) fn observe(&mut self, record: &TranscriptRecord) {
        let kind = record.kind();
        let p = record.decode_payload::<Value>().unwrap_or(Value::Null);
        let at = record.recorded_at_unix_ms();
        self.last_event = Some(self.last_event.unwrap_or(0).max(at));
        let scope = record
            .managed_agent_id()
            .map_or_else(|| "root".into(), |id| id.to_string());
        if kind == "runtime.epoch" {
            let epoch = p.get("epoch").map(Value::to_string);
            if epoch != self.epoch {
                for entity in self.entities.values_mut().filter(|e| e.state == "running") {
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
                self.entities.clear();
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
                .filter(|e| e.tool && e.scope == scope && e.state == "running")
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
            format!(
                "{scope}/{}",
                p["call_id"].as_str().unwrap_or("unidentified")
            )
        } else {
            scope.clone()
        };
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
        });
        if at < entity.at {
            return;
        }
        entity.at = at;
        for (field, dest) in [("family", &mut entity.family), ("model", &mut entity.model)] {
            if let Some(value) = p[field].as_str() {
                *dest = value.into();
            }
        }
        if let Some(parent) = p.get("parent_agent_id") {
            entity.parent = parent.to_string();
        }
        entity.step = p["tool"]
            .as_str()
            .or_else(|| p["name"].as_str())
            .unwrap_or(kind)
            .into();
        if kind == "tool.result" {
            entity.evidence = if p["outcome_unknown"] == true {
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
                if p["outcome_unknown"] == true || p["status"] == "unknown" {
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
            "stream {} | epoch {} | event {age} | context {percent}",
            if self.connected {
                "connected"
            } else {
                "offline/replay"
            },
            self.epoch.as_deref().unwrap_or("unreported")
        )
    }
    pub(crate) fn row_count(&self) -> usize {
        self.entities.len()
    }
    pub(crate) fn row_scope(&self, selected: usize) -> Option<&str> {
        self.entities
            .values()
            .nth(selected)
            .map(|e| e.scope.as_str())
    }
    pub(crate) fn panel(&self, now: u64, selected: usize, focused: bool, limit: usize) -> String {
        let running = self
            .entities
            .values()
            .filter(|e| e.state == "running")
            .count();
        let unknown = self
            .entities
            .values()
            .filter(|e| e.state == "unknown")
            .count();
        let completed = self
            .entities
            .values()
            .filter(|e| e.state == "completed")
            .count();
        let mut text = format!(
            "Status · legacy / best-effort\nDone: {completed} (not verified)\nIn progress: {running}\nBlocked/unknown: {unknown}\nNext: inspect unknown receipts\n\nTask tree · assignment states\n"
        );
        let first = selected.saturating_sub(limit.saturating_sub(1));
        text.push_str(&format!(
            "F6 tree · ↑↓/PgUp/PgDn · Enter\n{} loaded · row {}\n",
            self.entities.len(),
            selected.saturating_add(1).min(self.entities.len())
        ));
        for (index, (id, e)) in self.entities.iter().enumerate().skip(first).take(limit) {
            let age = now.saturating_sub(e.at) / 1000;
            let stale = if age > 60 && e.state == "running" {
                " · NO SIGNAL"
            } else if age > 60 {
                " · stale report"
            } else {
                ""
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
