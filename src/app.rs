use anyhow::Result;
use ratatui::{
    layout::Rect,
    Frame,
};

use crate::commands::CommandHandler;
use crate::config::Config;
use crate::ui::chat_list::{self, ChatListRow};
use crate::messages::{
    local_echo_message_data, message_mentions_user, slack_message_to_message_data,
    slack_message_to_message_data_with_reply_count,
};
use crate::persistence::{Aliases, AppSettings, AppState, LayoutData};

pub use crate::models::{ChatInfo, ThreadInfo};
use crate::slack::{SlackClient, SlackMessage};
use crate::split_view::{PaneNode, SplitDirection};
use crate::widgets::ChatPane;

pub struct App {
    pub config: Config,
    pub slack: SlackClient,
    pub my_user_id: String, // Current user's ID
    pub chats: Vec<ChatInfo>,
    pub selected_chat_idx: usize,
    /// When `Some`, the chat-list cursor is on a Threads-section row instead
    /// of a regular chat. Up/down/Enter dispatch accordingly.
    pub selected_thread_idx: Option<usize>,
    pub panes: Vec<ChatPane>,
    pub focused_pane_idx: usize,
    pub pane_tree: PaneNode,
    pub input_history: Vec<String>,
    pub aliases: Aliases,
    pub focus_on_chat_list: bool,
    pub status_message: Option<String>,
    pub status_expire: Option<std::time::Instant>,
    pub last_realtime_event: Option<std::time::Instant>,
    pub unread_dirty: bool,
    pub last_unread_save: Option<std::time::Instant>,
    pub pane_areas: std::collections::HashMap<usize, Rect>,
    pub chat_list_area: Option<Rect>,
    pub chat_list_scroll_offset: usize,
    pub pending_open_chat: bool,
    pub pending_open_thread: Option<usize>,
    pending_open_chat_load: Option<
        tokio::sync::oneshot::Receiver<Result<OpenChatLoadResult, String>>,
    >,
    pub pending_refresh_chats: bool,
    pub pending_reload_panes: bool,
    pub pending_workspace_switch: Option<tokio::sync::oneshot::Receiver<Result<(SlackClient, String), String>>>,

    pub settings: AppSettings,
    pub user_name_cache: std::collections::HashMap<String, String>,
    pub needs_redraw: bool,
    pub last_terminal_size: (u16, u16),
    pub next_local_echo_id: u64,
    pub unread_mentions: std::collections::HashMap<String, u32>, // workspace_name -> count
    pub workspace_unread_cache: std::collections::HashMap<String, std::collections::HashMap<String, u32>>,

    // Threads I'm involved in
    pub threads: Vec<ThreadInfo>,
    pub threads_dirty: bool,
    // (channel_id, ts) of messages I've sent — used to detect when somebody
    // starts a thread on one of my messages.
    pub my_message_ts: std::collections::HashSet<(String, String)>,

    // Inline image preview (Kitty graphics protocol via ratatui-image)
    pub image_picker: Option<ratatui_image::picker::Picker>,
    pub image_cache: std::cell::RefCell<std::collections::HashMap<String, ImageCacheEntry>>,
    pub image_load_tx: tokio::sync::mpsc::UnboundedSender<(String, std::result::Result<image::DynamicImage, String>)>,
    pub image_load_rx: tokio::sync::mpsc::UnboundedReceiver<(String, std::result::Result<image::DynamicImage, String>)>,
}

pub enum ImageCacheEntry {
    Loading,
    Loaded(ratatui_image::protocol::StatefulProtocol),
    Failed(String),
}

struct OpenChatLoadResult {
    pane_idx: usize,
    channel_id: String,
    messages: Vec<SlackMessage>,
    name_cache: std::collections::HashMap<String, String>,
}

/// Load persisted threads for the given workspace and convert into runtime
/// `ThreadInfo` entries. Returns an empty Vec if nothing is persisted.
fn load_threads_for(config: &Config, workspace_name: &str) -> Vec<ThreadInfo> {
    let store = crate::persistence::ThreadStore::load(config).unwrap_or_default();
    store
        .workspaces
        .get(workspace_name)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|p| ThreadInfo {
            channel_id: p.channel_id,
            channel_name: p.channel_name,
            thread_ts: p.thread_ts,
            last_reply_ts: p.last_reply_ts,
            unread: p.unread,
            mentioned: p.mentioned,
            on_my_message: p.on_my_message,
            i_replied: p.i_replied,
            last_reply_user: p.last_reply_user,
        })
        .collect()
}

impl App {
    fn invalidate_pane_caches(&mut self) {
        for pane in &mut self.panes {
            pane.invalidate_cache();
        }
        self.needs_redraw = true;
    }

