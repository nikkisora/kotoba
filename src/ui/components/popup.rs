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
            frame.render_widget(Clear, padded_rect(area, frame.size()));

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
                        let pos_tags: Vec<&str> = sense
                            .pos
                            .iter()
                            .map(|s| s.as_str())
                            .filter(|s| !s.is_empty())
                            .collect();
                        let pos = if pos_tags.is_empty() {
                            String::new()
                        } else {
                            format!("[{}] ", pos_tags.join(", "))
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

        PopupState::Help { scroll } => {
            let area = centered_rect(65, 85, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let heading = Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD);

            let lines = vec![
                Line::from(Span::styled(
                    "Keybindings & Commands",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                // ── CLI Commands ──
                Line::from(Span::styled("CLI Commands:", heading)),
                Line::from("  kotoba run                   Launch the TUI"),
                Line::from("  kotoba setup-dict            Download & set up JMdict"),
                Line::from("  kotoba import-dict <path>    Import JMdict XML manually"),
                Line::from("  kotoba import <file>         Import .txt/.srt/.ass/.epub"),
                Line::from("  kotoba import --clipboard    Import from clipboard"),
                Line::from("  kotoba import --url <URL>    Import from a web URL"),
                Line::from("  kotoba syosetu <ncode>       Import Syosetu novel"),
                Line::from("  kotoba syosetu <ncode> -c N  Import specific chapter"),
                Line::from("  kotoba dict <word>           Look up a word in JMdict"),
                Line::from("  kotoba tokenize <text>       Tokenize Japanese text"),
                Line::from(""),
                // ── Global ──
                Line::from(Span::styled("Global:", heading)),
                Line::from("  q / Ctrl+C   Quit"),
                Line::from("  Tab          Toggle Reader <-> previous screen"),
                Line::from("  ?            Toggle this help"),
                Line::from(""),
                // ── Home ──
                Line::from(Span::styled("Home:", heading)),
                Line::from("  Up/k Down/j  Navigate recent texts"),
                Line::from("  Enter        Open selected text in Reader"),
                Line::from("  l            Go to Library"),
                Line::from("  r            Go to Review"),
                Line::from("  i            Import menu"),
                Line::from("  f            Toggle show finished texts"),
                Line::from(""),
                // ── Library ──
                Line::from(Span::styled("Library:", heading)),
                Line::from("  Up/k Down/j  Navigate texts"),
                Line::from("  Enter        Open text / chapter select"),
                Line::from("  d            Delete selected text"),
                Line::from("  i            Import menu"),
                Line::from("  /            Search texts by title"),
                Line::from("  s            Cycle sort mode"),
                Line::from("  f            Cycle source type filter"),
                Line::from("  Esc          Back to Home"),
                Line::from(""),
                // ── Chapter Select ──
                Line::from(Span::styled("Chapter Select:", heading)),
                Line::from("  Up/k Down/j  Navigate chapters"),
                Line::from("  n / PgDn     Next page"),
                Line::from("  p / PgUp     Previous page"),
                Line::from("  Enter        Open selected chapter"),
                Line::from("  x            Toggle skip/unskip chapter"),
                Line::from("  P            Preprocess upcoming chapters"),
                Line::from("  Esc          Back to Library"),
                Line::from(""),
                // ── Reader ──
                Line::from(Span::styled("Reader:", heading)),
                Line::from("  Up/k Down/j  Previous / next sentence"),
                Line::from("  Left/h       Previous word"),
                Line::from("  Right/l      Next word"),
                Line::from("  1-4          Set Learning status 1-4"),
                Line::from("  5            Set Known"),
                Line::from("  i            Set Ignored"),
                Line::from("  Enter        Word detail (dictionary lookup)"),
                Line::from("  n            Edit word note"),
                Line::from("  c            Copy selected word to clipboard"),
                Line::from("  C            Copy current sentence to clipboard"),
                Line::from("  m            Mark expression (MWE mode)"),
                Line::from("  w            Toggle Known/Ignored in sidebar"),
                Line::from("  r            Toggle all readings in sidebar"),
                Line::from("  a            Toggle autopromotion"),
                Line::from("  Ctrl+Z       Undo last autopromotion"),
                Line::from("  Esc          Deselect word / back"),
                Line::from(""),
                // ── Expression Marking ──
                Line::from(Span::styled("Expression Marking (after m):", heading)),
                Line::from("  Left/h       Extend selection left"),
                Line::from("  Right/l      Extend selection right"),
                Line::from("  Enter        Save expression"),
                Line::from("  Esc          Cancel"),
                Line::from(""),
                // ── Import Menu ──
                Line::from(Span::styled("Import Menu:", heading)),
                Line::from("  c            Import from clipboard"),
                Line::from("  u            Import from URL"),
                Line::from("  f            Import from file"),
                Line::from("  s            Import from Syosetu"),
                Line::from("  Esc          Cancel"),
                Line::from(""),
                Line::from(Span::styled(
                    "↑/↓ to scroll • Esc or ? to close",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let block = Block::default()
                .title(Span::styled(
                    " Help ",
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

        PopupState::NoteEditor { text, .. } => {
            let area = centered_rect(50, 30, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

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
            frame.render_widget(Clear, padded_rect(area, frame.size()));

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

        PopupState::DeleteConfirm { title, .. } => {
            let area = centered_rect(50, 20, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Delete this text?",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    title.as_str(),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("This will remove the text and all its tokens."),
                Line::from(""),
                Line::from("  y — Yes, delete"),
                Line::from("  n — No, cancel"),
            ];

            let block = Block::default()
                .title(" Confirm Delete ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::DeleteSourceConfirm { title, .. } => {
            let area = centered_rect(55, 25, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Delete this source?",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    title.as_str(),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("This will delete the source, all chapters,"),
                Line::from("and all imported chapter texts."),
                Line::from(""),
                Line::from("  y — Yes, delete"),
                Line::from("  n — No, cancel"),
            ];

            let block = Block::default()
                .title(" Confirm Delete Source ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::ImportMenu => {
            let area = centered_rect(45, 30, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Import Source:",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("  c — Clipboard"),
                Line::from("  u — URL (web page)"),
                Line::from("  f — File (text / .srt / .ass / .epub)"),
                Line::from("  s — Syosetu novel (ncode)"),
                Line::from(""),
                Line::from(Span::styled(
                    "Esc to cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let block = Block::default()
                .title(" Import ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::UrlInput { text } => {
            let area = centered_rect(60, 20, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Enter URL to import:",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("▎{}_", text),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter to import • Esc to cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let block = Block::default()
                .title(" URL Import ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::FilePathInput { text } => {
            let area = centered_rect(60, 20, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Enter file path (.txt / .srt / .ass / .epub):",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("▎{}_", text),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter to import • Esc to cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let block = Block::default()
                .title(" File Import ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::SyosetuInput { text } => {
            let area = centered_rect(60, 20, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Enter Syosetu ncode or URL:",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("▎{}_", text),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter to load novel • Esc to cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let block = Block::default()
                .title(" Syosetu Import ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::ExpressionTranslation {
            surface,
            reading,
            gloss,
        } => {
            let area = centered_rect(55, 35, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Expression: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(surface.as_str(), Style::default().fg(Color::Cyan)),
            ]));
            if !reading.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Reading:    ", Style::default().fg(Color::DarkGray)),
                    Span::styled(reading.as_str(), Style::default().fg(Color::Green)),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Translation:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!("▎{}_", gloss),
                Style::default().fg(Color::Cyan),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Type to edit • Enter to save • Esc to cancel",
                Style::default().fg(Color::DarkGray),
            )));

            let block = Block::default()
                .title(Span::styled(
                    " Expression Translation ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }

        PopupState::SearchInput { text } => {
            let area = centered_rect(50, 15, frame.size());
            frame.render_widget(Clear, padded_rect(area, frame.size()));

            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Search texts by title:",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("▎{}_", text),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter to search • Esc to cancel/reset",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let block = Block::default()
                .title(" Search ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }
    }
}

/// Expand a rect by 1 cell on each side to clear wide CJK characters that straddle the border.
/// Clamped so it never exceeds the terminal bounds.
fn padded_rect(area: Rect, screen: Rect) -> Rect {
    let x = area.x.saturating_sub(1).max(screen.x);
    let y = area.y.saturating_sub(1).max(screen.y);
    let right = (area.x + area.width + 1).min(screen.x + screen.width);
    let bottom = (area.y + area.height + 1).min(screen.y + screen.height);
    Rect::new(x, y, right - x, bottom - y)
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
