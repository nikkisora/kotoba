use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::ui::theme::Theme;

/// Render a status bar message at the bottom of the screen.
pub fn render_message(frame: &mut Frame, msg: &str, theme: &Theme) {
    let area = frame.size();
    if area.height < 1 {
        return;
    }

    let bar_area = Rect {
        x: 0,
        y: area.height - 1,
        width: area.width,
        height: 1,
    };

    frame.render_widget(Clear, bar_area);
    let paragraph = Paragraph::new(Span::styled(
        format!(" {} ", msg),
        Style::default()
            .fg(theme.status_msg_fg)
            .bg(theme.status_msg_bg),
    ));
    frame.render_widget(paragraph, bar_area);
}
