use once_cell::sync::Lazy;
use ratatui::style::Color;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

static SLACK_EMOJI: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("+1", "\u{1F44D}");
    m.insert("thumbsup", "\u{1F44D}");
    m.insert("-1", "\u{1F44E}");
    m.insert("thumbsdown", "\u{1F44E}");
    m.insert("heart", "\u{2764}\u{FE0F}");
    m.insert("heart_eyes", "\u{1F60D}");
    m.insert("joy", "\u{1F602}");
    m.insert("rofl", "\u{1F923}");
    m.insert("smile", "\u{1F604}");
    m.insert("grinning", "\u{1F600}");
    m.insert("smiley", "\u{1F603}");
    m.insert("laughing", "\u{1F606}");
    m.insert("wink", "\u{1F609}");
    m.insert("blush", "\u{1F60A}");
    m.insert("yum", "\u{1F60B}");
    m.insert("sunglasses", "\u{1F60E}");
    m.insert("thinking_face", "\u{1F914}");
    m.insert("thinking", "\u{1F914}");
    m.insert("raised_hands", "\u{1F64C}");
    m.insert("clap", "\u{1F44F}");
    m.insert("fire", "\u{1F525}");
    m.insert("100", "\u{1F4AF}");
    m.insert("tada", "\u{1F389}");
    m.insert("party_popper", "\u{1F389}");
    m.insert("rocket", "\u{1F680}");
    m.insert("star", "\u{2B50}");
    m.insert("eyes", "\u{1F440}");
    m.insert("wave", "\u{1F44B}");
    m.insert("pray", "\u{1F64F}");
    m.insert("muscle", "\u{1F4AA}");
    m.insert("ok_hand", "\u{1F44C}");
    m.insert("v", "\u{270C}\u{FE0F}");
    m.insert("point_up", "\u{261D}\u{FE0F}");
    m.insert("point_down", "\u{1F447}");
    m.insert("point_left", "\u{1F448}");
    m.insert("point_right", "\u{1F449}");
    m.insert("sob", "\u{1F62D}");
    m.insert("cry", "\u{1F622}");
    m.insert("angry", "\u{1F620}");
    m.insert("rage", "\u{1F621}");
    m.insert("scream", "\u{1F631}");
    m.insert("fearful", "\u{1F628}");
    m.insert("sweat", "\u{1F613}");
    m.insert("disappointed", "\u{1F61E}");
    m.insert("confused", "\u{1F615}");
    m.insert("neutral_face", "\u{1F610}");
    m.insert("expressionless", "\u{1F611}");
    m.insert("unamused", "\u{1F612}");
    m.insert("rolling_eyes", "\u{1F644}");
    m.insert("grimacing", "\u{1F62C}");
    m.insert("relieved", "\u{1F60C}");
    m.insert("pensive", "\u{1F614}");
    m.insert("sleepy", "\u{1F62A}");
    m.insert("sleeping", "\u{1F634}");
    m.insert("mask", "\u{1F637}");
    m.insert("nerd_face", "\u{1F913}");
    m.insert("worried", "\u{1F61F}");
    m.insert("flushed", "\u{1F633}");
    m.insert("hugs", "\u{1F917}");
    m.insert("hugging_face", "\u{1F917}");
    m.insert("cowboy_hat_face", "\u{1F920}");
    m.insert("clown_face", "\u{1F921}");
    m.insert("shushing_face", "\u{1F92B}");
    m.insert("exploding_head", "\u{1F92F}");
    m.insert("partying_face", "\u{1F973}");
    m.insert("star_struck", "\u{1F929}");
    m.insert("money_mouth_face", "\u{1F911}");
    m.insert("zany_face", "\u{1F92A}");
    m.insert("skull", "\u{1F480}");
    m.insert("ghost", "\u{1F47B}");
    m.insert("alien", "\u{1F47D}");
    m.insert("robot_face", "\u{1F916}");
    m.insert("poop", "\u{1F4A9}");
    m.insert("hankey", "\u{1F4A9}");
    m.insert("see_no_evil", "\u{1F648}");
    m.insert("hear_no_evil", "\u{1F649}");
    m.insert("speak_no_evil", "\u{1F64A}");
    m.insert("kiss", "\u{1F48B}");
    m.insert("cupid", "\u{1F498}");
    m.insert("sparkling_heart", "\u{1F496}");
    m.insert("broken_heart", "\u{1F494}");
    m.insert("orange_heart", "\u{1F9E1}");
    m.insert("yellow_heart", "\u{1F49B}");
    m.insert("green_heart", "\u{1F49A}");
    m.insert("blue_heart", "\u{1F499}");
    m.insert("purple_heart", "\u{1F49C}");
    m.insert("black_heart", "\u{1F5A4}");
    m.insert("white_heart", "\u{1F90D}");
    m.insert("two_hearts", "\u{1F495}");
    m.insert("revolving_hearts", "\u{1F49E}");
    m.insert("check", "\u{2705}");
    m.insert("white_check_mark", "\u{2705}");
    m.insert("x", "\u{274C}");
    m.insert("heavy_check_mark", "\u{2714}\u{FE0F}");
    m.insert("warning", "\u{26A0}\u{FE0F}");
    m.insert("no_entry", "\u{26D4}");
    m.insert("question", "\u{2753}");
    m.insert("exclamation", "\u{2757}");
    m
});

