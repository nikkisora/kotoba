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
use ratatui::layout::{Constraint, Layout};
use ratatui::Terminal;
use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use app::{App, PopupState, ReviewPhase, Screen};
use core::srs::Rating;
use db::models::{self, VocabularyStatus};
use ui::events::{Event, EventLoop};

#[derive(Parser)]
#[command(
    name = "kotoba",
    version,
    about = "Terminal-based Japanese language learning app",
    long_about = "kotoba is a terminal-based Japanese language learning app.\n\n\
                  It combines an immersive reader with SRS flashcard review,\n\
                  vocabulary tracking, and dictionary lookup — all in the terminal.\n\n\
                  Get started:\n  \
                   kotoba                 Launch the interactive TUI\n  \
                  kotoba setup-dict      Download the JMdict dictionary\n  \
                  kotoba import <file>   Import a text (.txt, .epub, .srt, .ass)\n  \
                  kotoba config          Show current configuration"
)]
struct Cli {
    /// Path to config file (default: ~/.config/kotoba/kotoba.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Import content for reading
    #[command(long_about = "Import content for reading.\n\n\
                      Auto-detects format by file extension:\n  \
                      .txt          Plain text\n  \
                      .epub         EPUB ebook (imports all chapters)\n  \
                      .srt          SubRip subtitles\n  \
                      .ass / .ssa   Advanced SubStation Alpha subtitles\n\n\
                      Content is tokenized and stored in the database for reading in the TUI.")]
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
    /// Tokenize a Japanese text string and show morphological analysis
    Tokenize {
        /// The Japanese text to tokenize
        text: String,
    },
    /// Look up a word in the JMdict dictionary
    #[command(long_about = "Look up a word in the JMdict dictionary.\n\n\
                      Searches by kanji and reading. Requires JMdict to be imported first\n\
                      (run 'kotoba setup-dict' or 'kotoba import-dict <path>').")]
    Dict {
        /// The word to look up (kanji or kana)
        word: String,
    },
    /// Import a JMdict XML file into the database
    #[command(long_about = "Import a JMdict XML file into the database.\n\n\
                      Use this if you already have JMdict_e.xml downloaded.\n\
                      For automatic download, use 'kotoba setup-dict' instead.")]
    ImportDict {
        /// Path to JMdict_e.xml file
        path: PathBuf,
    },
    /// Download and set up the JMdict dictionary automatically
    #[command(
        long_about = "Download and set up the JMdict dictionary automatically.\n\n\
                      Downloads JMdict_e.gz from the EDRDG FTP server, decompresses it,\n\
                      and imports all entries into the database. This is required for\n\
                      dictionary lookups to work in the reader.\n\n\
                      The dictionary file is stored in the data directory\n\
                      (default: ~/.local/share/kotoba/)."
    )]
    SetupDict,
    /// Import a Syosetu novel chapter
    #[command(long_about = "Import a Syosetu (小説家になろう) novel chapter.\n\n\
                      Fetches a chapter from syosetu.com, tokenizes it, and stores it\n\
                      in the database. If no chapter number is given, shows the novel\n\
                      info and chapter list.\n\n\
                      Examples:\n  \
                      kotoba syosetu n1234ab               List chapters\n  \
                      kotoba syosetu n1234ab --chapter 1    Import chapter 1")]
    Syosetu {
        /// Novel URL or ncode (e.g. n1234ab)
        ncode: String,

        /// Chapter number to import (omit to see chapter list)
        #[arg(short, long)]
        chapter: Option<usize>,
    },
    /// Manage the LLM response cache
    #[command(
        subcommand,
        long_about = "Manage the LLM response cache.\n\n\
                      LLM responses are cached in the database to avoid redundant API calls.\n\
                      Use these commands to view cache statistics or clear the cache."
    )]
    Cache(CacheCommands),
    /// Launch the interactive TUI reader
    #[command(long_about = "Launch the interactive TUI reader.\n\n\
                      Opens the full-screen terminal interface with reader, sidebar,\n\
                      SRS review, vocabulary tracking, and more.\n\n\
                      Keybindings:\n  \
                      ?              Show help overlay\n  \
                      ↑↓             Navigate sentences\n  \
                      ←→             Navigate words\n  \
                      1-5            Set vocabulary status\n  \
                      Tab            Toggle reader\n  \
                      q              Quit")]
    Run,
    /// Show config file location and current settings
    #[command(long_about = "Show config file location and current settings.\n\n\
                      Displays the resolved paths for config and data directories,\n\
                      and prints all current configuration values.")]
    Config,
}

#[derive(Subcommand)]
enum CacheCommands {
    /// Show cache statistics (number of entries, total tokens used)
    Stats,
    /// Clear all cached LLM responses
    Clear,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cfg = config::AppConfig::load(cli.config.as_deref())?;
    let conn = db::connection::open_or_create(&cfg.db_path())?;
    db::schema::run_migrations(&conn)?;

