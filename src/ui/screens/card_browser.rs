use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::App;

/// Pad a string to exactly `target_width` terminal columns using spaces.
/// If the string is wider than target_width, truncate with "..".
fn pad_to_width(s: &str, target_width: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    if w >= target_width {
        // Truncate: walk chars until we reach target_width - 2, then add ".."
        let mut result = String::new();
        let mut current_width = 0;
        for ch in s.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if current_width + cw > target_width.saturating_sub(2) {
                break;
            }
            result.push(ch);
            current_width += cw;
        }
        result.push_str("..");
        current_width += 2;
        // Pad remainder
        for _ in current_width..target_width {
            result.push(' ');
        }
        result
    } else {
        let mut result = s.to_string();
        for _ in w..target_width {
            result.push(' ');
        }
        result
    }
}

/// Render the card browser screen.
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.size();
    let t = &app.theme;

    let outer = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(3),    // content
        Constraint::Length(1), // status
    ])
    .split(area);

    // Title bar
    let state = app.card_browser_state.as_ref();
    let filter_label = state.map(|s| s.filter.label()).unwrap_or("All");
    let sort_label = state.map(|s| s.sort.label()).unwrap_or("Due Date");
    let total = state.map(|s| s.entries.len()).unwrap_or(0);
    let filtered = app.card_browser_filtered_entries();
    let filtered_count = filtered.len();

    let block = Block::default()
        .title(" SRS Cards ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.info));
    let inner_area = block.inner(outer[1]);

    // header(1) + bottom page indicator(1) = 2 rows reserved
    let page_size = (inner_area.height as usize).saturating_sub(2);

    // Update page_size in state so key handler knows the page size
    if let Some(ref mut st) = app.card_browser_state {
        st.page_size = page_size.max(1);
    }

    let state = app.card_browser_state.as_ref();
    let selected = state.map(|s| s.selected).unwrap_or(0);
    let page_start = state.map(|s| s.page_start).unwrap_or(0);

    // Pagination
    let total_pages = if filtered_count == 0 {
        1
    } else {
        (filtered_count + page_size.max(1) - 1) / page_size.max(1)
    };
    let current_page = if page_size > 0 {
        page_start / page_size + 1
    } else {
        1
    };

    let title = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — SRS Cards"),
        Span::raw("  "),
        Span::styled(
            format!("[{}]", filter_label),
            Style::default().fg(t.warning),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Sort: {}", sort_label),
            Style::default().fg(t.muted),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{}/{} cards", filtered_count, total),
            Style::default().fg(t.muted),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(t.title_bar_bg)),
        outer[0],
    );

    // Content
    frame.render_widget(block, outer[1]);

    if filtered.is_empty() {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No SRS cards found.",
                Style::default().fg(t.muted),
            )),
            Line::from(""),
            Line::from("Cards are created when you set words to Learning (1-4) in the Reader,"),
            Line::from("or when you add sentence translations (T key in Reader)."),
        ]);
        frame.render_widget(msg, inner_area);
    } else {
        // Fixed column widths
        let col_type: usize = 6;
        let col_state: usize = 7;
        let col_due: usize = 10;

        let fixed = 2 + col_type + col_state + col_due;
        let remaining = (inner_area.width as usize).saturating_sub(fixed).max(20);
        let col_front = (remaining * 55 / 100).max(10);
        let col_trans = remaining.saturating_sub(col_front).max(8);

        let hdr_style = Style::default().fg(t.muted).add_modifier(Modifier::BOLD);

        // Header row
        let header = Line::from(vec![
            Span::styled("  ", hdr_style),
            Span::styled(pad_to_width("Type", col_type), hdr_style),
            Span::styled(pad_to_width("Word / Sentence", col_front), hdr_style),
            Span::styled(pad_to_width("Translation", col_trans), hdr_style),
            Span::styled(pad_to_width("State", col_state), hdr_style),
            Span::styled(pad_to_width("Due", col_due), hdr_style),
        ]);

        let mut items: Vec<ListItem> = Vec::with_capacity(page_size + 2);
        items.push(ListItem::new(header));

        // Only render the current page slice
        let page_end = (page_start + page_size).min(filtered_count);
        for display_idx in page_start..page_end {
            let &entry_idx = &filtered[display_idx];
            let entry = &state.unwrap().entries[entry_idx];
            let marker = if display_idx == selected {
                "▶ "
            } else {
                "  "
            };
            let style = if display_idx == selected {
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let type_label = match entry.card.card_type.as_str() {
                "word" => "W",
                "sentence" => "S",
                _ => "?",
            };

            let state_label = match entry.card.state.as_str() {
                "new" => "new",
                "learning" => "lrn",
                "review" => "rev",
                "relearning" => "rlrn",
                "retired" => "ret",
                s => s,
            };
            let state_color = match entry.card.state.as_str() {
                "new" => t.info,
                "learning" => t.warning,
                "review" => t.success,
                "relearning" => t.error,
                "retired" => t.muted,
                _ => t.fg,
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(
                    pad_to_width(type_label, col_type),
                    Style::default().fg(t.muted),
                ),
                Span::styled(pad_to_width(&entry.display_front, col_front), style),
                Span::styled(
                    pad_to_width(&entry.display_back, col_trans),
                    Style::default().fg(t.muted),
                ),
                Span::styled(
                    pad_to_width(state_label, col_state),
                    Style::default().fg(state_color),
                ),
                Span::styled(
                    pad_to_width(&entry.due_label, col_due),
                    Style::default().fg(t.progress_bar),
                ),
            ])));
        }

        // Page indicator at the bottom
        let page_indicator = Line::from(vec![Span::styled(
            format!("  Page {}/{}", current_page, total_pages),
            Style::default().fg(t.muted),
        )]);
        items.push(ListItem::new(page_indicator));

        let list = List::new(items);
        frame.render_widget(list, inner_area);
    }

    // Status bar
    let status = Line::from(vec![Span::styled(
        " ↑↓:navigate  ←→:page  Enter:detail  d:delete  r:reset  f:filter  s:sort  Esc:back ",
        Style::default().fg(t.muted),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(t.title_bar_bg)),
        outer[2],
    );
}
