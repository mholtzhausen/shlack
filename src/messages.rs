use std::collections::HashMap;

use crate::slack::{SlackAttachment, SlackFile, SlackMessage};
use crate::widgets::{AttachmentCard, MessageData};

/// Check if message text contains a mention of the specified user ID.
pub fn message_mentions_user(text: &str, user_id: &str) -> bool {
    if user_id.is_empty() {
        return false;
    }
    let pattern1 = format!("<@{user_id}>");
    let pattern2 = format!("<@{user_id}|");
    text.contains(&pattern1) || text.contains(&pattern2)
}

/// Returns (urls, file_names) for each `attachment.image_url` found.
fn extract_attachment_images(attachments: &[SlackAttachment]) -> (Vec<String>, Vec<String>) {
    let mut urls = Vec::new();
    let mut names = Vec::new();
    for att in attachments {
        if let Some(img) = att.image_url.as_ref().filter(|s| !s.is_empty()) {
            let name = img
                .rsplit('/')
                .next()
                .and_then(|s| s.split('?').next())
                .filter(|s| !s.is_empty())
                .unwrap_or("image")
                .to_string();
            urls.push(img.clone());
            names.push(name);
        }
    }
    (urls, names)
}

/// Combined media detection: file uploads + attachment image_url (Giphy etc).
pub fn detect_message_media(
    files: &[SlackFile],
    attachments: &[SlackAttachment],
) -> Option<(String, Vec<String>, Vec<String>, Vec<String>)> {
    let from_files = detect_media_type(files);
    let (att_urls, att_names) = extract_attachment_images(attachments);
    if att_urls.is_empty() {
        return from_files;
    }
    match from_files {
        Some((mt, ids, mut urls, mut names)) => {
            urls.extend(att_urls);
            names.extend(att_names);
            Some((mt, ids, urls, names))
        }
        None => Some(("image".to_string(), Vec::new(), att_urls, att_names)),
    }
}

pub fn detect_media_type(
    files: &[SlackFile],
) -> Option<(String, Vec<String>, Vec<String>, Vec<String>)> {
    if files.is_empty() {
        return None;
    }

    let mut has_image = false;
    let mut has_video = false;
    let mut file_ids = Vec::new();
    let mut file_urls = Vec::new();
    let mut file_names = Vec::new();

    for file in files {
        if let Some(ref id) = file.id {
            file_ids.push(id.clone());
        }

        let url = file
            .url_private_download
            .as_ref()
            .or_else(|| file.url_private.as_ref())
            .cloned();
        if let Some(url) = url {
            file_urls.push(url);
        }

        if let Some(ref name) = file.name {
            file_names.push(name.clone());
        } else {
            file_names.push("file".to_string());
        }

        if let Some(ref mimetype) = file.mimetype {
            if mimetype.starts_with("image/") {
                has_image = true;
            } else if mimetype.starts_with("video/") {
                has_video = true;
            }
        } else if let Some(ref filetype) = file.filetype {
            if filetype == "jpg"
                || filetype == "jpeg"
                || filetype == "png"
                || filetype == "gif"
                || filetype == "webp"
                || filetype == "svg"
            {
                has_image = true;
            } else if filetype == "mp4" || filetype == "mov" || filetype == "webm" {
                has_video = true;
            }
        }
    }

    if has_video {
        Some(("video".to_string(), file_ids, file_urls, file_names))
    } else if has_image {
        Some(("image".to_string(), file_ids, file_urls, file_names))
    } else {
        None
    }
}

fn media_fields_from_files_and_inline(
    files: &[SlackFile],
    inline_image_urls: &[(String, String)],
) -> (Option<String>, Vec<String>, Vec<String>, Vec<String>) {
    let mut detected = detect_media_type(files)
        .map(|(mt, ids, urls, names)| (Some(mt), ids, urls, names))
        .unwrap_or((None, Vec::new(), Vec::new(), Vec::new()));
    if !inline_image_urls.is_empty() {
        if detected.0.is_none() {
            detected.0 = Some("image".to_string());
        }
        for (u, n) in inline_image_urls {
            detected.2.push(u.clone());
            detected.3.push(n.clone());
        }
    }
    detected
}

pub fn resolve_sender_name(msg: &SlackMessage, name_cache: &HashMap<String, String>) -> String {
    if let Some(ref user_id) = msg.user {
        return name_cache
            .get(user_id)
            .cloned()
            .unwrap_or_else(|| user_id.clone());
    }
    if let Some(ref bot_profile) = msg.bot_profile {
        return bot_profile
            .name
            .clone()
            .unwrap_or_else(|| "Bot".to_string());
    }
    if let Some(ref username) = msg.username {
        return username.clone();
    }
    if let Some(ref bot_id) = msg.bot_id {
        return name_cache
            .get(bot_id)
            .cloned()
            .unwrap_or_else(|| bot_id.clone());
    }
    "Unknown".to_string()
}

pub fn slack_message_to_message_data(
    msg: &SlackMessage,
    my_user_id: &str,
    name_cache: &HashMap<String, String>,
) -> MessageData {
    slack_message_to_message_data_with_reply_count(msg, my_user_id, name_cache, None)
}

