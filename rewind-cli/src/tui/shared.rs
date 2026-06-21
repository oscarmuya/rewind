use std::sync::OnceLock;

use chrono::{DateTime, Datelike, Local, NaiveDate};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, ListItem, Padding, Paragraph},
};
use ratatui_textarea::{CursorMove, TextArea};
use rewind_core::{entry::Entry, query::Filter};

use crate::tui::themes::THEME;

const GUTTER_WIDTH: usize = 6;
const EDIT_MODAL_WIDTH_PERCENT: u16 = 70;
const EDIT_MODAL_HEIGHT: u16 = 8;

static HOME: OnceLock<Option<String>> = OnceLock::new();

pub enum Junction {
    Top, // ┬
}

pub struct CommandDisplay {
    time: String,
    status: &'static str,
    status_color: Color,
    branch: String,
}

impl CommandDisplay {
    pub fn new(entry: &Entry) -> Self {
        let local = entry.started_at.with_timezone(&Local);
        let (status, status_color) = status_parts(entry);

        Self {
            time: local.format("%H:%M").to_string(),
            status,
            status_color,
            branch: format_branch_label(entry.git_branch.as_deref()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FilterContext {
    pub cwd: String,
    pub git_repo: Option<String>,
    pub git_branch: Option<String>,
}

impl FilterContext {
    pub fn new(
        cwd: impl Into<String>,
        git_repo: Option<String>,
        git_branch: Option<String>,
    ) -> Self {
        Self {
            cwd: cwd.into(),
            git_repo,
            git_branch,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FilterToggle {
    Cwd,
    Repo,
    Branch,
    Ok,
    Fail,
    Deleted,
}

pub fn toggle_filter(filter: &mut Filter, toggle: FilterToggle, context: &FilterContext) {
    match toggle {
        FilterToggle::Cwd => {
            filter.cwd = filter.cwd.is_none().then(|| context.cwd.clone());
        }
        FilterToggle::Repo if context.git_repo.is_some() => {
            filter.git_repo = match filter.git_repo {
                Some(_) => None,
                None => context.git_repo.clone(),
            };
        }
        FilterToggle::Branch if context.git_branch.is_some() => {
            filter.git_branch = match filter.git_branch {
                Some(_) => None,
                None => context.git_branch.clone(),
            };
        }
        FilterToggle::Repo | FilterToggle::Branch => {}
        FilterToggle::Ok => {
            filter.only_success = !filter.only_success;
            if filter.only_success {
                filter.only_failure = false;
            }
        }
        FilterToggle::Fail => {
            filter.only_failure = !filter.only_failure;
            if filter.only_failure {
                filter.only_success = false;
            }
        }
        FilterToggle::Deleted => {
            filter.only_deleted = !filter.only_deleted;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FilterShortcut {
    pub key: &'static str,
    pub label: &'static str,
    pub toggle: FilterToggle,
}

pub fn selected_item_style() -> Style {
    Style::default()
        .bg(THEME.selected_item_bg)
        .add_modifier(Modifier::BOLD)
}

pub fn list_block<'a>(title: impl Into<Option<String>>) -> Block<'a> {
    match title.into() {
        Some(title) => Block::default().title(title),
        None => Block::default(),
    }
}

pub fn empty_history_item() -> ListItem<'static> {
    ListItem::new(gutter_line(
        "",
        vec![Span::styled(
            "No command history yet.",
            Style::default().fg(THEME.subtle),
        )],
    ))
}

pub fn command_item<'a>(entry: &'a Entry, display: &'a CommandDisplay) -> ListItem<'a> {
    command_item_with_prefix(entry, display, "", None)
}

pub fn command_group_item<'a>(
    entry: &'a Entry,
    display: &'a CommandDisplay,
    count: usize,
    expanded: bool,
) -> ListItem<'a> {
    let marker = if expanded { "⌄ " } else { "› " };
    command_item_with_prefix(entry, display, marker, Some(format!("  {count} runs")))
}

pub fn command_occurrence_item<'a>(entry: &'a Entry, display: &'a CommandDisplay) -> ListItem<'a> {
    command_item_with_prefix(entry, display, "  └ ", None)
}

fn command_item_with_prefix<'a>(
    entry: &'a Entry,
    display: &'a CommandDisplay,
    prefix: &'static str,
    suffix: Option<String>,
) -> ListItem<'a> {
    let mut spans = vec![
        Span::styled(display.status, Style::default().fg(display.status_color)),
        Span::styled(display.branch.as_str(), Style::default().fg(THEME.branch)),
        Span::raw("  "),
        Span::raw(prefix),
        Span::styled(entry.command.as_str(), Style::default().fg(THEME.text)),
    ];
    if let Some(suffix) = suffix {
        spans.push(Span::styled(suffix, Style::default().fg(THEME.subtle)));
    }

    ListItem::new(gutter_line(&display.time, spans))
}

pub fn context_bar(entry: Option<&Entry>) -> Line<'static> {
    let Some(entry) = entry else {
        return Line::from(Span::styled(
            "no selection",
            Style::default().fg(THEME.subtle),
        ));
    };

    let mut spans = Vec::new();
    let separator = Span::styled(" · ", Style::default().fg(THEME.subtle));

    push_exit_status(&mut spans, entry);

    if let Some(ms) = entry.duration_ms {
        spans.push(separator.clone());
        spans.push(Span::styled(
            format_duration(ms),
            Style::default().fg(THEME.muted),
        ));
    }

    // Environment variable lookups are relatively expensive and unnecessary to repeat
    // on every frame since the home directory does not change during the TUI session,
    // so we fetch and cache it once.
    spans.push(separator.clone());
    spans.push(Span::styled(
        shorten_home_path(&entry.cwd),
        Style::default().fg(THEME.muted),
    ));

    if let Some(branch) = &entry.git_branch {
        spans.push(separator.clone());
        spans.push(Span::styled(
            format!(" {branch} "),
            Style::default().fg(THEME.branch_text).bg(THEME.branch_bg),
        ));
    }

    spans.push(separator);
    spans.push(Span::styled(
        date_heading(entry),
        Style::default().fg(THEME.subtle),
    ));

    Line::from(spans)
}

pub fn separator_line(width: u16, junction: Junction) -> Line<'static> {
    let junction = match junction {
        Junction::Top => "┬",
    };

    h_line(width, junction)
}

pub fn search_bar(query: &str, result_count: usize, width: u16) -> Vec<Line<'static>> {
    let input = Line::from(vec![
        gutter_label(result_count.to_string()),
        border_span("│"),
        Span::styled(" / ", Style::default().fg(THEME.subtle)),
        Span::styled(query.to_owned(), Style::default().fg(THEME.text)),
    ]);

    vec![h_line(width, "┬"), input, h_line(width, "┴")]
}

pub fn top_bar(width: u16) -> Vec<Line<'static>> {
    let input = Line::from(vec![
        gutter_label(""),
        border_span("│"),
        Span::styled(
            " Rewind ",
            Style::default()
                .fg(THEME.heading)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("· ", Style::default().fg(THEME.subtle)),
        Span::styled(
            "per-project command history for your shell",
            Style::default().fg(THEME.subtle),
        ),
    ]);

    vec![h_line(width, "┬"), input, h_line(width, "┴")]
}

pub fn editor_for_command(command: &str) -> TextArea<'static> {
    let mut textarea = TextArea::new(command_lines(command));

    // Place cursor at the end of the last line.
    textarea.move_cursor(CursorMove::Bottom);
    textarea.move_cursor(CursorMove::End);

    textarea
}

pub fn tui_background() -> Block<'static> {
    Block::default().style(Style::default().bg(THEME.background))
}

