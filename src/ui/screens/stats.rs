use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, StatsFocus, StatsTimeRange};
use crate::db::models::VocabularyStatus;

/// Render the stats screen.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();
    let t = &app.theme;

    let outer = Layout::vertical([
        Constraint::Length(1), // title bar
        Constraint::Min(3),    // content
        Constraint::Length(1), // status bar
    ])
    .split(area);

    // Title bar
    let state = app.stats_state.as_ref();
    let time_range = state.map(|s| s.time_range).unwrap_or(StatsTimeRange::Month);
    let title = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Stats"),
        Span::raw("  "),
        Span::styled(
            format!("[t] {}", time_range.label()),
            Style::default().fg(t.muted),
        ),
        Span::raw("  "),
        Span::styled("[?]help", Style::default().fg(t.muted)),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(t.title_bar_bg)),
        outer[0],
    );

    // Content: left column (overview, vocab chart, status bar, SRS) + right column (coverage list)
    let content = Layout::horizontal([
        Constraint::Percentage(60), // left: stats panels
        Constraint::Percentage(40), // right: coverage list
    ])
    .split(outer[1]);

    let focus = state.map(|s| s.focus).unwrap_or(StatsFocus::Overview);

    // Left column: stack of panels
    render_left_panels(frame, content[0], app, focus);

    // Right column: per-text coverage
    render_coverage_panel(frame, content[1], app, focus);

    // Status bar
    let status = Line::from(vec![Span::styled(
        " Tab:switch panel  t:time range  ↑↓:scroll  Enter:open text  Esc:back  q:quit  ?:help ",
        Style::default().fg(t.muted),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(t.title_bar_bg)),
        outer[2],
    );
}

fn render_left_panels(frame: &mut Frame, area: Rect, app: &App, focus: StatsFocus) {
    let t = &app.theme;
    let state = match app.stats_state.as_ref() {
        Some(s) => s,
        None => return,
    };

    // Split left column into panels
    let panels = Layout::vertical([
        Constraint::Length(7),  // overview
        Constraint::Length(10), // vocabulary growth chart
        Constraint::Length(4),  // status breakdown bar
        Constraint::Min(6),     // SRS panel
    ])
    .split(area);

    let left_border = if focus == StatsFocus::Overview {
        Style::default().fg(t.accent)
    } else {
        Style::default().fg(t.info)
    };

    // ── Overview Panel ──
    render_overview(frame, panels[0], state, t, left_border);

    // ── Vocabulary Growth Chart ──
    render_vocab_chart(frame, panels[1], state, t, left_border);

    // ── Status Breakdown Bar ──
    render_status_bar_chart(frame, panels[2], state, t, left_border);

    // ── SRS Panel ──
    render_srs_panel(frame, panels[3], state, t, left_border);
}

