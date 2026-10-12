//! Real producer output: history pages and `GET /v1/agents/:id/surface` snapshots
//! captured by the managed surface-state curl recovery journey
//! (feat/surface-state-protocol), reduced through the shared adapter.
//! Goldens live beside them in `fixtures/managed/*.cards.json`; regenerate with
//! `UPDATE_SURFACE_GOLDEN=1`.

use nanocodex_surface_state::{
    Lifecycle, Projection, SurfaceState,
    agent_events::{AgentEventAdapter, ManagedSnapshot},
};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/managed")
}

fn read(name: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(dir().join(name)).expect("fixture")).expect("JSON")
}

fn history(name: &str) -> Vec<Value> {
    read(name)["data"].as_array().expect("history page").clone()
}

fn cursor(event: &Value) -> u64 {
    event["cursor"]
        .as_str()
        .and_then(|c| c.parse().ok())
        .expect("cursor")
}

struct Journey {
    adapter: AgentEventAdapter,
    state: SurfaceState,
    frames: Vec<Value>,
}

impl Journey {
    fn new() -> Self {
        Self {
            adapter: AgentEventAdapter::new(),
            state: SurfaceState::new(),
            frames: Vec::new(),
        }
    }

    fn events<'a>(&mut self, events: impl IntoIterator<Item = &'a Value>) {
        for event in events {
            for surface in self.adapter.observe_managed(event) {
                self.state.apply(&surface);
            }
        }
    }

    fn snapshot(&mut self, name: &str) -> ManagedSnapshot {
        let snapshot: ManagedSnapshot = serde_json::from_value(read(name)).expect("snapshot");
        let event = self.adapter.snapshot(&snapshot);
        self.state.apply(&event);
        snapshot
    }

    fn frame(&mut self, label: &str, now_ms: u64) -> Projection {
        let projection = self.state.project(now_ms);
        self.frames
            .push(json!({"label": label, "projection": projection}));
        projection
    }

    fn check(self, golden: &str) {
        let actual = Value::Array(self.frames);
        let path = dir().join(golden);
        if std::env::var_os("UPDATE_SURFACE_GOLDEN").is_some() {
            let mut body = serde_json::to_string_pretty(&actual).expect("golden");
            body.push('\n');
            fs::write(path, body).expect("write golden");
            return;
        }
        assert_eq!(actual, read(golden), "{golden}");
    }
}

fn card<'a>(projection: &'a Projection, id: &str) -> &'a nanocodex_surface_state::Card {
    projection
        .cards
        .iter()
        .find(|card| card.id == id)
        .unwrap_or_else(|| panic!("card {id} in {:#?}", projection.cards))
}

const ROOT_TURN: &str = "turn:4d83ebe3-6dfe-80df-9ba9-4a62fa024291";

/// A surface sees the root Code Mode cell start, loses its connection while
/// the runtime restarts, reconnects with the real surface snapshot, then
/// resumes the stream from the snapshot cursor.
#[test]
fn root_reconnects_across_a_runtime_restart_with_the_surface_snapshot() {
    let events = history("root-history.json");
    let mut journey = Journey::new();
    journey.events(events.iter().filter(|event| cursor(event) <= 10));
    let before = journey.frame(
        "connected: cell and nested shell running",
        1_791_838_772_401,
    );
    let turn = card(&before, ROOT_TURN);
    assert_eq!(
        turn.subject.as_deref(),
        Some("SURFACE_ROOT: apply S1 once.")
    );
    let shell = card(&before, "tool:root:surface-root-cell/code-1");
    assert_eq!((shell.state, shell.depth), (Lifecycle::Running, 2));

    let snapshot = journey.snapshot("root-surface-after-restart.json");
    let after = journey.frame(
        "reconnect: surface snapshot at epoch 1",
        snapshot.generated_at,
    );
    for id in [
        "tool:root:surface-root-cell",
        "tool:root:surface-root-cell/code-1",
    ] {
        let tool = card(&after, id);
        assert_eq!(tool.state, Lifecycle::Unknown, "{id}");
        assert!(
            tool.lines[0].text.contains("runtime restarted"),
            "{:?}",
            tool.lines
        );
    }
    // The accepted managed turn is still open in the snapshot.
    assert_eq!(card(&after, ROOT_TURN).state, Lifecycle::Running);
    assert_eq!(after.liveness.epoch, 1);

    let resume_from: u64 = snapshot.cursor.parse().expect("snapshot cursor");
    journey.events(events.iter().filter(|event| cursor(event) > resume_from));
    let done = journey.frame("deltas after the snapshot cursor", 1_791_838_774_396);
    assert_eq!(card(&done, ROOT_TURN).state, Lifecycle::Completed);
    assert!(
        done.cards
            .iter()
            .all(|card| card.state != Lifecycle::Running)
    );
    journey.check("root-journey.cards.json");
}