/// Like [`slack_message_to_message_data`] but allows overriding reply count (thread panes use 0).
pub fn slack_message_to_message_data_with_reply_count(
    msg: &SlackMessage,
    my_user_id: &str,
    name_cache: &HashMap<String, String>,
    reply_count_override: Option<u32>,
) -> MessageData {
    let text = msg.rendered_text();
    let sender_name = resolve_sender_name(msg, name_cache);
    let reactions: Vec<(String, u32)> = msg
        .reactions
        .iter()
        .map(|r| (r.name.clone(), r.count))
        .collect();
    let mentions_me = message_mentions_user(&text, my_user_id);
    let (media_type, file_ids, file_urls, file_names) =
        detect_message_media(&msg.files, &msg.attachments)
            .map(|(mt, ids, urls, names)| (Some(mt), ids, urls, names))
            .unwrap_or((None, Vec::new(), Vec::new(), Vec::new()));

    MessageData {
        sender_name,
        text,
        is_outgoing: msg.user.as_deref() == Some(my_user_id),
        ts: msg.ts.clone(),
        reactions,
    reply_count: reply_count_override.unwrap_or_else(|| msg.reply_count.unwrap_or(0)),
        cards: crate::slack::attachments_to_cards(&msg.attachments),
        mentions_me,
        local_echo_id: None,
        is_edited: false,
        is_deleted: false,
        media_type,
        file_ids,
        file_urls,
        file_names,
    }
}

/// Convert a realtime `SlackUpdate::NewMessage` payload into `MessageData`.
pub fn realtime_message_to_message_data(
    user_name: &str,
    text: &str,
    ts: &str,
    is_self: bool,
    cards: &[AttachmentCard],
    mentions_me: bool,
    files: &[SlackFile],
    inline_image_urls: &[(String, String)],
) -> MessageData {
    let (media_type, file_ids, file_urls, file_names) =
        media_fields_from_files_and_inline(files, inline_image_urls);

    MessageData {
        sender_name: user_name.to_string(),
        text: text.to_string(),
        is_outgoing: is_self,
        ts: ts.to_string(),
        reactions: Vec::new(),
        reply_count: 0,
        cards: cards.to_vec(),
        mentions_me,
        local_echo_id: None,
        is_edited: false,
        is_deleted: false,
        media_type,
        file_ids,
        file_urls,
        file_names,
    }
}

/// Local echo shown immediately after the user sends a message.
pub fn local_echo_message_data(
    sender_name: String,
    text: String,
    local_echo_id: u64,
) -> MessageData {
    MessageData {
        sender_name,
        text,
        is_outgoing: true,
        ts: format!(
            "{}.local.{}",
            chrono::Local::now().timestamp(),
            local_echo_id
        ),
        reactions: Vec::new(),
        reply_count: 0,
        cards: Vec::new(),
        mentions_me: false,
        local_echo_id: Some(local_echo_id),
        is_edited: false,
        is_deleted: false,
        media_type: None,
        file_ids: Vec::new(),
        file_urls: Vec::new(),
        file_names: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::{BotProfile, SlackReaction};

    fn sample_msg(text: &str, user: Option<&str>) -> SlackMessage {
        SlackMessage {
            msg_type: "message".to_string(),
            ts: "1234.5678".to_string(),
            user: user.map(str::to_string),
            text: text.to_string(),
            bot_id: None,
            username: None,
            app_id: None,
            bot_profile: None,
            reactions: vec![SlackReaction {
                name: "thumbsup".to_string(),
                count: 2,
            }],
            thread_ts: None,
            reply_count: Some(3),
            attachments: Vec::new(),
            files: Vec::new(),
            blocks: Vec::new(),
        }
    }

    #[test]
    fn test_message_mentions_user() {
        assert!(message_mentions_user("<@U123> hello", "U123"));
        assert!(message_mentions_user("hey <@U123|bob>", "U123"));
        assert!(!message_mentions_user("hello world", "U123"));
        assert!(!message_mentions_user("<@U123>", ""));
    }

    #[test]
    fn test_slack_message_to_message_data_user() {
        let msg = sample_msg("hello <@U999>", Some("U111"));
        let mut cache = HashMap::new();
        cache.insert("U111".to_string(), "Alice".to_string());
        let data = slack_message_to_message_data(&msg, "U111", &cache);
        assert_eq!(data.sender_name, "Alice");
        assert_eq!(data.text, "hello <@U999>");
        assert!(data.is_outgoing);
        assert_eq!(data.reactions.len(), 1);
        assert_eq!(data.reply_count, 3);
    }

    #[test]
    fn test_slack_message_to_message_data_bot_profile() {
        let mut msg = sample_msg("bot says hi", None);
        msg.bot_profile = Some(BotProfile {
            id: None,
            name: Some("Deploy Bot".to_string()),
            app_id: None,
        });
        let data = slack_message_to_message_data(&msg, "U111", &HashMap::new());
        assert_eq!(data.sender_name, "Deploy Bot");
        assert!(!data.is_outgoing);
    }

    #[test]
    fn test_detect_media_type_image() {
        let files = vec![SlackFile {
            id: Some("F1".to_string()),
            name: Some("pic.png".to_string()),
            mimetype: Some("image/png".to_string()),
            filetype: None,
            url_private: Some("https://example.com/pic".to_string()),
            url_private_download: None,
            thumb_64: None,
            thumb_360: None,
            thumb_480: None,
            thumb_720: None,
            thumb_800: None,
            thumb_960: None,
            thumb_1024: None,
            size: None,
        }];
        let result = detect_media_type(&files).unwrap();
        assert_eq!(result.0, "image");
        assert_eq!(result.1, vec!["F1".to_string()]);
    }

    #[test]
    fn test_realtime_message_with_inline_images() {
        let data = realtime_message_to_message_data(
            "Bot",
            "gif",
            "1.0",
            false,
            &[],
            false,
            &[],
            &[("https://example.com/a.gif".to_string(), "a.gif".to_string())],
        );
        assert_eq!(data.media_type.as_deref(), Some("image"));
        assert_eq!(data.file_urls.len(), 1);
    }
}
