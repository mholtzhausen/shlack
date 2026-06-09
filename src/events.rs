use std::time::{Duration, Instant};

use crate::app::App;
use crate::messages::realtime_message_to_message_data;
use crate::models::{ChatSection, ThreadInfo};
use crate::slack::SlackUpdate;
use crate::utils::send_desktop_notification_ex;

const UNREAD_SAVE_DEBOUNCE: Duration = Duration::from_secs(5);

/// True when `thread_ts` points at a parent message other than this message's own `ts`.
pub(crate) fn is_thread_reply(ts: &str, thread_ts: &Option<String>) -> bool {
    matches!(thread_ts.as_ref(), Some(t) if t != ts)
}

/// Thread unread is bumped for replies from others while the thread is not focused.
pub(crate) fn should_bump_thread_unread(is_self: bool, in_focused_thread: bool) -> bool {
    !is_self && !in_focused_thread
}

/// Whether a thread reply should appear in the Threads sidebar section.
pub(crate) fn thread_concerns_me(
    on_my_message: bool,
    mentions_me: bool,
    already_known: bool,
    i_replied_now: bool,
) -> bool {
    on_my_message || mentions_me || already_known || i_replied_now
}

/// Returns true when a debounced unread/threads save is due.
pub(crate) fn debounced_save_due(last_save: Option<Instant>, now: Instant) -> bool {
    last_save
        .map(|t| now.duration_since(t) >= UNREAD_SAVE_DEBOUNCE)
        .unwrap_or(true)
}

