// SPDX-License-Identifier: Apache-2.0

//! Card-stream rendering of a shared surface-state [`Projection`].
//!
//! The terminal draws exactly the cards, labels, badges and lines the shared
//! projection produces (the web mirror renders the same projection), so the
//! TUI performs no lifecycle inference of its own. Cards nest by task-tree
//! depth; pinned Status cards stay above the scrolled stream and the liveness
//! footer stays on the last row.

use crate::theme::Theme;
use nanocodex_surface_state::{Card, EntityKind, Projection, Tone};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const INDENT: usize = 2;
const MIN_CARD_WIDTH: usize = 24;

/// Terminal color of a shared tone.
#[must_use]
pub const fn tone_color(theme: &Theme, tone: Tone) -> Color {
    match tone {
        Tone::Text => theme.text(),
        Tone::Muted => theme.muted(),
        Tone::Info => Color::Blue,
        Tone::Live => Color::Cyan,
        Tone::Ok => Color::Green,
        Tone::Warn => Color::Yellow,
        Tone::Error => Color::Red,
    }
}

const fn kind_label(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Agent => "AGENT",
        EntityKind::Turn => "TURN",
        EntityKind::ToolCall => "TOOL",
        EntityKind::ProcessSession => "PROC",
        EntityKind::Run => "RUN",
        EntityKind::Hand => "HAND",
        EntityKind::Brain => "BRAIN",
        EntityKind::Decision => "DECIDE",
        EntityKind::Status => "STATUS",
    }
}

/// Truncate to `width` display columns, ending with an ellipsis when cut.
fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Greedy word wrap to `width` columns, hard-breaking overlong words.
/// Leading indentation is preserved on continuation lines.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let indent: String = text.chars().take_while(|ch| *ch == ' ').collect();
    let indent = if indent.width() * 2 > width {
        String::new()
    } else {
        indent
    };
    let mut lines = Vec::new();
    let mut current = indent.clone();
    let mut current_has_word = false;
    for word in text.split_whitespace() {
        let sep = usize::from(current_has_word);
        if current.width() + sep + word.width() <= width {
            if current_has_word {
                current.push(' ');
            }
            current.push_str(word);
            current_has_word = true;
            continue;
        }
        if current_has_word {
            lines.push(std::mem::replace(&mut current, indent.clone()));
        }
        let mut rest = word;
        while indent.width() + rest.width() > width {
            let room = width - indent.width();
            let mut cut = 0;
            let mut used = 0;
            for (index, ch) in rest.char_indices() {
                let w = ch.width().unwrap_or(0);
                if used + w > room.max(1) {
                    break;
                }
                used += w;
                cut = index + ch.len_utf8();
            }
            if cut == 0 {
                cut = rest.chars().next().map_or(rest.len(), char::len_utf8);
            }
            lines.push(format!("{indent}{}", &rest[..cut]));
            rest = &rest[cut..];
        }
        current = format!("{indent}{rest}");
        current_has_word = !rest.is_empty();
    }
    if current_has_word || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn pad(text: &str, width: usize) -> String {
    let used = text.width();
    if used >= width {
        text.to_owned()
    } else {
        format!("{text}{}", " ".repeat(width - used))
    }
}

