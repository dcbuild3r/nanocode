//! The card projection rendered by every surface.
//!
//! Every user-visible string is produced here, from integers and the
//! caller's clock, so the TUI and the web mirror show identical cards.

use crate::model::{Body, EntityKind, Lifecycle, Provenance, StatusSection};
use crate::reduce::{SurfaceState, Tracked};
use crate::{QUIET_MS, STALE_RESOURCES_MS};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Semantic color shared by surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Text,
    Muted,
    Info,
    Live,
    Ok,
    Warn,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Badge {
    pub text: String,
    pub tone: Tone,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CardLine {
    pub text: String,
    pub tone: Tone,
}

/// One rendered card. Surfaces draw these fields verbatim.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Card {
    pub id: String,
    pub kind: EntityKind,
    /// Task-tree depth; children render beneath their parent.
    pub depth: u32,
    /// Pinned Status cards stay at the top of the stream.
    pub pinned: bool,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub state: Lifecycle,
    pub state_label: String,
    pub tone: Tone,
    pub badges: Vec<Badge>,
    pub lines: Vec<CardLine>,
}

/// Connection and runtime liveness, shown as a footer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Liveness {
    pub epoch: u64,
    pub cursor: u64,
    pub open: usize,
    pub restarts: usize,
    pub replays_ignored: u64,
    pub regressions_ignored: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_pct: Option<u32>,
    pub label: String,
    pub tone: Tone,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Projection {
    pub now_ms: u64,
    pub cards: Vec<Card>,
    pub liveness: Liveness,
}

/// Compact elapsed time: `42s`, `4m 12s`, `71h 3m`.
#[must_use]
pub fn format_duration(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{} KiB", bytes / 1024)
    } else {
        format!("{} MiB", bytes / (1024 * 1024))
    }
}

fn line(text: impl Into<String>, tone: Tone) -> CardLine {
    CardLine {
        text: text.into(),
        tone,
    }
}

fn badge(text: impl Into<String>, tone: Tone) -> Badge {
    Badge {
        text: text.into(),
        tone,
    }
}

const fn provenance_tone(provenance: Provenance) -> Tone {
    match provenance {
        Provenance::Verified => Tone::Ok,
        Provenance::AgentReported => Tone::Info,
        Provenance::TestBuild => Tone::Warn,
    }
}

const fn section_tone(section: StatusSection) -> Tone {
    match section {
        StatusSection::Done => Tone::Ok,
        StatusSection::InProgress => Tone::Live,
        StatusSection::Blocked => Tone::Error,
        StatusSection::Next => Tone::Text,
    }
}

const fn is_environment(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Status | EntityKind::Hand | EntityKind::Brain
    )
}

impl SurfaceState {
    /// Project the current state into the ordered card stream: pinned Status
    /// cards, then Hand and Brain cards, then the task tree depth-first.
    #[must_use]
    pub fn project(&self, now_ms: u64) -> Projection {
        let ordered = self.ordered();
        let mut children: HashMap<&str, Vec<&Tracked>> = HashMap::new();
        let mut roots = Vec::new();
        for tracked in &ordered {
            let kind = tracked.entity.kind();
            if is_environment(kind) {
                continue;
            }
            match tracked.entity.parent_id.as_deref() {
                Some(parent)
                    if self
                        .entities
                        .get(parent)
                        .is_some_and(|parent| !is_environment(parent.entity.kind())) =>
                {
                    children.entry(parent).or_default().push(tracked);
                }
                _ => roots.push(*tracked),
            }
        }

        let mut cards = Vec::new();
        for kind in [EntityKind::Status, EntityKind::Hand, EntityKind::Brain] {
            for tracked in ordered
                .iter()
                .filter(|tracked| tracked.entity.kind() == kind)
            {
                cards.push(self.card(tracked, 0, now_ms, &children));
            }
        }
        let mut visited = HashSet::new();
        let mut stack: Vec<(&Tracked, u32)> =
            roots.into_iter().rev().map(|root| (root, 0)).collect();
        while let Some((tracked, depth)) = stack.pop() {
            if !visited.insert(tracked.entity.id.as_str()) {
                continue;
            }
            cards.push(self.card(tracked, depth, now_ms, &children));
            if let Some(kids) = children.get(tracked.entity.id.as_str()) {
                stack.extend(kids.iter().rev().map(|kid| (*kid, depth + 1)));
            }
        }

        Projection {
            now_ms,
            cards,
            liveness: self.liveness(now_ms),
        }
    }

