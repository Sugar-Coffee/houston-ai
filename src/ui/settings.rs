//! The Settings view: a navigable menu.
//!
//! Move with `j`/`k`, press Return to edit a row. Deliberately not a set of
//! keybindings — a keybind per setting does not scale past about three, and a
//! menu tells you what exists without your having to already know.
//!
//! Settings apply the moment a row is committed. There is no save button,
//! because a settings screen with one is a settings screen you can leave in a
//! state that does not match what the app is doing.

use crate::{app::App, config, form::Form, hooks, provider, ui::Theme, ui::form as form_ui};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(settings_height(&app.settings)), Constraint::Min(0)])
        .split(area);

    form_ui::render(frame, rows[0], &app.settings, theme, " settings ");
    render_status(frame, rows[1], app, theme);
}

/// Tall enough for the fields plus their borders and any completion list.
fn settings_height(form: &Form) -> u16 {
    let completions =
        form.focused().map_or(0, |field| u16::try_from(field.completions.len()).unwrap_or(0));
    u16::try_from(form.fields.len()).unwrap_or(2) + completions.min(6) + 3
}

/// What Houston has found, below the settings themselves.
fn render_status(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(
            " detected ",
            Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();

    if let Some(error) = &app.vault_error {
        lines.push(row("vault", error, theme.danger, theme));
    } else if let Some(browser) = &app.browser {
        lines.push(row(
            "vault",
            &format!("{} notes indexed", browser.vault.len()),
            theme.running,
            theme,
        ));
    }

    let available = provider::available();
    if available.is_empty() {
        lines.push(row(
            "agents",
            "none on PATH — new sessions will open a shell",
            theme.dim,
            theme,
        ));
    } else {
        for (index, found) in available.iter().enumerate() {
            let label = if index == 0 { "agents" } else { "" };
            let note = if index == 0 { "  (default)" } else { "" };
            let colour = if index == 0 { theme.running } else { theme.text };
            lines.push(row(label, &format!("{}{note}", found.label), colour, theme));
        }
    }

    lines.push(Line::from(""));
    for (label, path) in [
        ("config", config::config_path().ok()),
        ("worktrees", crate::worktree::root().ok()),
        ("socket", hooks::socket_path().ok()),
    ] {
        let shown = path
            .map_or_else(|| "unavailable".to_string(), |path| crate::paths::contract_home(&path));
        lines.push(row(label, &shown, theme.dim, theme));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn row<'a>(label: &str, value: &str, colour: ratatui::style::Color, theme: Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {label:<11}"), Style::default().fg(theme.dim)),
        Span::styled(value.to_string(), Style::default().fg(colour)),
    ])
}
