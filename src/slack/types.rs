use serde::{Deserialize, Serialize};

/// Updates received from Slack
#[derive(Debug, Clone)]
pub enum SlackUpdate {
    NewMessage {
        channel_id: String,
        user_name: String,
        text: String,
        ts: String,
        thread_ts: Option<String>,
        is_bot: bool,
        is_self: bool,
        cards: Vec<crate::widgets::AttachmentCard>,
        // (url, file_name) pairs from attachment.image_url — Giphy and other
        // unfurled image attachments rendered inline.
        inline_image_urls: Vec<(String, String)>,
        mentions_me: bool,
        files: Vec<SlackFile>,
    },
    MessageChanged {
        channel_id: String,
        ts: String,
        new_text: String,
    },
    MessageDeleted {
        channel_id: String,
        ts: String,
    },
    UserTyping {
        channel_id: String,
        user_name: String,
    },
    ReactionAdded {
        channel_id: String,
        message_ts: String,
        reaction: String,
    },
    ReactionRemoved {
        channel_id: String,
        message_ts: String,
        reaction: String,
    },
    MemberJoinedChannel {
        channel_id: String,
        user_id: String,
    },
    MemberLeftChannel {
        channel_id: String,
        user_id: String,
    },
    ChannelRenamed {
        channel_id: String,
        name: String,
    },
    ChannelLifecycle {
        channel_id: String,
        archived: bool,
    },
    UserProfileChanged {
        user_id: String,
    },
    RefreshChatList,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct SlackReaction {
    pub name: String,
    pub count: u32,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct SlackMessage {
    #[serde(rename = "type", default)]
    pub msg_type: String,
    pub ts: String,
    pub user: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub bot_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub bot_profile: Option<BotProfile>,
    #[serde(default)]
    pub reactions: Vec<SlackReaction>,
    #[serde(default)]
    pub thread_ts: Option<String>,
    #[serde(default)]
    pub reply_count: Option<u32>,
    #[serde(default)]
    pub attachments: Vec<SlackAttachment>,
    #[serde(default)]
    pub files: Vec<SlackFile>,
    #[serde(default)]
    pub blocks: Vec<SlackBlock>,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct BotProfile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub app_id: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SlackFile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mimetype: Option<String>,
    #[serde(default)]
    pub filetype: Option<String>,
    #[serde(default)]
    pub url_private: Option<String>,
    #[serde(default)]
    pub url_private_download: Option<String>,
    #[serde(default)]
    pub thumb_64: Option<String>,
    #[serde(default)]
    pub thumb_360: Option<String>,
    #[serde(default)]
    pub thumb_480: Option<String>,
    #[serde(default)]
    pub thumb_720: Option<String>,
    #[serde(default)]
    pub thumb_800: Option<String>,
    #[serde(default)]
    pub thumb_960: Option<String>,
    #[serde(default)]
    pub thumb_1024: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct SlackAttachment {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub pretext: Option<String>,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub author_link: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_link: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub blocks: Vec<SlackBlock>,
}

// ---------- Block Kit ----------

#[derive(Deserialize, Serialize, Clone, Default)]
pub struct SlackTextObject {
    #[serde(rename = "type", default)]
    pub obj_type: String,
    #[serde(default)]
    pub text: String,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum SlackBlock {
    #[serde(rename = "section")]
    Section {
        #[serde(default)]
        text: Option<SlackTextObject>,
        #[serde(default)]
        fields: Vec<SlackTextObject>,
    },
    #[serde(rename = "header")]
    Header {
        #[serde(default)]
        text: Option<SlackTextObject>,
    },
    #[serde(rename = "context")]
    Context {
        #[serde(default)]
        elements: Vec<SlackContextElement>,
    },
    #[serde(rename = "divider")]
    Divider,
    #[serde(rename = "rich_text")]
    RichText {
        #[serde(default)]
        elements: Vec<RichTextBlock>,
    },
    #[serde(rename = "image")]
    Image {
        #[serde(default)]
        title: Option<SlackTextObject>,
        #[serde(default)]
        alt_text: Option<String>,
        #[serde(default)]
        image_url: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum SlackContextElement {
    #[serde(rename = "mrkdwn")]
    Mrkdwn {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "plain_text")]
    PlainText {
        #[serde(default)]
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum RichTextBlock {
    #[serde(rename = "rich_text_section")]
    Section {
        #[serde(default)]
        elements: Vec<RichTextElement>,
    },
    #[serde(rename = "rich_text_list")]
    List {
        #[serde(default)]
        style: String,
        #[serde(default)]
        elements: Vec<RichTextBlock>,
    },
    #[serde(rename = "rich_text_quote")]
    Quote {
        #[serde(default)]
        elements: Vec<RichTextElement>,
    },
    #[serde(rename = "rich_text_preformatted")]
    Preformatted {
        #[serde(default)]
        elements: Vec<RichTextElement>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub struct RichTextStyle {
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub strike: bool,
    #[serde(default)]
    pub code: bool,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum RichTextElement {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
        #[serde(default)]
        style: RichTextStyle,
    },
    #[serde(rename = "link")]
    Link {
        #[serde(default)]
        url: String,
        #[serde(default)]
        text: Option<String>,
    },
    #[serde(rename = "emoji")]
    Emoji {
        #[serde(default)]
        name: String,
    },
    #[serde(rename = "user")]
    User {
        #[serde(default)]
        user_id: String,
    },
    #[serde(rename = "channel")]
    Channel {
        #[serde(default)]
        channel_id: String,
    },
    #[serde(rename = "usergroup")]
    UserGroup {
        #[serde(default)]
        usergroup_id: String,
    },
    #[serde(rename = "broadcast")]
    Broadcast {
        #[serde(default)]
        range: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct AuthTestResponse {
    pub(crate) ok: bool,
    pub(crate) user_id: String,
    pub(crate) team: String,
    pub(crate) team_id: String,
}

#[derive(Deserialize)]
pub(crate) struct ConversationsListResponse {
    pub(crate) ok: bool,
    pub(crate) channels: Vec<Channel>,
    #[serde(default)]
    pub(crate) error: Option<String>,
    #[serde(default)]
    pub(crate) response_metadata: Option<ResponseMetadata>,
}

#[derive(Deserialize)]
pub(crate) struct Channel {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) user: Option<String>,
    #[serde(default)]
    pub(crate) is_group: bool,
    #[serde(default)]
    pub(crate) is_im: bool,
    #[serde(default)]
    pub(crate) is_mpim: bool,
    #[serde(default)]
    pub(crate) is_private: bool,
    #[serde(default)]
    pub(crate) is_archived: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) is_member: bool,
    #[serde(default)]
    pub(crate) is_shared: bool,
    #[serde(default)]
    pub(crate) is_ext_shared: bool,
    #[serde(default)]
    pub(crate) is_org_shared: bool,
    #[serde(default)]
    pub(crate) unread_count: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct ConversationMembersResponse {
    pub(crate) ok: bool,
    #[serde(default)]
    pub(crate) members: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct ConversationHistoryResponse {
    pub(crate) ok: bool,
    pub(crate) messages: Vec<SlackMessage>,
    #[serde(default)]
    pub(crate) response_metadata: Option<ResponseMetadata>,
}

#[derive(Deserialize)]
pub(crate) struct ResponseMetadata {
    #[serde(default)]
    pub(crate) next_cursor: String,
}
#[derive(Deserialize)]
pub(crate) struct UserInfoResponse {
    pub(crate) ok: bool,
    pub(crate) user: User,
}

#[derive(Deserialize)]
pub(crate) struct UserProfile {
    #[serde(default)]
    pub(crate) display_name: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(crate) struct User {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) real_name: Option<String>,
    #[serde(default)]
    pub(crate) profile: Option<UserProfile>,
    #[serde(default)]
    pub(crate) is_bot: bool,
    #[serde(default)]
    pub(crate) deleted: bool,
}

#[derive(Clone)]
pub(crate) struct CachedUserInfo {
    pub(crate) name: String,
    pub(crate) is_bot: bool,
    pub(crate) deleted: bool,
}

#[derive(Deserialize)]
pub(crate) struct SocketModeConnectResponse {
    pub(crate) ok: bool,
    pub(crate) url: String,
}