fn render_overview(
    frame: &mut Frame,
    area: Rect,
    state: &crate::app::StatsState,
    t: &crate::ui::theme::Theme,
    border_style: Style,
) {
    let o = &state.overview;
    let streak_icon = if state.streak > 0 { "🔥 " } else { "" };

    let lines = vec![
        Line::from(vec![
            Span::styled(" Texts Read:     ", Style::default().fg(t.muted)),
            Span::styled(
                format_number(o.total_texts),
                Style::default().fg(t.stats_text),
            ),
            Span::raw("     "),
            Span::styled(" Total Vocabulary: ", Style::default().fg(t.muted)),
            Span::styled(
                format_number(o.total_vocabulary),
                Style::default().fg(t.stats_text),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Known:          ", Style::default().fg(t.muted)),
            Span::styled(format_number(o.known_words), Style::default().fg(t.success)),
            Span::raw("     "),
            Span::styled(" Learning:         ", Style::default().fg(t.muted)),
            Span::styled(
                format_number(o.learning_words),
                Style::default().fg(t.warning),
            ),
        ]),
        Line::from(vec![
            Span::styled(" New:            ", Style::default().fg(t.muted)),
            Span::styled(format_number(o.new_words), Style::default().fg(t.info)),
            Span::raw("     "),
            Span::styled(" Ignored:          ", Style::default().fg(t.muted)),
            Span::styled(format_number(o.ignored_words), Style::default().fg(t.muted)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw(" "),
            Span::raw(streak_icon),
            Span::styled(
                format!(
                    "Streak: {} day{}",
                    state.streak,
                    if state.streak == 1 { "" } else { "s" }
                ),
                Style::default()
                    .fg(if state.streak > 0 { t.warning } else { t.muted })
                    .add_modifier(if state.streak > 0 {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ]),
    ];

    let overview = Paragraph::new(lines).block(
        Block::default()
            .title(" Overview ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    frame.render_widget(overview, area);
}

fn render_vocab_chart(
    frame: &mut Frame,
    area: Rect,
    state: &crate::app::StatsState,
    t: &crate::ui::theme::Theme,
    border_style: Style,
) {
    let data = &state.known_over_time;

    // We have (date, cumulative_known) pairs.
    // Build a simple ASCII line chart using braille-style block characters.
    let inner_width = area.width.saturating_sub(2) as usize; // borders
    let inner_height = area.height.saturating_sub(2) as usize; // borders + title

    if data.is_empty() || inner_width < 4 || inner_height < 2 {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                " No vocabulary growth data yet",
                Style::default().fg(t.muted),
            )),
        ])
        .block(
            Block::default()
                .title(format!(
                    " Vocabulary Growth ({}) ",
                    state.time_range.label()
                ))
                .borders(Borders::ALL)
                .border_style(border_style),
        );
        frame.render_widget(empty, area);
        return;
    }

    // Sample/compress data to fit width
    let values: Vec<usize> = if data.len() <= inner_width {
        data.iter().map(|(_, v)| *v).collect()
    } else {
        // Downsample: pick evenly spaced points
        (0..inner_width)
            .map(|i| {
                let idx = i * (data.len() - 1) / (inner_width - 1);
                data[idx].1
            })
            .collect()
    };

    let max_val = values.iter().copied().max().unwrap_or(1).max(1);
    let min_val = values.iter().copied().min().unwrap_or(0);
    let range = (max_val - min_val).max(1);

    // Build chart rows (top = high values, bottom = low values)
    let chart_height = inner_height.saturating_sub(1); // leave 1 row for axis labels
    let mut chart_lines: Vec<Line> = Vec::new();

    // Block characters for chart: ▁▂▃▄▅▆▇█
    let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    for row in 0..chart_height {
        let row_from_bottom = chart_height - 1 - row;
        let mut spans = Vec::new();

        for col in 0..values.len().min(inner_width) {
            let val = values[col];
            let normalized = if range > 0 {
                (val - min_val) as f64 / range as f64
            } else {
                0.0
            };
            // Each row represents 1/chart_height of the range
            let row_threshold = row_from_bottom as f64 / chart_height as f64;
            let next_threshold = (row_from_bottom + 1) as f64 / chart_height as f64;

            let ch = if normalized >= next_threshold {
                '█' // fully filled
            } else if normalized > row_threshold {
                // Partially filled
                let frac = (normalized - row_threshold) / (next_threshold - row_threshold);
                let idx = (frac * 7.0).round() as usize;
                blocks[idx.min(7)]
            } else {
                ' '
            };

            spans.push(Span::styled(ch.to_string(), Style::default().fg(t.success)));
        }
        chart_lines.push(Line::from(spans));
    }

    // Axis label line
    let start_label = if let Some((d, _)) = data.first() {
        &d[5..] // MM-DD
    } else {
        ""
    };
    let end_label = if let Some((d, _)) = data.last() {
        &d[5..]
    } else {
        ""
    };
    let axis_padding = inner_width.saturating_sub(start_label.len() + end_label.len());
    let axis = format!("{}{}{}", start_label, " ".repeat(axis_padding), end_label);
    chart_lines.push(Line::from(vec![Span::styled(
        axis,
        Style::default().fg(t.muted),
    )]));

    let title = format!(
        " Vocabulary Growth ({}) — {} → {} ",
        state.time_range.label(),
        format_number(min_val),
        format_number(max_val)
    );
    let chart = Paragraph::new(chart_lines).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    frame.render_widget(chart, area);
}

fn render_status_bar_chart(
    frame: &mut Frame,
    area: Rect,
    state: &crate::app::StatsState,
    t: &crate::ui::theme::Theme,
    border_style: Style,
) {
    let ws = &state.words_by_status;

    let new = ws.get(&VocabularyStatus::New).copied().unwrap_or(0);
    let l1 = ws.get(&VocabularyStatus::Learning1).copied().unwrap_or(0);
    let l2 = ws.get(&VocabularyStatus::Learning2).copied().unwrap_or(0);
    let l3 = ws.get(&VocabularyStatus::Learning3).copied().unwrap_or(0);
    let l4 = ws.get(&VocabularyStatus::Learning4).copied().unwrap_or(0);
    let known = ws.get(&VocabularyStatus::Known).copied().unwrap_or(0);
    let ignored = ws.get(&VocabularyStatus::Ignored).copied().unwrap_or(0);
    let total = new + l1 + l2 + l3 + l4 + known + ignored;

    let inner_width = area.width.saturating_sub(2) as usize;

    if total == 0 || inner_width < 10 {
        let empty = Paragraph::new(Line::from(Span::styled(
            " No vocabulary data",
            Style::default().fg(t.muted),
        )))
        .block(
            Block::default()
                .title(" Status Breakdown ")
                .borders(Borders::ALL)
                .border_style(border_style),
        );
        frame.render_widget(empty, area);
        return;
    }

    // Build a stacked horizontal bar
    let segments: Vec<(usize, ratatui::style::Color, &str)> = vec![
        (known, t.success, "Known"),
        (l1, t.vocab_l1_bg, "L1"),
        (l2, t.vocab_l2_bg, "L2"),
        (l3, t.vocab_l3_bg, "L3"),
        (l4, t.vocab_l4_bg, "L4"),
        (new, t.info, "New"),
        (ignored, t.muted, "Ign"),
    ];

    // Build the bar
    let mut bar_spans: Vec<Span> = Vec::new();
    let mut chars_used = 0usize;

    for (count, color, _label) in &segments {
        if *count == 0 {
            continue;
        }
        let width = (*count as f64 / total as f64 * inner_width as f64).round() as usize;
        let width = width.max(if *count > 0 { 1 } else { 0 }); // at least 1 char if non-zero
        let remaining = inner_width.saturating_sub(chars_used);
        let width = width.min(remaining);
        if width > 0 {
            bar_spans.push(Span::styled("█".repeat(width), Style::default().fg(*color)));
            chars_used += width;
        }
    }
    // Fill any remaining space
    if chars_used < inner_width {
        bar_spans.push(Span::raw(" ".repeat(inner_width - chars_used)));
    }

    // Legend line
    let mut legend_spans: Vec<Span> = Vec::new();
    for (count, color, label) in &segments {
        if *count == 0 {
            continue;
        }
        if !legend_spans.is_empty() {
            legend_spans.push(Span::raw(" "));
        }
        legend_spans.push(Span::styled("█", Style::default().fg(*color)));
        let pct = *count * 100 / total;
        legend_spans.push(Span::styled(
            format!("{}:{}", label, pct),
            Style::default().fg(t.muted),
        ));
        legend_spans.push(Span::styled("%", Style::default().fg(t.muted)));
    }

    let lines = vec![Line::from(bar_spans), Line::from(legend_spans)];

    let bar = Paragraph::new(lines).block(
        Block::default()
            .title(" Status Breakdown ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    frame.render_widget(bar, area);
}

fn render_srs_panel(
    frame: &mut Frame,
    area: Rect,
    state: &crate::app::StatsState,
    t: &crate::ui::theme::Theme,
    border_style: Style,
) {
    let srs = &state.srs;

    let mut lines = vec![
        Line::from(vec![
            Span::styled(" Due Now:       ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>6}", format_number(srs.due_today)),
                if srs.due_today > 0 {
                    Style::default().fg(t.warning).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.stats_text)
                },
            ),
            Span::raw("     "),
            Span::styled(" Due Tomorrow:  ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>6}", format_number(srs.due_tomorrow)),
                Style::default().fg(t.stats_text),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Reviews Today: ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>6}", format_number(srs.reviews_today)),
                Style::default().fg(t.stats_text),
            ),
            Span::raw("     "),
            Span::styled(" Total Reviews: ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>6}", format_number(srs.total_reviews)),
                Style::default().fg(t.stats_text),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Accuracy (7d): ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>5}%", srs.avg_accuracy_7d),
                accuracy_style(t, srs.avg_accuracy_7d),
            ),
            Span::raw("     "),
            Span::styled(" Accuracy (30d):", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>5}%", srs.avg_accuracy_30d),
                accuracy_style(t, srs.avg_accuracy_30d),
            ),
        ]),
    ];

    if srs.retention_rate > 0.0 {
        lines.push(Line::from(vec![
            Span::styled(" Retention:     ", Style::default().fg(t.muted)),
            Span::styled(
                format!("{:>5.1}%", srs.retention_rate),
                accuracy_style(t, srs.retention_rate.round() as u8),
            ),
        ]));
    }

    // Card counts
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" Cards: ", Style::default().fg(t.muted)),
        Span::styled(
            format!("{} total", format_number(srs.total_cards)),
            Style::default().fg(t.stats_text),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} new", format_number(srs.cards_new)),
            Style::default().fg(t.info),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} learning", format_number(srs.cards_learning)),
            Style::default().fg(t.warning),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} review", format_number(srs.cards_review)),
            Style::default().fg(t.success),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} retired", format_number(srs.cards_retired)),
            Style::default().fg(t.muted),
        ),
    ]));

    let panel = Paragraph::new(lines).block(
        Block::default()
            .title(" SRS Review ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    frame.render_widget(panel, area);
}

fn render_coverage_panel(frame: &mut Frame, area: Rect, app: &App, focus: StatsFocus) {
    let t = &app.theme;
    let state = match app.stats_state.as_ref() {
        Some(s) => s,
        None => return,
    };

    let coverages = &state.coverages;
    let selected = state.coverage_selected;

    let cov_border = if focus == StatsFocus::Coverage {
        Style::default().fg(t.accent)
    } else {
        Style::default().fg(t.info)
    };

    if coverages.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                " No texts read yet",
                Style::default().fg(t.muted),
            )),
        ])
        .block(
            Block::default()
                .title(" Text Coverage ")
                .borders(Borders::ALL)
                .border_style(cov_border),
        );
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = coverages
        .iter()
        .enumerate()
        .map(|(i, cov)| {
            let is_selected = focus == StatsFocus::Coverage && i == selected;
            let marker = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            // Coverage bar
            let bar_width = 10;
            let filled = (cov.coverage_pct / 100.0 * bar_width as f64).round() as usize;
            let filled = filled.min(bar_width);
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_width - filled));

            // Truncate title to fit (char-aware to avoid slicing inside multi-byte chars)
            let max_title = (area.width as usize).saturating_sub(30);
            let title: String = {
                let mut width = 0;
                let mut result = String::new();
                let limit = max_title.saturating_sub(1); // leave room for '…'
                for ch in cov.title.chars() {
                    let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                    if width + w > limit {
                        result.push('…');
                        break;
                    }
                    result.push(ch);
                    width += w;
                }
                if width <= limit && result.len() == cov.title.len() {
                    result // no truncation needed
                } else if !result.ends_with('…') {
                    result.push('…');
                    result
                } else {
                    result
                }
            };

            ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(title, style),
                Span::raw("  "),
                Span::styled(
                    bar,
                    Style::default().fg(coverage_color(t, cov.coverage_pct)),
                ),
                Span::styled(
                    format!(" {:>3.0}%", cov.coverage_pct),
                    Style::default().fg(t.stats_text),
                ),
            ]))
        })
        .collect();

    // Show details for selected text below the list
    let detail_height = 5u16;
    let list_area;
    let detail_area;

    if focus == StatsFocus::Coverage && !coverages.is_empty() {
        let split =
            Layout::vertical([Constraint::Min(3), Constraint::Length(detail_height)]).split(area);
        list_area = split[0];
        detail_area = Some(split[1]);
    } else {
        list_area = area;
        detail_area = None;
    }

    let list = List::new(items).block(
        Block::default()
            .title(" Text Coverage ")
            .borders(Borders::ALL)
            .border_style(cov_border),
    );
    frame.render_widget(list, list_area);

    // Detail panel for selected coverage
    if let Some(d_area) = detail_area {
        if let Some(cov) = coverages.get(selected) {
            let detail_lines = vec![
                Line::from(vec![
                    Span::styled(" Tokens: ", Style::default().fg(t.muted)),
                    Span::styled(
                        format_number(cov.total_tokens),
                        Style::default().fg(t.stats_text),
                    ),
                    Span::raw("  "),
                    Span::styled("Known: ", Style::default().fg(t.muted)),
                    Span::styled(
                        format_number(cov.known_tokens),
                        Style::default().fg(t.success),
                    ),
                    Span::raw("  "),
                    Span::styled("Learning: ", Style::default().fg(t.muted)),
                    Span::styled(
                        format_number(cov.learning_tokens),
                        Style::default().fg(t.warning),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(" New: ", Style::default().fg(t.muted)),
                    Span::styled(format_number(cov.new_tokens), Style::default().fg(t.info)),
                    Span::raw("  "),
                    Span::styled("Ignored: ", Style::default().fg(t.muted)),
                    Span::styled(
                        format_number(cov.ignored_tokens),
                        Style::default().fg(t.muted),
                    ),
                    Span::raw("  "),
                    Span::styled("Coverage: ", Style::default().fg(t.muted)),
                    Span::styled(
                        format!("{:.1}%", cov.coverage_pct),
                        Style::default()
                            .fg(coverage_color(t, cov.coverage_pct))
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(Span::styled(
                    " Enter to open in Reader",
                    Style::default().fg(t.muted),
                )),
            ];
            let detail = Paragraph::new(detail_lines).block(
                Block::default()
                    .title(format!(" {} ", cov.title))
                    .borders(Borders::ALL)
                    .border_style(cov_border),
            );
            frame.render_widget(detail, d_area);
        }
    }
}

// ── Helpers ──

fn accuracy_style(t: &crate::ui::theme::Theme, pct: u8) -> Style {
    let color = if pct >= 90 {
        t.success
    } else if pct >= 70 {
        t.warning
    } else if pct > 0 {
        t.error
    } else {
        t.muted
    };
    Style::default().fg(color)
}

fn coverage_color(t: &crate::ui::theme::Theme, pct: f64) -> ratatui::style::Color {
    if pct >= 90.0 {
        t.success
    } else if pct >= 70.0 {
        t.warning
    } else if pct >= 50.0 {
        t.info
    } else {
        t.error
    }
}

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
