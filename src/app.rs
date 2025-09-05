use crate::sncf;
use crate::sncf::{fetch_places};
use jiff::Zoned;
use ratatui::widgets::ListItem;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Place { pub id: String, pub name: String, #[serde(default)] pub embedded_type: Option<String> }

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlacesResponse { pub places: Vec<Place> }

#[derive(Debug, Clone)]
pub struct JourneyRow { pub dep: Zoned, pub arr: Zoned, pub dep_hm: String, pub arr_hm: String, pub date_str: String, pub duration_secs: i64, pub nb_transfers: i64 }

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SavedPlace { pub id: String, pub name: String }

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AppConfig { pub start: SavedPlace, pub destination: SavedPlace, pub approach_minutes: u64 }

pub enum Mode { InputStart, InputDest, InputDuration, Timer }

pub struct InputState { pub text: String, pub cursor: usize, pub suggestions: Vec<Place>, pub selected: usize, pub last_edit_at: Instant, pub last_queried: String, pub loading: bool, pub error: Option<String> }

pub struct TimerState { pub start: Instant, pub duration: Duration, pub notified: bool, pub zero_at: Option<Instant> }

pub struct App {
    pub mode: Mode,
    pub input: InputState,
    pub timer: TimerState,
    pub client: reqwest::blocking::Client,
    pub api_key: String,
    pub chosen_start: Option<Place>,
    pub chosen_dest: Option<Place>,
    pub approach_minutes: Option<u64>,
    pub config: Option<AppConfig>,
    pub journeys: Vec<JourneyRow>,
    pub journeys_selected: usize,
    pub journeys_loading: bool,
    pub journeys_error: Option<String>,
}

pub const CONFIG_PATH: &str = "config.toml";
pub const SUGGESTION_DEBOUNCE_MS: u64 = 350;
pub const MIN_QUERY_LEN: usize = 2;
pub const DEFAULT_TIMER_SECS: u64 = 30;

impl App {
    pub fn new(api_key: String) -> Result<Self, Box<dyn std::error::Error>> {
        let client = reqwest::blocking::Client::builder().user_agent("tui-big-text/0.1").build()?;
        let loaded = load_config();
        Ok(Self {
            mode: if loaded.is_some() { Mode::Timer } else { Mode::InputStart },
            input: InputState { text: String::new(), cursor: 0, suggestions: vec![], selected: 0, last_edit_at: Instant::now(), last_queried: String::new(), loading: false, error: None },
            timer: TimerState { start: Instant::now(), duration: loaded.as_ref().map(|c| Duration::from_secs(c.approach_minutes * 60)).unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMER_SECS)), notified: false, zero_at: None },
            client,
            api_key,
            chosen_start: loaded.as_ref().map(|c| Place { id: c.start.id.clone(), name: c.start.name.clone(), embedded_type: Some("stop_area".into()) }),
            chosen_dest: loaded.as_ref().map(|c| Place { id: c.destination.id.clone(), name: c.destination.name.clone(), embedded_type: Some("stop_area".into()) }),
            approach_minutes: loaded.as_ref().map(|c| c.approach_minutes),
            config: loaded,
            journeys: vec![], journeys_selected: 0, journeys_loading: false, journeys_error: None,
        })
    }

    pub fn remaining_time(&self, elapsed: Duration) -> Duration { if elapsed >= self.timer.duration { Duration::from_secs(0) } else { self.timer.duration - elapsed } }

    pub fn input_title(&self) -> &'static str { match self.mode { Mode::InputStart => "Start station", Mode::InputDest => "Destination station", Mode::InputDuration => "Approach time (minutes)", Mode::Timer => "" } }

    pub fn suggestion_items(&self) -> Vec<ListItem> {
        match self.mode {
            Mode::InputDuration => {
                let mut v = Vec::new();
                if let Some(err) = &self.input.error { v.push(ListItem::new(format!("Error: {err}"))); }
                v.push(ListItem::new("Enter minutes and press Enter")); v
            }
            _ => {
                if self.input.loading { vec![ListItem::new("Loading...")] }
                else if let Some(err) = &self.input.error { vec![ListItem::new(format!("Error: {err}"))] }
                else if self.input.suggestions.is_empty() && self.input.text.len() >= MIN_QUERY_LEN { vec![ListItem::new("No results")] }
                else { self.input.suggestions.iter().map(|p| ListItem::new(p.name.clone())).collect() }
            }
        }
    }

    pub fn handle_station_keys(&mut self, code: crossterm::event::KeyCode) {
        use crossterm::event::KeyCode::*;
        match code {
            Char('q') | Esc => std::process::exit(0),
            Enter => {
                if let Some(place) = self.input.suggestions.get(self.input.selected).cloned() {
                    match self.mode {
                        Mode::InputStart => { self.chosen_start = Some(place); self.reset_input(); self.mode = Mode::InputDest; }
                        Mode::InputDest => { self.chosen_dest = Some(place); self.reset_input(); self.mode = Mode::InputDuration; }
                        _ => {}
                    }
                }
            }
            Backspace => { if self.input.cursor > 0 && self.input.cursor <= self.input.text.len() { self.input.text.remove(self.input.cursor - 1); self.input.cursor -= 1; self.input.last_edit_at = Instant::now(); } }
            Left => { if self.input.cursor > 0 { self.input.cursor -= 1; } }
            Right => { if self.input.cursor < self.input.text.len() { self.input.cursor += 1; } }
            Up => { if self.input.selected > 0 { self.input.selected -= 1; } }
            Down => { if self.input.selected + 1 < self.input.suggestions.len() { self.input.selected += 1; } }
            Char(c) => { self.input.text.insert(self.input.cursor, c); self.input.cursor += 1; self.input.last_edit_at = Instant::now(); }
            _ => {}
        }
    }

    pub fn handle_duration_keys(&mut self, code: crossterm::event::KeyCode) {
        use crossterm::event::KeyCode::*;
        match code {
            Char('q') | Esc => std::process::exit(0),
            Enter => {
                match parse_minutes(&self.input.text) {
                    Ok(mins) if mins > 0 => {
                        self.approach_minutes = Some(mins);
                        if let (Some(start), Some(dest), Some(minutes)) = (self.chosen_start.clone(), self.chosen_dest.clone(), self.approach_minutes) {
                            let conf = AppConfig { start: SavedPlace { id: start.id, name: start.name }, destination: SavedPlace { id: dest.id, name: dest.name }, approach_minutes: minutes };
                            let _ = save_config(&conf); self.config = Some(conf);
                        }
                        self.timer.duration = Duration::from_secs(self.approach_minutes.unwrap() * 60);
                        self.timer.start = Instant::now(); self.timer.notified = false; self.timer.zero_at = None; self.mode = Mode::Timer;
                    }
                    _ => { self.input.error = Some("Please enter minutes, e.g., 5 or 5mn".into()); }
                }
            }
            Backspace => { if self.input.cursor > 0 && self.input.cursor <= self.input.text.len() { self.input.text.remove(self.input.cursor - 1); self.input.cursor -= 1; } }
            Left => { if self.input.cursor > 0 { self.input.cursor -= 1; } }
            Right => { if self.input.cursor < self.input.text.len() { self.input.cursor += 1; } }
            Char(c) => { self.input.text.insert(self.input.cursor, c); self.input.cursor += 1; }
            _ => {}
        }
    }

    pub fn maybe_fetch_suggestions(&mut self) {
        if self.input.text.len() >= MIN_QUERY_LEN && self.input.text != self.input.last_queried && self.input.last_edit_at.elapsed() >= Duration::from_millis(SUGGESTION_DEBOUNCE_MS) {
            self.input.loading = true; let query = self.input.text.clone();
            match fetch_places(&self.client, &self.api_key, &query) {
                Ok(list) => { self.input.suggestions = list; self.input.selected = 0; self.input.error = None; self.input.last_queried = query; }
                Err(e) => { self.input.error = Some(format!("{e}")); }
            }
            self.input.loading = false;
        }
    }

    pub fn refresh_journeys(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let conf = match &self.config { Some(c) => c, None => return Ok(()) };
        self.journeys_loading = true; self.journeys_error = None;
        match sncf::fetch_journeys(&self.client, &self.api_key, conf) {
            Ok(rows) => { self.journeys = rows; if self.journeys_selected >= self.journeys.len() { self.journeys_selected = 0; } }
            Err(e) => { self.journeys_error = Some(format!("{e}")); }
        }
        self.journeys_loading = false; Ok(())
    }

    pub fn update_timer_from_selection(&mut self) {
        if self.journeys.is_empty() { return; }
        let sel = self.journeys_selected.min(self.journeys.len() - 1);
        let dep = self.journeys[sel].dep.clone();
        let now = Zoned::now();
        let dep_sec = crate::sncf::rfc3339z_to_epoch(&dep.timestamp().to_string()).unwrap_or(0);
        let now_sec = crate::sncf::rfc3339z_to_epoch(&now.timestamp().to_string()).unwrap_or(0);
        let mut secs = (dep_sec - now_sec).max(0);
        if let Some(conf) = &self.config { secs -= (conf.approach_minutes as i64) * 60; }
        if secs < 0 { secs = 0; }
        self.timer.start = Instant::now(); self.timer.duration = Duration::from_secs(secs as u64); self.timer.notified = false; self.timer.zero_at = None;
    }

    fn reset_input(&mut self) { self.input.text.clear(); self.input.cursor = 0; self.input.suggestions.clear(); self.input.selected = 0; self.input.last_queried.clear(); self.input.error = None; }
}

pub fn load_config() -> Option<AppConfig> { std::fs::read_to_string(CONFIG_PATH).ok().and_then(|d| toml::from_str(&d).ok()) }
pub fn save_config(conf: &AppConfig) -> Result<(), Box<dyn std::error::Error>> { let data = toml::to_string_pretty(conf)?; std::fs::write(CONFIG_PATH, data)?; Ok(()) }
pub fn parse_minutes(s: &str) -> Result<u64, ()> { let trimmed = s.trim().to_lowercase(); let mut digits=String::new(); for ch in trimmed.chars(){ if ch.is_ascii_digit(){digits.push(ch);} else { break; } } if digits.is_empty(){return Err(());} digits.parse::<u64>().map_err(|_| ()) }

