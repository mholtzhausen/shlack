use anyhow::{anyhow, Result};
use futures::StreamExt;
use reqwest::Client as HttpClient;

use crate::models::{ChatInfo, ChatSection};

use super::types::{
    CachedUserInfo, Channel, ConversationHistoryResponse, ConversationMembersResponse,
    ConversationsListResponse, SlackMessage, User, UserInfoResponse,
};
use super::SlackClient;

impl SlackClient {
    pub async fn resolve_user_name(&self, user_id: &str) -> String {
        // Check cache first
        {
            let cache = self.user_name_cache.lock().await;
            if let Some(name) = cache.get(user_id) {
                return name.clone();
            }
        }

        self.fetch_user_info_cached(user_id)
            .await
            .map(|info| info.name)
            .unwrap_or_else(|| user_id.to_string())
    }

    /// Get a snapshot of the user name cache for synchronous lookups.
    pub async fn get_user_name_cache(&self) -> std::collections::HashMap<String, String> {
        self.user_name_cache.lock().await.clone()
    }

    fn display_name_for_user(user: &User) -> String {
        user.profile
            .as_ref()
            .and_then(|p| p.display_name.as_ref())
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| user.name.clone())
    }

    pub(crate) async fn fetch_user_info(http: &HttpClient, token: &str, user_id: &str) -> Result<String> {
        let response: UserInfoResponse = http
            .get(&format!(
                "https://slack.com/api/users.info?user={}",
                user_id
            ))
            .bearer_auth(token)
            .send()
            .await?
            .json()
            .await?;

        if response.ok {
            Ok(Self::display_name_for_user(&response.user))
        } else {
            Ok(user_id.to_string())
        }
    }

    async fn fetch_user_info_cached(&self, user_id: &str) -> Option<CachedUserInfo> {
        {
            let cache = self.user_info_cache.lock().await;
            if let Some(info) = cache.get(user_id) {
                return Some(info.clone());
            }
        }

        let response: UserInfoResponse = self
            .http
            .get(&format!(
                "https://slack.com/api/users.info?user={}",
                user_id
            ))
            .bearer_auth(&self.token)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;

        if !response.ok {
            return None;
        }

        let info = CachedUserInfo {
            name: Self::display_name_for_user(&response.user),
            is_bot: response.user.is_bot,
            deleted: response.user.deleted,
        };

        self.user_info_cache
            .lock()
            .await
            .insert(user_id.to_string(), info.clone());
        self.user_name_cache
            .lock()
            .await
            .insert(user_id.to_string(), info.name.clone());
        Some(info)
    }

    async fn prefetch_user_infos(&self, user_ids: Vec<String>) {
        let user_ids: std::collections::HashSet<String> = user_ids.into_iter().collect();
        let missing: Vec<String> = {
            let cache = self.user_info_cache.lock().await;
            user_ids
                .into_iter()
                .filter(|user_id| !cache.contains_key(user_id))
                .collect()
        };

        futures::stream::iter(missing)
            .for_each_concurrent(16, |user_id| {
                let slack = self.clone();
                async move {
                    let _ = slack.fetch_user_info_cached(&user_id).await;
                }
            })
            .await;
    }

    pub async fn resolve_bot_name(&self, bot_id: &str) -> String {
        // Check cache first
        {
            let cache = self.user_name_cache.lock().await;
            if let Some(name) = cache.get(bot_id) {
                return name.clone();
            }
        }
        
        // Fetch bot info
        let resp = self
            .http
            .get(&format!(
                "https://slack.com/api/bots.info?bot={}",
                bot_id
            ))
            .bearer_auth(&self.token)
            .send()
            .await;

        if let Ok(resp) = resp {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if json.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                    if let Some(name) = json.get("bot")
                        .and_then(|b| b.get("name"))
                        .and_then(|n| n.as_str()) {
                        let name_str = name.to_string();
                        // Cache it
                        self.user_name_cache
                            .lock()
                            .await
                            .insert(bot_id.to_string(), name_str.clone());
                        return name_str;
                    }
                }
            }
        }
        
        bot_id.to_string()
    }

    pub async fn get_conversation_members(&self, channel_id: &str) -> Result<Vec<String>> {
        let response: ConversationMembersResponse = self
            .http
            .get(&format!(
                "https://slack.com/api/conversations.members?channel={}&limit=100",
                channel_id
            ))
            .bearer_auth(&self.token)
            .send()
            .await?
            .json()
            .await?;

        if !response.ok {
            return Err(anyhow!("Failed to fetch conversation members"));
        }

        Ok(response.members)
    }

    pub async fn get_conversations(&self) -> Result<Vec<ChatInfo>> {
        // Use users.conversations which returns everything the current user has
        // access to (public, private, shared, mpim, im) across paginated results.
        let mut all_channels: Vec<Channel> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut url = String::from(
                "https://slack.com/api/users.conversations?types=public_channel,private_channel,mpim,im&limit=200&exclude_archived=true",
            );
            if let Some(ref c) = cursor {
                url.push_str(&format!("&cursor={}", c));
            }

            let response: ConversationsListResponse = self
                .http
                .get(&url)
                .bearer_auth(&self.token)
                .send()
                .await?
                .json()
                .await?;

            if !response.ok {
                return Err(anyhow!("Failed to fetch conversations"));
            }

            all_channels.extend(response.channels);

            match response.response_metadata.and_then(|m| {
                if m.next_cursor.trim().is_empty() {
                    None
                } else {
                    Some(m.next_cursor)
                }
            }) {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }

        let my_user_id = self.get_my_user_id().await.unwrap_or_default();
        let dm_user_ids = all_channels
            .iter()
            .filter(|ch| ch.is_im)
            .filter_map(|ch| ch.user.clone())
            .collect();
        self.prefetch_user_infos(dm_user_ids).await;

        let mut chats = Vec::new();
        for ch in all_channels {
            if ch.is_archived {
                continue;
            }

            let dm_user_info = if ch.is_im {
                if let Some(ref uid) = ch.user {
                    self.fetch_user_info_cached(uid).await
                } else {
                    None
                }
            } else {
                None
            };
            
            // Skip DMs with deleted users
            if dm_user_info
                .as_ref()
                .map(|info| info.deleted)
                .unwrap_or(false)
            {
                continue;
            }

            // Determine section
            let section = if ch.is_mpim {
                ChatSection::Group
            } else if ch.is_im {
                if dm_user_info
                    .as_ref()
                    .map(|info| info.is_bot)
                    .unwrap_or(false)
                {
                    ChatSection::Bot
                } else {
                    ChatSection::DirectMessage
                }
            } else if ch.is_ext_shared || ch.is_shared || ch.is_org_shared {
                ChatSection::Shared
            } else if ch.is_private || ch.is_group {
                ChatSection::Private
            } else {
                ChatSection::Public
            };

            let name = match section {
                ChatSection::Group => {
                    // Fetch members and build "Name1, Name2" excluding self
                    match self.get_conversation_members(&ch.id).await {
                        Ok(members) => {
                            let members: Vec<String> = members
                                .into_iter()
                                .filter(|mid| mid != &my_user_id)
                                .collect();
                            self.prefetch_user_infos(members.clone()).await;

                            let mut names = Vec::new();
                            for mid in &members {
                                let n = self.resolve_user_name(mid).await;
                                // Use first name only
                                let first =
                                    n.split_whitespace().next().unwrap_or(&n).to_string();
                                names.push(first);
                            }
                            if names.is_empty() {
                                ch.name.unwrap_or_else(|| ch.id.clone())
                            } else {
                                names.join(", ")
                            }
                        }
                        Err(_) => ch.name.unwrap_or_else(|| ch.id.clone()),
                    }
                }
                ChatSection::DirectMessage | ChatSection::Bot => {
                    if let Some(info) = dm_user_info {
                        info.name
                    } else if let Some(ref user_id) = ch.user {
                        self.resolve_user_name(user_id).await
                    } else {
                        ch.name.unwrap_or_else(|| ch.id.clone())
                    }
                }
                _ => ch.name.unwrap_or_else(|| ch.id.clone()),
            };

            chats.push(ChatInfo {
                id: ch.id.clone(),
                name,
                username: ch.user.or(Some(ch.id)),
                unread: ch.unread_count.unwrap_or(0),
                section,
            });
        }

        Ok(chats)
    }

    pub async fn get_conversation_history(
        &self,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<SlackMessage>> {
        let mut all_messages: Vec<SlackMessage> = Vec::new();
        let mut cursor: Option<String> = None;
        let page_limit = limit.min(200).max(1);

        loop {
            let mut url = format!(
                "https://slack.com/api/conversations.history?channel={}&limit={}",
                channel_id, page_limit
            );
            if let Some(ref c) = cursor {
                url.push_str(&format!("&cursor={}", c));
            }

            let response: ConversationHistoryResponse = self
                .http
                .get(&url)
                .bearer_auth(&self.token)
                .send()
                .await?
                .json()
                .await?;

            if !response.ok {
                return Err(anyhow!("Failed to fetch conversation history"));
            }

            all_messages.extend(response.messages);
            if all_messages.len() >= limit {
                all_messages.truncate(limit);
                break;
            }

            let next_cursor = response
                .response_metadata
                .and_then(|m| {
                    if m.next_cursor.trim().is_empty() {
                        None
                    } else {
                        Some(m.next_cursor)
                    }
                });

            match next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }

        Ok(all_messages)
    }

    pub async fn get_thread_replies(
        &self,
        channel_id: &str,
        thread_ts: &str,
        limit: usize,
    ) -> Result<Vec<SlackMessage>> {
        let mut all_messages: Vec<SlackMessage> = Vec::new();
        let mut cursor: Option<String> = None;
        let page_limit = limit.min(200).max(1);

        loop {
            let mut url = format!(
                "https://slack.com/api/conversations.replies?channel={}&ts={}&limit={}",
                channel_id, thread_ts, page_limit
            );
            if let Some(ref c) = cursor {
                url.push_str(&format!("&cursor={}", c));
            }

            let response: ConversationHistoryResponse = self
                .http
                .get(&url)
                .bearer_auth(&self.token)
                .send()
                .await?
                .json()
                .await?;

            if !response.ok {
                return Err(anyhow!("Failed to fetch thread replies"));
            }

            all_messages.extend(response.messages);
            if all_messages.len() >= limit {
                all_messages.truncate(limit);
                break;
            }

            let next_cursor = response
                .response_metadata
                .and_then(|m| {
                    if m.next_cursor.trim().is_empty() {
                        None
                    } else {
                        Some(m.next_cursor)
                    }
                });

            match next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }

        Ok(all_messages)
    }

    pub async fn send_message(
        &self,
        channel_id: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<()> {
        let mut payload = serde_json::json!({
            "channel": channel_id,
            "text": text,
        });
        if let Some(ts) = thread_ts {
            payload["thread_ts"] = serde_json::Value::String(ts.to_string());
        }

        let response: serde_json::Value = self
            .http
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await?
            .json()
            .await?;

        if !response
            .get("ok")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Err(anyhow!("Failed to send message"));
        }

        Ok(())
    }

    pub async fn add_reaction(&self, channel_id: &str, timestamp: &str, emoji: &str) -> Result<()> {
        let payload = serde_json::json!({
            "channel": channel_id,
            "timestamp": timestamp,
            "name": emoji,
        });

        let response: serde_json::Value = self
            .http
            .post("https://slack.com/api/reactions.add")
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await?
            .json()
            .await?;

        if !response
            .get("ok")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Err(anyhow!("Failed to add reaction"));
        }

        Ok(())
    }

    pub async fn leave_conversation(&self, channel_id: &str) -> Result<()> {
        let payload = serde_json::json!({
            "channel": channel_id,
        });

        let response: serde_json::Value = self
            .http
            .post("https://slack.com/api/conversations.leave")
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await?
            .json()
            .await?;

        if !response
            .get("ok")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Err(anyhow!("Failed to leave conversation"));
        }

        Ok(())
    }
}