pub fn editor_block() -> Block<'static> {
    Block::default().padding(Padding {
        left: 1,
        right: 1,
        top: 0,
        bottom: 0,
    })
}

pub fn editor_footer(width: u16) -> Paragraph<'static> {
    let input = Line::from(vec![
        gutter_label(""),
        border_span("│"),
        Span::styled(
            " Rewind command ",
            Style::default()
                .fg(THEME.heading)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("· ", Style::default().fg(THEME.subtle)),
        Span::styled(
            "[Enter] run  [Alt+Enter] newline  [Esc] cancel ",
            Style::default().fg(THEME.subtle),
        ),
    ]);

    Paragraph::new(vec![h_line(width, "┬"), input, h_line(width, "┴")])
}

pub fn filter_footer(
    width: u16,
    filter: &Filter,
    context: &FilterContext,
    shortcuts: &[FilterShortcut],
) -> Paragraph<'static> {
    let mut spans = vec![
        gutter_label(""),
        border_span("│"),
        Span::styled(
            " Filters ",
            Style::default()
                .fg(THEME.heading)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("· ", Style::default().fg(THEME.subtle)),
    ];

    for (index, shortcut) in shortcuts.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }

        spans.push(Span::styled(
            format!("[{}] ", shortcut.key),
            Style::default().fg(THEME.subtle),
        ));
        spans.push(filter_label(shortcut, filter, context));
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled("[dd] ", Style::default().fg(THEME.subtle)));
    spans.push(Span::styled(
        if filter.only_deleted {
            "restore"
        } else {
            "delete"
        },
        Style::default().fg(THEME.muted),
    ));

    Paragraph::new(vec![
        h_line(width, "┼"),
        Line::from(spans),
        h_line(width, "┴"),
    ])
}

