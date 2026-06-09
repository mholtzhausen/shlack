use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::App;
use crate::models::ChatSection;
use crate::ui::{chat_list, chat_pane};
use crate::widgets::ChatPane;

pub fn draw(app: &mut App, f: &mut Frame) {
    let has_status = app.status_message.is_some();

    let current_workspace_name = app
        .config
        .workspaces
        .get(app.config.active_workspace)
        .map(|w| w.name.clone())
        .unwrap_or_default();
    let other_workspace_mentions: Vec<(String, u32)> = app
        .unread_mentions
        .iter()
        .filter(|(ws_name, count)| **ws_name != current_workspace_name && **count > 0)
        .map(|(name, count)| (name.clone(), *count))
        .collect();
    let has_other_mentions = !other_workspace_mentions.is_empty();

    let mut main_constraints: Vec<Constraint> = vec![Constraint::Min(0)];
    if has_other_mentions {
        main_constraints.push(Constraint::Length(1));
    }
    if has_status {
        main_constraints.push(Constraint::Length(1));
    }
    main_constraints.push(Constraint::Length(1));

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_constraints)
        .split(f.area());

    let (chat_area, pane_area) = if app.settings.show_chat_list {
        let chat_list_width = if let Some(w) = app.settings.chat_list_width {
            let total = outer[0].width;
            let max_w = total.saturating_sub(20).max(10);
            w.clamp(10, max_w)
        } else {
            let max_name_len = app
                .chats
                .iter()
                .map(|c| {
                    let prefix = if c.unread > 0 {
                        format!("({}) ", c.unread)
                    } else {
                        String::new()
                    };
                    let emoji = match c.section {
                        ChatSection::Public => "# ",
                        ChatSection::Private => "🔒 ",
                        ChatSection::Shared => "🔗 ",
                        ChatSection::DirectMessage => "👤 ",
                        ChatSection::Group => "👥 ",
                        ChatSection::Bot => "🤖 ",
                    };
                    prefix.len() + emoji.len() + c.name.len()
                })
                .max()
                .unwrap_or(20);

            (max_name_len + 6).min(40).max(15) as u16
        };

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(chat_list_width), Constraint::Min(0)])
            .split(outer[0]);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, outer[0])
    };

    if let Some(area) = chat_area {
        app.chat_list_area = Some(area);
        chat_list::draw_chat_list(app, f, area);
    } else {
        app.chat_list_area = None;
    }

    let render_fn = |f: &mut Frame, area: Rect, pane: &ChatPane, is_focused: bool| {
        chat_pane::draw_chat_pane(app, f, area, pane, is_focused);
    };

    let mut pane_areas = std::collections::HashMap::new();
    app.pane_tree.render(
        f,
        pane_area,
        &app.panes,
        app.focused_pane_idx,
        &render_fn,
        &mut pane_areas,
    );
    app.pane_areas = pane_areas;

    let mut footer_idx = outer.len() - 1;

    let stats_text = match app.last_realtime_event {
        Some(t) => {
            let age = t.elapsed().as_secs();
            let state = if age >= 30 { "stale" } else { "ok" };
            format!(" RT {} {}s", state, age)
        }
        None => " RT —".to_string(),
    };
    let stats = Paragraph::new(stats_text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default());
    f.render_widget(stats, outer[footer_idx]);

    if has_status {
        footer_idx -= 1;
        let status = Paragraph::new(app.status_message.as_ref().unwrap().clone())
            .style(Style::default().fg(Color::White))
            .block(Block::default());
        f.render_widget(status, outer[footer_idx]);
    }

    if has_other_mentions {
        footer_idx -= 1;
        let mention_text: String = other_workspace_mentions
            .iter()
            .map(|(name, count)| format!("{}: {}@", name, count))
            .collect::<Vec<_>>()
            .join(" | ");
        let notification = Paragraph::new(format!(
            " Mentions in other workspaces: {} (Ctrl+N to switch)",
            mention_text
        ))
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default());
        f.render_widget(notification, outer[footer_idx]);
    }
}
