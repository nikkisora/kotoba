use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::components::{furigana, sidebar};

/// Render the reader screen with main text area and sidebar.
pub fn render(frame: &mut Frame, app: &App) {
    let state = match app.reader_state.as_ref() {
        Some(s) => s,
        None => {
            let msg =
                Paragraph::new("No text loaded. Use 'kotoba import <file>' then 'kotoba run'.")
                    .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(msg, frame.size());
            return;
        }
    };

    let area = frame.size();

    // Top bar
    let outer = Layout::vertical([
        Constraint::Length(1), // title bar
        Constraint::Min(3),    // content
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
        Span::raw(" — "),
        Span::styled(&state.text_title, Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    // Content: main text (70%) | sidebar (30%)
    let sidebar_pct = app.config.reader.sidebar_width;
    let content = Layout::horizontal([
        Constraint::Percentage(100 - sidebar_pct),
        Constraint::Percentage(sidebar_pct),
    ])
    .split(outer[1]);

    // Main text area
    render_main_text(frame, app, state, content[0]);

    // Sidebar
    sidebar::render(content[1], frame.buffer_mut(), app);

    // Bottom status bar
    let sentence_info = if !state.sentences.is_empty() {
        format!(
            "Sentence {}/{}",
            state.sentence_index + 1,
            state.sentences.len()
        )
    } else {
        "No sentences".to_string()
    };

    let word_info = match state.word_index {
        Some(i) => {
            let sentence = &state.sentences[state.sentence_index];
            if i < sentence.tokens.len() {
                format!("  Word: {}", sentence.tokens[i].surface)
            } else {
                String::new()
            }
        }
        None => String::new(),
    };

    let autopromote_indicator = if state.autopromote_enabled {
        Span::styled(" [A] ", Style::default().fg(Color::Green))
    } else {
        Span::styled(" [a] ", Style::default().fg(Color::DarkGray))
    };

    let readings_indicator = if state.show_all_readings {
        Span::styled("[R] ", Style::default().fg(Color::Green))
    } else {
        Span::styled("[r] ", Style::default().fg(Color::DarkGray))
    };

    let known_indicator = if state.show_known_in_sidebar {
        Span::styled("[W] ", Style::default().fg(Color::Green))
    } else {
        Span::styled("[w] ", Style::default().fg(Color::DarkGray))
    };

    let translation_indicator = if state
        .sentence_translations
        .contains_key(&state.sentence_index)
    {
        Span::styled("[翻] ", Style::default().fg(Color::Green))
    } else {
        Span::raw("")
    };

    let status = Line::from(vec![
        Span::styled(
            format!(" {} {} ", sentence_info, word_info),
            Style::default().fg(Color::White),
        ),
        autopromote_indicator,
        readings_indicator,
        known_indicator,
        translation_indicator,
        Span::styled(
            " ↑↓:sent ←→:word 1-5:status t:translate T:sent-trans m:expr ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}

/// Render the main text area with all paragraphs and furigana.
fn render_main_text(frame: &mut Frame, app: &App, state: &crate::app::ReaderState, area: Rect) {
    let block = Block::default().borders(Borders::NONE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.sentences.is_empty() || inner.height < 2 {
        return;
    }

    let buf = frame.buffer_mut();

    // We need to render sentences, scrolling so the current sentence is centered.
    // First, compute exact heights of all sentences using the same layout algorithm
    // as render_sentence to find the correct scroll offset.
    let show_furigana = app.config.reader.furigana;

    // Compute exact heights: paragraph gaps and sentence gaps stored separately.
    let mut sentence_heights: Vec<u16> = Vec::new();
    let mut para_gaps: Vec<u16> = Vec::new(); // gap BEFORE each sentence (0 or 1)
    let mut sent_gaps: Vec<u16> = Vec::new(); // spacing gap AFTER each sentence (0 or 1)
    let mut prev_para: Option<usize> = None;

    for (sent_idx, sentence) in state.sentences.iter().enumerate() {
        let gap: u16 = if let Some(pp) = prev_para {
            if sentence.paragraph_idx != pp {
                1
            } else {
                0
            }
        } else {
            0
        };
        para_gaps.push(gap);

        let is_current = sent_idx == state.sentence_index;
        let h = furigana::sentence_height(
            &sentence.tokens,
            inner.width,
            show_furigana,
            is_current,
            false,
        );

        // Add 1-row gap after this sentence when the NEXT sentence does NOT have
        // furigana. A furigana sentence starts with a furigana row (small text)
        // which already provides visual separation. Non-furigana sentences need
        // an explicit gap to avoid text lines being packed together.
        let next_has_furi = if sent_idx + 1 < state.sentences.len() {
            furigana::sentence_has_furigana(
                &state.sentences[sent_idx + 1].tokens,
                show_furigana,
                false,
            )
        } else {
            true // no next sentence → no gap needed
        };
        let spacing: u16 = if !next_has_furi { 1 } else { 0 };
        sent_gaps.push(spacing);

        sentence_heights.push(h);
        prev_para = Some(sentence.paragraph_idx);
    }

    // Find scroll offset to center current sentence
    let total_height: u16 = sentence_heights.iter().sum::<u16>()
        + para_gaps.iter().sum::<u16>()
        + sent_gaps.iter().sum::<u16>();
    let current_y: u16 = sentence_heights[..state.sentence_index].iter().sum::<u16>()
        + para_gaps[..state.sentence_index].iter().sum::<u16>()
        + sent_gaps[..state.sentence_index].iter().sum::<u16>();
    let current_h = sentence_heights[state.sentence_index];

    let target_y = if total_height <= inner.height {
        0 // Everything fits, no scroll
    } else {
        let center = inner.height / 2;
        let ideal = current_y.saturating_sub(center.saturating_sub(current_h / 2));
        ideal.min(total_height.saturating_sub(inner.height))
    };

    // Now render, skipping lines before target_y.
    let mut y_pos: i32 = -(target_y as i32);

    for (sent_idx, sentence) in state.sentences.iter().enumerate() {
        // Paragraph gap before this sentence
        y_pos += para_gaps[sent_idx] as i32;

        let h = sentence_heights[sent_idx];

        // Skip if entirely above view
        if y_pos + h as i32 <= 0 {
            y_pos += h as i32;
            y_pos += sent_gaps[sent_idx] as i32;
            continue;
        }

        // Stop if below view
        if y_pos >= inner.height as i32 {
            break;
        }

        // Prepare tokens with selection state.
        // Expression marking mode: highlight the entire marked range.
        // Normal mode: when a group head is selected, highlight all group members.
        let mut display_tokens = sentence.tokens.clone();
        if sent_idx == state.sentence_index {
            if let Some((mark_start, mark_end)) = state.expression_mark {
                // Expression marking mode: highlight the range
                for idx in mark_start..=mark_end {
                    if idx < display_tokens.len() {
                        display_tokens[idx].is_selected = true;
                    }
                }
            } else if let Some(wi) = state.word_index {
                if wi < display_tokens.len() {
                    let selected_group = display_tokens[wi].group_id;
                    if let Some(gid) = selected_group {
                        // Highlight all tokens in this group
                        for tok in &mut display_tokens {
                            if tok.group_id == Some(gid) {
                                tok.is_selected = true;
                            }
                        }
                    } else {
                        // Standalone token
                        display_tokens[wi].is_selected = true;
                    }
                }
            }
        }

        let render_y = y_pos.max(0) as u16;
        let available_height = (inner.height).saturating_sub(render_y);

        if available_height > 0 {
            let render_area = Rect {
                x: inner.x,
                y: inner.y + render_y,
                width: inner.width,
                height: available_height,
            };

            let is_current = sent_idx == state.sentence_index;
            furigana::render_sentence(
                &display_tokens,
                render_area,
                buf,
                show_furigana,
                is_current,
                false,
            );
        }

        y_pos += h as i32;

        // Add sentence spacing gap (for non-furigana sentences)
        y_pos += sent_gaps[sent_idx] as i32;
    }
}