pub fn apply_update(app: &mut App, update: SlackUpdate) {
    match update {
        SlackUpdate::NewMessage {
            channel_id,
            user_name,
            text,
            ts,
            thread_ts,
            is_bot,
            is_self,
            cards,
            inline_image_urls,
            mentions_me,
            files,
        } => {
            let msg_data = realtime_message_to_message_data(
                &user_name,
                &text,
                &ts,
                is_self,
                &cards,
                mentions_me,
                &files,
                &inline_image_urls,
            );
            let is_thread_reply = is_thread_reply(&ts, &thread_ts);
            let root_thread_ts = thread_ts.clone().unwrap_or_else(|| ts.clone());

            // Track our own messages so we can later detect when
            // someone starts a thread on one of them.
            if is_self {
                app.my_message_ts.insert((channel_id.clone(), ts.clone()));
            }

            // Update the Threads section if this is a reply we care about.
            if is_thread_reply {
                let parent_ts = thread_ts.clone().unwrap_or_else(|| ts.clone());
                let on_my_message = app
                    .my_message_ts
                    .contains(&(channel_id.clone(), parent_ts.clone()));
                let already_known = app
                    .threads
                    .iter()
                    .any(|t| t.channel_id == channel_id && t.thread_ts == parent_ts);
                let i_replied_now = is_self;
                let concerns_me = thread_concerns_me(
                    on_my_message,
                    mentions_me,
                    already_known,
                    i_replied_now,
                );
                if concerns_me {
                    let channel_name = app
                        .chats
                        .iter()
                        .find(|c| c.id == channel_id)
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| channel_id.clone());

                    // Don't bump unread for replies in a thread we're
                    // currently viewing, or replies we wrote ourselves.
                    let in_focused_thread = app
                        .panes
                        .get(app.focused_pane_idx)
                        .map(|p| {
                            p.channel_id_str.as_deref() == Some(channel_id.as_str())
                                && p.thread_ts.as_deref() == Some(parent_ts.as_str())
                        })
                        .unwrap_or(false);
                    let bump_unread = should_bump_thread_unread(is_self, in_focused_thread);

                    if let Some(t) = app
                        .threads
                        .iter_mut()
                        .find(|t| t.channel_id == channel_id && t.thread_ts == parent_ts)
                    {
                        t.last_reply_ts = ts.clone();
                        t.last_reply_user = Some(user_name.clone());
                        if bump_unread {
                            t.unread = t.unread.saturating_add(1);
                        }
                        if mentions_me {
                            t.mentioned = true;
                        }
                        if i_replied_now {
                            t.i_replied = true;
                        }
                        if on_my_message {
                            t.on_my_message = true;
                        }
                    } else {
                        app.threads.push(ThreadInfo {
                            channel_id: channel_id.clone(),
                            channel_name,
                            thread_ts: parent_ts.clone(),
                            last_reply_ts: ts.clone(),
                            unread: if bump_unread { 1 } else { 0 },
                            mentioned: mentions_me,
                            on_my_message,
                            i_replied: i_replied_now,
                            last_reply_user: Some(user_name.clone()),
                        });
                    }
                    app.threads_dirty = true;
                }
            }

            // Update panes showing this channel/thread
            let mut seen_in_open_pane = false;
            for pane in &mut app.panes {
                if let Some(ref pane_channel_id) = pane.channel_id_str {
                    if *pane_channel_id == channel_id {
                        match &pane.thread_ts {
                            Some(pane_thread) => {
                                if let Some(msg_thread) = &thread_ts {
                                    if pane_thread == msg_thread {
                                        // Check if message already exists (by timestamp)
                                        let already_exists =
                                            pane.msg_data.iter().any(|m| m.ts == ts);

                                        if !already_exists {
                                            // Remove local echo if this is our own message
                                            if is_self {
                                                if let Some(pos) = pane.msg_data.iter().rposition(|m| {
                                                    m.text == text
                                                        && m.is_outgoing
                                                        && m.local_echo_id.is_some()
                                                }) {
                                                    pane.msg_data.remove(pos);
                                                }
                                            }

                                            pane.msg_data.push(msg_data.clone());
                                            pane.invalidate_cache();
                                            pane.scroll_offset = usize::MAX;
                                            seen_in_open_pane = true;
                                        }
                                    }
                                }
                            }
                            None => {
                                if is_thread_reply {
                                    if let Some(parent) = pane
                                        .msg_data
                                        .iter_mut()
                                        .find(|m| m.ts == root_thread_ts)
                                    {
                                        parent.reply_count = parent.reply_count.saturating_add(1);
                                    }
                                } else {
                                    // Check if message already exists (by timestamp)
                                    let already_exists =
                                        pane.msg_data.iter().any(|m| m.ts == ts);

                                    if !already_exists {
                                        // Remove local echo if this is our own message
                                        if is_self {
                                            // Find and remove the most recent local echo with matching text
                                            // that has a local_echo_id (to avoid removing old messages)
                                            if let Some(pos) = pane.msg_data.iter().rposition(|m| {
                                                m.text == text
                                                    && m.is_outgoing
                                                    && m.local_echo_id.is_some()
                                            }) {
                                                pane.msg_data.remove(pos);
                                            }
                                        }

                                        pane.msg_data.push(msg_data.clone());
                                        pane.invalidate_cache();
                                        pane.scroll_offset = usize::MAX;
                                        seen_in_open_pane = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Mark channel as unread if it's not currently the focused pane.
            // We still show an unread marker for channels visible in non-focused panes
            // so the user clearly sees where new activity happened.
            let is_focused_channel = app
                .panes
                .get(app.focused_pane_idx)
                .and_then(|p| p.channel_id_str.as_deref())
                .map(|cid| cid == channel_id.as_str())
                .unwrap_or(false);
            let focused_pane_saw_it =
                seen_in_open_pane && is_focused_channel && !app.focus_on_chat_list;

            let known_channel = app.chats.iter().any(|c| c.id == channel_id);
            if let Some(chat) = app.chats.iter_mut().find(|c| c.id == channel_id) {
                if focused_pane_saw_it {
                    if chat.unread != 0 {
                        chat.unread = 0;
                        app.unread_dirty = true;
                    }
                } else if !is_self {
                    chat.unread = chat.unread.saturating_add(1);
                    app.unread_dirty = true;
                }
            } else if !is_self {
                // Unknown channel (e.g. new DM, newly added channel) – refresh the
                // chat list from the API so it appears in the sidebar.
                app.pending_refresh_chats = true;
            }
            let _ = known_channel;

            app.needs_redraw = true;

            // Desktop notifications: notify for DMs and group chats on every
            // new message, and for any channel/bot when the user is mentioned.
            // Suppress notifications for the user's own messages, and when the
            // currently-focused pane already showed the message.
            if app.settings.show_notifications && !is_self && !focused_pane_saw_it {
                let chat_section = app
                    .chats
                    .iter()
                    .find(|c| c.id == channel_id)
                    .map(|c| c.section);
                let is_direct = matches!(
                    chat_section,
                    Some(ChatSection::DirectMessage) | Some(ChatSection::Group)
                );

                let should_notify = mentions_me || (is_direct && !is_bot);

                if should_notify {
                    let channel_name = app
                        .chats
                        .iter()
                        .find(|c| c.id == channel_id)
                        .map(|c| c.name.clone())
                        .or_else(|| {
                            app.panes
                                .iter()
                                .find(|p| {
                                    p.channel_id_str.as_deref() == Some(channel_id.as_str())
                                })
                                .map(|p| p.chat_name.clone())
                        })
                        .unwrap_or_else(|| channel_id.clone());

                    let workspace_name = app
                        .config
                        .workspaces
                        .get(app.config.active_workspace)
                        .map(|w| w.name.clone())
                        .unwrap_or_default();

                    let (title, urgency) = if mentions_me {
                        *app.unread_mentions
                            .entry(workspace_name.clone())
                            .or_insert(0) += 1;
                        (
                            format!(
                                "Slack [{}]: {} - mention",
                                workspace_name, channel_name
                            ),
                            "critical",
                        )
                    } else {
                        (
                            format!("Slack [{}]: {}", workspace_name, channel_name),
                            "normal",
                        )
                    };

                    let _ = send_desktop_notification_ex(
                        &title,
                        &format!("{}: {}", user_name, text),
                        urgency,
                    );
                }
            }
        }
        SlackUpdate::MessageChanged {
            channel_id,
            ts,
            new_text,
        } => {
            // Update the message in all panes showing this channel
            for pane in &mut app.panes {
                if let Some(ref pane_channel_id) = pane.channel_id_str {
                    if *pane_channel_id == channel_id {
                        // Find and update the message
                        if let Some(msg) = pane.msg_data.iter_mut().find(|m| m.ts == ts) {
                            msg.text = new_text.clone();
                            msg.is_edited = true;
                            pane.invalidate_cache();
                            app.needs_redraw = true;
                        }
                    }
                }
            }
        }
        SlackUpdate::MessageDeleted { channel_id, ts } => {
            // Mark the message as deleted in all panes
            for pane in &mut app.panes {
                if let Some(ref pane_channel_id) = pane.channel_id_str {
                    if *pane_channel_id == channel_id {
                        // Find and mark as deleted
                        if let Some(msg) = pane.msg_data.iter_mut().find(|m| m.ts == ts) {
                            msg.is_deleted = true;
                            msg.text = "[Message deleted]".to_string();
                            pane.invalidate_cache();
                            app.needs_redraw = true;
                        }
                    }
                }
            }
        }
        SlackUpdate::UserTyping {
            channel_id,
            user_name,
        } => {
            for pane in &mut app.panes {
                if let Some(ref pane_channel_id) = pane.channel_id_str {
                    if pane_channel_id == &channel_id {
                        pane.show_typing_indicator(&user_name);
                        break;
                    }
                }
            }
            app.needs_redraw = true;
        }
    }
}

pub fn maybe_debounced_save(app: &mut App) {
    // Debounced persistence of unread counters and threads: at most once
    // every 5s. Both flags share one save_state() call.
    if app.unread_dirty || app.threads_dirty {
        let now = Instant::now();
        if debounced_save_due(app.last_unread_save, now) {
            if app.save_state().is_ok() {
                app.unread_dirty = false;
                app.threads_dirty = false;
                app.last_unread_save = Some(now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_thread_reply_when_parent_differs() {
        assert!(is_thread_reply("100.001", &Some("100.000".to_string())));
        assert!(!is_thread_reply("100.001", &Some("100.001".to_string())));
        assert!(!is_thread_reply("100.001", &None));
    }

    #[test]
    fn should_bump_thread_unread_skips_self_and_focused() {
        assert!(!should_bump_thread_unread(true, false));
        assert!(!should_bump_thread_unread(true, true));
        assert!(!should_bump_thread_unread(false, true));
        assert!(should_bump_thread_unread(false, false));
    }

    #[test]
    fn thread_concerns_me_any_trigger() {
        assert!(!thread_concerns_me(false, false, false, false));
        assert!(thread_concerns_me(true, false, false, false));
        assert!(thread_concerns_me(false, true, false, false));
        assert!(thread_concerns_me(false, false, true, false));
        assert!(thread_concerns_me(false, false, false, true));
    }

    #[test]
    fn debounced_save_due_respects_interval() {
        let now = Instant::now();
        assert!(debounced_save_due(None, now));
        assert!(!debounced_save_due(
            Some(now - Duration::from_secs(2)),
            now
        ));
        assert!(debounced_save_due(
            Some(now - Duration::from_secs(5)),
            now
        ));
    }
}
