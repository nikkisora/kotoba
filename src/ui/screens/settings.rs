use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, SettingsValue};

/// Render the settings screen.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.size();

    let outer = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(3),    // content
        Constraint::Length(1), // status
    ])
    .split(area);

    // Title bar
    let dirty = app
        .settings_state
        .as_ref()
        .map(|s| s.dirty)
        .unwrap_or(false);
    let dirty_indicator = if dirty { " [modified]" } else { "" };

    let title = Line::from(vec![
        Span::styled(
            " kotoba",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — Settings"),
        Span::styled(dirty_indicator, Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[0],
    );

    // Content: two-panel layout
    let state = match app.settings_state.as_ref() {
        Some(s) => s,
        None => return,
    };

    let content = Layout::horizontal([
        Constraint::Length(20), // category list
        Constraint::Min(30),    // settings items
    ])
    .split(outer[1]);

    // Category list
    let cat_items: Vec<ListItem> = state
        .categories
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let style = if i == state.selected_category {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let marker = if i == state.selected_category {
                "▶ "
            } else {
                "  "
            };
            ListItem::new(Line::from(Span::styled(
                format!("{}{}", marker, cat.name),
                style,
            )))
        })
        .collect();

    let cat_list = List::new(cat_items).block(
        Block::default()
            .title(" Categories ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );
    frame.render_widget(cat_list, content[0]);

    // Settings items for selected category
    if state.selected_category < state.categories.len() {
        let category = &state.categories[state.selected_category];
        let items: Vec<ListItem> = category
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let is_selected = i == state.selected_item;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let marker = if is_selected { "▶ " } else { "  " };

                let value_display = if is_selected && state.editing {
                    Span::styled(
                        format!("[{}▎]", state.edit_buffer),
                        Style::default().fg(Color::Yellow),
                    )
                } else {
                    match &item.value {
                        SettingsValue::Bool(v) => {
                            if *v {
                                Span::styled("[✓]", Style::default().fg(Color::Green))
                            } else {
                                Span::styled("[✗]", Style::default().fg(Color::Red))
                            }
                        }
                        SettingsValue::Integer(v) => {
                            Span::styled(format!("[{}]", v), Style::default().fg(Color::Yellow))
                        }
                        SettingsValue::Text(v) => {
                            Span::styled(format!("[{}]", v), Style::default().fg(Color::Yellow))
                        }
                        SettingsValue::Choice(current, _options) => Span::styled(
                            format!("[◀ {} ▶]", current),
                            Style::default().fg(Color::Cyan),
                        ),
                    }
                };

                ListItem::new(Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(&item.label, style),
                    Span::raw("  "),
                    value_display,
                    Span::raw("  "),
                    Span::styled(&item.description, Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect();

        let items_list = List::new(items).block(
            Block::default()
                .title(format!(" {} ", category.name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );
        frame.render_widget(items_list, content[1]);
    }

    // Status bar
    let editing = state.editing;
    let status_text = if editing {
        " Type value, Enter to confirm, Esc to cancel "
    } else {
        " ↑↓:navigate  ←→/Tab:category  Enter/Space:toggle/edit  s:save  Esc:back "
    };
    let status = Line::from(vec![Span::styled(
        status_text,
        Style::default().fg(Color::DarkGray),
    )]);
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::Rgb(30, 30, 50))),
        outer[2],
    );
}
