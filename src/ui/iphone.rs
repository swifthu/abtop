use crate::app::App;
use crate::theme::Theme;
use ratatui::layout::Rect;
use ratatui::Frame;

/// Entry point for iPhone (46-column) compact mode.
///
/// Triggered by `src/ui/mod.rs::draw` when `width <= IPHONE_WIDTH` and
/// `height >= IPHONE_MIN_HEIGHT`. Renders a single-page integrated layout:
/// meta + quota + sessions (max 7 × 3 rows) + selected session chat (5)
/// + footer keybinds, separated by ── named dividers.
pub(crate) fn draw_iphone_mode(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    // TODO: implement meta / quota / sessions / chat / footer / dividers
    // For now, paint a placeholder so we can wire up dispatch in mod.rs.
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let line = Line::from(Span::styled(
        "iphone mode (placeholder)",
        Style::default()
            .fg(theme.title)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PanelVisibility;
    use crate::demo;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn iphone_mode_placeholder_renders() {
        let mut app = App::new_with_config(Theme::default(), &[], PanelVisibility::default());
        demo::populate_demo(&mut app);
        let backend = TestBackend::new(46, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_iphone_mode(f, &app, f.area(), &app.theme))
            .unwrap();
        let text = format!("{}", terminal.backend());
        assert!(
            text.contains("iphone mode"),
            "placeholder should render\n{text}"
        );
    }
}
