//! Rendering.
//!
//! The chrome — a tab strip on top, a keybind bar on the bottom — is the part
//! of Chloe the brief singled out as making it pleasant to live in, so it is
//! present from the first commit rather than bolted on later.

mod chrome;
mod theme;

pub use theme::Theme;

use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

pub fn render(frame: &mut Frame, app: &App) {
    let theme = Theme::default();

    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    chrome::tab_strip(frame, areas[0], app, theme);
    chrome::placeholder(frame, areas[1], app, theme);
    chrome::keybind_bar(frame, areas[2], theme);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Tab;
    use ratatui::{Terminal, backend::TestBackend};

    fn draw(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn chrome_shows_every_tab_and_the_keybinds() {
        let rendered = draw(&App::new(), 100, 24);
        for tab in Tab::ALL {
            assert!(rendered.contains(tab.title()), "tab strip missing {}", tab.title());
        }
        assert!(rendered.contains("houston"));
        assert!(rendered.contains("next view"));
        assert!(rendered.contains("quit"));
    }

    #[test]
    fn body_follows_the_selected_tab() {
        let mut app = App::new();
        app.select_tab(Tab::Vault);
        let rendered = draw(&app, 100, 24);
        assert!(rendered.contains("browse, search and follow wikilinks"));
    }

    /// A terminal can legitimately be one row tall mid-resize. Rendering must
    /// not panic when the chrome alone exceeds the available height.
    #[test]
    fn survives_a_degenerate_viewport() {
        for height in 0..=3 {
            let _ = draw(&App::new(), 20, height);
        }
    }
}