/// Convert a Slack emoji name to its Unicode character.
/// Looks up our curated static map first (fast common path), then falls back
/// to the `emojis` crate which covers every Unicode shortcode.
pub fn slack_emoji_to_unicode(name: &str) -> String {
    // Handle skin tone modifiers
    let base_name = if let Some(idx) = name.find("::skin-tone-") {
        &name[..idx]
    } else {
        name
    };

    if let Some(&emoji) = SLACK_EMOJI.get(base_name) {
        return emoji.to_string();
    }
    if let Some(e) = emojis::get_by_shortcode(base_name) {
        return e.as_str().to_string();
    }
    // Some Slack shortcodes use "_" but the emojis crate canonicalizes "-"
    // (or vice versa) — try the swap before giving up.
    let alt = if base_name.contains('_') {
        base_name.replace('_', "-")
    } else if base_name.contains('-') {
        base_name.replace('-', "_")
    } else {
        String::new()
    };
    if !alt.is_empty() {
        if let Some(e) = emojis::get_by_shortcode(&alt) {
            return e.as_str().to_string();
        }
    }
    format!(":{}:", name)
}

/// Replace :emoji_name: patterns in text with Unicode characters.
pub fn convert_slack_emojis(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == ':' {
            // Look for closing :
            let rest = &text[i + 1..];
            if let Some(end) = rest.find(':') {
                let name = &rest[..end];
                // Emoji names are alphanumeric with underscores, hyphens, plus, minus
                if !name.is_empty()
                    && !name.contains(' ')
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '+')
                {
                    let converted = slack_emoji_to_unicode(name);
                    if !converted.starts_with(':') {
                        result.push_str(&converted);
                        // Skip past the closing colon
                        for _ in 0..=end {
                            chars.next();
                        }
                        continue;
                    }
                }
            }
            result.push(c);
        } else {
            result.push(c);
        }
    }

    result
}

/// Convert Slack user mentions <@U12345> to @name.
#[allow(dead_code)]
pub fn convert_slack_mentions(text: &str, resolve_user: &(impl Fn(&str) -> String + ?Sized)) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("<@") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(end) = after.find('>') {
            let inner = &after[..end];
            // Could be <@U12345> or <@U12345|name>
            let user_id = if let Some(pipe) = inner.find('|') {
                &inner[..pipe]
            } else {
                inner
            };
            let name = resolve_user(user_id);
            result.push('@');
            result.push_str(&name);
            rest = &after[end + 1..];
        } else {
            result.push_str("<@");
            rest = after;
        }
    }
    result.push_str(rest);
    result
}

/// Convert Slack link format <URL|text> and <URL> to just the URL.
#[allow(dead_code)]
pub fn convert_slack_links(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('<') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('>') {
            let inner = &after[..end];
            if inner.starts_with("http://") || inner.starts_with("https://") {
                // <URL|text> -> URL, <URL> -> URL
                let url = if let Some(pipe) = inner.find('|') {
                    &inner[..pipe]
                } else {
                    inner
                };
                result.push_str(url);
            } else if inner.starts_with('@') {
                // User mention - keep as-is with angle brackets for convert_slack_mentions
                result.push('<');
                result.push_str(inner);
                result.push('>');
            } else {
                result.push_str(inner);
            }
            rest = &after[end + 1..];
        } else {
            result.push('<');
            rest = after;
        }
    }
    result.push_str(rest);
    result
}

