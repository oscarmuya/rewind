use crate::tui::shared::{Junction, search_bar, separator_line};

use super::shared::{command_item, context_bar, selected_item_style};
use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyModifiers};
use nucleo_matcher::{Config, Matcher};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::{List, ListState, Paragraph},
};
use rewind_core::{entry::Entry, fuzzy, query::recent};
use rusqlite::Connection;

const TUI_ENTRY_LIMIT: usize = 10_000;

struct App {
    query: String,
    entries: Vec<Entry>,
    filtered: Vec<usize>, // Indices into entries.
    list_state: ListState,
    matcher: Matcher,
}

impl App {
    fn new(entries: Vec<Entry>) -> Self {
        let matcher = Matcher::new(Config::DEFAULT);
        let filtered = (0..entries.len()).collect::<Vec<_>>();
        let mut list_state = ListState::default();

        if !filtered.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            query: String::new(),
            entries,
            filtered,
            list_state,
            matcher,
        }
    }

    fn refilter(&mut self) {
        self.filtered = if self.query.is_empty() {
            (0..self.entries.len()).collect()
        } else {
            fuzzy::search_fuzzy_indices(
                &mut self.matcher,
                &self.entries,
                &self.query,
                TUI_ENTRY_LIMIT,
            )
        };

        self.list_state
            .select((!self.filtered.is_empty()).then_some(0));
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.list_state
            .selected()
            .and_then(|selected| self.filtered.get(selected))
            .and_then(|&entry_index| self.entries.get(entry_index))
    }

    fn move_down(&mut self) {
        if self.filtered.is_empty() {
            return;
        }

        let selected = self.list_state.selected().unwrap_or(0);
        let last = self.filtered.len() - 1;

        self.list_state.select(Some((selected + 1).min(last)));
    }

    fn move_up(&mut self) {
        if self.filtered.is_empty() {
            return;
        }

        let selected = self.list_state.selected().unwrap_or(0);

        self.list_state.select(Some(selected.saturating_sub(1)));
    }

    fn push_query_char(&mut self, c: char) {
        self.query.push(c);
        self.refilter();
    }

    fn pop_query_char(&mut self) {
        self.query.pop();
        self.refilter();
    }

    fn clear_selection(&mut self) {
        self.list_state.select(None);
    }
}

pub fn run(
    conn: &Connection,
    project_root_str: &str,
    initial_query: &str,
) -> Result<Option<Entry>> {
    let entries = recent(conn, project_root_str, TUI_ENTRY_LIMIT)?;
    let mut app = App::new(entries);

    if !initial_query.is_empty() {
        app.query = initial_query.to_owned();
        app.refilter();
    }

    ratatui::run(|terminal| event_loop(terminal, &mut app))?;

    Ok(app.selected_entry().cloned())
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;

        let event = event::read()?;
        let Some(key) = event.as_key_press_event() else {
            continue;
        };

        match key.code {
            KeyCode::Esc => {
                app.clear_selection();
                return Ok(());
            }
            KeyCode::Enter => return Ok(()),
            KeyCode::Down => app.move_down(),
            KeyCode::Up => app.move_up(),
            KeyCode::Backspace => app.pop_query_char(),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.clear_selection();
                return Ok(());
            }
            KeyCode::Char('j') if key.modifiers.is_empty() => app.move_down(),
            KeyCode::Char('k') if key.modifiers.is_empty() => app.move_up(),
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                app.push_query_char(c);
            }
            _ => {}
        }
    }
}

fn ui(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search
            Constraint::Length(1), // context
            Constraint::Length(1), // separator line
            Constraint::Min(1),    // list
            Constraint::Length(1), // blank space
        ])
        .split(frame.area());

    let search = Paragraph::new(search_bar(&app.query, app.filtered.len(), chunks[0].width));
    frame.render_widget(search, chunks[0]);

    let detail = Paragraph::new(context_bar(app.selected_entry())).style(Style::default());

    frame.render_widget(detail, chunks[1]);

    let items = app
        .filtered
        .iter()
        .map(|&entry_index| command_item(&app.entries[entry_index]))
        .collect::<Vec<_>>();

    let list = List::new(items).highlight_style(selected_item_style());

    let sep = Paragraph::new(separator_line(chunks[1].width, Junction::Top));
    frame.render_widget(sep, chunks[2]);

    frame.render_stateful_widget(list, chunks[3], &mut app.list_state);

    let sep = Paragraph::new(separator_line(chunks[1].width, Junction::Bottom));
    frame.render_widget(sep, chunks[4]);
}
