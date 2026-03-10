use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, ReviewCardData, ReviewPhase, ReviewState, TokenDisplay};
use crate::db::models::{AnswerMode, VocabularyStatus};

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

    // Layout: title bar (1) + main content + status bar (1)
    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(area);

    render_title_bar(frame, state, outer[0]);

    match state.phase {
        ReviewPhase::PreSession => render_pre_session(frame, state, outer[1]),
        ReviewPhase::ShowFront => render_card_front(frame, state, outer[1]),
        ReviewPhase::ShowBack => render_card_back(frame, state, outer[1]),
        ReviewPhase::TypingAnswer => render_typing(frame, state, outer[1]),
        ReviewPhase::ShowResult => render_typed_result(frame, state, outer[1]),
        ReviewPhase::SessionSummary => render_summary(frame, state, outer[1]),
    }

    render_status_bar(frame, state, outer[2]);
}

fn render_title_bar(frame: &mut Frame, state: &ReviewState, area: Rect) {
    let progress = if state.queue.is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", state.current_index + 1, state.queue.len())
    };
    let title = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Review "),
        Span::styled(
            format!("[{}]", progress),
            Style::default().fg(Color::Yellow),
        ),
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
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        area,
    );
}

fn render_status_bar(frame: &mut Frame, state: &ReviewState, area: Rect) {
    let hints = match state.phase {
        ReviewPhase::PreSession => " Space/Enter:start  Esc:back  q:quit ",
        ReviewPhase::ShowFront => " Space:reveal  Esc:back  q:quit ",
        ReviewPhase::ShowBack => " 1:Again 2:Hard 3/Space:Good 4:Easy  Esc:back ",
        ReviewPhase::TypingAnswer => " Enter:submit  Esc:skip ",
        ReviewPhase::ShowResult => " 1:Again 2:Hard 3:Good 4:Easy  Enter:accept auto-rating ",
        ReviewPhase::SessionSummary => " Enter/Esc:back  r:continue ",
    };
    let status = Line::from(Span::styled(hints, Style::default().fg(Color::DarkGray)));
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        area,
    );
}

fn render_pre_session(frame: &mut Frame, state: &ReviewState, area: Rect) {
    let center = centered_rect(60, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Review Session ");

    let inner = block.inner(center);
    frame.render_widget(block, center);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {} cards due", state.total_due),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("  {} new cards", state.total_new),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(Span::styled(
            format!("  {} cards loaded for this session", state.queue.len()),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            "  Press Space or Enter to begin",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_card_front(frame: &mut Frame, state: &ReviewState, area: Rect) {
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
        .border_style(Style::default().fg(Color::Blue))
        .title(format!(" {} ", card_data.answer_mode.label()));
    let inner = block.inner(sections[0]);
    frame.render_widget(block, sections[0]);

    match card_data.answer_mode {
        AnswerMode::WordReview => {
            // Show word + context sentence, ask to recall reading and meaning
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    &card_data.display_surface,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ))
                .alignment(Alignment::Center),
                Line::from(""),
                Line::from(Span::styled(
                    "Recall the reading and meaning",
                    Style::default().fg(Color::DarkGray),
                ))
                .alignment(Alignment::Center),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Space to reveal",
                    Style::default().fg(Color::DarkGray),
                ))
                .alignment(Alignment::Center),
            ];
            frame.render_widget(Paragraph::new(lines), inner);
        }
        AnswerMode::SentenceCloze => {
            // Show sentence with word blanked out
            let cloze_spans = build_cloze_spans(card_data, true);
            let lines = vec![
                Line::from(""),
                Line::from("  Fill in the blank:").alignment(Alignment::Left),
                Line::from(""),
                Line::from(cloze_spans),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Space to reveal",
                    Style::default().fg(Color::DarkGray),
                ))
                .alignment(Alignment::Center),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        AnswerMode::SentenceFull => {
            // Show the full sentence, ask for translation
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Translate this sentence:",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("  {}", card_data.sentence_text),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Space to reveal translation",
                    Style::default().fg(Color::DarkGray),
                ))
                .alignment(Alignment::Center),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }

    // Sentence context below — blank the target word for cloze cards
    let blank = card_data.answer_mode == AnswerMode::SentenceCloze;
    render_sentence_context(frame, state, sections[1], blank);
}

