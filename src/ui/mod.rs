pub mod components;
pub mod events;
pub mod screens;

use ratatui::widgets::Clear;
use ratatui::Frame;

use crate::app::{App, Screen};

/// Main render function that dispatches to the active screen.
pub fn render(frame: &mut Frame, app: &App) {
    // Clear the entire frame first to prevent artifacts from previous screens.
    frame.render_widget(Clear, frame.size());

    match &app.screen {
        Screen::Home => screens::home::render(frame, app),
        Screen::Library => screens::library::render(frame, app),
        Screen::ChapterSelect { .. } => screens::chapter_select::render(frame, app),
        Screen::Reader => screens::reader::render(frame, app),
        Screen::Review => screens::review::render(frame, app),
        Screen::CardBrowser => screens::card_browser::render(frame, app),
        Screen::Settings => screens::settings::render(frame, app),
    }

    // Render popup overlay if any
    if let Some(ref popup) = app.popup {
        components::popup::render_popup(frame, app, popup);
    }

    // Render status bar message
    if let Some((ref msg, _)) = app.message {
        components::status_bar::render_message(frame, msg);
    }
}
