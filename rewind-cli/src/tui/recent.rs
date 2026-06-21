use std::{
    collections::{HashMap, HashSet, hash_map::Entry as MapEntry},
    io,
};

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
};
use nucleo_matcher::{Config, Matcher};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{List, ListItem, ListState, Paragraph},
};
use ratatui_textarea::TextArea;
use rewind_core::{
    entry::Entry,
    fuzzy,
    query::{self, Filter},
};
use rusqlite::Connection;

use crate::tui::{
    shared::{editor_for_command, render_editor_modal, tui_background},
    themes::init_theme,
};

use super::shared::{
    CommandDisplay, FilterContext, FilterShortcut, FilterToggle, Junction, command_group_item,
    command_item, command_occurrence_item, context_bar, empty_history_item, filter_footer,
    list_block, search_bar, search_footer, selected_item_style, separator_line, toggle_filter,
    top_bar,
};

const TUI_ENTRY_LIMIT: usize = 10_000;

const FILTER_SHORTCUTS: &[FilterShortcut] = &[
    FilterShortcut {
        key: "c",
        label: "cwd",
        toggle: FilterToggle::Cwd,
    },
    FilterShortcut {
        key: "r",
        label: "repo",
        toggle: FilterToggle::Repo,
    },
    FilterShortcut {
        key: "b",
        label: "branch",
        toggle: FilterToggle::Branch,
    },
    FilterShortcut {
        key: "o",
        label: "ok",
        toggle: FilterToggle::Ok,
    },
    FilterShortcut {
        key: "f",
        label: "fail",
        toggle: FilterToggle::Fail,
    },
];

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

#[derive(Debug, Clone)]
enum Row {
    Entry(usize),
    Group {
        entry_index: usize,
        count: usize,
        expanded: bool,
    },
    Occurrence(usize),
}

enum CommandGroup {
    One(usize),
    Many(Vec<usize>),
}

#[derive(Debug)]
struct EditedCommand {
    entry_index: usize,
    command: String,
}

struct App<'a> {
    conn: &'a Connection,
    entries: Vec<Entry>,
    display_entries: Vec<CommandDisplay>,
    visible_entries: Vec<usize>,
    rows: Vec<Row>,
    expanded_groups: HashSet<usize>,
    context: FilterContext,
    filter: Filter,
    list_state: ListState,
    list_area: Rect,
    command_to_run: Option<EditedCommand>,
    edit_input: Option<TextArea<'static>>,
    search_mode: bool,
    search_query: String,
    matcher: Matcher,
}

impl<'a> App<'a> {
    fn new(
        conn: &'a Connection,
        context: FilterContext,
        filter: Filter,
        initial_query: Option<String>,
    ) -> Result<Self> {
        let entries = query::fetch(conn, &filter)?;
        let search_mode = initial_query.is_some();

        let display_entries = entries.iter().map(CommandDisplay::new).collect::<Vec<_>>();

        let mut app = Self {
            conn,
            entries,
            display_entries,
            visible_entries: Vec::new(),
            rows: Vec::new(),
            expanded_groups: HashSet::new(),
            context,
            filter,
            list_state: ListState::default(),
            list_area: Rect::default(),
            command_to_run: None,
            edit_input: None,
            search_mode,
            search_query: initial_query.unwrap_or_default(),
            matcher: Matcher::new(Config::DEFAULT),
        };

        app.refilter();
        Ok(app)
    }

    fn is_editing(&self) -> bool {
        self.edit_input.is_some()
    }

    fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.refilter();
    }

    fn cancel_search_mode(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.refilter();
    }

    fn push_search_char(&mut self, character: char) {
        self.search_query.push(character);
        self.refilter();
    }

    fn pop_search_char(&mut self) {
        if self.search_query.pop().is_some() {
            self.refilter();
        }
    }

    fn refilter(&mut self) {
        self.visible_entries = if self.search_query.is_empty() {
            (0..self.entries.len()).collect()
        } else {
            fuzzy::search_fuzzy_indices(
                &mut self.matcher,
                &self.entries,
                &self.search_query,
                TUI_ENTRY_LIMIT,
            )
        };
        self.rows = grouped_rows(&self.entries, &self.visible_entries, &self.expanded_groups);
        self.select_first_entry();
    }

    fn selected_entry_index(&self) -> Option<usize> {
        self.list_state
            .selected()
            .and_then(|row_index| self.rows.get(row_index))
            .map(|row| match row {
                Row::Entry(entry_index) | Row::Occurrence(entry_index) => *entry_index,
                Row::Group { entry_index, .. } => *entry_index,
            })
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.selected_entry_index()
            .and_then(|entry_index| self.entries.get(entry_index))
    }

    fn select_first_entry(&mut self) {
        if let Some(row) = self.next_entry_row(0) {
            self.list_state.select(Some(row));
        } else {
            self.clear_selection();
        }
    }

    fn clear_selection(&mut self) {
        self.list_state.select(None);
    }

    fn move_down(&mut self) {
        let Some(selected) = self.list_state.selected() else {
            self.select_first_entry();
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

    fn select_clicked_entry(&mut self, mouse: MouseEvent) -> bool {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return false;
        }

        let left = self.list_area.x;
        let right = self.list_area.x.saturating_add(self.list_area.width);
        let top = self.list_area.y;
        let bottom = self.list_area.y.saturating_add(self.list_area.height);

        if mouse.column < left || mouse.column >= right || mouse.row < top || mouse.row >= bottom {
            return false;
        }

        let visible_offset = usize::from(mouse.row - top);
        let row_index = self.list_state.offset().saturating_add(visible_offset);

        if self.rows.get(row_index).is_some() {
            self.list_state.select(Some(row_index));
            return true;
        }

        false
    }

    fn open_editor_for_selected_entry(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };

        self.edit_input = Some(editor_for_command(&entry.command));
    }

    fn toggle_filter(&mut self, toggle: FilterToggle) -> Result<()> {
        let mut filter = self.filter.clone();
        toggle_filter(&mut filter, toggle, &self.context);

        if filter == self.filter {
            return Ok(());
        }

        let entries = query::fetch(self.conn, &filter)?;
        self.display_entries = entries.iter().map(CommandDisplay::new).collect();
        self.entries = entries;
        self.filter = filter;
        self.expanded_groups.clear();
        self.refilter();
        Ok(())
    }

    fn cancel_edit(&mut self) {
        // Close modal and return to the list without changing the command.
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

        // Plain Enter confirms and runs the edited command.
        self.command_to_run = Some(EditedCommand {
            entry_index,
            command: textarea.lines().join("\n"),
        });

        true
    }

    fn next_entry_row(&self, start: usize) -> Option<usize> {
        (start < self.rows.len()).then_some(start)
    }

    fn previous_entry_row(&self, start: usize) -> Option<usize> {
        (!self.rows.is_empty()).then_some(start.min(self.rows.len() - 1))
    }

    fn expand_selected_group(&mut self) {
        let Some(row_index) = self.list_state.selected() else {
            return;
        };
        let Some(Row::Group { entry_index, .. }) = self.rows.get(row_index) else {
            return;
        };
        if self.expanded_groups.insert(*entry_index) {
            self.rows = grouped_rows(&self.entries, &self.visible_entries, &self.expanded_groups);
            self.list_state.select(Some(row_index));
        }
    }

    fn collapse_selected_group(&mut self) {
        let Some(row_index) = self.list_state.selected() else {
            return;
        };
        let group_index = match self.rows.get(row_index) {
            Some(Row::Group { entry_index, .. }) => Some(*entry_index),
            Some(Row::Occurrence(_)) => self.rows[..row_index].iter().rev().find_map(|row| {
                if let Row::Group { entry_index, .. } = row {
                    Some(*entry_index)
                } else {
                    None
                }
            }),
            _ => None,
        };
        let Some(group_index) = group_index else {
            return;
        };
        if self.expanded_groups.remove(&group_index) {
            self.rows = grouped_rows(&self.entries, &self.visible_entries, &self.expanded_groups);
            if let Some(group_row) = self.rows.iter().position(
                |row| matches!(row, Row::Group { entry_index, .. } if *entry_index == group_index),
            ) {
                self.list_state.select(Some(group_row));
            }
        }
    }
}