fn render_card_back(frame: &mut Frame, state: &ReviewState, area: Rect) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    let card_area = centered_rect(70, 80, area);

    let sections =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(card_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(format!(" {} — Answer ", card_data.answer_mode.label()));
    let inner = block.inner(sections[0]);
    frame.render_widget(block, sections[0]);

    match card_data.answer_mode {
        AnswerMode::WordReview | AnswerMode::SentenceCloze => {
            // For both word review and sentence cloze, show full answer:
            // word + reading + user translation + definitions
            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        &card_data.display_surface,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("({})", card_data.display_reading),
                        Style::default().fg(Color::Cyan),
                    ),
                ]),
                Line::from(""),
            ];

            // For cloze, also show the sentence with word revealed
            if card_data.answer_mode == AnswerMode::SentenceCloze {
                let cloze_spans = build_cloze_spans(card_data, false);
                lines.push(Line::from(cloze_spans));
                lines.push(Line::from(""));
            }

            // Show user translation if available
            if let Some(ref translation) = card_data.vocabulary.translation {
                if !translation.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  ★ ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            translation.as_str(),
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    lines.push(Line::from(""));
                }
            }

            // Show definitions
            for entry in &card_data.definitions {
                for (i, sense) in entry.senses.iter().enumerate() {
                    let glosses = sense.glosses.join("; ");
                    let pos = if sense.pos.is_empty() {
                        String::new()
                    } else {
                        format!("[{}] ", sense.pos.join(", "))
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {}. ", i + 1),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(pos, Style::default().fg(Color::Yellow)),
                        Span::raw(glosses),
                    ]));
                }
            }

            if card_data.definitions.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (No dictionary entry found)",
                    Style::default().fg(Color::DarkGray),
                )));
            }

            lines.push(Line::from(""));
            lines.push(rating_hint_line());
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        AnswerMode::SentenceFull => {
            // Show sentence + translation
            let translation = card_data
                .sentence_translation_text
                .as_deref()
                .unwrap_or("(no translation)");
            let mut lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  {}", card_data.sentence_text),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Translation: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        translation,
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
            ];
            lines.push(rating_hint_line());
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }

    // Sentence context (answer revealed — don't blank)
    render_sentence_context(frame, state, sections[1], false);
}

fn render_typing(frame: &mut Frame, state: &ReviewState, area: Rect) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    let card_area = centered_rect(70, 80, area);
    let sections =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(card_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Typed Reading ");
    let inner = block.inner(sections[0]);
    frame.render_widget(block, sections[0]);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            &card_data.display_surface,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "Type the reading (hiragana, romaji, or kanji):",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![
            Span::raw("  > "),
            Span::styled(
                &state.typed_input,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled("_", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to submit",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    render_sentence_context(frame, state, sections[1], false);
}

fn render_typed_result(frame: &mut Frame, state: &ReviewState, area: Rect) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    let card_area = centered_rect(70, 80, area);
    let sections =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(card_area);

    let (ref input, ref expected, is_correct) =
        state.typed_result.as_ref().cloned().unwrap_or_default();

    let result_color = if is_correct { Color::Green } else { Color::Red };
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
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Your answer: ", Style::default().fg(Color::DarkGray)),
            Span::styled(input, Style::default().fg(result_color)),
        ]),
        Line::from(vec![
            Span::styled("  Expected:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(expected, Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
    ];

    // Show diff
    if !is_correct {
        let diff_spans = build_diff_spans(input, expected);
        lines.push(Line::from(vec![Span::styled(
            "  Diff: ",
            Style::default().fg(Color::DarkGray),
        )]));
        lines.push(Line::from(diff_spans));
    }

    // Show meaning/definitions for word cards (helpful when require_typed_input is on)
    if card_data.card.card_type == "word" {
        if let Some(ref translation) = card_data.vocabulary.translation {
            if !translation.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  ★ ", Style::default().fg(Color::Yellow)),
                    Span::styled(translation.as_str(), Style::default().fg(Color::Green)),
                ]));
            }
        }
        if let Some(entry) = card_data.definitions.first() {
            lines.push(Line::from(Span::styled(
                format!("  {}", entry.short_gloss()),
                Style::default().fg(Color::Cyan),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  {}", auto_rating_text),
        Style::default().fg(Color::Yellow),
    )));

    frame.render_widget(Paragraph::new(lines), inner);

    render_sentence_context(frame, state, sections[1], false);
}

