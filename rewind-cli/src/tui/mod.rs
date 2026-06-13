use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};
use rewind_core::{entry::Entry, query::recent};
use rusqlite::Connection;

const TUI_ENTRY_LIMIT: usize = 10_000;

struct App {
    query: String,
    entries: Vec<Entry>,
    filtered: Vec<usize>, // Indices into entries.
    list_state: ListState,
}

impl App {
    fn new(entries: Vec<Entry>) -> Self {
        let filtered = (0..entries.len()).collect::<Vec<_>>();
        let mut list_state = ListState::default();

        // Select the first row by default when results exist.
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            query: String::new(),
            entries,
            filtered,
            list_state,
        }
    }

    fn refilter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            let query = self.query.to_lowercase();

            // Keep only commands that contain the current query.
            self.filtered = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    entry
                        .command
                        .to_lowercase()
                        .contains(&query)
                        .then_some(index)
                })
                .collect();
        }

        // Reset selection to the top after every query change.
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

pub fn run(conn: &Connection, initial_query: &str) -> Result<()> {
    // Load recent entries upfront
    let entries = recent(conn, TUI_ENTRY_LIMIT)?;

    let mut app = App::new(entries);

    // Start with the query provided by the CLI, if any.
    if !initial_query.is_empty() {
        app.query = initial_query.to_owned();
        app.refilter();
    }

    // ratatui::run handles alternate screen, raw mode, panic hooks, and restoration.
    ratatui::run(|terminal| event_loop(terminal, &mut app))?;

    // Print the selected command after terminal restoration so shell integrations
    // can capture stdout without TUI escape sequences.
    if let Some(entry) = app.selected_entry() {
        println!("{}", entry.command);
    }

    Ok(())
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;

        // Ignore mouse, resize, focus, key release, and key repeat events.
        let event = event::read()?;
        let Some(key) = event.as_key_press_event() else {
            continue;
        };

        match key.code {
            // Exit without returning a selected command.
            KeyCode::Esc => {
                app.clear_selection();
                return Ok(());
            }

            // Confirm current selection.
            KeyCode::Enter => return Ok(()),

            // Navigation.
            KeyCode::Down => app.move_down(),
            KeyCode::Up => app.move_up(),

            // Query editing.
            KeyCode::Backspace => app.pop_query_char(),

            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.clear_selection();
                return Ok(());
            }

            KeyCode::Char('j') if key.modifiers.is_empty() => app.move_down(),
            KeyCode::Char('k') if key.modifiers.is_empty() => app.move_up(),

            // Accept normal printable characters, including shifted symbols.
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
            Constraint::Length(3), // Search bar.
            Constraint::Min(1),    // Results list.
            Constraint::Length(3), // Detail bar.
        ])
        .split(frame.area());

    // Search bar.
    let search = Paragraph::new(format!("> {}", app.query))
        .block(Block::bordered().title(" rewind "))
        .style(Style::default().fg(Color::White));

    frame.render_widget(search, chunks[0]);

    // Results list.
    let items = app
        .filtered
        .iter()
        .map(|&entry_index| {
            let entry = &app.entries[entry_index];

            let (status, status_color) = match entry.exit_code {
                Some(0) => ("✓", Color::Green),
                Some(_) => ("✗", Color::Red),
                Option::None => ("?", Color::DarkGray),
            };

            let branch = entry
                .git_branch
                .as_deref()
                .map(|branch| format!("[{branch}] "))
                .unwrap_or_default();

            ListItem::new(Line::from(vec![
                Span::styled(status, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(branch, Style::default().fg(Color::Cyan)),
                Span::raw(entry.command.clone()),
            ]))
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(Block::bordered().title(format!(" {} results ", app.filtered.len())))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, chunks[1], &mut app.list_state);

    // Detail bar: show cwd and duration for the selected entry.
    let detail_text = app
        .selected_entry()
        .map(|entry| {
            let duration = entry
                .duration_ms
                .map(|duration_ms| format!("  {duration_ms}ms"))
                .unwrap_or_default();

            format!("{}{}", entry.cwd, duration)
        })
        .unwrap_or_default();

    let detail = Paragraph::new(detail_text)
        .block(Block::bordered().title(" context "))
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(detail, chunks[2]);
}
