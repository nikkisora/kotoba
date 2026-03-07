mod app;
mod config;
mod core;
mod db;
mod import;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use app::{App, PopupState, Screen};
use db::models::{self, VocabularyStatus};
use ui::events::{Event, EventLoop};

#[derive(Parser)]
#[command(
    name = "kotoba",
    version,
    about = "Terminal-based Japanese language learning app"
)]
struct Cli {
    /// Path to config file
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Import content for reading (auto-detects format: .txt, .srt, .ass, .epub)
    Import {
        /// Path to the file to import
        #[arg(group = "source")]
        file: Option<PathBuf>,

        /// Import from system clipboard
        #[arg(long, group = "source")]
        clipboard: bool,

        /// Import from a URL (fetches and extracts article text)
        #[arg(long, group = "source")]
        url: Option<String>,
    },
    /// Tokenize a Japanese text string
    Tokenize {
        /// The Japanese text to tokenize
        text: String,
    },
    /// Look up a word in JMdict
    Dict {
        /// The word to look up (kanji or kana)
        word: String,
    },
    /// Import JMdict XML dictionary into the database
    ImportDict {
        /// Path to JMdict_e.xml file
        path: PathBuf,
    },
    /// Download and set up the JMdict dictionary automatically
    SetupDict,
    /// Import a Syosetu (小説家になろう) novel chapter
    Syosetu {
        /// Novel URL or ncode (e.g. n1234ab)
        ncode: String,

        /// Chapter number to import (omit to see chapter list)
        #[arg(short, long)]
        chapter: Option<usize>,
    },
    /// Launch the TUI reader
    Run,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cfg = config::AppConfig::load(cli.config.as_deref())?;
    let conn = db::connection::open_or_create(&cfg.db_path())?;
    db::schema::run_migrations(&conn)?;

    match cli.command {
        Commands::Import {
            file,
            clipboard,
            url,
        } => {
            if clipboard {
                let text_id = import::clipboard::import_clipboard(&conn)?;
                println!("Imported clipboard text with id: {text_id}");
            } else if let Some(url) = url {
                let text_id = import::web::import_url(&url, &conn)?;
                println!("Imported web content with id: {text_id}");
            } else if let Some(file) = file {
                // Auto-detect format by extension
                let ext = file
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                match ext.as_str() {
                    "srt" | "ass" | "ssa" => {
                        let text_id = import::subtitle::import_subtitle(&file, &conn)?;
                        println!("Imported subtitle with id: {text_id}");
                    }
                    "epub" => {
                        let chapters = import::epub::import_epub(&file, &conn)?;
                        println!("Imported {} chapters:", chapters.len());
                        for (id, title) in &chapters {
                            println!("  [{}] {}", id, title);
                        }
                    }
                    _ => {
                        // Default: plain text
                        let text_id = import::text::import_file(&file, &conn)?;
                        println!("Imported text with id: {text_id}");
                    }
                }
            } else {
                anyhow::bail!("Specify a file, --clipboard, or --url to import");
            }
        }
        Commands::Tokenize { text } => {
            let tokens = core::tokenizer::tokenize_sentence(&text)?;
            println!(
                "{:<20} {:<20} {:<15} {:<15} {:<20} {:<20}",
                "Surface", "Base Form", "Reading", "POS", "Conj. Form", "Conj. Type"
            );
            println!("{}", "-".repeat(110));
            for t in &tokens {
                println!(
                    "{:<20} {:<20} {:<15} {:<15} {:<20} {:<20}",
                    t.surface,
                    t.base_form,
                    t.reading,
                    t.pos,
                    t.conjugation_form,
                    t.conjugation_type
                );
            }
        }
        Commands::Dict { word } => {
            let entries = core::dictionary::lookup(&conn, &word, None)?;
            if entries.is_empty() {
                println!("No entries found for '{word}'");
            } else {
                for entry in &entries {
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    if !entry.kanji_forms.is_empty() {
                        println!("Kanji: {}", entry.kanji_forms.join(", "));
                    }
                    println!("Reading: {}", entry.readings.join(", "));
                    for (i, sense) in entry.senses.iter().enumerate() {
                        let pos = sense.pos.join(", ");
                        let glosses = sense.glosses.join("; ");
                        println!("  {}. [{}] {}", i + 1, pos, glosses);
                    }
                }
            }
        }
        Commands::ImportDict { path } => {
            core::dictionary::import_jmdict(&path, &conn)?;
        }
        Commands::SetupDict => {
            let db_path = cfg.db_path();
            let data_dir = db_path.parent().unwrap_or(std::path::Path::new("."));
            core::dictionary::setup_dict(&conn, data_dir)?;
        }
        Commands::Syosetu { ncode, chapter } => {
            let ncode = import::syosetu::parse_ncode(&ncode)?;

            if let Some(ch) = chapter {
                let text_id = import::syosetu::import_chapter(&ncode, ch, &conn)?;
                println!("Imported chapter {} with id: {text_id}", ch);
            } else {
                // Show novel info and chapter list
                println!("Fetching novel info for {}...", ncode);
                let novel = import::syosetu::fetch_novel_info(&ncode)?;
                println!("Title: {}", novel.title);
                println!("Author: {}", novel.author);
                println!("Chapters: {}", novel.total_chapters);
                println!();
                for ch in &novel.chapters {
                    println!("  {:>4}. {}", ch.number, ch.title);
                }
                println!();
                println!(
                    "Import a chapter with: kotoba syosetu {} --chapter <N>",
                    ncode
                );
            }
        }
        Commands::Run => {
            drop(conn); // Close CLI connection; TUI opens its own
            run_tui(cfg)?;
        }
    }

