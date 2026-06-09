use chrono::{Local, TimeZone};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, ImageCacheEntry};
use crate::formatting::{format_message_spans, slack_emoji_to_unicode, username_color};
use crate::input::cursor_visual_pos;
use crate::ui::cards::{build_input_preview, render_card, wrap_spans_hanging};
use crate::widgets::ChatPane;

/// Number of terminal rows reserved for an inline image / GIF preview.
const IMAGE_PREVIEW_ROWS: usize = 16;

pub fn draw_chat_pane(
    app: &App,
    f: &mut Frame,
    area: Rect,
    pane: &ChatPane,
    is_focused: bool,
) {
    let has_reply_preview = pane.reply_preview.is_some();
    let header_height = if !app.settings.show_borders {
        2
    } else if app.settings.compact_mode {
        2
    } else {
        3
    };
    let input_height: u16 = 3;
    let constraints = if has_reply_preview {
        vec![
            Constraint::Length(header_height),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(input_height),
        ]
    } else {
        vec![
            Constraint::Length(header_height),
            Constraint::Min(0),
            Constraint::Length(input_height),
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let header_style = if is_focused {
        if app.focus_on_chat_list {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        }
    } else {
        Style::default().fg(Color::Cyan)
    };

    let mut header_text = String::new();
    if is_focused && app.focus_on_chat_list {
        header_text.push_str("[TARGET] ");
    }
    header_text.push_str(&pane.header_text());

    let header = Paragraph::new(header_text)
        .block(if app.settings.show_borders {
            Block::default().borders(Borders::ALL)
        } else {
            Block::default()
        })
        .style(header_style);
    f.render_widget(header, chunks[0]);

    let messages_block = if app.settings.show_borders {
        Block::default().borders(Borders::ALL).title("Messages")
    } else {
        Block::default().padding(Padding::left(2))
    };
    let msg_inner = messages_block.inner(chunks[1]);
    let msg_width = msg_inner.width as usize;
    let msg_area_height = msg_inner.height as usize;

    let show_emojis = app.settings.show_emojis;
    let show_reactions = app.settings.show_reactions;
    let show_line_numbers = app.settings.show_line_numbers;
    let show_timestamps = app.settings.show_timestamps;
    let show_user_colors = app.settings.show_user_colors;
    let user_cache = &app.user_name_cache;
    let resolve_user = |id: &str| -> String {
        user_cache
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_string())
    };
    let format_ts = |ts: &str| -> Option<String> {
        if !show_timestamps {
            return None;
        }
        let secs: i64 = ts.split('.').next()?.parse().ok()?;
        let dt = Local.timestamp_opt(secs, 0).single()?;
        Some(dt.format("%H:%M").to_string())
    };

    let nick_pad_width: usize = pane
        .msg_data
        .iter()
        .map(|m| UnicodeWidthStr::width(m.sender_name.as_str()))
        .max()
        .unwrap_or(0);

    let mut message_lines: Vec<Line> = Vec::new();
    let mut image_overlays: Vec<(usize, String, String)> = Vec::new();
    for (idx, msg) in pane.msg_data.iter().enumerate() {
        let line_start = message_lines.len();
        let name_style = if !show_user_colors {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else if msg.is_outgoing {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        };

        let body_spans = format_message_spans(&msg.text, show_emojis, &resolve_user);

        let mut prefix_spans = Vec::new();

        if msg.is_deleted {
            prefix_spans.push(Span::styled(
                "[DELETED] ",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        if show_line_numbers {
            prefix_spans.push(Span::styled(
                format!("#{} ", idx + 1),
                Style::default().fg(Color::DarkGray),
            ));
        }

        if let Some(ts_fmt) = format_ts(&msg.ts) {
            prefix_spans.push(Span::styled(
                format!("[{}] ", ts_fmt),
                Style::default().fg(Color::DarkGray),
            ));
        }

        let username_style = if !show_user_colors {
            name_style
        } else if msg.is_outgoing {
            name_style
        } else {
            Style::default()
                .fg(username_color(&msg.sender_name))
                .add_modifier(Modifier::BOLD)
        };
        let nick_w = UnicodeWidthStr::width(msg.sender_name.as_str());
        let pad = nick_pad_width.saturating_sub(nick_w);
        prefix_spans.push(Span::styled(
            format!("{}{} ", msg.sender_name, " ".repeat(pad)),
            username_style,
        ));

        let mut content_spans: Vec<Span<'static>> = body_spans;

        if let Some(ref media_type) = msg.media_type {
            let indicator = match media_type.as_str() {
                "image" => "[img]",
                "video" => "[video]",
                _ => "",
            };
            if !indicator.is_empty() {
                content_spans.push(Span::styled(
                    format!(" {}", indicator),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }

        if msg.is_edited && !msg.is_deleted {
            content_spans.push(Span::styled(
                " (edited)",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        if msg.reply_count > 0 {
            content_spans.push(Span::styled(
                format!(" [{} replies]", msg.reply_count),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        if show_reactions && !msg.reactions.is_empty() {
            content_spans.push(Span::raw("  "));
            let pill_style = Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(60, 60, 65));
            for (idx, (name, count)) in msg.reactions.iter().enumerate() {
                if idx > 0 {
                    content_spans.push(Span::raw(" "));
                }
                let emoji = slack_emoji_to_unicode(name);
                let label = if *count > 1 {
                    format!(" {} {} ", emoji, count)
                } else {
                    format!(" {} ", emoji)
                };
                content_spans.push(Span::styled(label, pill_style));
            }
        }

        let prefix_width = spans_width(&prefix_spans);
        let indent = " ".repeat(prefix_width);
        let indent_width = UnicodeWidthStr::width(indent.as_str());
        let first_width = msg_width.saturating_sub(prefix_width);
        let rest_width = msg_width.saturating_sub(indent_width);
        let mut wrapped =
            wrap_spans_hanging(&content_spans, first_width, rest_width, indent.as_str());
        if wrapped.is_empty() {
            wrapped.push(Vec::new());
        }
        let mut first_line = prefix_spans;
        first_line.extend(wrapped.remove(0));
        message_lines.push(Line::from(first_line));
        for line in wrapped {
            message_lines.push(Line::from(line));
        }

        for card in &msg.cards {
            let card_lines = render_card(card, msg_width, show_emojis, &resolve_user);
            for line in card_lines {
                message_lines.push(line);
            }
        }

        if app.settings.show_image_preview
            && msg.media_type.as_deref() == Some("image")
            && !msg.file_urls.is_empty()
        {
            let url = msg.file_urls[0].clone();
            let name = msg
                .file_names
                .first()
                .cloned()
                .unwrap_or_else(|| "image".to_string());
            let abs_line = message_lines.len();
            for _ in 0..IMAGE_PREVIEW_ROWS {
                message_lines.push(Line::default());
            }
            image_overlays.push((abs_line, url, name));
        }

        {
            let block_bg = crate::formatting::CODE_BLOCK_BG;
            for li in line_start..message_lines.len() {
                let has_block = message_lines[li]
                    .spans
                    .iter()
                    .any(|s| s.style.bg == Some(block_bg));
                if !has_block {
                    continue;
                }
                let line = std::mem::take(&mut message_lines[li]);
                let mut visible_w = 0usize;
                for s in &line.spans {
                    visible_w += s
                        .content
                        .chars()
                        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
                        .sum::<usize>();
                }
                let mut new_spans: Vec<Span<'static>> = line
                    .spans
                    .into_iter()
                    .map(|s| Span::styled(s.content.into_owned(), s.style))
                    .collect();
                if visible_w < msg_width {
                    let pad = msg_width - visible_w;
                    new_spans.push(Span::styled(
                        " ".repeat(pad),
                        Style::default().bg(block_bg),
                    ));
                }
                message_lines[li] = Line::from(new_spans);
            }
        }

        if msg.mentions_me {
            let red = Style::default().bg(Color::Red).fg(Color::White);
            for li in line_start..message_lines.len() {
                let line = std::mem::take(&mut message_lines[li]);
                let mut new_spans: Vec<Span<'static>> =
                    Vec::with_capacity(line.spans.len() + 1);
                let mut visible_w = 0usize;
                for s in line.spans.into_iter() {
                    let w: usize = s
                        .content
                        .chars()
                        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
                        .sum();
                    visible_w += w;
                    let combined = s.style.patch(red);
                    new_spans.push(Span::styled(s.content.into_owned(), combined));
                }
                if visible_w < msg_width {
                    let pad = msg_width - visible_w;
                    new_spans.push(Span::styled(" ".repeat(pad), red));
                }
                message_lines[li] = Line::from(new_spans);
            }
        }
    }

    let messages = Paragraph::new(message_lines).block(messages_block);

    let vertical_space = if app.settings.show_borders { 2u16 } else { 0u16 };
    let total_wrapped_lines = messages
        .line_count(msg_inner.width)
        .saturating_sub(vertical_space as usize);
    let max_scroll = total_wrapped_lines.saturating_sub(msg_area_height);
    let scroll_offset = pane.scroll_offset.min(max_scroll);

    let messages = messages.scroll((scroll_offset as u16, 0));

    f.render_widget(messages, chunks[1]);

    if !image_overlays.is_empty() {
        let viewport_h = msg_inner.height as i32;
        let scroll = scroll_offset as i32;
        for (abs_line, url, name) in &image_overlays {
            let rel_top = *abs_line as i32 - scroll;
            let rel_bot = rel_top + IMAGE_PREVIEW_ROWS as i32;
            if rel_bot <= 0 || rel_top >= viewport_h {
                continue;
            }
            let clip_top = rel_top.max(0);
            let clip_bot = rel_bot.min(viewport_h);
            let h = (clip_bot - clip_top) as u16;
            if h == 0 || msg_inner.width == 0 {
                continue;
            }
            let rect = Rect {
                x: msg_inner.x,
                y: msg_inner.y + clip_top as u16,
                width: msg_inner.width,
                height: h,
            };

            if app.image_picker.is_none() {
                let p = Paragraph::new(format!("[image: {}]", name))
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(p, rect);
                continue;
            }

            let needs_load = !app.image_cache.borrow().contains_key(url);
            if needs_load {
                app.image_cache
                    .borrow_mut()
                    .insert(url.clone(), ImageCacheEntry::Loading);
                app.spawn_image_load(url.clone(), name.clone());
            }

            let mut cache = app.image_cache.borrow_mut();
            match cache.get_mut(url) {
                Some(ImageCacheEntry::Loading) => {
                    let p = Paragraph::new("[loading image...]")
                        .style(Style::default().fg(Color::DarkGray));
                    f.render_widget(p, rect);
                }
                Some(ImageCacheEntry::Failed(msg)) => {
                    let p = Paragraph::new(format!("[image failed: {}]", msg))
                        .style(Style::default().fg(Color::Red));
                    f.render_widget(p, rect);
                }
                Some(ImageCacheEntry::Loaded(proto)) => {
                    use ratatui::widgets::StatefulWidget;
                    let img_widget = ratatui_image::StatefulImage::<
                        ratatui_image::protocol::StatefulProtocol,
                    >::default();
                    StatefulWidget::render(img_widget, rect, f.buffer_mut(), proto);
                }
                None => {}
            }
        }
    }

    if has_reply_preview {
        if let Some(ref preview) = pane.reply_preview {
            let reply_bar =
                Paragraph::new(preview.as_str()).style(Style::default().fg(Color::Yellow));
            f.render_widget(reply_bar, chunks[2]);
        }
    }

    let input_chunk = if has_reply_preview {
        chunks[3]
    } else {
        chunks[2]
    };
    let input_style = if is_focused && !app.focus_on_chat_list {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Gray)
    };

    let top_margin: u16 = 1;
    let bottom_margin: u16 = 1;
    let input_inner = Rect {
        x: input_chunk.x,
        y: input_chunk.y + top_margin,
        width: input_chunk.width,
        height: input_chunk.height.saturating_sub(top_margin + bottom_margin),
    };
    let (cursor_line, cursor_col) = cursor_visual_pos(
        pane.input_buffer.as_str(),
        pane.input_cursor,
        input_inner.width as usize,
    );
    let input_scroll = if input_inner.height > 0 {
        cursor_line.saturating_sub(input_inner.height as usize - 1)
    } else {
        0
    };

    let input_text = build_input_preview(
        pane.input_buffer.as_str(),
        app.settings.show_emojis,
        input_style,
    );
    let input = Paragraph::new(input_text)
        .style(input_style)
        .wrap(Wrap { trim: false })
        .scroll((input_scroll as u16, 0));

    f.render_widget(input, input_inner);

    if is_focused && !app.focus_on_chat_list {
        let cursor_y = input_inner.y + cursor_line.saturating_sub(input_scroll) as u16;
        let cursor_x = input_inner.x + cursor_col as u16;
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn spans_width(spans: &[Span]) -> usize {
    spans
        .iter()
        .flat_map(|span| span.content.chars())
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}
