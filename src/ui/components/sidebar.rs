use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::app::App;
use crate::core::llm::SentenceAnalysis;
use crate::db::models::VocabularyStatus;
use crate::ui::components::furigana;
use crate::ui::theme::Theme;

/// Render the sidebar panel showing current sentence breakdown.
pub fn render(area: Rect, buf: &mut Buffer, app: &App) {
    let state = match app.reader_state.as_ref() {
        Some(s) => s,
        None => return,
    };

    if state.sentences.is_empty() {
        return;
    }

    let t = &app.theme;
    let sentence = &state.sentences[state.sentence_index];

    // Decide sidebar title and mode
    let (title, show_llm) = if state.show_llm_sidebar {
        (" LLM Analysis ", true)
    } else {
        (" Sentence Details ", false)
    };

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(t.muted))
        .title(Span::styled(
            title,
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    if inner.width < 4 || inner.height < 3 {
        return;
    }

    if show_llm {
        render_llm_sidebar(inner, buf, app, t);
    } else {
        render_word_list_sidebar(inner, buf, app, t, state, sentence);
    }
}

/// Render the LLM analysis sidebar view.
fn render_llm_sidebar(area: Rect, buf: &mut Buffer, app: &App, t: &Theme) {
    let state = app.reader_state.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::new();

    // Show loading spinner if LLM is pending
    if app.llm_pending {
        let spinner = app.spinner_char();
        lines.push(Line::from(Span::styled(
            format!("{} Analyzing...", spinner),
            Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Ctrl+T to cancel",
            Style::default().fg(t.muted),
        )));
    } else if let Some(analysis) = state.llm_analyses.get(&state.sentence_index) {
        render_analysis_lines(&mut lines, analysis, t);
    } else {
        lines.push(Line::from(Span::styled(
            "No analysis available",
            Style::default().fg(t.muted),
        )));
    }

    // Footer hint
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Ctrl+T: dismiss | Esc: dismiss",
        Style::default().fg(t.muted),
    )));

    let scroll = state.sidebar_scroll as u16;
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    paragraph.render(area, buf);
}

/// Build lines for displaying a SentenceAnalysis.
fn render_analysis_lines(lines: &mut Vec<Line>, analysis: &SentenceAnalysis, t: &Theme) {
    // Translation
    lines.push(Line::from(Span::styled(
        "Translation:",
        Style::default().fg(t.success).add_modifier(Modifier::BOLD),
    )));
    // Wrap long translation text manually into multiple lines
    for line_str in analysis.translation.lines() {
        lines.push(Line::from(Span::styled(
            line_str.to_string(),
            Style::default().fg(t.success),
        )));
    }
    lines.push(Line::from(""));

    // Component breakdown
    if !analysis.component_breakdown.is_empty() {
        lines.push(Line::from(Span::styled(
            "Components:",
            Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
        )));
        for comp in &analysis.component_breakdown {
            // Japanese + romaji
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}", comp.japanese),
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" ({})", comp.romaji), Style::default().fg(t.muted)),
            ]));
            // Meaning
            lines.push(Line::from(Span::styled(
                format!("    {}", comp.meaning),
                Style::default().fg(t.fg),
            )));
        }
        lines.push(Line::from(""));
    }

    // Explanation
    if !analysis.explanation.is_empty() {
        lines.push(Line::from(Span::styled(
            "Explanation:",
            Style::default().fg(t.info).add_modifier(Modifier::BOLD),
        )));
        for line_str in analysis.explanation.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", line_str),
                Style::default().fg(t.fg),
            )));
        }
    }
}

