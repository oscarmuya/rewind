use chrono::{Datelike, Local};
use ratatui::{
    style::Color,
    text::{Line, Span},
    widgets::ListItem,
};
use rewind_core::entry::Entry;

pub fn status_parts(entry: &Entry) -> (&'static str, Color) {
    match entry.exit_code {
        Some(0) => ("✓", Color::Green),
        Some(_) => ("✗", Color::Red),
        Option::None => ("?", Color::DarkGray),
    }
}

pub fn date_heading(entry: &Entry) -> String {
    let local = entry.started_at.with_timezone(&Local);
    let date = local.date_naive();
    let today = Local::now().date_naive();

    if date == today {
        "Today".to_string()
    } else if date == today.pred_opt().unwrap_or(today) {
        "Yesterday".to_string()
    } else if date.year() == today.year() {
        local.format("%A, %b %-d").to_string()
    } else {
        local.format("%A, %b %-d, %Y").to_string()
    }
}

pub fn empty_history_item() -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        "No command history yet.",
        ratatui::style::Style::default().fg(Color::DarkGray),
    )))
}
