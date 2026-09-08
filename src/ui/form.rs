//! Rendering forms — the Settings menu and the new-session dialog.

use crate::{
    form::{FieldKind, Form},
    ui::Theme,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

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

        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme.accent)),
            Span::styled(format!("{:<LABEL_WIDTH$}", field.label), label_style),
            Span::styled(value, value_style),
        ]));

        // Candidates from the last Tab, indented under the field they belong to.
        if focused && !field.completions.is_empty() {
            for candidate in field.completions.iter().take(6) {
                lines.push(Line::from(Span::styled(
                    format!("{:>width$}{candidate}/", "", width = LABEL_WIDTH + 4),
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
