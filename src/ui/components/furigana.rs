use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::app::TokenDisplay;
use crate::db::models::VocabularyStatus;
use crate::ui::theme::Theme;

/// Get the color for a vocabulary status in reader (inline text highlighting).
pub fn status_style(status: VocabularyStatus, is_selected: bool, theme: &Theme) -> Style {
    let base = match status {
        VocabularyStatus::New => Style::default()
            .bg(theme.vocab_new_bg)
            .fg(theme.vocab_new_fg),
        VocabularyStatus::Learning1 => Style::default().bg(theme.vocab_l1_bg).fg(theme.vocab_l1_fg),
        VocabularyStatus::Learning2 => Style::default().bg(theme.vocab_l2_bg).fg(theme.vocab_l2_fg),
        VocabularyStatus::Learning3 => Style::default().bg(theme.vocab_l3_bg).fg(theme.vocab_l3_fg),
        VocabularyStatus::Learning4 => Style::default().bg(theme.vocab_l4_bg).fg(theme.vocab_l4_fg),
        VocabularyStatus::Known => Style::default(),
        VocabularyStatus::Ignored => Style::default().fg(theme.vocab_ignored_fg),
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
    /// Original token indices that this slot covers (for mouse hit-testing).
    token_indices: Vec<usize>,
}

/// Build layout slots from tokens, merging consecutive tokens that share the same
/// group_id into a single slot. This ensures groups render as one visual unit with
/// one combined furigana span, eliminating both inter-token gaps and furigana overlap.
fn build_slots(
    tokens: &[TokenDisplay],
    show_furigana: bool,
    force_all_furigana: bool,
    theme: &Theme,
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
                status_style(head.vocabulary_status, any_selected, theme)
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
                token_indices: (group_start..i).collect(),
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
                status_style(t.vocabulary_status, t.is_selected, theme)
            };

            slots.push(Slot {
                surface: t.surface.clone(),
                reading: fr.to_string(),
                surface_width,
                reading_width,
                slot_width,
                needs_furigana,
                style,
                token_indices: vec![i],
            });
            i += 1;
        }
    }
    slots
}

/// Information about a single wrapped line within a sentence.
struct LineInfo {
    /// Index of the first slot on this line.
    start: usize,
    /// Index one past the last slot on this line.
    end: usize,
    /// Whether any slot on this line needs furigana.
    has_furigana: bool,
}

/// Determine line breaks and per-line furigana needs for a sentence.
fn compute_lines(slots: &[Slot], text_width: usize) -> Vec<LineInfo> {
    let mut lines: Vec<LineInfo> = Vec::new();
    let mut col: usize = 0;
    let mut line_start: usize = 0;
    let mut line_has_furigana = false;

    for (i, slot) in slots.iter().enumerate() {
        if col + slot.slot_width > text_width && col > 0 {
            lines.push(LineInfo {
                start: line_start,
                end: i,
                has_furigana: line_has_furigana,
            });
            col = 0;
            line_start = i;
            line_has_furigana = false;
        }
        if slot.needs_furigana {
            line_has_furigana = true;
        }
        col += slot.slot_width;
    }

    // Last line (always present even if slots is empty — gives minimum height of 1)
    lines.push(LineInfo {
        start: line_start,
        end: slots.len(),
        has_furigana: line_has_furigana,
    });

    lines
}

/// Compute the exact height (in terminal rows) a sentence would occupy when rendered.
/// Uses the same layout algorithm as `render_sentence` but without writing to a buffer.
/// Each line independently gets height 2 (if it has furigana tokens) or 1 (if not).
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
    // Use a default theme for height calculation — only layout matters, not colors
    let default_theme = Theme::tokyo_night();
    let slots = build_slots(tokens, show_furigana, force_all_furigana, &default_theme);
    let text_width = usable_width.saturating_sub(if is_current { 2 } else { 0 });
    if text_width == 0 {
        return 1;
    }

    let lines = compute_lines(&slots, text_width);
    lines
        .iter()
        .map(|l| if l.has_furigana { 2u16 } else { 1 })
        .sum()
}

