use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::formatting::format_message_spans;

/// Parse Slack attachment color into a ratatui Color.
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.trim();
    match s.to_ascii_lowercase().as_str() {
        "good" => return Some(Color::Green),
        "warning" => return Some(Color::Yellow),
        "danger" => return Some(Color::Red),
        _ => {}
    }
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Convert the raw compose-input buffer into styled lines for live preview.
/// Each `\n` in the buffer starts a new Line; mrkdwn markers stay visible
/// as text so cursor positions computed against the raw buffer line up.
pub fn build_input_preview(buffer: &str, show_emojis: bool, base_style: Style) -> Text<'static> {
    let resolve_noop = |s: &str| s.to_string();
    let spans = format_message_spans(buffer, show_emojis, &resolve_noop);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    for span in spans {
        let style = base_style.patch(span.style);
        let content = span.content.into_owned();
        let mut first = true;
        for piece in content.split('\n') {
            if !first {
                lines.push(Line::from(std::mem::take(&mut current)));
            }
            first = false;
            if !piece.is_empty() {
                current.push(Span::styled(piece.to_string(), style));
            }
        }
    }
    lines.push(Line::from(current));
    Text::from(lines)
}

/// Render an attachment card as a compact bordered box with a colored side bar.
/// Layout:
///   ╭ @author · *Title*
///   │ pretext (italic dim)
///   │ body line 1
///   │ body line 2
///   │ footer (dim)
///   ╰
pub fn render_card(
    card: &crate::widgets::AttachmentCard,
    width: usize,
    show_emojis: bool,
    resolve_user: &dyn Fn(&str) -> String,
) -> Vec<Line<'static>> {
    let bar_color = card
        .color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or(Color::Cyan);
    let bar_style = Style::default().fg(bar_color);

    let header_open: &str = "╭ ";
    let body_bar: &str = "│ ";
    let footer_close: &str = "╰";
    let bar_width = UnicodeWidthStr::width(body_bar);
    let inner_first = width.saturating_sub(bar_width).max(1);
    let indent_str = " ".repeat(bar_width);

    let mut out: Vec<Line<'static>> = Vec::new();

    // ----- header (top corner + author/title) -----
    let mut header_spans: Vec<Span<'static>> = vec![Span::styled(header_open.to_string(), bar_style)];
    let author = card.author.clone().filter(|s| !s.is_empty());
    let title = card.title.clone().filter(|s| !s.is_empty());
    if let Some(a) = author.as_ref() {
        header_spans.push(Span::styled(
            format!("@{}", a),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if author.is_some() && title.is_some() {
        header_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
    }
    if let Some(t) = title.as_ref() {
        // Underline the title when title_link is present, signaling clickability.
        let mut title_style = Style::default().add_modifier(Modifier::BOLD);
        if card.title_link.as_ref().filter(|s| !s.is_empty()).is_some() {
            title_style = title_style.add_modifier(Modifier::UNDERLINED);
        }
        header_spans.push(Span::styled(t.clone(), title_style));
    }
    if author.is_none() && title.is_none() {
        // Top corner only
        header_spans.push(Span::raw(""));
    }
    out.push(Line::from(header_spans));

    // Helper to apply a base style on top of the per-span styles produced by
    // the markdown parser (so e.g. dim footer text still shows bold/italic markers).
    let overlay = |spans: Vec<Span<'static>>, base: Style| -> Vec<Span<'static>> {
        spans
            .into_iter()
            .map(|s| {
                let combined = base.patch(s.style);
                Span::styled(s.content.into_owned(), combined)
            })
            .collect()
    };

    // ----- pretext -----
    if let Some(pretext) = card.pretext.as_ref().filter(|s| !s.is_empty()) {
        let parsed = format_message_spans(pretext, show_emojis, resolve_user);
        let base = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);
        let spans = overlay(parsed, base);
        let wrapped = wrap_spans_hanging(&spans, inner_first, inner_first, &indent_str);
        for piece in wrapped {
            let mut line = vec![Span::styled(body_bar.to_string(), bar_style)];
            line.extend(piece);
            out.push(Line::from(line));
        }
    }

    // ----- body -----
    const MAX_BODY_LINES: usize = 24;
    let mut body_count = 0usize;
    let mut truncated = false;
    'outer: for source_line in card.body.split('\n') {
        let parsed = format_message_spans(source_line, show_emojis, resolve_user);
        let spans: Vec<Span<'static>> = if parsed.is_empty() {
            vec![Span::raw(String::new())]
        } else {
            parsed
        };
        let mut wrapped = wrap_spans_hanging(&spans, inner_first, inner_first, &indent_str);
        if wrapped.is_empty() {
            wrapped.push(Vec::new());
        }
        for piece in wrapped {
            if body_count >= MAX_BODY_LINES {
                truncated = true;
                break 'outer;
            }
            let mut line = vec![Span::styled(body_bar.to_string(), bar_style)];
            line.extend(piece);
            out.push(Line::from(line));
            body_count += 1;
        }
    }
    if truncated {
        let mut line = vec![Span::styled(body_bar.to_string(), bar_style)];
        line.push(Span::styled(
            "…".to_string(),
            Style::default().fg(Color::DarkGray),
        ));
        out.push(Line::from(line));
    }

    // ----- footer -----
    if let Some(footer) = card.footer.as_ref().filter(|s| !s.is_empty()) {
        let parsed = format_message_spans(footer, show_emojis, resolve_user);
        let base = Style::default().fg(Color::DarkGray);
        let spans = overlay(parsed, base);
        let wrapped = wrap_spans_hanging(&spans, inner_first, inner_first, &indent_str);
        for piece in wrapped {
            let mut line = vec![Span::styled(body_bar.to_string(), bar_style)];
            line.extend(piece);
            out.push(Line::from(line));
        }
    }

    // ----- bottom corner -----
    out.push(Line::from(vec![Span::styled(
        footer_close.to_string(),
        bar_style,
    )]));

    out
}