/// Remove skin-tone modifiers like :skin-tone-6: from text
fn remove_skin_tone_modifiers(text: &str) -> String {
    // Pattern: :skin-tone-X: where X is a digit
    // Use a simple string replacement since we know the exact pattern
    let mut result = text.to_string();
    for i in 1..=6 {
        let pattern = format!(":skin-tone-{}:", i);
        result = result.replace(&pattern, "");
    }
    result
}

/// Format message text: convert links, mentions, and emojis.
#[allow(dead_code)]
pub fn format_message_text(
    text: &str,
    show_emojis: bool,
    resolve_user: &(impl Fn(&str) -> String + ?Sized),
) -> String {
    let mut out = convert_slack_links(text);
    out = remove_skin_tone_modifiers(&out);
    out = convert_slack_mentions(&out, resolve_user);
    if show_emojis {
        out = convert_slack_emojis(&out);
    }
    out
}

/// Background color used for triple-backtick code blocks. Exposed so the
/// renderer can detect block-only lines and pad the row.
pub const CODE_BLOCK_BG: ratatui::style::Color = ratatui::style::Color::Rgb(25, 25, 35);

/// Format message text into styled spans, parsing Slack mrkdwn:
///   *bold*, _italic_, ~strike~, `code`, ```code block```
/// Plus line-level constructs: `> ` blockquote, `• ` / `N. ` lists.
/// Mentions, channel refs, links and emojis are also resolved.
pub fn format_message_spans(
    text: &str,
    show_emojis: bool,
    resolve_user: &(impl Fn(&str) -> String + ?Sized),
) -> Vec<ratatui::text::Span<'static>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::Span;

    // Don't re-process line prefixes inside a triple-backtick block.
    // Detect blocks first and split the text into (in_code, slice) chunks.
    let chunks = split_code_fences(text);

    let mut out: Vec<Span<'static>> = Vec::new();
    for (in_code, chunk) in chunks {
        if in_code {
            // Keep the raw text exactly so character offsets are preserved
            // (matters for live input preview where cursor positioning is
            // computed against the raw buffer). Use the dedicated CODE_BLOCK_BG
            // shade so the renderer can recognize block lines and pad the row.
            out.push(Span::styled(
                chunk.to_string(),
                Style::default().fg(Color::LightYellow).bg(CODE_BLOCK_BG),
            ));
            continue;
        }
        let mut first_line = true;
        for line in chunk.split('\n') {
            if !first_line {
                out.push(Span::raw("\n"));
            }
            first_line = false;

            let (prefix, content, base) = classify_line_prefix(line);
            for span in prefix {
                out.push(span);
            }
            // Encode mentions/channels/broadcasts/links into private-use
            // sentinels so the mrkdwn parser leaves them alone. Then extract
            // them as chip-styled spans after parsing.
            let mut prepared = remove_skin_tone_modifiers(content);
            prepared = encode_special_tokens(&prepared, resolve_user);
            if show_emojis {
                prepared = convert_slack_emojis(&prepared);
            }
            let parsed = parse_mrkdwn_spans(&prepared);
            let chipped = extract_chip_spans(parsed);
            if let Some(base_style) = base {
                for s in chipped {
                    let combined = base_style.patch(s.style);
                    out.push(Span::styled(s.content.into_owned(), combined));
                }
            } else {
                out.extend(chipped);
            }
        }
    }

    // If everything resolved to nothing, still return a single empty span so
    // downstream wrapping treats it as one line.
    if out.is_empty() {
        out.push(Span::raw(String::new()));
    }
    out
}

/// Split text on triple-backtick fences. Returns (in_code, slice) tuples.
fn split_code_fences(text: &str) -> Vec<(bool, &str)> {
    let mut out: Vec<(bool, &str)> = Vec::new();
    let bytes = text.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    let mut last = 0;
    while i + 3 <= n {
        if bytes[i] == b'`' && bytes[i + 1] == b'`' && bytes[i + 2] == b'`' {
            // close fence?
            let open = i;
            let mut j = open + 3;
            while j + 3 <= n {
                if bytes[j] == b'`' && bytes[j + 1] == b'`' && bytes[j + 2] == b'`' {
                    if last < open {
                        out.push((false, &text[last..open]));
                    }
                    out.push((true, &text[open + 3..j]));
                    i = j + 3;
                    last = i;
                    break;
                }
                j += 1;
            }
            if j + 3 > n {
                // unterminated; treat rest as plain
                break;
            }
            continue;
        }
        i += 1;
    }
    if last < n {
        out.push((false, &text[last..n]));
    }
    out
}

