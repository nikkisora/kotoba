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

    let status = Line::from(vec![
        Span::styled(
            format!(" {} {} ", sentence_info, word_info),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            " ↑↓:sentence ←→:word 1-5:status i:ignore Enter:detail ",
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
    // First, compute heights of all sentences to find scroll offset.
    let show_furigana = app.config.reader.furigana;

    // Simple approach: render sentences sequentially, with a virtual Y position.
    // Determine which sentence to start rendering from to center the current one.

    // Estimate: each sentence takes ~2 lines (furigana + text) + 1 for paragraph gap.
    // For proper centering, we calculate cumulative heights.

    let mut sentence_heights: Vec<u16> = Vec::new();
    let mut prev_para: Option<usize> = None;

    for sentence in &state.sentences {
        let mut h: u16 = 0;
        // Paragraph gap
        if let Some(pp) = prev_para {
            if sentence.paragraph_idx != pp {
                h += 1; // blank line between paragraphs
            }
        }
        // Estimate height: furigana doubles line count, plus wrapping
        let has_kanji_tokens = sentence
            .tokens
            .iter()
            .any(|t| !t.is_trivial && !t.reading.is_empty() && t.surface != t.reading);
        let base_height: u16 = if show_furigana && has_kanji_tokens {
            2
        } else {
            1
        };

        // Estimate wrapping
        let total_width: usize = sentence
            .tokens
            .iter()
            .map(|t| unicode_width::UnicodeWidthStr::width(t.surface.as_str()))
            .sum();
        let line_count =
            ((total_width as u16).max(1) + inner.width.saturating_sub(3)) / inner.width.max(1);
        h += line_count.max(1) * base_height;

        sentence_heights.push(h);
        prev_para = Some(sentence.paragraph_idx);
    }

    // Find scroll offset to center current sentence
    let total_height: u16 = sentence_heights.iter().sum();
    let current_y: u16 = sentence_heights[..state.sentence_index].iter().sum();
    let current_h = sentence_heights[state.sentence_index];

    let target_y = if total_height <= inner.height {
        0 // Everything fits, no scroll
    } else {
        let center = inner.height / 2;
        let ideal = current_y.saturating_sub(center.saturating_sub(current_h / 2));
        ideal.min(total_height.saturating_sub(inner.height))
    };

    // Now render, skipping lines before target_y
    let mut y_pos: i32 = -(target_y as i32);
    prev_para = None;

    for (sent_idx, sentence) in state.sentences.iter().enumerate() {
        // Paragraph gap
        if let Some(pp) = prev_para {
            if sentence.paragraph_idx != pp {
                y_pos += 1;
            }
        }
        prev_para = Some(sentence.paragraph_idx);

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

        // Prepare tokens with selection state
        let mut display_tokens = sentence.tokens.clone();
        if sent_idx == state.sentence_index {
            if let Some(wi) = state.word_index {
                if wi < display_tokens.len() {
                    display_tokens[wi].is_selected = true;
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
            furigana::render_sentence(&display_tokens, render_area, buf, show_furigana, is_current);
        }

        y_pos += h as i32;
    }
}
