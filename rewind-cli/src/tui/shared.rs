use chrono::{Datelike, Local};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, ListItem},
};
use rewind_core::entry::Entry;

const GUTTER_WIDTH: usize = 6;

pub struct TuiTheme {
    pub text: Color,
    pub muted: Color,
    pub subtle: Color,
    pub border: Color,
    pub heading: Color,
    pub success: Color,
    pub error: Color,
    pub branch: Color,
    pub branch_text: Color,
    pub branch_bg: Color,
    pub selected_item_bg: Color,
}

pub const THEME: TuiTheme = TuiTheme {
    text: Color::White,
    muted: Color::Gray,
    subtle: Color::DarkGray,
    border: Color::Rgb(54, 68, 58),
    heading: Color::Yellow,
    success: Color::Green,
    error: Color::Red,
    branch: Color::Cyan,
    branch_text: Color::Rgb(148, 163, 184),
    branch_bg: Color::Rgb(30, 41, 59),
    selected_item_bg: Color::Rgb(64, 64, 64),
};

pub fn selected_item_style() -> Style {
    Style::default()
        .bg(THEME.selected_item_bg)
        .add_modifier(Modifier::BOLD)
}

pub fn list_block<'a>(title: impl Into<Option<String>>) -> Block<'a> {
    let block = Block::default();

    match title.into() {
        Some(title) => block.title(title),
        Option::None => block,
    }
}

fn status_parts(entry: &Entry) -> (&'static str, Color) {
    match entry.exit_code {
        Some(0) => ("✓", THEME.success),
        Some(_) => ("✗", THEME.error),
        Option::None => ("?", THEME.subtle),
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
    ListItem::new(gutter_line(
        "",
        vec![Span::styled(
            "No command history yet.",
            Style::default().fg(THEME.subtle),
        )],
    ))
}

pub fn date_heading_item(heading: &str) -> ListItem<'static> {
    ListItem::new(gutter_line(
        "",
        vec![Span::styled(
            heading.to_owned(),
            Style::default()
                .fg(THEME.heading)
                .add_modifier(Modifier::BOLD),
        )],
    ))
}

pub fn command_item(entry: &Entry) -> ListItem<'static> {
    let (status, status_color) = status_parts(entry);
    let time = entry
        .started_at
        .with_timezone(&Local)
        .format("%H:%M")
        .to_string();
    let branch = entry
        .git_branch
        .as_deref()
        .map(|branch| format!(" [{branch}]"))
        .unwrap_or_default();

    ListItem::new(gutter_line(
        time,
        vec![
            Span::styled(status, Style::default().fg(status_color)),
            Span::styled(branch, Style::default().fg(THEME.branch)),
            Span::raw("  "),
            Span::styled(entry.command.clone(), Style::default().fg(THEME.text)),
        ],
    ))
}

fn gutter_line(label: impl AsRef<str>, mut content: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            format!("{:>GUTTER_WIDTH$} ", label.as_ref()),
            Style::default().fg(THEME.subtle),
        ),
        Span::styled("│", Style::default().fg(THEME.border)),
        Span::raw(" "),
    ];

    spans.append(&mut content);
    Line::from(spans)
}

pub fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}m{}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

pub fn context_bar(entry: Option<&Entry>) -> Line<'static> {
    let Some(entry) = entry else {
        return Line::from(Span::styled(
            "no selection",
            Style::default().fg(THEME.subtle),
        ));
    };

    let mut spans: Vec<Span> = Vec::new();
    let sep = Span::styled(" · ", Style::default().fg(THEME.subtle));

    // exit code
    match entry.exit_code {
        Option::None => spans.push(Span::styled("● running", Style::default().fg(THEME.subtle))),
        Some(0) => spans.push(Span::styled("exit 0", Style::default().fg(THEME.success))),
        Some(n) => spans.push(Span::styled(
            format!("exit {}", n),
            Style::default().fg(THEME.error),
        )),
    }

    // duration
    if let Some(ms) = entry.duration_ms {
        spans.push(sep.clone());
        spans.push(Span::styled(
            format_duration(ms),
            Style::default().fg(THEME.muted),
        ));
    }

    // cwd (shorten ~/home prefix)
    spans.push(sep.clone());
    let home = std::env::var("HOME").unwrap_or_default();
    let cwd = if entry.cwd.starts_with(&home) {
        format!("~{}", &entry.cwd[home.len()..])
    } else {
        entry.cwd.clone()
    };
    spans.push(Span::styled(cwd, Style::default().fg(THEME.muted)));

    // git branch
    if let Some(branch) = &entry.git_branch {
        spans.push(sep.clone());
        spans.push(Span::styled(
            format!(" {} ", branch),
            Style::default().fg(THEME.branch_text).bg(THEME.branch_bg),
        ));
    }

    // timestamp
    spans.push(sep.clone());
    spans.push(Span::styled(
        date_heading(entry),
        Style::default().fg(THEME.subtle),
    ));

    Line::from(spans)
}

fn h_line(width: u16, junction: &'static str) -> Line<'static> {
    let gutter = "─".repeat(GUTTER_WIDTH + 1);
    let tail = "─".repeat((width as usize).saturating_sub(GUTTER_WIDTH + 2));
    Line::from(vec![
        Span::styled(gutter, Style::default().fg(THEME.border)),
        Span::styled(junction, Style::default().fg(THEME.border)),
        Span::styled(tail, Style::default().fg(THEME.border)),
    ])
}

pub enum Junction {
    Top,    // ┬
    Bottom, // ┴
}

pub fn separator_line(width: u16, junction: Junction) -> Line<'static> {
    let ch = match junction {
        Junction::Top => "┬",
        Junction::Bottom => "┴",
    };
    h_line(width, ch)
}

pub fn search_bar(query: &str, result_count: usize, width: u16) -> Vec<Line<'static>> {
    let input = Line::from(vec![
        Span::styled(
            format!("{:>GUTTER_WIDTH$} ", result_count),
            Style::default().fg(THEME.subtle),
        ),
        Span::styled("│", Style::default().fg(THEME.border)),
        Span::styled(" / ", Style::default().fg(THEME.subtle)),
        Span::styled(query.to_string(), Style::default().fg(THEME.text)),
    ]);

    vec![h_line(width, "┬"), input, h_line(width, "┴")]
}

pub fn top_bar(width: u16) -> Vec<Line<'static>> {
    let input = Line::from(vec![
        Span::styled(
            format!("{:>GUTTER_WIDTH$} ", ""),
            Style::default().fg(THEME.subtle),
        ),
        Span::styled("│", Style::default().fg(THEME.border)),
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