/// Render one card into terminal lines `width` columns wide.
#[must_use]
pub fn card_lines(card: &Card, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let indent = (usize::try_from(card.depth)
        .unwrap_or(usize::MAX)
        .saturating_mul(INDENT))
    .min(width.saturating_sub(MIN_CARD_WIDTH));
    let card_width = width.saturating_sub(indent).max(8);
    let inner = card_width.saturating_sub(4);
    let tone = Style::new().fg(tone_color(theme, card.tone));
    let border = if card.pinned || matches!(card.tone, Tone::Warn | Tone::Error) {
        tone
    } else {
        Style::new().fg(theme.border())
    };
    let gutter = Span::raw(" ".repeat(indent));
    let mut lines = Vec::new();

    // ╭─ KIND Title subject ───── state ─╮
    let kind = kind_label(card.kind);
    let state = truncate(&card.state_label, inner / 2);
    let fixed = 3 + kind.width() + 1 + 2 + state.width() + 3;
    let available = card_width.saturating_sub(fixed);
    let title = truncate(&card.title, available);
    let subject = card.subject.as_deref().map_or_else(String::new, |subject| {
        let room = available.saturating_sub(title.width() + 1);
        if room < 4 {
            String::new()
        } else {
            truncate(subject, room)
        }
    });
    let used = fixed
        + title.width()
        + if subject.is_empty() {
            0
        } else {
            subject.width() + 1
        };
    let fill = card_width.saturating_sub(used);
    let mut header = vec![
        gutter.clone(),
        Span::styled("╭─ ", border),
        Span::styled(
            kind,
            Style::new().fg(theme.muted()).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            title,
            Style::new().fg(theme.text()).add_modifier(Modifier::BOLD),
        ),
    ];
    if !subject.is_empty() {
        header.push(Span::raw(" "));
        header.push(Span::styled(subject, Style::new().fg(theme.text())));
    }
    header.push(Span::raw(" "));
    header.push(Span::styled("─".repeat(fill), border));
    header.push(Span::raw(" "));
    header.push(Span::styled(state, tone.add_modifier(Modifier::BOLD)));
    header.push(Span::styled(" ─╮", border));
    lines.push(Line::from(header));

    let body = |spans: Vec<Span<'static>>, used: usize| {
        let mut row = vec![gutter.clone(), Span::styled("│ ", border)];
        row.extend(spans);
        row.push(Span::raw(" ".repeat(inner.saturating_sub(used))));
        row.push(Span::styled(" │", border));
        Line::from(row)
    };

    // Badges, wrapped as whole chips.
    let mut chips: Vec<Span<'static>> = Vec::new();
    let mut chips_width = 0;
    for badge in &card.badges {
        let chip = truncate(&format!("[{}]", badge.text), inner);
        let sep = usize::from(chips_width > 0);
        if chips_width + sep + chip.width() > inner && chips_width > 0 {
            lines.push(body(std::mem::take(&mut chips), chips_width));
            chips_width = 0;
        }
        if chips_width > 0 {
            chips.push(Span::raw(" "));
            chips_width += 1;
        }
        chips_width += chip.width();
        chips.push(Span::styled(
            chip,
            Style::new().fg(tone_color(theme, badge.tone)),
        ));
    }
    if !chips.is_empty() {
        lines.push(body(chips, chips_width));
    }

    for card_line in &card.lines {
        let style = Style::new().fg(tone_color(theme, card_line.tone));
        for wrapped in wrap(&card_line.text, inner) {
            let used = wrapped.width();
            lines.push(body(vec![Span::styled(pad(&wrapped, 0), style)], used));
        }
    }

    lines.push(Line::from(vec![
        gutter,
        Span::styled(
            format!("╰{}╯", "─".repeat(card_width.saturating_sub(2))),
            border,
        ),
    ]));
    lines
}

/// The pinned cards and the scrollable stream, rendered at `width`.
#[must_use]
pub fn stream_lines(
    projection: &Projection,
    theme: &Theme,
    width: u16,
) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let mut pinned = Vec::new();
    let mut stream = Vec::new();
    for card in &projection.cards {
        let target = if card.pinned {
            &mut pinned
        } else {
            &mut stream
        };
        target.extend(card_lines(card, theme, width));
    }
    (pinned, stream)
}

/// Liveness footer line.
#[must_use]
pub fn liveness_line(projection: &Projection, theme: &Theme, width: u16) -> Line<'static> {
    let liveness = &projection.liveness;
    let style = Style::new().fg(tone_color(theme, liveness.tone));
    let marker = if liveness.tone == Tone::Ok {
        "●"
    } else {
        "▲"
    };
    Line::from(vec![
        Span::styled(format!("{marker} "), style),
        Span::styled(
            truncate(&liveness.label, usize::from(width).saturating_sub(2)),
            style,
        ),
    ])
}

/// Largest useful scroll offset for the stream at `area`; scrolling to it
/// follows the newest cards.
#[must_use]
pub fn max_scroll(projection: &Projection, theme: &Theme, area: Rect) -> usize {
    if area.height == 0 {
        return 0;
    }
    let (pinned, stream) = stream_lines(projection, theme, area.width);
    let body_height = usize::from(area.height - 1);
    let pinned_rows = pinned.len().min(body_height / 2);
    stream.len().saturating_sub(body_height - pinned_rows)
}

/// Card-stream widget: pinned Status cards (at most half the height), the
/// scrolled stream, and the liveness footer.
pub struct CardStream<'a> {
    projection: &'a Projection,
    theme: &'a Theme,
    scroll: usize,
}

impl<'a> CardStream<'a> {
    #[must_use]
    pub const fn new(projection: &'a Projection, theme: &'a Theme) -> Self {
        Self {
            projection,
            theme,
            scroll: 0,
        }
    }

    /// Rows of the scrolled stream to skip.
    #[must_use]
    pub const fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }
}

impl Widget for CardStream<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let (pinned, stream) = stream_lines(self.projection, self.theme, area.width);
        let footer_y = area.y + area.height - 1;
        let body_height = usize::from(area.height - 1);
        let pinned_rows = pinned.len().min(body_height / 2);
        let mut y = area.y;
        for line in pinned.into_iter().take(pinned_rows) {
            buf.set_line(area.x, y, &line, area.width);
            y += 1;
        }
        let rows = body_height - pinned_rows;
        let scroll = self.scroll.min(stream.len().saturating_sub(rows));
        for line in stream.into_iter().skip(scroll).take(rows) {
            buf.set_line(area.x, y, &line, area.width);
            y += 1;
        }
        let footer = liveness_line(self.projection, self.theme, area.width);
        buf.set_line(area.x, footer_y, &footer, area.width);
    }
}
