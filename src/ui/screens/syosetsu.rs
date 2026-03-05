use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, SyosetsuState};

/// Render the Syosetsu novel browser screen.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();

    let state = match app.syosetsu_state.as_ref() {
        Some(s) => s,
        None => {
            // Show input prompt for ncode
            render_ncode_input(frame, app);
            return;
        }
    };

    let outer = Layout::vertical([
        Constraint::Length(1), // title bar
        Constraint::Length(3), // novel info
        Constraint::Min(3),   // chapter list
        Constraint::Length(1), // status bar
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
        Span::raw(" — Syosetsu Browser"),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    // Novel info
    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                &state.novel.title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("by {}", state.novel.author),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::raw(format!(
                "  {} chapters  •  ncode: {}",
                state.novel.total_chapters, state.novel.ncode
            )),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );
    frame.render_widget(info, outer[1]);

    // Chapter list
    let items: Vec<ListItem> = state
        .novel
        .chapters
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let marker = if i == state.selected_chapter { "▶ " } else { "  " };
            let imported = if ch.text_id.is_some() { "✓" } else { " " };
            let style = if i == state.selected_chapter {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if ch.text_id.is_some() {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(
                    format!("[{}] ", imported),
                    if ch.text_id.is_some() {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(format!("Ch.{:>4}  ", ch.number), Style::default().fg(Color::DarkGray)),
                Span::styled(&ch.title, style),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(" Chapters ({}) ", state.novel.chapters.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );
    frame.render_widget(list, outer[2]);

    // Status bar
    let status = Line::from(vec![Span::styled(
        " ↑↓:navigate  Enter:import & open  r:refresh  Esc:back  q:quit  ?:help ",
        Style::default().fg(Color::DarkGray),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[3],
    );
}

/// Render the initial ncode input screen (when no novel is loaded).
fn render_ncode_input(frame: &mut Frame, _app: &App) {
    let area = frame.size();

    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    let title = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Syosetsu Browser"),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    let msg = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "No novel loaded.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from("Use the CLI to browse a Syosetsu novel:"),
        Line::from("  kotoba syosetsu <ncode-or-url>"),
        Line::from(""),
        Line::from("Or press Esc to go back to Library."),
    ])
    .block(
        Block::default()
            .title(" Syosetsu ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );
    frame.render_widget(msg, outer[1]);

    let status = Line::from(vec![Span::styled(
        " Esc:back  Tab:screens  q:quit ",
        Style::default().fg(Color::DarkGray),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}
