use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, ReviewCardData, ReviewPhase, ReviewState, TokenDisplay};
use crate::db::models::{AnswerMode, VocabularyStatus};
use crate::ui::theme::Theme;

/// Render the review screen.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();
    let state = match app.review_state.as_ref() {
        Some(s) => s,
        None => {
            let msg = Paragraph::new("No review session active. Press Esc to go back.")
                .alignment(Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }
    };

    let t = &app.theme;

    // Layout: title bar (1) + main content + status bar (1)
    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(area);

    render_title_bar(frame, state, outer[0], t);

    // If LLM analysis is available, split the main area to show it in a side panel
    let has_llm = state.llm_analysis.is_some();
    let main_area = if has_llm {
        let split = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer[1]);
        // Render LLM analysis in the right panel
        if let Some(ref analysis) = state.llm_analysis {
            crate::ui::components::sidebar::render_review_llm_analysis(
                split[1],
                frame.buffer_mut(),
                analysis,
                t,
            );
        }
        split[0]
    } else {
        outer[1]
    };

    match state.phase {
        ReviewPhase::PreSession => render_pre_session(frame, state, main_area, t),
        ReviewPhase::ShowFront => render_card_front(frame, state, main_area, t),
        ReviewPhase::ShowBack => render_card_back(frame, state, main_area, t),
        ReviewPhase::TypingAnswer => render_typing(frame, state, main_area, t),
        ReviewPhase::ShowResult => render_typed_result(frame, state, main_area, t),
        ReviewPhase::SessionSummary => render_summary(frame, state, main_area, t),
    }

    render_status_bar(frame, state, outer[2], t);
}

fn render_title_bar(frame: &mut Frame, state: &ReviewState, area: Rect, t: &Theme) {
    let progress = if state.queue.is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", state.current_index + 1, state.queue.len())
    };
    let title = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Review "),
        Span::styled(format!("[{}]", progress), Style::default().fg(t.warning)),
        Span::raw(format!(
            "  Reviewed: {} | Correct: {}%",
            state.total_reviewed,
            if state.total_reviewed > 0 {
                (state.correct_count * 100) / state.total_reviewed
            } else {
                0
            }
        )),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(t.title_bar_bg)),
        area,
    );
}

fn render_status_bar(frame: &mut Frame, state: &ReviewState, area: Rect, t: &Theme) {
    let hints = match state.phase {
        ReviewPhase::PreSession => " Space/Enter:start  Esc:back  q:quit ",
        ReviewPhase::ShowFront => " Space:reveal  Esc:back  q:quit ",
        ReviewPhase::ShowBack => " 1:Again 2:Hard 3/Space:Good 4:Easy  Esc:back ",
        ReviewPhase::TypingAnswer => " Enter:submit  Esc:skip ",
        ReviewPhase::ShowResult => " 1:Again 2:Hard 3:Good 4:Easy  Enter:accept auto-rating ",
        ReviewPhase::SessionSummary => " Enter/Esc:back  r:continue ",
    };
    let status = Line::from(Span::styled(hints, Style::default().fg(t.muted)));
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(t.title_bar_bg)),
        area,
    );
}

fn render_pre_session(frame: &mut Frame, state: &ReviewState, area: Rect, t: &Theme) {
    let center = centered_rect(60, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.accent))
        .title(" Review Session ");

    let inner = block.inner(center);
    frame.render_widget(block, center);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {} cards due", state.total_due),
            Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("  {} new cards", state.total_new),
            Style::default().fg(t.accent),
        )),
        Line::from(Span::styled(
            format!("  {} cards loaded for this session", state.queue.len()),
            Style::default().fg(t.fg),
        )),
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            "  Press Space or Enter to begin",
            Style::default().fg(t.success).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_card_front(frame: &mut Frame, state: &ReviewState, area: Rect, t: &Theme) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    let card_area = centered_rect(70, 80, area);

    // Split into card display + sentence context
    let sections =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(card_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.info))
        .title(format!(" {} ", card_data.answer_mode.label()));
    let inner = block.inner(sections[0]);
    frame.render_widget(block, sections[0]);

    match card_data.answer_mode {
        AnswerMode::WordReview => {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    &card_data.display_surface,
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ))
                .alignment(Alignment::Center),
                Line::from(""),
                Line::from(Span::styled(
                    "Recall the reading and meaning",
                    Style::default().fg(t.muted),
                ))
                .alignment(Alignment::Center),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Space to reveal",
                    Style::default().fg(t.muted),
                ))
                .alignment(Alignment::Center),
            ];
            frame.render_widget(Paragraph::new(lines), inner);
        }
        AnswerMode::SentenceCloze => {
            let cloze_spans = build_cloze_spans(card_data, true, t);
            let lines = vec![
                Line::from(""),
                Line::from("  Fill in the blank:").alignment(Alignment::Left),
                Line::from(""),
                Line::from(cloze_spans),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Space to reveal",
                    Style::default().fg(t.muted),
                ))
                .alignment(Alignment::Center),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        AnswerMode::SentenceFull => {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Translate this sentence:",
                    Style::default().fg(t.muted),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("  {}", card_data.sentence_text),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Space to reveal translation",
                    Style::default().fg(t.muted),
                ))
                .alignment(Alignment::Center),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }

    // Sentence context below — blank the target word for cloze cards
    let blank = card_data.answer_mode == AnswerMode::SentenceCloze;
    render_sentence_context(frame, state, sections[1], blank, t);
}