/// A surface attaching mid-swarm builds the task tree from the snapshot
/// alone; a later snapshot after the restart closes the lost calls and shows
/// the child's new work.
#[test]
fn child_tree_comes_from_snapshots_alone() {
    let mut journey = Journey::new();
    let during = journey.snapshot("child-surface-during.json");
    let first = journey.frame("attach: snapshot during the swarm", during.generated_at);
    let ids: Vec<(&str, u32)> = first
        .cards
        .iter()
        .map(|card| (card.id.as_str(), card.depth))
        .collect();
    assert_eq!(
        ids,
        [
            ("turn:3b72a0f2-a68e-8737-b618-9cdeadbafb12", 0),
            ("agent:1", 1),
            ("tool:1:surface-child-cell", 2),
            ("tool:1:surface-child-cell/code-1", 3),
            ("tool:root:surface-wait", 1),
            ("tool:root:surface-wait/code-1", 2),
        ]
    );
    assert_eq!(card(&first, "agent:1").title, "Surface child");

    let moved = journey.snapshot("child-surface-moved-on.json");
    let second = journey.frame("snapshot after restart: child moved on", moved.generated_at);
    assert_eq!(
        card(&second, "tool:1:surface-child-cell/code-1").state,
        Lifecycle::Unknown
    );
    assert_eq!(
        card(&second, "tool:1:surface-child-moved-on/code-1").state,
        Lifecycle::Running
    );
    assert_eq!(card(&second, "agent:1").state, Lifecycle::Running);
    journey.check("child-snapshots.cards.json");
}

/// The full captured child history reduces without a snapshot: the epoch
/// marker settles the lost calls and the child's later calls complete.
#[test]
fn child_history_replays_through_the_adapter() {
    let events = history("child-history.json");
    let last = events
        .last()
        .map(|event| event["created_at"].as_u64().unwrap())
        .unwrap();
    let mut journey = Journey::new();
    journey.events(&events);
    let projection = journey.frame("full child history", last);
    let lost = card(&projection, "tool:1:surface-child-cell/code-1");
    assert_eq!(lost.state, Lifecycle::Unknown);
    assert_eq!(
        card(&projection, "tool:1:surface-child-submit/code-1").state,
        Lifecycle::Completed
    );
    assert_eq!(projection.liveness.restarts, 1);
    journey.check("child-history.cards.json");
}

/// Real hosted-stack swarm (reviewer harness: protocol workerd stack with a
/// scripted model, 17 children, six runtime restarts). Children whose runs
/// ended without an accepted result are reported failed by `wait_agent`; their
/// cards must say failed, titled by role with their task as subject, and agree
/// with the service's own snapshot.
#[test]
fn swarm_children_show_their_reported_outcome() {
    let events = history("swarm-history.json");
    let last = events
        .last()
        .and_then(|event| event["created_at"].as_u64())
        .unwrap();
    let mut journey = Journey::new();
    journey.events(&events);
    let replayed = journey.frame("full swarm history (6 restarts)", last);
    let workers: Vec<_> = replayed
        .cards
        .iter()
        .filter(|card| card.kind == nanocodex_surface_state::EntityKind::Agent)
        .collect();
    assert!(!workers.is_empty());
    for worker in &workers {
        assert!(worker.title.starts_with("Worker "), "{worker:#?}");
        assert!(
            worker
                .subject
                .as_deref()
                .is_some_and(|task| task.starts_with("CHILD ")),
            "{worker:#?}"
        );
    }
    let failed = workers
        .iter()
        .filter(|card| card.state == Lifecycle::Failed)
        .count();
    assert!(
        failed >= 5,
        "reported failures must not read as done: {workers:#?}"
    );
    assert!(
        workers
            .iter()
            .all(|card| card.state != Lifecycle::Completed)
    );
    assert_eq!(replayed.liveness.epoch, 6);

    let snapshot = journey.snapshot("swarm-surface-after-epoch6.json");
    let settled = journey.frame("service snapshot after epoch 6", snapshot.generated_at);
    for agent in &snapshot.agents {
        let card = card(&settled, &format!("agent:{}", agent.agent_id));
        assert_eq!(card.state, Lifecycle::Failed, "{card:#?}");
        assert_eq!(card.title, agent.role);
    }
    assert!(
        settled
            .cards
            .iter()
            .all(|card| card.state != Lifecycle::Running)
    );
    journey.check("swarm.cards.json");
}
