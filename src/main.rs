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
use db::models::VocabularyStatus;
use ui::events::{Event, EventLoop};

#[derive(Parser)]
#[command(name = "kotoba", version, about = "Terminal-based Japanese language learning app")]
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
    /// Import a Syosetsu (小説家になろう) novel chapter
    Syosetsu {
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
        Commands::Import { file, clipboard, url } => {
            if clipboard {
                let text_id = import::clipboard::import_clipboard(&conn)?;
                println!("Imported clipboard text with id: {text_id}");
            } else if let Some(url) = url {
                let text_id = import::web::import_url(&url, &conn)?;
                println!("Imported web content with id: {text_id}");
            } else if let Some(file) = file {
                // Auto-detect format by extension
                let ext = file.extension()
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
                    t.surface, t.base_form, t.reading, t.pos, t.conjugation_form, t.conjugation_type
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
        Commands::Syosetsu { ncode, chapter } => {
            let ncode = import::syosetsu::parse_ncode(&ncode)?;

            if let Some(ch) = chapter {
                let text_id = import::syosetsu::import_chapter(&ncode, ch, &conn)?;
                println!("Imported chapter {} with id: {text_id}", ch);
            } else {
                // Show novel info and chapter list
                println!("Fetching novel info for {}...", ncode);
                let novel = import::syosetsu::fetch_novel_info(&ncode)?;
                println!("Title: {}", novel.title);
                println!("Author: {}", novel.author);
                println!("Chapters: {}", novel.total_chapters);
                println!();
                for ch in &novel.chapters {
                    println!("  {:>4}. {}", ch.number, ch.title);
                }
                println!();
                println!("Import a chapter with: kotoba syosetsu {} --chapter <N>", ncode);
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

    // Load library on start
    if let Err(e) = app.refresh_library() {
        app.set_message(format!("Error loading library: {}", e));
    }

    let events = EventLoop::new(Duration::from_millis(60));

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
            app.screen = app.screen.next();
            if app.screen == Screen::Library {
                let _ = app.refresh_library();
            }
            return;
        }
        KeyCode::Char('?') => {
            app.popup = Some(PopupState::Help);
            return;
        }
        _ => {}
    }

    match app.screen {
        Screen::Library => handle_library_key(app, key),
        Screen::Reader => handle_reader_key(app, key),
        Screen::Syosetsu => handle_syosetsu_key(app, key),
        Screen::Review => {}
        Screen::Stats => {}
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
            KeyCode::Char('y') | KeyCode::Char('Y') => app.should_quit = true,
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
        PopupState::ImportMenu => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Char('c') => {
                app.popup = None;
                match app.import_clipboard() {
                    Ok(title) => app.set_message(format!("Imported: {}", title)),
                    Err(e) => app.set_message(format!("Clipboard import failed: {}", e)),
                }
            }
            KeyCode::Char('u') => {
                app.popup = Some(PopupState::UrlInput { text: String::new() });
            }
            _ => {}
        },
        PopupState::UrlInput { .. } => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Enter => {
                if let Some(PopupState::UrlInput { ref text }) = app.popup {
                    let url = text.clone();
                    app.popup = None;
                    match app.import_url(&url) {
                        Ok(title) => app.set_message(format!("Imported: {}", title)),
                        Err(e) => app.set_message(format!("URL import failed: {}", e)),
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
                let max = lib.texts.len().saturating_sub(1);
                lib.selected = (lib.selected + 1).min(max);
            }
        }
        KeyCode::Enter => {
            let text_id = app
                .library_state
                .as_ref()
                .and_then(|lib| lib.texts.get(lib.selected))
                .map(|t| t.id);
            if let Some(id) = text_id {
                if let Err(e) = app.load_text(id) {
                    app.set_message(format!("Error loading text: {}", e));
                }
            }
        }
        KeyCode::Char('d') => {
            // Delete selected text (with confirmation)
            if let Some(ref lib) = app.library_state {
                if let Some(text) = lib.texts.get(lib.selected) {
                    app.popup = Some(PopupState::DeleteConfirm {
                        text_id: text.id,
                        title: text.title.clone(),
                    });
                }
            }
        }
        KeyCode::Char('i') => {
            // Import sub-menu
            app.popup = Some(PopupState::ImportMenu);
        }
        KeyCode::Char('/') => {
            // Search
            app.popup = Some(PopupState::SearchInput { text: String::new() });
        }
        KeyCode::Char('s') => {
            // Cycle sort mode
            if let Err(e) = app.cycle_library_sort() {
                app.set_message(format!("Sort error: {}", e));
            }
        }
        KeyCode::Char('f') => {
            // Cycle source type filter
            if let Err(e) = app.cycle_library_filter() {
                app.set_message(format!("Filter error: {}", e));
            }
        }
        _ => {}
    }
}

fn handle_syosetsu_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut state) = app.syosetsu_state {
                state.selected_chapter = state.selected_chapter.saturating_sub(1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut state) = app.syosetsu_state {
                let max = state.novel.chapters.len().saturating_sub(1);
                state.selected_chapter = (state.selected_chapter + 1).min(max);
            }
        }
        KeyCode::Enter => {
            // Import selected chapter and open in reader
            let info = app.syosetsu_state.as_ref().map(|s| {
                let ch = &s.novel.chapters[s.selected_chapter];
                (s.novel.ncode.clone(), ch.number, ch.text_id)
            });
            if let Some((ncode, chapter_num, existing_text_id)) = info {
                if let Some(text_id) = existing_text_id {
                    // Already imported — just open it
                    if let Err(e) = app.load_text(text_id) {
                        app.set_message(format!("Error loading: {}", e));
                    }
                } else {
                    // Import the chapter
                    app.set_message(format!("Importing chapter {}...", chapter_num));
                    let conn = match app.open_db() {
                        Ok(c) => c,
                        Err(e) => {
                            app.set_message(format!("DB error: {}", e));
                            return;
                        }
                    };
                    match crate::import::syosetsu::import_chapter_quiet(&ncode, chapter_num, &conn) {
                        Ok((text_id, title)) => {
                            // Update the chapter's text_id
                            if let Some(ref mut state) = app.syosetsu_state {
                                state.novel.chapters[state.selected_chapter].text_id = Some(text_id);
                            }
                            app.set_message(format!("Imported: {}", title));
                            // Open in reader
                            if let Err(e) = app.load_text(text_id) {
                                app.set_message(format!("Error loading: {}", e));
                            }
                        }
                        Err(e) => {
                            app.set_message(format!("Import failed: {}", e));
                        }
                    }
                }
            }
        }
        KeyCode::Esc => {
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
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut state) = app.reader_state {
                if state.sentence_index + 1 < state.sentences.len() {
                    state.sentence_index += 1;
                    state.word_index = None;
                    state.sidebar_scroll = 0;
                }
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
                                    state.word_index = prev_sentence.tokens.iter().rposition(|t| !t.is_trivial);
                                }
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(ref mut state) = app.reader_state {
                if state.sentences.is_empty() {
                    return;
                }
                let sentence = &state.sentences[state.sentence_index];
                match state.word_index {
                    None => {
                        state.word_index = sentence.tokens.iter().position(|t| !t.is_trivial);
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
                                    state.sentence_index += 1;
                                    state.sidebar_scroll = 0;
                                    let next_sentence = &state.sentences[state.sentence_index];
                                    state.word_index = next_sentence.tokens.iter().position(|t| !t.is_trivial);
                                }
                            }
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

        // Deselect word
        KeyCode::Esc => {
            if let Some(ref mut state) = app.reader_state {
                state.word_index = None;
            }
        }

        _ => {}
    }
}
