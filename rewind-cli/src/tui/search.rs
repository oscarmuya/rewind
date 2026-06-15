use super::shared::status_parts;
use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyModifiers};
use nucleo_matcher::{Config, Matcher};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
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
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let search = Paragraph::new(format!("> {}", app.query))
        .block(Block::bordered().title(" rewind "))
        .style(Style::default().fg(Color::White));

    frame.render_widget(search, chunks[0]);

    let items = app
        .filtered
        .iter()
        .map(|&entry_index| search_item(&app.entries[entry_index]))
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(Block::bordered().title(format!(" {} results ", app.filtered.len())))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, chunks[1], &mut app.list_state);

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

fn search_item(entry: &Entry) -> ListItem<'static> {
    let (status, status_color) = status_parts(entry);
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
}