    match cli.command.unwrap_or(Commands::Run) {
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
        Commands::Cache(subcmd) => match subcmd {
            CacheCommands::Stats => {
                let (count, total_tokens) = models::get_llm_cache_stats(&conn)?;
                println!("LLM Cache Statistics:");
                println!("  Cached responses: {}", count);
                println!("  Total tokens used: {}", total_tokens);
            }
            CacheCommands::Clear => {
                let count = models::clear_llm_cache(&conn)?;
                println!("Cleared {} cached LLM responses", count);
            }
        },
        Commands::Run => {
            drop(conn); // Close CLI connection; TUI opens its own
            run_tui(cfg)?;
        }
        Commands::Config => {
            // Print config file location
            let config_path = dirs::config_dir()
                .map(|d| d.join("kotoba").join("kotoba.toml"))
                .unwrap_or_else(|| PathBuf::from("kotoba.toml"));
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("kotoba");

            println!("Config file:  {}", config_path.display());
            if config_path.exists() {
                println!("  (exists)");
            } else {
                println!("  (not found — using defaults)");
            }
            println!();
            println!("Data dir:     {}", data_dir.display());
            println!("Database:     {}", cfg.db_path().display());
            println!();
            println!("Current settings:");
            println!("  [general]");
            println!("    theme             = {}", cfg.general.theme);
            println!(
                "    db_path           = {}",
                cfg.general
                    .db_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(default)".into())
            );
            println!();
            println!("  [reader]");
            println!("    sidebar_width     = {}", cfg.reader.sidebar_width);
            println!("    furigana          = {}", cfg.reader.furigana);
            println!("    sentence_gaps     = {}", cfg.reader.sentence_gaps);
            println!("    preprocess_ahead  = {}", cfg.reader.preprocess_ahead);
            println!();
            println!("  [srs]");
            println!("    new_cards_per_day  = {}", cfg.srs.new_cards_per_day);
            println!(
                "    max_reviews        = {}",
                cfg.srs.max_reviews_per_session
            );
            println!("    review_order       = {}", cfg.srs.review_order);
            println!("    require_typed      = {}", cfg.srs.require_typed_input);
            println!("    sentence_cloze     = {}", cfg.srs.enable_sentence_cloze);
            println!("    cloze_ratio        = {}%", cfg.srs.sentence_cloze_ratio);
            println!();
            println!("  [llm]");
            println!("    endpoint          = {}", cfg.llm.endpoint);
            println!(
                "    api_key           = {}",
                if cfg.llm.api_key.is_empty() {
                    "(not set)"
                } else {
                    "***"
                }
            );
            println!("    model             = {}", cfg.llm.model);
            println!("    max_tokens        = {}", cfg.llm.max_tokens);
            println!();
            println!("Themes dir:   {}", ui::theme::Theme::themes_dir().display());
            println!(
                "Available themes: {}",
                ui::theme::Theme::available_themes().join(", ")
            );
            println!("  (place custom .toml files in the themes dir to add more)");
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

    // Store event sender for async LLM operations
    app.init_event_tx(events.sender());

    // Main event loop
    loop {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        match events.next()? {
            Event::Key(key) => {
                handle_key_event(&mut app, key);
            }
            Event::Tick => {
                app.tick();
            }
            Event::Resize(_, _) => {}
            Event::Mouse(mouse) => {
                handle_mouse_event(&mut app, mouse, &terminal);
            }
            Event::Import(evt) => {
                app.handle_import_event(evt);
            }
            Event::Llm(evt) => {
                app.handle_llm_event(evt);
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
        KeyCode::Tab | KeyCode::BackTab => {
            // On Settings/CardBrowser/Home screens, Tab switches panels instead
            // of triggering the global reader toggle.
            if app.screen == Screen::Settings
                || app.screen == Screen::CardBrowser
                || app.screen == Screen::Home
                || app.screen == Screen::Stats
            {
                match &app.screen.clone() {
                    Screen::Settings => handle_settings_key(app, key),
                    Screen::CardBrowser => handle_card_browser_key(app, key),
                    Screen::Home => handle_home_key(app, key),
                    Screen::Stats => handle_stats_key(app, key),
                    _ => {}
                }
                return;
            }
            // BackTab shouldn't trigger reader toggle
            if key.code == KeyCode::BackTab {
                return;
            }
            // Tab toggles between Reader <-> non-Reader
            if app.screen == Screen::Reader {
                // Go back to previous screen
                let _ = app.back_from_reader();
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
            app.popup = Some(PopupState::Help { scroll: 0 });
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
        Screen::Review => handle_review_key(app, key),
        Screen::CardBrowser => handle_card_browser_key(app, key),
        Screen::Settings => handle_settings_key(app, key),
        Screen::Stats => handle_stats_key(app, key),
    }
}

/// Handle mouse events — currently only supports clicking on words in the Reader.
fn handle_mouse_event(
    app: &mut App,
    mouse: crossterm::event::MouseEvent,
    terminal: &Terminal<CrosstermBackend<std::io::Stdout>>,
) {
    use crossterm::event::{MouseButton, MouseEventKind};

    if app.screen != Screen::Reader {
        return;
    }
    // Don't handle clicks when a popup is open
    if app.popup.is_some() {
        return;
    }

    // Scroll wheel: navigate sentences
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if let Some(ref mut state) = app.reader_state {
                if state.sentence_index > 0 {
                    state.sentence_index -= 1;
                    state.word_index = None;
                    state.expression_mark = None;
                    let _ = app.save_reading_progress();
                }
            }
            return;
        }
        MouseEventKind::ScrollDown => {
            if let Some(ref mut state) = app.reader_state {
                if state.sentence_index + 1 < state.sentences.len() {
                    state.sentence_index += 1;
                    state.word_index = None;
                    state.expression_mark = None;
                    let _ = app.save_reading_progress();
                }
            }
            return;
        }
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return,
    }

    let state = match app.reader_state.as_ref() {
        Some(s) => s,
        None => return,
    };
    if state.sentences.is_empty() {
        return;
    }

    let term_size = match terminal.size() {
        Ok(s) => s,
        Err(_) => return,
    };

    // Recompute the reader layout areas (must match reader.rs render logic exactly)
    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(term_size);

    let sidebar_pct = app.config.reader.sidebar_width;
    let content = Layout::horizontal([
        Constraint::Percentage(100 - sidebar_pct),
        Constraint::Percentage(sidebar_pct),
    ])
    .split(outer[1]);

    let inner = content[0]; // Main text area (no borders)

    // Check if click is inside the main text area
    if mouse.column < inner.x
        || mouse.column >= inner.x + inner.width
        || mouse.row < inner.y
        || mouse.row >= inner.y + inner.height
    {
        return;
    }

    // Recompute scroll offset and sentence positions (mirrors render_main_text logic)
    let show_furigana = app.config.reader.furigana;
    let sentence_gaps = app.config.reader.sentence_gaps;
    let mut sentence_heights: Vec<u16> = Vec::new();
    let mut gaps: Vec<u16> = Vec::new();
    let mut prev_para: Option<usize> = None;

    for (sent_idx, sentence) in state.sentences.iter().enumerate() {
        let gap: u16 = if let Some(pp) = prev_para {
            if sentence.paragraph_idx != pp {
                1
            } else if sentence_gaps {
                1
            } else {
                0
            }
        } else {
            0
        };
        gaps.push(gap);

        let is_current = sent_idx == state.sentence_index;
        let h = ui::components::furigana::sentence_height(
            &sentence.tokens,
            inner.width,
            show_furigana,
            is_current,
            false,
        );
        sentence_heights.push(h);
        prev_para = Some(sentence.paragraph_idx);
    }

    let total_height: u16 = sentence_heights.iter().sum::<u16>() + gaps.iter().sum::<u16>();
    let current_y: u16 = sentence_heights[..state.sentence_index].iter().sum::<u16>()
        + gaps[..state.sentence_index].iter().sum::<u16>();
    let current_h = sentence_heights[state.sentence_index];

    let target_y = if total_height <= inner.height {
        0
    } else {
        let center = inner.height / 2;
        let ideal = current_y.saturating_sub(center.saturating_sub(current_h / 2));
        ideal.min(total_height.saturating_sub(inner.height))
    };

    // Walk sentences to find which one the click falls into
    let mut y_pos: i32 = -(target_y as i32);

    for (sent_idx, sentence) in state.sentences.iter().enumerate() {
        y_pos += gaps[sent_idx] as i32;
        let h = sentence_heights[sent_idx];

        if y_pos + h as i32 <= 0 {
            y_pos += h as i32;
            continue;
        }
        if y_pos >= inner.height as i32 {
            break;
        }

        let render_y = y_pos.max(0) as u16;
        let available_height = inner.height.saturating_sub(render_y);
        if available_height == 0 {
            y_pos += h as i32;
            continue;
        }

        let render_area = ratatui::layout::Rect {
            x: inner.x,
            y: inner.y + render_y,
            width: inner.width,
            height: available_height,
        };

        let is_current = sent_idx == state.sentence_index;

        if let Some(token_idx) = ui::components::furigana::hit_test_sentence(
            &sentence.tokens,
            render_area,
            mouse.column,
            mouse.row,
            show_furigana,
            is_current,
        ) {
            // Resolve to navigable token: if clicked token is in a group,
            // select the group head. If trivial, still select it (user intent).
            let tok = &sentence.tokens[token_idx];
            let nav_idx = if let Some(gid) = tok.group_id {
                // Find the group head
                sentence
                    .tokens
                    .iter()
                    .position(|t| t.group_id == Some(gid) && t.is_group_head)
                    .unwrap_or(token_idx)
            } else {
                token_idx
            };

            // Update state
            let state = app.reader_state.as_mut().unwrap();
            if sent_idx != state.sentence_index {
                state.sentence_index = sent_idx;
                state.word_index = Some(nav_idx);
                state.expression_mark = None;
                let _ = app.save_reading_progress();
            } else {
                state.word_index = Some(nav_idx);
            }
            return;
        }

        y_pos += h as i32;
    }
}

fn handle_card_browser_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut state) = app.card_browser_state {
                if state.selected > 0 {
                    state.selected -= 1;
                    // Jump to previous page if we go above current page
                    if state.selected < state.page_start {
                        state.page_start = state.page_start.saturating_sub(state.page_size);
                    }
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let filtered = app.card_browser_filtered_entries();
            let max = filtered.len().saturating_sub(1);
            if let Some(ref mut state) = app.card_browser_state {
                if state.selected < max {
                    state.selected += 1;
                    // Jump to next page if we go below current page
                    if state.selected >= state.page_start + state.page_size {
                        state.page_start += state.page_size;
                    }
                }
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            // Previous page
            if let Some(ref mut state) = app.card_browser_state {
                if state.page_start > 0 {
                    state.page_start = state.page_start.saturating_sub(state.page_size);
                    state.selected = state.page_start;
                }
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            // Next page
            let filtered_len = app.card_browser_filtered_entries().len();
            if let Some(ref mut state) = app.card_browser_state {
                let next_start = state.page_start + state.page_size;
                if next_start < filtered_len {
                    state.page_start = next_start;
                    state.selected = next_start;
                }
            }
        }
        KeyCode::Char('f') => {
            // Cycle filter
            if let Some(ref mut state) = app.card_browser_state {
                state.filter = state.filter.next();
                state.selected = 0;
                state.page_start = 0;
            }
        }
        KeyCode::Char('s') => {
            // Cycle sort
            if let Some(ref mut state) = app.card_browser_state {
                state.sort = state.sort.next();
            }
        }
        KeyCode::Char('r') => {
            // Reset selected card
            let card_id = {
                let filtered = app.card_browser_filtered_entries();
                app.card_browser_state.as_ref().and_then(|state| {
                    filtered
                        .get(state.selected)
                        .and_then(|&idx| state.entries.get(idx))
                        .map(|e| e.card.id)
                })
            };
            if let Some(card_id) = card_id {
                let conn = match app.open_db() {
                    Ok(c) => c,
                    Err(e) => {
                        app.set_message(format!("DB error: {}", e));
                        return;
                    }
                };
                if let Err(e) = models::reset_srs_card(&conn, card_id) {
                    app.set_message(format!("Error: {}", e));
                } else {
                    app.set_message("Card reset to new");
                    let _ = app.load_card_browser();
                }
            }
        }
        KeyCode::Enter => {
            // Open card detail popup
            if let Err(e) = app.open_card_browser_detail() {
                app.set_message(format!("Error: {}", e));
            }
        }
        KeyCode::Char('d') => {
            // Delete selected card (with confirmation)
            let card_id = {
                let filtered = app.card_browser_filtered_entries();
                app.card_browser_state.as_ref().and_then(|state| {
                    filtered
                        .get(state.selected)
                        .and_then(|&idx| state.entries.get(idx))
                        .map(|e| e.card.id)
                })
            };
            if let Some(card_id) = card_id {
                app.popup = Some(PopupState::DeleteCardConfirm { card_id });
            }
        }
        KeyCode::Esc => {
            app.card_browser_state = None;
            app.navigate_back();
        }
        _ => {}
    }
}

fn handle_settings_key(app: &mut App, key: KeyEvent) {
    let editing = app
        .settings_state
        .as_ref()
        .map(|s| s.editing)
        .unwrap_or(false);

    if editing {
        // In edit mode for text/integer values
        match key.code {
            KeyCode::Esc => {
                if let Some(ref mut state) = app.settings_state {
                    state.editing = false;
                    state.edit_buffer.clear();
                }
            }
            KeyCode::Enter => {
                // Apply the edit buffer to the current setting
                if let Some(ref mut state) = app.settings_state {
                    let cat = state.selected_category;
                    let item = state.selected_item;
                    if let Some(setting) = state
                        .categories
                        .get_mut(cat)
                        .and_then(|c| c.items.get_mut(item))
                    {
                        match &setting.value {
                            app::SettingsValue::Integer(_) => {
                                if let Ok(v) = state.edit_buffer.parse::<i64>() {
                                    setting.value = app::SettingsValue::Integer(v);
                                    state.dirty = true;
                                }
                            }
                            app::SettingsValue::Text(_) => {
                                setting.value = app::SettingsValue::Text(state.edit_buffer.clone());
                                state.dirty = true;
                            }
                            _ => {}
                        }
                    }
                    state.editing = false;
                    state.edit_buffer.clear();
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut state) = app.settings_state {
                    state.edit_buffer.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(ref mut state) = app.settings_state {
                    state.edit_buffer.push(c);
                }
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut state) = app.settings_state {
                if state.selected_item > 0 {
                    state.selected_item -= 1;
                } else if state.selected_category > 0 {
                    state.selected_category -= 1;
                    state.selected_item = state.categories[state.selected_category]
                        .items
                        .len()
                        .saturating_sub(1);
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut state) = app.settings_state {
                let cat_items = state.categories[state.selected_category].items.len();
                if state.selected_item + 1 < cat_items {
                    state.selected_item += 1;
                } else if state.selected_category + 1 < state.categories.len() {
                    state.selected_category += 1;
                    state.selected_item = 0;
                }
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            // Switch to previous category
            if let Some(ref mut state) = app.settings_state {
                if state.selected_category > 0 {
                    state.selected_category -= 1;
                    state.selected_item = 0;
                }
            }
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
            // Switch to next category
            if let Some(ref mut state) = app.settings_state {
                if state.selected_category + 1 < state.categories.len() {
                    state.selected_category += 1;
                    state.selected_item = 0;
                }
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            // Toggle booleans, cycle choices, enter edit mode for text/integer
            if let Some(ref mut state) = app.settings_state {
                let cat = state.selected_category;
                let item = state.selected_item;
                if let Some(setting) = state
                    .categories
                    .get_mut(cat)
                    .and_then(|c| c.items.get_mut(item))
                {
                    match &setting.value {
                        app::SettingsValue::Bool(v) => {
                            setting.value = app::SettingsValue::Bool(!v);
                            state.dirty = true;
                        }
                        app::SettingsValue::Choice(current, options) => {
                            // Cycle to next option
                            let idx = options.iter().position(|o| o == current).unwrap_or(0);
                            let next_idx = (idx + 1) % options.len();
                            let new_val = options[next_idx].clone();
                            let opts = options.clone();
                            let key = setting.key.clone();
                            setting.value = app::SettingsValue::Choice(new_val.clone(), opts);
                            state.dirty = true;

                            // Live-preview theme changes
                            if key == "general.theme" {
                                let mut new_theme = crate::ui::theme::Theme::load(&new_val, None);
                                new_theme.apply_color_fallback();
                                app.theme = new_theme;
                            }
                        }
                        app::SettingsValue::Integer(v) => {
                            state.edit_buffer = v.to_string();
                            state.editing = true;
                        }
                        app::SettingsValue::Text(v) => {
                            state.edit_buffer = v.clone();
                            state.editing = true;
                        }
                    }
                }
            }
        }
        KeyCode::Char('s') => {
            // Save settings
            if let Err(e) = app.apply_settings() {
                app.set_message(format!("Error saving settings: {}", e));
            }
        }
        KeyCode::Esc => {
            // Check if dirty and warn, or just go back
            let dirty = app
                .settings_state
                .as_ref()
                .map(|s| s.dirty)
                .unwrap_or(false);
            if dirty {
                // Auto-save on exit
                if let Err(e) = app.apply_settings() {
                    app.set_message(format!("Error saving settings: {}", e));
                }
            }
            app.settings_state = None;
            app.navigate_back();
        }
        _ => {}
    }
}

/// Handle keys while in expression marking mode.
/// Left/Right extend the selection range, Enter saves, Esc cancels.
fn handle_expression_mode_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut state) = app.reader_state {
                if let Some((ref mut start, _end)) = state.expression_mark {
                    if *start > 0 {
                        *start -= 1;
                    }
                }
            }
            show_expression_preview(app);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(ref mut state) = app.reader_state {
                let max_idx = state.sentences[state.sentence_index]
                    .tokens
                    .len()
                    .saturating_sub(1);
                if let Some((_start, ref mut end)) = state.expression_mark {
                    if *end < max_idx {
                        *end += 1;
                    }
                }
            }
            show_expression_preview(app);
        }
        KeyCode::Enter => {
            if let Err(e) = app.save_expression_mark() {
                app.set_message(format!("Error saving expression: {}", e));
            }
        }
        KeyCode::Esc => {
            if let Some(ref mut state) = app.reader_state {
                state.expression_mark = None;
            }
            app.set_message("Expression marking cancelled");
        }
        _ => {}
    }
}

/// Show a preview of the expression being marked in the status bar.
fn show_expression_preview(app: &mut App) {
    if let Some(ref state) = app.reader_state {
        if let Some((start, end)) = state.expression_mark {
            let sentence = &state.sentences[state.sentence_index];
            let surface: String = sentence.tokens[start..=end]
                .iter()
                .map(|t| t.surface.as_str())
                .collect();
            app.set_message(format!(
                "Marking: 「{}」 — ←/→ extend, Enter save, Esc cancel",
                surface
            ));
        }
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
        PopupState::CardDetail { .. } => match key.code {
            KeyCode::Esc | KeyCode::Enter => app.popup = None,
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(PopupState::CardDetail { ref mut scroll, .. }) = app.popup {
                    *scroll = scroll.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(PopupState::CardDetail { ref mut scroll, .. }) = app.popup {
                    *scroll += 1;
                }
            }
            _ => {}
        },
        PopupState::Help { .. } => match key.code {
            KeyCode::Esc | KeyCode::Char('?') => app.popup = None,
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(PopupState::Help { ref mut scroll }) = app.popup {
                    *scroll = scroll.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(PopupState::Help { ref mut scroll }) = app.popup {
                    *scroll = scroll.saturating_add(1);
                }
            }
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
        PopupState::ExpressionTranslation { .. } => match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.set_message("Expression cancelled");
            }
            KeyCode::Enter => {
                if let Err(e) = app.save_expression_with_translation() {
                    app.set_message(format!("Error saving expression: {}", e));
                }
            }
            KeyCode::Backspace => {
                if let Some(PopupState::ExpressionTranslation { ref mut gloss, .. }) = app.popup {
                    gloss.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(PopupState::ExpressionTranslation { ref mut gloss, .. }) = app.popup {
                    gloss.push(c);
                }
            }
            _ => {}
        },
        PopupState::TranslationEditor { .. } => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Enter => {
                if let Err(e) = app.save_word_translation() {
                    app.set_message(format!("Error saving translation: {}", e));
                }
            }
            KeyCode::Backspace => {
                if let Some(PopupState::TranslationEditor { ref mut text, .. }) = app.popup {
                    text.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(PopupState::TranslationEditor { ref mut text, .. }) = app.popup {
                    text.push(c);
                }
            }
            _ => {}
        },
        PopupState::SentenceTranslationEditor { .. } => match key.code {
            KeyCode::Esc => app.popup = None,
            KeyCode::Enter => {
                if let Err(e) = app.save_sentence_translation() {
                    app.set_message(format!("Error saving translation: {}", e));
                }
            }
            KeyCode::Backspace => {
                if let Some(PopupState::SentenceTranslationEditor {
                    ref mut translation,
                    ..
                }) = app.popup
                {
                    translation.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(PopupState::SentenceTranslationEditor {
                    ref mut translation,
                    ..
                }) = app.popup
                {
                    translation.push(c);
                }
            }
            _ => {}
        },
        PopupState::DeleteCardConfirm { .. } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(PopupState::DeleteCardConfirm { card_id }) = app.popup {
                    let conn = match app.open_db() {
                        Ok(c) => c,
                        Err(e) => {
                            app.set_message(format!("DB error: {}", e));
                            app.popup = None;
                            return;
                        }
                    };
                    if let Err(e) = models::delete_srs_card(&conn, card_id) {
                        app.set_message(format!("Error deleting card: {}", e));
                    } else {
                        app.set_message("Card deleted");
                        // Reload card browser
                        let _ = app.load_card_browser();
                    }
                }
                app.popup = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.popup = None,
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
    use app::HomeFocus;

    // Tab switches between heatmap and text list
    if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
        if let Some(ref mut home) = app.home_state {
            home.focus = match home.focus {
                HomeFocus::Heatmap => HomeFocus::TextList,
                HomeFocus::TextList => HomeFocus::Heatmap,
            };
        }
        return;
    }

    // Delegate to the focused panel
    let focus = app
        .home_state
        .as_ref()
        .map(|h| h.focus)
        .unwrap_or(HomeFocus::TextList);

    match focus {
        HomeFocus::Heatmap => handle_home_heatmap_key(app, key),
        HomeFocus::TextList => handle_home_textlist_key(app, key),
    }
}

fn handle_home_heatmap_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut home) = app.home_state {
                // Move cursor left (previous day = previous column)
                let weeks = home.heatmap_cells / 7;
                if weeks == 0 {
                    return;
                }
                let row = home.heatmap_cursor / weeks;
                let col = home.heatmap_cursor % weeks;
                if col > 0 {
                    home.heatmap_cursor = row * weeks + col - 1;
                }
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(ref mut home) = app.home_state {
                let weeks = home.heatmap_cells / 7;
                if weeks == 0 {
                    return;
                }
                let row = home.heatmap_cursor / weeks;
                let col = home.heatmap_cursor % weeks;
                if col + 1 < weeks {
                    home.heatmap_cursor = row * weeks + col + 1;
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut home) = app.home_state {
                let weeks = home.heatmap_cells / 7;
                if weeks == 0 {
                    return;
                }
                let row = home.heatmap_cursor / weeks;
                if row > 0 {
                    let col = home.heatmap_cursor % weeks;
                    home.heatmap_cursor = (row - 1) * weeks + col;
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut home) = app.home_state {
                let weeks = home.heatmap_cells / 7;
                if weeks == 0 {
                    return;
                }
                let row = home.heatmap_cursor / weeks;
                let col = home.heatmap_cursor % weeks;
                if row + 1 < 7 {
                    home.heatmap_cursor = (row + 1) * weeks + col;
                }
            }
        }
        // Pass through common shortcuts
        _ => handle_home_common_key(app, key),
    }
}

fn handle_home_textlist_key(app: &mut App, key: KeyEvent) {
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
                let filtered = home_filtered_texts(home);
                home.selected = home.selected.min(filtered.len().saturating_sub(1));
            }
        }
        _ => handle_home_common_key(app, key),
    }
}

fn handle_home_common_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('l') => {
            app.screen = Screen::Library;
            if let Err(e) = app.refresh_library() {
                app.set_message(format!("Error: {}", e));
            }
        }
        KeyCode::Char('r') => {
            if let Err(e) = app.start_review_session() {
                app.set_message(format!("Review error: {}", e));
            }
        }
        KeyCode::Char('i') => {
            app.popup = Some(PopupState::ImportMenu);
        }
        KeyCode::Char('c') => {
            if let Err(e) = app.load_card_browser() {
                app.set_message(format!("Card browser error: {}", e));
            }
        }
        KeyCode::Char('s') => {
            app.load_settings();
        }
        KeyCode::Char('S') => {
            if let Err(e) = app.load_stats() {
                app.set_message(format!("Stats error: {}", e));
            }
        }
        _ => {}
    }
}

