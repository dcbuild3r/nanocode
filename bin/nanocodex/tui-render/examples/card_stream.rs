// SPDX-License-Identifier: Apache-2.0

//! Drive the card-stream renderer from a shared surface-state fixture.
//!
//! ```text
//! card_stream frames <fixture.json> [--width N] [--height N]
//! card_stream live   <fixture.json>
//! ```
//!
//! `frames` replays each fixture step through the shared reducer and prints
//! the rendered terminal buffer. `live` draws the same frames in the real
//! terminal (alternate screen): n/space advances one step, j/k scroll, q quits.

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use nanocodex_surface_state::{Projection, SurfaceEvent, SurfaceState};
use nanocodex_tui_render::{card_stream::CardStream, theme::Theme};
use ratatui::{
    Terminal,
    backend::{CrosstermBackend, TestBackend},
    text::Line,
};
use serde::Deserialize;
use std::{error::Error, fs, io};

#[derive(Deserialize)]
struct Fixture {
    name: String,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    label: String,
    now_ms: u64,
    events: Vec<SurfaceEvent>,
}

fn frames(fixture: &Fixture) -> Vec<(String, Projection)> {
    let mut state = SurfaceState::new();
    fixture
        .steps
        .iter()
        .map(|step| {
            for event in &step.events {
                state.apply(event);
            }
            (step.label.clone(), state.project(step.now_ms))
        })
        .collect()
}

fn flag(args: &[String], name: &str, default: u16) -> u16 {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn print_frames(fixture: &Fixture, width: u16, height: u16) -> Result<(), Box<dyn Error>> {
    let theme = Theme::default();
    for (index, (label, projection)) in frames(fixture).iter().enumerate() {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal
            .draw(|frame| frame.render_widget(CardStream::new(projection, &theme), frame.area()))?;
        println!(
            "=== {} · step {}: {label} ({width}x{height})",
            fixture.name,
            index + 1
        );
        let buffer = terminal.backend().buffer();
        for y in 0..height {
            let row: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            println!("{}", row.trim_end());
        }
        println!();
    }
    Ok(())
}

fn live(fixture: &Fixture) -> Result<(), Box<dyn Error>> {
    let theme = Theme::default();
    let frames = frames(fixture);
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let (mut step, mut scroll) = (0usize, 0usize);
    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            let (label, projection) = &frames[step];
            terminal.draw(|frame| {
                let area = frame.area();
                let title = format!(
                    "{} · step {}/{}: {label}   [n]ext [j/k] scroll [q]uit",
                    fixture.name,
                    step + 1,
                    frames.len()
                );
                frame.render_widget(Line::from(title), area);
                let mut body = area;
                body.y += 1;
                body.height = body.height.saturating_sub(1);
                frame.render_widget(CardStream::new(projection, &theme).scroll(scroll), body);
            })?;
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('n' | ' ') => {
                        step = (step + 1).min(frames.len() - 1);
                        scroll = 0;
                    }
                    KeyCode::Char('j') | KeyCode::Down => scroll += 1,
                    KeyCode::Char('k') | KeyCode::Up => scroll = scroll.saturating_sub(1),
                    _ => {}
                }
            }
        }
    })();
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(mode), Some(path)) = (args.first(), args.get(1)) else {
        eprintln!("usage: card_stream <frames|live> <fixture.json> [--width N] [--height N]");
        std::process::exit(2);
    };
    let fixture: Fixture = serde_json::from_str(&fs::read_to_string(path)?)?;
    match mode.as_str() {
        "frames" => print_frames(
            &fixture,
            flag(&args, "--width", 100),
            flag(&args, "--height", 40),
        ),
        "live" => live(&fixture),
        other => Err(format!("unknown mode {other}").into()),
    }
}
