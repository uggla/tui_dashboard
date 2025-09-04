use std::fs;
use std::io;
use std::time::{Duration, Instant};
use std::env;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use notify_rust::{Hint, Notification, Timeout, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use tui_big_text::BigText;
use serde::Deserialize;

const SUGGESTION_DEBOUNCE_MS: u64 = 350;
const MIN_QUERY_LEN: usize = 2;
const DEFAULT_TIMER_SECS: u64 = 30;

#[derive(Debug, Clone, Deserialize)]
struct Place {
    id: String,
    name: String,
    #[serde(default)]
    embedded_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlacesResponse {
    places: Vec<Place>,
}

enum Mode {
    InputStart,
    InputDest,
    InputDuration,
    Timer,
}

struct InputState {
    text: String,
    cursor: usize,
    suggestions: Vec<Place>,
    selected: usize,
    last_edit_at: Instant,
    last_queried: String,
    loading: bool,
    error: Option<String>,
}

struct TimerState {
    start: Instant,
    duration: Duration,
    notified: bool,
    zero_at: Option<Instant>,
}

struct App {
    mode: Mode,
    input: InputState,
    timer: TimerState,
    client: reqwest::blocking::Client,
    api_key: String,
    chosen_start: Option<Place>,
    chosen_dest: Option<Place>,
    approach_minutes: Option<u64>,
    config: Option<AppConfig>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct SavedPlace {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct AppConfig {
    start: SavedPlace,
    destination: SavedPlace,
    approach_minutes: u64,
}

const CONFIG_PATH: &str = "config.toml";

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

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    api_key: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("tui-big-text/0.1")
        .build()?;

    let loaded_config = load_config();
    let mut app = App {
        mode: if loaded_config.is_some() { Mode::Timer } else { Mode::InputStart },
        input: InputState {
            text: String::new(),
            cursor: 0,
            suggestions: Vec::new(),
            selected: 0,
            last_edit_at: Instant::now(),
            last_queried: String::new(),
            loading: false,
            error: None,
        },
        timer: TimerState {
            start: Instant::now(),
            duration: loaded_config
                .as_ref()
                .map(|c| Duration::from_secs(c.approach_minutes * 60))
                .unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMER_SECS)),
            notified: false,
            zero_at: None,
        },
        client,
        api_key,
        chosen_start: loaded_config.as_ref().map(|c| Place { id: c.start.id.clone(), name: c.start.name.clone(), embedded_type: Some("stop_area".into()) }),
        chosen_dest: loaded_config.as_ref().map(|c| Place { id: c.destination.id.clone(), name: c.destination.name.clone(), embedded_type: Some("stop_area".into()) }),
        approach_minutes: loaded_config.as_ref().map(|c| c.approach_minutes),
        config: loaded_config,
    };

    loop {
        terminal.draw(|f| match app.mode {
            Mode::InputStart | Mode::InputDest | Mode::InputDuration => draw_input(f, &app),
            Mode::Timer => draw_timer(f, &app),
        })?;

        match app.mode {
            Mode::InputStart | Mode::InputDest => {
                // Debounced fetch for suggestions
                if app.input.text.len() >= MIN_QUERY_LEN
                    && app.input.text != app.input.last_queried
                    && app.input.last_edit_at.elapsed() >= Duration::from_millis(SUGGESTION_DEBOUNCE_MS)
                {
                    app.input.loading = true;
                    let query = app.input.text.clone();
                    match fetch_places(&app.client, &app.api_key, &query) {
                        Ok(list) => {
                            app.input.suggestions = list;
                            app.input.selected = 0;
                            app.input.error = None;
                            app.input.last_queried = query;
                        }
                        Err(e) => {
                            app.input.error = Some(format!("{e}"));
                        }
                    }
                    app.input.loading = false;
                }
            }
            Mode::InputDuration => { /* no suggestions */ }
            Mode::Timer => {
                // After reaching zero, send notification once
                let elapsed = app.timer.start.elapsed();
                let remaining = remaining_time(&app.timer, elapsed);
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
                    app.timer.zero_at = Some(Instant::now());
                }
            }
        }

        // Input handling; poll ~10 times per second
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    match app.mode {
                        Mode::InputStart | Mode::InputDest => handle_station_keys(&mut app, k.code),
                        Mode::InputDuration => handle_duration_keys(&mut app, k.code),
                        Mode::Timer => {
                            match k.code {
                                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('c')
                                    if k.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                                {
                                    break;
                                }
                                KeyCode::Char('q') | KeyCode::Esc => break,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn remaining_time(timer: &TimerState, elapsed: Duration) -> Duration {
    if elapsed >= timer.duration {
        Duration::from_secs(0)
    } else {
        timer.duration - elapsed
    }
}

fn handle_station_keys(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            // quit from input
            std::process::exit(0);
        }
        KeyCode::Enter => {
            if let Some(place) = app.input.suggestions.get(app.input.selected).cloned() {
                match app.mode {
                    Mode::InputStart => {
                        app.chosen_start = Some(place);
                        app.input.text.clear();
                        app.input.cursor = 0;
                        app.input.suggestions.clear();
                        app.input.selected = 0;
                        app.input.last_queried.clear();
                        app.input.error = None;
                        app.mode = Mode::InputDest;
                    }
                    Mode::InputDest => {
                        app.chosen_dest = Some(place);
                        app.input.text.clear();
                        app.input.cursor = 0;
                        app.input.suggestions.clear();
                        app.input.selected = 0;
                        app.input.last_queried.clear();
                        app.input.error = None;
                        app.mode = Mode::InputDuration;
                    }
                    _ => {}
                }
            }
        }
        KeyCode::Backspace => {
            if app.input.cursor > 0 && app.input.cursor <= app.input.text.len() {
                app.input.text.remove(app.input.cursor - 1);
                app.input.cursor -= 1;
                app.input.last_edit_at = Instant::now();
            }
        }
        KeyCode::Left => {
            if app.input.cursor > 0 {
                app.input.cursor -= 1;
            }
        }
        KeyCode::Right => {
            if app.input.cursor < app.input.text.len() {
                app.input.cursor += 1;
            }
        }
        KeyCode::Up => {
            if app.input.selected > 0 {
                app.input.selected -= 1;
            }
        }
        KeyCode::Down => {
            if app.input.selected + 1 < app.input.suggestions.len() {
                app.input.selected += 1;
            }
        }
        KeyCode::Char(c) => {
            app.input.text.insert(app.input.cursor, c);
            app.input.cursor += 1;
            app.input.last_edit_at = Instant::now();
        }
        _ => {}
    }
}

fn handle_duration_keys(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => std::process::exit(0),
        KeyCode::Enter => {
            match parse_minutes(&app.input.text) {
                Ok(mins) if mins > 0 => {
                    app.approach_minutes = Some(mins);
                    if let (Some(start), Some(dest), Some(minutes)) = (
                        app.chosen_start.clone(),
                        app.chosen_dest.clone(),
                        app.approach_minutes,
                    ) {
                        let conf = AppConfig {
                            start: SavedPlace { id: start.id, name: start.name },
                            destination: SavedPlace { id: dest.id, name: dest.name },
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
            }
        }
        KeyCode::Backspace => {
            if app.input.cursor > 0 && app.input.cursor <= app.input.text.len() {
                app.input.text.remove(app.input.cursor - 1);
                app.input.cursor -= 1;
            }
        }
        KeyCode::Left => { if app.input.cursor > 0 { app.input.cursor -= 1; } }
        KeyCode::Right => { if app.input.cursor < app.input.text.len() { app.input.cursor += 1; } }
        KeyCode::Char(c) => {
            app.input.text.insert(app.input.cursor, c);
            app.input.cursor += 1;
        }
        _ => {}
    }
}

fn draw_input(f: &mut ratatui::Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // input
            Constraint::Min(3),    // suggestions
        ])
        .split(centered_rect(80, 70, area));

    // Input box
    let cursor_pos = app.input.cursor.min(app.input.text.len());
    let (left, right) = app.input.text.split_at(cursor_pos);
    let input_line = Line::from(vec![
        Span::raw(left),
        Span::styled("|", Style::default().fg(Color::Yellow)),
        Span::raw(right),
    ]);
    let title = match app.mode {
        Mode::InputStart => "Start station",
        Mode::InputDest => "Destination station",
        Mode::InputDuration => "Approach time (minutes, e.g., 5 or 5mn)",
        Mode::Timer => "",
    };
    let input = Paragraph::new(input_line)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(input, chunks[0]);

    // Suggestions list
    let items: Vec<ListItem> = match app.mode {
        Mode::InputDuration => {
            let mut v = Vec::new();
            if let Some(err) = &app.input.error { v.push(ListItem::new(format!("Error: {err}"))); }
            v.push(ListItem::new("Enter minutes and press Enter"));
            v
        }
        _ => {
            if app.input.loading {
                vec![ListItem::new("Loading...")]
            } else if let Some(err) = &app.input.error {
                vec![ListItem::new(format!("Error: {err}"))]
            } else if app.input.suggestions.is_empty() && app.input.text.len() >= MIN_QUERY_LEN {
                vec![ListItem::new("No results")]
            } else {
                app.input
                    .suggestions
                    .iter()
                    .map(|p| ListItem::new(p.name.clone()))
                    .collect()
            }
        }
    };
    let list_len = items.len();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Suggestions"))
        .highlight_symbol("▶ ");
    let mut state = ratatui::widgets::ListState::default();
    if list_len > 0 {
        let sel = app.input.selected.min(list_len - 1);
        state.select(Some(sel));
    }
    f.render_stateful_widget(list, chunks[1], &mut state);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1]);
    horizontal[1]
}

fn draw_timer(f: &mut ratatui::Frame, app: &App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(size);

    if let Some(conf) = &app.config {
        let header = Paragraph::new(Line::from(vec![
            Span::styled(format!("{} ", conf.start.name), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("→ "),
            Span::styled(format!("{}  ", conf.destination.name), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("• approach {} min", conf.approach_minutes)),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Config"));
        f.render_widget(header, chunks[0]);
    }

    let elapsed = app.timer.start.elapsed();
    let remaining = remaining_time(&app.timer, elapsed);
    let show = if let Some(z) = app.timer.zero_at {
        ((Instant::now() - z).as_millis() / 500) % 2 == 0
    } else {
        true
    };

    let time_str = format_mmss(remaining);
    if show {
        let big = BigText::builder()
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center)
            .lines(vec![Line::from(time_str)])
            .build();
        f.render_widget(big, chunks[1]);
    } else {
        f.render_widget(Clear, chunks[1]);
    }
}

fn format_mmss(dur: Duration) -> String {
    let secs = dur.as_secs();
    let m = (secs / 60) % 100;
    let s = secs % 60;
    format!("{m:02}:{s:02}")
}

fn fetch_places(
    client: &reqwest::blocking::Client,
    api_key: &str,
    query: &str,
) -> Result<Vec<Place>, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.sncf.com/v1/coverage/sncf/places?q={}",
        urlencoding::encode(query)
    );
    let resp = client
        .get(url)
        .basic_auth(api_key, Some(""))
        .send()?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()).into());
    }
    let parsed: PlacesResponse = resp.json()?;
    let only_stop_areas = parsed
        .places
        .into_iter()
        .filter(|p| matches!(p.embedded_type.as_deref(), Some("stop_area")))
        .collect();
    Ok(only_stop_areas)
}

fn load_config() -> Option<AppConfig> {
    let data = fs::read_to_string(CONFIG_PATH).ok()?;
    toml::from_str(&data).ok()
}

fn save_config(conf: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let data = toml::to_string_pretty(conf)?;
    fs::write(CONFIG_PATH, data)?;
    Ok(())
}

fn parse_minutes(s: &str) -> Result<u64, ()> {
    let trimmed = s.trim().to_lowercase();
    let mut digits = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() { digits.push(ch); } else { break; }
    }
    if digits.is_empty() { return Err(()); }
    digits.parse::<u64>().map_err(|_| ())
}
