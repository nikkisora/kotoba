use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Source type display icon/label.
fn source_icon(source_type: &str) -> &str {
    match source_type {
        "text" => "📄",
        "clipboard" => "📋",
        "web" => "🌐",
        "syosetsu" => "📖",
        "subtitle" => "🎬",
        "epub" => "📚",
        _ => "📝",
    }
}

/// Render the library screen showing imported texts.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();

    let outer = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(3),   // content
        Constraint::Length(1), // status
    ])
    .split(area);

    let lib = app.library_state.as_ref();
    let texts = lib.map(|l| &l.texts);
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
    match texts {
        Some(texts) if !texts.is_empty() => {
            let stats_map = lib.map(|l| &l.stats);

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
                    let icon = source_icon(&t.source_type);

                    // Build stats string
                    let stats_str = stats_map
                        .and_then(|m| m.get(&t.id))
                        .map(|s| {
                            let pct = if s.unique_vocab == 0 {
                                0
                            } else {
                                (s.known_count * 100) / s.unique_vocab
                            };
                            format!(
                                "  {}w  K:{} L:{} N:{}  {}%",
                                s.total_tokens, s.known_count, s.learning_count, s.new_count, pct
                            )
                        })
                        .unwrap_or_default();

                    ListItem::new(Line::from(vec![
                        Span::styled(marker, style),
                        Span::raw(format!("{} ", icon)),
                        Span::styled(&t.title, style),
                        Span::styled(
                            format!("  [{}]  {}", t.source_type, &t.created_at[..10.min(t.created_at.len())]),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            stats_str,
                            Style::default().fg(Color::Rgb(100, 140, 180)),
                        ),
                    ]))
                })
                .collect();

            let count_label = format!(" Imported Texts ({}) ", texts.len());
            let list = List::new(items).block(
                Block::default()
                    .title(count_label)
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
                Line::from("Import a text:"),
                Line::from("  kotoba import <file>          — text, .srt, .ass, .epub"),
                Line::from("  kotoba import --clipboard     — from clipboard"),
                Line::from("  kotoba import --url <URL>     — from web page"),
                Line::from("  kotoba syosetsu <ncode>       — from Syosetsu novel"),
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
    }

    // Status bar
    let status = Line::from(vec![
        Span::styled(
            " ↑↓:navigate  Enter:open  d:delete  i:import  /:search  s:sort  f:filter  Tab:screens  q:quit ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}