    pub async fn new() -> Result<Self> {
        let config = Config::load()?;
        
        // Get the active workspace
        if config.workspaces.is_empty() {
            return Err(anyhow::anyhow!("No workspaces configured"));
        }
        // Ensure active_workspace is within bounds
        let active_idx = config.active_workspace.min(config.workspaces.len() - 1);
        let workspace = &config.workspaces[active_idx];
        
        let slack = SlackClient::new(&workspace.token, &workspace.app_token).await?;
        let my_user_id = slack.get_my_user_id().await?;

        // Start event listener
        slack.start_event_listener(workspace.app_token.clone()).await?;

        let app_state = AppState::load(&config).unwrap_or_else(|_| AppState {
            settings: crate::persistence::AppSettings::default(),
            aliases: Aliases::default(),
            layout: LayoutData::default(),
            workspace_unread_counts: std::collections::HashMap::new(),
            workspace_threads: std::collections::HashMap::new(),
        });

        // Load initial chats
        let mut chats = slack.get_conversations().await.unwrap_or_else(|e| {
            eprintln!("Failed to load conversations: {e}");
            Vec::new()
        });
        if let Some(saved_unread) = app_state.workspace_unread_counts.get(&workspace.name) {
            for chat in &mut chats {
                if chat.unread == 0 {
                    if let Some(saved) = saved_unread.get(&chat.id) {
                        chat.unread = *saved;
                    }
                }
            }
        }
        chats.sort_by_key(|c| (c.section as u8, c.name.to_lowercase()));

        // Load pane tree
        let (pane_tree, required_indices) = if let Some(saved_tree) = app_state.layout.pane_tree {
            let indices = saved_tree.get_pane_indices();
            (saved_tree, indices)
        } else {
            let tree = PaneNode::new_single(0);
            let indices = tree.get_pane_indices();
            (tree, indices)
        };

        let max_required_idx = required_indices.iter().max().copied().unwrap_or(0);
        let total_panes_needed = (max_required_idx + 1)
            .max(app_state.layout.panes.len())
            .max(1);

        let mut panes: Vec<ChatPane> = Vec::new();
        for i in 0..total_panes_needed {
            if let Some(ps) = app_state.layout.panes.get(i) {
                let mut pane = ChatPane::new();
                pane.chat_id = ps.chat_id;
                pane.channel_id_str = ps.channel_id.clone();
                pane.chat_name = ps.chat_name.clone();
                pane.scroll_offset = ps.scroll_offset;
                pane.thread_ts = ps.thread_ts.clone();
                panes.push(pane);
            } else {
                panes.push(ChatPane::new());
            }
        }

        let focused_pane_idx = if app_state.layout.focused_pane < panes.len() {
            app_state.layout.focused_pane
        } else {
            0
        };

        let workspace_unread_cache = app_state.workspace_unread_counts.clone();

        let (image_load_tx, image_load_rx) = tokio::sync::mpsc::unbounded_channel();

        let initial_threads = load_threads_for(&config, &workspace.name);

        let app = Self {
            config,
            slack,
            my_user_id,
            chats,
            selected_chat_idx: 0,
            selected_thread_idx: None,
            panes,
            focused_pane_idx,
            pane_tree,
            input_history: Vec::new(),
            aliases: app_state.aliases,
            focus_on_chat_list: true,
            status_message: None,
            status_expire: None,
            last_realtime_event: None,
            unread_dirty: false,
            last_unread_save: None,
            chat_list_area: None,
            chat_list_scroll_offset: 0,
            pending_open_chat: false,
            pending_open_thread: None,
            pending_open_chat_load: None,
            pending_refresh_chats: false,
            pending_reload_panes: false,
            pending_workspace_switch: None,
            pane_areas: std::collections::HashMap::new(),
            settings: app_state.settings,
            user_name_cache: std::collections::HashMap::new(),
            needs_redraw: true,
            last_terminal_size: (0, 0),
            next_local_echo_id: 1,
            unread_mentions: std::collections::HashMap::new(),
            workspace_unread_cache,
            threads: initial_threads,
            threads_dirty: false,
            my_message_ts: std::collections::HashSet::new(),
            image_picker: None,
            image_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            image_load_tx,
            image_load_rx,
        };

        Ok(app)
    }
    
    /// Load chat history for all panes that have channels assigned
    pub async fn load_all_pane_histories(&mut self) -> Result<()> {
        // Collect panes to load (with channel_id and optionally thread_ts)
        let panes_to_load: Vec<(usize, String, Option<String>)> = self.panes
            .iter()
            .enumerate()
            .filter_map(|(idx, pane)| {
                pane.channel_id_str.as_ref().map(|id| {
                    (idx, id.clone(), pane.thread_ts.clone())
                })
            })
            .collect();

        for (pane_idx, channel_id, thread_ts) in panes_to_load {
            let result = if let Some(ref thread_ts) = thread_ts {
                // This is a thread pane - load thread replies
                self.slack.get_thread_replies(&channel_id, thread_ts, 100).await
            } else {
                // Regular channel pane - load channel history
                self.slack.get_conversation_history(&channel_id, 100).await
            };
            
            match result {
                Ok(messages) => {
                    // Collect unique user IDs and bot IDs and resolve names in batch
                    let mut name_cache: std::collections::HashMap<String, String> =
                        std::collections::HashMap::new();
                    for slack_msg in &messages {
                        if let Some(ref uid) = slack_msg.user {
                            if !name_cache.contains_key(uid) {
                                let name = self.slack.resolve_user_name(uid).await;
                                name_cache.insert(uid.clone(), name);
                            }
                        }
                        if let Some(ref bot_id) = slack_msg.bot_id {
                            if !name_cache.contains_key(bot_id) {
                                let name = self.slack.resolve_bot_name(bot_id).await;
                                name_cache.insert(bot_id.clone(), name);
                            }
                        }
                    }

                    // Add messages to pane
                    let pane = &mut self.panes[pane_idx];
                    pane.msg_data.clear();
                    pane.invalidate_cache();
                    
                    // Thread replies come in chronological order, channel history comes newest first
                    if thread_ts.is_some() {
                        for slack_msg in &messages {
                            pane.msg_data.push(slack_message_to_message_data(
                                slack_msg,
                                &self.my_user_id,
                                &name_cache,
                            ));
                        }
                    } else {
                        for slack_msg in messages.iter().rev() {
                            pane.msg_data.push(slack_message_to_message_data(
                                slack_msg,
                                &self.my_user_id,
                                &name_cache,
                            ));
                        }
                    }
                    
                    // Auto-scroll to bottom
                    pane.scroll_offset = usize::MAX;
                }
                Err(e) => {
                    eprintln!("Failed to load messages for pane {}: {}", pane_idx, e);
                }
            }
        }
        
        // Sync user name cache
        self.user_name_cache = self.slack.get_user_name_cache().await;
        
        Ok(())
    }

    pub async fn process_slack_events(&mut self) -> Result<()> {
        let updates = self.slack.get_pending_updates().await;
        if updates.is_empty() {
            return Ok(());
        }
        self.last_realtime_event = Some(std::time::Instant::now());
        self.needs_redraw = true;
        for update in updates {
            crate::events::apply_update(self, update);
        }
        crate::events::maybe_debounced_save(self);
        Ok(())
    }