fn render_card_back(frame: &mut Frame, state: &ReviewState, area: Rect, t: &Theme) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    let card_area = centered_rect(70, 80, area);

    let sections =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(card_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.success))
        .title(format!(" {} — Answer ", card_data.answer_mode.label()));
    let inner = block.inner(sections[0]);
    frame.render_widget(block, sections[0]);

    match card_data.answer_mode {
        AnswerMode::WordReview | AnswerMode::SentenceCloze => {
            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        &card_data.display_surface,
                        Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("({})", card_data.display_reading),
                        Style::default().fg(t.accent),
                    ),
                ]),
                Line::from(""),
            ];

            if card_data.answer_mode == AnswerMode::SentenceCloze {
                let cloze_spans = build_cloze_spans(card_data, false, t);
                lines.push(Line::from(cloze_spans));
                lines.push(Line::from(""));
            }

            if let Some(ref translation) = card_data.vocabulary.translation {
                if !translation.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  ★ ", Style::default().fg(t.warning)),
                        Span::styled(
                            translation.as_str(),
                            Style::default().fg(t.success).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    lines.push(Line::from(""));
                }
            }

            for entry in &card_data.definitions {
                for (i, sense) in entry.senses.iter().enumerate() {
                    let glosses = sense.glosses.join("; ");
                    let pos = if sense.pos.is_empty() {
                        String::new()
                    } else {
                        format!("[{}] ", sense.pos.join(", "))
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {}. ", i + 1), Style::default().fg(t.muted)),
                        Span::styled(pos, Style::default().fg(t.warning)),
                        Span::raw(glosses),
                    ]));
                }
            }

            if card_data.definitions.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (No dictionary entry found)",
                    Style::default().fg(t.muted),
                )));
            }

            lines.push(Line::from(""));
            lines.push(rating_hint_line(t));
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        AnswerMode::SentenceFull => {
            let translation = card_data
                .sentence_translation_text
                .as_deref()
                .unwrap_or("(no translation)");
            let mut lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  {}", card_data.sentence_text),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Translation: ", Style::default().fg(t.muted)),
                    Span::styled(
                        translation,
                        Style::default().fg(t.success).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
            ];
            lines.push(rating_hint_line(t));
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }

    // Sentence context (answer revealed — don't blank)
    render_sentence_context(frame, state, sections[1], false, t);
}

fn render_typing(frame: &mut Frame, state: &ReviewState, area: Rect, t: &Theme) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    let card_area = centered_rect(70, 80, area);
    let sections =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(card_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.warning))
        .title(" Typed Reading ");
    let inner = block.inner(sections[0]);
    frame.render_widget(block, sections[0]);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            &card_data.display_surface,
            Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "Type the reading (hiragana, romaji, or kanji):",
            Style::default().fg(t.muted),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![
            Span::raw("  > "),
            Span::styled(
                &state.typed_input,
                Style::default()
                    .fg(t.accent)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled("_", Style::default().fg(t.fg)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to submit",
            Style::default().fg(t.muted),
        ))
        .alignment(Alignment::Center),
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    render_sentence_context(frame, state, sections[1], false, t);
}

fn render_typed_result(frame: &mut Frame, state: &ReviewState, area: Rect, t: &Theme) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    let card_area = centered_rect(70, 80, area);
    let sections =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(card_area);

    let (ref input, ref expected, is_correct) =
        state.typed_result.as_ref().cloned().unwrap_or_default();

    let result_color = if is_correct { t.success } else { t.error };
    let result_text = if is_correct { "Correct!" } else { "Incorrect" };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(result_color))
        .title(format!(" {} ", result_text));
    let inner = block.inner(sections[0]);
    frame.render_widget(block, sections[0]);

    let auto_rating_text = state
        .auto_rating
        .map(|r| {
            format!(
                "Auto-rating: {} (press Enter to accept, or 1-4 to override)",
                r.label()
            )
        })
        .unwrap_or_default();

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            &card_data.display_surface,
            Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Your answer: ", Style::default().fg(t.muted)),
            Span::styled(input, Style::default().fg(result_color)),
        ]),
        Line::from(vec![
            Span::styled("  Expected:    ", Style::default().fg(t.muted)),
            Span::styled(expected, Style::default().fg(t.success)),
        ]),
        Line::from(""),
    ];

    if !is_correct {
        let diff_spans = build_diff_spans(input, expected, t);
        lines.push(Line::from(vec![Span::styled(
            "  Diff: ",
            Style::default().fg(t.muted),
        )]));
        lines.push(Line::from(diff_spans));
    }

    if card_data.card.card_type == "word" {
        if let Some(ref translation) = card_data.vocabulary.translation {
            if !translation.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  ★ ", Style::default().fg(t.warning)),
                    Span::styled(translation.as_str(), Style::default().fg(t.success)),
                ]));
            }
        }
        if let Some(entry) = card_data.definitions.first() {
            lines.push(Line::from(Span::styled(
                format!("  {}", entry.short_gloss()),
                Style::default().fg(t.accent),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  {}", auto_rating_text),
        Style::default().fg(t.warning),
    )));

    frame.render_widget(Paragraph::new(lines), inner);

    render_sentence_context(frame, state, sections[1], false, t);
}