pub fn wrap_spans_hanging(
    spans: &[Span],
    first_width: usize,
    rest_width: usize,
    indent: &str,
) -> Vec<Vec<Span<'static>>> {
    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut remaining = first_width.max(1);
    let rest_width = rest_width.max(1);
    let indent_style = spans.first().map(|span| span.style).unwrap_or_default();
    let mut line_has_content = false;

    let start_new_line = |lines: &mut Vec<Vec<Span<'static>>>,
                          current: &mut Vec<Span<'static>>,
                          remaining: &mut usize,
                          line_has_content: &mut bool| {
        lines.push(std::mem::take(current));
        if !indent.is_empty() {
            current.push(Span::styled(indent.to_string(), indent_style));
        }
        *remaining = rest_width;
        *line_has_content = false;
    };

    for span in spans {
        let style = span.style;
        let mut text = span.content.as_ref();
        while !text.is_empty() {
            let (segment, next) = if let Some(pos) = text.find('\n') {
                (&text[..pos], Some(&text[pos + 1..]))
            } else {
                (text, None)
            };

            if !segment.is_empty() {
                let mut tokens: Vec<(String, bool)> = Vec::new();
                let mut buf = String::new();
                let mut buf_is_space: Option<bool> = None;
                for ch in segment.chars() {
                    let is_space = ch.is_whitespace();
                    if let Some(current_space) = buf_is_space {
                        if current_space == is_space {
                            buf.push(ch);
                        } else {
                            tokens.push((std::mem::take(&mut buf), current_space));
                            buf.push(ch);
                            buf_is_space = Some(is_space);
                        }
                    } else {
                        buf.push(ch);
                        buf_is_space = Some(is_space);
                    }
                }
                if let Some(current_space) = buf_is_space {
                    if !buf.is_empty() {
                        tokens.push((buf, current_space));
                    }
                }

                for (token, is_space) in tokens {
                    let token_width = UnicodeWidthStr::width(token.as_str());
                    if is_space {
                        if line_has_content && token_width <= remaining {
                            current.push(Span::styled(token, style));
                            remaining = remaining.saturating_sub(token_width);
                        }
                        continue;
                    }

                    if token_width <= remaining {
                        current.push(Span::styled(token, style));
                        remaining = remaining.saturating_sub(token_width);
                        line_has_content = true;
                        continue;
                    }

                    if line_has_content {
                        start_new_line(&mut lines, &mut current, &mut remaining, &mut line_has_content);
                    }

                    if token_width <= remaining {
                        current.push(Span::styled(token, style));
                        remaining = remaining.saturating_sub(token_width);
                        line_has_content = true;
                        continue;
                    }

                    let mut word_buf = String::new();
                    for ch in token.chars() {
                        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
                        if line_has_content && width > remaining {
                            if !word_buf.is_empty() {
                                current.push(Span::styled(std::mem::take(&mut word_buf), style));
                            }
                            start_new_line(&mut lines, &mut current, &mut remaining, &mut line_has_content);
                        }
                        if remaining == 0 && line_has_content {
                            if !word_buf.is_empty() {
                                current.push(Span::styled(std::mem::take(&mut word_buf), style));
                            }
                            start_new_line(&mut lines, &mut current, &mut remaining, &mut line_has_content);
                        }

                        word_buf.push(ch);
                        remaining = remaining.saturating_sub(width);
                        line_has_content = true;
                    }
                    if !word_buf.is_empty() {
                        current.push(Span::styled(word_buf, style));
                    }
                }
            }

            if next.is_some() {
                start_new_line(&mut lines, &mut current, &mut remaining, &mut line_has_content);
            }
            if let Some(next_text) = next {
                text = next_text;
            } else {
                break;
            }
        }
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}