pub fn run(
    conn: &Connection,
    context: FilterContext,
    filter: Filter,
    initial_query: Option<String>,
) -> Result<Option<Entry>> {
    let _mouse = MouseCaptureGuard::enable()?;
    let mut app = App::new(conn, context, filter, initial_query)?;
    init_theme();

    ratatui::run(|terminal| event_loop(terminal, &mut app))?;

    // If the user confirmed an edited command, return the selected entry
    // with its command replaced by whatever they typed in the modal.
    // If they just exited without confirming, return None.
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

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App<'_>) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if app.is_editing() {
                    // While the edit modal is open, route all keys into it first.
                    if handle_editor_key(app, key) {
                        return Ok(());
                    }
                } else if app.search_mode {
                    if handle_search_key(app, key) {
                        return Ok(());
                    }
                } else if handle_list_key(app, key)? {
                    return Ok(());
                }
            }
            Event::Mouse(mouse) => handle_mouse_event(app, mouse),
            _ => {}
        }
    }
}

fn handle_search_key(app: &mut App<'_>, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => app.cancel_search_mode(),
        KeyCode::Enter => app.open_editor_for_selected_entry(),
        KeyCode::Down => app.move_down(),
        KeyCode::Up => app.move_up(),
        KeyCode::Right => app.expand_selected_group(),
        KeyCode::Left => app.collapse_selected_group(),
        KeyCode::Backspace => app.pop_search_char(),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.clear_selection();
            return true;
        }
        KeyCode::Char(character)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            app.push_search_char(character);
        }
        _ => {}
    }

    false
}

fn handle_editor_key(app: &mut App<'_>, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.cancel_edit();
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            // Alt+Enter inserts a newline for multiline editing.
            if let Some(textarea) = app.edit_input.as_mut() {
                textarea.insert_newline();
            }
        }
        KeyCode::Enter => {
            return app.confirm_edit();
        }
        _ => {
            // tui-textarea handles cursor movement, selection, backspace,
            // delete, home/end, and arrows across lines.
            if let Some(textarea) = app.edit_input.as_mut() {
                textarea.input(key);
            }
        }
    }

    false
}

fn handle_list_key(app: &mut App<'_>, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('/') if key.modifiers.is_empty() => {
            app.enter_search_mode();
            Ok(false)
        }
        KeyCode::Esc => {
            app.clear_selection();
            Ok(true)
        }
        KeyCode::Enter => {
            // Open edit modal pre-populated with the selected command.
            app.open_editor_for_selected_entry();
            Ok(false)
        }
        KeyCode::Down => {
            app.move_down();
            Ok(false)
        }
        KeyCode::Up => {
            app.move_up();
            Ok(false)
        }
        KeyCode::Right => {
            app.expand_selected_group();
            Ok(false)
        }
        KeyCode::Left => {
            app.collapse_selected_group();
            Ok(false)
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.clear_selection();
            Ok(true)
        }
        KeyCode::Char('j') if key.modifiers.is_empty() => {
            app.move_down();
            Ok(false)
        }
        KeyCode::Char('k') if key.modifiers.is_empty() => {
            app.move_up();
            Ok(false)
        }
        KeyCode::Char('l') if key.modifiers.is_empty() => {
            app.expand_selected_group();
            Ok(false)
        }
        KeyCode::Char('h') if key.modifiers.is_empty() => {
            app.collapse_selected_group();
            Ok(false)
        }
        KeyCode::Char('c') if key.modifiers.is_empty() => {
            app.toggle_filter(FilterToggle::Cwd)?;
            Ok(false)
        }
        KeyCode::Char('r') if key.modifiers.is_empty() => {
            app.toggle_filter(FilterToggle::Repo)?;
            Ok(false)
        }
        KeyCode::Char('b') if key.modifiers.is_empty() => {
            app.toggle_filter(FilterToggle::Branch)?;
            Ok(false)
        }
        KeyCode::Char('o') if key.modifiers.is_empty() => {
            app.toggle_filter(FilterToggle::Ok)?;
            Ok(false)
        }
        KeyCode::Char('f') if key.modifiers.is_empty() => {
            app.toggle_filter(FilterToggle::Fail)?;
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn handle_mouse_event(app: &mut App<'_>, mouse: MouseEvent) {
    if app.is_editing() {
        return;
    }

    match mouse.kind {
        MouseEventKind::ScrollDown => app.move_down(),
        MouseEventKind::ScrollUp => app.move_up(),
        MouseEventKind::Down(MouseButton::Left) if app.select_clicked_entry(mouse) => {
            app.open_editor_for_selected_entry();
        }
        _ => {}
    }
}

fn ui(frame: &mut Frame, app: &mut App<'_>) {
    frame.render_widget(tui_background(), frame.area());

    let padded_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1)])
        .horizontal_margin(1)
        .split(frame.area())[0];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // top bar
            Constraint::Length(1), // context
            Constraint::Length(1), // separator line
            Constraint::Min(1),    // command list
            Constraint::Length(3), // filter footer
        ])
        .split(padded_area);

    if app.search_mode {
        render_search_bar(frame, app, chunks[0]);
    } else {
        render_top_bar(frame, chunks[0]);
    }
    render_context_bar(frame, app, chunks[1]);
    render_separator(frame, chunks[2], chunks[1].width, Junction::Top);
    render_history_list(frame, app, chunks[3]);
    if app.search_mode {
        render_search_footer(frame, chunks[4]);
    } else {
        render_filter_footer(frame, app, chunks[4]);
    }

    if let Some(textarea) = app.edit_input.as_mut() {
        render_editor_modal(frame, textarea);
    }
}