/// Detect line-level prefixes (blockquote, list bullet, numbered list).
/// Returns the styled prefix spans, the remaining content, and an optional
/// base style to apply to the content.
fn classify_line_prefix(line: &str) -> (Vec<ratatui::text::Span<'static>>, &str, Option<ratatui::style::Style>) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Span;

    let bar_color = Color::Cyan;

    // Blockquote: `> ` or just `>`
    if let Some(rest) = line.strip_prefix("> ") {
        let prefix = vec![Span::styled(
            "▎ ".to_string(),
            Style::default().fg(bar_color),
        )];
        let base = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);
        return (prefix, rest, Some(base));
    }
    if line == ">" {
        let prefix = vec![Span::styled(
            "▎".to_string(),
            Style::default().fg(bar_color),
        )];
        return (prefix, "", None);
    }

    // Unordered list: `• ` (Slack bullet) or `- `
    if let Some(rest) = line.strip_prefix("• ") {
        let prefix = vec![Span::styled(
            "• ".to_string(),
            Style::default().fg(bar_color),
        )];
        return (prefix, rest, None);
    }
    if let Some(rest) = line.strip_prefix("- ") {
        let prefix = vec![Span::styled(
            "• ".to_string(),
            Style::default().fg(bar_color),
        )];
        return (prefix, rest, None);
    }

    // Ordered list: `N. ` or `N) `
    let bytes = line.as_bytes();
    let mut digits = 0;
    while digits < bytes.len() && bytes[digits].is_ascii_digit() {
        digits += 1;
    }
    if digits > 0 && digits < bytes.len() {
        let sep = bytes[digits];
        if (sep == b'.' || sep == b')')
            && bytes.get(digits + 1).copied() == Some(b' ')
        {
            let marker = &line[..digits + 2];
            let rest = &line[digits + 2..];
            let prefix = vec![Span::styled(
                marker.to_string(),
                Style::default().fg(bar_color),
            )];
            return (prefix, rest, None);
        }
    }

    (Vec::new(), line, None)
}

fn is_left_boundary(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | '[' | '{' | '<' | '"' | '\'' | '*' | '_' | '~' | '`')
}

fn is_right_boundary(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ')' | ']' | '}' | '>' | '"' | '\'' | '.' | ',' | '!' | '?' | ';' | ':' | '*' | '_' | '~' | '`'
        )
}

fn parse_mrkdwn_spans(text: &str) -> Vec<ratatui::text::Span<'static>> {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Span;

    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    // Inline code uses a slightly lighter bg than CODE_BLOCK_BG so the
    // renderer can distinguish "fill the whole row" (block) from "leave
    // surrounding text untouched" (inline).
    let code_style = Style::default()
        .fg(Color::LightYellow)
        .bg(Color::Rgb(55, 55, 65));

    while i < n {
        // Triple-backtick block
        if i + 3 <= n && chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`' {
            let search_start = i + 3;
            let mut j = search_start;
            let mut closed = None;
            while j + 3 <= n {
                if chars[j] == '`' && chars[j + 1] == '`' && chars[j + 2] == '`' {
                    closed = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(end) = closed {
                if !buf.is_empty() {
                    out.push(Span::raw(std::mem::take(&mut buf)));
                }
                let inner: String = chars[search_start..end].iter().collect();
                let trimmed = inner.trim_matches('\n').to_string();
                out.push(Span::styled(trimmed, code_style));
                i = end + 3;
                continue;
            }
        }

        // Inline `code`
        if chars[i] == '`' {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == '`') {
                if rel > 0 {
                    if !buf.is_empty() {
                        out.push(Span::raw(std::mem::take(&mut buf)));
                    }
                    let inner: String = chars[i + 1..i + 1 + rel].iter().collect();
                    out.push(Span::styled(inner, code_style));
                    i = i + 1 + rel + 1;
                    continue;
                }
            }
        }

        // *bold* / _italic_ / ~strike~
        let m = chars[i];
        if matches!(m, '*' | '_' | '~') {
            let left_ok = i == 0 || is_left_boundary(chars[i - 1]);
            let next_ok = chars
                .get(i + 1)
                .map(|&c| !c.is_whitespace() && c != m)
                .unwrap_or(false);
            if left_ok && next_ok {
                // Find the closing marker on the same logical run.
                let mut j = i + 1;
                let mut found = None;
                while j < n {
                    if chars[j] == m {
                        let prev = chars[j - 1];
                        let right_ok = j + 1 >= n || is_right_boundary(chars[j + 1]);
                        if !prev.is_whitespace() && prev != m && right_ok {
                            found = Some(j);
                            break;
                        }
                    } else if chars[j] == '\n' {
                        // Style runs don't cross blank lines.
                        break;
                    }
                    j += 1;
                }
                if let Some(end) = found {
                    if !buf.is_empty() {
                        out.push(Span::raw(std::mem::take(&mut buf)));
                    }
                    let inner: String = chars[i + 1..end].iter().collect();
                    let style = match m {
                        '*' => Style::default().add_modifier(Modifier::BOLD),
                        '_' => Style::default().add_modifier(Modifier::ITALIC),
                        '~' => Style::default().add_modifier(Modifier::CROSSED_OUT),
                        _ => Style::default(),
                    };
                    out.push(Span::styled(inner, style));
                    i = end + 1;
                    continue;
                }
            }
        }

        buf.push(chars[i]);
        i += 1;
    }
    if !buf.is_empty() {
        out.push(Span::raw(buf));
    }
    out
}