/// Render a sentence with furigana into a buffer area.
/// Returns the number of rows consumed. Each line independently gets height 2
/// (if it contains furigana tokens) or 1 (if not).
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
    theme: &Theme,
) -> u16 {
    if area.width < 2 || area.height < 1 {
        return 0;
    }

    let usable_width = area.width as usize;
    let slots = build_slots(tokens, show_furigana, force_all_furigana, theme);
    let text_width = usable_width.saturating_sub(if is_current { 2 } else { 0 });
    let text_x = area.x + if is_current { 2 } else { 0 };

    let lines = compute_lines(&slots, text_width);

    // Current sentence marker on the first line
    if is_current && area.width >= 3 && !lines.is_empty() {
        let first_line_h: u16 = if lines[0].has_furigana { 2 } else { 1 };
        if first_line_h <= area.height {
            let marker_y = if lines[0].has_furigana {
                area.y + 1
            } else {
                area.y
            };
            if marker_y < area.y + area.height {
                buf.set_string(area.x, marker_y, "▶", Style::default().fg(theme.accent));
            }
        }
    }

    let mut y_pos: u16 = 0; // cumulative row offset from area.y
    let mut total_rows: u16 = 0;

    for line in &lines {
        let line_height: u16 = if line.has_furigana { 2 } else { 1 };
        let current_line_y = area.y + y_pos;

        if current_line_y + line_height > area.y + area.height {
            break;
        }

        let mut col: usize = 0;
        for slot in &slots[line.start..line.end] {
            let x = text_x + col as u16;

            // Render furigana line (centered above the slot)
            if line.has_furigana && slot.needs_furigana {
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
                        Style::default().fg(theme.muted),
                    );
                }
            }

            // Render surface text
            let text_y = if line.has_furigana {
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

        y_pos += line_height;
        total_rows += line_height;
    }

    total_rows
}

/// Given a click position (column, row) and the same parameters used for rendering,
/// return the token index under that position (if any).
///
/// Uses the same layout algorithm as `render_sentence` / `sentence_height` to map
/// screen coordinates back to token indices. Returns the first token in the slot
/// (which is the group head for merged groups).
pub fn hit_test_sentence(
    tokens: &[TokenDisplay],
    area: Rect,
    click_col: u16,
    click_row: u16,
    show_furigana: bool,
    is_current: bool,
) -> Option<usize> {
    if area.width < 2 || area.height < 1 {
        return None;
    }
    // Quick bounds check
    if click_col < area.x
        || click_col >= area.x + area.width
        || click_row < area.y
        || click_row >= area.y + area.height
    {
        return None;
    }

    let default_theme = Theme::tokyo_night();
    let usable_width = area.width as usize;
    let slots = build_slots(tokens, show_furigana, false, &default_theme);
    let text_width = usable_width.saturating_sub(if is_current { 2 } else { 0 });
    let text_x = area.x + if is_current { 2 } else { 0 };

    if text_width == 0 {
        return None;
    }

    let lines = compute_lines(&slots, text_width);

    let mut y_pos: u16 = 0;

    for line in &lines {
        let line_height: u16 = if line.has_furigana { 2 } else { 1 };
        let current_line_y = area.y + y_pos;

        // Surface text row
        let text_y = if line.has_furigana {
            current_line_y + 1
        } else {
            current_line_y
        };

        // Check if click_row is on the surface row of this line
        if click_row == text_y {
            let mut col: usize = 0;
            for slot in &slots[line.start..line.end] {
                let x = text_x + col as u16;
                let slot_end = x + slot.slot_width as u16;
                if click_col >= x && click_col < slot_end {
                    // Found the slot — return the first token index (group head)
                    return slot.token_indices.first().copied();
                }
                col += slot.slot_width;
            }
        }
        // Also check furigana row (clicking reading should select the word too)
        if line.has_furigana && click_row == current_line_y {
            let mut col: usize = 0;
            for slot in &slots[line.start..line.end] {
                let x = text_x + col as u16;
                let slot_end = x + slot.slot_width as u16;
                if click_col >= x && click_col < slot_end && slot.needs_furigana {
                    return slot.token_indices.first().copied();
                }
                col += slot.slot_width;
            }
        }

        y_pos += line_height;
    }

    None
}
