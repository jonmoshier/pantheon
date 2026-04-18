use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::theme::Theme;

pub fn to_lines(content: &str, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut bold = false;
    let mut italic = false;
    let mut in_heading = false;

    let (text, dim, code_fg, code_bg, heading) =
        (theme.text, theme.dim, theme.code_fg, theme.code_bg, theme.heading);

    let parser = Parser::new_ext(content, Options::all());

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut spans, &mut lines);
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                lines.push(Line::default());
            }
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                flush(&mut spans, &mut lines);
                lines.push(Line::default());
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush(&mut spans, &mut lines);
                lines.push(Line::default());
            }
            Event::Start(Tag::List(_)) => {}
            Event::End(TagEnd::List(_)) => {
                lines.push(Line::default());
            }
            Event::Start(Tag::Item) => {
                spans.push(Span::styled("• ", Style::default().fg(dim)));
            }
            Event::End(TagEnd::Item) => {
                flush(&mut spans, &mut lines);
            }
            Event::Rule => {
                lines.push(Line::from(Span::styled(
                    "──────────────────────────────────────",
                    Style::default().fg(dim),
                )));
            }
            Event::Text(t) => {
                if in_code_block {
                    for line in t.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(code_fg).bg(code_bg),
                        )));
                    }
                } else {
                    let mut style = Style::default().fg(text);
                    if bold || in_heading {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if in_heading {
                        style = style.fg(heading);
                    }
                    if italic {
                        style = style.add_modifier(Modifier::ITALIC);
                    }
                    spans.push(Span::styled(t.into_string(), style));
                }
            }
            Event::Code(t) => {
                spans.push(Span::styled(t.into_string(), Style::default().fg(code_fg)));
            }
            Event::SoftBreak => {
                spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                flush(&mut spans, &mut lines);
            }
            _ => {}
        }
    }

    flush(&mut spans, &mut lines);
    lines
}

fn flush(spans: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>) {
    if !spans.is_empty() {
        lines.push(Line::from(spans.drain(..).collect::<Vec<_>>()));
    }
}