/// Render the normal word list sidebar (original behavior).
fn render_word_list_sidebar(
    inner: Rect,
    buf: &mut Buffer,
    app: &App,
    t: &Theme,
    state: &crate::app::ReaderState,
    sentence: &crate::app::SentenceData,
) {
    // --- Section 1: Sentence header with furigana (rendered directly to buffer) ---
    let header_label_y = inner.y;
    buf.set_string(
        inner.x + 1,
        header_label_y,
        "Current Sentence:",
        Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
    );

    let sentence_area = Rect::new(
        inner.x + 1,
        header_label_y + 1,
        inner.width.saturating_sub(1),
        // Reserve at most half the sidebar height for the sentence, minimum 2 lines
        inner.height.saturating_sub(2).min(inner.height / 2).max(2),
    );

    let show_furigana = app.config.reader.furigana;
    let sentence_height = furigana::render_sentence(
        &sentence.tokens,
        sentence_area,
        buf,
        show_furigana,
        false,
        state.show_all_readings,
        t,
    );

    // --- Section 2: Word list + stats (rendered as Paragraph with wrap+scroll) ---
    let words_y = header_label_y + 1 + sentence_height + 1; // +1 blank line gap
    if words_y >= inner.y + inner.height {
        return;
    }
    let words_area = Rect::new(
        inner.x,
        words_y,
        inner.width,
        inner.y + inner.height - words_y,
    );

    let mut lines: Vec<Line> = Vec::new();

    // Show sentence translation if available
    if let Some(trans) = state.sentence_translations.get(&state.sentence_index) {
        if !trans.translation.is_empty() {
            let label = match trans.source.as_str() {
                "llm" | "llm_cached" => "Translation (LLM):",
                _ => "Translation:",
            };
            lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(t.success).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                &trans.translation,
                Style::default().fg(t.success),
            )));
            lines.push(Line::from(""));
        }
    } else if app.llm_pending {
        let spinner = app.spinner_char();
        lines.push(Line::from(Span::styled(
            format!("{} Translating...", spinner),
            Style::default().fg(t.muted).add_modifier(Modifier::ITALIC),
        )));
        lines.push(Line::from(""));
    }

    // Separator
    lines.push(Line::from(Span::styled(
        "Words:",
        Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
    )));

    // Word list — by default hide Known and Ignored unless toggled.
    // Skip non-head group members (they are merged with their head token).
    for (i, token) in sentence.tokens.iter().enumerate() {
        if token.is_trivial {
            continue;
        }

        // Skip non-head group members (auxiliaries in conjugation groups)
        if token.group_id.is_some() && !token.is_group_head {
            continue;
        }

        let is_known_or_ignored = matches!(
            token.vocabulary_status,
            VocabularyStatus::Known | VocabularyStatus::Ignored
        );
        let is_selected = state.word_index == Some(i);

        // Skip Known/Ignored words unless toggled on or the word is currently selected
        if is_known_or_ignored && !state.show_known_in_sidebar && !is_selected {
            continue;
        }

        let marker = if is_selected { ">> " } else { "   " };

        let status_char = match token.vocabulary_status {
            VocabularyStatus::Ignored => "I",
            VocabularyStatus::New => "N",
            VocabularyStatus::Learning1 => "1",
            VocabularyStatus::Learning2 => "2",
            VocabularyStatus::Learning3 => "3",
            VocabularyStatus::Learning4 => "4",
            VocabularyStatus::Known => "K",
        };

        // For group heads, show the combined surface of the whole group
        let display_surface = if let Some(gid) = token.group_id {
            sentence
                .tokens
                .iter()
                .filter(|t| t.group_id == Some(gid))
                .map(|t| t.surface.as_str())
                .collect::<String>()
        } else {
            token.surface.clone()
        };

        // Show surface reading (conjugated form) if available, else lemma reading.
        let display_reading: String = if let Some(gid) = token.group_id {
            sentence
                .tokens
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
        let should_show_reading = state.show_all_readings
            || matches!(
                token.vocabulary_status,
                VocabularyStatus::New
                    | VocabularyStatus::Learning1
                    | VocabularyStatus::Learning2
                    | VocabularyStatus::Learning3
                    | VocabularyStatus::Learning4
            );
        let reading_part = if should_show_reading
            && !display_reading.is_empty()
            && display_reading != display_surface
        {
            format!(" ({})", display_reading)
        } else {
            String::new()
        };

        // Show user translation (green) > MWE gloss > JMdict gloss
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

        // Show conjugation description (human-readable) instead of raw form
        let conj_part = if !token.conjugation_desc.is_empty() {
            format!(" [{}]", token.conjugation_desc)
        } else if !token.conjugation_form.is_empty() {
            format!(" [{}]", token.conjugation_form)
        } else {
            String::new()
        };

        let line_text = format!(
            "{}{}{}{} [{}]{}",
            marker, display_surface, reading_part, gloss_part, status_char, conj_part
        );

        let style = if is_selected {
            Style::default().add_modifier(Modifier::BOLD).fg(t.accent)
        } else if is_known_or_ignored {
            Style::default().fg(t.muted)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(line_text, style)));
    }

    // Stats at bottom — count only navigable tokens (skip trivial + non-head group members)
    lines.push(Line::from(""));
    let new_count = sentence
        .tokens
        .iter()
        .filter(|tok| tok.is_navigable() && tok.vocabulary_status == VocabularyStatus::New)
        .count();
    let learning = sentence
        .tokens
        .iter()
        .filter(|tok| {
            tok.is_navigable()
                && matches!(
                    tok.vocabulary_status,
                    VocabularyStatus::Learning1
                        | VocabularyStatus::Learning2
                        | VocabularyStatus::Learning3
                        | VocabularyStatus::Learning4
                )
        })
        .count();
    let known = sentence
        .tokens
        .iter()
        .filter(|tok| tok.is_navigable() && tok.vocabulary_status == VocabularyStatus::Known)
        .count();

    lines.push(Line::from(vec![
        Span::styled("N:", Style::default().fg(t.info)),
        Span::raw(format!("{} ", new_count)),
        Span::styled("L:", Style::default().fg(t.warning)),
        Span::raw(format!("{} ", learning)),
        Span::styled("K:", Style::default().fg(t.success)),
        Span::raw(format!("{}", known)),
    ]));

    let scroll = state.sidebar_scroll as u16;
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    paragraph.render(words_area, buf);
}

/// Render LLM analysis in the SRS review screen sidebar/popup area.
pub fn render_review_llm_analysis(
    area: Rect,
    buf: &mut Buffer,
    analysis: &SentenceAnalysis,
    t: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.muted))
        .title(Span::styled(
            " LLM Analysis ",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    render_analysis_lines(&mut lines, analysis, t);

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });

    paragraph.render(inner, buf);
}
