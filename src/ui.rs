use crate::app::App;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table};
use std::time::Duration;
use tui_big_text::BigText;
// no direct Zoned usage here

pub fn draw_input(f: &mut ratatui::Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(centered_rect(80, 70, area));

    let cursor_pos = app.input.cursor.min(app.input.text.len());
    let (left, right) = app.input.text.split_at(cursor_pos);
    let input_line = Line::from(vec![
        Span::raw(left),
        Span::styled("|", Style::default()),
        Span::raw(right),
    ]);
    let title = app.input_title();
    let input =
        Paragraph::new(input_line).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(input, chunks[0]);

    let items: Vec<ListItem> = app.suggestion_items();
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

pub fn draw_timer(f: &mut ratatui::Frame, app: &App) {
    let size = f.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(size);
    if let Some(conf) = &app.config {
        let header = Paragraph::new(Line::from(vec![Span::raw(format!(
            "{} → {} • approach {} min",
            conf.start.name, conf.destination.name, conf.approach_minutes
        ))]))
        .block(Block::default().borders(Borders::ALL).title("Config"));
        f.render_widget(header, rows[0]);
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(rows[1]);
    draw_journeys(f, app, cols[0]);

    let elapsed = app.timer.start.elapsed();
    let remaining = app.remaining_time(elapsed);
    let show = if let Some(z) = app.timer.zero_at {
        ((std::time::Instant::now() - z).as_millis() / 500) % 2 == 0
    } else {
        true
    };
    let time_str = format_hhmmss(remaining);
    // Right panel (timer) with a visible border
    let timer_block = Block::default().borders(Borders::ALL).title("Timer");
    let timer_area = cols[1];
    f.render_widget(timer_block.clone(), timer_area);

    let inner = timer_block.inner(timer_area);
    if show {
        let big = BigText::builder()
            .style(Style::default().fg(Color::Cyan))
            .alignment(ratatui::prelude::Alignment::Center)
            .lines(vec![Line::from(time_str)])
            .build();
        f.render_widget(big, inner);
    } else {
        f.render_widget(Clear, inner);
    }
}

pub fn draw_journeys(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Journeys (r to refresh)");
    if app.journeys_loading {
        let p = Paragraph::new("Loading...").block(block);
        f.render_widget(p, area);
        return;
    }
    if let Some(err) = &app.journeys_error {
        let p = Paragraph::new(format!("Error: {err}")).block(block);
        f.render_widget(p, area);
        return;
    }
    let header = Row::new(vec![
        Cell::from("Date"),
        Cell::from("Dep"),
        Cell::from("Arr"),
        Cell::from("Dur"),
        Cell::from("Changes"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows = app.journeys.iter().map(|j| {
        let dur_min = (j.duration_secs / 60).max(0);
        Row::new(vec![
            Cell::from(j.date_str.clone()),
            Cell::from(j.dep_hm.clone()),
            Cell::from(j.arr_hm.clone()),
            Cell::from(format!("{}m", dur_min)),
            Cell::from(format!("{}", j.nb_transfers)),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(block)
    .highlight_symbol("▶ ");
    let mut state = ratatui::widgets::TableState::default();
    if !app.journeys.is_empty() {
        let sel = app.journeys_selected.min(app.journeys.len() - 1);
        state.select(Some(sel));
    }
    f.render_stateful_widget(table, area, &mut state);
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

pub fn format_hhmmss(dur: Duration) -> String {
    let secs = dur.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

// formatting helpers live in the sncf crate; no extra helpers here
