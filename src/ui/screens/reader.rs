use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
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
                    .style(Style::default().fg(app.theme.muted));
            frame.render_widget(msg, frame.size());
            return;
        }
    };

    let area = frame.size();
    let t = &app.theme;

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
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — "),
        Span::styled(&state.text_title, Style::default().fg(t.title_bar_fg)),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(t.muted)),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(t.title_bar_bg)),
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
        Span::styled(" [A] ", Style::default().fg(t.success))
    } else {
        Span::styled(" [a] ", Style::default().fg(t.muted))
    };

    let readings_indicator = if state.show_all_readings {
        Span::styled("[R] ", Style::default().fg(t.success))
    } else {
        Span::styled("[r] ", Style::default().fg(t.muted))
    };

    let known_indicator = if state.show_known_in_sidebar {
        Span::styled("[W] ", Style::default().fg(t.success))
    } else {
        Span::styled("[w] ", Style::default().fg(t.muted))
    };

    let translation_indicator = if state
        .sentence_translations
        .contains_key(&state.sentence_index)
    {
        Span::styled("[翻] ", Style::default().fg(t.success))
    } else {
        Span::raw("")
    };

    let status = Line::from(vec![
        Span::styled(
            format!(" {} {} ", sentence_info, word_info),
            Style::default().fg(t.title_bar_fg),
        ),
        autopromote_indicator,
        readings_indicator,
        known_indicator,
        translation_indicator,
        Span::styled(
            " ↑↓:sent ←→:word 1-5:status t:translate g:jisho G:browser m:expr ",
            Style::default().fg(t.muted),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(t.title_bar_bg)),
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

    // Compute exact heights and a single gap BEFORE each sentence.
    // - First sentence: no gap.
    // - Paragraph boundary: always 1-row gap (visual paragraph break).
    // - Same paragraph: 1-row gap only if sentence_gaps is enabled.
    let sentence_gaps = app.config.reader.sentence_gaps;
    let mut sentence_heights: Vec<u16> = Vec::new();
    let mut gaps: Vec<u16> = Vec::new(); // gap BEFORE each sentence
    let mut prev_para: Option<usize> = None;

    for (sent_idx, sentence) in state.sentences.iter().enumerate() {
        let gap: u16 = if let Some(pp) = prev_para {
            if sentence.paragraph_idx != pp {
                1 // paragraph break — always 1 row
            } else if sentence_gaps {
                1 // intra-paragraph gap when enabled
            } else {
                0
            }
        } else {
            0
        };
        gaps.push(gap);

        let is_current = sent_idx == state.sentence_index;
        let h = furigana::sentence_height(
            &sentence.tokens,
            inner.width,
            show_furigana,
            is_current,
            false,
        );

        sentence_heights.push(h);
        prev_para = Some(sentence.paragraph_idx);
    }

    // Find scroll offset to center current sentence
    let total_height: u16 = sentence_heights.iter().sum::<u16>() + gaps.iter().sum::<u16>();
    let current_y: u16 = sentence_heights[..state.sentence_index].iter().sum::<u16>()
        + gaps[..state.sentence_index].iter().sum::<u16>();
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
        // Gap before this sentence
        y_pos += gaps[sent_idx] as i32;

        let h = sentence_heights[sent_idx];

        // Skip if entirely above view
        if y_pos + h as i32 <= 0 {
            y_pos += h as i32;
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
                &app.theme,
            );
        }

        y_pos += h as i32;
    }
}
