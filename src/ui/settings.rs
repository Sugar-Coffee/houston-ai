//! The Settings view.
//!
//! Currently one editable setting — where the vault lives — plus a read-only
//! picture of what Houston has detected. That is enough to make the difference
//! ADR-0007 cares about: pointing Houston at an existing Obsidian vault is a
//! deliberate act you can perform in the app, not an environment variable you
//! have to know about.

use crate::{app::App, config, hooks, provider, ui::Theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.dim))
        .title(Span::styled(
            " settings ",
            Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![heading("Vault", theme), vault_line(app, theme), Line::from("")];

    if let Some(error) = &app.vault_error {
        lines.push(indented(error, theme.danger, theme));
        lines.push(Line::from(""));
    } else if let Some(browser) = &app.browser {
        lines.push(indented(&format!("{} notes indexed", browser.vault.len()), theme.dim, theme));
        lines.push(Line::from(""));
    }

    lines.push(indented(
        "e  change the vault folder      ~ is expanded      HOUSTON_VAULT overrides this",
        theme.dim,
        theme,
    ));
    lines.push(Line::from(""));

    lines.push(heading("Agents detected", theme));
    let available = provider::available();
    if available.is_empty() {
        lines.push(indented(
            "none on PATH — new sessions will open a shell instead",
            theme.dim,
            theme,
        ));
    } else {
        for (index, found) in available.iter().enumerate() {
            let note = if index == 0 { "  (default)" } else { "" };
            let colour = if index == 0 { theme.running } else { theme.text };
            lines.push(indented(
                &format!("{} — {}{note}", found.label, found.command),
                colour,
                theme,
            ));
        }
    }
    lines.push(Line::from(""));

    lines.push(heading("Paths", theme));
    for (label, path) in
        [("config", config::config_path().ok()), ("hook socket", hooks::socket_path().ok())]
    {
        let shown = path.map_or_else(|| "unavailable".to_string(), |p| p.display().to_string());
        lines.push(indented(&format!("{label}: {shown}"), theme.dim, theme));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The vault row, which doubles as the edit field while you are typing.
fn vault_line<'a>(app: &App, theme: Theme) -> Line<'a> {
    if let Some(draft) = &app.editing_vault {
        return Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("{draft}▏"),
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    let current = app
        .config
        .vault_root()
        .map_or_else(|_| "unavailable".to_string(), |root| root.display().to_string());

    let source = if std::env::var_os("HOUSTON_VAULT").is_some() {
        "  (from HOUSTON_VAULT)"
    } else if app.config.vault.is_some() {
        "  (from config)"
    } else {
        "  (default — Houston created this)"
    };

    Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(current, Style::default().fg(theme.text)),
        Span::styled(source, Style::default().fg(theme.dim)),
    ])
}

fn heading(text: &str, theme: Theme) -> Line<'_> {
    Line::from(Span::styled(text, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)))
}

fn indented<'a>(text: &str, colour: ratatui::style::Color, _theme: Theme) -> Line<'a> {
    Line::from(Span::styled(format!("  {text}"), Style::default().fg(colour)))
}
