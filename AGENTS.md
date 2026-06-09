# shlack — Agent Guide

Terminal-native Slack client written in Rust with [ratatui](https://ratatui.rs). Multi-workspace, multi-pane, real-time via Slack Socket Mode, with optional inline image previews via the Kitty graphics protocol.

**Binary:** `shlack`  
**Crate:** `shlack` v0.1.0 (Rust 2021)  
**User docs:** [README.md](README.md)

## Quick commands

```bash
cargo build --release    # or ./build.sh
cargo test
./target/release/shlack
```

CI (`.github/workflows/rust.yml`) runs `cargo build` and `cargo test` on push/PR to `main`.

## Architecture

```
main.rs          Event loop: crossterm input, terminal draw, async polls
    │
    ├── app.rs           App orchestration: workspace, panes, save_state, async loads
    ├── models.rs        ChatInfo, ChatSection, ThreadInfo (shared domain types)
    ├── messages.rs      SlackMessage → MessageData conversion + media detection
    ├── events.rs        Apply SlackUpdate variants to App state
    ├── input.rs         Keybindings and compose-buffer editing
    ├── ui/              Chat list, pane, layout drawing (ratatui)
    ├── slack/           HTTP API, Socket Mode, Block Kit, file download
    ├── widgets.rs       ChatPane, MessageData, filters, attachment cards
    ├── split_view.rs    PaneNode tree for split layouts (serde-persisted)
    ├── commands.rs      Slash-command parsing and handlers
    ├── formatting.rs    Slack mrkdwn → ratatui spans (emoji, mentions, links)
    ├── persistence.rs   layout.json, aliases, threads, app settings on disk
    ├── config.rs        Multi-workspace config, first-run setup, migration
    └── utils.rs         Desktop notifications (notify-send / osascript)
```

### Event flow

1. **Startup** (`main.rs`): `App::new()` authenticates and connects Socket Mode *before* entering raw TUI mode. Kitty graphics picker is probed via stdio, then terminal alt-screen is entered.
2. **Loop**: Poll Slack updates → `events::apply_update` → poll async completions (workspace switch, chat load, image load) → expiry timers (typing, status) → resize detection → draw if `needs_redraw` → `input::handle_key`.
3. **Shutdown**: Save state, shutdown WebSocket, restore terminal.

Real-time messages arrive as `SlackUpdate` variants from `slack/socket.rs` and are drained by `App::process_slack_events()` → `events::apply_update()`.

### Key types

| Module | Type | Role |
|--------|------|------|
| `app.rs` | `App` | Central state: panes, chats, settings, caches, async channels |
| `models.rs` | `ChatInfo`, `ThreadInfo`, `ChatSection` | Sidebar channel/thread metadata |
| `widgets.rs` | `ChatPane` | Per-pane messages, scroll, input, filters, render cache |
| `widgets.rs` | `MessageData` | Raw message metadata for formatting and `/media` |
| `split_view.rs` | `PaneNode` | Binary tree of splits; maps to `App.panes` indices |
| `slack/` | `SlackClient` | HTTP + background WS task; `pending_updates` queue |
| `slack/types.rs` | `SlackUpdate` | New/changed/deleted message, typing indicator |
| `config.rs` | `Config`, `Workspace`, `Settings` | Tokens, workspaces, display defaults |
| `persistence.rs` | `LayoutData`, `PaneState`, `AppSettings` | Serialized pane layout and toggles |
| `commands.rs` | `CommandHandler` | `/thread`, `/react`, `/filter`, `/workspace`, etc. |

## Module ownership (where to edit)

| Change type | Primary file |
|-------------|--------------|
| Domain types | `src/models.rs` |
| Message convert/apply | `src/messages.rs`, `src/events.rs` |
| UI drawing | `src/ui/` |
| Keybindings / compose input | `src/input.rs` |
| Event loop / startup | `src/main.rs` |
| Workspace / pane orchestration | `src/app.rs` |
| Slack API / Socket Mode | `src/slack/` |
| Slash commands | `src/commands.rs` |
| Message rendering (mrkdwn) | `src/formatting.rs` |
| Pane data structures | `src/widgets.rs` |
| Split layout tree | `src/split_view.rs` |
| Disk persistence | `src/persistence.rs` |
| Config / setup | `src/config.rs` |

## Configuration

Runtime config directory resolution (`config.rs::get_config_dir`):

1. `<project>/config/` when run from `target/release` or `target/debug`
2. Otherwise `./config/` relative to cwd
3. Fallback: `~/.config/shlack/` (auto-migrates from legacy `~/.config/slack_client_rs/`)

| File | Purpose |
|------|---------|
| `slack_config.json` | Workspaces (name, token, app_token), active index, settings |
| `layout_<workspace>.json` | Open panes, scroll, filters, pane tree |
| `aliases.json` | Text expansion for `/alias` |
| `threads.json` | Persisted thread involvement |
| `settings.json` | Additional app settings |

Example: `config/slack_config.example.json`. First run prompts interactively if no config exists. Old single-workspace and Python-client configs are auto-migrated.

**Tokens:** User (`xoxp-`) or bot (`xoxb-`) OAuth token plus app-level Socket Mode token (`xapp-` with `connections:write`).

## UI conventions

- **Redraw model:** Set `app.needs_redraw = true` after state changes; `main` draws only when flagged.
- **Timers:** Use `std::time::Instant` (monotonic) for typing expiry and status messages — not wall clock.
- **Input:** Per-pane `input_buffer` / `input_cursor`; `Shift+Enter` for newline; `@` tab completion.
- **Splits:** `PaneNode::split_pane` on focused pane; ratios stored in tree; close pane updates indices.
- **Image preview:** `ratatui-image` + Kitty protocol; async load via `image_load_tx`/`rx`; cache in `image_cache` `RefCell<HashMap>`.
- **Render cache:** `ChatPane.cached_lines` invalidated on message/setting/resize changes via `invalidate_cache()`.

## Slack integration

- **HTTP:** `reqwest` with `rustls-tls` — conversations, history, post message, reactions, files.
- **Socket Mode:** `tokio-tungstenite` background task; shutdown via `broadcast` channel.
- **Scopes:** See README for required OAuth scopes and event subscriptions.
- **Message parsing:** `SlackMessage`, blocks, attachments → `format_message_spans`, `attachments_to_cards`.

## Slash commands

Handled in `commands.rs` when input starts with `/`:

`/thread` `/react` `/filter` `/alias` `/unalias` `/workspace` `/leave` `/help` `/media` and `/1`–`/9` for workspace switch.

## Testing

Unit tests live in:

- `src/messages.rs` — message conversion, mentions, media detection
- `src/events.rs` — thread reply / unread / debounced-save helpers
- `src/split_view.rs` — pane tree split/close/layout
- `src/formatting.rs` — emoji, mentions, link conversion
- `src/persistence.rs` — AppSettings serde round-trip

No integration tests; manual testing requires valid Slack tokens.

## Dependencies (notable)

| Crate | Use |
|-------|-----|
| `ratatui` 0.29 | TUI rendering (`unstable-rendered-line-info`) |
| `crossterm` 0.28 | Terminal I/O, raw mode, mouse |
| `tokio` 1.x | Async runtime |
| `reqwest` 0.12 | Slack REST API |
| `tokio-tungstenite` 0.24 | Socket Mode WebSocket |
| `serde` / `serde_json` | Config and persistence |
| `ratatui-image` 9.0 | Kitty inline images |
| `emojis`, `unicode-width` | Emoji and width-aware layout |

Release profile: LTO, `opt-level = 3`, stripped binary.

## Common change patterns

**Add a display toggle:** Add field to `AppSettings` in `persistence.rs`, wire Ctrl+key in `input.rs`, use in `ui/` draw path; `App.settings` is persisted via `save_state()`.

**Add a slash command:** Parse in `CommandHandler::handle_command`, implement handler, update `/help` text.

**Handle new Slack event:** Extend `SlackUpdate` in `slack/types.rs`, parse in `slack/socket.rs`, handle in `events::apply_update()`.

**New pane state field:** Add to `ChatPane`, `PaneState` in `persistence.rs`, load/save in `App::save_state` / layout restore.

## Docs site

MkDocs Material config in `mkdocs.yml`; deploy workflow in `.github/workflows/docs.yml`. Docs source expects `docs/index.md` (may need syncing from README).

## Security

- Never commit real tokens; use `config/slack_config.example.json` placeholders.
- Tokens are stored in local JSON under `config/` or `~/.config/shlack/`.
