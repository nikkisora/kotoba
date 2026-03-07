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

/// Whether a token should show furigana given current display settings.
fn token_needs_furigana(t: &TokenDisplay, show_furigana: bool, force_all_furigana: bool) -> bool {
    let furigana_reading = if !t.surface_reading.is_empty() {
        &t.surface_reading
    } else {
        &t.reading
    };
    show_furigana
        && has_kanji(&t.surface)
        && !furigana_reading.is_empty()
        && t.surface != furigana_reading.as_str()
        && !t.is_trivial
        && (force_all_furigana
            || matches!(
                t.vocabulary_status,
                VocabularyStatus::New
                    | VocabularyStatus::Learning1
                    | VocabularyStatus::Learning2
                    | VocabularyStatus::Learning3
            ))
}

/// Get the furigana reading string for a token.
fn furigana_reading(t: &TokenDisplay) -> &str {
    if !t.surface_reading.is_empty() {
        &t.surface_reading
    } else {
        &t.reading
    }
}

/// A layout slot — either a single standalone token or a merged group of tokens.
struct Slot {
    surface: String,
    reading: String,
    surface_width: usize,
    reading_width: usize,
    slot_width: usize,
    needs_furigana: bool,
    style: Style,
}

/// Build layout slots from tokens, merging consecutive tokens that share the same
/// group_id into a single slot. This ensures groups render as one visual unit with
/// one combined furigana span, eliminating both inter-token gaps and furigana overlap.
fn build_slots(
    tokens: &[TokenDisplay],
    show_furigana: bool,
    force_all_furigana: bool,
) -> Vec<Slot> {
    let mut slots: Vec<Slot> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];

        if let Some(gid) = t.group_id {
            // Collect all consecutive tokens with the same group_id.
            // (Group members are always consecutive in the token list.)
            let group_start = i;
            while i < tokens.len() && tokens[i].group_id == Some(gid) {
                i += 1;
            }
            let group = &tokens[group_start..i];

            // Merge surface and reading from all group members
            let mut merged_surface = String::new();
            let mut merged_reading = String::new();
            let mut any_needs = false;
            // Use the head's style (all members have the same vocabulary_status)
            let head = group.iter().find(|t| t.is_group_head).unwrap_or(&group[0]);
            let any_selected = group.iter().any(|t| t.is_selected);
            let style = if head.is_trivial {
                if any_selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                }
            } else {
                status_style(head.vocabulary_status, any_selected)
            };

            for member in group {
                merged_surface.push_str(&member.surface);
                // For the merged reading: use surface_reading/reading for kanji
                // tokens, and the surface itself for kana-only tokens (particles, etc.)
                if token_needs_furigana(member, show_furigana, force_all_furigana) {
                    any_needs = true;
                    merged_reading.push_str(furigana_reading(member));
                } else {
                    // Kana-only member or furigana suppressed: use surface as reading
                    merged_reading.push_str(&member.surface);
                }
            }

            let surface_width = UnicodeWidthStr::width(merged_surface.as_str());
            let reading_width = if any_needs {
                UnicodeWidthStr::width(merged_reading.as_str())
            } else {
                0
            };
            // Merged reading may differ from merged surface even if individual
            // kana tokens were the same, because kanji tokens contribute readings.
            let needs_furigana = any_needs && merged_surface != merged_reading;
            let reading_width = if needs_furigana { reading_width } else { 0 };
            let slot_width = surface_width.max(reading_width);

            slots.push(Slot {
                surface: merged_surface,
                reading: merged_reading,
                surface_width,
                reading_width,
                slot_width,
                needs_furigana,
                style,
            });
        } else {
            // Standalone token — same logic as before
            let surface_width = UnicodeWidthStr::width(t.surface.as_str());
            let fr = furigana_reading(t);
            let needs_furigana = token_needs_furigana(t, show_furigana, force_all_furigana);
            let reading_width = if needs_furigana {
                UnicodeWidthStr::width(fr)
            } else {
                0
            };
            let slot_width = surface_width.max(reading_width);
            let style = if t.is_trivial {
                if t.is_selected {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                }
            } else {
                status_style(t.vocabulary_status, t.is_selected)
            };

            slots.push(Slot {
                surface: t.surface.clone(),
                reading: fr.to_string(),
                surface_width,
                reading_width,
                slot_width,
                needs_furigana,
                style,
            });
            i += 1;
        }
    }
    slots
}

/// Compute the exact height (in terminal rows) a sentence would occupy when rendered.
/// Uses the same layout algorithm as `render_sentence` but without writing to a buffer.
pub fn sentence_height(
    tokens: &[TokenDisplay],
    width: u16,
    show_furigana: bool,
    is_current: bool,
    force_all_furigana: bool,
) -> u16 {
    if width < 2 {
        return 0;
    }

    let usable_width = width as usize;
    let slots = build_slots(tokens, show_furigana, force_all_furigana);
    let any_furigana = slots.iter().any(|s| s.needs_furigana);

    let line_height: u16 = if any_furigana { 2 } else { 1 };
    let text_width = usable_width.saturating_sub(if is_current { 2 } else { 0 });
    if text_width == 0 {
        return line_height;
    }

    let mut lines_used: u16 = 0;
    let mut col: usize = 0;

    for slot in &slots {
        if col + slot.slot_width > text_width && col > 0 {
            lines_used += 1;
            col = 0;
        }
        col += slot.slot_width;
    }

    (lines_used + 1) * line_height
}

/// Render a sentence with furigana into a buffer area.
/// Returns the number of lines consumed (each token pair = 2 lines if furigana shown).
///
/// When `force_all_furigana` is true, furigana is shown for all kanji words regardless
/// of vocabulary status (used in sidebar). When false, furigana respects the status-based
/// rules (hidden for Learning4, Known, Ignored).
pub fn render_sentence(
    tokens: &[TokenDisplay],
    area: Rect,
    buf: &mut Buffer,
    show_furigana: bool,
    is_current: bool,
    force_all_furigana: bool,
) -> u16 {
    if area.width < 2 || area.height < 1 {
        return 0;
    }

    let usable_width = area.width as usize;
    let slots = build_slots(tokens, show_furigana, force_all_furigana);
    let any_furigana = slots.iter().any(|s| s.needs_furigana);
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

    for slot in &slots {
        // Check if slot fits on current line
        if col + slot.slot_width > text_width && col > 0 {
            lines_used += 1;
            col = 0;
            current_line_y = area.y + lines_used * line_height;
        }

        if current_line_y + line_height > area.y + area.height {
            break;
        }

        let x = text_x + col as u16;

        // Render furigana line (centered above the slot)
        if any_furigana && slot.needs_furigana {
            let furigana_y = current_line_y;
            let pad = if slot.slot_width > slot.reading_width {
                (slot.slot_width - slot.reading_width) / 2
            } else {
                0
            };

            if furigana_y < area.y + area.height {
                buf.set_string(
                    x + pad as u16,
                    furigana_y,
                    &slot.reading,
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
            let surface_pad = if slot.slot_width > slot.surface_width {
                (slot.slot_width - slot.surface_width) / 2
            } else {
                0
            };
            buf.set_string(x + surface_pad as u16, text_y, &slot.surface, slot.style);
        }

        col += slot.slot_width;
    }

    (lines_used + 1) * line_height
}
