use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::widgets::TabCompleteState;

pub async fn handle_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        // --- Quit ---
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.save_state()?;
            return Ok(true);
        }

        // --- Toggles (Ctrl+*) ---
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.refresh_chats().await?;
        }
        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.split_vertical();
        }
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.split_horizontal();
        }
        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_split_direction();
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.close_pane();
        }
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_chat_list();
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.clear_pane();
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_reactions();
        }
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_emojis();
        }
        // Some terminals report Ctrl+P as DC1 ('\u{10}') without CONTROL modifier.
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_image_preview();
        }
        KeyCode::Char('\u{10}') => {
            app.toggle_image_preview();
        }
        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_timestamps();
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_compact_mode();
        }
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_line_numbers();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_user_colors();
        }
        KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_borders();
        }
        KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_mouse_support();
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.show_workspace_list();
        }
        KeyCode::Char(c @ '1'..='9') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let workspace_idx = (c as u8 - b'1') as usize;
            app.switch_workspace(workspace_idx);
        }

        // --- Navigation ---
        KeyCode::Tab => {
            if !app.focus_on_chat_list
                && !app.panes[app.focused_pane_idx].input_buffer.is_empty()
            {
                tab_complete(app);
            } else {
                app.next_pane();
            }
        }
        KeyCode::Enter if !app.focus_on_chat_list => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                input_newline(app);
            } else {
                app.send_message().await?;
            }
        }
        KeyCode::Enter if app.focus_on_chat_list => {
            if let Some(idx) = app.selected_thread_idx {
                app.pending_open_thread = Some(idx);
                app.open_pending_thread().await?;
            } else {
                app.open_selected_chat().await?;
            }
        }
        KeyCode::Char(' ') if app.focus_on_chat_list => {
            app.toggle_selected_section();
        }
        KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.scroll_up();
        }
        KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.scroll_down();
        }
        KeyCode::Up => {
            if app.focus_on_chat_list {
                app.select_previous_chat();
            } else if app.panes[app.focused_pane_idx].input_buffer.is_empty() {
                app.scroll_up();
            } else {
                move_cursor_up(app);
            }
        }
        KeyCode::Down => {
            if app.focus_on_chat_list {
                app.select_next_chat();
            } else if app.panes[app.focused_pane_idx].input_buffer.is_empty() {
                app.scroll_down();
            } else {
                move_cursor_down(app);
            }
        }
        KeyCode::PageUp if !app.focus_on_chat_list => {
            app.page_up();
        }
        KeyCode::PageDown if !app.focus_on_chat_list => {
            app.page_down();
        }
        KeyCode::Home if !app.focus_on_chat_list => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                app.scroll_to_top();
            } else {
                move_cursor_home(app);
            }
        }
        KeyCode::End if !app.focus_on_chat_list => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                app.scroll_to_bottom();
            } else {
                move_cursor_end(app);
            }
        }
        KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.resize_chat_list(-2);
        }
        KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.resize_chat_list(2);
        }
        KeyCode::Left if !app.focus_on_chat_list => {
            move_cursor_left(app);
        }
        KeyCode::Right if !app.focus_on_chat_list => {
            move_cursor_right(app);
        }

        // --- Editing ---
        KeyCode::Backspace if !app.focus_on_chat_list => {
            backspace(app);
        }
        KeyCode::Delete if !app.focus_on_chat_list => {
            delete_forward(app);
        }
        KeyCode::Esc => {
            cancel_reply(app);
        }
        KeyCode::Char(c) if !app.focus_on_chat_list && !key.modifiers.contains(KeyModifiers::CONTROL) => {
            input_char(app, c);
        }

        _ => {}
    }

    Ok(false)
}

fn input_char(app: &mut App, c: char) {
    app.ensure_valid_pane_idx();
    let pane = &mut app.panes[app.focused_pane_idx];
    pane.input_buffer.insert(pane.input_cursor, c);
    pane.input_cursor += c.len_utf8();
    pane.tab_complete_state = None;
}