    pub async fn refresh_chats(&mut self) -> Result<()> {
        // Preserve locally tracked unread counts across refresh (the Slack API's
        // unread_count field is only populated for user tokens with specific scopes
        // and would otherwise wipe our live activity markers).
        let workspace_name = self.config.workspaces
            .get(self.config.active_workspace)
            .map(|w| w.name.clone())
            .unwrap_or_default();
        let prev_unread: std::collections::HashMap<String, u32> = self
            .chats
            .iter()
            .map(|c| (c.id.clone(), c.unread))
            .collect();
        let saved_unread = self
            .workspace_unread_cache
            .get(&workspace_name)
            .cloned()
            .unwrap_or_default();

        let mut new_chats = self.slack.get_conversations().await?;
        for chat in &mut new_chats {
            if chat.unread == 0 {
                if let Some(prev) = prev_unread.get(&chat.id) {
                    chat.unread = *prev;
                } else if let Some(saved) = saved_unread.get(&chat.id) {
                    chat.unread = *saved;
                }
            }
        }
        self.chats = new_chats;
        self.chats
            .sort_by_key(|c| (c.section as u8, c.name.to_lowercase()));
        self.workspace_unread_cache.insert(
            workspace_name,
            self.chats
                .iter()
                .map(|c| (c.id.clone(), c.unread))
                .collect(),
        );
        if self.selected_chat_idx >= self.chats.len() {
            self.selected_chat_idx = self.chats.len().saturating_sub(1);
        }
        self.set_status("Chats refreshed");
        Ok(())
    }

    pub async fn reload_pane_contents(&mut self) -> Result<()> {
        self.load_all_pane_histories().await
    }

