//! Cross-surface contract: every fixture event stream reduces to the golden
//! card projection in `fixtures/<name>.cards.json`. The JavaScript mirror
//! (`js/nanocodex-surface-state`) checks the same goldens, so the TUI and web
//! surfaces render identical cards from identical runtime events.
//!
//! Regenerate goldens after an intentional projection change with
//! `UPDATE_SURFACE_GOLDEN=1 cargo test -p nanocodex-surface-state`.

use nanocodex_surface_state::{SurfaceEvent, SurfaceState};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn run(name: &str) {
    let path = fixtures_dir().join(format!("{name}.json"));
    let fixture: Value = serde_json::from_str(&fs::read_to_string(&path).expect("read fixture"))
        .expect("fixture JSON");
    let mut state = SurfaceState::new();
    let mut frames = Vec::new();
    for step in fixture["steps"].as_array().expect("steps") {
        for raw in step["events"].as_array().expect("events") {
            let event: SurfaceEvent = serde_json::from_value(raw.clone())
                .unwrap_or_else(|error| panic!("{name}: {error}: {raw}"));
            // The wire shape must round-trip exactly so both implementations
            // read the same fields.
            assert_eq!(
                &serde_json::to_value(&event).expect("serialize"),
                raw,
                "{name}"
            );
            state.apply(&event);
        }
        let now_ms = step["now_ms"].as_u64().expect("now_ms");
        frames.push(json!({
            "label": step["label"],
            "projection": state.project(now_ms),
        }));
    }
    let actual = Value::Array(frames);
    let golden = fixtures_dir().join(format!("{name}.cards.json"));
    if std::env::var_os("UPDATE_SURFACE_GOLDEN").is_some() {
        let mut body = serde_json::to_string_pretty(&actual).expect("golden JSON");
        body.push('\n');
        fs::write(&golden, body).expect("write golden");
        return;
    }
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(&golden).expect("read golden")).expect("golden");
    assert_eq!(actual, expected, "{name} projection differs from golden");
}

#[test]
fn normal_turn() {
    run("normal-turn");
}

#[test]
fn subagent_swarm() {
    run("subagent-swarm");
}

#[test]
fn restart_mid_tool() {
    run("restart-mid-tool");
}

#[test]
fn reconnect() {
    run("reconnect");
}

#[test]
fn stale_summary() {
    run("stale-summary");
}

#[test]
fn orphaned_tool() {
    run("orphaned-tool");
}

#[test]
fn control_frames() {
    run("control-frames");
}

#[test]
fn streams() {
    run("streams");
}
