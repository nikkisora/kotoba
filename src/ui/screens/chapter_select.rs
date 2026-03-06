use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, ChapterReadState};

/// Render the chapter select screen.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();

    let state = match app.chapter_select_state.as_ref() {
        Some(s) => s,
        None => {
            // Should not happen but handle gracefully
            super::placeholder::render(frame, "Chapters", "No source loaded");
            return;
        }
    };

    let outer = Layout::vertical([
        Constraint::Length(1), // title bar
        Constraint::Length(4), // source info (2 border + 2 content)
        Constraint::Min(3),    // chapter list
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
        Span::raw(" — Chapters"),
        Span::raw("  "),
        Span::styled(
            format!("page {}/{}", state.page + 1, state.total_pages()),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    // Source info
    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                &state.source.title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("[{}]", state.source.source_type),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![Span::raw(format!(
            "  {} chapters  •  {} imported  •  {} skipped",
            state.total_chapters, state.total_imported, state.total_skipped,
        ))]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );
    frame.render_widget(info, outer[1]);

    // Loading state — if loading AND no chapters yet, show full-screen loading
    if state.loading && state.chapters.is_empty() {
        let spinner = app.spinner_char();
        let loading_msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {} Fetching novel info from Syosetu...", spinner),
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  This may take a moment for novels with many chapters.",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .title(" Chapters ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );
        frame.render_widget(loading_msg, outer[2]);

        let status = Line::from(vec![Span::styled(
            " Esc:back  q:quit ",
            Style::default().fg(Color::DarkGray),
        )]);
        frame.render_widget(
            Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
            outer[3],
        );
        return;
    }

    // Chapter list (paginated)
    let visible = state.visible_chapters();
    let page_offset = state.page * state.page_size;

    // Build list items, inserting group headers when the group name changes
    let mut items: Vec<ListItem> = Vec::new();
    let mut last_group: Option<&str> = None;

    for (i, ch) in visible.iter().enumerate() {
        // Insert group header if the group changed
        if !ch.chapter_group.is_empty() {
            let show_header = match last_group {
                Some(prev) => prev != ch.chapter_group,
                None => true,
            };
            if show_header {
                items.push(ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("── {} ──", ch.chapter_group),
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])));
            }
            last_group = Some(&ch.chapter_group);
        } else if last_group.is_some() && ch.chapter_group.is_empty() {
            // Transitioning from a group to ungrouped
            last_group = None;
        }

        let global_idx = page_offset + i;
        let is_selected = global_idx == state.selected;
        let marker = if is_selected { "▶ " } else { "  " };

        let is_preprocessing = app.preprocessing_chapters.contains(&ch.id);
        let read_state = state.chapter_read_states.get(&ch.id).copied();

        // 5 distinct states:
        // S (red/dim)      = Skipped
        // ⠋ (yellow)       = Preprocessing (animated spinner)
        // — (dark gray)    = Not imported
        // ○ (white)        = Imported, unread
        // ◐ (blue)         = In progress
        // ● (green)        = Finished
        let spinner_str = app.spinner_char().to_string();
        let progress_info = app.preprocessing_progress.get(&ch.id).copied();
        let (status_icon, status_style) = if ch.is_skipped {
            (
                "S".to_string(),
                Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
            )
        } else if is_preprocessing {
            let pct_str = match progress_info {
                Some((_phase, pct)) => format!("{} {}%", spinner_str, pct),
                None => spinner_str,
            };
            (pct_str, Style::default().fg(Color::Yellow))
        } else {
            match read_state {
                Some(ChapterReadState::Finished) => {
                    ("●".to_string(), Style::default().fg(Color::Green))
                }
                Some(ChapterReadState::InProgress) => (
                    "◐".to_string(),
                    Style::default().fg(Color::Rgb(100, 150, 255)),
                ),
                Some(ChapterReadState::Unread) => {
                    ("○".to_string(), Style::default().fg(Color::White))
                }
                _ => ("—".to_string(), Style::default().fg(Color::DarkGray)),
            }
        };

        let base_style = if ch.is_skipped {
            Style::default().fg(Color::DarkGray)
        } else if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            match read_state {
                Some(ChapterReadState::Finished) => Style::default().fg(Color::Green),
                Some(ChapterReadState::InProgress) => {
                    Style::default().fg(Color::Rgb(100, 150, 255))
                }
                _ => Style::default(),
            }
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(marker, base_style),
            Span::styled(format!("[{}] ", &status_icon), status_style),
            Span::styled(
                format!("{:>4}. ", ch.chapter_number),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(&ch.title, base_style),
        ])));
    }

    let list_title = if state.loading {
        format!(
            " Chapters ({}) — Page {}/{} {} Loading more... ",
            state.total_chapters,
            state.page + 1,
            state.total_pages(),
            app.spinner_char(),
        )
    } else {
        format!(
            " Chapters ({}) — Page {}/{} ",
            state.total_chapters,
            state.page + 1,
            state.total_pages()
        )
    };
    let list = List::new(items).block(
        Block::default()
            .title(list_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );
    frame.render_widget(list, outer[2]);

    // Status bar
    let status_text = if app.pending_open_chapter.is_some() {
        format!(
            " {} Importing chapter... will open when ready  Esc:cancel ",
            app.spinner_char()
        )
    } else {
        " ↑↓:nav  Enter:open  x:skip  P:preprocess  p/n:page  Esc:back  Tab:reader  q:quit  ?:help "
            .to_string()
    };
    let status = Line::from(vec![Span::styled(
        status_text,
        Style::default().fg(Color::DarkGray),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2 + 1], // index 3
    );
}
