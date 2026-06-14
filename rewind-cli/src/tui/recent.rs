use anyhow::Result;
use chrono::Local;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};
use rewind_core::entry::Entry;
use std::io;

use super::shared::{date_heading, empty_history_item, status_parts};

struct MouseCaptureGuard;

impl MouseCaptureGuard {
    fn enable() -> Result<Self> {
        execute!(io::stdout(), EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for MouseCaptureGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
}

#[derive(Clone)]
enum Row {
    Header(String),
    Entry(usize),
}

struct App {
    entries: Vec<Entry>,
    rows: Vec<Row>,
    list_state: ListState,
    list_area: Rect,
}

impl App {
    fn new(entries: Vec<Entry>) -> Self {
        let rows = grouped_rows(&entries);
        let mut app = Self {
            entries,
            rows,
            list_state: ListState::default(),
            list_area: Rect::default(),
        };

        if let Some(first_entry) = app.next_entry_row(0) {
            app.list_state.select(Some(first_entry));
        }

        app
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.list_state
            .selected()
            .and_then(|row| self.rows.get(row))
            .and_then(|row| match row {
                Row::Header(_) => None,
                Row::Entry(entry_index) => self.entries.get(*entry_index),
            })
    }

    fn clear_selection(&mut self) {
        self.list_state.select(None);
    }

    fn move_down(&mut self) {
        let Some(selected) = self.list_state.selected() else {
            if let Some(row) = self.next_entry_row(0) {
                self.list_state.select(Some(row));
            }
            return;
        };

        if let Some(row) = self.next_entry_row(selected.saturating_add(1)) {
            self.list_state.select(Some(row));
        }
    }

    fn move_up(&mut self) {
        let Some(selected) = self.list_state.selected() else {
            if let Some(row) = self.previous_entry_row(self.rows.len().saturating_sub(1)) {
                self.list_state.select(Some(row));
            }
            return;
        };

        if selected == 0 {
            return;
        }

        if let Some(row) = self.previous_entry_row(selected - 1) {
            self.list_state.select(Some(row));
        }
    }

    fn select_clicked(&mut self, mouse: MouseEvent) -> bool {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return false;
        }

        let top = self.list_area.y.saturating_add(1);
        let bottom = self
            .list_area
            .y
            .saturating_add(self.list_area.height.saturating_sub(1));

        if mouse.row < top || mouse.row >= bottom {
            return false;
        }

        let visible_offset = usize::from(mouse.row - top);
        let row = self.list_state.offset().saturating_add(visible_offset);

        if matches!(self.rows.get(row), Some(Row::Entry(_))) {
            self.list_state.select(Some(row));
            return true;
        }

        false
    }

    fn next_entry_row(&self, start: usize) -> Option<usize> {
        self.rows
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(row, item)| matches!(item, Row::Entry(_)).then_some(row))
    }

    fn previous_entry_row(&self, start: usize) -> Option<usize> {
        self.rows
            .iter()
            .enumerate()
            .take(start.saturating_add(1))
            .rev()
            .find_map(|(row, item)| matches!(item, Row::Entry(_)).then_some(row))
    }
}

pub fn run(entries: Vec<Entry>) -> Result<Option<Entry>> {
    let _mouse = MouseCaptureGuard::enable()?;
    let mut app = App::new(entries);

    ratatui::run(|terminal| event_loop(terminal, &mut app))?;

    Ok(app.selected_entry().cloned())
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;

        match event::read()? {
            Event::Key(key) if key.kind == event::KeyEventKind::Press => match key.code {
                KeyCode::Esc => {
                    app.clear_selection();
                    return Ok(());
                }
                KeyCode::Enter => return Ok(()),
                KeyCode::Down => app.move_down(),
                KeyCode::Up => app.move_up(),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.clear_selection();
                    return Ok(());
                }
                KeyCode::Char('j') if key.modifiers.is_empty() => app.move_down(),
                KeyCode::Char('k') if key.modifiers.is_empty() => app.move_up(),
                _ => {}
            },
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => app.move_down(),
                MouseEventKind::ScrollUp => app.move_up(),
                MouseEventKind::Down(MouseButton::Left) if app.select_clicked(mouse) => {
                    return Ok(());
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn ui(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    app.list_area = chunks[0];

    let items = if app.rows.is_empty() {
        vec![empty_history_item()]
    } else {
        app.rows
            .iter()
            .map(|row| match row {
                Row::Header(heading) => ListItem::new(Line::from(Span::styled(
                    heading.clone(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))),
                Row::Entry(entry_index) => recent_item(&app.entries[*entry_index]),
            })
            .collect::<Vec<_>>()
    };

    let list = List::new(items)
        .block(Block::bordered().title(" rewind "))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, chunks[0], &mut app.list_state);

    let detail_text = app
        .selected_entry()
        .map(|entry| format!("Enter or click to rerun from {}", entry.cwd))
        .unwrap_or_else(|| "Esc cancels".to_string());

    let detail = Paragraph::new(detail_text)
        .block(Block::bordered().title(" context "))
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(detail, chunks[1]);
}

fn recent_item(entry: &Entry) -> ListItem<'static> {
    let (status, status_color) = status_parts(entry);
    let time = entry.started_at.with_timezone(&Local).format("%H:%M");
    let branch = entry
        .git_branch
        .as_deref()
        .map(|branch| format!(" [{branch}]"))
        .unwrap_or_default();

    ListItem::new(Line::from(vec![
        Span::styled(format!("  {time} "), Style::default().fg(Color::DarkGray)),
        Span::styled(status, Style::default().fg(status_color)),
        Span::styled(branch, Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::raw(entry.command.clone()),
    ]))
}

fn grouped_rows(entries: &[Entry]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut last_heading = String::new();

    for (index, entry) in entries.iter().enumerate() {
        let heading = date_heading(entry);
        if heading != last_heading {
            rows.push(Row::Header(heading.clone()));
            last_heading = heading;
        }

        rows.push(Row::Entry(index));
    }

    rows
}
