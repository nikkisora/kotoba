pub mod components;
pub mod events;
pub mod screens;
pub mod theme;

use ratatui::style::Style;
use ratatui::widgets::{Block, Clear};
use ratatui::Frame;

use crate::app::{App, Screen};

/// Main render function that dispatches to the active screen.
pub fn render(frame: &mut Frame, app: &mut App) {
    // Clear the entire frame first to prevent artifacts from previous screens.
    frame.render_widget(Clear, frame.size());

    // Fill with theme base background and foreground so light themes work.
    let base = Block::default().style(Style::default().bg(app.theme.bg).fg(app.theme.fg));
    frame.render_widget(base, frame.size());

    match &app.screen.clone() {
        Screen::Home => screens::home::render(frame, &mut *app),
        Screen::Library => screens::library::render(frame, app),
        Screen::ChapterSelect { .. } => screens::chapter_select::render(frame, app),
        Screen::Reader => screens::reader::render(frame, app),
        Screen::Review => screens::review::render(frame, app),
        Screen::CardBrowser => screens::card_browser::render(frame, app),
        Screen::Settings => screens::settings::render(frame, app),
        Screen::Stats => screens::stats::render(frame, app),
    }

    // Render popup overlay if any
    if let Some(ref popup) = app.popup {
        components::popup::render_popup(frame, app, popup);
    }

    // Render status bar message
    if let Some((ref msg, _)) = app.message {
        components::status_bar::render_message(frame, msg, &app.theme);
    }
}
