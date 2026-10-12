//! Wire model shared with `js/nanocodex-surface-state`.

use serde::{Deserialize, Serialize};

/// Lifecycle shared by every entity. `unknown` is terminal: the runtime could
/// not observe the outcome (for example it restarted mid-call).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Admitted,
    Running,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

impl Lifecycle {
    /// Whether no further progress is expected.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Admitted | Self::Running)
    }

    /// Whether the outcome was actually observed.
    #[must_use]
    pub const fn is_definitive(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Where a claim came from. Surfaces badge every claim with it so verified
/// evidence is distinguishable from agent self-reports and test builds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Independently checked evidence for a claim.
    Verified,
    /// The runtime observed the call return (a tool receipt). It proves the
    /// call finished, not that its claim is true.
    Receipt,
    AgentReported,
    TestBuild,
}

impl Provenance {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Receipt => "receipt",
            Self::AgentReported => "agent-reported",
            Self::TestBuild => "test-build",
        }
    }
}

/// Entity kind, carried by [`Body`] as the `kind` tag.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Agent,
    Turn,
    ToolCall,
    ProcessSession,
    Run,
    Hand,
    Brain,
    Decision,
    Status,
}

impl EntityKind {
    /// Kinds whose in-flight work cannot survive a runtime restart.
    #[must_use]
    pub const fn settles_on_epoch(self) -> bool {
        matches!(
            self,
            Self::Agent | Self::Turn | Self::ToolCall | Self::ProcessSession | Self::Run
        )
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Turn => "turn",
            Self::ToolCall => "tool_call",
            Self::ProcessSession => "process_session",
            Self::Run => "run",
            Self::Hand => "hand",
            Self::Brain => "brain",
            Self::Decision => "decision",
            Self::Status => "status",
        }
    }
}

/// A runtime entity. The kind-specific payload is flattened beside the
/// common fields and tagged by `kind`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Entity {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Runtime epoch that last owned this entity.
    pub epoch: u64,
    pub state: Lifecycle,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Terminal reason, such as an error or "runtime restarted".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    pub updated_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    #[serde(flatten)]
    pub body: Body,
}

impl Entity {
    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        self.body.kind()
    }
}

/// Kind-specific entity payload.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Body {
    Agent {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<Summary>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_pct: Option<u32>,
    },
    Turn {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    ToolCall {
        tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hand_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_bytes: Option<u64>,
    },
    ProcessSession {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hand_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i64>,
    },
    Run {},
    Hand {
        online: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        os: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resources: Option<Resources>,
    },
    Brain {
        workspace: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        memory_notes: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dirty_files: Option<u64>,
    },
    Decision {
        question: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recommendation: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chosen: Option<String>,
    },
    Status {
        #[serde(default)]
        items: Vec<StatusItem>,
    },
}

impl Body {
    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        match self {
            Self::Agent { .. } => EntityKind::Agent,
            Self::Turn { .. } => EntityKind::Turn,
            Self::ToolCall { .. } => EntityKind::ToolCall,
            Self::ProcessSession { .. } => EntityKind::ProcessSession,
            Self::Run {} => EntityKind::Run,
            Self::Hand { .. } => EntityKind::Hand,
            Self::Brain { .. } => EntityKind::Brain,
            Self::Decision { .. } => EntityKind::Decision,
            Self::Status { .. } => EntityKind::Status,
        }
    }
}

/// An agent's self-description, anchored to the cursor it was written at so
/// later activity reveals that it is stale.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Summary {
    pub text: String,
    pub at_cursor: u64,
    pub provenance: Provenance,
}

/// Last observed Hand resources. Integers keep both implementations' text
/// identical.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Resources {
    pub observed_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ram_free_gb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_free_gb: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusSection {
    Done,
    InProgress,
    Blocked,
    Next,
}

impl StatusSection {
    pub const ALL: [Self; 4] = [Self::Done, Self::InProgress, Self::Blocked, Self::Next];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Done => "Done",
            Self::InProgress => "In progress",
            Self::Blocked => "Blocked",
            Self::Next => "Next",
        }
    }
}

/// One line of a pinned per-root-turn Status card.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StatusItem {
    pub section: StatusSection,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Runtime-to-surface event. Every delta carries a monotonically increasing
/// cursor; a reconnecting surface receives a snapshot followed by deltas
/// after its cursor.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SurfaceEvent {
    Snapshot {
        epoch: u64,
        cursor: u64,
        at_ms: u64,
        entities: Vec<Entity>,
    },
    Upsert {
        cursor: u64,
        at_ms: u64,
        entity: Entity,
    },
    RuntimeEpoch {
        epoch: u64,
        cursor: u64,
        at_ms: u64,
        reason: String,
    },
    /// Runtime liveness control frame. It carries the producer's cursor
    /// high-water mark for display only and never fences deltas.
    Heartbeat {
        cursor: u64,
        at_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_pct: Option<u32>,
    },
    /// Surface-local transport state: the event stream dropped or resumed.
    Connection { connected: bool, at_ms: u64 },
}

impl SurfaceEvent {
    /// The delta cursor that fences replays; control frames have none.
    #[must_use]
    pub const fn cursor(&self) -> Option<u64> {
        match self {
            Self::Snapshot { cursor, .. }
            | Self::Upsert { cursor, .. }
            | Self::RuntimeEpoch { cursor, .. } => Some(*cursor),
            Self::Heartbeat { .. } | Self::Connection { .. } => None,
        }
    }
}
