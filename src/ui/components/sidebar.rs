use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::app::App;
use crate::db::models::VocabularyStatus;

/// Render the sidebar panel showing current sentence breakdown.
pub fn render(area: Rect, buf: &mut Buffer, app: &App) {
    let state = match app.reader_state.as_ref() {
        Some(s) => s,
        None => return,
    };

    if state.sentences.is_empty() {
        return;
    }

    let sentence = &state.sentences[state.sentence_index];

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " Sentence Details ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    if inner.width < 4 || inner.height < 3 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Current sentence text
    lines.push(Line::from(Span::styled(
        "Current Sentence:",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::raw(&sentence.text)));
    lines.push(Line::from(""));

    // Separator
    lines.push(Line::from(Span::styled(
        "Words:",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    // Word list
    for (i, token) in sentence.tokens.iter().enumerate() {
        if token.is_trivial {
            continue;
        }

        let is_selected = state.word_index == Some(i);
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

        // Show surface reading (conjugated form) if available, else lemma reading
        let display_reading = if !token.surface_reading.is_empty() {
            &token.surface_reading
        } else {
            &token.reading
        };
        let reading_part = if !display_reading.is_empty() && *display_reading != token.surface {
            format!(" ({})", display_reading)
        } else {
            String::new()
        };

        let gloss_part = if !token.short_gloss.is_empty() {
            format!(" = {}", token.short_gloss)
        } else {
            String::new()
        };

        let conj_part = if !token.conjugation_form.is_empty() {
            format!(" [{}]", token.conjugation_form)
        } else {
            String::new()
        };

        let line_text = format!(
            "{}{}{}{} [{}]{}",
            marker, token.surface, reading_part, gloss_part, status_char, conj_part
        );

        let style = if is_selected {
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(line_text, style)));
    }

    // Stats at bottom
    lines.push(Line::from(""));
    let _total = sentence.tokens.iter().filter(|t| !t.is_trivial).count();
    let new_count = sentence
        .tokens
        .iter()
        .filter(|t| !t.is_trivial && t.vocabulary_status == VocabularyStatus::New)
        .count();
    let learning = sentence
        .tokens
        .iter()
        .filter(|t| {
            !t.is_trivial
                && matches!(
                    t.vocabulary_status,
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
        .filter(|t| !t.is_trivial && t.vocabulary_status == VocabularyStatus::Known)
        .count();

    lines.push(Line::from(vec![
        Span::styled("New: ", Style::default().fg(Color::Blue)),
        Span::raw(format!("{}  ", new_count)),
        Span::styled("Learning: ", Style::default().fg(Color::Yellow)),
        Span::raw(format!("{}  ", learning)),
        Span::styled("Known: ", Style::default().fg(Color::Green)),
        Span::raw(format!("{}", known)),
    ]));

    let scroll = state.sidebar_scroll as u16;
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    paragraph.render(inner, buf);
}
