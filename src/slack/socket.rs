use anyhow::Result;
use futures::{SinkExt, StreamExt};
use reqwest::Client as HttpClient;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use super::blocks::{attachments_to_cards, render_blocks, text_mentions_user};
use super::types::{
    CachedUserInfo, SlackAttachment, SlackBlock, SlackFile, SlackUpdate, SocketModeConnectResponse,
};
use super::SlackClient;

impl SlackClient {
    pub async fn start_event_listener(&self, app_token: String) -> Result<()> {
        tracing::debug!("start_event_listener called");
        let pending_updates = self.pending_updates.clone();
        let http = self.http.clone();
        let token = self.token.clone();
        let user_id = self.user_id.clone();
        let user_name_cache = self.user_name_cache.clone();
        let user_info_cache = self.user_info_cache.clone();
        let app_token = app_token.clone();

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
        *self.ws_shutdown.lock().await = Some(shutdown_tx);

        let handle = tokio::spawn(async move {
            tracing::debug!("WebSocket task starting...");

            let mut backoff_secs: u64 = 1;

            'reconnect: loop {
                // Bail out early if shutdown was requested between reconnects
                if shutdown_rx.try_recv().is_ok() {
                    tracing::debug!("Shutdown requested before reconnect, exiting");
                    break 'reconnect;
                }

                // Fetch a fresh socket URL on every (re)connect; URLs are single-use
                let url = match http
                    .post("https://slack.com/api/apps.connections.open")
                    .bearer_auth(&app_token)
                    .send()
                    .await
                {
                    Ok(resp) => match resp.json::<SocketModeConnectResponse>().await {
                        Ok(r) if r.ok => r.url,
                        Ok(_) => {
                            tracing::debug!("apps.connections.open returned ok=false");
                            tokio::select! {
                                _ = tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)) => {}
                                _ = shutdown_rx.recv() => break 'reconnect,
                            }
                            backoff_secs = (backoff_secs * 2).min(60);
                            continue;
                        }
                        Err(e) => {
                            tracing::debug!("Failed to parse connect response: {}", e);
                            tokio::select! {
                                _ = tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)) => {}
                                _ = shutdown_rx.recv() => break 'reconnect,
                            }
                            backoff_secs = (backoff_secs * 2).min(60);
                            continue;
                        }
                    },
                    Err(e) => {
                        tracing::debug!("apps.connections.open request failed: {}", e);
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)) => {}
                            _ = shutdown_rx.recv() => break 'reconnect,
                        }
                        backoff_secs = (backoff_secs * 2).min(60);
                        continue;
                    }
                };

                let mut ws_stream = match connect_async(&url).await {
                    Ok((s, _)) => {
                        tracing::debug!("WebSocket connected successfully");
                        backoff_secs = 1;
                        s
                    }
                    Err(e) => {
                        tracing::debug!("Failed to connect WebSocket: {}", e);
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)) => {}
                            _ = shutdown_rx.recv() => break 'reconnect,
                        }
                        backoff_secs = (backoff_secs * 2).min(60);
                        continue;
                    }
                };

                loop {
                    tokio::select! {
                        next = ws_stream.next() => {
                            match next {
                                Some(Ok(Message::Text(text))) => {
                                    tracing::debug!("Received WebSocket message: {}", &text[..text.len().min(200)]);
                                    if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&text) {
                                        let env_type = envelope.get("type").and_then(|v| v.as_str()).unwrap_or("");

                                        // Slack periodically tells the client to reconnect (refresh / warning)
                                        if env_type == "disconnect" {
                                            tracing::debug!("Received disconnect from Slack, reconnecting");
                                            let _ = ws_stream.close(None).await;
                                            break;
                                        }

                                        // Acknowledge envelope
                                        if let Some(envelope_id) =
                                            envelope.get("envelope_id").and_then(|v| v.as_str())
                                        {
                                            let ack = serde_json::json!({
                                                "envelope_id": envelope_id
                                            });
                                            if let Err(e) = ws_stream.send(Message::Text(ack.to_string())).await {
                                                tracing::debug!("Failed to acknowledge envelope {}, reconnecting: {}", envelope_id, e
                                                );
                                                break;
                                            }
                                            tracing::debug!("Acknowledged envelope: {}", envelope_id);
                                        }

                                        tracing::debug!("Event type: {}", env_type);
                                        if env_type == "events_api" {
                                            if let Some(event) =
                                                envelope.get("payload").and_then(|p| p.get("event"))
                                            {
                                                let event_owned = event.clone();
                                                let pending_updates = pending_updates.clone();
                                                let http = http.clone();
                                                let token = token.clone();
                                                let user_id = user_id.clone();
                                                let user_name_cache = user_name_cache.clone();
                                                let user_info_cache = user_info_cache.clone();

                                                tokio::spawn(async move {
                                                    tracing::debug!("Processing event: {:?}", event_owned);
                                                    Self::process_event(
                                                        &event_owned,
                                                        &pending_updates,
                                                        &http,
                                                        &token,
                                                        &user_id,
                                                        &user_name_cache,
                                                        &user_info_cache,
                                                    )
                                                    .await;
                                                    tracing::debug!("Event processed, added to pending_updates");
                                                });
                                            }
                                        }
                                    }
                                }
                                Some(Ok(Message::Ping(p))) => {
                                    let _ = ws_stream.send(Message::Pong(p)).await;
                                }
                                Some(Ok(Message::Close(frame))) => {
                                    tracing::debug!("WebSocket closed by server: {:?}, reconnecting", frame);
                                    break;
                                }
                                Some(Ok(_)) => {
                                    // Pong/Binary/Frame: ignore
                                }
                                Some(Err(e)) => {
                                    tracing::debug!("WebSocket error: {}, reconnecting", e);
                                    break;
                                }
                                None => {
                                    tracing::debug!("WebSocket stream ended, reconnecting");
                                    break;
                                }
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            tracing::debug!("Received shutdown signal, closing WebSocket gracefully");
                            let _ = ws_stream.close(None).await;
                            tracing::debug!("WebSocket closed");
                            break 'reconnect;
                        }
                    }
                }
            }
            tracing::debug!("WebSocket task exiting");
        });

        *self.ws_handle.lock().await = Some(handle);
        Ok(())
    }

    async fn process_event(
        event: &serde_json::Value,
        pending_updates: &Arc<Mutex<Vec<SlackUpdate>>>,
        http: &HttpClient,
        token: &str,
        user_id: &Arc<Mutex<Option<String>>>,
        user_name_cache: &Arc<Mutex<std::collections::HashMap<String, String>>>,
        user_info_cache: &Arc<Mutex<std::collections::HashMap<String, CachedUserInfo>>>,
    ) {
        if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
            match event_type {
                "message" => {
                    // Check for message subtypes (edited, deleted)
                    let subtype = event.get("subtype").and_then(|v| v.as_str());
                    
                    match subtype {
                        Some("message_changed") => {
                            // Message was edited
                            if let (Some(channel_id), Some(message)) = (
                                event.get("channel").and_then(|v| v.as_str()),
                                event.get("message"),
                            ) {
                                if let (Some(ts), Some(new_text)) = (
                                    message.get("ts").and_then(|v| v.as_str()),
                                    message.get("text").and_then(|v| v.as_str()),
                                ) {
                                    pending_updates.lock().await.push(SlackUpdate::MessageChanged {
                                        channel_id: channel_id.to_string(),
                                        ts: ts.to_string(),
                                        new_text: new_text.to_string(),
                                    });
                                }
                            }
                            return;
                        }
                        Some("message_deleted") => {
                            // Message was deleted
                            if let (Some(channel_id), Some(deleted_ts)) = (
                                event.get("channel").and_then(|v| v.as_str()),
                                event.get("deleted_ts").and_then(|v| v.as_str()),
                            ) {
                                pending_updates.lock().await.push(SlackUpdate::MessageDeleted {
                                    channel_id: channel_id.to_string(),
                                    ts: deleted_ts.to_string(),
                                });
                            }
                            return;
                        }
                        _ => {}
                    }
                    
                    // Regular new message
                    if let (Some(channel_id), Some(ts)) = (
                        event.get("channel").and_then(|v| v.as_str()),
                        event.get("ts").and_then(|v| v.as_str()),
                    ) {
                        let raw_text = event
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let user_id_event = event
                            .get("user")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let is_bot = event.get("bot_id").is_some();
                        let thread_ts = event
                            .get("thread_ts")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let attachments: Vec<SlackAttachment> = event
                            .get("attachments")
                            .and_then(|a| serde_json::from_value(a.clone()).ok())
                            .unwrap_or_default();
                        let blocks: Vec<SlackBlock> = event
                            .get("blocks")
                            .and_then(|b| serde_json::from_value(b.clone()).ok())
                            .unwrap_or_default();
                        // Prefer rendered blocks over raw text (matches Slack client behavior).
                        let from_blocks = render_blocks(&blocks);
                        let text_owned = if !from_blocks.is_empty() {
                            from_blocks
                        } else if !raw_text.is_empty() {
                            raw_text.to_string()
                        } else {
                            attachments
                                .iter()
                                .find_map(|a| {
                                    let r = render_blocks(&a.blocks);
                                    if !r.is_empty() {
                                        Some(r)
                                    } else {
                                        a.text
                                            .clone()
                                            .or_else(|| a.pretext.clone())
                                            .or_else(|| a.fallback.clone())
                                            .filter(|t| !t.is_empty())
                                    }
                                })
                                .unwrap_or_default()
                        };
                        let text = text_owned.as_str();
                        if text.is_empty() && attachments.is_empty() && blocks.is_empty() {
                            return;
                        }
                        let cards = attachments_to_cards(&attachments);
                        let inline_image_urls: Vec<(String, String)> = attachments
                            .iter()
                            .filter_map(|a| {
                                a.image_url.as_ref().filter(|s| !s.is_empty()).map(|u| {
                                    let name = u
                                        .rsplit('/')
                                        .next()
                                        .and_then(|s| s.split('?').next())
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or("image")
                                        .to_string();
                                    (u.clone(), name)
                                })
                            })
                            .collect();

                        let my_id = user_id.lock().await.clone().unwrap_or_default();
                        let is_self = !my_id.is_empty() && user_id_event == my_id;
                        
                        // Check if the message mentions the current user
                        let mentions_me = !my_id.is_empty() && text_mentions_user(text, &my_id);

                        // Fetch user name - prioritize user field first (real users), then bot_profile, username, bot_id
                        let user_name = if event.get("user").is_some() && user_id_event != "unknown" {
                            // Regular user - prefer cache to avoid HTTP on every event
                            if let Some(info) = user_info_cache.lock().await.get(user_id_event).cloned() {
                                info.name
                            } else {
                                if let Ok(user_info) = Self::fetch_user_info(http, token, user_id_event).await {
                                    user_name_cache
                                        .lock()
                                        .await
                                        .insert(user_id_event.to_string(), user_info.clone());
                                    user_info_cache.lock().await.insert(
                                        user_id_event.to_string(),
                                        CachedUserInfo {
                                            name: user_info.clone(),
                                            is_bot: false,
                                            deleted: false,
                                        },
                                    );
                                    user_info
                                } else {
                                    user_id_event.to_string()
                                }
                            }
                        } else if let Some(bot_profile) = event.get("bot_profile") {
                            // Slack app/webhook with bot_profile (only if no user field)
                            let name = bot_profile
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("Bot")
                                .to_string();
                            name
                        } else if let Some(username) = event.get("username").and_then(|u| u.as_str()) {
                            // Bot with username field
                            username.to_string()
                        } else if let Some(bot_id) = event.get("bot_id").and_then(|b| b.as_str()) {
                            // Bot message - fetch bot info
                            let client = SlackClient {
                                http: http.clone(),
                                token: token.to_string(),
                                user_id: user_id.clone(),
                                pending_updates: pending_updates.clone(),
                                ws_handle: Arc::new(Mutex::new(None)),
                                ws_shutdown: Arc::new(Mutex::new(None)),
                                user_name_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
                                user_info_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
                            };
                            client.resolve_bot_name(bot_id).await
                        } else {
                            user_id_event.to_string()
                        };

                        // Extract files from event
                        let files: Vec<SlackFile> = event
                            .get("files")
                            .and_then(|f| serde_json::from_value(f.clone()).ok())
                            .unwrap_or_default();

                        pending_updates.lock().await.push(SlackUpdate::NewMessage {
                            channel_id: channel_id.to_string(),
                            user_name,
                            text: text.to_string(),
                            ts: ts.to_string(),
                            thread_ts,
                            is_bot,
                            is_self,
                            cards,
                            inline_image_urls,
                            mentions_me,
                            files,
                        });
                    }
                }
                "user_typing" => {
                    if let (Some(channel_id), Some(user_id)) = (
                        event.get("channel").and_then(|v| v.as_str()),
                        event.get("user").and_then(|v| v.as_str()),
                    ) {
                        let user_name = if let Ok(user_info) =
                            Self::fetch_user_info(http, token, user_id).await
                        {
                            user_info
                        } else {
                            user_id.to_string()
                        };

                        pending_updates.lock().await.push(SlackUpdate::UserTyping {
                            channel_id: channel_id.to_string(),
                            user_name,
                        });
                    }
                }
                "reaction_added" | "reaction_removed" => {
                    if let (Some(item), Some(reaction)) = (
                        event.get("item"),
                        event.get("reaction").and_then(|v| v.as_str()),
                    ) {
                        if item.get("type").and_then(|v| v.as_str()) == Some("message") {
                            if let (Some(channel_id), Some(message_ts)) = (
                                item.get("channel").and_then(|v| v.as_str()),
                                item.get("ts").and_then(|v| v.as_str()),
                            ) {
                                let update = if event_type == "reaction_added" {
                                    SlackUpdate::ReactionAdded {
                                        channel_id: channel_id.to_string(),
                                        message_ts: message_ts.to_string(),
                                        reaction: reaction.to_string(),
                                    }
                                } else {
                                    SlackUpdate::ReactionRemoved {
                                        channel_id: channel_id.to_string(),
                                        message_ts: message_ts.to_string(),
                                        reaction: reaction.to_string(),
                                    }
                                };
                                pending_updates.lock().await.push(update);
                            }
                        }
                    }
                }
                "member_joined_channel" => {
                    if let (Some(channel_id), Some(user_id)) = (
                        event.get("channel").and_then(|v| v.as_str()),
                        event.get("user").and_then(|v| v.as_str()),
                    ) {
                        pending_updates.lock().await.push(SlackUpdate::MemberJoinedChannel {
                            channel_id: channel_id.to_string(),
                            user_id: user_id.to_string(),
                        });
                    }
                }
                "member_left_channel" => {
                    if let (Some(channel_id), Some(user_id)) = (
                        event.get("channel").and_then(|v| v.as_str()),
                        event.get("user").and_then(|v| v.as_str()),
                    ) {
                        pending_updates.lock().await.push(SlackUpdate::MemberLeftChannel {
                            channel_id: channel_id.to_string(),
                            user_id: user_id.to_string(),
                        });
                    }
                }
                "channel_rename" => {
                    if let Some(channel) = event.get("channel") {
                        if let (Some(channel_id), Some(name)) = (
                            channel.get("id").and_then(|v| v.as_str()),
                            channel.get("name").and_then(|v| v.as_str()),
                        ) {
                            pending_updates.lock().await.push(SlackUpdate::ChannelRenamed {
                                channel_id: channel_id.to_string(),
                                name: name.to_string(),
                            });
                        }
                    }
                }
                "channel_archive" | "channel_deleted" => {
                    if let Some(channel_id) = event.get("channel").and_then(|v| v.as_str()) {
                        pending_updates.lock().await.push(SlackUpdate::ChannelLifecycle {
                            channel_id: channel_id.to_string(),
                            archived: true,
                        });
                    }
                }
                "channel_unarchive" => {
                    if let Some(channel_id) = event.get("channel").and_then(|v| v.as_str()) {
                        pending_updates.lock().await.push(SlackUpdate::ChannelLifecycle {
                            channel_id: channel_id.to_string(),
                            archived: false,
                        });
                    }
                }
                "channel_created"
                | "channel_id_changed"
                | "team_join"
                | "shared_channel_invite_accepted"
                | "shared_channel_invite_approved"
                | "shared_channel_invite_declined"
                | "shared_channel_invite_received" => {
                    pending_updates.lock().await.push(SlackUpdate::RefreshChatList);
                }
                "user_change" => {
                    if let Some(user_id) = event.get("user").and_then(|v| v.as_str()) {
                        pending_updates.lock().await.push(SlackUpdate::UserProfileChanged {
                            user_id: user_id.to_string(),
                        });
                    }
                }
                "user_profile_changed" => {
                    if let Some(user_id) = event.get("user").and_then(|v| v.get("id")).and_then(|v| v.as_str()) {
                        pending_updates.lock().await.push(SlackUpdate::UserProfileChanged {
                            user_id: user_id.to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
}
