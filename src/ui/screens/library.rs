use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, LibraryItem};

/// Source type display icon/label.
fn source_icon(source_type: &str) -> &str {
    match source_type {
        "text" => "📄",
        "clipboard" => "📋",
        "web" => "🌐",
        "syosetu" => "📖",
        "subtitle" => "🎬",
        "epub" => "📚",
        _ => "📝",
    }
}

/// Render a mini progress bar.
fn progress_bar(current: u64, total: u64, width: usize) -> String {
    if total == 0 {
        return "░".repeat(width);
    }
    let filled = ((current as f64 / total as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// Render the library screen showing imported texts and sources.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();

    let outer = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(3),    // content
        Constraint::Length(1), // status
    ])
    .split(area);

    let lib = app.library_state.as_ref();
    let items = lib.map(|l| &l.items);
    let selected = lib.map(|l| l.selected).unwrap_or(0);
    let sort_label = lib.map(|l| l.sort.label()).unwrap_or("Date ↓");
    let filter_label = lib
        .and_then(|l| l.filter_source.as_deref())
        .unwrap_or("all");

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
        Span::styled(
            format!("sort:{}", sort_label),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("filter:{}", filter_label),
            Style::default().fg(Color::Green),
        ),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    // Content
    let has_pending = !app.pending_imports.is_empty();
    let has_items = items.map(|i| !i.is_empty()).unwrap_or(false);

    match items {
        Some(items) if has_items || has_pending => {
            let stats_map = lib.map(|l| &l.stats);
            let chapter_counts = lib.map(|l| &l.source_chapter_counts);

            // Pending imports shown at top with spinner
            let spinner = app.spinner_char();
            let mut list_items: Vec<ListItem> = app
                .pending_imports
                .iter()
                .map(|label| {
                    ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(format!("{} ", spinner), Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!("Importing: {}", label),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]))
                })
                .collect();

            // Regular library items
            list_items.extend(items.iter().enumerate().map(|(i, item)| {
                let marker = if i == selected { "▶ " } else { "  " };
                let style = if i == selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let icon = source_icon(item.source_type());

                let detail_str = match item {
                    LibraryItem::Text(t) => {
                        let progress = (t.last_sentence_index + 1).min(t.total_sentences);
                        let pbar = progress_bar(progress as u64, t.total_sentences as u64, 10);
                        let pct = if t.total_sentences > 0 {
                            (progress * 100 / t.total_sentences) as u8
                        } else {
                            0
                        };

                        let word_stats = stats_map
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
                                format!(" K:{}% L:{}% N:{}%", kpct, lpct, npct)
                            })
                            .unwrap_or_default();

                        format!("  {} {}%{}", pbar, pct, word_stats)
                    }
                    LibraryItem::Source(ws) => {
                        let counts = chapter_counts
                            .and_then(|m| m.get(&ws.id))
                            .copied()
                            .unwrap_or((0, 0, 0));
                        format!("  ({} ch, {} imported)", counts.0, counts.1)
                    }
                };

                let date_str = &item.created_at()[..10.min(item.created_at().len())];

                ListItem::new(Line::from(vec![
                    Span::styled(marker, style),
                    Span::raw(format!("{} ", icon)),
                    Span::styled(item.title(), style),
                    Span::styled(
                        format!("  [{}]  {}", item.source_type(), date_str),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(detail_str, Style::default().fg(Color::Rgb(100, 140, 180))),
                ]))
            }));

            let count_label = format!(" Library ({}) ", items.len());
            let list = List::new(list_items).block(
                Block::default()
                    .title(count_label)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            );
            frame.render_widget(list, outer[1]);
        }
        _ if !has_pending => {
            let msg = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No texts imported yet.",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from("Import a text:"),
                Line::from("  kotoba import <file>          — text, .srt, .ass, .epub"),
                Line::from("  kotoba import --clipboard     — from clipboard"),
                Line::from("  kotoba import --url <URL>     — from web page"),
                Line::from("  kotoba syosetu <ncode>       — from Syosetu novel"),
                Line::from(""),
                Line::from("Or press [i] here to import from clipboard or URL."),
            ])
            .block(
                Block::default()
                    .title(" Library ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            );
            frame.render_widget(msg, outer[1]);
        }
        _ => {} // has_pending with no items — handled by first arm
    }

    // Status bar
    let status = Line::from(vec![
        Span::styled(
            " ↑↓:navigate  Enter:open  d:delete  i:import  /:search  s:sort  f:filter  Esc:home  Tab:reader  q:quit ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}
