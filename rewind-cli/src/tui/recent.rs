use std::io;

use anyhow::Result;
use chrono::Local;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{List, ListItem, ListState, Paragraph},
};
use ratatui_textarea::TextArea;
use rewind_core::entry::Entry;

use crate::tui::shared::{editor_for_command, render_editor_modal};

use super::shared::{
    CommandDisplay, Junction, command_item, context_bar, date_heading_item, empty_history_item,
    list_block, selected_item_style, separator_line, top_bar,
};

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
    Header(String),
    Entry(usize),
}

#[derive(Debug)]
struct EditedCommand {
    entry_index: usize,
    command: String,
}

struct App {
    entries: Vec<Entry>,
    display_entries: Vec<CommandDisplay>,
    rows: Vec<Row>,
    list_state: ListState,
    list_area: Rect,
    command_to_run: Option<EditedCommand>,
    edit_input: Option<TextArea<'static>>,
}

impl App {
    fn new(entries: Vec<Entry>) -> Self {
        let today = Local::now().date_naive();

        let display_entries = entries
            .iter()
            .map(|entry| CommandDisplay::new(entry, today))
            .collect::<Vec<_>>();

        let rows = grouped_rows(&display_entries);

        let mut app = Self {
            entries,
            display_entries,
            rows,
            list_state: ListState::default(),
            list_area: Rect::default(),
            command_to_run: None,
            edit_input: None,
        };

        app.select_first_entry();
        app
    }

    fn is_editing(&self) -> bool {
        self.edit_input.is_some()
    }

    fn selected_entry_index(&self) -> Option<usize> {
        self.list_state
            .selected()
            .and_then(|row_index| self.rows.get(row_index))
            .and_then(|row| match row {
                Row::Header(_) => None,
                Row::Entry(entry_index) => Some(*entry_index),
            })
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.selected_entry_index()
            .and_then(|entry_index| self.entries.get(entry_index))
    }

    fn select_first_entry(&mut self) {
        if let Some(row) = self.next_entry_row(0) {
            self.list_state.select(Some(row));
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

        let top = self.list_area.y;
        let bottom = self.list_area.y.saturating_add(self.list_area.height);

        if mouse.row < top || mouse.row >= bottom {
            return false;
        }

        let visible_offset = usize::from(mouse.row - top);
        let row_index = self.list_state.offset().saturating_add(visible_offset);

        if matches!(self.rows.get(row_index), Some(Row::Entry(_))) {
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
        self.rows
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(row_index, row)| matches!(row, Row::Entry(_)).then_some(row_index))
    }

    fn previous_entry_row(&self, start: usize) -> Option<usize> {
        self.rows
            .iter()
            .enumerate()
            .take(start.saturating_add(1))
            .rev()
            .find_map(|(row_index, row)| matches!(row, Row::Entry(_)).then_some(row_index))
    }
}

pub fn run(entries: Vec<Entry>) -> Result<Option<Entry>> {
    let _mouse = MouseCaptureGuard::enable()?;
    let mut app = App::new(entries);

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

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if app.is_editing() {
                    // While the edit modal is open, route all keys into it first.
                    if handle_editor_key(app, key) {
                        return Ok(());
                    }
                } else if handle_list_key(app, key) {
                    // Normal list navigation when the modal is closed.
                    return Ok(());
                }
            }
            Event::Mouse(mouse) => handle_mouse_event(app, mouse),
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

fn handle_list_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.clear_selection();
            true
        }
        KeyCode::Enter => {
            // Open edit modal pre-populated with the selected command.
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
        _ => false,
    }
}

fn handle_mouse_event(app: &mut App, mouse: MouseEvent) {
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

fn ui(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // top bar
            Constraint::Length(1), // context
            Constraint::Length(1), // separator line
            Constraint::Min(1),    // command list
            Constraint::Length(1), // bottom separator / blank space
        ])
        .split(frame.area());

    render_top_bar(frame, chunks[0]);
    render_context_bar(frame, app, chunks[1]);
    render_separator(frame, chunks[2], chunks[1].width, Junction::Top);
    render_history_list(frame, app, chunks[3]);
    render_separator(frame, chunks[4], chunks[1].width, Junction::Bottom);

    if let Some(textarea) = app.edit_input.as_mut() {
        render_editor_modal(frame, textarea);
    }
}

fn render_top_bar(frame: &mut Frame, area: Rect) {
    let top = Paragraph::new(top_bar(area.width));
    frame.render_widget(top, area);
}

fn render_context_bar(frame: &mut Frame, app: &App, area: Rect) {
    let detail = Paragraph::new(context_bar(app.selected_entry())).style(Style::default());
    frame.render_widget(detail, area);
}

fn render_separator(frame: &mut Frame, area: Rect, width: u16, junction: Junction) {
    let separator = Paragraph::new(separator_line(width, junction));
    frame.render_widget(separator, area);
}

fn render_history_list(frame: &mut Frame, app: &mut App, area: Rect) {
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
            Row::Header(heading) => date_heading_item(heading),
            Row::Entry(entry_index) => {
                command_item(&entries[*entry_index], &display_entries[*entry_index])
            }
        })
        .collect()
}

fn grouped_rows(display_entries: &[CommandDisplay]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(display_entries.len().saturating_mul(2));
    let mut last_heading = None;

    for (index, display) in display_entries.iter().enumerate() {
        let heading = display.heading.as_str();

        if last_heading != Some(heading) {
            rows.push(Row::Header(display.heading.clone()));
            last_heading = Some(heading);
        }

        rows.push(Row::Entry(index));
    }

    rows
}
