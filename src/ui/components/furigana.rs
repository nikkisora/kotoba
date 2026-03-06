use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::app::TokenDisplay;
use crate::db::models::VocabularyStatus;

/// Get the color for a vocabulary status.
pub fn status_style(status: VocabularyStatus, is_selected: bool) -> Style {
    let base = match status {
        VocabularyStatus::New => Style::default().bg(Color::Blue).fg(Color::White),
        VocabularyStatus::Learning1 => Style::default().bg(Color::Yellow).fg(Color::Black),
        VocabularyStatus::Learning2 => Style::default()
            .bg(Color::Rgb(200, 180, 60))
            .fg(Color::Black),
        VocabularyStatus::Learning3 => Style::default()
            .bg(Color::Rgb(160, 150, 80))
            .fg(Color::Black),
        VocabularyStatus::Learning4 => Style::default()
            .bg(Color::Rgb(120, 120, 100))
            .fg(Color::White),
        VocabularyStatus::Known => Style::default(),
        VocabularyStatus::Ignored => Style::default().fg(Color::DarkGray),
    };

    if is_selected {
        base.add_modifier(Modifier::REVERSED)
    } else {
        base
    }
}

/// Returns true if the surface contains kanji (CJK Unified Ideographs).
fn has_kanji(s: &str) -> bool {
    s.chars().any(|c| {
        ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
            || ('\u{F900}'..='\u{FAFF}').contains(&c)
    })
}

/// Render a sentence with furigana into a buffer area.
/// Returns the number of lines consumed (each token pair = 2 lines if furigana shown).
pub fn render_sentence(
    tokens: &[TokenDisplay],
    area: Rect,
    buf: &mut Buffer,
    show_furigana: bool,
    is_current: bool,
) -> u16 {
    if area.width < 2 || area.height < 1 {
        return 0;
    }

    let usable_width = area.width as usize;

    struct TokenLayout {
        surface: String,
        reading: String,
        surface_width: usize,
        reading_width: usize,
        /// The slot width this token occupies (max of surface and reading widths).
        slot_width: usize,
        needs_furigana: bool,
        style: Style,
    }

    let layouts: Vec<TokenLayout> = tokens
        .iter()
        .map(|t| {
            let surface_width = UnicodeWidthStr::width(t.surface.as_str());
            // Use surface_reading for furigana (matches conjugated form),
            // falling back to lemma reading if surface_reading is empty.
            let furigana_reading = if !t.surface_reading.is_empty() {
                &t.surface_reading
            } else {
                &t.reading
            };
            let needs_furigana = show_furigana
                && has_kanji(&t.surface)
                && !furigana_reading.is_empty()
                && t.surface != furigana_reading.as_str()
                && !t.is_trivial
                && matches!(
                    t.vocabulary_status,
                    VocabularyStatus::New
                        | VocabularyStatus::Learning1
                        | VocabularyStatus::Learning2
                        | VocabularyStatus::Learning3
                );
            let reading_width = if needs_furigana {
                UnicodeWidthStr::width(furigana_reading.as_str())
            } else {
                0
            };
            // The slot width is the max of surface and reading to prevent overlap
            let slot_width = if needs_furigana {
                surface_width.max(reading_width)
            } else {
                surface_width
            };
            let style = if t.is_trivial {
                if t.is_selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                }
            } else {
                status_style(t.vocabulary_status, t.is_selected)
            };
            TokenLayout {
                surface: t.surface.clone(),
                reading: furigana_reading.to_string(),
                surface_width,
                reading_width,
                slot_width,
                needs_furigana,
                style,
            }
        })
        .collect();

    let any_furigana = layouts.iter().any(|l| l.needs_furigana);
    let line_height: u16 = if any_furigana { 2 } else { 1 };

    let mut lines_used: u16 = 0;
    let mut col: usize = 0;
    let mut current_line_y = area.y + lines_used * line_height;

    // Current sentence marker
    if is_current && area.width >= 3 {
        if current_line_y + line_height <= area.y + area.height {
            let marker_y = if any_furigana {
                current_line_y + 1
            } else {
                current_line_y
            };
            if marker_y < area.y + area.height {
                buf.set_string(area.x, marker_y, "▶", Style::default().fg(Color::Cyan));
            }
        }
    }

    let text_x = area.x + if is_current { 2 } else { 0 };
    let text_width = usable_width.saturating_sub(if is_current { 2 } else { 0 });

    for layout in &layouts {
        // Check if token fits on current line
        if col + layout.slot_width > text_width && col > 0 {
            lines_used += 1;
            col = 0;
            current_line_y = area.y + lines_used * line_height;
        }

        if current_line_y + line_height > area.y + area.height {
            break;
        }

        let x = text_x + col as u16;

        // Render furigana line (centered above the slot)
        if any_furigana && layout.needs_furigana {
            let furigana_y = current_line_y;
            let pad = if layout.slot_width > layout.reading_width {
                (layout.slot_width - layout.reading_width) / 2
            } else {
                0
            };

            if furigana_y < area.y + area.height {
                buf.set_string(
                    x + pad as u16,
                    furigana_y,
                    &layout.reading,
                    Style::default().fg(Color::DarkGray),
                );
            }
        }

        // Render surface text (centered within the slot if reading is wider)
        let text_y = if any_furigana {
            current_line_y + 1
        } else {
            current_line_y
        };

        if text_y < area.y + area.height {
            let surface_pad = if layout.slot_width > layout.surface_width {
                (layout.slot_width - layout.surface_width) / 2
            } else {
                0
            };
            buf.set_string(
                x + surface_pad as u16,
                text_y,
                &layout.surface,
                layout.style,
            );
        }

        col += layout.slot_width;
    }

    (lines_used + 1) * line_height
}
