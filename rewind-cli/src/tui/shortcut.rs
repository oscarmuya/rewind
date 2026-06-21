use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::Span,
    widgets::{List, ListItem, ListState, Paragraph},
};
use ratatui_textarea::TextArea;
use rewind_core::{query_shortcuts, shortcut::Shortcut};
use rusqlite::Connection;

use super::{
    shared::{
        action_footer, editor_for_command, gutter_line, list_block, render_editor_modal,
        selected_item_style, top_bar, tui_background,
    },
    themes::{THEME, init_theme},
};

struct App<'a> {
    conn: &'a Connection,
    shortcuts: Vec<Shortcut>,
    list_state: ListState,
    edit_input: Option<TextArea<'static>>,
    delete_pending: bool,
}

impl<'a> App<'a> {
    fn new(conn: &'a Connection, project_dir: &str) -> Result<Self> {
        let shortcuts = query_shortcuts::for_project(conn, project_dir)?;
        let mut list_state = ListState::default();
        if !shortcuts.is_empty() {
            list_state.select(Some(0));
        }

        Ok(Self {
            conn,
            shortcuts,
            list_state,
            edit_input: None,
            delete_pending: false,
        })
    }

    fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }

    fn selected(&self) -> Option<&Shortcut> {
        self.selected_index()
            .and_then(|index| self.shortcuts.get(index))
    }

    fn move_down(&mut self) {
        self.delete_pending = false;
        if self.shortcuts.is_empty() {
            return;
        }
        let next = self.selected_index().unwrap_or(0).saturating_add(1);
        self.list_state
            .select(Some(next.min(self.shortcuts.len() - 1)));
    }

    fn move_up(&mut self) {
        self.delete_pending = false;
        if self.shortcuts.is_empty() {
            return;
        }
        let previous = self.selected_index().unwrap_or(0).saturating_sub(1);
        self.list_state.select(Some(previous));
    }

    fn open_editor(&mut self) {
        if let Some(shortcut) = self.selected() {
            self.edit_input = Some(editor_for_command(&shortcut.command));
        }
    }

    fn save_edit(&mut self) -> Result<()> {
        let Some(index) = self.selected_index() else {
            self.edit_input = None;
            return Ok(());
        };
        let Some(input) = self.edit_input.as_ref() else {
            return Ok(());
        };
        let command = input.lines().join("\n");
        if command.trim().is_empty() {
            return Ok(());
        }

        let shortcut = &mut self.shortcuts[index];
        query_shortcuts::update_command(self.conn, shortcut.id, &command)?;
        shortcut.command = command;
        self.edit_input = None;
        Ok(())
    }

    fn delete_selected(&mut self) -> Result<()> {
        let Some(index) = self.selected_index() else {
            return Ok(());
        };
        let shortcut = self.shortcuts.remove(index);
        query_shortcuts::delete(self.conn, shortcut.id)?;
        self.delete_pending = false;

        if self.shortcuts.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state
                .select(Some(index.min(self.shortcuts.len() - 1)));
        }
        Ok(())
    }
}

pub fn run(conn: &Connection, project_dir: &str) -> Result<()> {
    init_theme();
    let mut app = App::new(conn, project_dir)?;
    ratatui::run(|terminal| event_loop(terminal, &mut app))?;
    Ok(())
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App<'_>) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            if app.edit_input.is_some() {
                handle_editor_key(app, key)?;
            } else if handle_list_key(app, key)? {
                return Ok(());
            }
        }
    }
}

fn handle_editor_key(app: &mut App<'_>, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => app.edit_input = None,
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            if let Some(input) = app.edit_input.as_mut() {
                input.insert_newline();
            }
        }
        KeyCode::Enter => app.save_edit()?,
        _ => {
            if let Some(input) = app.edit_input.as_mut() {
                input.input(key);
            }
        }
    }
    Ok(())
}

fn handle_list_key(app: &mut App<'_>, key: KeyEvent) -> Result<bool> {
    if key.code == KeyCode::Char('d') && key.modifiers.is_empty() {
        if app.delete_pending {
            app.delete_selected()?;
        } else {
            app.delete_pending = true;
        }
        return Ok(false);
    }

    app.delete_pending = false;
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
        KeyCode::Enter | KeyCode::Char('e') => app.open_editor(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        _ => {}
    }
    Ok(false)
}

fn ui(frame: &mut Frame, app: &mut App<'_>) {
    frame.render_widget(tui_background(), frame.area());
    let padded = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1)])
        .horizontal_margin(1)
        .split(frame.area())[0];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(padded);

    frame.render_widget(
        Paragraph::new(top_bar(chunks[0].width, "saved command shortcuts")),
        chunks[0],
    );
    render_list(frame, app, chunks[1]);
    frame.render_widget(
        action_footer(
            chunks[2].width,
            "Shortcuts",
            &[
                ("↑/↓", "navigate"),
                ("enter", "edit"),
                ("dd", "delete"),
                ("q", "quit"),
            ],
        ),
        chunks[2],
    );

    if let Some(input) = app.edit_input.as_mut() {
        render_editor_modal(frame, input, "save");
    }
}

fn render_list(frame: &mut Frame, app: &mut App<'_>, area: Rect) {
    let alias_width = app
        .shortcuts
        .iter()
        .map(|shortcut| shortcut.alias.len())
        .max()
        .unwrap_or(0);
    let items = app
        .shortcuts
        .iter()
        .map(|shortcut| {
            let scope = if shortcut.global() {
                "(glob)"
            } else {
                "(proj)"
            };
            ListItem::new(gutter_line(
                scope,
                vec![
                    Span::styled(
                        format!("{:<alias_width$}", shortcut.alias),
                        Style::default().fg(THEME.heading),
                    ),
                    Span::styled(
                        format!("  {}", shortcut.command),
                        Style::default().fg(THEME.text),
                    ),
                ],
            ))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(list_block(None))
        .highlight_style(selected_item_style());
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rewind_core::{db, shortcut::Shortcut};

    fn connection_with_shortcut() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE shortcuts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                alias TEXT NOT NULL,
                command TEXT NOT NULL,
                project_dir TEXT NOT NULL,
                git_repo TEXT,
                is_global BOOLEAN DEFAULT FALSE,
                created_at DATETIME NOT NULL,
                UNIQUE(alias, project_dir)
            );",
        )
        .unwrap();
        db::insert_shortcut(
            &conn,
            &Shortcut::new("test", "cargo test", "/project", None, false),
        )
        .unwrap();
        conn
    }

    fn app(conn: &Connection) -> App<'_> {
        App::new(conn, "/project").unwrap()
    }

    #[test]
    fn dd_deletes_selected_shortcut() {
        let conn = connection_with_shortcut();
        let mut app = app(&conn);
        let d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        assert!(!handle_list_key(&mut app, d).unwrap());
        assert!(!handle_list_key(&mut app, d).unwrap());
        assert!(app.shortcuts.is_empty());
    }

    #[test]
    fn editor_saves_command() {
        let conn = connection_with_shortcut();
        let mut app = app(&conn);
        app.edit_input = Some(editor_for_command("cargo test --workspace"));
        app.save_edit().unwrap();
        assert_eq!(app.shortcuts[0].command, "cargo test --workspace");
    }
}