    fn liveness(&self, now_ms: u64) -> Liveness {
        let open = self
            .entities
            .values()
            .filter(|tracked| {
                tracked.entity.kind().settles_on_epoch() && !tracked.entity.state.is_terminal()
            })
            .count();
        let event_age = self.last_event_at_ms.map(|at| now_ms.saturating_sub(at));
        let heartbeat_age = self
            .last_heartbeat_at_ms
            .map(|at| now_ms.saturating_sub(at));
        let mut parts = vec![
            format!("epoch {}", self.epoch),
            format!("cursor {}", self.cursor),
        ];
        parts.push(format!("{open} open"));
        if let Some(age) = event_age {
            parts.push(format!("last event {} ago", format_duration(age)));
        }
        if let Some(age) = heartbeat_age {
            parts.push(format!("heartbeat {} ago", format_duration(age)));
        }
        if let Some(pct) = self.context_pct {
            parts.push(format!("context {pct}%"));
        }
        if !self.restarts.is_empty() {
            parts.push(format!("{} restart(s)", self.restarts.len()));
        }
        if self.replays_ignored > 0 {
            parts.push(format!("{} replay(s) ignored", self.replays_ignored));
        }
        let quiet = |age: Option<u64>| age.is_none_or(|age| age > QUIET_MS);
        let stalled = open > 0 && quiet(event_age) && quiet(heartbeat_age);
        let tone = if stalled || self.context_pct.is_some_and(|pct| pct >= 90) {
            Tone::Warn
        } else {
            Tone::Ok
        };
        Liveness {
            epoch: self.epoch,
            cursor: self.cursor,
            open,
            restarts: self.restarts.len(),
            replays_ignored: self.replays_ignored,
            regressions_ignored: self.regressions_ignored,
            context_pct: self.context_pct,
            label: parts.join(" · "),
            tone,
        }
    }

    /// Latest event time anywhere in this entity's subtree: a parent is not
    /// quiet while its children make progress.
    fn subtree_last_event_at(
        &self,
        root: &Tracked,
        children: &HashMap<&str, Vec<&Tracked>>,
    ) -> u64 {
        let mut latest = 0;
        let mut visited = HashSet::new();
        let mut stack = vec![root];
        while let Some(tracked) = stack.pop() {
            if !visited.insert(tracked.entity.id.as_str()) {
                continue;
            }
            latest = latest.max(tracked.last_event_at_ms);
            if let Some(kids) = children.get(tracked.entity.id.as_str()) {
                stack.extend(kids.iter().copied());
            }
        }
        latest
    }

    /// Changes in an agent's subtree applied after `cursor`, with the newest.
    fn newer_activity(
        &self,
        root: &Tracked,
        cursor: u64,
        children: &HashMap<&str, Vec<&Tracked>>,
    ) -> (usize, u64) {
        let mut newer = 0;
        let mut latest = 0;
        let mut visited = HashSet::new();
        let mut stack = vec![root];
        while let Some(tracked) = stack.pop() {
            if !visited.insert(tracked.entity.id.as_str()) {
                continue;
            }
            if tracked.last_cursor > cursor {
                newer += 1;
                latest = latest.max(tracked.last_cursor);
            }
            if let Some(kids) = children.get(tracked.entity.id.as_str()) {
                stack.extend(kids.iter().copied());
            }
        }
        (newer, latest)
    }

