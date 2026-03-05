use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Render the library screen showing imported texts.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();

    let outer = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(3),   // content
        Constraint::Length(1), // status
    ])
    .split(area);

    // Title bar
    let title = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Library"),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    // Content
    let lib = app.library_state.as_ref();
    let texts = lib.map(|l| &l.texts);
    let selected = lib.map(|l| l.selected).unwrap_or(0);

    match texts {
        Some(texts) if !texts.is_empty() => {
            let items: Vec<ListItem> = texts
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let marker = if i == selected { "▶ " } else { "  " };
                    let style = if i == selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(marker, style),
                        Span::styled(&t.title, style),
                        Span::styled(
                            format!("  [{}]  {}", t.source_type, t.created_at),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();

            let list = List::new(items).block(
                Block::default()
                    .title(" Imported Texts ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            );
            frame.render_widget(list, outer[1]);
        }
        _ => {
            let msg = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No texts imported yet.",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from("Import a text with: kotoba import <file>"),
                Line::from("Then launch: kotoba run"),
            ])
            .block(
                Block::default()
                    .title(" Library ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            );
            frame.render_widget(msg, outer[1]);
        }
    }

    // Status bar
    let status = Line::from(vec![
        Span::styled(
            " ↑↓:navigate  Enter:open  Tab:screens  q:quit  ?:help ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}
