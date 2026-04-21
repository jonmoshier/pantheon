use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AppMode, Role};

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let theme = app.theme();

    f.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

    let content = area.inner(Margin {
        horizontal: 3,
        vertical: 0,
    });

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

    if matches!(app.mode, AppMode::Help) {
        render_help_dialog(f, app);
    }
}

fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(Span::styled(
        "Pantheon",
        Style::default()
            .fg(theme.title)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Ctrl+P model  ·  Ctrl+T theme  ·  Ctrl+X cancel  ·  Alt+Enter newline  ·  /help for commands",
        Style::default().fg(theme.dim),
    )));
    lines.push(Line::default());

    let sep_width = area.width as usize;
    let sep = "─".repeat(sep_width.min(120));

    // If no messages, show startup hint
    if app.messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "Type /help for a list of commands.",
            Style::default().fg(theme.dim),
        )));
        lines.push(Line::default());
    }

    for msg in &app.messages {
        match msg.role {
            Role::User => {
                lines.push(Line::from(Span::styled(
                    "you",
                    Style::default()
                        .fg(theme.user_accent)
                        .add_modifier(Modifier::BOLD),
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
        lines.push(Line::from(Span::styled(
            sep.clone(),
            Style::default().fg(theme.sep),
        )));
        lines.push(Line::default());
    }

    if app.streaming && !app.current_stream.is_empty() {
        for md_line in crate::markdown::to_lines(&app.current_stream, theme) {
            lines.push(indent_line(md_line));
        }
    }

    let total = wrapped_line_count(&lines, area.width);
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

    let (left, fg) = if let AppMode::Confirm(ref desc) = app.mode {
        (
            format!("▶ {}   [ Y ] approve   [ N ] deny", desc),
            theme.user_accent,
        )
    } else if let Some((ref msg, _)) = app.status_msg {
        (msg.clone(), theme.dim)
    } else if app.streaming {
        let spinner = SPINNER[app.spinner_tick as usize % SPINNER.len()];
        let base = app.model().label.to_string();
        let label = match &app.resolved_model {
            Some(id) if id != &app.model().id => format!("{} ({})", base, id),
            _ => base,
        };
        let tok_s = app
            .stream_start
            .map(|t| {
                let secs = t.elapsed().as_secs_f64();
                if secs > 0.2 {
                    let toks = (app.stream_chars as f64 / 4.0) / secs;
                    format!("  {:.0} tok/s", toks)
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
        (format!("{} {}{}", label, spinner, tok_s), theme.status_fg)
    } else {
        let base = app.model().label.to_string();
        let label = match &app.resolved_model {
            Some(id) if id != &app.model().id => format!("{} ({})", base, id),
            _ => base,
        };
        (label, theme.status_fg)
    };

    let cwd = std::env::current_dir()
        .map(|p| {
            let home = std::env::var("HOME").unwrap_or_default();
            let s = p.to_string_lossy().to_string();
            if !home.is_empty() && s.starts_with(&home) {
                format!("~{}", &s[home.len()..])
            } else {
                s
            }
        })
        .unwrap_or_default();

    let line = Line::from(vec![
        Span::styled(left, Style::default().fg(fg)),
        Span::styled(format!("  ·  {}", cwd), Style::default().fg(theme.dim)),
    ]);

    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme.bg)),
        area,
    );
}

fn render_input(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let box_area = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));

    let border_color = if app.streaming {
        theme.border
    } else {
        theme.border_active
    };
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

    let max_label = app.models.iter().map(|m| m.label.len()).max().unwrap_or(10);
    // prefix(4) + label + gap(2) + ctx(5) + gap(2) + price(9) + inner padding(2)
    let inner_width = 4 + max_label + 2 + 5 + 2 + 9 + 4; // +4 right padding
    let popup_width = (inner_width + 2) as u16; // +2 for borders
    let popup_height = (app.models.len() + 2) as u16;
    let area = centered_rect(popup_width, popup_height, f.area());

    f.render_widget(Clear, area);

    let items: Vec<ListItem> = app
        .models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (prefix, style) = if i == app.picker_idx {
                (
                    "▸ ",
                    Style::default()
                        .fg(theme.user_accent)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ", Style::default().fg(theme.dim))
            };

            let ctx = match m.context_window {
                Some(n) if n >= 1_000_000 => format!("{:>3}M", n / 1_000_000),
                Some(n) => format!("{:>3}K", n / 1_000),
                None => "   —".to_string(),
            };

            let price = match m.cost_per_mtok_input {
                Some(p) if p == 0.0 => "    free ".to_string(),
                Some(p) => format!("${:>6.2}/M", p),
                None => "         ".to_string(),
            };

            let row = format!(
                "  {}{:<label_w$}  {:>5}  {}",
                prefix,
                m.label,
                ctx,
                price,
                label_w = max_label,
            );
            ListItem::new(row).style(style)
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

#[allow(clippy::vec_init_then_push)]
fn render_help_dialog(f: &mut Frame, app: &App) {
    let theme = app.theme();
    let popup_width = 70u16;
    let popup_height = 18u16;
    let area = centered_rect(popup_width, popup_height, f.area());

    f.render_widget(Clear, area);

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Title
    lines.push(Line::from(Span::styled(
        "SLASH COMMANDS",
        Style::default()
            .fg(theme.user_accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "  /help              Show this help dialog",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  /model             Open model picker",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  /theme             Show available themes",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  /save [name]       Save conversation to ~/.pantheon/conversations/",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  /load [name]       Load a saved conversation (no name = list saves)",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  /context           Show model, cwd, and loaded context files",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  /clear             Clear conversation history",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  /quit              Exit Pantheon",
        Style::default().fg(theme.text),
    )));

    lines.push(Line::default());

    lines.push(Line::from(Span::styled(
        "KEYBINDINGS",
        Style::default()
            .fg(theme.user_accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "  Enter              Send message",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  Alt+Up / Alt+Down  Navigate input history",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  Alt+Enter          Insert newline",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  Ctrl+P             Open model picker",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  Ctrl+T             Cycle theme",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  Ctrl+X             Cancel request",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  Page Up/Down       Scroll history",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  Ctrl+C / Ctrl+D    Quit",
        Style::default().fg(theme.text),
    )));
    lines.push(Line::from(Span::styled(
        "  Esc                Close dialog",
        Style::default().fg(theme.text),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_active))
        .style(Style::default().bg(theme.surface))
        .title(" Help ")
        .title_style(Style::default().fg(theme.dim));

    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().bg(theme.surface).fg(theme.text)),
        area,
    );
}

fn wrapped_line_count(lines: &[Line<'static>], width: u16) -> u16 {
    if width == 0 {
        return lines.len().min(u16::MAX as usize) as u16;
    }
    let total: u32 = lines
        .iter()
        .map(|line| {
            let chars: u32 = line
                .spans
                .iter()
                .map(|s| s.content.chars().count() as u32)
                .sum();
            if chars == 0 {
                1
            } else {
                ((chars - 1) / width as u32) + 1
            }
        })
        .sum();
    total.min(u16::MAX as u32) as u16
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
