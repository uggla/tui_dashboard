use jiff::{Unit, Zoned};
use ratatui::widgets::ListItem;
use sncf::{client::ReqwestClient, fetch_places};
use std::time::{Duration, Instant};

pub use sncf::{JourneyRow, Place};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SavedPlace {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AppConfig {
    pub start: SavedPlace,
    pub destination: SavedPlace,
    pub approach_minutes: u64,
}

pub enum Mode {
    InputStart,
    InputDest,
    InputDuration,
    Timer,
}

pub struct InputState {
    pub text: String,
    pub cursor: usize,
    pub suggestions: Vec<Place>,
    pub selected: usize,
    pub last_edit_at: Instant,
    pub last_queried: String,
    pub loading: bool,
    pub error: Option<String>,
}

pub struct TimerState {
    pub start: Instant,
    pub duration: Duration,
    pub notified: bool,
    pub zero_at: Option<Instant>,
}

pub struct App {
    pub mode: Mode,
    pub input: InputState,
    pub timer: TimerState,
    pub client: ReqwestClient,
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
    pub fn new(api_key: String) -> anyhow::Result<Self> {
        let client = sncf::client::ReqwestClient::new();
        let loaded = load_config();
        Ok(Self {
            mode: if loaded.is_some() {
                Mode::Timer
            } else {
                Mode::InputStart
            },
            input: InputState {
                text: String::new(),
                cursor: 0,
                suggestions: vec![],
                selected: 0,
                last_edit_at: Instant::now(),
                last_queried: String::new(),
                loading: false,
                error: None,
            },
            timer: TimerState {
                start: Instant::now(),
                duration: loaded
                    .as_ref()
                    .map(|c| Duration::from_secs(c.approach_minutes * 60))
                    .unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMER_SECS)),
                notified: false,
                zero_at: None,
            },
            client,
            api_key,
            chosen_start: loaded.as_ref().map(|c| Place {
                id: c.start.id.clone(),
                name: c.start.name.clone(),
                embedded_type: Some("stop_area".into()),
            }),
            chosen_dest: loaded.as_ref().map(|c| Place {
                id: c.destination.id.clone(),
                name: c.destination.name.clone(),
                embedded_type: Some("stop_area".into()),
            }),
            approach_minutes: loaded.as_ref().map(|c| c.approach_minutes),
            config: loaded,
            journeys: vec![],
            journeys_selected: 0,
            journeys_loading: false,
            journeys_error: None,
        })
    }

    pub fn remaining_time(&self, elapsed: Duration) -> Duration {
        if elapsed >= self.timer.duration {
            Duration::from_secs(0)
        } else {
            self.timer.duration - elapsed
        }
    }

    pub fn input_title(&self) -> &'static str {
        match self.mode {
            Mode::InputStart => "Start station",
            Mode::InputDest => "Destination station",
            Mode::InputDuration => "Approach time (minutes)",
            Mode::Timer => "",
        }
    }

    pub fn suggestion_items(&self) -> Vec<ListItem<'_>> {
        match self.mode {
            Mode::InputDuration => {
                let mut v = Vec::new();
                if let Some(err) = &self.input.error {
                    v.push(ListItem::new(format!("Error: {err}")));
                }
                v.push(ListItem::new("Enter minutes and press Enter"));
                v
            }
            _ => {
                if self.input.loading {
                    vec![ListItem::new("Loading...")]
                } else if let Some(err) = &self.input.error {
                    vec![ListItem::new(format!("Error: {err}"))]
                } else if self.input.suggestions.is_empty()
                    && self.input.text.len() >= MIN_QUERY_LEN
                {
                    vec![ListItem::new("No results")]
                } else {
                    self.input
                        .suggestions
                        .iter()
                        .map(|p| ListItem::new(p.name.clone()))
                        .collect()
                }
            }
        }
    }

    pub async fn maybe_fetch_suggestions(&mut self) {
        if self.input.text.len() >= MIN_QUERY_LEN
            && self.input.text != self.input.last_queried
            && self.input.last_edit_at.elapsed() >= Duration::from_millis(SUGGESTION_DEBOUNCE_MS)
        {
            self.input.loading = true;
            let query = self.input.text.clone();
            match fetch_places(&self.client, &self.api_key, &query).await {
                Ok(list) => {
                    self.input.suggestions = list;
                    self.input.selected = 0;
                    self.input.error = None;
                    self.input.last_queried = query;
                }
                Err(e) => {
                    self.input.error = Some(format!("{e}"));
                }
            }
            self.input.loading = false;
        }
    }

    pub async fn refresh_journeys(&mut self) -> anyhow::Result<()> {
        let conf = match &self.config {
            Some(c) => c,
            None => return Ok(()),
        };
        self.journeys_loading = true;
        self.journeys_error = None;
        match sncf::fetch_journeys(
            &self.client,
            &self.api_key,
            &conf.start.id,
            &conf.destination.id,
        )
        .await
        {
            Ok(rows) => {
                self.journeys = rows;
                if self.journeys_selected >= self.journeys.len() {
                    self.journeys_selected = 0;
                }
            }
            Err(e) => {
                self.journeys_error = Some(format!("{e}"));
            }
        }
        self.journeys_loading = false;
        Ok(())
    }

    pub fn update_timer_from_selection(&mut self) {
        if self.journeys.is_empty() {
            return;
        }
        let sel = self.journeys_selected.min(self.journeys.len() - 1);
        let dep = self.journeys[sel].dep.clone();
        let now = Zoned::now();
        // Compute seconds until departure using Jiff spans (DST-aware)
        let mut secs = match now.until(&dep) {
            Ok(span) => span.total(Unit::Second).unwrap() as i64,

            Err(_) => 0,
        };
        if let Some(conf) = &self.config {
            secs -= (conf.approach_minutes as i64) * 60;
        }
        if secs < 0 {
            secs = 0;
        }
        self.timer.start = Instant::now();
        self.timer.duration = Duration::from_secs(secs as u64);
        self.timer.notified = false;
        self.timer.zero_at = None;
    }

    pub fn reset_input(&mut self) {
        self.input.text.clear();
        self.input.cursor = 0;
        self.input.suggestions.clear();
        self.input.selected = 0;
        self.input.last_queried.clear();
        self.input.error = None;
    }
}

pub fn load_config() -> Option<AppConfig> {
    std::fs::read_to_string(CONFIG_PATH)
        .ok()
        .and_then(|d| toml::from_str(&d).ok())
}
pub fn save_config(conf: &AppConfig) -> anyhow::Result<()> {
    let data = toml::to_string_pretty(conf)?;
    std::fs::write(CONFIG_PATH, data)?;
    Ok(())
}
