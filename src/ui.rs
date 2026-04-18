use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, AppMode, Role, MODELS};

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let theme = app.theme();

    f.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

    let content = area.inner(Margin { horizontal: 3, vertical: 0 });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(5),
        ])
        .split(content);

    render_messages(f, app, chunks[0]);
    render_status(f, app, chunks[1]);
    render_input(f, app, chunks[2]);

    if matches!(app.mode, AppMode::ModelSelect) {
        render_model_picker(f, app);
    }
    if let AppMode::Confirm(ref desc) = app.mode {
        render_confirm(f, app, desc.clone());
    }
}

fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(Span::styled(
        "Pantheon",
        Style::default().fg(theme.title).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Ctrl+P model  ·  Ctrl+T theme  ·  Ctrl+X cancel  ·  Alt+Enter newline  ·  /quit",
        Style::default().fg(theme.dim),
    )));
    lines.push(Line::default());

    let sep_width = area.width as usize;
    let sep = "─".repeat(sep_width.min(120));

    for msg in &app.messages {
        match msg.role {
            Role::User => {
                lines.push(Line::from(Span::styled(
                    "you",
                    Style::default().fg(theme.user_accent).add_modifier(Modifier::BOLD),
                )));
                for text_line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", text_line),
                        Style::default().fg(theme.text),
                    )));
                }
            }
            Role::Assistant => {
                let label = msg.model_label.as_deref().unwrap_or("assistant");
                lines.push(Line::from(Span::styled(
                    label.to_string(),
                    Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
                )));
                for md_line in crate::markdown::to_lines(&msg.content, theme) {
                    lines.push(indent_line(md_line));
                }
            }
            Role::System => {
                for text_line in msg.content.lines() {
                    let style = if text_line.starts_with("error") {
                        Style::default().fg(theme.error)
                    } else {
                        Style::default().fg(theme.dim)
                    };
                    lines.push(Line::from(Span::styled(text_line.to_string(), style)));
                }
            }
        }
        lines.push(Line::from(Span::styled(sep.clone(), Style::default().fg(theme.sep))));
        lines.push(Line::default());
    }

    if app.streaming {
        let spinner = SPINNER[app.spinner_tick as usize % SPINNER.len()];
        let label = app.model().label;
        lines.push(Line::from(Span::styled(
            format!("{} {}", label, spinner),
            Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
        )));
        if !app.current_stream.is_empty() {
            for md_line in crate::markdown::to_lines(&app.current_stream, theme) {
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
            .style(Style::default().bg(theme.bg).fg(theme.text))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme();
    let spinner = SPINNER[app.spinner_tick as usize % SPINNER.len()];
    let text = if app.streaming {
        format!("{} {}  streaming", app.model().label, spinner)
    } else {
        app.model().label.to_string()
    };
    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(theme.status_fg).bg(theme.bg)),
        area,
    );
}

fn render_input(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let box_area = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));

    let border_color = if app.streaming { theme.border } else { theme.border_active };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme.surface));
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    f.render_widget(&app.textarea, inner);
}

fn render_model_picker(f: &mut Frame, app: &App) {
    let theme = app.theme();
    let popup_width = 38u16;
    let popup_height = (MODELS.len() + 2) as u16;
    let area = centered_rect(popup_width, popup_height, f.area());

    f.render_widget(Clear, area);

    let items: Vec<ListItem> = MODELS
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (prefix, style) = if i == app.picker_idx {
                ("  ▸ ", Style::default().fg(theme.user_accent).add_modifier(Modifier::BOLD))
            } else {
                ("    ", Style::default().fg(theme.text))
            };
            ListItem::new(format!("{}{}", prefix, m.label)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_active))
            .style(Style::default().bg(theme.surface))
            .title(" Select Model ")
            .title_style(Style::default().fg(theme.dim)),
    );

    f.render_widget(list, area);
}

fn render_confirm(f: &mut Frame, app: &App, desc: String) {
    let theme = app.theme();
    let popup_width = (desc.len() as u16 + 6).max(42).min(f.area().width - 4);
    let popup_height = 5u16;
    let area = centered_rect(popup_width, popup_height, f.area());

    f.render_widget(Clear, area);

    let content = vec![
        Line::default(),
        Line::from(Span::styled(
            format!("  {}", desc),
            Style::default().fg(theme.text),
        )),
        Line::default(),
        Line::from(vec![
            Span::styled("  [ Y ] approve", Style::default().fg(theme.user_accent).add_modifier(Modifier::BOLD)),
            Span::styled("   [ N ] deny", Style::default().fg(theme.error)),
        ]),
    ];

    f.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.border_active))
                .style(Style::default().bg(theme.surface))
                .title(" Approve Tool ")
                .title_style(Style::default().fg(theme.dim)),
        ),
        area,
    );
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