fn backspace(app: &mut App) {
    app.ensure_valid_pane_idx();
    let pane = &mut app.panes[app.focused_pane_idx];
    if pane.input_cursor == 0 {
        return;
    }
    let prev = prev_char_boundary(&pane.input_buffer, pane.input_cursor);
    pane.input_buffer.drain(prev..pane.input_cursor);
    pane.input_cursor = prev;
    pane.tab_complete_state = None;
}

fn delete_forward(app: &mut App) {
    let pane = &mut app.panes[app.focused_pane_idx];
    if pane.input_cursor >= pane.input_buffer.len() {
        return;
    }
    let next = next_char_boundary(&pane.input_buffer, pane.input_cursor);
    pane.input_buffer.drain(pane.input_cursor..next);
    pane.tab_complete_state = None;
}

fn input_newline(app: &mut App) {
    app.ensure_valid_pane_idx();
    let pane = &mut app.panes[app.focused_pane_idx];
    pane.input_buffer.insert(pane.input_cursor, '\n');
    pane.input_cursor += 1;
    pane.tab_complete_state = None;
}

fn move_cursor_left(app: &mut App) {
    let pane = &mut app.panes[app.focused_pane_idx];
    if pane.input_cursor == 0 {
        return;
    }
    pane.input_cursor = prev_char_boundary(&pane.input_buffer, pane.input_cursor);
    pane.tab_complete_state = None;
}

fn move_cursor_right(app: &mut App) {
    let pane = &mut app.panes[app.focused_pane_idx];
    if pane.input_cursor >= pane.input_buffer.len() {
        return;
    }
    pane.input_cursor = next_char_boundary(&pane.input_buffer, pane.input_cursor);
    pane.tab_complete_state = None;
}

fn move_cursor_home(app: &mut App) {
    let pane = &mut app.panes[app.focused_pane_idx];
    let (line_start, _) = line_bounds(&pane.input_buffer, pane.input_cursor);
    pane.input_cursor = line_start;
    pane.tab_complete_state = None;
}

fn move_cursor_end(app: &mut App) {
    let pane = &mut app.panes[app.focused_pane_idx];
    let (_, line_end) = line_bounds(&pane.input_buffer, pane.input_cursor);
    pane.input_cursor = line_end;
    pane.tab_complete_state = None;
}

fn move_cursor_up(app: &mut App) {
    let pane = &mut app.panes[app.focused_pane_idx];
    let (line_start, _) = line_bounds(&pane.input_buffer, pane.input_cursor);
    if line_start == 0 {
        return;
    }
    let target_col = column_in_line(&pane.input_buffer, line_start, pane.input_cursor);
    let prev_line_end = line_start.saturating_sub(1);
    let prev_line_start = pane.input_buffer[..prev_line_end]
        .rfind('\n')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let new_cursor = index_from_column(
        &pane.input_buffer,
        prev_line_start,
        prev_line_end,
        target_col,
    );
    pane.input_cursor = new_cursor.min(pane.input_buffer.len());
    pane.tab_complete_state = None;
}

fn move_cursor_down(app: &mut App) {
    let pane = &mut app.panes[app.focused_pane_idx];
    let (line_start, line_end) = line_bounds(&pane.input_buffer, pane.input_cursor);
    if line_end >= pane.input_buffer.len() {
        return;
    }
    let target_col = column_in_line(&pane.input_buffer, line_start, pane.input_cursor);
    let next_line_start = line_end + 1;
    let next_line_end = pane.input_buffer[next_line_start..]
        .find('\n')
        .map(|idx| next_line_start + idx)
        .unwrap_or_else(|| pane.input_buffer.len());
    let new_cursor = index_from_column(
        &pane.input_buffer,
        next_line_start,
        next_line_end,
        target_col,
    );
    pane.input_cursor = new_cursor.min(pane.input_buffer.len());
    pane.tab_complete_state = None;
}

