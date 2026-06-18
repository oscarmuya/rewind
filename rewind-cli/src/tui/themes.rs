use ratatui::style::Color;
use std::ops::Deref;
use std::process::Command;
use std::sync::OnceLock;

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
    pub modal_overlay_bg: Color,
    pub modal_overlay_fg: Color,
    pub background: Color,
}

const THEME_DARK: TuiTheme = TuiTheme {
    text: Color::White,
    muted: Color::Gray,
    subtle: Color::Rgb(156, 163, 175),
    border: Color::Rgb(54, 68, 58),
    heading: Color::Yellow,
    success: Color::Green,
    error: Color::Red,
    branch: Color::Cyan,
    branch_text: Color::Rgb(148, 163, 184),
    branch_bg: Color::Rgb(30, 41, 59),
    selected_item_bg: Color::Rgb(64, 64, 64),
    modal_overlay_bg: Color::Black,
    modal_overlay_fg: Color::DarkGray,
    background: Color::Rgb(15, 17, 21),
};

const THEME_LIGHT: TuiTheme = TuiTheme {
    text: Color::Rgb(15, 23, 42),
    muted: Color::Rgb(100, 116, 139),
    subtle: Color::Rgb(148, 163, 184),
    border: Color::Rgb(180, 196, 183),
    heading: Color::Rgb(133, 77, 14),
    success: Color::Rgb(22, 101, 52),
    error: Color::Rgb(185, 28, 28),
    branch: Color::Rgb(14, 116, 144),
    branch_text: Color::Rgb(30, 41, 59),
    branch_bg: Color::Rgb(186, 230, 253),
    selected_item_bg: Color::Rgb(226, 232, 240),
    modal_overlay_bg: Color::Rgb(241, 245, 249),
    modal_overlay_fg: Color::Rgb(100, 116, 139),
    background: Color::Rgb(248, 250, 252),
};

static THEME_INSTANCE: OnceLock<&'static TuiTheme> = OnceLock::new();

fn detect_dark_mode() -> bool {
    detect_terminal_dark_mode()
        .or_else(detect_platform_dark_mode)
        .unwrap_or(true)
}

fn detect_terminal_dark_mode() -> Option<bool> {
    if let Ok(term) = std::env::var("COLORFGBG")
        && let Some(bg) = term.split(';').next_back()
        && let Ok(n) = bg.trim().parse::<u8>()
    {
        return Some(n < 8);
    }

    if let Ok(val) = std::env::var("TERM_BACKGROUND") {
        let val = val.trim().to_ascii_lowercase();

        if val == "dark" {
            return Some(true);
        }

        if val == "light" {
            return Some(false);
        }
    }

    None
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();

    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

#[cfg(target_os = "macos")]
fn detect_platform_dark_mode() -> Option<bool> {
    // Dark mode returns "Dark"; light mode usually exits non-zero because the key is absent.
    match command_stdout("defaults", &["read", "-g", "AppleInterfaceStyle"]) {
        Some(value) => Some(value.eq_ignore_ascii_case("dark")),
        None => Some(false),
    }
}

#[cfg(target_os = "windows")]
fn detect_platform_dark_mode() -> Option<bool> {
    // AppsUseLightTheme: 0 = dark, 1 = light.
    let output = command_stdout(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ],
    )?;

    parse_windows_apps_use_light_theme(&output)
}

#[cfg(target_os = "windows")]
fn parse_windows_apps_use_light_theme(output: &str) -> Option<bool> {
    let line = output
        .lines()
        .find(|line| line.contains("AppsUseLightTheme"))?;

    let value = line.split_whitespace().last()?;

    if value == "0x0" || value == "0" {
        return Some(true);
    }

    if value == "0x1" || value == "1" {
        return Some(false);
    }

    None
}

#[cfg(target_os = "linux")]
fn detect_platform_dark_mode() -> Option<bool> {
    detect_gnome_dark_mode().or_else(detect_kde_dark_mode)
}

#[cfg(target_os = "linux")]
fn detect_gnome_dark_mode() -> Option<bool> {
    // GNOME 42+: "'prefer-dark'", "'prefer-light'", or "'default'".
    if let Some(value) = command_stdout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "color-scheme"],
    ) {
        let value = value.trim_matches('\'').to_ascii_lowercase();

        if value.contains("prefer-dark") {
            return Some(true);
        }

        if value.contains("prefer-light") {
            return Some(false);
        }
    }

    // Older GNOME setups often encode the mode in the GTK theme name.
    let value = command_stdout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "gtk-theme"],
    )?;

    let value = value.trim_matches('\'').to_ascii_lowercase();

    if value.contains("dark") {
        Some(true)
    } else if value.contains("light") {
        Some(false)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_kde_dark_mode() -> Option<bool> {
    // KDE Plasma 6/5: read the active color scheme from kdeglobals.
    for reader in ["kreadconfig6", "kreadconfig5", "kreadconfig"] {
        if let Some(value) = command_stdout(
            reader,
            &[
                "--file",
                "kdeglobals",
                "--group",
                "General",
                "--key",
                "ColorScheme",
            ],
        ) {
            let value = value.to_ascii_lowercase();

            if value.contains("dark") {
                return Some(true);
            }

            if value.contains("light") {
                return Some(false);
            }
        }
    }

    detect_kde_dark_mode_from_config()
}

#[cfg(target_os = "linux")]
fn detect_kde_dark_mode_from_config() -> Option<bool> {
    let path = kdeglobals_path()?;
    let contents = std::fs::read_to_string(path).ok()?;

    let mut in_window_colors = false;

    for raw_line in contents.lines() {
        let line = raw_line.trim();

        if line.starts_with('[') && line.ends_with(']') {
            in_window_colors = line == "[Colors:Window]";
            continue;
        }

        if !in_window_colors {
            continue;
        }

        let Some(value) = line.strip_prefix("BackgroundNormal=") else {
            continue;
        };

        return parse_rgb_is_dark(value);
    }

    None
}

#[cfg(target_os = "linux")]
fn kdeglobals_path() -> Option<std::path::PathBuf> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(std::path::PathBuf::from(config_home).join("kdeglobals"));
    }

    let home = std::env::var_os("HOME")?;
    Some(
        std::path::PathBuf::from(home)
            .join(".config")
            .join("kdeglobals"),
    )
}

#[cfg(target_os = "linux")]
fn parse_rgb_is_dark(value: &str) -> Option<bool> {
    let mut parts = value.split(',').map(|part| part.trim().parse::<u16>());

    let r = parts.next()?.ok()?;
    let g = parts.next()?.ok()?;
    let b = parts.next()?.ok()?;

    // Perceived luminance threshold.
    let luminance = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;

    Some(luminance < 128.0)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn detect_platform_dark_mode() -> Option<bool> {
    None
}

pub fn init_theme() {
    THEME_INSTANCE.get_or_init(selected_theme);
}

pub fn theme() -> &'static TuiTheme {
    THEME_INSTANCE.get_or_init(selected_theme)
}

fn selected_theme() -> &'static TuiTheme {
    if detect_dark_mode() {
        &THEME_DARK
    } else {
        &THEME_LIGHT
    }
}

pub static THEME: ThemeProxy = ThemeProxy;

pub struct ThemeProxy;

impl Deref for ThemeProxy {
    type Target = TuiTheme;

    fn deref(&self) -> &'static TuiTheme {
        theme()
    }
}