fn render_search_bar(frame: &mut Frame, app: &App<'_>, area: Rect) {
    let search = Paragraph::new(search_bar(
        &app.search_query,
        app.visible_entries.len(),
        area.width,
    ));
    frame.render_widget(search, area);
}

fn render_top_bar(frame: &mut Frame, area: Rect) {
    let top = Paragraph::new(top_bar(area.width));
    frame.render_widget(top, area);
}

fn render_context_bar(frame: &mut Frame, app: &App<'_>, area: Rect) {
    let detail = Paragraph::new(context_bar(app.selected_entry())).style(Style::default());
    frame.render_widget(detail, area);
}

fn render_separator(frame: &mut Frame, area: Rect, width: u16, junction: Junction) {
    let separator = Paragraph::new(separator_line(width, junction));
    frame.render_widget(separator, area);
}

fn render_filter_footer(frame: &mut Frame, app: &App<'_>, area: Rect) {
    let footer = filter_footer(area.width, &app.filter, &app.context, FILTER_SHORTCUTS);
    frame.render_widget(footer, area);
}

fn render_search_footer(frame: &mut Frame, area: Rect) {
    frame.render_widget(search_footer(area.width), area);
}

fn render_history_list(frame: &mut Frame, app: &mut App<'_>, area: Rect) {
    app.list_area = area;

    let items = history_items(&app.rows, &app.entries, &app.display_entries);

    let list = List::new(items)
        .block(list_block(None))
        .highlight_style(selected_item_style());

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn history_items<'a>(
    rows: &'a [Row],
    entries: &'a [Entry],
    display_entries: &'a [CommandDisplay],
) -> Vec<ListItem<'a>> {
    if rows.is_empty() {
        return vec![empty_history_item()];
    }

    rows.iter()
        .map(|row| match row {
            Row::Entry(entry_index) => {
                command_item(&entries[*entry_index], &display_entries[*entry_index])
            }
            Row::Group {
                entry_index,
                count,
                expanded,
            } => command_group_item(
                &entries[*entry_index],
                &display_entries[*entry_index],
                *count,
                *expanded,
            ),
            Row::Occurrence(entry_index) => {
                command_occurrence_item(&entries[*entry_index], &display_entries[*entry_index])
            }
        })
        .collect()
}