pub fn search_footer(width: u16) -> Paragraph<'static> {
    let input = Line::from(vec![
        gutter_label(""),
        border_span("│"),
        Span::styled(" [esc] ", Style::default().fg(THEME.subtle)),
        Span::styled("cancel", Style::default().fg(THEME.text)),
        Span::styled("  [enter] ", Style::default().fg(THEME.subtle)),
        Span::styled("select", Style::default().fg(THEME.text)),
        Span::styled("  [↑/↓] ", Style::default().fg(THEME.subtle)),
        Span::styled("navigate", Style::default().fg(THEME.text)),
    ]);

    Paragraph::new(vec![h_line(width, "┼"), input, h_line(width, "┴")])
}

pub fn render_editor_modal(frame: &mut Frame, textarea: &mut TextArea<'static>) {
    let screen_area = frame.area();

    let dim_block = Block::default().style(
        Style::default()
            .bg(THEME.modal_overlay_bg)
            .fg(THEME.modal_overlay_fg),
    );

    frame.render_widget(dim_block, screen_area);

    let modal_area = edit_modal_area(screen_area, textarea);
    frame.render_widget(Clear, modal_area);

    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(modal_area);
    let editor_area = chunks[1];
    let footer_area = chunks[0];

    textarea.set_block(editor_block());

    frame.render_widget(&*textarea, editor_area);
    frame.render_widget(editor_footer(footer_area.width), footer_area);
}

pub fn centered_modal(percent_x: u16, height: u16, area: Rect) -> Rect {
    let percent_x = percent_x.clamp(1, 100);
    let side_percent = (100 - percent_x) / 2;

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(side_percent),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(side_percent),
        ])
        .split(vertical[1])[1]
}

fn edit_modal_area(screen_area: Rect, textarea: &TextArea<'static>) -> Rect {
    let line_count = textarea.lines().len().max(1) as u16;

    let desired_height = line_count.saturating_add(2);
    let min_height = screen_area.height.min(EDIT_MODAL_HEIGHT);
    let max_height = (screen_area.height / 2).max(min_height);
    let modal_height = desired_height.clamp(min_height, max_height);

    centered_modal(EDIT_MODAL_WIDTH_PERCENT, modal_height, screen_area)
}

pub fn date_heading(entry: &Entry) -> String {
    let local = entry.started_at.with_timezone(&Local);
    let today = Local::now().date_naive();

    date_heading_from_local(&local, today)
}

pub fn format_duration(ms: i64) -> String {
    match ms {
        ms if ms < 1_000 => format!("{ms}ms"),
        ms if ms < 60_000 => format!("{:.1}s", ms as f64 / 1_000.0),
        ms => format!("{}m{}s", ms / 60_000, (ms % 60_000) / 1_000),
    }
}

fn status_parts(entry: &Entry) -> (&'static str, Color) {
    match entry.exit_code {
        Some(0) => ("✓", THEME.success),
        Some(_) => ("✗", THEME.error),
        None => ("?", THEME.subtle),
    }
}

fn format_branch_label(branch: Option<&str>) -> String {
    branch
        .map(|branch| format!(" [{branch}]"))
        .unwrap_or_default()
}

fn date_heading_from_local(local: &DateTime<Local>, today: NaiveDate) -> String {
    let date = local.date_naive();

    if date == today {
        "Today".to_string()
    } else if today.pred_opt().is_some_and(|yesterday| date == yesterday) {
        "Yesterday".to_string()
    } else if date.year() == today.year() {
        local.format("%A, %b %-d").to_string()
    } else {
        local.format("%A, %b %-d, %Y").to_string()
    }
}

fn push_exit_status(spans: &mut Vec<Span<'static>>, entry: &Entry) {
    match entry.exit_code {
        None => spans.push(Span::styled("● running", Style::default().fg(THEME.subtle))),
        Some(0) => spans.push(Span::styled("exit 0", Style::default().fg(THEME.success))),
        Some(code) => spans.push(Span::styled(
            format!("exit {code}"),
            Style::default().fg(THEME.error),
        )),
    }
}

fn shorten_home_path(cwd: &str) -> String {
    let Some(home) = HOME
        .get_or_init(|| std::env::var("HOME").ok().filter(|home| !home.is_empty()))
        .as_deref()
    else {
        return cwd.to_owned();
    };

    if cwd == home {
        return "~".to_string();
    }

    cwd.strip_prefix(home)
        .filter(|suffix| suffix.starts_with(std::path::MAIN_SEPARATOR))
        .map(|suffix| format!("~{suffix}"))
        .unwrap_or_else(|| cwd.to_owned())
}

