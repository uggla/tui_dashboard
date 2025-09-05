# tui-big-text

A small Ratatui terminal app that shows a large countdown timer and a list of SNCF journeys for a selected origin/destination. It uses the `tui-big-text` crate for the big timer, `jiff` for time-zone aware math, and a separate `sncf` workspace crate for HTTP calls to the SNCF API.

## Features
- Split UI: journeys on the left, HH:MM:SS timer on the right.
- Start/destination live search (SNCF places) with debounced suggestions.
- Persisted config (`config.toml`): start, destination, approach minutes.
- Desktop notification + single system sound at 00:00.

## Quick Start
1) Put your key in `.env`:
```
SNCF_API_KEY=YOUR_KEY_HERE
```
2) Build and run:
```
cargo run
```

## Workspace Layout
- `src/main.rs` – entry + event loop
- `src/app.rs` – app state, config IO, handlers
- `src/ui.rs` – drawing (journeys table, big timer)
- `src/events.rs` – crossterm event wrapper
- `sncf/` – SNCF API helpers (`fetch_places`, `fetch_journeys`)

## Development
- Lint: `cargo clippy` (should be warning‑free)
- Format: `cargo fmt` (check: `cargo fmt -- --check`)
- Build (workspace): `cargo build -p tui-big-text -p sncf`

## Controls
- Arrows: navigate journeys / input list
- Enter: select journey / submit duration
- r: refresh journeys
- q / Esc / Ctrl‑C: quit

## Notes
- Do not commit secrets. `.env` is loaded locally via `dotenvy`.
- Dependencies are pinned to exact versions in Cargo.toml for reproducibility.