fn grouped_rows(
    entries: &[Entry],
    visible_entries: &[usize],
    expanded_groups: &HashSet<usize>,
) -> Vec<Row> {
    let mut groups: HashMap<&str, CommandGroup> = HashMap::with_capacity(visible_entries.len());
    for &index in visible_entries {
        let command = entries[index].command.as_str();
        match groups.entry(command) {
            MapEntry::Vacant(entry) => {
                entry.insert(CommandGroup::One(index));
            }
            MapEntry::Occupied(mut entry) => match entry.get_mut() {
                CommandGroup::One(first) => {
                    let first = *first;
                    entry.insert(CommandGroup::Many(vec![first, index]));
                }
                CommandGroup::Many(occurrences) => occurrences.push(index),
            },
        }
    }

    let mut rows = Vec::with_capacity(visible_entries.len());
    for &index in visible_entries {
        let command = entries[index].command.as_str();
        if let Some(group) = groups.remove(command) {
            match group {
                CommandGroup::One(representative) => rows.push(Row::Entry(representative)),
                CommandGroup::Many(occurrences) => {
                    let representative = occurrences[0];
                    let expanded = expanded_groups.contains(&representative);
                    rows.push(Row::Group {
                        entry_index: representative,
                        count: occurrences.len(),
                        expanded,
                    });
                    if expanded {
                        rows.extend(occurrences.iter().copied().map(Row::Occurrence));
                    }
                }
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;
    use rewind_core::db;

    fn connection_with_entries() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                command TEXT NOT NULL,
                cwd TEXT NOT NULL,
                project_cwd TEXT NOT NULL,
                git_repo TEXT,
                git_branch TEXT,
                exit_code INTEGER,
                duration_ms INTEGER,
                started_at TEXT NOT NULL
            );",
        )
        .unwrap();

        db::insert(
            &conn,
            &Entry::new("git status", "/project", "/project", None, None),
        )
        .unwrap();
        db::insert(
            &conn,
            &Entry::new("cargo test", "/project", "/project", None, None),
        )
        .unwrap();
        conn
    }

    fn app(conn: &Connection) -> App<'_> {
        App::new(
            conn,
            FilterContext::default(),
            Filter::new().limit(500),
            None,
        )
        .unwrap()
    }

    #[test]
    fn initial_query_starts_in_search_mode() {
        let conn = connection_with_entries();
        let app = App::new(
            &conn,
            FilterContext::default(),
            Filter::new().limit(500),
            Some(String::new()),
        )
        .unwrap();

        assert!(app.search_mode);
        assert!(app.search_query.is_empty());

        let app = App::new(
            &conn,
            FilterContext::default(),
            Filter::new().limit(500),
            Some("cargo".to_string()),
        )
        .unwrap();

        assert!(app.search_mode);
        assert_eq!(app.search_query, "cargo");
        assert_eq!(app.visible_entries.len(), 1);
    }

    #[test]
    fn slash_enters_search_and_escape_clears_and_cancels() {
        let conn = connection_with_entries();
        let mut app = app(&conn);

        assert!(
            !handle_list_key(
                &mut app,
                KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)
            )
            .unwrap()
        );
        assert!(app.search_mode);

        for character in ['g', 'i', 't'] {
            assert!(!handle_search_key(
                &mut app,
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
            ));
        }
        assert_eq!(app.search_query, "git");
        assert_eq!(app.visible_entries.len(), 1);

        assert!(!handle_search_key(
            &mut app,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ));
        assert!(!app.search_mode);
        assert!(app.search_query.is_empty());
        assert_eq!(app.visible_entries.len(), 2);
    }

    #[test]
    fn enter_opens_the_selected_search_result_in_the_editor() {
        let conn = connection_with_entries();
        let mut app = app(&conn);
        app.enter_search_mode();

        assert!(!handle_search_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ));
        assert!(app.is_editing());
        assert!(app.command_to_run.is_none());
    }

    #[test]
    fn repeated_commands_collapse_in_newest_first_order() {
        let conn = connection_with_entries();
        db::insert(
            &conn,
            &Entry::new("git status", "/other", "/project", None, None),
        )
        .unwrap();
        let app = app(&conn);

        assert_eq!(app.rows.len(), 2);
        assert!(matches!(
            app.rows[0],
            Row::Group {
                count: 2,
                expanded: false,
                ..
            }
        ));
        let Row::Entry(entry_index) = app.rows[1] else {
            panic!("expected the unique command to remain a normal row");
        };
        assert_eq!(app.entries[entry_index].command, "cargo test");
    }

    #[test]
    fn expanding_group_exposes_exact_occurrences_and_left_collapses() {
        let conn = connection_with_entries();
        db::insert(
            &conn,
            &Entry::new("git status", "/other", "/project", None, None),
        )
        .unwrap();
        let mut app = app(&conn);

        app.expand_selected_group();
        assert_eq!(app.rows.len(), 4);
        assert!(matches!(app.rows[1], Row::Occurrence(_)));
        assert!(matches!(app.rows[2], Row::Occurrence(_)));

        app.list_state.select(Some(2));
        assert_eq!(app.selected_entry().unwrap().cwd, "/project");
        app.collapse_selected_group();
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.list_state.selected(), Some(0));
    }
}
