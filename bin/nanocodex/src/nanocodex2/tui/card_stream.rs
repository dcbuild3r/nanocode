// SPDX-License-Identifier: Apache-2.0

//! Card-stream view (F7): the transcript pane replaced by the shared
//! surface-state projection. Records feed one shared reducer through the
//! shared agent-event adapter; this module only routes input and renders.

use super::{theme::Theme, transcript::TranscriptRecord};
use nanocodex_subagents::AgentUpdate;
use nanocodex_surface_state::SurfaceEvent;
use nanocodex_surface_state::{
    SurfaceState,
    agent_events::{AgentEventAdapter, EventScope, ManagedSnapshot},
};
use nanocodex_tui_render::card_stream::{CardStream, max_scroll};
use ratatui::{Frame, layout::Rect};
use serde_json::Value;
use std::{
    cell::Cell,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Liveness ages are wall-clock text; redraw them while the view is open.
const TICK: Duration = Duration::from_secs(1);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[derive(Clone, Default)]
pub(crate) struct CardStreamView {
    state: SurfaceState,
    adapter: AgentEventAdapter,
    open: bool,
    /// Rows scrolled from the top; `None` follows the newest cards.
    scroll: Option<usize>,
    /// The last rendered maximum scroll, for keyboard paging.
    max_scroll: Cell<usize>,
    workspace_shown: bool,
    last_tick: Option<Instant>,
}

impl CardStreamView {
    fn feed(&mut self, kind: &str, payload: &Value, scope: EventScope<'_>, at_ms: u64) {
        for event in self.adapter.observe(kind, payload, scope, at_ms) {
            self.state.apply(&event);
        }
    }

    pub(crate) fn observe(&mut self, record: &TranscriptRecord) {
        let payload = record.decode_payload::<Value>().unwrap_or(Value::Null);
        let turn = record.managed_turn_id();
        let scope = EventScope {
            agent_id: record.managed_agent_id(),
            turn_id: turn.as_deref(),
        };
        self.feed(record.kind(), &payload, scope, record.recorded_at_unix_ms());
    }

    /// Local subagents report through registry updates rather than records.
    pub(crate) fn observe_subagent(&mut self, update: &AgentUpdate) {
        let at_ms = now_ms();
        match update {
            AgentUpdate::Added(descriptor) => {
                if let Ok(id) = descriptor.id.to_string().parse::<u64>()
                    && let Some(event) = self.adapter.agent_role(id, &descriptor.role, at_ms)
                {
                    self.state.apply(&event);
                }
            }
            AgentUpdate::Event { id, event } => {
                let Ok(agent_id) = id.to_string().parse::<u64>() else {
                    return;
                };
                let kind = serde_json::to_value(event.kind).unwrap_or(Value::Null);
                let payload = serde_json::from_str(event.payload.get()).unwrap_or(Value::Null);
                if let Some(kind) = kind.as_str() {
                    let scope = EventScope {
                        agent_id: Some(agent_id),
                        turn_id: None,
                    };
                    self.feed(kind, &payload, scope, at_ms);
                }
            }
            _ => {}
        }
    }

    /// A runtime epoch from the live surface stream.
    pub(crate) fn runtime_epoch(&mut self, epoch: u64, reason: &str, at_ms: u64) {
        let payload = serde_json::json!({"epoch": epoch, "reason": reason});
        self.feed("runtime_epoch", &payload, EventScope::default(), at_ms);
    }

    /// Apply the service's surface snapshot (`GET /v1/agents/:id/surface`)
    /// read on reconnect: open entities come from it, not from inference over
    /// whatever history the gap left behind.
    pub(crate) fn apply_snapshot(&mut self, snapshot: &nanocodex_managed::SurfaceSnapshot) {
        let Ok(snapshot) =
            serde_json::to_value(snapshot).and_then(serde_json::from_value::<ManagedSnapshot>)
        else {
            return;
        };
        let event = self.adapter.snapshot(&snapshot);
        self.state.apply(&event);
    }

    pub(crate) fn set_workspace(&mut self, workspace: &Path) {
        let event = self
            .adapter
            .workspace(&workspace.display().to_string(), now_ms());
        self.state.apply(&event);
        self.workspace_shown = true;
    }

    /// Show the workspace card in a fresh session that replayed no history.
    pub(crate) fn ensure_workspace(&mut self, workspace: &Path) {
        if !self.workspace_shown {
            self.set_workspace(workspace);
        }
    }

    /// Keep the user's view state across a history replay.
    pub(crate) fn preserve_view_from(&mut self, previous: &Self) {
        // A replay rebuilds state from history; the stream is still down.
        if let Some(since) = previous.state.disconnected_since() {
            self.state.apply(&SurfaceEvent::Connection {
                connected: false,
                at_ms: since,
            });
        }
        self.open = previous.open;
        self.scroll = previous.scroll;
    }

    pub(crate) const fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) const fn toggle(&mut self) {
        self.open = !self.open;
        self.scroll = None;
    }

    /// The event stream dropped (`false`) or resumed (`true`).
    pub(crate) fn connection(&mut self, connected: bool) {
        self.state.apply(&SurfaceEvent::Connection {
            connected,
            at_ms: now_ms(),
        });
    }

    /// Next redraw for the live liveness footer while the view is open.
    pub(crate) fn tick_deadline(&self) -> Option<Instant> {
        self.open
            .then(|| self.last_tick.map_or_else(Instant::now, |last| last + TICK))
    }

    /// Whether the view is due for a liveness redraw at `now`.
    pub(crate) fn tick(&mut self, now: Instant) -> bool {
        if self.tick_deadline().is_some_and(|deadline| now >= deadline) {
            self.last_tick = Some(now);
            true
        } else {
            false
        }
    }

    pub(crate) const fn close(&mut self) {
        self.open = false;
    }

    /// Jump to the newest cards and keep following them.
    pub(crate) const fn follow(&mut self) {
        self.scroll = None;
    }

    pub(crate) fn scroll_to_top(&mut self) {
        self.scroll = (self.max_scroll.get() > 0).then_some(0);
    }

    /// Page the stream; paging past the end resumes following new cards.
    pub(crate) fn scroll_by(&mut self, rows: isize) {
        let max = self.max_scroll.get();
        let next = self.scroll.unwrap_or(max).saturating_add_signed(rows);
        self.scroll = (next < max).then_some(next);
    }

    /// The shared projection at `now_ms`; any TUI renderer (card stream,
    /// Mission Control) draws from this instead of keeping its own state.
    pub(crate) fn projection(&self, now_ms: u64) -> nanocodex_surface_state::Projection {
        self.state.project(now_ms)
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let projection = self.projection(now_ms());
        let max = max_scroll(&projection, theme, area);
        self.max_scroll.set(max);
        let scroll = self.scroll.unwrap_or(max).min(max);
        frame.render_widget(CardStream::new(&projection, theme).scroll(scroll), area);
    }
}
