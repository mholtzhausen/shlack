use chrono::{Local, TimeZone};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::App;
use crate::models::ChatSection;

pub enum ChatListRow {
    Header(String),
    Chat(usize),
    Thread(usize),
}

/// Format a Slack ts ("1234567890.123456") as a short HH:MM string for the
/// Threads section label. Falls back to the raw ts if parsing fails.
pub fn format_thread_time(ts: &str) -> String {
    let secs: Option<i64> = ts.split('.').next().and_then(|s| s.parse().ok());
    if let Some(secs) = secs {
        if let Some(dt) = Local.timestamp_opt(secs, 0).single() {
            return dt.format("%H:%M").to_string();
        }
    }
    ts.to_string()
}

/// Build the display rows for the chat list with a "New" section on top.
pub fn build_chat_list_rows(app: &App) -> Vec<ChatListRow> {
    let sections = [
        ChatSection::Public,
        ChatSection::Private,
        ChatSection::Shared,
        ChatSection::Group,
        ChatSection::DirectMessage,
        ChatSection::Bot,
    ];

    let mut rows: Vec<ChatListRow> = Vec::new();

    if !app.threads.is_empty() {
        let label = "Threads".to_string();
        let collapsed = app.settings.collapsed_sections.contains(&label);
        rows.push(ChatListRow::Header(label));
        if !collapsed {
            let mut indices: Vec<usize> = (0..app.threads.len()).collect();
            indices.sort_by(|a, b| {
                app.threads[*b]
                    .last_reply_ts
                    .cmp(&app.threads[*a].last_reply_ts)
            });
            for i in indices {
                rows.push(ChatListRow::Thread(i));
            }
        }
    }

    let new_chats: Vec<usize> = app
        .chats
        .iter()
        .enumerate()
        .filter(|(_, c)| c.unread > 0)
        .map(|(i, _)| i)
        .collect();
    if !new_chats.is_empty() {
        let label = "New".to_string();
        let collapsed = app.settings.collapsed_sections.contains(&label);
        rows.push(ChatListRow::Header(label));
        if !collapsed {
            for idx in new_chats {
                rows.push(ChatListRow::Chat(idx));
            }
        }
    }

    for section in &sections {
        let section_chats: Vec<usize> = app
            .chats
            .iter()
            .enumerate()
            .filter(|(_, c)| c.section == *section && c.unread == 0)
            .map(|(i, _)| i)
            .collect();

        if section_chats.is_empty() {
            continue;
        }

        let label = section.label().to_string();
        let collapsed = app.settings.collapsed_sections.contains(&label);
        rows.push(ChatListRow::Header(label));
        if !collapsed {
            for idx in section_chats {
                rows.push(ChatListRow::Chat(idx));
            }
        }
    }
    rows
}

/// Find the display row index for a given chat index.
pub fn chat_idx_to_row(rows: &[ChatListRow], chat_idx: usize) -> usize {
    rows.iter()
        .position(|r| matches!(r, ChatListRow::Chat(idx) if *idx == chat_idx))
        .unwrap_or(0)
}

pub fn draw_chat_list(app: &mut App, f: &mut Frame, area: Rect) {
    let visible_height = area.height.saturating_sub(2) as usize;
    if visible_height == 0 {
        return;
    }

    let rows = build_chat_list_rows(app);
    let selected_row = chat_idx_to_row(&rows, app.selected_chat_idx);

    let active_channel_id = app
        .panes
        .get(app.focused_pane_idx)
        .and_then(|p| p.channel_id_str.clone());
    let active_thread_ts = app
        .panes
        .get(app.focused_pane_idx)
        .and_then(|p| p.thread_ts.clone());
    let active_chat_idx: Option<usize> = if active_thread_ts.is_some() {
        None
    } else {
        active_channel_id
            .as_ref()
            .and_then(|id| app.chats.iter().position(|c| &c.id == id))
    };
    let active_thread_idx: Option<usize> = match (&active_channel_id, &active_thread_ts) {
        (Some(cid), Some(tts)) => app
            .threads
            .iter()
            .position(|t| &t.channel_id == cid && &t.thread_ts == tts),
        _ => None,
    };

    if selected_row < app.chat_list_scroll_offset {
        app.chat_list_scroll_offset = selected_row;
    } else if selected_row >= app.chat_list_scroll_offset + visible_height {
        app.chat_list_scroll_offset = selected_row + 1 - visible_height;
    }

    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .skip(app.chat_list_scroll_offset)
        .take(visible_height)
        .map(|(_, row)| match row {
            ChatListRow::Header(label) => {
                let collapsed = app.settings.collapsed_sections.contains(label);
                let marker = if collapsed { ">" } else { "v" };
                ListItem::new(Line::from(Span::styled(
                    format!("{} {}", marker, label),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )))
            }
            ChatListRow::Chat(chat_idx) => {
                let chat = &app.chats[*chat_idx];
                let is_active = active_chat_idx == Some(*chat_idx);
                let is_cursor = *chat_idx == app.selected_chat_idx;
                let has_activity = chat.unread > 0;

                let base_style = if is_active {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else if has_activity {
                    Style::default()
                        .bg(Color::Red)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let marker_style = base_style;

                let mut spans = vec![];
                let prefix = if is_cursor && app.focus_on_chat_list && !is_active {
                    "▸ "
                } else if has_activity || is_active {
                    "  "
                } else {
                    "  "
                };
                spans.push(Span::styled(prefix.to_string(), base_style));
                spans.push(Span::styled(chat.name.clone(), base_style));
                if has_activity {
                    spans.push(Span::styled(
                        format!(" ({})", chat.unread),
                        marker_style,
                    ));
                }

                ListItem::new(Line::from(spans)).style(base_style)
            }
            ChatListRow::Thread(thread_idx) => {
                let t = &app.threads[*thread_idx];
                let is_active = active_thread_idx == Some(*thread_idx);
                let is_cursor = app.selected_thread_idx == Some(*thread_idx);
                let highlight = (t.unread > 0) && (t.mentioned || t.on_my_message);

                let base_style = if is_active {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else if highlight {
                    Style::default()
                        .bg(Color::Red)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else if t.unread > 0 {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let prefix = if is_cursor && app.focus_on_chat_list && !is_active {
                    "▸ "
                } else {
                    "  "
                };
                let time = format_thread_time(&t.thread_ts);
                let mut label = format!("↳ {} · {}", t.channel_name, time);
                if t.unread > 0 {
                    label.push_str(&format!(" ({})", t.unread));
                }
                let spans = vec![
                    Span::styled(prefix.to_string(), base_style),
                    Span::styled(label, base_style),
                ];
                ListItem::new(Line::from(spans)).style(base_style)
            }
        })
        .collect();

    let list_block = if app.settings.show_borders {
        Block::default()
            .borders(Borders::ALL)
            .title(if app.focus_on_chat_list {
                "Channels [FOCUSED]"
            } else {
                "Channels"
            })
            .border_style(if app.focus_on_chat_list {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            })
    } else {
        Block::default()
    };
    let list = List::new(items).block(list_block);

    f.render_widget(list, area);
}
