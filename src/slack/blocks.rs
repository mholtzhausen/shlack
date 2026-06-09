use super::types::{
    RichTextBlock, RichTextElement, SlackAttachment, SlackBlock, SlackContextElement, SlackMessage,
};

/// Render a Block Kit blocks array into mrkdwn-flavored text suitable for the
/// existing message formatter (which already handles `<@U..>`, `<#C..>`, `:emoji:`,
/// and `<url|label>` tokens).
pub fn render_blocks(blocks: &[SlackBlock]) -> String {
    let mut out: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            SlackBlock::Section { text, fields } => {
                if let Some(t) = text {
                    if !t.text.is_empty() {
                        out.push(t.text.clone());
                    }
                }
                for f in fields {
                    if !f.text.is_empty() {
                        out.push(f.text.clone());
                    }
                }
            }
            SlackBlock::Header { text } => {
                if let Some(t) = text {
                    if !t.text.is_empty() {
                        out.push(format!("*{}*", t.text));
                    }
                }
            }
            SlackBlock::Context { elements } => {
                let mut parts = Vec::new();
                for el in elements {
                    match el {
                        SlackContextElement::Mrkdwn { text }
                        | SlackContextElement::PlainText { text } => {
                            if !text.is_empty() {
                                parts.push(text.clone());
                            }
                        }
                        SlackContextElement::Other => {}
                    }
                }
                if !parts.is_empty() {
                    out.push(parts.join(" · "));
                }
            }
            SlackBlock::Divider => {
                out.push("─────".to_string());
            }
            SlackBlock::RichText { elements } => {
                let rendered = render_rich_text(elements);
                if !rendered.is_empty() {
                    out.push(rendered);
                }
            }
            SlackBlock::Image { title, alt_text, image_url } => {
                let mut parts = Vec::new();
                if let Some(t) = title {
                    if !t.text.is_empty() {
                        parts.push(t.text.clone());
                    }
                }
                if let Some(alt) = alt_text.as_ref().filter(|s| !s.is_empty()) {
                    parts.push(alt.clone());
                }
                if let Some(url) = image_url.as_ref().filter(|s| !s.is_empty()) {
                    parts.push(format!("<{}>", url));
                }
                if !parts.is_empty() {
                    out.push(format!("[image] {}", parts.join(" — ")));
                }
            }
            SlackBlock::Other => {}
        }
    }
    out.join("\n")
}

fn render_rich_text(blocks: &[RichTextBlock]) -> String {
    let mut out: Vec<String> = Vec::new();
    for block in blocks {
        match block {
            RichTextBlock::Section { elements } => {
                let s = render_rich_elements(elements);
                if !s.is_empty() {
                    out.push(s);
                }
            }
            RichTextBlock::List { style, elements } => {
                let bullet_style = style.as_str();
                for (i, item) in elements.iter().enumerate() {
                    let inner = match item {
                        RichTextBlock::Section { elements } => render_rich_elements(elements),
                        other => render_rich_text(std::slice::from_ref(other)),
                    };
                    let prefix = if bullet_style == "ordered" {
                        format!("{}. ", i + 1)
                    } else {
                        "• ".to_string()
                    };
                    out.push(format!("{}{}", prefix, inner));
                }
            }
            RichTextBlock::Quote { elements } => {
                let s = render_rich_elements(elements);
                for line in s.lines() {
                    out.push(format!("> {}", line));
                }
            }
            RichTextBlock::Preformatted { elements } => {
                let s = render_rich_elements(elements);
                out.push(format!("```\n{}\n```", s));
            }
            RichTextBlock::Other => {}
        }
    }
    out.join("\n")
}

