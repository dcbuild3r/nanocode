// SPDX-License-Identifier: Apache-2.0

//! Render every shared surface-state fixture through the card-stream widget
//! at narrow, normal and wide terminals, asserting the frames users see:
//! closed card borders, the liveness footer, and settled restart cards.

use nanocodex_surface_state::{Lifecycle, Projection, SurfaceEvent, SurfaceState};
use nanocodex_tui_render::{card_stream::CardStream, theme::Theme};
use ratatui::{Terminal, backend::TestBackend};
use serde::Deserialize;
use std::{fs, path::PathBuf};
use unicode_width::UnicodeWidthStr;

#[derive(Deserialize)]
struct Fixture {
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    now_ms: u64,
    events: Vec<SurfaceEvent>,
}

fn projections(name: &str) -> Vec<Projection> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/nanocodex-surface-state/fixtures")
        .join(format!("{name}.json"));
    let fixture: Fixture =
        serde_json::from_str(&fs::read_to_string(path).expect("fixture")).expect("fixture JSON");
    let mut state = SurfaceState::new();
    fixture
        .steps
        .iter()
        .map(|step| {
            for event in &step.events {
                state.apply(event);
            }
            state.project(step.now_ms)
        })
        .collect()
}

fn frame(projection: &Projection, width: u16, height: u16) -> Vec<String> {
    let theme = Theme::default();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| frame.render_widget(CardStream::new(projection, &theme), frame.area()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn every_fixture_renders_closed_cards_at_every_width() {
    for name in [
        "normal-turn",
        "subagent-swarm",
        "restart-mid-tool",
        "reconnect",
        "stale-summary",
        "orphaned-tool",
        "control-frames",
    ] {
        for projection in projections(name) {
            for width in [40, 80, 120] {
                let rows = frame(&projection, width, 200);
                let footer = rows.last().expect("footer");
                assert!(
                    footer.contains(&projection.liveness.label[..10]),
                    "{name}@{width}: {footer}"
                );
                let mut open_width = None;
                for row in rows.iter().filter(|row| !row.is_empty()) {
                    let trimmed = row.trim_start();
                    let indent = row.width() - trimmed.width();
                    if trimmed.starts_with('╭') {
                        assert!(trimmed.ends_with('╮'), "{name}@{width}: {row}");
                        open_width = Some((indent, trimmed.width()));
                    } else if trimmed.starts_with('│') || trimmed.starts_with('╰') {
                        let (card_indent, card_width) = open_width.expect("row inside a card");
                        assert_eq!(indent, card_indent, "{name}@{width}: {row}");
                        // Body rows are padded to the card width; trim_end only
                        // removes padding when the closing border is missing.
                        assert_eq!(trimmed.width(), card_width, "{name}@{width}: {row}");
                        assert!(
                            trimmed.ends_with('│') || trimmed.ends_with('╯'),
                            "{name}@{width}: {row}"
                        );
                    }
                }
                assert_eq!(
                    rows.iter()
                        .filter(|row| row.trim_start().starts_with('╭'))
                        .count(),
                    projection.cards.len(),
                    "{name}@{width}: one box per card"
                );
            }
        }
    }
}

#[test]
fn restart_settles_running_cards_in_the_rendered_frame() {
    let steps = projections("restart-mid-tool");
    let before = frame(&steps[0], 80, 40).join("\n");
    let after = frame(&steps[1], 80, 40).join("\n");
    assert!(before.contains("running 1h 11m"), "{before}");
    assert!(before.contains("[quiet 1h 11m]"), "{before}");
    assert!(!after.contains("running"), "{after}");
    assert!(
        after.contains("[outcome unknown]") && after.contains("runtime restarted"),
        "{after}"
    );
    assert!(
        steps[1]
            .cards
            .iter()
            .filter(|card| card.kind.settles_on_epoch())
            .all(|card| card.state == Lifecycle::Unknown)
    );
}
