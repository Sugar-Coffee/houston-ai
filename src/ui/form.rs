//! Rendering forms — the Settings menu and the new-session dialog.

use crate::{
    form::{FieldKind, Form},
    ui::{Theme, keycap},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

/// Every option on the row, with the current one filled in.
///
/// A cycling field makes you press a key to find out what else it could be.
/// Three options fit on a line, so they are all just there.
///
/// **The fill is `accent`, not the value's own colour.** It was the meaning
/// colour first — green for `open`, grey for `low` — which is more informative
/// and, on the quiet end of every scale, unreadable: picking `low` lit it in
/// the same grey as the options you had not picked. A control has one job
/// before it has any other, which is to show what you have chosen, and
/// `accent` is the hue that already means exactly that everywhere else in
/// Houston. The meanings are still coloured where they are *read*, on the card
/// and in the pane's stat bar.
fn segments<'a>(options: &[String], current: &str, theme: Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::with_capacity(options.len() * 2);

    for option in options {
        spans.push(Span::styled(
            format!(" {option} "),
            if option == current {
                Style::default().fg(theme.surface).bg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.dim)
            },
        ));
        spans.push(Span::raw(" "));
    }
    spans
}

/// Tags as separate things rather than as one comma-separated string.
///
/// "auth, security, api" in a text field is a sentence you have to parse to
/// see that it is three of something. Filled in, they are three of something
/// at a glance — which matters most while you are adding a fourth.
///
/// All one colour on purpose. A palette per tag would be a new hue for every
/// word somebody invents, and `ui::theme` has exactly as many hues as it has
/// meanings.
fn chips<'a>(value: &str, focused: bool, editing: bool, theme: Theme) -> Vec<Span<'a>> {
    // A selected row is filled in `highlight`, so chips filled in `highlight`
    // vanish into it exactly when you are looking at them. On the filled row
    // they go the other way and sit *below* it instead.
    let fill = if focused { theme.surface } else { theme.highlight };

    let mut parts: Vec<&str> = value.split(',').collect();

    // While typing, the fragment after the last comma is not a tag yet — it is
    // what you are in the middle of. Filling it in as you type would have it
    // flicker between chip and text on every keystroke.
    let partial = if editing { parts.pop().unwrap_or("") } else { "" };

    let mut spans: Vec<Span<'a>> = parts
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .flat_map(|tag| {
            [
                Span::styled(format!(" {tag} "), Style::default().fg(theme.text).bg(fill)),
                Span::raw(" "),
            ]
        })
        .collect();

    if editing {
        spans.push(Span::styled(
            format!("{}\u{258f}", partial.trim_start()),
            Style::default().fg(theme.code).add_modifier(Modifier::BOLD),
        ));
    } else if spans.is_empty() {
        spans.push(Span::styled(
            "none".to_string(),
            Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC),
        ));
    }

    spans
}

/// Width of the label column, so values line up into a readable column.
const LABEL_WIDTH: usize = 22;

pub fn render(frame: &mut Frame, area: Rect, form: &Form, theme: Theme, title: &str) {
    let editing = form.is_editing();

    // Yellow while typing, matching every other text field in the app.
    let border = if editing { theme.code } else { theme.dim };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(border).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();

    for (index, field) in form.fields.iter().enumerate() {
        if !field.visible {
            continue;
        }
        let focused = index == form.focused_index();

        let marker = if focused { "▸ " } else { "  " };
        let label_style = if focused {
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.dim)
        };

        // The value shows a cursor only while that field is being edited, so
        // there is never a question about where typing will land.
        let (value, value_style) = if focused && editing {
            (
                format!("{}▏", field.value),
                Style::default().fg(theme.code).add_modifier(Modifier::BOLD),
            )
        } else if field.kind == FieldKind::Toggle {
            let colour = if field.on { theme.running } else { theme.dim };
            (field.display(), Style::default().fg(colour))
        } else if field.value.is_empty() {
            (field.display(), Style::default().fg(theme.dim).add_modifier(Modifier::ITALIC))
        } else {
            (field.display(), Style::default().fg(theme.text))
        };

        let mut spans = vec![
            Span::styled(marker, Style::default().fg(theme.accent)),
            Span::styled(format!("{:<LABEL_WIDTH$}", field.label), label_style),
        ];

        // Two kinds draw their value as more than a string, because for both
        // of them the *shape* is the information: which of these three, and
        // how many of these are there.
        match field.kind {
            FieldKind::Segments => spans.extend(segments(&field.options, &field.value, theme)),
            FieldKind::Tags => spans.extend(chips(&field.value, focused, editing, theme)),
            // Filled in the hue a project has everywhere else, so the dialog
            // and the pane's stat bar are visibly showing the same fact.
            FieldKind::Pick if !field.value.is_empty() => spans.push(Span::styled(
                format!(" {} ", field.value),
                Style::default().fg(theme.surface).bg(theme.link),
            )),
            _ => spans.push(Span::styled(value, value_style)),
        }

        let line = Line::from(spans);
        lines.push(if focused {
            keycap::fill(line, inner.width).style(keycap::selected_row(theme))
        } else {
            line
        });

        // Candidates, indented under the field they belong to.
        if focused && !field.completions.is_empty() {
            // A trailing slash says "this is a folder, there is more path to
            // come". A tag is the whole thing, so it does not get one.
            let suffix = if field.kind == crate::form::FieldKind::Tags { "" } else { "/" };

            for candidate in field.completions.iter().take(6) {
                lines.push(Line::from(Span::styled(
                    format!("{:>width$}{candidate}{suffix}", "", width = LABEL_WIDTH + 4),
                    Style::default().fg(theme.link),
                )));
            }
            if field.completions.len() > 6 {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{:>width$}… {} more",
                        "",
                        field.completions.len() - 6,
                        width = LABEL_WIDTH + 4
                    ),
                    Style::default().fg(theme.dim),
                )));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}