fn tab_complete(app: &mut App) {
    app.ensure_valid_pane_idx();
    let pane = &mut app.panes[app.focused_pane_idx];

    if let Some(ref mut state) = pane.tab_complete_state {
        if state.candidates.is_empty() {
            return;
        }
        state.index = (state.index + 1) % state.candidates.len();
        let replacement = &state.candidates[state.index];

        if state.before.starts_with('/') {
            pane.input_buffer = format!("/{} {}", replacement, state.after);
            pane.input_cursor = replacement.len() + 2;
        } else {
            pane.input_buffer = format!("{}@{} {}", state.before, replacement, state.after);
            pane.input_cursor = state.before.len() + replacement.len() + 2;
        }
    } else {
        let input = &pane.input_buffer;
        let cursor = pane.input_cursor.min(input.len());
        let before_cursor = &input[..cursor];

        if before_cursor.starts_with('/') && !before_cursor.contains(' ') {
            let prefix = &before_cursor[1..];
            let prefix_lower = prefix.to_lowercase();

            let commands = vec![
                "thread", "t", "react", "filter", "alias", "unalias", "workspace", "ws", "leave",
                "help", "h",
            ];

            let mut candidates: Vec<String> = commands
                .into_iter()
                .filter(|cmd| cmd.starts_with(&prefix_lower))
                .map(|s| s.to_string())
                .collect();

            if candidates.is_empty() {
                return;
            }

            candidates.sort();
            let after = input[cursor..].to_string();
            let replacement = &candidates[0];

            pane.input_buffer = format!("/{} {}", replacement, after);
            pane.input_cursor = replacement.len() + 2;

            pane.tab_complete_state = Some(TabCompleteState {
                before: "/".to_string(),
                after,
                candidates,
                index: 0,
            });
            return;
        }

        let at_pos = before_cursor.rfind('@');
        if at_pos.is_none() {
            return;
        }
        let at_pos = at_pos.unwrap();
        let prefix = &before_cursor[at_pos + 1..];
        if prefix.is_empty() || prefix.contains(' ') {
            return;
        }
        let prefix_lower = prefix.to_lowercase();

        let mut candidates: Vec<String> = app
            .user_name_cache
            .values()
            .filter(|name| name.to_lowercase().starts_with(&prefix_lower))
            .cloned()
            .collect();
        candidates.sort();
        candidates.dedup();

        if candidates.is_empty() {
            return;
        }

        let replacement = &candidates[0];
        let before = input[..at_pos].to_string();
        let after = input[cursor..].to_string();
        pane.input_buffer = format!("{}@{} {}", before, replacement, after);
        pane.input_cursor = before.len() + replacement.len() + 2;

        pane.tab_complete_state = Some(TabCompleteState {
            before,
            after,
            candidates,
            index: 0,
        });
    }
}

fn cancel_reply(app: &mut App) {
    let pane = &mut app.panes[app.focused_pane_idx];
    pane.reply_to_message = None;
    pane.hide_reply_preview();
}

pub fn cursor_visual_pos(s: &str, cursor: usize, width: usize) -> (usize, usize) {
    if width == 0 {
        return (0, 0);
    }
    let mut line = 0;
    let mut col = 0;
    for (byte_idx, ch) in s.char_indices() {
        if byte_idx >= cursor {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
            continue;
        }
        col += 1;
        if col >= width {
            line += 1;
            col = 0;
        }
    }
    (line, col)
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    s[..idx].char_indices().last().map(|(i, _)| i).unwrap_or(0)
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut iter = s[idx..].char_indices();
    iter.next();
    if let Some((next_i, _)) = iter.next() {
        idx + next_i
    } else {
        s.len()
    }
}

fn line_bounds(s: &str, cursor: usize) -> (usize, usize) {
    let cursor = cursor.min(s.len());
    let line_start = s[..cursor]
        .rfind('\n')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let line_end = s[cursor..]
        .find('\n')
        .map(|idx| cursor + idx)
        .unwrap_or_else(|| s.len());
    (line_start, line_end)
}

fn column_in_line(s: &str, line_start: usize, cursor: usize) -> usize {
    s[line_start..cursor.min(s.len())].chars().count()
}

fn index_from_column(s: &str, line_start: usize, line_end: usize, target_col: usize) -> usize {
    let mut col = 0;
    for (byte_idx, _) in s[line_start..line_end].char_indices() {
        if col >= target_col {
            return line_start + byte_idx;
        }
        col += 1;
    }
    line_end
}
