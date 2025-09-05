use std::time::{Duration, Instant};

use crossterm::event::{self, KeyCode};

use crate::app::{App, AppConfig, Mode, SavedPlace, save_config};

pub async fn handle_keys(app: &mut App, key: event::KeyEvent) -> Option<Result<(), anyhow::Error>> {
    match app.mode {
        Mode::InputStart | Mode::InputDest => handle_station_keys(app, key.code),
        Mode::InputDuration => handle_duration_keys(app, key.code),
        Mode::Timer => match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                return Some(Ok(()));
            }
            KeyCode::Char('q') | KeyCode::Esc => return Some(Ok(())),
            KeyCode::Char('r') => {
                let _ = app.refresh_journeys().await;
            }
            KeyCode::Up => {
                if app.journeys_selected > 0 {
                    app.journeys_selected -= 1;
                    app.update_timer_from_selection();
                }
            }
            KeyCode::Down => {
                if app.journeys_selected + 1 < app.journeys.len() {
                    app.journeys_selected += 1;
                    app.update_timer_from_selection();
                }
            }
            KeyCode::Enter => app.update_timer_from_selection(),
            _ => {}
        },
    }
    None
}

pub fn handle_station_keys(app: &mut App, code: crossterm::event::KeyCode) {
    use crossterm::event::KeyCode::*;
    match code {
        Char('q') | Esc => std::process::exit(0),
        Enter => {
            if let Some(place) = app.input.suggestions.get(app.input.selected).cloned() {
                match app.mode {
                    Mode::InputStart => {
                        app.chosen_start = Some(place);
                        app.reset_input();
                        app.mode = Mode::InputDest;
                    }
                    Mode::InputDest => {
                        app.chosen_dest = Some(place);
                        app.reset_input();
                        app.mode = Mode::InputDuration;
                    }
                    _ => {}
                }
            }
        }
        Backspace => {
            if app.input.cursor > 0 && app.input.cursor <= app.input.text.len() {
                app.input.text.remove(app.input.cursor - 1);
                app.input.cursor -= 1;
                app.input.last_edit_at = Instant::now();
            }
        }
        Left => {
            if app.input.cursor > 0 {
                app.input.cursor -= 1;
            }
        }
        Right => {
            if app.input.cursor < app.input.text.len() {
                app.input.cursor += 1;
            }
        }
        Up => {
            if app.input.selected > 0 {
                app.input.selected -= 1;
            }
        }
        Down => {
            if app.input.selected + 1 < app.input.suggestions.len() {
                app.input.selected += 1;
            }
        }
        Char(c) => {
            app.input.text.insert(app.input.cursor, c);
            app.input.cursor += 1;
            app.input.last_edit_at = Instant::now();
        }
        _ => {}
    }
}

pub fn handle_duration_keys(app: &mut App, code: crossterm::event::KeyCode) {
    use crossterm::event::KeyCode::*;
    match code {
        Char('q') | Esc => std::process::exit(0),
        Enter => match parse_minutes(&app.input.text) {
            Ok(mins) if mins > 0 => {
                app.approach_minutes = Some(mins);
                if let (Some(start), Some(dest), Some(minutes)) = (
                    app.chosen_start.clone(),
                    app.chosen_dest.clone(),
                    app.approach_minutes,
                ) {
                    let conf = AppConfig {
                        start: SavedPlace {
                            id: start.id,
                            name: start.name,
                        },
                        destination: SavedPlace {
                            id: dest.id,
                            name: dest.name,
                        },
                        approach_minutes: minutes,
                    };
                    let _ = save_config(&conf);
                    app.config = Some(conf);
                }
                app.timer.duration = Duration::from_secs(app.approach_minutes.unwrap() * 60);
                app.timer.start = Instant::now();
                app.timer.notified = false;
                app.timer.zero_at = None;
                app.mode = Mode::Timer;
            }
            _ => {
                app.input.error = Some("Please enter minutes, e.g., 5 or 5mn".into());
            }
        },
        Backspace => {
            if app.input.cursor > 0 && app.input.cursor <= app.input.text.len() {
                app.input.text.remove(app.input.cursor - 1);
                app.input.cursor -= 1;
            }
        }
        Left => {
            if app.input.cursor > 0 {
                app.input.cursor -= 1;
            }
        }
        Right => {
            if app.input.cursor < app.input.text.len() {
                app.input.cursor += 1;
            }
        }
        Char(c) => {
            app.input.text.insert(app.input.cursor, c);
            app.input.cursor += 1;
        }
        _ => {}
    }
}

fn parse_minutes(s: &str) -> Result<u64, ()> {
    let trimmed = s.trim().to_lowercase();
    let mut digits = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            break;
        }
    }
    if digits.is_empty() {
        return Err(());
    }
    digits.parse::<u64>().map_err(|_| ())
}
