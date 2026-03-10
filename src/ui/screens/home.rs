use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, HomeFocus};
use crate::db::models::DailyActivity;
use crate::ui::theme::Theme;

/// Render a mini progress bar using block characters.
fn progress_bar(current: u64, total: u64, width: usize) -> String {
    if total == 0 {
        return "░".repeat(width);
    }
    let filled = ((current as f64 / total as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// Map a daily activity total to a heatmap color.
fn activity_color(t: &Theme, total: i64) -> ratatui::style::Color {
    match total {
        0 => t.heatmap_empty,
        1..=19 => t.heatmap_low,
        20..=79 => t.heatmap_mid,
        80..=159 => t.heatmap_high,
        _ => t.heatmap_max,
    }
}

// ── Heatmap grid helpers ────────────────────────────────────────────

/// Number of weeks to show in the heatmap (columns).
const HEATMAP_WEEKS: usize = 26; // ~6 months
/// Number of rows in the heatmap (days of the week: Mon-Sun).
const HEATMAP_ROWS: usize = 7;

/// Build a 7×N grid of (date_string, activity_total) aligned to calendar weeks.
/// Returns (grid, week_count, today_index) where grid[row][col] = Option<(date, total)>.
/// Row 0 = Monday, Row 6 = Sunday. Columns are weeks, rightmost = current week.
fn build_heatmap_grid(
    activity: &[DailyActivity],
    weeks: usize,
) -> (Vec<Vec<Option<(String, i64)>>>, usize, usize) {
    use std::collections::HashMap;

    // Build a lookup map from date -> total
    let activity_map: HashMap<&str, i64> = activity
        .iter()
        .map(|a| (a.date.as_str(), a.total()))
        .collect();

    // We need to figure out today and work backward.
    // Since we don't have chrono, we'll use the date strings from the DB activity data
    // and fill in a grid based on simple date arithmetic.
    // We'll use our own date math.

    let total_days = weeks * 7;
    let mut grid: Vec<Vec<Option<(String, i64)>>> = vec![vec![None; weeks]; HEATMAP_ROWS];
    let mut today_idx = 0usize;

    // Generate all dates for the grid period
    // Today is at the rightmost column, at the appropriate row for its day of week.
    // We'll compute dates by working backward from "today" using simple arithmetic.

    // Get today's date string from the activity data, or use a placeholder
    // We need to compute today. Let's find the max date in activity, or just use
    // date strings. Since we can't call the DB here, we'll compute using system time.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Unix timestamp to (year, month, day, day_of_week)
    let today = unix_to_date(now as i64);
    // day_of_week: 0=Monday, 6=Sunday (ISO)
    let today_dow = today.3 as usize;

    // The grid ends at today. The last column contains the current (possibly partial) week.
    // The rightmost column, row=today_dow is today.
    // Total cells from top-left to today = (weeks-1)*7 + today_dow
    // We need to fill the grid going backward from today.

    // For each cell (row, col), compute how many days before today it is:
    // days_before = (weeks - 1 - col) * 7 + (today_dow as isize - row as isize)
    // But we want the grid to be aligned: last column = current week,
    // so cell (row, weeks-1) = this week's day `row`.
    // days_before_today for cell (row, col) = (weeks - 1 - col) * 7 + (today_dow - row)
    // This can be negative if row > today_dow in the last column (future days).

    for col in 0..weeks {
        for row in 0..HEATMAP_ROWS {
            let days_offset =
                (weeks as isize - 1 - col as isize) * 7 + (today_dow as isize - row as isize);
            if days_offset < 0 {
                // Future day in the current week — leave as None
                continue;
            }
            if days_offset >= total_days as isize {
                continue;
            }
            let ts = now as i64 - days_offset as i64 * 86400;
            let d = unix_to_date(ts);
            let date_str = format!("{:04}-{:02}-{:02}", d.0, d.1, d.2);
            let total = activity_map.get(date_str.as_str()).copied().unwrap_or(0);
            grid[row][col] = Some((date_str, total));

            if days_offset == 0 {
                today_idx = row * weeks + col;
            }
        }
    }

    (grid, weeks, today_idx)
}

/// Convert Unix timestamp to (year, month, day, day_of_week).
/// day_of_week: 0=Monday, 6=Sunday (ISO).
fn unix_to_date(ts: i64) -> (i32, u32, u32, u32) {
    // Days since Unix epoch (1970-01-01, which was a Thursday = dow 3)
    let days = (ts / 86400) as i32;
    // Day of week: 1970-01-01 was Thursday. Monday=0, so Thursday=3.
    let dow = ((days % 7 + 3) % 7) as u32; // 0=Mon, 6=Sun

    // Convert days since epoch to y/m/d using the algorithm from
    // Howard Hinnant's chrono-compatible date algorithms.
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d, dow)
}

/// Get a short month label for the first day of each week that starts a new month.
fn month_labels(grid: &[Vec<Option<(String, i64)>>], weeks: usize) -> Vec<(usize, &'static str)> {
    let mut labels = Vec::new();
    let mut last_month = 0u32;
    let months = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    for col in 0..weeks {
        // Check Monday (row 0) of this week
        if let Some(Some((ref date, _))) = grid.first().map(|r| r.get(col)).flatten() {
            if let Some(month) = date.split('-').nth(1).and_then(|m| m.parse::<u32>().ok()) {
                if month != last_month {
                    last_month = month;
                    if (month as usize) < months.len() {
                        labels.push((col, months[month as usize]));
                    }
                }
            }
        }
    }
    labels
}

/// Render the heatmap calendar widget.
fn render_heatmap(frame: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let home = match app.home_state.as_ref() {
        Some(h) => h,
        None => return,
    };

    let focus = home.focus;
    let cursor = home.heatmap_cursor;

    // Determine how many weeks fit in the available width
    // Layout: 4 chars for day labels ("Mon ") + 2 chars per week column + 1 for border
    let border_overhead = 2; // left + right border
    let label_width = 4; // "Mon " etc
    let available = area
        .width
        .saturating_sub(border_overhead + label_width as u16) as usize;
    let weeks = (available / 2).min(HEATMAP_WEEKS).max(4);

    let (grid, _, today_idx) = build_heatmap_grid(&home.activity, weeks);
    let month_labs = month_labels(&grid, weeks);

    // Build month label line
    let mut month_line_parts: Vec<Span> = vec![Span::raw("    ")]; // indent for day labels
    {
        let mut col = 0usize;
        for &(label_col, label) in &month_labs {
            // Add spaces to reach label_col
            while col < label_col {
                month_line_parts.push(Span::raw("  "));
                col += 1;
            }
            // Place the label (3 chars), but truncate to fit
            let label_str = if label.len() <= (weeks - col) * 2 {
                format!("{:<3}", label)
            } else {
                label[..((weeks - col) * 2).min(label.len())].to_string()
            };
            let chars_used = label_str.len();
            month_line_parts.push(Span::styled(label_str, Style::default().fg(t.muted)));
            // Advance col by chars used / 2 (each col is 2 chars wide)
            col += (chars_used + 1) / 2;
        }
    }

    let day_labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(month_line_parts));

    for row in 0..HEATMAP_ROWS {
        let mut spans: Vec<Span> = Vec::new();
        // Day label (show Mon, Wed, Fri only for compactness, blank for others)
        let day_label = match row {
            0 | 2 | 4 => format!("{} ", day_labels[row]),
            _ => "   ".to_string(),
        };
        spans.push(Span::styled(day_label, Style::default().fg(t.muted)));

        for col in 0..weeks {
            let cell_idx = row * weeks + col;
            let (cell_char, cell_color) = match &grid[row][col] {
                None => ("  ", t.bg), // future day — blank
                Some((_, total)) => {
                    let color = activity_color(t, *total);
                    ("██", color)
                }
            };

            let is_cursor = focus == HomeFocus::Heatmap && cell_idx == cursor;
            let is_today = cell_idx == today_idx;

            if is_cursor {
                // Highlight with cursor color border
                spans.push(Span::styled(
                    cell_char,
                    Style::default()
                        .fg(t.heatmap_cursor)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if is_today && grid[row][col].is_some() {
                // Subtle underline for today
                let color = match &grid[row][col] {
                    Some((_, total)) => activity_color(t, *total),
                    None => t.heatmap_empty,
                };
                spans.push(Span::styled(
                    cell_char,
                    Style::default()
                        .fg(color)
                        .add_modifier(Modifier::UNDERLINED),
                ));
            } else {
                spans.push(Span::styled(cell_char, Style::default().fg(cell_color)));
            }
        }

        lines.push(Line::from(spans));
    }

    // Legend line
    let mut legend_spans = vec![Span::raw("    ")];
    legend_spans.push(Span::styled("Less ", Style::default().fg(t.muted)));
    legend_spans.push(Span::styled("██", Style::default().fg(t.heatmap_empty)));
    legend_spans.push(Span::styled("██", Style::default().fg(t.heatmap_low)));
    legend_spans.push(Span::styled("██", Style::default().fg(t.heatmap_mid)));
    legend_spans.push(Span::styled("██", Style::default().fg(t.heatmap_high)));
    legend_spans.push(Span::styled("██", Style::default().fg(t.heatmap_max)));
    legend_spans.push(Span::styled(" More", Style::default().fg(t.muted)));

    // If cursor is on a specific day, show details
    if focus == HomeFocus::Heatmap {
        let cursor_row = cursor / weeks;
        let cursor_col = cursor % weeks;
        if cursor_row < HEATMAP_ROWS {
            if let Some(Some((ref date, _))) =
                grid.get(cursor_row).map(|r| r.get(cursor_col)).flatten()
            {
                // Look up full activity for this date
                let act = home.activity.iter().find(|a| a.date == *date);
                let detail = if let Some(a) = act {
                    format!(
                        "  {} — {}r {}s {}w",
                        date, a.reviews_completed, a.sentences_read, a.words_learned
                    )
                } else {
                    format!("  {} — no activity", date)
                };
                legend_spans.push(Span::styled(detail, Style::default().fg(t.stats_text)));
            }
        }
    }

    lines.push(Line::from(legend_spans));

    let border_style = if focus == HomeFocus::Heatmap {
        Style::default().fg(t.accent)
    } else {
        Style::default().fg(t.info)
    };

    let heatmap = Paragraph::new(lines).block(
        Block::default()
            .title(" Activity ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    frame.render_widget(heatmap, area);

    // Store heatmap_cells in the app state (we can't mutate here, so we rely on
    // the caller to set it. We'll compute it deterministically from area width.)
}

/// Compute the number of weeks that fit in a given width (matches render logic).
pub fn heatmap_weeks_for_width(width: u16) -> usize {
    let border_overhead = 2u16;
    let label_width = 4u16;
    let available = width.saturating_sub(border_overhead + label_width) as usize;
    (available / 2).min(HEATMAP_WEEKS).max(4)
}

/// Render the quick stats panel.
fn render_quick_stats(frame: &mut Frame, area: Rect, app: &App) {
    let t = &app.theme;
    let home = match app.home_state.as_ref() {
        Some(h) => h,
        None => return,
    };

    let streak_icon = if home.streak > 0 { "🔥" } else { "  " };

    let mut lines = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::raw(streak_icon),
            Span::styled(
                format!(
                    " Streak: {} day{}",
                    home.streak,
                    if home.streak == 1 { "" } else { "s" }
                ),
                Style::default()
                    .fg(if home.streak > 0 { t.warning } else { t.muted })
                    .add_modifier(if home.streak > 0 {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Words Known:   ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>6}", format_number(home.words_known)),
                Style::default().fg(t.success),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Words Learning:", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>6}", format_number(home.words_learning)),
                Style::default().fg(t.warning),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Reviews Today: ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>6}", home.reviews_today),
                Style::default().fg(t.stats_text),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Due Now:       ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>6}", home.due_card_counts.0),
                if home.due_card_counts.0 > 0 {
                    Style::default().fg(t.warning).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.stats_text)
                },
            ),
        ]),
    ];

    if home.accuracy_7d > 0 {
        lines.push(Line::from(vec![
            Span::styled(" Accuracy (7d): ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>5}%", home.accuracy_7d),
                Style::default().fg(t.stats_text),
            ),
        ]));
    }

    let stats = Paragraph::new(lines).block(
        Block::default()
            .title(" Quick Stats ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(t.info)),
    );
    frame.render_widget(stats, area);
}

