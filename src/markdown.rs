use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

const TEXT: Color = Color::Rgb(212, 212, 212);
const DIM: Color = Color::Rgb(85, 85, 85);
const CODE_FG: Color = Color::Rgb(206, 145, 120);
const CODE_BG: Color = Color::Rgb(30, 30, 30);
const HEADING_COLOR: Color = Color::Rgb(86, 156, 214);

pub fn to_lines(content: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut bold = false;
    let mut italic = false;
    let mut in_heading = false;

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
                spans.push(Span::styled("• ", Style::default().fg(DIM)));
            }
            Event::End(TagEnd::Item) => {
                flush(&mut spans, &mut lines);
            }
            Event::Rule => {
                lines.push(Line::from(Span::styled(
                    "──────────────────────────────────────",
                    Style::default().fg(DIM),
                )));
            }
            Event::Text(text) => {
                if in_code_block {
                    for line in text.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(CODE_FG).bg(CODE_BG),
                        )));
                    }
                } else {
                    let mut style = Style::default().fg(TEXT);
                    if bold || in_heading {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if in_heading {
                        style = style.fg(HEADING_COLOR);
                    }
                    if italic {
                        style = style.add_modifier(Modifier::ITALIC);
                    }
                    spans.push(Span::styled(text.into_string(), style));
                }
            }
            Event::Code(text) => {
                spans.push(Span::styled(
                    text.into_string(),
                    Style::default().fg(CODE_FG),
                ));
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
