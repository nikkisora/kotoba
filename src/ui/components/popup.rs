use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, PopupState};

/// Render a popup overlay.
pub fn render_popup(frame: &mut Frame, _app: &App, popup: &PopupState) {
    match popup {
        PopupState::WordDetail {
            base_form,
            reading,
            entries,
            conjugations,
            notes,
            scroll,
        } => {
            let area = centered_rect(60, 80, frame.size());
            frame.render_widget(Clear, area);

            let mut lines: Vec<Line> = Vec::new();

            // Header
            lines.push(Line::from(vec![
                Span::styled(
                    base_form,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(reading, Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(""));

            // Dictionary entries
            if entries.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No dictionary entries found",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                for entry in entries {
                    if !entry.kanji_forms.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("Kanji: {}", entry.kanji_forms.join(", ")),
                            Style::default().fg(Color::Yellow),
                        )));
                    }
                    lines.push(Line::from(Span::styled(
                        format!("Readings: {}", entry.readings.join(", ")),
                        Style::default().fg(Color::Green),
                    )));

                    for (i, sense) in entry.senses.iter().enumerate() {
                        let pos = if sense.pos.is_empty() {
                            String::new()
                        } else {
                            format!("[{}] ", sense.pos.join(", "))
                        };
                        let glosses = sense.glosses.join("; ");
                        lines.push(Line::from(format!("  {}. {}{}", i + 1, pos, glosses)));
                    }
                    lines.push(Line::from(""));
                }
            }

            // Conjugation encounters
            if !conjugations.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Encountered Forms:",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )));
                for (surface, count) in conjugations {
                    lines.push(Line::from(format!("  {} (×{})", surface, count)));
                }
                lines.push(Line::from(""));
            }

            // Notes
            if let Some(notes_text) = notes {
                if !notes_text.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "Notes:",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(notes_text.as_str()));
                    lines.push(Line::from(""));
                }
            }

            lines.push(Line::from(Span::styled(
                "Press Esc or Enter to close",
                Style::default().fg(Color::DarkGray),
            )));

            let block = Block::default()
                .title(Span::styled(
                    " Word Detail ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue));

            let paragraph = Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((*scroll as u16, 0));

            frame.render_widget(paragraph, area);
        }

        PopupState::Help => {
            let area = centered_rect(60, 70, frame.size());
            frame.render_widget(Clear, area);

            let lines = vec![
                Line::from(Span::styled(
                    "Keybindings",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled("Global:", Style::default().add_modifier(Modifier::BOLD))),
                Line::from("  q / Ctrl+C  — Quit"),
                Line::from("  Tab         — Cycle screens"),
                Line::from("  ?           — Toggle this help"),
                Line::from(""),
                Line::from(Span::styled("Reader:", Style::default().add_modifier(Modifier::BOLD))),
                Line::from("  ↑/k         — Previous sentence"),
                Line::from("  ↓/j         — Next sentence"),
                Line::from("  ←/h         — Previous word"),
                Line::from("  →/l         — Next word"),
                Line::from("  1-4         — Set Learning status 1-4"),
                Line::from("  5           — Set Known status"),
                Line::from("  i           — Set Ignored status"),
                Line::from("  Enter       — Word detail popup"),
                Line::from("  n           — Edit word note"),
                Line::from("  Esc         — Deselect word / close popup"),
                Line::from(""),
                Line::from(Span::styled("Library:", Style::default().add_modifier(Modifier::BOLD))),
                Line::from("  ↑/↓         — Navigate texts"),
                Line::from("  Enter       — Open text in Reader"),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Esc or ? to close",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let block = Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::NoteEditor { text, .. } => {
            let area = centered_rect(50, 30, frame.size());
            frame.render_widget(Clear, area);

            let lines = vec![
                Line::from(Span::styled(
                    "Edit Note:",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(text.as_str()),
                Line::from(""),
                Line::from(Span::styled(
                    "Type to edit • Enter to save • Esc to cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let block = Block::default()
                .title(" Note Editor ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::QuitConfirm => {
            let area = centered_rect(40, 15, frame.size());
            frame.render_widget(Clear, area);

            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Quit kotoba?",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("  y — Yes, quit"),
                Line::from("  n — No, cancel"),
            ];

            let block = Block::default()
                .title(" Confirm ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }
    }
}

/// Create a centered rect with percentage of the total area.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}
