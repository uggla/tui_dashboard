use std::env;

use tui_dashboard::exit_gui;
use tui_dashboard::run_app;
use tui_dashboard::start_gui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load env and API key
    let _ = dotenvy::dotenv();
    let api_key = env::var("SNCF_API_KEY")?;

    // Setup terminal
    let mut terminal = start_gui()?;

    let res = run_app(&mut terminal, api_key).await;

    // Restore terminal
    exit_gui(terminal)?;
    res
}