/// Format a number with comma separators (simple implementation).
fn format_number(n: usize) -> String {
    if n < 1000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Render the home screen.
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.size();

    // Update heatmap_cells based on actual terminal width so key handlers know bounds.
    // The heatmap gets 65% of the content width.
    let heatmap_area_width = (area.width as f32 * 0.65) as u16;
    let weeks = heatmap_weeks_for_width(heatmap_area_width);
    let total_cells = 7 * weeks;
    if let Some(ref mut home) = app.home_state {
        if home.heatmap_cells != total_cells {
            home.heatmap_cells = total_cells;
            // Clamp cursor
            if home.heatmap_cursor >= total_cells {
                home.heatmap_cursor = total_cells.saturating_sub(1);
            }
        }
    }
    let t = &app.theme;

    let outer = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(3),    // content
        Constraint::Length(1), // status
    ])
    .split(area);

    // Title bar
    let home = app.home_state.as_ref();
    let due_counts = home.map(|h| h.due_card_counts).unwrap_or((0, 0));
    let review_indicator = if due_counts.0 > 0 {
        vec![
            Span::raw("  "),
            Span::styled(
                format!("[r] {} due", due_counts.0),
                Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };
    let mut title_spans = vec![
        Span::styled(
            " kotoba",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Home"),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(t.muted)),
    ];
    title_spans.extend(review_indicator);
    let title = Line::from(title_spans);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(t.title_bar_bg)),
        outer[0],
    );

    // Dict warning banner
    let dict_loaded = home.map(|h| h.dict_loaded).unwrap_or(true);
    let content_area = if !dict_loaded {
        let banner_split = Layout::vertical([
            Constraint::Length(2), // warning banner
            Constraint::Min(1),    // remaining content
        ])
        .split(outer[1]);

        let banner = Paragraph::new(Line::from(vec![
            Span::styled(
                " ! ",
                Style::default().fg(t.error).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Dictionary not loaded. Run ",
                Style::default().fg(t.warning),
            ),
            Span::styled(
                "kotoba setup-dict",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to download and import JMdict for word lookups.",
                Style::default().fg(t.warning),
            ),
        ]))
        .style(Style::default().bg(t.title_bar_bg));
        frame.render_widget(banner, banner_split[0]);
        banner_split[1]
    } else {
        outer[1]
    };

    // Content: split into dashboard (top) and text list (bottom)
    // Heatmap needs: 1 (month labels) + 7 (day rows) + 1 (legend) + 2 (borders) = 11 rows
    let dashboard_height = 11u16;

    let content_split = Layout::vertical([
        Constraint::Length(dashboard_height), // dashboard: heatmap + quick stats
        Constraint::Min(3),                   // text list
    ])
    .split(content_area);

    // Dashboard: heatmap (left) + quick stats (right)
    let dashboard = Layout::horizontal([
        Constraint::Percentage(65), // heatmap
        Constraint::Percentage(35), // quick stats
    ])
    .split(content_split[0]);

    render_heatmap(frame, dashboard[0], app);
    render_quick_stats(frame, dashboard[1], app);

    // Text list
    let show_finished = home.map(|h| h.show_finished).unwrap_or(false);
    let selected = home.map(|h| h.selected).unwrap_or(0);
    let focus = home.map(|h| h.focus).unwrap_or(HomeFocus::TextList);

    let filtered: Vec<&crate::db::models::Text> = home
        .map(|h| {
            h.recent_texts
                .iter()
                .filter(|t| {
                    if show_finished {
                        return true;
                    }
                    t.total_sentences == 0 || t.last_sentence_index < t.total_sentences - 1
                })
                .collect()
        })
        .unwrap_or_default();

    if !filtered.is_empty() {
        let stats_map = home.map(|h| &h.recent_stats);

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let is_selected = focus == HomeFocus::TextList && i == selected;
                let marker = if is_selected { "▶ " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let progress = (text.last_sentence_index + 1).min(text.total_sentences);
                let pbar = progress_bar(progress as u64, text.total_sentences as u64, 12);
                let pct = if text.total_sentences > 0 {
                    (progress * 100 / text.total_sentences) as u8
                } else {
                    0
                };

                let stats_str = stats_map
                    .and_then(|m| m.get(&text.id))
                    .map(|s| {
                        let kpct = if s.unique_vocab == 0 {
                            0
                        } else {
                            s.known_count * 100 / s.unique_vocab
                        };
                        let lpct = if s.unique_vocab == 0 {
                            0
                        } else {
                            s.learning_count * 100 / s.unique_vocab
                        };
                        let npct = if s.unique_vocab == 0 {
                            0
                        } else {
                            s.new_count * 100 / s.unique_vocab
                        };
                        format!("  K:{}% L:{}% N:{}%", kpct, lpct, npct)
                    })
                    .unwrap_or_default();

                ListItem::new(Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(&text.title, style),
                    Span::raw("  "),
                    Span::styled(
                        format!("{} {}%", pbar, pct),
                        Style::default().fg(t.progress_bar),
                    ),
                    Span::styled(stats_str, Style::default().fg(t.stats_text)),
                ]))
            })
            .collect();

        let block_title = if show_finished {
            " Recently Read (all) [f] "
        } else {
            " Recently Read (in progress) [f] "
        };
        let text_border = if focus == HomeFocus::TextList {
            Style::default().fg(t.accent)
        } else {
            Style::default().fg(t.info)
        };
        let list = List::new(items).block(
            Block::default()
                .title(block_title)
                .borders(Borders::ALL)
                .border_style(text_border),
        );
        frame.render_widget(list, content_split[1]);
    } else {
        let mut welcome_lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Welcome to kotoba!",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if !dict_loaded {
            welcome_lines.push(Line::from(Span::styled(
                "Set up the dictionary first:",
                Style::default().fg(t.warning),
            )));
            welcome_lines.push(Line::from("  kotoba setup-dict"));
            welcome_lines.push(Line::from(""));
        }

        welcome_lines.extend([
            Line::from("Get started:"),
            Line::from("  [l] Open Library"),
            Line::from("  [i] Import text (clipboard, URL, file, Syosetu)"),
            Line::from(""),
            Line::from("Or use the CLI:"),
            Line::from("  kotoba import <file>"),
            Line::from("  kotoba syosetu <ncode>"),
        ]);

        let msg = Paragraph::new(welcome_lines).block(
            Block::default()
                .title(" Home ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(t.info)),
        );
        frame.render_widget(msg, content_split[1]);
    }

    // Status bar
    let status = Line::from(vec![Span::styled(
        " ↑↓:navigate  Tab:panel  Enter:open  r:review  c:cards  s:settings  S:stats  f:finished  l:library  i:import  q:quit ",
        Style::default().fg(t.muted),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(t.title_bar_bg)),
        outer[2],
    );
}