    Ok(())
}

/// Run the TUI application.
fn run_tui(config: config::AppConfig) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(crossterm::event::EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Ensure terminal is restored on panic
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        let _ = stdout().execute(crossterm::event::DisableMouseCapture);
        default_panic(info);
    }));

    let mut app = App::new(config);

    // Load home screen on start
    if let Err(e) = app.refresh_home() {
        app.set_message(format!("Error loading home: {}", e));
    }

    let events = EventLoop::new(Duration::from_millis(60));

    // Initialize background importer with event sender
    app.init_background_importer(events.sender());

    // Main event loop
    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        match events.next()? {
            Event::Key(key) => {
                handle_key_event(&mut app, key);
            }
            Event::Tick => {
                app.tick();
            }
            Event::Resize(_, _) => {}
            Event::Mouse(_) => {}
            Event::Import(evt) => {
                app.handle_import_event(evt);
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    stdout().execute(crossterm::event::DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}

/// Handle a key event, dispatching based on current state.
fn handle_key_event(app: &mut App, key: KeyEvent) {
    // Handle popups first
    if let Some(ref popup) = app.popup.clone() {
        handle_popup_key(app, key, &popup);
        return;
    }

    // Global keybindings
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return;
        }
        KeyCode::Char('q') => {
            if app.screen == Screen::Reader && app.reader_state.is_some() {
                app.popup = Some(PopupState::QuitConfirm);
            } else {
                app.should_quit = true;
            }
            return;
        }
        KeyCode::Tab => {
            // Tab toggles between Reader <-> non-Reader
            if app.screen == Screen::Reader {
                // Go back to previous screen
                let _ = app.save_reading_progress();
                let target = app.previous_screen.take().unwrap_or(Screen::Home);
                app.screen = target.clone();
                match &target {
                    Screen::Library => {
                        let _ = app.refresh_library();
                    }
                    Screen::Home => {
                        let _ = app.refresh_home();
                    }
                    Screen::ChapterSelect { source_id } => {
                        let sid = *source_id;
                        let _ = app.load_chapter_select(sid);
                    }
                    _ => {}
                }
            } else {
                // From any non-Reader screen: open most recently read text
                let conn = match app.open_db() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let recent = models::list_recent_texts(&conn, 1).unwrap_or_default();
                if let Some(text) = recent.first() {
                    let text_id = text.id;
                    app.previous_screen = Some(app.screen.clone());
                    if let Err(e) = app.load_text(text_id) {
                        app.set_message(format!("Error: {}", e));
                    }
                } else {
                    app.set_message("No texts read yet — import something first");
                }
            }
            return;
        }
        KeyCode::Char('?') => {
            app.popup = Some(PopupState::Help);
            return;
        }
        _ => {}
    }

    match &app.screen.clone() {
        Screen::Home => handle_home_key(app, key),
        Screen::Library => handle_library_key(app, key),
        Screen::ChapterSelect { source_id } => {
            let sid = *source_id;
            handle_chapter_select_key(app, key, sid);
        }
        Screen::Reader => handle_reader_key(app, key),
        Screen::Review => {}
    }
}