fn render_summary(frame: &mut Frame, state: &ReviewState, area: Rect, t: &Theme) {
    let center = centered_rect(60, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.success))
        .title(" Session Complete ");
    let inner = block.inner(center);
    frame.render_widget(block, center);

    let accuracy = if state.total_reviewed > 0 {
        (state.correct_count * 100) / state.total_reviewed
    } else {
        0
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Review Session Complete!",
            Style::default().fg(t.success).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("  Total reviewed: {}", state.total_reviewed)),
        Line::from(format!(
            "  Correct: {} ({}%)",
            state.correct_count, accuracy
        )),
        Line::from(""),
        if state.queue.is_empty() && state.total_reviewed == 0 {
            Line::from(Span::styled(
                "  No cards due for review right now.",
                Style::default().fg(t.muted),
            ))
        } else {
            Line::from("")
        },
        Line::from(""),
        Line::from(Span::styled(
            "  Press Enter or Esc to return",
            Style::default().fg(t.muted),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Render the sentence context below the card.
fn render_sentence_context(
    frame: &mut Frame,
    state: &ReviewState,
    area: Rect,
    blank_target: bool,
    t: &Theme,
) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    if card_data.sentence_tokens.is_empty() {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.muted))
        .title(" Sentence Context ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let target_base = &card_data.vocabulary.base_form;
    let target_reading = &card_data.vocabulary.reading;
    let target_gid = card_data.target_group_id;

    let mut lines: Vec<Line> = Vec::new();

    // Build colored sentence spans
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw("  "));

    let mut blank_emitted = false;
    for (i, token) in card_data.sentence_tokens.iter().enumerate() {
        let is_target = if let Some(gid) = target_gid {
            token.group_id == Some(gid)
        } else {
            token.base_form == *target_base && token.reading == *target_reading
        };
        let is_selected = state.context_word_index == Some(i);

        if blank_target && is_target {
            if !blank_emitted {
                spans.push(Span::styled(
                    "____",
                    Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
                ));
                blank_emitted = true;
            }
        } else if is_selected {
            spans.push(Span::styled(
                &token.surface,
                Style::default()
                    .bg(t.info)
                    .fg(t.fg)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(&token.surface, review_token_style(token, t)));
        }
    }
    lines.push(Line::from(spans));

    // Word info for learning words
    lines.push(Line::from(""));
    let mut seen = std::collections::HashSet::new();

    for (i, token) in card_data.sentence_tokens.iter().enumerate() {
        if token.is_trivial {
            continue;
        }
        if token.group_id.is_some() && !token.is_group_head {
            continue;
        }
        let is_target = if let Some(gid) = target_gid {
            token.group_id == Some(gid)
        } else {
            token.base_form == *target_base && token.reading == *target_reading
        };
        if is_target {
            continue;
        }
        if token.vocabulary_status == VocabularyStatus::Known {
            continue;
        }
        let key = (token.base_form.clone(), token.reading.clone());
        if !seen.insert(key.clone()) {
            continue;
        }

        let is_selected = state.context_word_index == Some(i);

        let display_surface = if let Some(gid) = token.group_id {
            card_data
                .sentence_tokens
                .iter()
                .filter(|t| t.group_id == Some(gid))
                .map(|t| t.surface.as_str())
                .collect::<String>()
        } else {
            token.surface.clone()
        };

        let display_reading: String = if let Some(gid) = token.group_id {
            card_data
                .sentence_tokens
                .iter()
                .filter(|t| t.group_id == Some(gid))
                .map(|t| {
                    if !t.surface_reading.is_empty() {
                        t.surface_reading.as_str()
                    } else if !t.reading.is_empty() {
                        t.reading.as_str()
                    } else {
                        t.surface.as_str()
                    }
                })
                .collect()
        } else if !token.surface_reading.is_empty() {
            token.surface_reading.clone()
        } else {
            token.reading.clone()
        };
        let reading_part = if !display_reading.is_empty() && display_reading != display_surface {
            format!(" ({})", display_reading)
        } else {
            String::new()
        };

        let user_translation = state
            .vocabulary_cache
            .get(&key)
            .and_then(|v| v.translation.as_ref());
        let gloss_part = if let Some(trans) = user_translation {
            format!(" = {}", trans)
        } else if !token.mwe_gloss.is_empty() {
            format!(" = {}", token.mwe_gloss)
        } else if !token.short_gloss.is_empty() {
            format!(" = {}", token.short_gloss)
        } else {
            String::new()
        };

        let marker = if is_selected { "▸ " } else { "  " };
        let style = if is_selected {
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.muted)
        };

        lines.push(Line::from(Span::styled(
            format!(
                "{}{}{}{}",
                marker, display_surface, reading_part, gloss_part
            ),
            style,
        )));
    }

    let text = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(text, inner);
}