fn render_summary(frame: &mut Frame, state: &ReviewState, area: Rect) {
    let center = centered_rect(60, 50, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
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
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
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
                Style::default().fg(Color::DarkGray),
            ))
        } else {
            Line::from("")
        },
        Line::from(""),
        Line::from(Span::styled(
            "  Press Enter or Esc to return",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Render the sentence context below the card.
/// If `blank_target` is true, the card's target word is replaced with ____ in the sentence.
fn render_sentence_context(frame: &mut Frame, state: &ReviewState, area: Rect, blank_target: bool) {
    let card_data = match state.queue.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    if card_data.sentence_tokens.is_empty() {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
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
            // Emit a single ____ for the whole group
            if !blank_emitted {
                spans.push(Span::styled(
                    "____",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
                blank_emitted = true;
            }
        } else if is_selected {
            spans.push(Span::styled(
                &token.surface,
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(&token.surface, token_style(token)));
        }
    }
    lines.push(Line::from(spans));

    // Word info for learning words (excluding the card's target word)
    // Show reading + short gloss for non-trivial, non-target learning words
    lines.push(Line::from(""));
    let mut seen = std::collections::HashSet::new();

    for (i, token) in card_data.sentence_tokens.iter().enumerate() {
        if token.is_trivial {
            continue;
        }
        // Skip non-head group members
        if token.group_id.is_some() && !token.is_group_head {
            continue;
        }
        // Skip the card's target word (including entire group)
        let is_target = if let Some(gid) = target_gid {
            token.group_id == Some(gid)
        } else {
            token.base_form == *target_base && token.reading == *target_reading
        };
        if is_target {
            continue;
        }
        // Skip Known words (they don't need hints)
        if token.vocabulary_status == VocabularyStatus::Known {
            continue;
        }
        // Deduplicate by base_form+reading
        let key = (token.base_form.clone(), token.reading.clone());
        if !seen.insert(key) {
            continue;
        }

        let is_selected = state.context_word_index == Some(i);

        // Build display surface for group heads
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

        // Aggregate reading from all group members (same as sidebar logic)
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

        // Show user translation > MWE gloss > JMdict gloss (same priority as sidebar)
        let key = (token.base_form.clone(), token.reading.clone());
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
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
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
/// If `blank` is true, the target word/group is replaced with ____. Otherwise it's highlighted.
/// Uses `target_group_id` to blank the entire compound group, not just the head token.
fn build_cloze_spans<'a>(card_data: &'a ReviewCardData, blank: bool) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    spans.push(Span::raw("  "));

    let target_base = &card_data.vocabulary.base_form;
    let target_reading = &card_data.vocabulary.reading;
    let target_gid = card_data.target_group_id;

    // Track whether we've already emitted the blank for a group
    let mut blank_emitted = false;

    for token in &card_data.sentence_tokens {
        let is_target = if let Some(gid) = target_gid {
            token.group_id == Some(gid)
        } else {
            token.base_form == *target_base && token.reading == *target_reading
        };

        if is_target {
            if blank {
                // Emit a single ____ for the whole group
                if !blank_emitted {
                    spans.push(Span::styled(
                        "____",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                    blank_emitted = true;
                }
                // Skip additional group members (they're part of the blank)
            } else {
                spans.push(Span::styled(
                    token.surface.as_str(),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                        .bg(Color::Rgb(30, 60, 30)),
                ));
            }
        } else {
            spans.push(Span::styled(token.surface.as_str(), token_style(token)));
        }
    }
    spans
}

/// Build diff spans comparing input to expected reading.
fn build_diff_spans<'a>(input: &'a str, expected: &'a str) -> Vec<Span<'a>> {
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
                spans.push(Span::styled(
                    a.to_string(),
                    Style::default().fg(Color::Green),
                ));
            }
            (Some(a), Some(_b)) => {
                spans.push(Span::styled(a.to_string(), Style::default().fg(Color::Red)));
            }
            (Some(a), None) => {
                spans.push(Span::styled(a.to_string(), Style::default().fg(Color::Red)));
            }
            (None, Some(b)) => {
                spans.push(Span::styled(
                    b.to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            (None, None) => {}
        }
    }
    spans
}

fn rating_hint_line<'a>() -> Line<'a> {
    Line::from(vec![
        Span::styled("  Rate: ", Style::default().fg(Color::DarkGray)),
        Span::styled("1", Style::default().fg(Color::Red)),
        Span::raw("=Again "),
        Span::styled("2", Style::default().fg(Color::Yellow)),
        Span::raw("=Hard "),
        Span::styled("3", Style::default().fg(Color::Green)),
        Span::raw("=Good "),
        Span::styled("4", Style::default().fg(Color::Cyan)),
        Span::raw("=Easy"),
    ])
}

/// Style for a token — trivial tokens (punctuation, whitespace) get no highlighting.
fn token_style(token: &TokenDisplay) -> Style {
    if token.is_trivial {
        Style::default()
    } else {
        status_style(token.vocabulary_status)
    }
}

/// Color style for a vocabulary status.
fn status_style(status: VocabularyStatus) -> Style {
    match status {
        VocabularyStatus::New => Style::default().bg(Color::Rgb(60, 80, 160)),
        VocabularyStatus::Learning1 => Style::default().bg(Color::Rgb(120, 100, 40)),
        VocabularyStatus::Learning2 => Style::default().bg(Color::Rgb(100, 85, 30)),
        VocabularyStatus::Learning3 => Style::default().bg(Color::Rgb(80, 70, 20)),
        VocabularyStatus::Learning4 => Style::default().bg(Color::Rgb(60, 55, 15)),
        VocabularyStatus::Known => Style::default(),
        VocabularyStatus::Ignored => Style::default().fg(Color::DarkGray),
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
