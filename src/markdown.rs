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

    let (text, dim, code_fg, code_bg, heading) = (
        theme.text,
        theme.dim,
        theme.code_fg,
        theme.code_bg,
        theme.heading,
    );

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
        lines.push(Line::from(std::mem::take(spans)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::THEMES;

    fn theme() -> &'static Theme {
        &THEMES[0]
    }

    fn text_content(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn plain_text_renders_as_single_line() {
        let lines = to_lines("hello world", theme());
        let content = text_content(&lines);
        assert!(content.contains("hello world"));
    }

    #[test]
    fn code_block_indented_with_spaces() {
        let lines = to_lines("```\nfoo\n```", theme());
        let code_line = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.contains("foo")));
        assert!(code_line.is_some());
        let first_span = &code_line.unwrap().spans[0];
        assert!(
            first_span.content.starts_with("  "),
            "expected indented code, got: {:?}",
            first_span.content
        );
    }

    #[test]
    fn empty_input_returns_empty_lines() {
        let lines = to_lines("", theme());
        assert!(lines.is_empty());
    }

    #[test]
    fn bullet_list_includes_bullet_char() {
        let lines = to_lines("- item one\n- item two", theme());
        let content = text_content(&lines);
        assert!(
            content.contains('•'),
            "expected bullet char in: {}",
            content
        );
    }

    #[test]
    fn inline_code_renders() {
        let lines = to_lines("use `cargo test`", theme());
        let content = text_content(&lines);
        assert!(content.contains("cargo test"));
    }
}
