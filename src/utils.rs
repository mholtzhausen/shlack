/// Send a desktop notification with an explicit urgency level.
/// Urgency must be one of: "low", "normal", "critical".
/// On macOS, urgency is ignored (osascript has no concept of urgency).
pub fn send_desktop_notification_ex(title: &str, message: &str, urgency: &str) {
    use std::process::Command;

    #[cfg(target_os = "macos")]
    {
        let _ = urgency;
        let safe_title = title.replace('"', "\\\"");
        let safe_msg = message.replace('"', "\\\"");
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            safe_msg, safe_title
        );
        let _ = Command::new("osascript").arg("-e").arg(&script).output();
    }

    #[cfg(target_os = "linux")]
    {
        let urgency = match urgency {
            "low" | "normal" | "critical" => urgency,
            _ => "normal",
        };
        // `--category=im.received` and `--hint=string:desktop-entry:shlack`
        // help notification daemons (Dunst, GNOME Shell, KDE) group/route the
        // popup correctly and show it in the persistent notification list.
        // `--icon=mail-message-new` gives a sensible default icon.
        let _ = Command::new("notify-send")
            .arg("--app-name=shlack")
            .arg(format!("--urgency={}", urgency))
            .arg("--expire-time=8000")
            .arg("--category=im.received")
            .arg("--icon=mail-message-new")
            .arg("--hint=string:desktop-entry:shlack")
            .arg(title)
            .arg(message)
            .output();
    }
}