// ---- Chip / mention / link styling --------------------------------------------

// Private-use sentinel codepoints used to mark up special tokens before
// mrkdwn parsing, then extracted into chip-styled spans afterwards.
// Pairs are start/end characters wrapping the visible chip text.
const SEN_MENTION_O: char = '\u{E000}';
const SEN_MENTION_C: char = '\u{E001}';
const SEN_CHANNEL_O: char = '\u{E002}';
const SEN_CHANNEL_C: char = '\u{E003}';
const SEN_BROADCAST_O: char = '\u{E004}';
const SEN_BROADCAST_C: char = '\u{E005}';
const SEN_LINK_O: char = '\u{E006}';
const SEN_LINK_C: char = '\u{E007}';

/// Encode Slack `<...>` tokens (mentions, channels, broadcasts, links) into
/// private-use sentinels. The mrkdwn parser sees the visible chip text as
/// plain text and won't try to interpret characters inside.
fn encode_special_tokens(
    text: &str,
    resolve_user: &(impl Fn(&str) -> String + ?Sized),
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('>') {
            let inner = &after[..end];
            encode_one_token(inner, resolve_user, &mut out);
            rest = &after[end + 1..];
        } else {
            out.push('<');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn encode_one_token(
    inner: &str,
    resolve_user: &(impl Fn(&str) -> String + ?Sized),
    out: &mut String,
) {
    if inner.is_empty() {
        out.push_str("<>");
        return;
    }
    let first = inner.chars().next().unwrap();
    match first {
        '@' => {
            // <@U12345> or <@U12345|name>
            let body = &inner[1..];
            let (id, label) = match body.find('|') {
                Some(p) => (&body[..p], Some(&body[p + 1..])),
                None => (body, None),
            };
            let display = label
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| resolve_user(id));
            out.push(SEN_MENTION_O);
            out.push('@');
            out.push_str(&display);
            out.push(SEN_MENTION_C);
        }
        '#' => {
            // <#C12345> or <#C12345|channel-name>
            let body = &inner[1..];
            let (_id, label) = match body.find('|') {
                Some(p) => (&body[..p], Some(&body[p + 1..])),
                None => (body, None),
            };
            let display = label
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| inner.to_string());
            out.push(SEN_CHANNEL_O);
            out.push('#');
            out.push_str(&display);
            out.push(SEN_CHANNEL_C);
        }
        '!' => {
            // <!here>, <!channel>, <!everyone>, <!subteam^S123|name>, <!date^...>
            let body = &inner[1..];
            let (kind, label_opt) = match body.find('|') {
                Some(p) => (&body[..p], Some(&body[p + 1..])),
                None => (body, None),
            };
            let label = match (kind, label_opt) {
                ("here", _) => "@here".to_string(),
                ("channel", _) => "@channel".to_string(),
                ("everyone", _) => "@everyone".to_string(),
                (k, Some(l)) if k.starts_with("subteam") => {
                    if l.starts_with('@') {
                        l.to_string()
                    } else {
                        format!("@{}", l)
                    }
                }
                (_, Some(l)) => l.to_string(),
                (k, None) => format!("@{}", k.split('^').next().unwrap_or(k)),
            };
            out.push(SEN_BROADCAST_O);
            out.push_str(&label);
            out.push(SEN_BROADCAST_C);
        }
        _ if inner.starts_with("http://") || inner.starts_with("https://") => {
            let visible = match inner.find('|') {
                Some(p) => &inner[p + 1..],
                None => inner,
            };
            out.push(SEN_LINK_O);
            out.push_str(visible);
            out.push(SEN_LINK_C);
        }
        _ => {
            // Unknown token type — pass through.
            out.push_str(inner);
        }
    }
}