fn handle_stats_key(app: &mut App, key: KeyEvent) {
    use app::StatsFocus;

    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            if let Some(ref mut state) = app.stats_state {
                state.focus = match state.focus {
                    StatsFocus::Overview => StatsFocus::Coverage,
                    StatsFocus::Coverage => StatsFocus::Overview,
                };
            }
        }
        KeyCode::Char('t') => {
            // Toggle time range and reload data
            let next = app
                .stats_state
                .as_ref()
                .map(|s| s.time_range.next())
                .unwrap_or(app::StatsTimeRange::Month);
            if let Err(e) = app.load_stats_with_range(next) {
                app.set_message(format!("Stats error: {}", e));
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut state) = app.stats_state {
                match state.focus {
                    StatsFocus::Coverage => {
                        state.coverage_selected = state.coverage_selected.saturating_sub(1);
                    }
                    StatsFocus::Overview => {
                        state.scroll = state.scroll.saturating_sub(1);
                    }
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut state) = app.stats_state {
                match state.focus {
                    StatsFocus::Coverage => {
                        let max = state.coverages.len().saturating_sub(1);
                        state.coverage_selected = (state.coverage_selected + 1).min(max);
                    }
                    StatsFocus::Overview => {
                        state.scroll += 1;
                    }
                }
            }
        }
        KeyCode::Enter => {
            // Open selected text in Reader (from coverage list)
            let text_id = app.stats_state.as_ref().and_then(|s| {
                if s.focus == StatsFocus::Coverage {
                    s.coverages.get(s.coverage_selected).map(|c| c.text_id)
                } else {
                    None
                }
            });
            if let Some(id) = text_id {
                app.previous_screen = Some(Screen::Stats);
                if let Err(e) = app.load_text(id) {
                    app.set_message(format!("Error loading text: {}", e));
                }
            }
        }
        KeyCode::Esc => {
            app.navigate_back();
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

fn handle_review_key(app: &mut App, key: KeyEvent) {
    let phase = app
        .review_state
        .as_ref()
        .map(|s| s.phase.clone())
        .unwrap_or(ReviewPhase::SessionSummary);

    match phase {
        ReviewPhase::PreSession => match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                app.begin_review();
            }
            KeyCode::Esc => {
                app.review_state = None;
                app.navigate_back();
            }
            _ => {}
        },
        ReviewPhase::ShowFront => match key.code {
            KeyCode::Char(' ') => {
                app.reveal_answer();
            }
            KeyCode::Enter => {
                // If a word is selected in sentence context, show dict entry; otherwise flip
                let has_selection = app
                    .review_state
                    .as_ref()
                    .map(|s| s.context_word_index.is_some())
                    .unwrap_or(false);
                if has_selection {
                    if let Err(e) = app.open_review_word_detail() {
                        app.set_message(format!("Error: {}", e));
                    }
                } else {
                    app.reveal_answer();
                }
            }
            // LLM analysis of card's source sentence
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let sentence_text = app
                    .review_state
                    .as_ref()
                    .and_then(|s| s.queue.get(s.current_index))
                    .map(|c| c.sentence_text.clone())
                    .unwrap_or_default();
                if !sentence_text.is_empty() {
                    if let Err(e) = app.request_llm_analysis_for_sentence(sentence_text) {
                        app.set_message(format!("LLM error: {}", e));
                    }
                } else {
                    app.set_message("No sentence context for this card");
                }
            }
            KeyCode::Esc => {
                app.review_state = None;
                app.navigate_back();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(ref mut state) = app.review_state {
                    state.context_word_index =
                        Some(state.context_word_index.unwrap_or(0).saturating_sub(1));
                }
            }
            KeyCode::Right => {
                if let Some(ref mut state) = app.review_state {
                    if let Some(card_data) = state.queue.get(state.current_index) {
                        let max_idx = card_data.sentence_tokens.len().saturating_sub(1);
                        state.context_word_index = Some(
                            state
                                .context_word_index
                                .map(|i| (i + 1).min(max_idx))
                                .unwrap_or(0),
                        );
                    }
                }
            }
            _ => {}
        },
        ReviewPhase::ShowBack | ReviewPhase::ShowResult => match key.code {
            KeyCode::Char('1') => {
                if let Err(e) = app.rate_card(Rating::Again) {
                    app.set_message(format!("Rating error: {}", e));
                }
            }
            KeyCode::Char('2') => {
                if let Err(e) = app.rate_card(Rating::Hard) {
                    app.set_message(format!("Rating error: {}", e));
                }
            }
            KeyCode::Char('3') | KeyCode::Char(' ') => {
                if let Err(e) = app.rate_card(Rating::Good) {
                    app.set_message(format!("Rating error: {}", e));
                }
            }
            KeyCode::Char('4') => {
                if let Err(e) = app.rate_card(Rating::Easy) {
                    app.set_message(format!("Rating error: {}", e));
                }
            }
            KeyCode::Enter => {
                // Accept auto-rating (for typed reading mode)
                let rating = app.review_state.as_ref().and_then(|s| s.auto_rating);
                if let Some(r) = rating {
                    if let Err(e) = app.rate_card(r) {
                        app.set_message(format!("Rating error: {}", e));
                    }
                }
            }
            // LLM analysis of card's source sentence
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let sentence_text = app
                    .review_state
                    .as_ref()
                    .and_then(|s| s.queue.get(s.current_index))
                    .map(|c| c.sentence_text.clone())
                    .unwrap_or_default();
                if !sentence_text.is_empty() {
                    if let Err(e) = app.request_llm_analysis_for_sentence(sentence_text) {
                        app.set_message(format!("LLM error: {}", e));
                    }
                } else {
                    app.set_message("No sentence context for this card");
                }
            }
            KeyCode::Esc => {
                app.review_state = None;
                app.navigate_back();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(ref mut state) = app.review_state {
                    state.context_word_index =
                        Some(state.context_word_index.unwrap_or(0).saturating_sub(1));
                }
            }
            KeyCode::Right => {
                if let Some(ref mut state) = app.review_state {
                    if let Some(card_data) = state.queue.get(state.current_index) {
                        let max = card_data.sentence_tokens.len().saturating_sub(1);
                        state.context_word_index = Some(
                            state
                                .context_word_index
                                .map(|i| (i + 1).min(max))
                                .unwrap_or(0),
                        );
                    }
                }
            }
            _ => {}
        },
        ReviewPhase::TypingAnswer => match key.code {
            KeyCode::Enter => {
                app.submit_typed_answer();
            }
            KeyCode::Backspace => {
                if let Some(ref mut state) = app.review_state {
                    state.typed_input.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(ref mut state) = app.review_state {
                    state.typed_input.push(c);
                }
            }
            KeyCode::Esc => {
                // Skip this card (reveal answer instead)
                app.reveal_answer();
            }
            _ => {}
        },
        ReviewPhase::SessionSummary => match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                app.review_state = None;
                app.navigate_back();
            }
            KeyCode::Char('r') => {
                // Continue with more reviews
                if let Err(e) = app.start_review_session() {
                    app.set_message(format!("Review error: {}", e));
                }
            }
            _ => {}
        },
    }
}