fn handle_popup_key(app: &mut App, key: KeyEvent, popup: &PopupState) {
    match popup {
        PopupState::WordDetail { .. } => match key.code {
            KeyCode::Esc | KeyCode::Enter => app.popup = None,
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(PopupState::WordDetail { ref mut scroll, .. }) = app.popup {
                    *scroll = scroll.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(PopupState::WordDetail { ref mut scroll, .. }) = app.popup {
                    *scroll += 1;
                }
            }
            _ => {}
        },
        PopupState::Help => match key.code {
            KeyCode::Esc | KeyCode::Char('?') => app.popup = None,
            _ => {}
        },
        PopupState::NoteEditor { .. } => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Enter => {
                if let Err(e) = app.save_note() {
                    app.set_message(format!("Error saving note: {}", e));
                }
            }
            KeyCode::Backspace => {
                if let Some(PopupState::NoteEditor { ref mut text, .. }) = app.popup {
                    text.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(PopupState::NoteEditor { ref mut text, .. }) = app.popup {
                    text.push(c);
                }
            }
            _ => {}
        },
        PopupState::QuitConfirm => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = app.save_reading_progress();
                app.should_quit = true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.popup = None,
            _ => {}
        },
        PopupState::DeleteConfirm { .. } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(PopupState::DeleteConfirm { text_id, ref title }) = app.popup {
                    let title = title.clone();
                    if let Err(e) = app.delete_text(text_id) {
                        app.set_message(format!("Error deleting: {}", e));
                    } else {
                        app.set_message(format!("Deleted \"{}\"", title));
                    }
                }
                app.popup = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.popup = None,
            _ => {}
        },
        PopupState::DeleteSourceConfirm { .. } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(PopupState::DeleteSourceConfirm {
                    source_id,
                    ref title,
                }) = app.popup
                {
                    let title = title.clone();
                    if let Err(e) = app.delete_source(source_id) {
                        app.set_message(format!("Error deleting: {}", e));
                    } else {
                        app.set_message(format!("Deleted source \"{}\"", title));
                    }
                }
                app.popup = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.popup = None,
            _ => {}
        },
        PopupState::ImportMenu => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Char('c') => {
                app.popup = None;
                if let Some(ref importer) = app.background_importer {
                    importer.import_clipboard(app.db_path.clone());
                } else {
                    match app.import_clipboard() {
                        Ok(title) => app.set_message(format!("Imported: {}", title)),
                        Err(e) => app.set_message(format!("Clipboard import failed: {}", e)),
                    }
                }
            }
            KeyCode::Char('u') => {
                app.popup = Some(PopupState::UrlInput {
                    text: String::new(),
                });
            }
            KeyCode::Char('f') => {
                app.popup = Some(PopupState::FilePathInput {
                    text: String::new(),
                });
            }
            KeyCode::Char('s') => {
                app.popup = Some(PopupState::SyosetuInput {
                    text: String::new(),
                });
            }
            _ => {}
        },
        PopupState::UrlInput { .. } => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Enter => {
                if let Some(PopupState::UrlInput { ref text }) = app.popup {
                    let url = text.clone();
                    app.popup = None;
                    if let Some(ref importer) = app.background_importer {
                        importer.import_url(url, app.db_path.clone());
                    } else {
                        match app.import_url(&url) {
                            Ok(title) => app.set_message(format!("Imported: {}", title)),
                            Err(e) => app.set_message(format!("URL import failed: {}", e)),
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(PopupState::UrlInput { ref mut text }) = app.popup {
                    text.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(PopupState::UrlInput { ref mut text }) = app.popup {
                    text.push(c);
                }
            }
            _ => {}
        },
        PopupState::SearchInput { .. } => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                let _ = app.refresh_library(); // Reset to full list
            }
            KeyCode::Enter => {
                if let Some(PopupState::SearchInput { ref text }) = app.popup {
                    let query = text.clone();
                    app.popup = None;
                    if let Err(e) = app.search_library(&query) {
                        app.set_message(format!("Search error: {}", e));
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(PopupState::SearchInput { ref mut text }) = app.popup {
                    text.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(PopupState::SearchInput { ref mut text }) = app.popup {
                    text.push(c);
                }
            }
            _ => {}
        },
        PopupState::FilePathInput { .. } => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Enter => {
                if let Some(PopupState::FilePathInput { ref text }) = app.popup {
                    let path_str = text.clone();
                    app.popup = None;
                    let path = std::path::PathBuf::from(&path_str);
                    if !path.exists() {
                        app.set_message(format!("File not found: {}", path_str));
                    } else if let Some(ref importer) = app.background_importer {
                        importer.import_file(path, app.db_path.clone());
                    } else {
                        match app.import_file_path(&path_str) {
                            Ok(result) => app.set_message(format!("Imported: {}", result)),
                            Err(e) => app.set_message(format!("File import failed: {}", e)),
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(PopupState::FilePathInput { ref mut text }) = app.popup {
                    text.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(PopupState::FilePathInput { ref mut text }) = app.popup {
                    text.push(c);
                }
            }
            _ => {}
        },
        PopupState::SyosetuInput { .. } => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Enter => {
                if let Some(PopupState::SyosetuInput { ref text }) = app.popup {
                    let ncode = text.clone();
                    app.popup = None;
                    match app.load_syosetu(&ncode) {
                        Ok(()) => app.set_message("Syosetu novel loaded"),
                        Err(e) => app.set_message(format!("Syosetu load failed: {}", e)),
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(PopupState::SyosetuInput { ref mut text }) = app.popup {
                    text.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(PopupState::SyosetuInput { ref mut text }) = app.popup {
                    text.push(c);
                }
            }
            _ => {}
        },
    }
}

/// Get the filtered recent texts for the home screen.
fn home_filtered_texts(home: &app::HomeState) -> Vec<usize> {
    home.recent_texts
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            home.show_finished
                || t.total_sentences == 0
                || t.last_sentence_index < t.total_sentences - 1
        })
        .map(|(i, _)| i)
        .collect()
}

fn handle_home_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut home) = app.home_state {
                home.selected = home.selected.saturating_sub(1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut home) = app.home_state {
                let filtered = home_filtered_texts(home);
                let max = filtered.len().saturating_sub(1);
                home.selected = (home.selected + 1).min(max);
            }
        }
        KeyCode::Enter => {
            let text_id = app.home_state.as_ref().and_then(|h| {
                let filtered = home_filtered_texts(h);
                filtered
                    .get(h.selected)
                    .and_then(|&idx| h.recent_texts.get(idx))
                    .map(|t| t.id)
            });
            if let Some(id) = text_id {
                app.previous_screen = Some(Screen::Home);
                if let Err(e) = app.load_text(id) {
                    app.set_message(format!("Error loading text: {}", e));
                }
            }
        }
        KeyCode::Char('f') => {
            if let Some(ref mut home) = app.home_state {
                home.show_finished = !home.show_finished;
                // Clamp selection to new filtered list length
                let filtered = home_filtered_texts(home);
                home.selected = home.selected.min(filtered.len().saturating_sub(1));
            }
        }
        KeyCode::Char('l') => {
            app.screen = Screen::Library;
            if let Err(e) = app.refresh_library() {
                app.set_message(format!("Error: {}", e));
            }
        }
        KeyCode::Char('r') => {
            app.screen = Screen::Review;
        }
        KeyCode::Char('i') => {
            app.popup = Some(PopupState::ImportMenu);
        }
        _ => {}
    }
}

fn handle_library_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut lib) = app.library_state {
                lib.selected = lib.selected.saturating_sub(1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut lib) = app.library_state {
                let max = lib.items.len().saturating_sub(1);
                lib.selected = (lib.selected + 1).min(max);
            }
        }
        KeyCode::Enter => {
            if let Some(ref lib) = app.library_state {
                if let Some(item) = lib.items.get(lib.selected) {
                    match item {
                        app::LibraryItem::Text(t) => {
                            let id = t.id;
                            app.previous_screen = Some(Screen::Library);
                            if let Err(e) = app.load_text(id) {
                                app.set_message(format!("Error loading text: {}", e));
                            }
                        }
                        app::LibraryItem::Source(ws) => {
                            let sid = ws.id;
                            if let Err(e) = app.load_chapter_select(sid) {
                                app.set_message(format!("Error loading chapters: {}", e));
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('d') => {
            // Delete selected item (with confirmation)
            if let Some(ref lib) = app.library_state {
                match lib.items.get(lib.selected) {
                    Some(app::LibraryItem::Text(text)) => {
                        app.popup = Some(PopupState::DeleteConfirm {
                            text_id: text.id,
                            title: text.title.clone(),
                        });
                    }
                    Some(app::LibraryItem::Source(ws)) => {
                        app.popup = Some(PopupState::DeleteSourceConfirm {
                            source_id: ws.id,
                            title: ws.title.clone(),
                        });
                    }
                    None => {}
                }
            }
        }
        KeyCode::Char('i') => {
            app.popup = Some(PopupState::ImportMenu);
        }
        KeyCode::Char('/') => {
            app.popup = Some(PopupState::SearchInput {
                text: String::new(),
            });
        }
        KeyCode::Char('s') => {
            if let Err(e) = app.cycle_library_sort() {
                app.set_message(format!("Sort error: {}", e));
            }
        }
        KeyCode::Char('f') => {
            if let Err(e) = app.cycle_library_filter() {
                app.set_message(format!("Filter error: {}", e));
            }
        }
        KeyCode::Esc => {
            app.screen = Screen::Home;
            let _ = app.refresh_home();
        }
        _ => {}
    }
}

/// Adjust page_start so that `state.selected` is within the visible range.
fn ensure_selected_visible(state: &mut app::ChapterSelectState) {
    // If selected is before the current page, scroll up
    if state.selected < state.page_start {
        state.page_start = state.selected;
        return;
    }
    // If selected is beyond the visible chapters, scroll forward
    let vis = state.visible_chapters();
    let page_end = state.page_start + vis.len();
    if state.selected >= page_end {
        // Scroll so selected is the last visible item — walk backwards from selected
        // to find a good page_start
        let target = state.selected;
        // Start from the selected item and walk backwards, counting rows
        let mut rows_left = state.page_size;
        let mut candidate_start = target;

        // Walk backwards from target to find how many chapters fit
        for idx in (0..=target).rev() {
            let ch = &state.chapters[idx];
            let mut rows_needed = 1; // the chapter itself

            // Check if this chapter is the first in its group (looking at the chapter before it)
            if !ch.chapter_group.is_empty() {
                let is_first_in_group = if idx == 0 {
                    true
                } else {
                    state.chapters[idx - 1].chapter_group != ch.chapter_group
                };
                if is_first_in_group {
                    rows_needed += 1; // group header
                }
            }

            if rows_needed > rows_left {
                break;
            }
            rows_left -= rows_needed;
            candidate_start = idx;
        }
        state.page_start = candidate_start;
    }
}

fn handle_chapter_select_key(app: &mut App, key: KeyEvent, source_id: i64) {
    // Recalculate page_size on every key event to handle terminal resize
    if let Some(ref mut state) = app.chapter_select_state {
        let new_page_size = app::chapter_page_size_for_terminal();
        if state.page_size != new_page_size {
            state.page_size = new_page_size;
            // Ensure selected is still visible after resize
            ensure_selected_visible(state);
        }
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut state) = app.chapter_select_state {
                state.selected = state.selected.saturating_sub(1);
                ensure_selected_visible(state);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut state) = app.chapter_select_state {
                let max = state.chapters.len().saturating_sub(1);
                state.selected = (state.selected + 1).min(max);
                ensure_selected_visible(state);
            }
        }
        KeyCode::Char('n') | KeyCode::PageDown => {
            if let Some(ref mut state) = app.chapter_select_state {
                let next_start = state.next_page_start();
                if next_start < state.chapters.len() {
                    state.page_start = next_start;
                    state.selected = next_start;
                }
            }
        }
        KeyCode::Char('p') | KeyCode::PageUp => {
            if let Some(ref mut state) = app.chapter_select_state {
                if state.page_start > 0 {
                    // Go back by approximately page_size chapters
                    state.page_start = state.page_start.saturating_sub(state.page_size);
                    state.selected = state.page_start;
                }
            }
        }
        KeyCode::Char('x') => {
            // Toggle skip on selected chapter
            let chapter_info = app
                .chapter_select_state
                .as_ref()
                .and_then(|s| s.chapters.get(s.selected))
                .map(|ch| (ch.id, ch.is_skipped));
            if let Some((cid, _was_skipped)) = chapter_info {
                let conn = match app.open_db() {
                    Ok(c) => c,
                    Err(e) => {
                        app.set_message(format!("DB error: {}", e));
                        return;
                    }
                };
                match models::toggle_chapter_skip(&conn, cid) {
                    Ok(now_skipped) => {
                        if now_skipped {
                            // Cancel any in-progress preprocessing
                            if let Some(ref importer) = app.background_importer {
                                importer.cancel_chapter(cid);
                            }
                            app.preprocessing_chapters.remove(&cid);
                        }
                        // Update in-memory state directly (avoid full DB reload)
                        if let Some(ref mut state) = app.chapter_select_state {
                            if let Some(ch) = state.chapters.iter_mut().find(|c| c.id == cid) {
                                ch.is_skipped = now_skipped;
                            }
                            if now_skipped {
                                state.total_skipped += 1;
                                // Remove from read states so it doesn't count toward preprocess budget
                                state.chapter_read_states.remove(&cid);
                            } else {
                                state.total_skipped = state.total_skipped.saturating_sub(1);
                                // Re-add as NotImported (or Unread if it has a text_id)
                                let has_text = state
                                    .chapters
                                    .iter()
                                    .find(|c| c.id == cid)
                                    .and_then(|c| c.text_id)
                                    .is_some();
                                state.chapter_read_states.insert(
                                    cid,
                                    if has_text {
                                        app::ChapterReadState::Unread
                                    } else {
                                        app::ChapterReadState::NotImported
                                    },
                                );
                            }
                        }
                        app.set_message(if now_skipped {
                            "Chapter skipped"
                        } else {
                            "Chapter unskipped"
                        });
                        // Top up preprocessing queue (a skipped chapter freed a slot)
                        app.start_preprocessing();
                    }
                    Err(e) => app.set_message(format!("Error: {}", e)),
                }
            }
        }
        KeyCode::Char('P') => {
            // Manual preprocessing trigger
            app.start_preprocessing();
            app.set_message("Preprocessing queued");
        }
        KeyCode::Enter => {
            // Open chapter — import first if needed
            let chapter_info = app
                .chapter_select_state
                .as_ref()
                .and_then(|s| s.chapters.get(s.selected).cloned());
            let source_type = app
                .chapter_select_state
                .as_ref()
                .map(|s| s.source.source_type.clone());

            if let Some(ch) = chapter_info {
                if ch.is_skipped {
                    app.set_message("Chapter is skipped — press [x] to unskip first");
                } else if let Some(text_id) = ch.text_id {
                    // Already imported — open it
                    app.previous_screen = Some(Screen::ChapterSelect { source_id });
                    if let Err(e) = app.load_text(text_id) {
                        app.set_message(format!("Error loading: {}", e));
                    }
                } else if source_type.as_deref() == Some("syosetu") {
                    // Not yet imported — queue for background import and wait
                    let is_already_processing = app.preprocessing_chapters.contains(&ch.id);
                    app.pending_open_chapter = Some(ch.id);

                    if !is_already_processing {
                        // Force-queue this specific chapter
                        if let Some(ref mut importer) = app.background_importer {
                            let state = app.chapter_select_state.as_ref().unwrap();
                            importer.queue_single(
                                state.source.id,
                                &state.source.source_type,
                                &state.source.external_id,
                                ch.id,
                                ch.chapter_number,
                                &app.db_path,
                            );
                        }
                        app.preprocessing_chapters.insert(ch.id);
                    }
                    app.set_message(format!(
                        "Importing chapter {}... will open when ready",
                        ch.chapter_number
                    ));
                } else {
                    app.set_message("Chapter not yet imported");
                }
            }
        }
        KeyCode::Esc => {
            // Cancel pending chapter open if any
            app.pending_open_chapter = None;
            app.screen = Screen::Library;
            let _ = app.refresh_library();
        }
        _ => {}
    }
}

fn handle_reader_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Sentence navigation
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut state) = app.reader_state {
                if state.sentence_index > 0 {
                    state.sentence_index -= 1;
                    state.word_index = None;
                    state.sidebar_scroll = 0;
                }
            }
            let _ = app.save_reading_progress();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let departing = {
                let result = if let Some(ref mut state) = app.reader_state {
                    if state.sentence_index + 1 < state.sentences.len() {
                        let dep = state.sentence_index;
                        state.sentence_index += 1;
                        state.word_index = None;
                        state.sidebar_scroll = 0;
                        Some((dep, false)) // (departing_index, is_at_end)
                    } else {
                        Some((state.sentence_index, true))
                    }
                } else {
                    None
                };
                result
            };
            if let Some((dep, is_at_end)) = departing {
                if let Err(e) = app.autopromote_sentence(dep) {
                    app.set_message(format!("Autopromote error: {}", e));
                }
                if is_at_end {
                    let _ = app.save_reading_progress();
                    match app.advance_to_next_chapter() {
                        Ok(true) => {}
                        _ => {
                            app.set_message("End of text. Press Esc to return.");
                        }
                    }
                }
            }
            if !matches!(departing, Some((_, true))) {
                let _ = app.save_reading_progress();
            }
        }

        // Word navigation
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut state) = app.reader_state {
                if state.sentences.is_empty() {
                    return;
                }
                let sentence = &state.sentences[state.sentence_index];
                match state.word_index {
                    None => {
                        state.word_index = sentence.tokens.iter().rposition(|t| !t.is_trivial);
                    }
                    Some(i) => {
                        let prev = if i > 0 {
                            sentence.tokens[..i].iter().rposition(|t| !t.is_trivial)
                        } else {
                            None
                        };
                        match prev {
                            Some(p) => state.word_index = Some(p),
                            None => {
                                // First word — go to previous sentence, select last word
                                if state.sentence_index > 0 {
                                    state.sentence_index -= 1;
                                    state.sidebar_scroll = 0;
                                    let prev_sentence = &state.sentences[state.sentence_index];
                                    state.word_index =
                                        prev_sentence.tokens.iter().rposition(|t| !t.is_trivial);
                                }
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            let departing: Option<(usize, bool)> = {
                let mut advanced_from: Option<(usize, bool)> = None;
                if let Some(ref mut state) = app.reader_state {
                    if !state.sentences.is_empty() {
                        let sentence = &state.sentences[state.sentence_index];
                        match state.word_index {
                            None => {
                                state.word_index =
                                    sentence.tokens.iter().position(|t| !t.is_trivial);
                            }
                            Some(i) => {
                                let next = sentence.tokens[i + 1..]
                                    .iter()
                                    .position(|t| !t.is_trivial)
                                    .map(|p| p + i + 1);
                                match next {
                                    Some(n) => state.word_index = Some(n),
                                    None => {
                                        // Last word — advance to next sentence, select first word
                                        if state.sentence_index + 1 < state.sentences.len() {
                                            let dep = state.sentence_index;
                                            state.sentence_index += 1;
                                            state.sidebar_scroll = 0;
                                            let next_sentence =
                                                &state.sentences[state.sentence_index];
                                            state.word_index = next_sentence
                                                .tokens
                                                .iter()
                                                .position(|t| !t.is_trivial);
                                            advanced_from = Some((dep, false));
                                        } else {
                                            // Last word of last sentence
                                            advanced_from = Some((state.sentence_index, true));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                advanced_from
            };
            if let Some((dep, is_at_end)) = departing {
                if let Err(e) = app.autopromote_sentence(dep) {
                    app.set_message(format!("Autopromote error: {}", e));
                }
                if is_at_end {
                    let _ = app.save_reading_progress();
                    // Try to advance to next chapter automatically
                    match app.advance_to_next_chapter() {
                        Ok(true) => {} // navigated to next chapter, message already set
                        _ => {
                            app.set_message("End of text. Press Esc to return.");
                        }
                    }
                }
            }
        }

        // Vocabulary status
        KeyCode::Char('1') => {
            if let Err(e) = app.set_word_status(VocabularyStatus::Learning1) {
                app.set_message(format!("Error: {}", e));
            }
        }
        KeyCode::Char('2') => {
            if let Err(e) = app.set_word_status(VocabularyStatus::Learning2) {
                app.set_message(format!("Error: {}", e));
            }
        }
        KeyCode::Char('3') => {
            if let Err(e) = app.set_word_status(VocabularyStatus::Learning3) {
                app.set_message(format!("Error: {}", e));
            }
        }
        KeyCode::Char('4') => {
            if let Err(e) = app.set_word_status(VocabularyStatus::Learning4) {
                app.set_message(format!("Error: {}", e));
            }
        }
        KeyCode::Char('5') => {
            if let Err(e) = app.set_word_status(VocabularyStatus::Known) {
                app.set_message(format!("Error: {}", e));
            }
        }
        KeyCode::Char('i') => {
            if let Err(e) = app.set_word_status(VocabularyStatus::Ignored) {
                app.set_message(format!("Error: {}", e));
            }
        }

        // Word detail
        KeyCode::Enter => {
            if let Err(e) = app.open_word_detail() {
                app.set_message(format!("Error: {}", e));
            }
        }

        // Note editor
        KeyCode::Char('n') => {
            if let Err(e) = app.open_note_editor() {
                app.set_message(format!("Error: {}", e));
            }
        }

        // Toggle showing Known/Ignored words in sidebar
        KeyCode::Char('w') => {
            if let Some(ref mut state) = app.reader_state {
                state.show_known_in_sidebar = !state.show_known_in_sidebar;
                let status = if state.show_known_in_sidebar {
                    "showing all words"
                } else {
                    "hiding Known/Ignored"
                };
                app.set_message(format!("Sidebar: {}", status));
            }
        }

        // Toggle sidebar readings for all words
        KeyCode::Char('r') => {
            if let Some(ref mut state) = app.reader_state {
                state.show_all_readings = !state.show_all_readings;
                let status = if state.show_all_readings { "ON" } else { "OFF" };
                app.set_message(format!("Show all readings {}", status));
            }
        }

        // Toggle autopromotion
        KeyCode::Char('a') => {
            if let Some(ref mut state) = app.reader_state {
                state.autopromote_enabled = !state.autopromote_enabled;
                let status = if state.autopromote_enabled {
                    "ON"
                } else {
                    "OFF"
                };
                app.set_message(format!("Autopromotion {}", status));
            }
        }

        // Undo last autopromotion batch
        KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Err(e) = app.undo_last_autopromote() {
                app.set_message(format!("Undo error: {}", e));
            }
        }

        // Deselect word, or go back to previous screen if no word selected
        KeyCode::Esc => {
            let has_word = app
                .reader_state
                .as_ref()
                .map(|s| s.word_index.is_some())
                .unwrap_or(false);
            if has_word {
                if let Some(ref mut state) = app.reader_state {
                    state.word_index = None;
                }
            } else {
                if let Err(e) = app.back_from_reader() {
                    app.set_message(format!("Error: {}", e));
                }
            }
        }

        _ => {}
    }
}