/// Walk parsed mrkdwn spans and split each span on chip sentinels, emitting
/// chip-styled spans for the marked ranges. The base style of the original
/// span is preserved on surrounding text and patched onto the chip style so
/// e.g. a *_<@user>_* still renders bold-italic with the chip background.
fn extract_chip_spans(spans: Vec<ratatui::text::Span<'static>>) -> Vec<ratatui::text::Span<'static>> {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Span;

    let mention_style = Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(67, 108, 191))
        .add_modifier(Modifier::BOLD);
    let channel_style = mention_style;
    let broadcast_style = Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(190, 60, 60))
        .add_modifier(Modifier::BOLD);
    let link_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::UNDERLINED);

    let mut out: Vec<Span<'static>> = Vec::new();
    for span in spans {
        let base = span.style;
        let text = span.content.into_owned();
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        let mut i = 0;
        let mut buf = String::new();
        while i < n {
            let c = chars[i];
            let chip_kind = match c {
                SEN_MENTION_O => Some((SEN_MENTION_C, mention_style)),
                SEN_CHANNEL_O => Some((SEN_CHANNEL_C, channel_style)),
                SEN_BROADCAST_O => Some((SEN_BROADCAST_C, broadcast_style)),
                SEN_LINK_O => Some((SEN_LINK_C, link_style)),
                _ => None,
            };
            if let Some((closer, chip_style)) = chip_kind {
                if let Some(rel) = chars[i + 1..].iter().position(|&x| x == closer) {
                    if !buf.is_empty() {
                        out.push(Span::styled(std::mem::take(&mut buf), base));
                    }
                    let inner: String = chars[i + 1..i + 1 + rel].iter().collect();
                    out.push(Span::styled(inner, base.patch(chip_style)));
                    i = i + 1 + rel + 1;
                    continue;
                }
            }
            buf.push(c);
            i += 1;
        }
        if !buf.is_empty() {
            out.push(Span::styled(buf, base));
        }
    }
    out
}

/// Generate a consistent color for a username using a hash function.
pub fn username_color(username: &str) -> Color {
    let colors = [
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::LightCyan,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::Rgb(255, 165, 0),
        Color::Rgb(147, 112, 219),
        Color::Rgb(64, 224, 208),
        Color::Rgb(255, 105, 180),
        Color::Rgb(50, 205, 50),
        Color::Rgb(255, 215, 0),
    ];

    let mut hasher = DefaultHasher::new();
    username.hash(&mut hasher);
    let hash = hasher.finish();

    colors[(hash as usize) % colors.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emoji_conversion() {
        assert_eq!(
            convert_slack_emojis("hello :fire: world"),
            "hello \u{1F525} world"
        );
        assert_eq!(convert_slack_emojis(":thumbsup:"), "\u{1F44D}");
        assert_eq!(convert_slack_emojis(":unknown_emoji:"), ":unknown_emoji:");
        assert_eq!(convert_slack_emojis("no emojis here"), "no emojis here");
    }

    #[test]
    fn test_slack_links() {
        assert_eq!(
            convert_slack_links("<https://example.com|click>"),
            "https://example.com"
        );
        assert_eq!(
            convert_slack_links("<https://example.com>"),
            "https://example.com"
        );
    }

    #[test]
    fn test_mentions() {
        let resolve = |id: &str| -> String {
            if id == "U123" {
                "Alice".into()
            } else {
                id.into()
            }
        };
        assert_eq!(convert_slack_mentions("hi <@U123>", &resolve), "hi @Alice");
        assert_eq!(
            convert_slack_mentions("hi <@U123|bob>", &resolve),
            "hi @Alice"
        );
    }
}
