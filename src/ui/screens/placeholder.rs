use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::ui::theme::Theme;

/// Render a placeholder screen for unimplemented features.
pub fn render(frame: &mut Frame, title: &str, description: &str, theme: &Theme) {
    let area = frame.size();

    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    let title_line = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" — {}", title)),
    ]);
    frame.render_widget(
        Paragraph::new(title_line).style(Style::default().bg(theme.title_bar_bg)),
        outer[0],
    );

    let content = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(description, Style::default().fg(theme.muted))),
        Line::from(""),
        Line::from("Press Tab to switch screens."),
    ]);
    frame.render_widget(content, outer[1]);

    let status = Line::from(Span::styled(
        " Tab:screens  q:quit  ?:help ",
        Style::default().fg(theme.muted),
    ));
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(theme.title_bar_bg)),
        outer[2],
    );
}
