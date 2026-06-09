use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

mod app;
mod commands;
mod events;
mod config;
mod formatting;
mod input;
mod messages;
mod models;
mod persistence;
mod slack;
mod split_view;
mod ui;
mod utils;
mod widgets;

use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Create app BEFORE entering TUI mode (so authentication can work)
    let mut app = App::new().await?;

    // Query terminal for graphics protocol BEFORE entering raw/alt-screen mode.
    let image_picker = match ratatui_image::picker::Picker::from_query_stdio() {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("Image preview disabled (picker init failed): {e}");
            None
        }
    };
    app.image_picker = image_picker;

    // Load chat history for saved panes
    let _ = app.load_all_pane_histories().await;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?; // Cursor shown only when input is focused

    // Run app
    let _res = run_app(&mut terminal, &mut app).await;

    // Save state before exiting (even if there was an error)
    let _ = app.save_state();
    
    // Shutdown WebSocket connection
    app.slack.shutdown().await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    loop {
        // Ensure pane indices are valid (workspace switch may have changed pane count)
        app.ensure_valid_pane_idx();

        // Process Slack events
        app.process_slack_events().await?;

        // Poll for workspace switch completion
        if app.poll_workspace_switch() {
            app.needs_redraw = true;
        }

        // Poll for async chat-load completion
        if app.poll_open_chat_load() {
            app.needs_redraw = true;
        }

        // Poll for async image-load completion
        if app.poll_image_loads() {
            app.needs_redraw = true;
        }

        // Handle pending chat refresh (from workspace switch)
        if app.pending_refresh_chats {
            app.pending_refresh_chats = false;
            let _ = app.refresh_chats().await;
            app.needs_redraw = true;
        }
        
        // Handle pending pane reload (from workspace switch)
        if app.pending_reload_panes {
            app.pending_reload_panes = false;
            let _ = app.reload_pane_contents().await;
            app.needs_redraw = true;
        }

        // Handle pending chat open (from mouse click)
        if app.pending_open_chat {
            app.pending_open_chat = false;
            app.open_selected_chat().await?;
            app.needs_redraw = true;
        }

        // Handle pending thread open (from mouse click on a thread row)
        if app.pending_open_thread.is_some() {
            app.open_pending_thread().await?;
            app.needs_redraw = true;
        }

        // Check expiry timers
        let now = std::time::Instant::now();
        let mut next_wake = std::time::Duration::from_millis(50);

        for pane in &mut app.panes {
            if let Some(expire) = pane.typing_expire {
                if now >= expire {
                    pane.check_typing_expired();
                    app.needs_redraw = true;
                } else {
                    let remaining = expire - now;
                    next_wake = next_wake.min(remaining);
                }
            }
        }

        if let Some(expire) = app.status_expire {
            if now >= expire {
                app.status_message = None;
                app.status_expire = None;
                app.needs_redraw = true;
            } else {
                next_wake = next_wake.min(expire - now);
            }
        }

        // Resize detection
        let size = terminal.size()?;
        if (size.width, size.height) != app.last_terminal_size {
            app.last_terminal_size = (size.width, size.height);
            for pane in &mut app.panes {
                pane.invalidate_cache();
            }
            app.needs_redraw = true;
        }

        // Draw ONLY if something changed
        if app.needs_redraw {
            terminal.draw(|f| app.draw(f))?;
            app.needs_redraw = false;
        }

        if event::poll(next_wake)? {
            let event = event::read()?;
            match event {
                Event::Key(key) => {
                    if input::handle_key(app, key).await? {
                        break;
                    }
                }
                Event::Mouse(mouse_event) => {
                    // Only handle mouse events if mouse support is enabled
                    if !app.settings.mouse_support {
                        continue;
                    }
                    
                    use crossterm::event::MouseEventKind;
                    match mouse_event.kind {
                        MouseEventKind::Down(_) => {
                            app.handle_mouse_click(mouse_event.column, mouse_event.row);
                        }
                        MouseEventKind::ScrollUp => {
                            let in_chat_list = app.chat_list_area.map_or(false, |area| {
                                mouse_event.column >= area.x
                                    && mouse_event.column < area.x + area.width
                                    && mouse_event.row >= area.y
                                    && mouse_event.row < area.y + area.height
                            });
                            if in_chat_list {
                                app.select_previous_chat();
                            } else {
                                app.scroll_up();
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            let in_chat_list = app.chat_list_area.map_or(false, |area| {
                                mouse_event.column >= area.x
                                    && mouse_event.column < area.x + area.width
                                    && mouse_event.row >= area.y
                                    && mouse_event.row < area.y + area.height
                            });
                            if in_chat_list {
                                app.select_next_chat();
                            } else {
                                app.scroll_down();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            app.needs_redraw = true;
        }
    }

    Ok(())
}
