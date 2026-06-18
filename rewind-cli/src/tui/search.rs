use anyhow::Result;
use chrono::Local;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use nucleo_matcher::{Config, Matcher};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{List, ListState, Paragraph},
};
use ratatui_textarea::TextArea;
use rewind_core::{entry::Entry, fuzzy, query::recent};
use rusqlite::Connection;

use super::shared::{
    CommandDisplay, Junction, command_item, context_bar, search_bar, selected_item_style,
    separator_line,
};
use crate::tui::shared::{editor_for_command, render_editor_modal};

const TUI_ENTRY_LIMIT: usize = 10_000;

#[derive(Debug)]
struct EditedCommand {
    entry_index: usize,
    command: String,
}

struct App {
    query: String,
    entries: Vec<Entry>,
    display_entries: Vec<CommandDisplay>,
    filtered: Vec<usize>, // Indices into entries.
    list_state: ListState,
    matcher: Matcher,
    command_to_run: Option<EditedCommand>,
    edit_input: Option<TextArea<'static>>,
}

impl App {
    fn new(entries: Vec<Entry>, initial_query: &str) -> Self {
        let today = Local::now().date_naive();

        let display_entries = entries
            .iter()
            .map(|entry| CommandDisplay::new(entry, today))
            .collect();

        let mut app = Self {
            query: initial_query.to_owned(),
            entries,
            display_entries,
            filtered: Vec::new(),
            list_state: ListState::default(),
            matcher: Matcher::new(Config::DEFAULT),
            command_to_run: None,
            edit_input: None,
        };

        app.refilter();
        app
    }

    fn is_editing(&self) -> bool {
        self.edit_input.is_some()
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

        self.select_first_result();
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.selected_entry_index()
            .and_then(|entry_index| self.entries.get(entry_index))
    }

    fn selected_entry_index(&self) -> Option<usize> {
        self.list_state
            .selected()
            .and_then(|selected| self.filtered.get(selected))
            .copied()
    }

    fn select_first_result(&mut self) {
        self.list_state
            .select((!self.filtered.is_empty()).then_some(0));
    }

    fn clear_selection(&mut self) {
        self.list_state.select(None);
    }

    fn move_down(&mut self) {
        if self.filtered.is_empty() {
            return;
        }

        let selected = self.list_state.selected().unwrap_or_default();
        let last = self.filtered.len().saturating_sub(1);

        self.list_state
            .select(Some(selected.saturating_add(1).min(last)));
    }

    fn move_up(&mut self) {
        if self.filtered.is_empty() {
            return;
        }

        let selected = self.list_state.selected().unwrap_or_default();
        self.list_state.select(Some(selected.saturating_sub(1)));
    }

    fn push_query_char(&mut self, character: char) {
        self.query.push(character);
        self.refilter();
    }

    fn pop_query_char(&mut self) {
        if self.query.pop().is_some() {
            self.refilter();
        }
    }

    fn open_editor_for_selected_entry(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };

        self.edit_input = Some(editor_for_command(&entry.command));
    }

    fn cancel_edit(&mut self) {
        self.edit_input = None;
    }

    fn confirm_edit(&mut self) -> bool {
        let Some(entry_index) = self.selected_entry_index() else {
            self.edit_input = None;
            return false;
        };

        let Some(textarea) = self.edit_input.take() else {
            return false;
        };

        self.command_to_run = Some(EditedCommand {
            entry_index,
            command: textarea.lines().join("\n"),
        });

        true
    }
}

pub fn run(
    conn: &Connection,
    project_root_str: &str,
    initial_query: &str,
) -> Result<Option<Entry>> {
    let entries = recent(conn, project_root_str, TUI_ENTRY_LIMIT)?;
    let mut app = App::new(entries, initial_query);

    ratatui::run(|terminal| event_loop(terminal, &mut app))?;

    let Some(edited) = app.command_to_run.take() else {
        return Ok(None);
    };

    let entry = app
        .entries
        .get(edited.entry_index)
        .cloned()
        .map(|mut entry| {
            entry.command = edited.command;
            entry
        });

    Ok(entry)
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if app.is_editing() {
                    if handle_editor_key(app, key) {
                        return Ok(());
                    }
                } else if handle_search_key(app, key) {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
}

fn handle_editor_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.cancel_edit();
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            if let Some(textarea) = app.edit_input.as_mut() {
                textarea.insert_newline();
            }
        }
        KeyCode::Enter => {
            return app.confirm_edit();
        }
        _ => {
            if let Some(textarea) = app.edit_input.as_mut() {
                textarea.input(key);
            }
        }
    }

    false
}

fn handle_search_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.clear_selection();
            true
        }
        KeyCode::Enter => {
            app.open_editor_for_selected_entry();
            false
        }
        KeyCode::Down => {
            app.move_down();
            false
        }
        KeyCode::Up => {
            app.move_up();
            false
        }
        KeyCode::Backspace => {
            app.pop_query_char();
            false
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.clear_selection();
            true
        }
        KeyCode::Char('j') if key.modifiers.is_empty() => {
            app.move_down();
            false
        }
        KeyCode::Char('k') if key.modifiers.is_empty() => {
            app.move_up();
            false
        }
        KeyCode::Char(character) if is_plain_text_input(key.modifiers) => {
            app.push_query_char(character);
            false
        }
        _ => false,
    }
}

fn is_plain_text_input(modifiers: KeyModifiers) -> bool {
    !modifiers.contains(KeyModifiers::CONTROL) && !modifiers.contains(KeyModifiers::ALT)
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

    render_search(frame, app, chunks[0]);
    render_context(frame, app, chunks[1]);
    render_separator(frame, chunks[2], chunks[1].width, Junction::Top);
    render_entry_list(frame, app, chunks[3]);
    render_separator(frame, chunks[4], chunks[1].width, Junction::Bottom);

    if let Some(textarea) = app.edit_input.as_mut() {
        render_editor_modal(frame, textarea);
    }
}

fn render_search(frame: &mut Frame, app: &App, area: Rect) {
    let search = Paragraph::new(search_bar(&app.query, app.filtered.len(), area.width));
    frame.render_widget(search, area);
}

fn render_context(frame: &mut Frame, app: &App, area: Rect) {
    let detail = Paragraph::new(context_bar(app.selected_entry())).style(Style::default());
    frame.render_widget(detail, area);
}

fn render_separator(frame: &mut Frame, area: Rect, width: u16, junction: Junction) {
    let separator = Paragraph::new(separator_line(width, junction));
    frame.render_widget(separator, area);
}

fn render_entry_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let items = app
        .filtered
        .iter()
        .map(|&entry_index| {
            command_item(&app.entries[entry_index], &app.display_entries[entry_index])
        })
        .collect::<Vec<_>>();

    let list = List::new(items).highlight_style(selected_item_style());

    frame.render_stateful_widget(list, area, &mut app.list_state);
}
