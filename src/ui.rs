use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{App, Role};

const BG: Color = Color::Rgb(13, 13, 13);
const SURFACE: Color = Color::Rgb(17, 17, 17);
const BORDER: Color = Color::Rgb(51, 51, 51);
const DIM: Color = Color::Rgb(85, 85, 85);
const USER_BLUE: Color = Color::Rgb(86, 156, 214);
const TEXT: Color = Color::Rgb(212, 212, 212);
const STATUS_FG: Color = Color::Rgb(136, 136, 136);
const TITLE: Color = Color::Rgb(204, 204, 204);
const ERROR_RED: Color = Color::Rgb(244, 71, 71);

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);

    // ── output ────────────────────────────────────────────────────────────

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        "  Pantheon",
        Style::default().fg(TITLE).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "  /model  ·  /quit",
        Style::default().fg(DIM),
    )));
    lines.push(Line::default());

    for msg in &app.messages {
        match msg.role {
            Role::User => {
                lines.push(Line::from(vec![
                    Span::styled("  you  ", Style::default().fg(USER_BLUE).add_modifier(Modifier::BOLD)),
                    Span::styled(msg.content.clone(), Style::default().fg(TEXT)),
                ]));
            }
            Role::Assistant => {
                lines.push(Line::from(Span::styled(
                    format!("  {}", app.model().label),
                    Style::default().fg(DIM),
                )));
                for text_line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", text_line),
                        Style::default().fg(TEXT),
                    )));
                }
            }
            Role::System => {
                for text_line in msg.content.lines() {
                    let style = if text_line.starts_with("error") {
                        Style::default().fg(ERROR_RED)
                    } else {
                        Style::default().fg(DIM)
                    };
                    lines.push(Line::from(Span::styled(format!("  {}", text_line), style)));
                }
            }
        }
        lines.push(Line::default());
    }

    if app.streaming && !app.current_stream.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", app.model().label),
            Style::default().fg(DIM),
        )));
        for text_line in app.current_stream.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", text_line),
                Style::default().fg(TEXT),
            )));
        }
    }

    let total = lines.len() as u16;
    let visible = chunks[0].height;
    let scroll = if app.auto_scroll {
        total.saturating_sub(visible)
    } else {
        app.scroll_offset.min(total.saturating_sub(visible))
    };
    // Keep scroll_offset in sync so manual scrolling starts from the right place
    if app.auto_scroll {
        app.scroll_offset = scroll;
    }

    let output = Paragraph::new(lines)
        .style(Style::default().bg(BG).fg(TEXT))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(output, chunks[0]);

    // ── status bar ────────────────────────────────────────────────────────

    let status_text = if app.streaming {
        format!("  {} · streaming…", app.model().label)
    } else {
        format!("  {}", app.model().label)
    };
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(STATUS_FG).bg(SURFACE));
    f.render_widget(status, chunks[1]);

    // ── input ─────────────────────────────────────────────────────────────

    let input_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));
    let inner = input_block.inner(chunks[2]);
    f.render_widget(input_block, chunks[2]);

    let prefix = "  you >  ";
    let input_text = format!("{}{}", prefix, app.input);
    let input_widget = Paragraph::new(input_text)
        .style(Style::default().fg(TEXT).bg(BG));
    f.render_widget(input_widget, inner);

    let cursor_x = inner.x + prefix.len() as u16 + app.cursor_col();
    let cursor_y = inner.y;
    f.set_cursor_position((cursor_x, cursor_y));
}