    pub async fn open_selected_chat(&mut self) -> Result<()> {
        self.ensure_valid_pane_idx();
        if self.selected_chat_idx >= self.chats.len() {
            return Ok(());
        }

        let chat = self.chats[self.selected_chat_idx].clone();
        let pane_idx = self.focused_pane_idx;
        let pane = &mut self.panes[pane_idx];

        // Use string channel ID (Slack IDs are not numeric)
        pane.chat_id = None;
        pane.channel_id_str = Some(chat.id.clone());
        pane.chat_name = chat.name.clone();
        pane.username = chat.username.clone();
        pane.thread_ts = None;
        pane.msg_data.clear();
        pane.invalidate_cache();

        // Clear unread counter when opening the chat
        if let Some(chat_info) = self.chats.get_mut(self.selected_chat_idx) {
            if chat_info.unread != 0 {
                chat_info.unread = 0;
                self.unread_dirty = true;
            }
        }
        
        // Clear mention counter for current workspace when opening any chat
        let workspace_name = self.config.workspaces
            .get(self.config.active_workspace)
            .map(|w| w.name.clone())
            .unwrap_or_default();
        self.unread_mentions.insert(workspace_name, 0);

        self.pending_open_chat_load = None;
        let slack = self.slack.clone();
        let channel_id = chat.id.clone();
        let known_names = self.user_name_cache.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let result = async {
                let messages = slack
                    .get_conversation_history(&channel_id, 100)
                    .await
                    .map_err(|e| e.to_string())?;

                let mut name_cache = known_names;
                let mut users_to_fetch = std::collections::HashSet::new();
                let mut bots_to_fetch = std::collections::HashSet::new();

                for slack_msg in &messages {
                    if let Some(ref uid) = slack_msg.user {
                        if !name_cache.contains_key(uid) {
                            users_to_fetch.insert(uid.clone());
                        }
                    }
                    if let Some(ref bot_id) = slack_msg.bot_id {
                        if !name_cache.contains_key(bot_id) {
                            bots_to_fetch.insert(bot_id.clone());
                        }
                    }
                }

                let mut fetch_tasks = Vec::new();
                for uid in users_to_fetch {
                    let slack = slack.clone();
                    fetch_tasks.push(tokio::spawn(async move {
                        (uid.clone(), slack.resolve_user_name(&uid).await)
                    }));
                }
                for bot_id in bots_to_fetch {
                    let slack = slack.clone();
                    fetch_tasks.push(tokio::spawn(async move {
                        (bot_id.clone(), slack.resolve_bot_name(&bot_id).await)
                    }));
                }

                for task in fetch_tasks {
                    if let Ok((id, name)) = task.await {
                        name_cache.insert(id, name);
                    }
                }

                Ok(OpenChatLoadResult {
                    pane_idx,
                    channel_id: channel_id.clone(),
                    messages,
                    name_cache,
                })
            }
            .await;

            let _ = tx.send(result);
        });

        self.pending_open_chat_load = Some(rx);
        self.set_status(&format!("Loading {}...", chat.name));
        self.needs_redraw = true;
        self.focus_on_chat_list = false;
        Ok(())
    }

    /// Open the thread referenced by `pending_open_thread` (set by mouse click
    /// on a thread row in the chat list). Resets the thread's unread counter.
    pub async fn open_pending_thread(&mut self) -> Result<()> {
        let idx = match self.pending_open_thread.take() {
            Some(i) => i,
            None => return Ok(()),
        };
        let info = match self.threads.get(idx).cloned() {
            Some(t) => t,
            None => return Ok(()),
        };
        // Reset visual highlight before opening.
        if let Some(t) = self.threads.get_mut(idx) {
            t.unread = 0;
            t.mentioned = false;
        }
        let parent_user = info.last_reply_user.unwrap_or_else(|| info.channel_name.clone());
        self.open_thread(&info.channel_id, &info.thread_ts, &parent_user)
            .await?;
        Ok(())
    }

    pub async fn open_thread(
        &mut self,
        channel_id_str: &str,
        thread_ts: &str,
        parent_user: &str,
    ) -> Result<()> {
        // Create new pane for thread
        let new_idx = self.panes.len();
        let mut thread_pane = ChatPane::new();
        thread_pane.channel_id_str = Some(channel_id_str.to_string());
        thread_pane.thread_ts = Some(thread_ts.to_string());
        thread_pane.chat_name = format!("Thread: {}", parent_user);
        self.panes.push(thread_pane);

        // Open the thread in a vertical split (side-by-side) on the focused pane.
        if !self.pane_tree.split_pane_with_ratio(self.focused_pane_idx, SplitDirection::Vertical, new_idx, 50) {
            self.pane_tree.split_with_ratio(SplitDirection::Vertical, new_idx, 50);
        }

        // Focus the new thread pane and put the cursor in its input box.
        self.focused_pane_idx = new_idx;
        self.focus_on_chat_list = false;

        // Load thread replies
        match self
            .slack
            .get_thread_replies(channel_id_str, thread_ts, 100)
            .await
        {
            Ok(messages) => {
                let mut name_cache: std::collections::HashMap<String, String> =
                    std::collections::HashMap::new();
                for slack_msg in &messages {
                    if let Some(ref uid) = slack_msg.user {
                        if !name_cache.contains_key(uid) {
                            let name = self.slack.resolve_user_name(uid).await;
                            name_cache.insert(uid.clone(), name);
                        }
                    }
                    if let Some(ref bot_id) = slack_msg.bot_id {
                        if !name_cache.contains_key(bot_id) {
                            let name = self.slack.resolve_bot_name(bot_id).await;
                            name_cache.insert(bot_id.clone(), name);
                        }
                    }
                }

                let pane = &mut self.panes[new_idx];
                for slack_msg in &messages {
                    pane.msg_data.push(slack_message_to_message_data_with_reply_count(
                        slack_msg,
                        &self.my_user_id,
                        &name_cache,
                        Some(0),
                    ));
                }
            }
            Err(e) => {
                self.set_status(&format!("Failed to load thread: {}", e));
            }
        }

        // Sync user name cache
        self.user_name_cache = self.slack.get_user_name_cache().await;

        // Auto-scroll to bottom
        self.panes[new_idx].scroll_offset = usize::MAX;
        self.focused_pane_idx = new_idx;
        self.focus_on_chat_list = false;
        Ok(())
    }

    /// Convert @username mentions to Slack's <@USER_ID> format
    fn convert_mentions_to_ids(&self, text: &str) -> String {
        let mut result = text.to_string();
        
        // Build a reverse lookup map: name -> user_id
        let mut name_to_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (user_id, user_name) in &self.user_name_cache {
            name_to_id.insert(user_name.to_lowercase(), user_id.clone());
        }
        
        // Find all @mentions in the text
        let mut offset = 0;
        while let Some(at_pos) = result[offset..].find('@') {
            let abs_pos = offset + at_pos;
            let after_at = &result[abs_pos + 1..];
            
            // Find the end of the mention (space, punctuation, or end of string)
            let mention_end = after_at
                .find(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
                .unwrap_or(after_at.len());
            
            if mention_end > 0 {
                let mention_name = &after_at[..mention_end];
                let mention_lower = mention_name.to_lowercase();
                
                // Look up the user ID
                if let Some(user_id) = name_to_id.get(&mention_lower) {
                    // Replace @username with <@USER_ID>
                    let replacement = format!("<@{}>", user_id);
                    result.replace_range(abs_pos..abs_pos + 1 + mention_end, &replacement);
                    offset = abs_pos + replacement.len();
                } else {
                    offset = abs_pos + 1;
                }
            } else {
                offset = abs_pos + 1;
            }
        }
        
        result
    }

    pub async fn send_message(&mut self) -> Result<()> {
        self.ensure_valid_pane_idx();
        let pane_idx = self.focused_pane_idx;
        let input = self.panes[pane_idx].input_buffer.trim().to_string();

        if input.is_empty() {
            return Ok(());
        }

        // Check if it's a command
        if input.starts_with('/') {
            let mut handler = CommandHandler::new();
            handler.handle_command(self, &input).await?;
            // After handle_command, pane_idx might be invalid if workspace was switched
            // Use focused_pane_idx which is always kept valid by ensure_valid_pane_idx
            self.ensure_valid_pane_idx();
            let idx = self.focused_pane_idx;
            self.panes[idx].input_buffer.clear();
            self.panes[idx].input_cursor = 0;
            self.panes[idx].tab_complete_state = None;
            return Ok(());
        }

        let channel_id_str = self.panes[pane_idx].channel_id_str.clone();
        let thread_ts = self.panes[pane_idx].thread_ts.clone();
        if let Some(channel_id) = channel_id_str {
            // Convert @username mentions to <@USER_ID> format
            let message_to_send = self.convert_mentions_to_ids(&input);
            
            // Local echo: Add message immediately to UI (with original text)
            let my_name = self.user_name_cache.get(&self.my_user_id)
                .cloned()
                .unwrap_or_else(|| "You".to_string());
            
            let local_echo_id = self.next_local_echo_id;
            self.next_local_echo_id += 1;
            
            let local_msg = local_echo_message_data(my_name, input.clone(), local_echo_id);
            
            self.panes[pane_idx].msg_data.push(local_msg);
            self.panes[pane_idx].invalidate_cache();
            self.panes[pane_idx].scroll_offset = usize::MAX;
            self.needs_redraw = true;
            
            // Clear input immediately
            self.input_history.push(input.clone());
            self.panes[pane_idx].input_buffer.clear();
            self.panes[pane_idx].input_cursor = 0;
            self.panes[pane_idx].tab_complete_state = None;
            
            // Send to Slack with converted mentions
            match self
                .slack
                .send_message(&channel_id, &message_to_send, thread_ts.as_deref())
                .await
            {
                Ok(_) => {
                    // Message sent successfully, the real message will come via WebSocket
                    // and replace the local echo (or we can keep it since it has .local timestamp)
                }
                Err(e) => {
                    self.set_status(&format!("Failed to send: {}", e));
                    // Optionally: remove the local echo message on error
                    // self.panes[pane_idx].msg_data.pop();
                }
            }
        }

        Ok(())
    }

    pub fn draw(&mut self, f: &mut Frame) {
        crate::ui::layout::draw(self, f);
    }

    /// Toggle a section's collapsed state. Returns true if a section was toggled.
    pub fn toggle_section(&mut self, label: &str) -> bool {
        if let Some(pos) = self
            .settings
            .collapsed_sections
            .iter()
            .position(|s| s == label)
        {
            self.settings.collapsed_sections.remove(pos);
        } else {
            self.settings.collapsed_sections.push(label.to_string());
        }
        self.needs_redraw = true;
        true
    }

    /// Toggle the section at the currently-selected row (if it is a header).
    /// Returns true if a header was toggled.
    pub fn toggle_selected_section(&mut self) -> bool {
        let rows = chat_list::build_chat_list_rows(self);
        if let Some(chat) = self.chats.get(self.selected_chat_idx) {
            let label = if chat.unread > 0 {
                "New".to_string()
            } else {
                chat.section.label().to_string()
            };
            if rows
                .iter()
                .any(|r| matches!(r, ChatListRow::Header(l) if l == &label))
            {
                return self.toggle_section(&label);
            }
        }
        false
    }


    pub fn set_status(&mut self, message: &str) {
        self.status_message = Some(message.to_string());
        self.status_expire = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
        self.needs_redraw = true;
    }

    pub fn save_state(&self) -> Result<()> {
        let current_workspace_name = self
            .config
            .workspaces
            .get(self.config.active_workspace)
            .map(|w| w.name.clone())
            .unwrap_or_default();

        // Persist threads for this workspace. Merge into existing store so
        // other workspaces' threads are preserved.
        if !current_workspace_name.is_empty() {
            let mut store = crate::persistence::ThreadStore::load(&self.config)
                .unwrap_or_default();
            let serialized: Vec<crate::persistence::PersistedThread> = self
                .threads
                .iter()
                .map(|t| crate::persistence::PersistedThread {
                    channel_id: t.channel_id.clone(),
                    channel_name: t.channel_name.clone(),
                    thread_ts: t.thread_ts.clone(),
                    last_reply_ts: t.last_reply_ts.clone(),
                    unread: t.unread,
                    mentioned: t.mentioned,
                    on_my_message: t.on_my_message,
                    i_replied: t.i_replied,
                    last_reply_user: t.last_reply_user.clone(),
                })
                .collect();
            store
                .workspaces
                .insert(current_workspace_name.clone(), serialized);
            let _ = store.save(&self.config);
        }
        let mut workspace_unread_counts = self.workspace_unread_cache.clone();
        workspace_unread_counts.insert(
            current_workspace_name,
            self.chats
                .iter()
                .map(|c| (c.id.clone(), c.unread))
                .collect(),
        );

        let state = AppState {
            settings: self.settings.clone(),
            aliases: self.aliases.clone(),
            layout: LayoutData {
                panes: self
                    .panes
                    .iter()
                    .map(|p| crate::persistence::PaneState {
                        chat_id: p.chat_id,
                        channel_id: p.channel_id_str.clone(),
                        chat_name: p.chat_name.clone(),
                        scroll_offset: p.scroll_offset,
                        filter_type: None,
                        filter_value: None,
                        thread_ts: p.thread_ts.clone(),
                    })
                    .collect(),
                focused_pane: self.focused_pane_idx,
                pane_tree: Some(self.pane_tree.clone()),
            },
            workspace_unread_counts,
            workspace_threads: std::collections::HashMap::new(),
        };

        state.save(&self.config)
    }

    // Navigation methods. These walk the visible (non-collapsed) chat rows.
    fn visible_chat_indices(&self) -> Vec<usize> {
        chat_list::build_chat_list_rows(self)
            .into_iter()
            .filter_map(|r| match r {
                ChatListRow::Chat(idx) => Some(idx),
                _ => None,
            })
            .collect()
    }

    /// Visible thread indices in the order they appear in the chat list.
    fn visible_thread_indices(&self) -> Vec<usize> {
        chat_list::build_chat_list_rows(self)
            .into_iter()
            .filter_map(|r| match r {
                ChatListRow::Thread(idx) => Some(idx),
                _ => None,
            })
            .collect()
    }

    pub fn select_next_chat(&mut self) {
        let threads = self.visible_thread_indices();
        let chats = self.visible_chat_indices();

        // Currently on a thread row?
        if let Some(cur) = self.selected_thread_idx {
            let pos = threads.iter().position(|&i| i == cur).unwrap_or(0);
            if pos + 1 < threads.len() {
                self.selected_thread_idx = Some(threads[pos + 1]);
            } else {
                // Past last thread → jump to first chat row.
                self.selected_thread_idx = None;
                if let Some(&first) = chats.first() {
                    self.selected_chat_idx = first;
                }
            }
            return;
        }

        // On a chat row.
        if chats.is_empty() {
            return;
        }
        let pos = chats
            .iter()
            .position(|&i| i == self.selected_chat_idx)
            .unwrap_or(0);
        let next = (pos + 1).min(chats.len() - 1);
        self.selected_chat_idx = chats[next];
    }

    pub fn select_previous_chat(&mut self) {
        let threads = self.visible_thread_indices();
        let chats = self.visible_chat_indices();

        if let Some(cur) = self.selected_thread_idx {
            let pos = threads.iter().position(|&i| i == cur).unwrap_or(0);
            let prev = pos.saturating_sub(1);
            self.selected_thread_idx = Some(threads[prev]);
            return;
        }

        if chats.is_empty() {
            // No chats but maybe threads — jump to last thread.
            if let Some(&last) = threads.last() {
                self.selected_thread_idx = Some(last);
            }
            return;
        }
        let pos = chats
            .iter()
            .position(|&i| i == self.selected_chat_idx)
            .unwrap_or(0);
        if pos == 0 {
            // At top of chat list → wrap up to last thread, if any.
            if let Some(&last) = threads.last() {
                self.selected_thread_idx = Some(last);
            }
            return;
        }
        self.selected_chat_idx = chats[pos - 1];
    }

    pub fn next_pane(&mut self) {
        if self.focus_on_chat_list {
            self.focus_on_chat_list = false;
        } else if self.panes.len() > 1 {
            self.focused_pane_idx = (self.focused_pane_idx + 1) % self.panes.len();
        }
        self.clear_unread_for_focused_pane();
    }
    
    fn clear_unread_for_focused_pane(&mut self) {
        let pane = self.panes.get(self.focused_pane_idx);
        let channel_id = pane.and_then(|p| p.channel_id_str.clone());
        let thread_ts = pane.and_then(|p| p.thread_ts.clone());

        if let (Some(cid), Some(tts)) = (channel_id.as_ref(), thread_ts.as_ref()) {
            if let Some(t) = self
                .threads
                .iter_mut()
                .find(|t| &t.channel_id == cid && &t.thread_ts == tts)
            {
                t.unread = 0;
                t.mentioned = false;
            }
        } else if let Some(cid) = channel_id.as_ref() {
            if let Some(chat) = self.chats.iter_mut().find(|c| &c.id == cid) {
                chat.unread = 0;
            }
        }
        
        // Clear mention counter for current workspace
        let workspace_name = self.config.workspaces
            .get(self.config.active_workspace)
            .map(|w| w.name.clone())
            .unwrap_or_default();
        self.unread_mentions.insert(workspace_name, 0);
    }

    pub fn scroll_up(&mut self) {
        self.ensure_valid_pane_idx();
        self.panes[self.focused_pane_idx].scroll_up();
    }

    pub fn scroll_down(&mut self) {
        self.ensure_valid_pane_idx();
        self.panes[self.focused_pane_idx].scroll_down();
    }

    pub fn page_up(&mut self) {
        for _ in 0..10 {
            self.panes[self.focused_pane_idx].scroll_up();
        }
    }

    pub fn page_down(&mut self) {
        for _ in 0..10 {
            self.panes[self.focused_pane_idx].scroll_down();
        }
    }

    pub fn scroll_to_top(&mut self) {
        self.panes[self.focused_pane_idx].scroll_offset = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.panes[self.focused_pane_idx].scroll_offset = usize::MAX;
    }

    // Split management
    pub fn split_vertical(&mut self) {
        let new_idx = self.panes.len();
        self.panes.push(ChatPane::new());
        // Split the focused pane, not the root
        if !self.pane_tree.split_pane(self.focused_pane_idx, SplitDirection::Vertical, new_idx) {
            // Fallback: split at root if focused pane not found
            self.pane_tree.split(SplitDirection::Vertical, new_idx);
        }
        self.focused_pane_idx = new_idx; // Focus the new pane
    }

    pub fn split_horizontal(&mut self) {
        let new_idx = self.panes.len();
        self.panes.push(ChatPane::new());
        // Split the focused pane, not the root
        if !self.pane_tree.split_pane(self.focused_pane_idx, SplitDirection::Horizontal, new_idx) {
            // Fallback: split at root if focused pane not found
            self.pane_tree.split(SplitDirection::Horizontal, new_idx);
        }
        self.focused_pane_idx = new_idx; // Focus the new pane
    }

    pub fn toggle_split_direction(&mut self) {
        self.pane_tree.toggle_direction();
    }

    pub fn close_pane(&mut self) {
        if self.panes.len() <= 1 {
            self.set_status("Cannot close the last pane");
            return;
        }
        
        let pane_idx = self.focused_pane_idx;
        
        // Get all pane indices before closing
        let all_indices = self.pane_tree.get_pane_indices();
        
        // Find a new pane to focus on (prefer next pane, or previous if we're at the end)
        let current_pos = all_indices.iter().position(|&idx| idx == pane_idx).unwrap_or(0);
        let new_focus_idx = if current_pos + 1 < all_indices.len() {
            all_indices[current_pos + 1]
        } else if current_pos > 0 {
            all_indices[current_pos - 1]
        } else {
            0
        };
        
        // Remove the pane from the tree
        self.pane_tree.close_pane(pane_idx);
        
        // Remove the pane from the array
        self.panes.remove(pane_idx);
        
        // Reindex all pane indices in the tree (shift down indices > pane_idx)
        self.pane_tree.reindex_after_removal(pane_idx);
        
        // Update focused pane index (adjust if it was after the removed pane)
        self.focused_pane_idx = if new_focus_idx > pane_idx {
            new_focus_idx - 1
        } else {
            new_focus_idx
        };
        
        // Ensure focused index is valid
        if self.focused_pane_idx >= self.panes.len() {
            self.focused_pane_idx = self.panes.len().saturating_sub(1);
        }
    }

    pub fn clear_pane(&mut self) {
        self.panes[self.focused_pane_idx].clear();
    }

    // Toggle settings
    pub fn toggle_chat_list(&mut self) {
        self.settings.show_chat_list = !self.settings.show_chat_list;
        self.needs_redraw = true;
    }

    pub fn resize_chat_list(&mut self, delta: i16) {
        if !self.settings.show_chat_list {
            return;
        }
        let current = self.settings.chat_list_width.unwrap_or_else(|| {
            self.chat_list_area.map(|a| a.width).unwrap_or(25)
        }) as i32;
        let next = (current + delta as i32).clamp(10, 200) as u16;
        self.settings.chat_list_width = Some(next);
        self.needs_redraw = true;
    }

    pub fn toggle_reactions(&mut self) {
        self.settings.show_reactions = !self.settings.show_reactions;
        self.invalidate_pane_caches();
    }

    pub fn toggle_emojis(&mut self) {
        self.settings.show_emojis = !self.settings.show_emojis;
        self.invalidate_pane_caches();
    }

    pub fn toggle_image_preview(&mut self) {
        self.settings.show_image_preview = !self.settings.show_image_preview;
        let status = if self.settings.show_image_preview {
            "Image preview: ON"
        } else {
            "Image preview: OFF"
        };
        self.set_status(status);
        self.invalidate_pane_caches();
    }

    pub fn toggle_timestamps(&mut self) {
        self.settings.show_timestamps = !self.settings.show_timestamps;
        self.invalidate_pane_caches();
    }

    pub fn toggle_compact_mode(&mut self) {
        self.settings.compact_mode = !self.settings.compact_mode;
        self.invalidate_pane_caches();
    }

    pub fn toggle_line_numbers(&mut self) {
        self.settings.show_line_numbers = !self.settings.show_line_numbers;
        self.invalidate_pane_caches();
    }

    pub fn toggle_user_colors(&mut self) {
        self.settings.show_user_colors = !self.settings.show_user_colors;
        let status = if self.settings.show_user_colors {
            "Username colors: ON"
        } else {
            "Username colors: OFF"
        };
        self.set_status(status);
        self.invalidate_pane_caches();
    }

    pub fn toggle_borders(&mut self) {
        self.settings.show_borders = !self.settings.show_borders;
        self.needs_redraw = true;
    }

    pub fn toggle_mouse_support(&mut self) {
        self.settings.mouse_support = !self.settings.mouse_support;
        let status = if self.settings.mouse_support {
            "Mouse support enabled (click to focus panes)"
        } else {
            "Mouse support disabled (use Shift+drag to select text)"
        };
        self.set_status(status);
    }

    pub fn handle_mouse_click(&mut self, x: u16, y: u16) {
        // Check if click is in chat list
        if let Some(area) = self.chat_list_area {
            if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
                self.focus_on_chat_list = true;
                // Calculate which chat was clicked (accounting for scroll offset and border)
                let border_offset = if self.settings.show_borders { 1 } else { 0 };
                let relative_y = y.saturating_sub(area.y + border_offset);
                let row_idx = relative_y as usize + self.chat_list_scroll_offset;
                let rows = chat_list::build_chat_list_rows(self);
                match rows.get(row_idx) {
                    Some(ChatListRow::Header(label)) => {
                        let label = label.clone();
                        self.toggle_section(&label);
                    }
                    Some(ChatListRow::Chat(chat_idx)) => {
                        self.selected_chat_idx = *chat_idx;
                        self.selected_thread_idx = None;
                        self.pending_open_chat = true;
                        // Single-click should open the chat and immediately
                        // hand focus back to the pane so typing works without
                        // a second click into the message area.
                        self.focus_on_chat_list = false;
                    }
                    Some(ChatListRow::Thread(thread_idx)) => {
                        self.selected_thread_idx = Some(*thread_idx);
                        self.pending_open_thread = Some(*thread_idx);
                        self.focus_on_chat_list = false;
                    }
                    None => {}
                }
                return;
            }
        }

        // Check if click is in a pane
        for (idx, area) in &self.pane_areas {
            if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
                self.focused_pane_idx = *idx;
                self.focus_on_chat_list = false;
                self.clear_unread_for_focused_pane();
                return;
            }
        }
    }

    pub fn switch_workspace(&mut self, workspace_idx: usize) {
        if workspace_idx >= self.config.workspaces.len() {
            self.set_status("Invalid workspace index");
            return;
        }

        if workspace_idx == self.config.active_workspace {
            self.set_status("Already on this workspace");
            return;
        }

        if self.pending_workspace_switch.is_some() {
            self.set_status("Workspace switch already in progress");
            return;
        }

        // Save current workspace state
        let _ = self.save_state();

        // Shutdown old WebSocket task
        let old_slack = self.slack.clone();
        tokio::spawn(async move { old_slack.shutdown().await });

        // Update active workspace
        self.config.active_workspace = workspace_idx;
        let _ = self.config.save();

        let workspace_name = self.config.workspaces[workspace_idx].name.clone();
        let workspace_token = self.config.workspaces[workspace_idx].token.clone();
        let workspace_app_token = self.config.workspaces[workspace_idx].app_token.clone();

        // Clear old chats and restore layout synchronously
        self.chats.clear();

        // Load saved layout for this workspace
        let app_state = AppState::load(&self.config).unwrap_or_else(|_| AppState {
            settings: self.settings.clone(),
            aliases: self.aliases.clone(),
            layout: LayoutData::default(),
            workspace_unread_counts: std::collections::HashMap::new(),
            workspace_threads: std::collections::HashMap::new(),
        });

        // Restore pane tree
        let (pane_tree, required_indices) = if let Some(saved_tree) = app_state.layout.pane_tree {
            let indices = saved_tree.get_pane_indices();
            (saved_tree, indices)
        } else {
            let tree = PaneNode::new_single(0);
            let indices = tree.get_pane_indices();
            (tree, indices)
        };
        self.pane_tree = pane_tree;

        // Restore panes
        let max_required_idx = required_indices.iter().max().copied().unwrap_or(0);
        let total_panes_needed = (max_required_idx + 1)
            .max(app_state.layout.panes.len())
            .max(1);

        self.panes.clear();
        for i in 0..total_panes_needed {
            if let Some(ps) = app_state.layout.panes.get(i) {
                let mut pane = ChatPane::new();
                pane.chat_id = ps.chat_id;
                pane.channel_id_str = ps.channel_id.clone();
                pane.chat_name = ps.chat_name.clone();
                pane.scroll_offset = ps.scroll_offset;
                pane.thread_ts = ps.thread_ts.clone();
                self.panes.push(pane);
            } else {
                self.panes.push(ChatPane::new());
            }
        }

        if app_state.layout.focused_pane < self.panes.len() {
            self.focused_pane_idx = app_state.layout.focused_pane;
        } else {
            self.focused_pane_idx = 0;
        }

        // Spawn async connection work in background
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = async {
                let slack = SlackClient::new(&workspace_token, &workspace_app_token)
                    .await
                    .map_err(|e| e.to_string())?;
                let my_user_id = slack.get_my_user_id().await.map_err(|e| e.to_string())?;
                slack
                    .start_event_listener(workspace_app_token)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok((slack, my_user_id))
            }
            .await;
            let _ = tx.send(result);
        });
        self.pending_workspace_switch = Some(rx);

        self.set_status(&format!("Connecting to workspace: {}...", workspace_name));
    }

    /// Called from the event loop to check if a background workspace switch completed.
    pub fn poll_workspace_switch(&mut self) -> bool {
        let rx = match self.pending_workspace_switch.as_mut() {
            Some(rx) => rx,
            None => return false,
        };

        match rx.try_recv() {
            Ok(Ok((slack, my_user_id))) => {
                self.slack = slack;
                self.my_user_id = my_user_id;
                self.pending_workspace_switch = None;
                self.pending_refresh_chats = true;
                self.pending_reload_panes = true;
                let name = self.config.workspaces[self.config.active_workspace].name.clone();
                self.set_status(&format!("Switched to workspace: {}", name));
                true
            }
            Ok(Err(e)) => {
                self.pending_workspace_switch = None;
                self.set_status(&format!("Workspace switch failed: {}", e));
                false
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => false,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.pending_workspace_switch = None;
                self.set_status("Workspace switch failed: task dropped");
                false
            }
        }
    }

    /// Drain any completed image loads and update the cache. Returns true if changed.
    pub fn poll_image_loads(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.image_load_rx.try_recv() {
                Ok((url, result)) => {
                    let mut cache = self.image_cache.borrow_mut();
                    match result {
                        Ok(dyn_img) => {
                            if let Some(picker) = self.image_picker.as_ref() {
                                let proto = picker.new_resize_protocol(dyn_img);
                                cache.insert(url, ImageCacheEntry::Loaded(proto));
                            } else {
                                cache.insert(url, ImageCacheEntry::Failed("no picker".into()));
                            }
                        }
                        Err(e) => {
                            cache.insert(url, ImageCacheEntry::Failed(e));
                        }
                    }
                    changed = true;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    /// Spawn an async task to download + decode an image and feed result back via channel.
    pub(crate) fn spawn_image_load(&self, url: String, name: String) {
        let slack = self.slack.clone();
        let tx = self.image_load_tx.clone();
        tokio::spawn(async move {
            let res: std::result::Result<image::DynamicImage, String> =
                match slack.download_file_from_url(&url, &name).await {
                    Ok(path) => match image::open(&path) {
                        Ok(img) => Ok(img),
                        Err(e) => Err(format!("decode: {e}")),
                    },
                    Err(e) => Err(format!("download: {e}")),
                };
            let _ = tx.send((url, res));
        });
    }

    /// Called from the event loop to check if an async chat-load completed.
    pub fn poll_open_chat_load(&mut self) -> bool {
        let rx = match self.pending_open_chat_load.as_mut() {
            Some(rx) => rx,
            None => return false,
        };

        match rx.try_recv() {
            Ok(Ok(load)) => {
                self.pending_open_chat_load = None;

                if load.pane_idx >= self.panes.len() {
                    return false;
                }

                let loaded_chat_name = {
                    let pane = &mut self.panes[load.pane_idx];
                    if pane.channel_id_str.as_deref() != Some(load.channel_id.as_str()) {
                        // Stale result after the user switched panes/channels again.
                        return false;
                    }

                    pane.msg_data.clear();
                    for slack_msg in load.messages.iter().rev() {
                        pane.msg_data.push(slack_message_to_message_data(
                            slack_msg,
                            &self.my_user_id,
                            &load.name_cache,
                        ));
                    }

                    pane.invalidate_cache();
                    pane.scroll_offset = usize::MAX;
                    pane.chat_name.clone()
                };
                // Backfill the Threads section from this loaded history. We
                // only see parent messages here (history doesn't return replies
                // inline), so we seed threads where either the parent is mine
                // or the parent mentions me, and reply_count > 0.
                let channel_name_for_threads = self
                    .chats
                    .iter()
                    .find(|c| c.id == load.channel_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| load.channel_id.clone());
                let mut backfilled = false;
                for slack_msg in &load.messages {
                    let parent_ts = slack_msg.ts.clone();
                    let reply_count = slack_msg.reply_count.unwrap_or(0);
                    if reply_count == 0 {
                        continue;
                    }
                    let parent_is_mine = slack_msg.user.as_deref() == Some(&self.my_user_id);
                    let parent_mentions_me =
                        message_mentions_user(&slack_msg.rendered_text(), &self.my_user_id);
                    if !(parent_is_mine || parent_mentions_me) {
                        continue;
                    }
                    if parent_is_mine {
                        self.my_message_ts
                            .insert((load.channel_id.clone(), parent_ts.clone()));
                    }
                    if let Some(t) = self.threads.iter_mut().find(|t| {
                        t.channel_id == load.channel_id && t.thread_ts == parent_ts
                    }) {
                        if parent_is_mine {
                            t.on_my_message = true;
                        }
                    } else {
                        self.threads.push(ThreadInfo {
                            channel_id: load.channel_id.clone(),
                            channel_name: channel_name_for_threads.clone(),
                            thread_ts: parent_ts.clone(),
                            last_reply_ts: parent_ts.clone(),
                            unread: 0, // history backfill: treat as already seen
                            mentioned: false,
                            on_my_message: parent_is_mine,
                            i_replied: false,
                            last_reply_user: None,
                        });
                        backfilled = true;
                    }
                }
                if backfilled {
                    self.threads_dirty = true;
                }
                self.user_name_cache = load.name_cache;
                self.needs_redraw = true;
                self.set_status(&format!("Loaded {}", loaded_chat_name));
                true
            }
            Ok(Err(e)) => {
                self.pending_open_chat_load = None;
                self.set_status(&format!("Failed to load messages: {}", e));
                self.needs_redraw = true;
                false
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => false,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.pending_open_chat_load = None;
                self.set_status("Failed to load messages: task dropped");
                self.needs_redraw = true;
                false
            }
        }
    }

    pub fn get_workspace_list(&self) -> Vec<(usize, String, bool)> {
        self.config.workspaces
            .iter()
            .enumerate()
            .map(|(idx, ws)| (idx, ws.name.clone(), idx == self.config.active_workspace))
            .collect()
    }

    pub fn show_workspace_list(&mut self) {
        let workspaces = self.get_workspace_list();
        let mut msg = String::from("Workspaces (Ctrl+1-9 to switch):\n");
        for (idx, name, is_active) in workspaces {
            let marker = if is_active { "* " } else { "  " };
            let mention_count = self.unread_mentions.get(&name).copied().unwrap_or(0);
            let mention_indicator = if mention_count > 0 {
                format!(" [{}@]", mention_count)
            } else {
                String::new()
            };
            msg.push_str(&format!("{}{}. {}{}\n", marker, idx + 1, name, mention_indicator));
        }
        self.set_status(&msg);
    }

    pub fn ensure_valid_pane_idx(&mut self) {
        if self.panes.is_empty() {
            self.panes.push(ChatPane::new());
            self.focused_pane_idx = 0;
        } else if self.focused_pane_idx >= self.panes.len() {
            self.focused_pane_idx = self.panes.len() - 1;
        }
    }
}