fn render_rich_elements(elements: &[RichTextElement]) -> String {
    let mut s = String::new();
    for el in elements {
        match el {
            RichTextElement::Text { text, style } => {
                let mut piece = text.clone();
                if style.code {
                    piece = format!("`{}`", piece);
                }
                if style.bold {
                    piece = format!("*{}*", piece);
                }
                if style.italic {
                    piece = format!("_{}_", piece);
                }
                if style.strike {
                    piece = format!("~{}~", piece);
                }
                s.push_str(&piece);
            }
            RichTextElement::Link { url, text } => {
                if let Some(label) = text.as_ref().filter(|t| !t.is_empty()) {
                    s.push_str(&format!("<{}|{}>", url, label));
                } else {
                    s.push_str(&format!("<{}>", url));
                }
            }
            RichTextElement::Emoji { name } => {
                s.push_str(&format!(":{}:", name));
            }
            RichTextElement::User { user_id } => {
                s.push_str(&format!("<@{}>", user_id));
            }
            RichTextElement::Channel { channel_id } => {
                s.push_str(&format!("<#{}>", channel_id));
            }
            RichTextElement::UserGroup { usergroup_id } => {
                s.push_str(&format!("<!subteam^{}>", usergroup_id));
            }
            RichTextElement::Broadcast { range } => {
                s.push_str(&format!("<!{}>", range));
            }
            RichTextElement::Other => {}
        }
    }
    s
}

/// Convert Slack attachments into structured cards for boxed UI rendering.
/// Each card carries author/title/body/footer separately so the renderer can
/// style each piece distinctly.
pub fn attachments_to_cards(attachments: &[SlackAttachment]) -> Vec<crate::widgets::AttachmentCard> {
    let mut out = Vec::new();
    for att in attachments {
        let body = if !att.blocks.is_empty() {
            let rendered = render_blocks(&att.blocks);
            if !rendered.is_empty() {
                rendered
            } else {
                att.text.clone().unwrap_or_default()
            }
        } else if let Some(text) = att.text.as_ref().filter(|t| !t.is_empty()) {
            text.clone()
        } else if let Some(fallback) = att.fallback.as_ref().filter(|t| !t.is_empty()) {
            // Only use fallback when nothing else is available.
            if fallback.len() > 400 {
                format!("{}...", &fallback[..400])
            } else {
                fallback.clone()
            }
        } else {
            String::new()
        };

        // Skip a card that has no displayable content at all.
        if body.is_empty()
            && att.author_name.as_ref().map_or(true, |s| s.is_empty())
            && att.title.as_ref().map_or(true, |s| s.is_empty())
            && att.pretext.as_ref().map_or(true, |s| s.is_empty())
            && att.footer.as_ref().map_or(true, |s| s.is_empty())
        {
            continue;
        }

        out.push(crate::widgets::AttachmentCard {
            color: att.color.clone().filter(|s| !s.is_empty()),
            author: att.author_name.clone().filter(|s| !s.is_empty()),
            title: att.title.clone().filter(|s| !s.is_empty()),
            title_link: att.title_link.clone().filter(|s| !s.is_empty()),
            pretext: att.pretext.clone().filter(|s| !s.is_empty()),
            body,
            footer: att.footer.clone().filter(|s| !s.is_empty()),
        });
    }
    out
}

/// Check if the text contains a mention of the specified user ID
/// Looks for patterns like <@U12345> or <@U12345|name>
pub(crate) fn text_mentions_user(text: &str, user_id: &str) -> bool {
    if user_id.is_empty() {
        return false;
    }
    
    // Look for <@USER_ID> or <@USER_ID|...>
    let pattern1 = format!("<@{}>", user_id);
    let pattern2 = format!("<@{}|", user_id);
    
    text.contains(&pattern1) || text.contains(&pattern2)
}

impl SlackMessage {
    /// Returns the message text. Slack's own clients prefer `blocks` over the
    /// `text` field (which is a notification fallback), so we do the same:
    /// rendered blocks win when present, then plain text, then attachment content.
    pub fn rendered_text(&self) -> String {
        let from_blocks = render_blocks(&self.blocks);
        if !from_blocks.is_empty() {
            return from_blocks;
        }
        if !self.text.is_empty() {
            return self.text.clone();
        }
        for att in &self.attachments {
            let rendered = render_blocks(&att.blocks);
            if !rendered.is_empty() {
                return rendered;
            }
            if let Some(t) = att.text.as_ref().filter(|t| !t.is_empty()) {
                return t.clone();
            }
            if let Some(t) = att.pretext.as_ref().filter(|t| !t.is_empty()) {
                return t.clone();
            }
            if let Some(t) = att.fallback.as_ref().filter(|t| !t.is_empty()) {
                return t.clone();
            }
        }
        String::new()
    }
}