/// Build spans for sentence cloze display.
fn build_cloze_spans<'a>(card_data: &'a ReviewCardData, blank: bool, t: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    spans.push(Span::raw("  "));

    let target_base = &card_data.vocabulary.base_form;
    let target_reading = &card_data.vocabulary.reading;
    let target_gid = card_data.target_group_id;

    let mut blank_emitted = false;

    for token in &card_data.sentence_tokens {
        let is_target = if let Some(gid) = target_gid {
            token.group_id == Some(gid)
        } else {
            token.base_form == *target_base && token.reading == *target_reading
        };

        if is_target {
            if blank {
                if !blank_emitted {
                    spans.push(Span::styled(
                        "____",
                        Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
                    ));
                    blank_emitted = true;
                }
            } else {
                spans.push(Span::styled(
                    token.surface.as_str(),
                    Style::default()
                        .fg(t.cloze_reveal_fg)
                        .add_modifier(Modifier::BOLD)
                        .bg(t.cloze_reveal_bg),
                ));
            }
        } else {
            spans.push(Span::styled(
                token.surface.as_str(),
                review_token_style(token, t),
            ));
        }
    }
    spans
}

/// Build diff spans comparing input to expected reading.
fn build_diff_spans<'a>(input: &'a str, expected: &'a str, t: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    spans.push(Span::raw("          "));

    let input_chars: Vec<char> = input.chars().collect();
    let expected_chars: Vec<char> = expected.chars().collect();
    let max_len = input_chars.len().max(expected_chars.len());

    for i in 0..max_len {
        let ic = input_chars.get(i);
        let ec = expected_chars.get(i);
        match (ic, ec) {
            (Some(a), Some(b)) if a == b => {
                spans.push(Span::styled(a.to_string(), Style::default().fg(t.success)));
            }
            (Some(a), Some(_b)) => {
                spans.push(Span::styled(a.to_string(), Style::default().fg(t.error)));
            }
            (Some(a), None) => {
                spans.push(Span::styled(a.to_string(), Style::default().fg(t.error)));
            }
            (None, Some(b)) => {
                spans.push(Span::styled(b.to_string(), Style::default().fg(t.muted)));
            }
            (None, None) => {}
        }
    }
    spans
}

fn rating_hint_line<'a>(t: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled("  Rate: ", Style::default().fg(t.muted)),
        Span::styled("1", Style::default().fg(t.error)),
        Span::raw("=Again "),
        Span::styled("2", Style::default().fg(t.warning)),
        Span::raw("=Hard "),
        Span::styled("3", Style::default().fg(t.success)),
        Span::raw("=Good "),
        Span::styled("4", Style::default().fg(t.accent)),
        Span::raw("=Easy"),
    ])
}

/// Style for a token in review context — uses the subtler review-specific colors.
fn review_token_style(token: &TokenDisplay, t: &Theme) -> Style {
    if token.is_trivial {
        Style::default()
    } else {
        review_status_style(token.vocabulary_status, t)
    }
}

/// Color style for a vocabulary status in review context.
fn review_status_style(status: VocabularyStatus, t: &Theme) -> Style {
    match status {
        VocabularyStatus::New => Style::default().bg(t.review_vocab_new_bg),
        VocabularyStatus::Learning1 => Style::default().bg(t.review_vocab_l1_bg),
        VocabularyStatus::Learning2 => Style::default().bg(t.review_vocab_l2_bg),
        VocabularyStatus::Learning3 => Style::default().bg(t.review_vocab_l3_bg),
        VocabularyStatus::Learning4 => Style::default().bg(t.review_vocab_l4_bg),
        VocabularyStatus::Known => Style::default(),
        VocabularyStatus::Ignored => Style::default().fg(t.muted),
    }
}

/// Create a centered rectangle with given percentage width and height.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