    fn card(
        &self,
        tracked: &Tracked,
        depth: u32,
        now_ms: u64,
        children: &HashMap<&str, Vec<&Tracked>>,
    ) -> Card {
        let entity = &tracked.entity;
        let kind = entity.kind();
        let mut badges = Vec::new();
        let mut lines = Vec::new();
        let (mut state_label, mut tone) = match entity.state {
            Lifecycle::Admitted => ("queued".to_owned(), Tone::Info),
            Lifecycle::Running => (
                entity.started_at_ms.map_or_else(
                    || "running".to_owned(),
                    |started| {
                        format!(
                            "running {}",
                            format_duration(now_ms.saturating_sub(started))
                        )
                    },
                ),
                Tone::Live,
            ),
            Lifecycle::Completed => (
                entity.started_at_ms.map_or_else(
                    || "done".to_owned(),
                    |started| {
                        format!(
                            "done in {}",
                            format_duration(entity.updated_at_ms.saturating_sub(started))
                        )
                    },
                ),
                Tone::Ok,
            ),
            Lifecycle::Failed => ("failed".to_owned(), Tone::Error),
            Lifecycle::Cancelled => ("cancelled".to_owned(), Tone::Muted),
            Lifecycle::Unknown => ("unknown".to_owned(), Tone::Warn),
        };

        if let Some(provenance) = entity.provenance {
            badges.push(badge(provenance.label(), provenance_tone(provenance)));
        }
        if entity.state == Lifecycle::Unknown && tracked.settled_locally {
            badges.push(badge("outcome unknown", Tone::Warn));
        }
        if tracked.reconciled {
            badges.push(badge("outcome reconciled", Tone::Info));
        }
        let quiet_for = now_ms.saturating_sub(self.subtree_last_event_at(tracked, children));
        if !entity.state.is_terminal() && kind.settles_on_epoch() && quiet_for > QUIET_MS {
            badges.push(badge(
                format!("quiet {}", format_duration(quiet_for)),
                Tone::Warn,
            ));
        }
        if let Some(reason) = &entity.reason {
            let reason_tone = match entity.state {
                Lifecycle::Failed => Tone::Error,
                Lifecycle::Unknown => Tone::Warn,
                _ => Tone::Muted,
            };
            lines.push(line(reason.clone(), reason_tone));
        }

        match &entity.body {
            Body::Agent {
                role,
                summary,
                context_pct,
            } => {
                if let Some(role) = role {
                    lines.push(line(format!("role: {role}"), Tone::Muted));
                }
                if let Some(summary) = summary {
                    lines.push(line(summary.text.clone(), Tone::Text));
                    let (newer, latest) = self.newer_activity(tracked, summary.at_cursor, children);
                    if newer > 0 {
                        badges.push(badge("summary stale", Tone::Warn));
                        lines.push(line(
                            format!(
                                "summary ({}) from cursor {}; {newer} newer change(s) through cursor {latest}",
                                summary.provenance.label(),
                                summary.at_cursor
                            ),
                            Tone::Warn,
                        ));
                    } else {
                        lines.push(line(
                            format!(
                                "summary ({}) current at cursor {}",
                                summary.provenance.label(),
                                summary.at_cursor
                            ),
                            Tone::Muted,
                        ));
                    }
                }
                if let Some(pct) = context_pct {
                    let tone = if *pct >= 90 { Tone::Warn } else { Tone::Muted };
                    lines.push(line(format!("context {pct}%"), tone));
                }
            }
            Body::Turn { prompt } => {
                if let Some(prompt) = prompt {
                    lines.push(line(format!("> {prompt}"), Tone::Text));
                }
            }
            Body::ToolCall {
                tool,
                cwd,
                hand_id,
                exit_code,
                output_bytes,
            } => {
                let mut place = vec![tool.clone()];
                if let Some(cwd) = cwd {
                    place.push(format!("in {cwd}"));
                }
                if let Some(hand) = hand_id {
                    place.push(format!("on {hand}"));
                }
                lines.push(line(place.join(" "), Tone::Muted));
                let mut result = Vec::new();
                if let Some(code) = exit_code {
                    result.push(format!("exit {code}"));
                }
                if let Some(bytes) = output_bytes {
                    result.push(format!("{} output", format_bytes(*bytes)));
                }
                if !result.is_empty() {
                    let failed = exit_code.is_some_and(|code| code != 0);
                    lines.push(line(
                        result.join(" · "),
                        if failed { Tone::Error } else { Tone::Muted },
                    ));
                }
            }
            Body::ProcessSession { hand_id, exit_code } => {
                if let Some(hand) = hand_id {
                    lines.push(line(format!("on {hand}"), Tone::Muted));
                }
                if let Some(code) = exit_code {
                    let tone = if *code == 0 { Tone::Muted } else { Tone::Error };
                    lines.push(line(format!("exit {code}"), tone));
                }
            }
            Body::Run {} => {}
            Body::Hand {
                online,
                os,
                resources,
            } => {
                if *online {
                    state_label = "online".to_owned();
                    tone = Tone::Ok;
                } else {
                    state_label = "offline".to_owned();
                    tone = Tone::Muted;
                    badges.push(badge("offline", Tone::Muted));
                }
                if let Some(os) = os {
                    lines.push(line(os.clone(), Tone::Muted));
                }
                match resources {
                    Some(resources) => {
                        let mut parts = Vec::new();
                        if let Some(cpu) = resources.cpu_pct {
                            parts.push(format!("CPU {cpu}%"));
                        }
                        if let Some(ram) = resources.ram_free_gb {
                            parts.push(format!("RAM {ram} GB free"));
                        }
                        if let Some(disk) = resources.disk_free_gb {
                            parts.push(format!("disk {disk} GB free"));
                        }
                        let age = now_ms.saturating_sub(resources.observed_at_ms);
                        parts.push(format!("observed {} ago", format_duration(age)));
                        let stale = age > STALE_RESOURCES_MS;
                        if stale {
                            badges.push(badge("stale resources", Tone::Warn));
                            if *online {
                                tone = Tone::Warn;
                            }
                        }
                        lines.push(line(
                            parts.join(" · "),
                            if stale { Tone::Warn } else { Tone::Muted },
                        ));
                    }
                    None => lines.push(line("resources unknown", Tone::Muted)),
                }
            }
            Body::Brain {
                workspace,
                memory_notes,
                dirty_files,
            } => {
                if entity.state == Lifecycle::Running {
                    state_label = "attached".to_owned();
                    tone = Tone::Ok;
                }
                lines.push(line(workspace.clone(), Tone::Text));
                let mut parts = Vec::new();
                if let Some(notes) = memory_notes {
                    parts.push(format!("{notes} memory note(s)"));
                }
                if let Some(dirty) = dirty_files {
                    parts.push(format!("{dirty} uncommitted file(s)"));
                }
                if !parts.is_empty() {
                    lines.push(line(parts.join(" · "), Tone::Muted));
                }
            }
            Body::Decision {
                question,
                recommendation,
                options,
                chosen,
            } => {
                match entity.state {
                    Lifecycle::Admitted | Lifecycle::Running => {
                        state_label = "awaiting decision".to_owned();
                        tone = Tone::Info;
                    }
                    Lifecycle::Completed => state_label = "decided".to_owned(),
                    _ => {}
                }
                lines.push(line(question.clone(), Tone::Text));
                if let Some(recommendation) = recommendation {
                    lines.push(line(format!("recommended: {recommendation}"), Tone::Info));
                }
                if !options.is_empty() {
                    lines.push(line(
                        format!("options: {}", options.join(" / ")),
                        Tone::Muted,
                    ));
                }
                if let Some(chosen) = chosen {
                    lines.push(line(
                        format!("chosen: {chosen} (intent recorded)"),
                        Tone::Ok,
                    ));
                }
            }
            Body::Status { items } => {
                let has = |section| items.iter().any(|item| item.section == section);
                let (label, status_tone) = if has(StatusSection::Blocked) {
                    ("blocked", Tone::Error)
                } else if has(StatusSection::InProgress) {
                    ("in progress", Tone::Live)
                } else if has(StatusSection::Next) {
                    ("next steps", Tone::Info)
                } else {
                    ("done", Tone::Ok)
                };
                state_label = label.to_owned();
                tone = status_tone;
                for section in StatusSection::ALL {
                    let section_items: Vec<_> = items
                        .iter()
                        .filter(|item| item.section == section)
                        .collect();
                    if section_items.is_empty() {
                        continue;
                    }
                    lines.push(line(section.label(), section_tone(section)));
                    for item in section_items {
                        let provenance = item.provenance.map_or("unsourced", Provenance::label);
                        let tone = item.provenance.map_or(Tone::Muted, provenance_tone);
                        lines.push(line(format!("  [{provenance}] {}", item.text), tone));
                        if let Some(evidence) = &item.evidence {
                            lines.push(line(format!("    evidence: {evidence}"), Tone::Muted));
                        }
                    }
                }
            }
        }

        Card {
            id: entity.id.clone(),
            kind,
            depth,
            pinned: kind == EntityKind::Status,
            title: entity.title.clone(),
            subject: entity.subject.clone(),
            state: entity.state,
            state_label,
            tone,
            badges,
            lines,
        }
    }
}