fn h_line(width: u16, junction: &'static str) -> Line<'static> {
    let gutter = "─".repeat(GUTTER_WIDTH + 1);
    let tail_width = usize::from(width).saturating_sub(GUTTER_WIDTH + 2);
    let tail = "─".repeat(tail_width);

    Line::from(vec![
        border_span(gutter),
        border_span(junction),
        border_span(tail),
    ])
}

fn gutter_line<'a>(label: impl AsRef<str>, mut content: Vec<Span<'a>>) -> Line<'a> {
    let mut spans = vec![
        gutter_label(label.as_ref()),
        border_span("│"),
        Span::raw(" "),
    ];

    spans.append(&mut content);
    Line::from(spans)
}

fn gutter_label<'a>(label: impl AsRef<str>) -> Span<'a> {
    Span::styled(
        format!("{:>GUTTER_WIDTH$} ", label.as_ref()),
        Style::default().fg(THEME.subtle),
    )
}

fn border_span<'a>(content: impl Into<std::borrow::Cow<'a, str>>) -> Span<'a> {
    Span::styled(content.into(), Style::default().fg(THEME.border))
}

fn filter_label(
    shortcut: &FilterShortcut,
    filter: &Filter,
    context: &FilterContext,
) -> Span<'static> {
    let is_active = match shortcut.toggle {
        FilterToggle::Cwd => filter.cwd.is_some(),
        FilterToggle::Repo => filter.git_repo.is_some(),
        FilterToggle::Branch => filter.git_branch.is_some(),
        FilterToggle::Ok => filter.only_success,
        FilterToggle::Fail => filter.only_failure,
        FilterToggle::Deleted => filter.only_deleted,
    };

    let is_available = match shortcut.toggle {
        FilterToggle::Repo => context.git_repo.is_some(),
        FilterToggle::Branch => context.git_branch.is_some(),
        FilterToggle::Cwd | FilterToggle::Ok | FilterToggle::Fail | FilterToggle::Deleted => true,
    };

    let style = if is_active {
        Style::default()
            .fg(THEME.branch_text)
            .bg(THEME.branch_bg)
            .add_modifier(Modifier::BOLD)
    } else if is_available {
        Style::default().fg(THEME.muted)
    } else {
        Style::default().fg(THEME.subtle)
    };

    let label = if is_active {
        format!(" {} ", shortcut.label)
    } else if is_available {
        shortcut.label.to_owned()
    } else {
        format!("{} n/a", shortcut.label)
    };

    Span::styled(label, style)
}

fn command_lines(command: &str) -> Vec<String> {
    if command.is_empty() {
        return vec![String::new()];
    }

    command.lines().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> FilterContext {
        FilterContext::new(
            "/workspace/repo/crate",
            Some("/workspace/repo".to_owned()),
            Some("main".to_owned()),
        )
    }

    #[test]
    fn toggles_apply_current_context_without_changing_limit() {
        let context = context();
        let mut filter = Filter::new().limit(25);

        toggle_filter(&mut filter, FilterToggle::Cwd, &context);
        toggle_filter(&mut filter, FilterToggle::Repo, &context);
        toggle_filter(&mut filter, FilterToggle::Branch, &context);
        toggle_filter(&mut filter, FilterToggle::Ok, &context);

        assert_eq!(filter.cwd.as_deref(), Some("/workspace/repo/crate"));
        assert_eq!(filter.git_repo.as_deref(), Some("/workspace/repo"));
        assert_eq!(filter.git_branch.as_deref(), Some("main"));
        assert!(filter.only_success);
        assert!(!filter.only_failure);
        assert_eq!(filter.limit, Some(25));
    }

    #[test]
    fn success_and_failure_filters_are_exclusive_when_toggled() {
        let context = context();
        let mut filter = Filter::new();

        toggle_filter(&mut filter, FilterToggle::Ok, &context);
        toggle_filter(&mut filter, FilterToggle::Fail, &context);

        assert!(!filter.only_success);
        assert!(filter.only_failure);
    }

    #[test]
    fn unavailable_git_filters_do_not_activate() {
        let context = FilterContext::new("/workspace", None, None);
        let mut filter = Filter::new();

        toggle_filter(&mut filter, FilterToggle::Repo, &context);
        toggle_filter(&mut filter, FilterToggle::Branch, &context);

        assert!(filter.git_repo.is_none());
        assert!(filter.git_branch.is_none());
    }
}
