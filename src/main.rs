use std::io;
use std::time::Duration;
use std::env;

use crossterm::event::{KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use notify_rust::{Hint, Notification, Timeout, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

mod app;
mod ui;
mod events;
use app::{App, Mode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load env and API key
    let _ = dotenvy::dotenv();
    let api_key = env::var("SNCF_API_KEY")?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, api_key);

    // Restore terminal
    disable_raw_mode().ok();
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )
    .ok();
    terminal.show_cursor().ok();

    res
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, api_key: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new(api_key)?;
    if app.config.is_some() { let _ = app.refresh_journeys(); app.update_timer_from_selection(); }

    loop {
        terminal.draw(|f| match app.mode { Mode::InputStart | Mode::InputDest | Mode::InputDuration => ui::draw_input(f, &app), Mode::Timer => ui::draw_timer(f, &app) })?;

        match app.mode {
            Mode::InputStart | Mode::InputDest => { app.maybe_fetch_suggestions(); }
            Mode::InputDuration => { /* no suggestions */ }
            Mode::Timer => {
                // After reaching zero, send notification once
                let elapsed = app.timer.start.elapsed();
                let remaining = app.remaining_time(elapsed);
                if remaining.is_zero() && !app.timer.notified {
                    let _ = Notification::new()
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
                        .show();
                    app.timer.notified = true;
                    app.timer.zero_at = Some(std::time::Instant::now());
                }
            }
        }

        if let Some(events::AppEvent::Key(k)) = events::poll_event(Duration::from_millis(100))
            && k.kind == KeyEventKind::Press
        {
            match app.mode {
                Mode::InputStart | Mode::InputDest => app.handle_station_keys(k.code),
                Mode::InputDuration => app.handle_duration_keys(k.code),
                Mode::Timer => match k.code {
                    KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('c') if k.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => return Ok(()),
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('r') => { let _ = app.refresh_journeys(); },
                    KeyCode::Up => { if app.journeys_selected > 0 { app.journeys_selected -= 1; app.update_timer_from_selection(); } },
                    KeyCode::Down => { if app.journeys_selected + 1 < app.journeys.len() { app.journeys_selected += 1; app.update_timer_from_selection(); } },
                    KeyCode::Enter => app.update_timer_from_selection(),
                    _ => {}
                },
            }
        }
    }
}
