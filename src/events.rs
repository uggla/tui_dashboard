use crossterm::event::{self, Event, KeyEvent};
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
}

pub fn poll_event(timeout: Duration) -> Option<AppEvent> {
    if event::poll(timeout).ok()? {
        match event::read().ok()? {
            Event::Key(k) => Some(AppEvent::Key(k)),
            _ => None,
        }
    } else {
        None
    }
}
