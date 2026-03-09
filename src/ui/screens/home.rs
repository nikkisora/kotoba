use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Render a mini progress bar using block characters.
fn progress_bar(current: u64, total: u64, width: usize) -> String {
    if total == 0 {
        return "░".repeat(width);
    }
    let filled = ((current as f64 / total as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// Render the home screen.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();

    let outer = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(3),    // content
        Constraint::Length(1), // status
    ])
    .split(area);

    // Title bar
    let home = app.home_state.as_ref();
    let due_counts = home.map(|h| h.due_card_counts).unwrap_or((0, 0));
    let review_indicator = if due_counts.0 > 0 {
        vec![
            Span::raw("  "),
            Span::styled(
                format!("[r] {} due", due_counts.0),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };
    let mut title_spans = vec![
        Span::styled(
            " kotoba",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Home"),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(Color::DarkGray)),
    ];
    title_spans.extend(review_indicator);
    let title = Line::from(title_spans);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    // Content
    let show_finished = home.map(|h| h.show_finished).unwrap_or(false);
    let selected = home.map(|h| h.selected).unwrap_or(0);

    // Filter recent texts: hide finished unless toggled
    let filtered: Vec<&crate::db::models::Text> = home
        .map(|h| {
            h.recent_texts
                .iter()
                .filter(|t| {
                    if show_finished {
                        return true;
                    }
                    // Not finished: total_sentences == 0 or last_sentence_index < total_sentences - 1
                    t.total_sentences == 0 || t.last_sentence_index < t.total_sentences - 1
                })
                .collect()
        })
        .unwrap_or_default();

    if !filtered.is_empty() {
        let stats_map = home.map(|h| &h.recent_stats);

        let items: Vec<ListItem> = filtered
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

                let progress = (t.last_sentence_index + 1).min(t.total_sentences);
                let pbar = progress_bar(progress as u64, t.total_sentences as u64, 12);
                let pct = if t.total_sentences > 0 {
                    (progress * 100 / t.total_sentences) as u8
                } else {
                    0
                };

                let stats_str = stats_map
                    .and_then(|m| m.get(&t.id))
                    .map(|s| {
                        let kpct = if s.unique_vocab == 0 {
                            0
                        } else {
                            s.known_count * 100 / s.unique_vocab
                        };
                        let lpct = if s.unique_vocab == 0 {
                            0
                        } else {
                            s.learning_count * 100 / s.unique_vocab
                        };
                        let npct = if s.unique_vocab == 0 {
                            0
                        } else {
                            s.new_count * 100 / s.unique_vocab
                        };
                        format!("  K:{}% L:{}% N:{}%", kpct, lpct, npct)
                    })
                    .unwrap_or_default();

                ListItem::new(Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(&t.title, style),
                    Span::raw("  "),
                    Span::styled(
                        format!("{} {}%", pbar, pct),
                        Style::default().fg(Color::Rgb(100, 180, 100)),
                    ),
                    Span::styled(stats_str, Style::default().fg(Color::Rgb(100, 140, 180))),
                ]))
            })
            .collect();

        let block_title = if show_finished {
            " Recently Read (all) [f] "
        } else {
            " Recently Read (in progress) [f] "
        };
        let list = List::new(items).block(
            Block::default()
                .title(block_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );
        frame.render_widget(list, outer[1]);
    } else {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Welcome to kotoba!",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("No texts read yet. Get started:"),
            Line::from("  [l] Open Library"),
            Line::from("  [i] Import text (clipboard, URL, file, Syosetu)"),
            Line::from(""),
            Line::from("Or use the CLI:"),
            Line::from("  kotoba import <file>"),
            Line::from("  kotoba syosetu <ncode>"),
        ])
        .block(
            Block::default()
                .title(" Home ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );
        frame.render_widget(msg, outer[1]);
    }

    // Status bar
    let status = Line::from(vec![Span::styled(
        " ↑↓:navigate  Enter:open  r:review  f:toggle finished  l:library  i:import  Tab:reader  q:quit  ?:help ",
        Style::default().fg(Color::DarkGray),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}
