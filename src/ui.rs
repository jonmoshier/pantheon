use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, AppMode, Role, MODELS};

const BG: Color = Color::Rgb(13, 13, 13);
const SURFACE: Color = Color::Rgb(24, 24, 24);
const BORDER: Color = Color::Rgb(50, 50, 50);
const BORDER_ACTIVE: Color = Color::Rgb(65, 65, 65);
const DIM: Color = Color::Rgb(80, 80, 80);
const SEP_COLOR: Color = Color::Rgb(30, 30, 30);
const USER_BLUE: Color = Color::Rgb(86, 156, 214);
const TEXT: Color = Color::Rgb(212, 212, 212);
const STATUS_FG: Color = Color::Rgb(90, 90, 90);
const TITLE: Color = Color::Rgb(200, 200, 200);
const ERROR_RED: Color = Color::Rgb(244, 71, 71);

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Full-area background
    f.render_widget(Block::default().style(Style::default().bg(BG)), area);

    // Horizontal padding
    let content = area.inner(Margin { horizontal: 3, vertical: 0 });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(5), // 4 for box + 1 bottom gap
        ])
        .split(content);

    render_messages(f, app, chunks[0]);
    render_status(f, app, chunks[1]);
    render_input(f, app, chunks[2]);

    if matches!(app.mode, AppMode::ModelSelect) {
        render_model_picker(f, app);
    }
}

fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(Span::styled(
        "Pantheon",
        Style::default().fg(TITLE).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Ctrl+P model  ·  Ctrl+X cancel  ·  Alt+Enter newline  ·  /quit",
        Style::default().fg(DIM),
    )));
    lines.push(Line::default());

    let sep_width = area.width as usize;
    let sep = "─".repeat(sep_width.min(120));

    for msg in &app.messages {
        match msg.role {
            Role::User => {
                lines.push(Line::from(Span::styled(
                    "you",
                    Style::default().fg(USER_BLUE).add_modifier(Modifier::BOLD),
                )));
                for text_line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", text_line),
                        Style::default().fg(TEXT),
                    )));
                }
            }
            Role::Assistant => {
                let label = msg.model_label.as_deref().unwrap_or("assistant");
                lines.push(Line::from(Span::styled(
                    label.to_string(),
                    Style::default().fg(DIM).add_modifier(Modifier::BOLD),
                )));
                for md_line in crate::markdown::to_lines(&msg.content) {
                    lines.push(indent_line(md_line));
                }
            }
            Role::System => {
                for text_line in msg.content.lines() {
                    let style = if text_line.starts_with("error") {
                        Style::default().fg(ERROR_RED)
                    } else {
                        Style::default().fg(DIM)
                    };
                    lines.push(Line::from(Span::styled(text_line.to_string(), style)));
                }
            }
        }
        lines.push(Line::from(Span::styled(sep.clone(), Style::default().fg(SEP_COLOR))));
        lines.push(Line::default());
    }

    if app.streaming {
        let spinner = SPINNER[app.spinner_tick as usize % SPINNER.len()];
        let label = app.model().label;
        lines.push(Line::from(Span::styled(
            format!("{} {}", label, spinner),
            Style::default().fg(DIM).add_modifier(Modifier::BOLD),
        )));
        if !app.current_stream.is_empty() {
            for md_line in crate::markdown::to_lines(&app.current_stream) {
                lines.push(indent_line(md_line));
            }
        }
    }

    let total = lines.len() as u16;
    let visible = area.height;
    let max_scroll = total.saturating_sub(visible);
    let scroll = if app.auto_scroll {
        max_scroll
    } else {
        app.scroll_offset.min(max_scroll)
    };
    app.scroll_offset = scroll;

    f.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(BG).fg(TEXT))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let spinner = SPINNER[app.spinner_tick as usize % SPINNER.len()];
    let text = if app.streaming {
        format!("{} {}  streaming", app.model().label, spinner)
    } else {
        app.model().label.to_string()
    };
    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(STATUS_FG).bg(BG)),
        area,
    );
}

fn render_input(f: &mut Frame, app: &mut App, area: Rect) {
    // Leave 1 row bottom gap for the "lifted" look
    let box_area = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));

    let border_color = if app.streaming { BORDER } else { BORDER_ACTIVE };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    f.render_widget(&app.textarea, inner);
}

fn render_model_picker(f: &mut Frame, app: &App) {
    let popup_width = 38u16;
    let popup_height = (MODELS.len() + 2) as u16;
    let area = centered_rect(popup_width, popup_height, f.area());

    f.render_widget(Clear, area);

    let items: Vec<ListItem> = MODELS
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (prefix, style) = if i == app.picker_idx {
                ("  ▸ ", Style::default().fg(USER_BLUE).add_modifier(Modifier::BOLD))
            } else {
                ("    ", Style::default().fg(TEXT))
            };
            ListItem::new(format!("{}{}", prefix, m.label)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER_ACTIVE))
            .style(Style::default().bg(SURFACE))
            .title(" Select Model ")
            .title_style(Style::default().fg(DIM)),
    );

    f.render_widget(list, area);
}

fn indent_line(line: Line<'static>) -> Line<'static> {
    if line.spans.is_empty() {
        return line;
    }
    let mut spans = vec![Span::raw("  ")];
    spans.extend(line.spans);
    Line::from(spans)
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
