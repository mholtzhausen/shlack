#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChatSection {
    Public = 0,
    Private = 1,
    Shared = 2,
    Group = 3,
    DirectMessage = 4,
    Bot = 5,
}

impl ChatSection {
    pub fn label(&self) -> &'static str {
        match self {
            ChatSection::Public => "Public Channels",
            ChatSection::Private => "Private Channels",
            ChatSection::Shared => "Shared Channels",
            ChatSection::Group => "Group Chats",
            ChatSection::DirectMessage => "DMs",
            ChatSection::Bot => "Bots & Apps",
        }
    }
}

/// A Slack thread the current user is involved in.
#[derive(Clone, Debug)]
pub struct ThreadInfo {
    pub channel_id: String,
    pub channel_name: String,
    pub thread_ts: String,
    pub last_reply_ts: String,
    pub unread: u32,
    pub mentioned: bool,
    pub on_my_message: bool,
    pub i_replied: bool,
    pub last_reply_user: Option<String>,
}

#[derive(Clone)]
pub struct ChatInfo {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub unread: u32,
    pub section: ChatSection,
}
