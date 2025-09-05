mod app;
mod events;
mod ui;

use app::{App, Mode};
use crossterm::{ExecutableCommand, event};
use std::io::{self, Write, stdout};
use std::time::Duration;

use crossterm::event::Event;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use notify_rust::{Hint, Notification, Timeout, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::events::handle_keys;

pub async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    api_key: String,
) -> anyhow::Result<()> {
    let mut app = App::new(api_key)?;
    if app.config.is_some() {
        let _ = app.refresh_journeys().await;
        app.update_timer_from_selection();
    }

    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        terminal.draw(|f| match app.mode {
            Mode::InputStart | Mode::InputDest | Mode::InputDuration => ui::draw_input(f, &app),
            Mode::Timer => ui::draw_timer(f, &app),
        })?;

        match app.mode {
            Mode::InputStart | Mode::InputDest => {
                app.maybe_fetch_suggestions().await;
            }
            Mode::InputDuration => { /* no suggestions */ }
            Mode::Timer => {
                // After reaching zero, send notification once
                let elapsed = app.timer.start.elapsed();
                let remaining = app.remaining_time(elapsed);
                if remaining.is_zero() && !app.timer.notified {
                    let _ = tokio::task::spawn_blocking(|| {
                        Notification::new()
                            .summary("Timer finished")
                            .body("00:00")
                            .icon("dialog-information")
                            .appname("tui-big-text")
                            .timeout(Timeout::Never)
                            .hint(Hint::Resident(true))
                            .hint(Hint::Transient(false))
                            .hint(Hint::Urgency(Urgency::Normal))
                            .hint(Hint::SoundName("complete".to_owned()))
                            .hint(Hint::SuppressSound(false))
                            .show()
                    })
                    .await;
                    app.timer.notified = true;
                    app.timer.zero_at = Some(std::time::Instant::now());
                }
            }
        }

        // tick for a short wait and handle key input
        let _ = tick.tick().await;
        if event::poll(Duration::from_millis(0))?
            && let Event::Key(key) = event::read()?
            && let Some(value) = handle_keys(&mut app, key).await
        {
            return value;
        }
    }
}

pub fn exit_gui(
    mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), anyhow::Error> {
    disable_raw_mode()?;
    ExecutableCommand::execute(&mut stdout(), LeaveAlternateScreen)?;
    stdout().flush()?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn start_gui() -> Result<Terminal<CrosstermBackend<std::io::Stdout>>, anyhow::Error> {
    ExecutableCommand::execute(&mut stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.hide_cursor()?;
    Ok(terminal)
}
