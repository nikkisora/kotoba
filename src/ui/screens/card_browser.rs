use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
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
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();

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
    let filtered_count = app.card_browser_filtered_entries().len();

    let title = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — SRS Cards"),
        Span::raw("  "),
        Span::styled(
            format!("[{}]", filter_label),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Sort: {}", sort_label),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{}/{} cards", filtered_count, total),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    // Content
    let filtered = app.card_browser_filtered_entries();
    let selected = state.map(|s| s.selected).unwrap_or(0);

    // Fixed column widths (in terminal columns), including trailing space as separator.
    // marker(2) + type(6) + front(variable) + mode(18) + state(7) + due(10)
    let col_type: usize = 6;
    let col_mode: usize = 18;
    let col_state: usize = 7;
    let col_due: usize = 10;

    let block = Block::default()
        .title(" SRS Cards ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let inner_area = block.inner(outer[1]);
    frame.render_widget(block, outer[1]);

    if filtered.is_empty() {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No SRS cards found.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from("Cards are created when you set words to Learning (1-4) in the Reader,"),
            Line::from("or when you add sentence translations (T key in Reader)."),
        ]);
        frame.render_widget(msg, inner_area);
    } else {
        // Front column takes remaining width
        let fixed = 2 + col_type + col_mode + col_state + col_due;
        let col_front = (inner_area.width as usize).saturating_sub(fixed).max(12);

        let hdr_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        // Header row
        let header = Line::from(vec![
            Span::styled("  ", hdr_style),
            Span::styled(pad_to_width("Type", col_type), hdr_style),
            Span::styled(pad_to_width("Word / Sentence", col_front), hdr_style),
            Span::styled(pad_to_width("Mode", col_mode), hdr_style),
            Span::styled(pad_to_width("State", col_state), hdr_style),
            Span::styled(pad_to_width("Due", col_due), hdr_style),
        ]);

        let mut items: Vec<ListItem> = Vec::with_capacity(filtered.len() + 1);
        items.push(ListItem::new(header));

        for (display_idx, &entry_idx) in filtered.iter().enumerate() {
            let entry = &state.unwrap().entries[entry_idx];
            let marker = if display_idx == selected {
                "▶ "
            } else {
                "  "
            };
            let style = if display_idx == selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let type_label = match entry.card.card_type.as_str() {
                "word" => "W",
                "sentence" => "S",
                _ => "?",
            };

            let mode_label =
                crate::db::models::AnswerMode::from_str(&entry.card.answer_mode).label();

            let state_label = match entry.card.state.as_str() {
                "new" => "new",
                "learning" => "lrn",
                "review" => "rev",
                "relearning" => "rlrn",
                "retired" => "ret",
                s => s,
            };
            let state_color = match entry.card.state.as_str() {
                "new" => Color::Blue,
                "learning" => Color::Yellow,
                "review" => Color::Green,
                "relearning" => Color::Red,
                "retired" => Color::DarkGray,
                _ => Color::White,
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(
                    pad_to_width(type_label, col_type),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(pad_to_width(&entry.display_front, col_front), style),
                Span::styled(
                    pad_to_width(mode_label, col_mode),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    pad_to_width(state_label, col_state),
                    Style::default().fg(state_color),
                ),
                Span::styled(
                    pad_to_width(&entry.due_label, col_due),
                    Style::default().fg(Color::Rgb(100, 180, 100)),
                ),
            ])));
        }

        let list = List::new(items);
        frame.render_widget(list, inner_area);
    }

    // Status bar
    let status = Line::from(vec![Span::styled(
        " ↑↓:navigate  d:delete  m:change mode  r:reset  R:retire  f:filter  s:sort  Esc:back ",
        Style::default().fg(Color::DarkGray),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}
