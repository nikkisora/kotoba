use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
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
    let t = &app.theme;

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
                Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };
    let mut title_spans = vec![
        Span::styled(
            " kotoba",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Home"),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(t.muted)),
    ];
    title_spans.extend(review_indicator);
    let title = Line::from(title_spans);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(t.title_bar_bg)),
        outer[0],
    );

    // Dict warning banner
    let dict_loaded = home.map(|h| h.dict_loaded).unwrap_or(true);
    let content_area = if !dict_loaded {
        let banner_split = Layout::vertical([
            Constraint::Length(2), // warning banner
            Constraint::Min(1),    // remaining content
        ])
        .split(outer[1]);

        let banner = Paragraph::new(Line::from(vec![
            Span::styled(
                " ! ",
                Style::default().fg(t.error).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Dictionary not loaded. Run ",
                Style::default().fg(t.warning),
            ),
            Span::styled(
                "kotoba setup-dict",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to download and import JMdict for word lookups.",
                Style::default().fg(t.warning),
            ),
        ]))
        .style(Style::default().bg(t.title_bar_bg));
        frame.render_widget(banner, banner_split[0]);
        banner_split[1]
    } else {
        outer[1]
    };

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
            .map(|(i, text)| {
                let marker = if i == selected { "▶ " } else { "  " };
                let style = if i == selected {
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let progress = (text.last_sentence_index + 1).min(text.total_sentences);
                let pbar = progress_bar(progress as u64, text.total_sentences as u64, 12);
                let pct = if text.total_sentences > 0 {
                    (progress * 100 / text.total_sentences) as u8
                } else {
                    0
                };

                let stats_str = stats_map
                    .and_then(|m| m.get(&text.id))
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
                    Span::styled(&text.title, style),
                    Span::raw("  "),
                    Span::styled(
                        format!("{} {}%", pbar, pct),
                        Style::default().fg(t.progress_bar),
                    ),
                    Span::styled(stats_str, Style::default().fg(t.stats_text)),
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
                .border_style(Style::default().fg(t.info)),
        );
        frame.render_widget(list, content_area);
    } else {
        let mut welcome_lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Welcome to kotoba!",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if !dict_loaded {
            welcome_lines.push(Line::from(Span::styled(
                "Set up the dictionary first:",
                Style::default().fg(t.warning),
            )));
            welcome_lines.push(Line::from("  kotoba setup-dict"));
            welcome_lines.push(Line::from(""));
        }

        welcome_lines.extend([
            Line::from("Get started:"),
            Line::from("  [l] Open Library"),
            Line::from("  [i] Import text (clipboard, URL, file, Syosetu)"),
            Line::from(""),
            Line::from("Or use the CLI:"),
            Line::from("  kotoba import <file>"),
            Line::from("  kotoba syosetu <ncode>"),
        ]);

        let msg = Paragraph::new(welcome_lines).block(
            Block::default()
                .title(" Home ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(t.info)),
        );
        frame.render_widget(msg, content_area);
    }

    // Status bar
    let status = Line::from(vec![Span::styled(
        " ↑↓:navigate  Enter:open  r:review  c:cards  s:settings  f:finished  l:library  i:import  Tab:reader  q:quit ",
        Style::default().fg(t.muted),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(t.title_bar_bg)),
        outer[2],
    );
}