fn handle_reader_key(app: &mut App, key: KeyEvent) {
    // Check if we're in expression marking mode
    let in_expression_mode = app
        .reader_state
        .as_ref()
        .map(|s| s.expression_mark.is_some())
        .unwrap_or(false);

    if in_expression_mode {
        handle_expression_mode_key(app, key);
        return;
    }

    match key.code {
        // Sentence navigation
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut state) = app.reader_state {
                if state.sentence_index > 0 {
                    state.sentence_index -= 1;
                    state.word_index = None;
                    state.sidebar_scroll = 0;
                    state.show_llm_sidebar = false;
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
                        state.show_llm_sidebar = false;
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
                // Collect sentence contexts for Learning words before autopromoting
                let _ = app.collect_sentence_contexts(dep);
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
                } else {
                    // Record the sentence advancement for daily activity
                    app.record_sentence_read();
                }
            }
            if !matches!(departing, Some((_, true))) {
                let _ = app.save_reading_progress();
            }
        }

        // Word navigation (skips trivial tokens AND non-head group members)
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut state) = app.reader_state {
                if state.sentences.is_empty() {
                    return;
                }
                let sentence = &state.sentences[state.sentence_index];
                match state.word_index {
                    None => {
                        state.word_index = sentence.tokens.iter().rposition(|t| t.is_navigable());
                    }
                    Some(i) => {
                        let prev = if i > 0 {
                            sentence.tokens[..i].iter().rposition(|t| t.is_navigable())
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
                                        prev_sentence.tokens.iter().rposition(|t| t.is_navigable());
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
                                    sentence.tokens.iter().position(|t| t.is_navigable());
                            }
                            Some(i) => {
                                let next = sentence.tokens[i + 1..]
                                    .iter()
                                    .position(|t| t.is_navigable())
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
                                                .position(|t| t.is_navigable());
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
                // Collect sentence contexts for Learning words before autopromoting
                let _ = app.collect_sentence_contexts(dep);
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

        // Copy selected word to clipboard
        KeyCode::Char('c') => {
            if let Err(e) = app.copy_word_to_clipboard() {
                app.set_message(format!("Copy failed: {}", e));
            }
        }

        // Copy current sentence to clipboard
        KeyCode::Char('C') => {
            if let Err(e) = app.copy_sentence_to_clipboard() {
                app.set_message(format!("Copy failed: {}", e));
            }
        }

        // LLM auto-translate current sentence
        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Err(e) = app.request_llm_translation() {
                app.set_message(format!("LLM error: {}", e));
            }
        }

        // Word translation editor
        KeyCode::Char('t') => {
            if let Err(e) = app.open_translation_editor() {
                app.set_message(format!("Error: {}", e));
            }
        }

        // Sentence translation editor (manual)
        KeyCode::Char('T') => {
            if let Err(e) = app.open_sentence_translation_editor() {
                app.set_message(format!("Error: {}", e));
            }
        }

        // Open selected word on Jisho.org
        KeyCode::Char('g') => {
            if let Err(e) = app.open_word_in_jisho() {
                app.set_message(format!("Error: {}", e));
            }
        }

        // Open current sentence in browser translation service (DeepL/Google)
        KeyCode::Char('G') => {
            if let Err(e) = app.open_sentence_in_browser() {
                app.set_message(format!("Error: {}", e));
            }
        }

        // Mark expression (enter expression marking mode)
        KeyCode::Char('m') => {
            if let Some(ref mut state) = app.reader_state {
                if let Some(wi) = state.word_index {
                    state.expression_mark = Some((wi, wi));
                    app.set_message("Expression mode: ←/→ to extend, Enter to save, Esc to cancel");
                } else {
                    app.set_message("Select a word first (←/→), then press 'm' to mark expression");
                }
            }
        }

        // Deselect word, dismiss LLM sidebar, or go back to previous screen
        KeyCode::Esc => {
            // First dismiss LLM sidebar if showing
            let showing_llm = app
                .reader_state
                .as_ref()
                .map(|s| s.show_llm_sidebar)
                .unwrap_or(false);
            if showing_llm {
                if let Some(ref mut state) = app.reader_state {
                    state.show_llm_sidebar = false;
                    state.sidebar_scroll = 0;
                }
                return;
            }
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
