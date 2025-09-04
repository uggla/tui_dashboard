use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use notify_rust::{Hint, Notification, Timeout, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::Alignment;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Clear;
use tui_big_text::BigText;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let res = run_app(&mut terminal);

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

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let start_duration = Duration::from_secs(30); // 30 seconds
    let mut notified = false;
    let mut zero_at: Option<Instant> = None;

    loop {
        let elapsed = start.elapsed();
        let remaining = if elapsed >= start_duration {
            Duration::from_secs(0)
        } else {
            start_duration - elapsed
        };

        // Determine blink state: after hitting zero, toggle visibility every 500ms
        let show = if let Some(z) = zero_at {
            ((Instant::now() - z).as_millis() / 500) % 2 == 0
        } else {
            true
        };

        terminal.draw(|f| {
            let size = f.area();

            // Optional: center a bit with layout, though BigText fits whole area
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(20),
                    Constraint::Percentage(60),
                    Constraint::Percentage(20),
                ])
                .split(size);

            let time_str = format_mmss(remaining);

            if show {
                let big = BigText::builder()
                    .style(Style::default().fg(Color::Cyan))
                    .alignment(Alignment::Center)
                    .lines(vec![Line::from(time_str)])
                    .build();
                f.render_widget(big, chunks[1]);
            } else {
                // Clear the area to blink off
                f.render_widget(Clear, chunks[1]);
            }
        })?;

        // When timer reaches zero, notify + bell once, then keep blinking until user quits
        if remaining.is_zero() && !notified {
            // Generic, DE-agnostic notification request
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

            notified = true;
            zero_at = Some(Instant::now());
        }

        // Poll for input; tick ~10 times per second
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(k) = event::read()?
            && k.kind == KeyEventKind::Press
        {
            match k.code {
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('c')
                    if k.modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
                    break;
                }
                KeyCode::Char('q') | KeyCode::Esc => break,
                _ => {}
            }
        }
    }

    Ok(())
}

fn format_mmss(dur: Duration) -> String {
    let secs = dur.as_secs();
    let m = (secs / 60) % 100;
    let s = secs % 60;
    format!("{m:02}:{s:02}")
}
