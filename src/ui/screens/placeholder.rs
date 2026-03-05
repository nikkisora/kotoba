use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Render a placeholder screen for unimplemented features.
pub fn render(frame: &mut Frame, title: &str, description: &str) {
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
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" — {}", title)),
    ]);
    frame.render_widget(
        Paragraph::new(title_line).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    let content = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(description, Style::default().fg(Color::DarkGray))),
        Line::from(""),
        Line::from("Press Tab to switch screens."),
    ]);
    frame.render_widget(content, outer[1]);

    let status = Line::from(Span::styled(
        " Tab:screens  q:quit  ?:help ",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}
