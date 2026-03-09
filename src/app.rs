use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::time::Instant;

use crate::config::AppConfig;
use crate::core::dictionary::{self, DictEntry};
use crate::core::srs::{Rating, SrsEngine};
use crate::core::tokenizer::{self, GroupToken};
use crate::db::models::{self, AnswerMode, SrsCard, TextStats, Vocabulary, VocabularyStatus};

/// Compute a reasonable page_size for chapter select based on terminal height.
/// Layout: 1 title bar + 4 source info + 2 list borders + 1 status bar = 8 fixed rows.
pub fn chapter_page_size_for_terminal() -> usize {
    let height = crossterm::terminal::size()
        .map(|(_, h)| h as usize)
        .unwrap_or(40);
    // Available rows for chapter items = total - fixed overhead
    height.saturating_sub(8).max(5)
}

/// Which screen is currently active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    Library,
    ChapterSelect { source_id: i64 },
    Reader,
    Review,
    CardBrowser,
    Settings,
}

/// What kind of popup is being shown.
#[derive(Debug, Clone)]
pub enum PopupState {
    /// Full dictionary entry for a word.
    WordDetail {
        base_form: String,
        reading: String,
        entries: Vec<DictEntry>,
        conjugations: Vec<(String, i32)>,
        notes: Option<String>,
        scroll: usize,
    },
    /// Help overlay showing keybindings.
    Help { scroll: usize },
    /// Note editor for a word.
    NoteEditor { vocabulary_id: i64, text: String },
    /// Quit confirmation.
    QuitConfirm,
    /// Delete text confirmation.
    DeleteConfirm { text_id: i64, title: String },
    /// Delete web source confirmation.
    DeleteSourceConfirm { source_id: i64, title: String },
    /// Import sub-menu (clipboard / URL / file).
    ImportMenu,
    /// URL text input for web import.
    UrlInput { text: String },
    /// Search/filter input for library.
    SearchInput { text: String },
    /// File path input for file/epub/subtitle import.
    FilePathInput { text: String },
    /// Syosetu ncode input.
    SyosetuInput { text: String },
    /// Translation input for a newly created expression.
    /// Shown after the user marks an expression and presses Enter.
    ExpressionTranslation {
        surface: String,
        reading: String,
        gloss: String,
    },
    /// Word translation editor (keybinding: `t` in Reader).
    TranslationEditor {
        vocabulary_id: i64,
        base_form: String,
        reading: String,
        text: String,
    },
    /// Sentence translation editor (keybinding: `T` in Reader).
    SentenceTranslationEditor {
        text_id: i64,
        sentence_index: usize,
        sentence_text: String,
        translation: String,
    },
    /// Delete SRS card confirmation.
    DeleteCardConfirm { card_id: i64 },
}

/// A token as displayed in the reader.
#[derive(Debug, Clone)]
pub struct TokenDisplay {
    pub surface: String,
    pub base_form: String,
    /// Lemma/base form reading (for vocabulary matching).
    pub reading: String,
    /// Surface form reading (for furigana display — matches the conjugated form).
    pub surface_reading: String,
    pub pos: String,
    pub vocabulary_status: VocabularyStatus,
    pub is_selected: bool,
    pub short_gloss: String,
    pub conjugation_form: String,
    pub conjugation_type: String,
    pub is_trivial: bool,
    /// Shared group index within the sentence (None = standalone token).
    pub group_id: Option<usize>,
    /// True for the vocabulary-bearing head token of a conjugation/MWE group.
    pub is_group_head: bool,
    /// Human-readable conjugation description: "verb, negative, past".
    pub conjugation_desc: String,
    /// For MWE groups: the expression's English meaning.
    pub mwe_gloss: String,
}

impl TokenDisplay {
    /// Whether this token should be a navigation target when using Left/Right keys.
    /// Skips trivial tokens and non-head group members (auxiliaries in conjugation groups).
    pub fn is_navigable(&self) -> bool {
        !self.is_trivial && (self.group_id.is_none() || self.is_group_head)
    }
}

/// A multi-word expression match detected in a sentence.
#[derive(Debug, Clone)]
pub struct MweMatch {
    /// First token index in the sentence.
    pub start: usize,
    /// One past the last token index.
    pub end: usize,
    /// Concatenated surface text of the matched tokens.
    pub surface: String,
    /// Reading from JMdict or user expression.
    pub reading: String,
    /// English meaning.
    pub gloss: String,
}

/// Data for a single sentence in the reader.
#[derive(Debug, Clone)]
pub struct SentenceData {
    pub paragraph_idx: usize,
    pub start_token: usize,
    pub end_token: usize,
    pub tokens: Vec<TokenDisplay>,
    pub text: String,
}

/// Data for a paragraph in the reader.
#[derive(Debug, Clone)]
pub struct ParagraphData {
    pub id: i64,
    pub position: i32,
    pub content: String,
    pub db_tokens: Vec<models::Token>,
}

/// A batch of words that were autopromoted from New to Known on a single sentence advance.
#[derive(Debug, Clone)]
pub struct AutopromotionBatch {
    /// Which sentence the user was leaving when this batch was created.
    pub sentence_index: usize,
    /// The words that were promoted: (base_form, reading, vocabulary_id).
    pub words: Vec<(String, String, i64)>,
}

/// State for the reader screen.
pub struct ReaderState {
    pub text_id: i64,
    pub text_title: String,
    pub paragraphs: Vec<ParagraphData>,
    pub sentences: Vec<SentenceData>,
    pub sentence_index: usize,
    pub word_index: Option<usize>,
    pub vocabulary_cache: HashMap<(String, String), Vocabulary>,
    /// Cached short glosses keyed by base_form to avoid repeated DB lookups.
    pub gloss_cache: HashMap<String, String>,
    pub scroll_offset: usize,
    pub sidebar_scroll: usize,
    /// Whether autopromotion of New words to Known is active (per-session, default true).
    pub autopromote_enabled: bool,
    /// Undo stack: each entry is a batch of words autopromoted on a single sentence advance.
    pub autopromote_history: Vec<AutopromotionBatch>,
    /// Whether to show readings for all words in the sidebar (per-session toggle, default false).
    pub show_all_readings: bool,
    /// Whether to show Known/Ignored words in the sidebar word list (per-session toggle, default false).
    pub show_known_in_sidebar: bool,
    /// Cached MWE matches per sentence (computed once during load_text).
    pub mwe_matches: Vec<Vec<MweMatch>>,
    /// Expression marking mode: Some((start, end)) token indices in current sentence.
    /// When active, Left/Right extend the range. Enter saves, Esc cancels.
    pub expression_mark: Option<(usize, usize)>,
    /// Cached sentence translations keyed by sentence_index.
    pub sentence_translations: HashMap<usize, models::SentenceTranslation>,
}

/// How the library list is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySort {
    DateDesc,
    DateAsc,
    TitleAsc,
    Completion,
}

impl LibrarySort {
    pub fn next(self) -> Self {
        match self {
            Self::DateDesc => Self::DateAsc,
            Self::DateAsc => Self::TitleAsc,
            Self::TitleAsc => Self::Completion,
            Self::Completion => Self::DateDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DateDesc => "Date ↓",
            Self::DateAsc => "Date ↑",
            Self::TitleAsc => "Title A-Z",
            Self::Completion => "Completion %",
        }
    }
}

/// An item shown in the library list — either a standalone text or a grouped source.
#[derive(Debug, Clone)]
pub enum LibraryItem {
    /// A single text (plain text, clipboard, web, subtitle).
    Text(models::Text),
    /// A grouped multi-chapter source (Syosetu, EPUB).
    Source(models::WebSource),
}

impl LibraryItem {
    pub fn title(&self) -> &str {
        match self {
            LibraryItem::Text(t) => &t.title,
            LibraryItem::Source(s) => &s.title,
        }
    }

    pub fn source_type(&self) -> &str {
        match self {
            LibraryItem::Text(t) => &t.source_type,
            LibraryItem::Source(s) => &s.source_type,
        }
    }

    pub fn created_at(&self) -> &str {
        match self {
            LibraryItem::Text(t) => &t.created_at,
            LibraryItem::Source(s) => &s.last_synced,
        }
    }
}

/// Library screen state.
pub struct LibraryState {
    pub items: Vec<LibraryItem>,
    pub stats: HashMap<i64, TextStats>,
    pub source_chapter_counts: HashMap<i64, (usize, usize, usize)>, // source_id -> (total, imported, skipped)
    pub selected: usize,
    pub sort: LibrarySort,
    pub filter_source: Option<String>,
    /// All unique source types present in the DB.
    pub source_types: Vec<String>,
}

/// Reading state of a chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterReadState {
    /// Not imported yet.
    NotImported,
    /// Imported but not started reading.
    Unread,
    /// Reading in progress (has progress but not finished).
    InProgress,
    /// Finished reading (reached the end).
    Finished,
}

/// Chapter select screen state (for multi-chapter sources: Syosetu, EPUB, etc.).
pub struct ChapterSelectState {
    pub source: models::WebSource,
    pub chapters: Vec<models::WebSourceChapter>,
    pub selected: usize,
    /// Index of the first chapter visible on the current page.
    pub page_start: usize,
    /// Available rows for the chapter list (terminal height minus chrome).
    pub page_size: usize,
    pub total_chapters: usize,
    pub total_imported: usize,
    pub total_skipped: usize,
    /// True while novel metadata is being fetched in background.
    pub loading: bool,
    /// Reading state per chapter (keyed by chapter id).
    pub chapter_read_states: HashMap<i64, ChapterReadState>,
}

impl ChapterSelectState {
    /// Get the chapters visible on the current page, accounting for group
    /// headers that consume extra rows.
    pub fn visible_chapters(&self) -> &[models::WebSourceChapter] {
        let start = self.page_start;
        if start >= self.chapters.len() {
            return &[];
        }

        let mut rows_left = self.page_size;
        let mut last_group: Option<&str> = None;
        let mut end = start;

        for ch in &self.chapters[start..] {
            // Check if this chapter triggers a new group header
            if !ch.chapter_group.is_empty() {
                let new_group = match last_group {
                    Some(prev) => prev != ch.chapter_group.as_str(),
                    None => true,
                };
                if new_group {
                    if rows_left == 0 {
                        break;
                    }
                    rows_left -= 1; // group header takes a row
                }
                last_group = Some(&ch.chapter_group);
            } else if last_group.is_some() {
                last_group = None;
            }

            if rows_left == 0 {
                break;
            }
            rows_left -= 1; // chapter itself takes a row
            end += 1;
        }

        &self.chapters[start..end]
    }

    /// Compute the start index of the next page (after current visible chapters).
    pub fn next_page_start(&self) -> usize {
        let vis = self.visible_chapters();
        if vis.is_empty() {
            self.chapters.len()
        } else {
            self.page_start + vis.len()
        }
    }

    /// Index within the current page.
    pub fn page_selected(&self) -> usize {
        self.selected.saturating_sub(self.page_start)
    }

    /// Approximate total number of pages (for display only).
    pub fn total_pages(&self) -> usize {
        if self.chapters.is_empty() {
            1
        } else {
            // Approximate: we can't know exact page count without walking all
            // group headers, but page_size gives a reasonable estimate.
            (self.chapters.len() + self.page_size - 1) / self.page_size
        }
    }

    /// Current page number (1-indexed for display), computed from page_start.
    pub fn current_page_display(&self) -> usize {
        if self.chapters.is_empty() {
            1
        } else {
            // Approximate page number based on position
            (self.page_start / self.page_size.max(1)) + 1
        }
    }
}

/// Home screen state.
pub struct HomeState {
    pub recent_texts: Vec<models::Text>,
    pub recent_stats: HashMap<i64, TextStats>,
    pub selected: usize,
    /// Whether to show finished texts in the recent list (default false).
    pub show_finished: bool,
    /// Number of SRS cards due for review (due_total, due_new).
    pub due_card_counts: (usize, usize),
}

/// Phase of the review card display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewPhase {
    /// Showing the front of the card (question).
    ShowFront,
    /// Showing the back of the card (answer).
    ShowBack,
    /// User is typing an answer (typed reading mode).
    TypingAnswer,
    /// Showing result after typing (diff display).
    ShowResult,
    /// Session summary (all cards done).
    SessionSummary,
    /// Pre-session: showing card count and asking to start.
    PreSession,
}

/// Data needed to display a review card.
#[derive(Debug, Clone)]
pub struct ReviewCardData {
    /// The SRS card from DB.
    pub card: SrsCard,
    /// The vocabulary entry.
    pub vocabulary: Vocabulary,
    /// JMdict definitions for display.
    pub definitions: Vec<DictEntry>,
    /// Sentence context tokens (for coloring).
    pub sentence_tokens: Vec<TokenDisplay>,
    /// Sentence text.
    pub sentence_text: String,
    /// Answer mode for this card.
    pub answer_mode: AnswerMode,
    /// Full display surface (compound surface for groups, e.g. "食べられない").
    /// Falls back to vocabulary.base_form for standalone words.
    pub display_surface: String,
    /// Full display reading for compounds (e.g. "たべられない").
    /// Falls back to vocabulary.reading for standalone words.
    pub display_reading: String,
    /// Group ID of the target word in sentence_tokens (for cloze blanking).
    pub target_group_id: Option<usize>,
    /// For SentenceFull cards: the sentence translation text.
    pub sentence_translation_text: Option<String>,
}

/// Review screen state.
pub struct ReviewState {
    /// Queue of cards to review this session.
    pub queue: Vec<ReviewCardData>,
    /// Current position in the queue.
    pub current_index: usize,
    /// Current phase of card display.
    pub phase: ReviewPhase,
    /// User's typed input (for typed reading mode).
    pub typed_input: String,
    /// When the current card was shown (for elapsed time tracking).
    pub card_shown_at: Instant,
    /// Word index for navigating sentence context.
    pub context_word_index: Option<usize>,
    /// Session statistics.
    pub total_reviewed: usize,
    pub correct_count: usize,
    /// Total due cards (before filtering/limiting).
    pub total_due: usize,
    /// Total new cards.
    pub total_new: usize,
    /// Vocabulary cache for sentence context rendering.
    pub vocabulary_cache: HashMap<(String, String), Vocabulary>,
    /// Auto-rating from typed answer comparison.
    pub auto_rating: Option<Rating>,
    /// Typed answer comparison result: (input, expected, is_correct).
    pub typed_result: Option<(String, String, bool)>,
}

// ─── Card Browser ────────────────────────────────────────────────────

/// An entry in the card browser list.
#[derive(Debug, Clone)]
pub struct CardBrowserEntry {
    pub card: SrsCard,
    pub vocabulary: Option<Vocabulary>,
    pub sentence_translation: Option<models::SentenceTranslation>,
    pub display_front: String,
    pub display_back: String,
    pub due_label: String,
}

/// Filter for the card browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardBrowserFilter {
    All,
    DueNow,
    WordCards,
    SentenceCards,
    New,
    Learning,
    Review,
    Retired,
}

impl CardBrowserFilter {
    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::DueNow => "Due Now",
            Self::WordCards => "Word Cards",
            Self::SentenceCards => "Sentence Cards",
            Self::New => "State: New",
            Self::Learning => "State: Learning",
            Self::Review => "State: Review",
            Self::Retired => "State: Retired",
        }
    }
    pub fn next(&self) -> Self {
        match self {
            Self::All => Self::DueNow,
            Self::DueNow => Self::WordCards,
            Self::WordCards => Self::SentenceCards,
            Self::SentenceCards => Self::New,
            Self::New => Self::Learning,
            Self::Learning => Self::Review,
            Self::Review => Self::Retired,
            Self::Retired => Self::All,
        }
    }
}

/// Sort order for the card browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardBrowserSort {
    DueDate,
    CreatedDate,
    Word,
}

impl CardBrowserSort {
    pub fn label(&self) -> &'static str {
        match self {
            Self::DueDate => "Due Date",
            Self::CreatedDate => "Created",
            Self::Word => "Word",
        }
    }
    pub fn next(&self) -> Self {
        match self {
            Self::DueDate => Self::CreatedDate,
            Self::CreatedDate => Self::Word,
            Self::Word => Self::DueDate,
        }
    }
}

/// Card browser screen state.
pub struct CardBrowserState {
    pub entries: Vec<CardBrowserEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub filter: CardBrowserFilter,
    pub sort: CardBrowserSort,
}

// ─── Settings ────────────────────────────────────────────────────────

/// A setting value type.
#[derive(Debug, Clone)]
pub enum SettingsValue {
    Bool(bool),
    Integer(i64),
    Text(String),
    /// A value chosen from a fixed list of options: (current_value, all_options).
    Choice(String, Vec<String>),
}

/// A single settings item.
#[derive(Debug, Clone)]
pub struct SettingsItem {
    pub key: String,
    pub label: String,
    pub description: String,
    pub value: SettingsValue,
}

/// A category of settings.
#[derive(Debug, Clone)]
pub struct SettingsCategory {
    pub name: String,
    pub items: Vec<SettingsItem>,
}

/// Settings screen state.
pub struct SettingsState {
    pub categories: Vec<SettingsCategory>,
    pub selected_category: usize,
    pub selected_item: usize,
    pub editing: bool,
    pub edit_buffer: String,
    pub dirty: bool,
}

/// Central application state.
pub struct App {
    pub screen: Screen,
    pub previous_screen: Option<Screen>,
    pub config: AppConfig,
    pub reader_state: Option<ReaderState>,
    pub library_state: Option<LibraryState>,
    pub chapter_select_state: Option<ChapterSelectState>,
    pub home_state: Option<HomeState>,
    pub review_state: Option<ReviewState>,
    pub card_browser_state: Option<CardBrowserState>,
    pub settings_state: Option<SettingsState>,
    pub srs_engine: Option<SrsEngine>,
    pub background_importer: Option<crate::import::background::BackgroundImporter>,
    /// Set of chapter_ids currently being preprocessed (for UI spinners).
    pub preprocessing_chapters: std::collections::HashSet<i64>,
    /// Progress of preprocessing chapters: chapter_id -> (phase, percent).
    pub preprocessing_progress: HashMap<i64, (&'static str, u8)>,
    pub popup: Option<PopupState>,
    pub message: Option<(String, Instant)>,
    pub should_quit: bool,
    pub db_path: std::path::PathBuf,
    /// Monotonic tick counter for animations (spinners, etc.).
    pub tick_count: u64,
    /// Chapter ID we're waiting on to auto-open when its import completes.
    pub pending_open_chapter: Option<i64>,
    /// Labels of text imports currently running in the background.
    pub pending_imports: Vec<String>,
    /// Persistent clipboard handle — kept alive so Linux clipboard managers
    /// can read the content before it disappears.
    pub clipboard: Option<arboard::Clipboard>,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        let db_path = config.db_path();
        Self {
            screen: Screen::Home,
            previous_screen: None,
            config,
            reader_state: None,
            library_state: None,
            chapter_select_state: None,
            home_state: None,
            review_state: None,
            card_browser_state: None,
            settings_state: None,
            srs_engine: SrsEngine::new().ok(),
            background_importer: None,
            preprocessing_chapters: std::collections::HashSet::new(),
            preprocessing_progress: HashMap::new(),
            popup: None,
            message: None,
            should_quit: false,
            db_path,
            tick_count: 0,
            pending_open_chapter: None,
            pending_imports: Vec::new(),
            clipboard: arboard::Clipboard::new().ok(),
        }
    }

    /// Initialize the background importer with an event sender.
    pub fn init_background_importer(
        &mut self,
        event_tx: std::sync::mpsc::Sender<crate::ui::events::Event>,
    ) {
        let import_tx = event_tx.clone();
        let (itx, irx) = std::sync::mpsc::channel();

        // Spawn a bridge thread that forwards ImportEvents into the main Event channel
        std::thread::spawn(move || {
            while let Ok(evt) = irx.recv() {
                if import_tx
                    .send(crate::ui::events::Event::Import(evt))
                    .is_err()
                {
                    break;
                }
            }
        });

        self.background_importer = Some(crate::import::background::BackgroundImporter::new(itx, 3));
    }

    /// Start preprocessing chapters for the current chapter select source.
    /// Ensures that the next `preprocess_ahead` unimported, unskipped chapters
    /// (counting from the first such chapter) are either already preprocessed,
    /// in-flight, or queued.
    pub fn start_preprocessing(&mut self) {
        let target = self.config.reader.preprocess_ahead;
        let state = match self.chapter_select_state.as_ref() {
            Some(s) => s,
            None => return,
        };

        // Find up to `target` chapters that need preprocessing:
        // Walk the chapter list, skip imported/skipped/in-flight/queued,
        // collect the ones that are not yet imported and not being processed.
        let mut to_queue: Vec<(i64, i32)> = Vec::new(); // (chapter_id, chapter_number)
        let mut budget = target;

        for ch in &state.chapters {
            if budget == 0 {
                break;
            }
            if ch.is_skipped {
                continue;
            }
            if ch.text_id.is_some() {
                // Already imported — counts against budget only if unread
                if matches!(
                    state.chapter_read_states.get(&ch.id),
                    Some(ChapterReadState::Unread)
                ) {
                    budget -= 1;
                }
                continue;
            }
            // Not imported — either already processing or needs to be queued
            if self.preprocessing_chapters.contains(&ch.id) {
                budget -= 1;
                continue;
            }
            // Needs to be queued
            to_queue.push((ch.id, ch.chapter_number));
            budget -= 1;
        }

        if to_queue.is_empty() {
            return;
        }

        let source_id = state.source.id;
        let source_type = state.source.source_type.clone();
        let external_id = state.source.external_id.clone();
        let db_path = self.db_path.clone();
        if let Some(ref mut importer) = self.background_importer {
            for (ch_id, ch_num) in to_queue {
                importer.queue_single(
                    source_id,
                    &source_type,
                    &external_id,
                    ch_id,
                    ch_num,
                    &db_path,
                );
                self.preprocessing_chapters.insert(ch_id);
            }
        }
    }

    /// Handle a background import event.
    pub fn handle_import_event(&mut self, event: crate::import::background::ImportEvent) {
        use crate::import::background::ImportEvent;
        match event {
            ImportEvent::Started {
                chapter_id,
                chapter_number,
                ..
            } => {
                self.preprocessing_chapters.insert(chapter_id);
                self.preprocessing_progress.insert(chapter_id, ("fetch", 0));
                self.set_message(format!("Preprocessing chapter {}...", chapter_number));
            }
            ImportEvent::Progress {
                chapter_id,
                phase,
                percent,
            } => {
                self.preprocessing_progress
                    .insert(chapter_id, (phase, percent));
            }
            ImportEvent::Completed {
                source_id,
                chapter_id,
                chapter_number,
                text_id,
            } => {
                self.preprocessing_chapters.remove(&chapter_id);
                self.preprocessing_progress.remove(&chapter_id);
                self.set_message(format!("Chapter {} ready", chapter_number));

                // Auto-open if we were waiting on this chapter
                if self.pending_open_chapter == Some(chapter_id) {
                    self.pending_open_chapter = None;
                    self.previous_screen = Some(Screen::ChapterSelect { source_id });
                    if let Err(e) = self.load_text(text_id) {
                        self.set_message(format!("Error loading: {}", e));
                    }
                    return;
                }

                // Refresh chapter select and top up preprocessing queue
                if let Screen::ChapterSelect { source_id: sid } = &self.screen {
                    if *sid == source_id {
                        let _ = self.load_chapter_select(source_id);
                        self.start_preprocessing();
                    }
                }
            }
            ImportEvent::Failed {
                chapter_id,
                chapter_number,
                error,
                ..
            } => {
                self.preprocessing_chapters.remove(&chapter_id);
                self.preprocessing_progress.remove(&chapter_id);
                self.set_message(format!("Chapter {} failed: {}", chapter_number, error));
            }
            ImportEvent::Cancelled { chapter_id, .. } => {
                self.preprocessing_chapters.remove(&chapter_id);
                self.preprocessing_progress.remove(&chapter_id);
            }
            ImportEvent::ChaptersPageLoaded {
                source_id,
                page,
                total_so_far,
                has_next,
            } => {
                // Refresh chapter list if we're viewing this source
                if let Screen::ChapterSelect { source_id: sid } = &mut self.screen {
                    if *sid == source_id || *sid == -1 {
                        // Update placeholder sentinel to the real source_id
                        *sid = source_id;
                        // Reload chapters from DB without triggering auto-refresh
                        let _ = self.refresh_chapter_list(source_id);
                        // Keep loading state if more pages are coming
                        if let Some(cs) = self.chapter_select_state.as_mut() {
                            cs.loading = has_next;
                        }
                    }
                }
                if has_next {
                    self.set_message(format!(
                        "Loading chapters... page {}, {} so far",
                        page, total_so_far
                    ));
                }
            }
            ImportEvent::NovelInfoLoaded { source_id, title } => {
                self.set_message(format!("Loaded: {}", title));
                // Mark loading as done and do a final refresh
                if let Screen::ChapterSelect { source_id: sid } = &mut self.screen {
                    if *sid == source_id || *sid == -1 {
                        // Update placeholder sentinel to the real source_id
                        *sid = source_id;
                        // Use refresh_chapter_list to avoid spawning duplicate
                        // refresh threads — the initial fetch just completed
                        let _ = self.refresh_chapter_list(source_id);
                        if let Some(cs) = self.chapter_select_state.as_mut() {
                            cs.loading = false;
                        }
                        self.start_preprocessing();
                    }
                }
            }
            ImportEvent::NovelInfoFailed { ncode, error } => {
                self.set_message(format!("Failed to load {}: {}", ncode, error));
            }
            ImportEvent::TextImportStarted { label } => {
                self.pending_imports.push(label);
            }
            ImportEvent::TextImportCompleted { title } => {
                // Remove the first pending import (FIFO)
                if !self.pending_imports.is_empty() {
                    self.pending_imports.remove(0);
                }
                self.set_message(format!("Imported: {}", title));
                // Refresh whichever list screen we're on
                match &self.screen {
                    Screen::Library => {
                        let _ = self.refresh_library();
                    }
                    Screen::Home => {
                        let _ = self.refresh_home();
                    }
                    _ => {}
                }
            }
            ImportEvent::TextImportFailed { label, error } => {
                // Remove matching pending import
                if let Some(pos) = self.pending_imports.iter().position(|l| *l == label) {
                    self.pending_imports.remove(pos);
                }
                self.set_message(format!("Import failed ({}): {}", label, error));
            }
        }
    }

    pub fn open_db(&self) -> Result<Connection> {
        crate::db::connection::open_or_create(&self.db_path)
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
    }

    /// Clear expired messages (older than 3 seconds) and advance tick counter.
    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        if let Some((_, when)) = &self.message {
            if when.elapsed().as_secs() >= 3 {
                self.message = None;
            }
        }
    }

    /// Get the current spinner character (Braille animation).
    pub fn spinner_char(&self) -> char {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        // Tick is ~60ms, so divide by 3 for ~180ms per frame (good visual speed)
        SPINNER[(self.tick_count as usize / 3) % SPINNER.len()]
    }

    /// Refresh the home screen state.
    pub fn refresh_home(&mut self) -> Result<()> {
        let conn = self.open_db()?;
        let recent = models::list_recent_texts(&conn, 15)?;
        let mut stats = HashMap::new();
        for t in &recent {
            if let Ok(s) = models::get_text_stats(&conn, t.id) {
                stats.insert(t.id, s);
            }
        }
        // Use the SRS engine's filtered count to match what the review session
        // actually shows (accounts for sentence cloze cards with Known vocabulary
        // being filtered out, and card type filter settings).
        let due_card_counts = if let Some(ref srs) = self.srs_engine {
            let cards = srs
                .get_due_cards(
                    &conn,
                    9999,
                    self.config.srs.review_word_cards,
                    self.config.srs.review_sentence_cloze_cards,
                    self.config.srs.review_sentence_full_cards,
                    self.config.srs.new_cards_per_day,
                )
                .unwrap_or_default();
            let new_count = cards.iter().filter(|c| c.state == "new").count();
            (cards.len(), new_count)
        } else {
            models::get_due_card_counts(&conn).unwrap_or((0, 0))
        };
        let prev = self.home_state.as_ref();
        let show_finished = prev.map(|s| s.show_finished).unwrap_or(false);
        let selected = prev
            .map(|s| s.selected.min(recent.len().saturating_sub(1)))
            .unwrap_or(0);
        self.home_state = Some(HomeState {
            recent_texts: recent,
            recent_stats: stats,
            selected,
            show_finished,
            due_card_counts,
        });
        Ok(())
    }

    /// Refresh the library list: standalone texts + grouped web sources.
    pub fn refresh_library(&mut self) -> Result<()> {
        let conn = self.open_db()?;

        // Preserve current sort/filter from existing state
        let (sort, filter_source) = self
            .library_state
            .as_ref()
            .map(|s| (s.sort, s.filter_source.clone()))
            .unwrap_or((LibrarySort::DateDesc, None));

        // Get standalone texts (not belonging to any web_source)
        let standalone_texts = models::list_standalone_texts(&conn)?;
        let web_sources = models::list_web_sources(&conn)?;

        // Build unified items list
        let mut items: Vec<LibraryItem> = Vec::new();

        for t in standalone_texts {
            if let Some(ref src) = filter_source {
                if t.source_type != *src {
                    continue;
                }
            }
            items.push(LibraryItem::Text(t));
        }

        for ws in &web_sources {
            if let Some(ref src) = filter_source {
                if ws.source_type != *src {
                    continue;
                }
            }
            items.push(LibraryItem::Source(ws.clone()));
        }

        // Collect unique source types
        let mut source_types: Vec<String> =
            items.iter().map(|i| i.source_type().to_string()).collect();
        source_types.sort();
        source_types.dedup();

        // Load per-text stats for standalone texts
        let mut stats = HashMap::new();
        for item in &items {
            if let LibraryItem::Text(t) = item {
                if let Ok(s) = models::get_text_stats(&conn, t.id) {
                    stats.insert(t.id, s);
                }
            }
        }

        // Load chapter counts for web sources
        let mut source_chapter_counts = HashMap::new();
        for ws in &web_sources {
            if let Ok(counts) = models::get_source_chapter_counts(&conn, ws.id) {
                source_chapter_counts.insert(ws.id, counts);
            }
        }

        // Apply sort
        match sort {
            LibrarySort::DateDesc => items.sort_by(|a, b| b.created_at().cmp(a.created_at())),
            LibrarySort::DateAsc => items.sort_by(|a, b| a.created_at().cmp(b.created_at())),
            LibrarySort::TitleAsc => items.sort_by(|a, b| a.title().cmp(b.title())),
            LibrarySort::Completion => {
                // Sort by completion for texts, sources go at the end
                items.sort_by(|a, b| {
                    let pct_a = match a {
                        LibraryItem::Text(t) => stats
                            .get(&t.id)
                            .map(|s| {
                                if s.unique_vocab == 0 {
                                    0.0
                                } else {
                                    s.known_count as f64 / s.unique_vocab as f64
                                }
                            })
                            .unwrap_or(0.0),
                        _ => 0.0,
                    };
                    let pct_b = match b {
                        LibraryItem::Text(t) => stats
                            .get(&t.id)
                            .map(|s| {
                                if s.unique_vocab == 0 {
                                    0.0
                                } else {
                                    s.known_count as f64 / s.unique_vocab as f64
                                }
                            })
                            .unwrap_or(0.0),
                        _ => 0.0,
                    };
                    pct_b
                        .partial_cmp(&pct_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        let selected = self
            .library_state
            .as_ref()
            .map(|s| s.selected.min(items.len().saturating_sub(1)))
            .unwrap_or(0);

        self.library_state = Some(LibraryState {
            items,
            stats,
            source_chapter_counts,
            selected,
            sort,
            filter_source,
            source_types,
        });
        Ok(())
    }

    /// Cycle the library sort mode.
    pub fn cycle_library_sort(&mut self) -> Result<()> {
        if let Some(ref mut lib) = self.library_state {
            lib.sort = lib.sort.next();
        }
        self.refresh_library()
    }

    /// Cycle the library source type filter.
    pub fn cycle_library_filter(&mut self) -> Result<()> {
        if let Some(ref mut lib) = self.library_state {
            let types = &lib.source_types;
            if types.is_empty() {
                return Ok(());
            }
            lib.filter_source = match &lib.filter_source {
                None => Some(types[0].clone()),
                Some(current) => {
                    let idx = types.iter().position(|t| t == current).unwrap_or(0);
                    if idx + 1 >= types.len() {
                        None // Cycle back to "all"
                    } else {
                        Some(types[idx + 1].clone())
                    }
                }
            };
        }
        self.refresh_library()
    }

    /// Load a text into the reader by text_id.
    pub fn load_text(&mut self, text_id: i64) -> Result<()> {
        let conn = self.open_db()?;

        let text = models::get_text_by_id(&conn, text_id)?
            .ok_or_else(|| anyhow::anyhow!("Text not found: {}", text_id))?;

        let db_paragraphs = models::list_paragraphs_by_text(&conn, text_id)?;

        let mut paragraphs = Vec::new();
        for p in &db_paragraphs {
            let tokens = models::list_tokens_by_paragraph(&conn, p.id)?;
            paragraphs.push(ParagraphData {
                id: p.id,
                position: p.position,
                content: p.content.clone(),
                db_tokens: tokens,
            });
        }

        // Build vocabulary cache
        let mut vocabulary_cache = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT * FROM vocabulary")?;
            let rows = stmt.query_map([], Vocabulary::from_row)?;
            for row in rows {
                if let Ok(v) = row {
                    vocabulary_cache.insert((v.base_form.clone(), v.reading.clone()), v);
                }
            }
        }

        // Build gloss cache: batch-lookup all unique base_forms at once
        let mut gloss_cache = HashMap::new();
        {
            // Collect unique non-trivial base_forms
            let mut base_forms: Vec<String> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for para in &paragraphs {
                for tok in &para.db_tokens {
                    if !is_trivial_pos(&tok.pos, &tok.surface) && seen.insert(tok.base_form.clone())
                    {
                        base_forms.push(tok.base_form.clone());
                    }
                }
            }

            // Batch lookup: query kanji index and reading index in bulk
            // Use a prepared statement to avoid repeated parsing
            let mut kanji_stmt = conn.prepare(
                "SELECT k.kanji_element, e.json_blob FROM jmdict_kanji k JOIN jmdict_entries e ON k.entry_id = e.ent_seq WHERE k.kanji_element = ?1 LIMIT 1"
            )?;
            let mut reading_stmt = conn.prepare(
                "SELECT r.reading_element, e.json_blob FROM jmdict_readings r JOIN jmdict_entries e ON r.entry_id = e.ent_seq WHERE r.reading_element = ?1 LIMIT 1"
            )?;

            for bf in &base_forms {
                // Try kanji lookup first, then reading fallback
                let gloss = kanji_stmt
                    .query_row(rusqlite::params![bf], |row| row.get::<_, String>(1))
                    .ok()
                    .or_else(|| {
                        reading_stmt
                            .query_row(rusqlite::params![bf], |row| row.get::<_, String>(1))
                            .ok()
                    })
                    .and_then(|json| {
                        serde_json::from_str::<dictionary::DictEntry>(&json)
                            .ok()
                            .map(|e| e.short_gloss())
                    })
                    .unwrap_or_default();
                gloss_cache.insert(bf.clone(), gloss);
            }
        }

        let mut sentences = build_sentences(&paragraphs, &vocabulary_cache, &gloss_cache);

        // Detect MWE matches (JMdict + user expressions) and apply to tokens
        let user_expressions = models::list_user_expressions(&conn).unwrap_or_default();
        let mwe_matches = detect_all_mwe_matches(&conn, &sentences, &user_expressions);
        for (sent_idx, matches) in mwe_matches.iter().enumerate() {
            apply_mwe_matches(&mut sentences[sent_idx].tokens, matches);
        }

        // Restore saved reading progress
        let saved_index = models::get_reading_progress(&conn, text_id).unwrap_or(0);
        let sentence_index = if saved_index < sentences.len() {
            saved_index
        } else {
            0
        };

        // Load sentence translations
        let mut sentence_translations = HashMap::new();
        if let Ok(trans) = models::get_sentence_translations(&conn, text_id) {
            for t in trans {
                sentence_translations.insert(t.sentence_index as usize, t);
            }
        }

        // Update total_sentences and touch last_read_at
        let _ = models::update_total_sentences(&conn, text_id, sentences.len());
        let _ = models::touch_last_read(&conn, text_id);

        self.reader_state = Some(ReaderState {
            text_id,
            text_title: text.title.clone(),
            paragraphs,
            sentences,
            sentence_index,
            word_index: None,
            vocabulary_cache,
            gloss_cache,
            scroll_offset: 0,
            sidebar_scroll: 0,
            autopromote_enabled: true,
            autopromote_history: Vec::new(),
            show_all_readings: false,
            show_known_in_sidebar: false,
            mwe_matches,
            expression_mark: None,
            sentence_translations,
        });

        self.previous_screen = Some(self.screen.clone());
        self.screen = Screen::Reader;
        self.set_message(format!("Loaded: {}", text.title));
        Ok(())
    }

    /// Refresh token display after vocabulary changes.
    pub fn refresh_reader_display(&mut self) -> Result<()> {
        if let Some(ref mut state) = self.reader_state {
            state.sentences = build_sentences(
                &state.paragraphs,
                &state.vocabulary_cache,
                &state.gloss_cache,
            );
            // Re-apply cached MWE matches (no DB access needed)
            for (sent_idx, matches) in state.mwe_matches.iter().enumerate() {
                if sent_idx < state.sentences.len() {
                    apply_mwe_matches(&mut state.sentences[sent_idx].tokens, matches);
                }
            }
        }
        Ok(())
    }

    /// Update vocabulary status for the currently selected word.
    pub fn set_word_status(&mut self, status: VocabularyStatus) -> Result<()> {
        let (base_form, reading) = {
            let state = match self.reader_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            let word_idx = match state.word_index {
                Some(i) => i,
                None => {
                    self.set_message("No word selected — use ←/→ to select a word first");
                    return Ok(());
                }
            };
            let sentence = &state.sentences[state.sentence_index];
            if word_idx >= sentence.tokens.len() {
                return Ok(());
            }
            let token = &sentence.tokens[word_idx];
            if token.is_trivial {
                self.set_message("Cannot set status on punctuation/whitespace");
                return Ok(());
            }
            (token.base_form.clone(), token.reading.clone())
        };

        let conn = self.open_db()?;
        let vid = models::upsert_vocabulary(&conn, &base_form, &reading, "")?;
        models::update_vocabulary_status(&conn, vid, status)?;

        // Auto-create or retire SRS cards based on new status
        if let Some(ref srs) = self.srs_engine {
            if let Err(e) =
                srs.on_status_change(&conn, vid, status, &self.config.srs.default_answer_mode)
            {
                // Log but don't fail — SRS is secondary to vocabulary tracking
                self.set_message(format!("SRS card error: {}", e));
            }
        }

        // Update cache & patch affected tokens in-place (no full rebuild)
        if let Some(ref mut state) = self.reader_state {
            if let Some(vocab) = models::get_vocabulary_by_id(&conn, vid)? {
                state
                    .vocabulary_cache
                    .insert((base_form.clone(), reading.clone()), vocab);
            }
            // Patch all tokens matching this base_form+reading, and propagate
            // status to all group members (auxiliaries) in the same group.
            for sentence in &mut state.sentences {
                // First pass: update the head tokens that match
                let mut affected_groups: Vec<usize> = Vec::new();
                for token in sentence.tokens.iter_mut() {
                    if token.base_form == base_form && token.reading == reading {
                        token.vocabulary_status = status;
                        if let Some(gid) = token.group_id {
                            affected_groups.push(gid);
                        }
                    }
                }
                // Second pass: propagate to group members
                if !affected_groups.is_empty() {
                    for token in sentence.tokens.iter_mut() {
                        if let Some(gid) = token.group_id {
                            if affected_groups.contains(&gid) {
                                token.vocabulary_status = status;
                            }
                        }
                    }
                }
            }
        }

        let status_name = match status {
            VocabularyStatus::Ignored => "Ignored",
            VocabularyStatus::New => "New",
            VocabularyStatus::Learning1 => "Learning 1",
            VocabularyStatus::Learning2 => "Learning 2",
            VocabularyStatus::Learning3 => "Learning 3",
            VocabularyStatus::Learning4 => "Learning 4",
            VocabularyStatus::Known => "Known",
        };
        self.set_message(format!("{} → {}", base_form, status_name));
        Ok(())
    }

    /// Autopromote all New words in the given sentence to Known.
    /// Called when advancing past a sentence. Returns the number of words promoted.
    pub fn autopromote_sentence(&mut self, sentence_index: usize) -> Result<usize> {
        let state = match self.reader_state.as_ref() {
            Some(s) => s,
            None => return Ok(0),
        };

        if !state.autopromote_enabled {
            return Ok(0);
        }

        if sentence_index >= state.sentences.len() {
            return Ok(0);
        }

        // Collect New words from the departing sentence (deduplicated by base_form+reading).
        // Only promote navigable tokens (skip trivial + non-head group members).
        let mut seen = std::collections::HashSet::new();
        let mut to_promote: Vec<(String, String)> = Vec::new();
        for token in &state.sentences[sentence_index].tokens {
            if !token.is_navigable() {
                continue;
            }
            if token.vocabulary_status != VocabularyStatus::New {
                continue;
            }
            let key = (token.base_form.clone(), token.reading.clone());
            if seen.insert(key.clone()) {
                to_promote.push(key);
            }
        }

        if to_promote.is_empty() {
            return Ok(0);
        }

        let conn = self.open_db()?;
        let mut batch_words: Vec<(String, String, i64)> = Vec::new();

        for (base_form, reading) in &to_promote {
            let vid = models::upsert_vocabulary(&conn, base_form, reading, "")?;
            // Only promote if still New in DB (may have been changed by manual action)
            if let Some(vocab) = models::get_vocabulary_by_id(&conn, vid)? {
                if vocab.status == VocabularyStatus::New {
                    models::update_vocabulary_status(&conn, vid, VocabularyStatus::Known)?;
                    batch_words.push((base_form.clone(), reading.clone(), vid));
                }
            }
        }

        let count = batch_words.len();

        if count > 0 {
            // Update in-memory cache and patch all tokens
            if let Some(ref mut state) = self.reader_state {
                for (base_form, reading, vid) in &batch_words {
                    if let Some(vocab) = models::get_vocabulary_by_id(&conn, *vid)? {
                        state
                            .vocabulary_cache
                            .insert((base_form.clone(), reading.clone()), vocab);
                    }
                    for sentence in &mut state.sentences {
                        let mut affected_groups: Vec<usize> = Vec::new();
                        for token in sentence.tokens.iter_mut() {
                            if token.base_form == *base_form && token.reading == *reading {
                                token.vocabulary_status = VocabularyStatus::Known;
                                if let Some(gid) = token.group_id {
                                    affected_groups.push(gid);
                                }
                            }
                        }
                        // Propagate to group members
                        if !affected_groups.is_empty() {
                            for token in sentence.tokens.iter_mut() {
                                if let Some(gid) = token.group_id {
                                    if affected_groups.contains(&gid) {
                                        token.vocabulary_status = VocabularyStatus::Known;
                                    }
                                }
                            }
                        }
                    }
                }

                // Push onto undo stack
                state.autopromote_history.push(AutopromotionBatch {
                    sentence_index,
                    words: batch_words,
                });
            }
        }

        Ok(count)
    }

    /// Undo the most recent autopromotion batch, reverting words to New.
    pub fn undo_last_autopromote(&mut self) -> Result<()> {
        let batch = match self.reader_state.as_mut() {
            Some(state) => match state.autopromote_history.pop() {
                Some(b) => b,
                None => {
                    self.set_message("Nothing to undo");
                    return Ok(());
                }
            },
            None => return Ok(()),
        };

        let conn = self.open_db()?;

        for (base_form, reading, vid) in &batch.words {
            models::update_vocabulary_status(&conn, *vid, VocabularyStatus::New)?;

            // Update in-memory cache and patch all tokens
            if let Some(ref mut state) = self.reader_state {
                if let Some(vocab) = models::get_vocabulary_by_id(&conn, *vid)? {
                    state
                        .vocabulary_cache
                        .insert((base_form.clone(), reading.clone()), vocab);
                }
                for sentence in &mut state.sentences {
                    for token in &mut sentence.tokens {
                        if token.base_form == *base_form && token.reading == *reading {
                            token.vocabulary_status = VocabularyStatus::New;
                        }
                    }
                }
            }
        }

        self.set_message(format!(
            "Undo: {} words reverted to New (sentence {})",
            batch.words.len(),
            batch.sentence_index + 1,
        ));
        Ok(())
    }

    /// Open word detail popup for the currently selected word.
    pub fn open_word_detail(&mut self) -> Result<()> {
        let (base_form, reading, notes, vocab_id, mwe_surface, mwe_gloss) = {
            let state = match self.reader_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            let word_idx = match state.word_index {
                Some(i) => i,
                None => {
                    self.set_message("No word selected");
                    return Ok(());
                }
            };
            let sentence = &state.sentences[state.sentence_index];
            if word_idx >= sentence.tokens.len() {
                return Ok(());
            }
            let token = &sentence.tokens[word_idx];
            if token.is_trivial {
                self.set_message("No dictionary entry for punctuation");
                return Ok(());
            }

            // Check if this token is an MWE group head
            let (mwe_surface, mwe_gloss) =
                if token.is_group_head && !token.mwe_gloss.is_empty() && token.group_id.is_some() {
                    // Reconstruct the full MWE surface from all group members
                    let gid = token.group_id.unwrap();
                    let surface: String = sentence
                        .tokens
                        .iter()
                        .filter(|t| t.group_id == Some(gid))
                        .map(|t| t.surface.as_str())
                        .collect();
                    (Some(surface), Some(token.mwe_gloss.clone()))
                } else {
                    (None, None)
                };

            let key = (token.base_form.clone(), token.reading.clone());
            let vocab = state.vocabulary_cache.get(&key);
            let notes = vocab.and_then(|v| v.notes.clone());
            let vocab_id = vocab.map(|v| v.id);
            (
                token.base_form.clone(),
                token.reading.clone(),
                notes,
                vocab_id,
                mwe_surface,
                mwe_gloss,
            )
        };

        let conn = self.open_db()?;

        // For MWE group heads, look up the full expression surface in JMdict;
        // fall back to the head's base_form if no expression-level entry is found.
        let (display_form, entries) = if let Some(ref mwe_surf) = mwe_surface {
            let mwe_entries = dictionary::lookup(&conn, mwe_surf, None)?;
            if mwe_entries.is_empty() {
                // No JMdict entry for the full expression — fall back to head word
                (
                    base_form.clone(),
                    dictionary::lookup(&conn, &base_form, None)?,
                )
            } else {
                (mwe_surf.clone(), mwe_entries)
            }
        } else {
            (
                base_form.clone(),
                dictionary::lookup(&conn, &base_form, None)?,
            )
        };

        let conjugations = if let Some(vid) = vocab_id {
            let mut stmt = conn.prepare(
                "SELECT surface, encounter_count FROM conjugation_encounters WHERE vocabulary_id = ?1 ORDER BY encounter_count DESC"
            )?;
            let rows = stmt.query_map(rusqlite::params![vid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
            })?;
            rows.filter_map(|r| r.ok()).collect()
        } else {
            vec![]
        };

        // For MWE expressions where JMdict had no entry, show the gloss from
        // the MWE match as a synthetic note so the user still sees the meaning.
        let effective_notes = if entries.is_empty() {
            if let Some(ref gloss) = mwe_gloss {
                Some(format!(
                    "{}{}",
                    gloss,
                    notes.map(|n| format!("\n{}", n)).unwrap_or_default()
                ))
            } else {
                notes
            }
        } else {
            notes
        };

        self.popup = Some(PopupState::WordDetail {
            base_form: display_form,
            reading,
            entries,
            conjugations,
            notes: effective_notes,
            scroll: 0,
        });

        Ok(())
    }

    /// Open note editor for the currently selected word.
    pub fn open_note_editor(&mut self) -> Result<()> {
        let (vocab_id, existing_notes) = {
            let state = match self.reader_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            let word_idx = match state.word_index {
                Some(i) => i,
                None => {
                    self.set_message("No word selected");
                    return Ok(());
                }
            };
            let sentence = &state.sentences[state.sentence_index];
            if word_idx >= sentence.tokens.len() {
                return Ok(());
            }
            let token = &sentence.tokens[word_idx];
            let key = (token.base_form.clone(), token.reading.clone());
            match state.vocabulary_cache.get(&key) {
                Some(v) => (v.id, v.notes.clone().unwrap_or_default()),
                None => {
                    self.set_message("Word not in vocabulary yet");
                    return Ok(());
                }
            }
        };

        self.popup = Some(PopupState::NoteEditor {
            vocabulary_id: vocab_id,
            text: existing_notes,
        });
        Ok(())
    }

    /// Delete a text and refresh the library.
    pub fn delete_text(&mut self, text_id: i64) -> Result<()> {
        let conn = self.open_db()?;
        models::delete_text(&conn, text_id)?;
        self.refresh_library()?;
        Ok(())
    }

    /// Delete a web source (and all its chapters/texts) and refresh the library.
    pub fn delete_source(&mut self, source_id: i64) -> Result<()> {
        // Cancel any in-flight preprocessing for this source
        if let Some(ref importer) = self.background_importer {
            if let Some(ref state) = self.chapter_select_state {
                if state.source.id == source_id {
                    for ch in &state.chapters {
                        importer.cancel_chapter(ch.id);
                        self.preprocessing_chapters.remove(&ch.id);
                    }
                }
            }
        }
        let conn = self.open_db()?;
        models::delete_web_source(&conn, source_id)?;
        self.refresh_library()?;
        Ok(())
    }

    /// Import from clipboard (TUI context).
    pub fn import_clipboard(&mut self) -> Result<String> {
        let conn = self.open_db()?;
        let (_text_id, title) = crate::import::clipboard::import_clipboard_quiet(&conn)?;
        self.refresh_library()?;
        Ok(title)
    }

    /// Import from URL (TUI context).
    pub fn import_url(&mut self, url: &str) -> Result<String> {
        let conn = self.open_db()?;
        let (_text_id, title) = crate::import::web::import_url_quiet(url, &conn)?;
        self.refresh_library()?;
        Ok(title)
    }

    /// Import a file from a path (TUI context). Auto-detects format.
    pub fn import_file_path(&mut self, path_str: &str) -> Result<String> {
        let path = std::path::PathBuf::from(path_str);
        if !path.exists() {
            anyhow::bail!("File not found: {}", path_str);
        }
        let conn = self.open_db()?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let result = match ext.as_str() {
            "srt" | "ass" | "ssa" => {
                let (_, title) = crate::import::subtitle::import_subtitle_quiet(&path, &conn)?;
                title
            }
            "epub" => {
                let chapters = crate::import::epub::import_epub_quiet(&path, &conn)?;
                format!("{} chapters imported", chapters.len())
            }
            _ => {
                let text_id = crate::import::text::import_text_quiet(
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Untitled"),
                    &std::fs::read_to_string(&path)?,
                    "text",
                    None,
                    &conn,
                )?;
                let text = models::get_text_by_id(&conn, text_id)?
                    .map(|t| t.title)
                    .unwrap_or_default();
                text
            }
        };
        self.refresh_library()?;
        Ok(result)
    }

    /// Start loading a Syosetu novel by ncode in the background.
    /// Goes to chapter select immediately with a loading state.
    pub fn load_syosetu(&mut self, ncode_input: &str) -> Result<()> {
        let ncode = crate::import::syosetu::parse_ncode(ncode_input)?;

        // Check if already in DB
        let conn = self.open_db()?;
        if let Ok(Some(ws)) = models::find_web_source(&conn, "syosetu", &ncode) {
            // Already fetched — go directly to chapter select
            return self.load_chapter_select(ws.id);
        }
        drop(conn);

        // Set up a loading chapter select screen immediately
        let placeholder_source = models::WebSource {
            id: -1, // sentinel — will be replaced when data arrives
            source_type: "syosetu".to_string(),
            external_id: ncode.clone(),
            title: format!("Loading {}...", ncode),
            metadata_json: String::new(),
            last_synced: String::new(),
        };
        self.chapter_select_state = Some(ChapterSelectState {
            source: placeholder_source,
            chapters: vec![],
            selected: 0,
            page_start: 0,
            page_size: chapter_page_size_for_terminal(),
            total_chapters: 0,
            total_imported: 0,
            total_skipped: 0,
            loading: true,
            chapter_read_states: HashMap::new(),
        });
        self.screen = Screen::ChapterSelect { source_id: -1 };

        // Fetch in background
        if let Some(ref importer) = self.background_importer {
            importer.fetch_novel_info(ncode, self.db_path.clone());
        } else {
            anyhow::bail!("Background importer not initialized");
        }
        Ok(())
    }

    /// Reload just the chapter list data from DB without triggering
    /// auto-refresh or preprocessing. Used during incremental page loading
    /// to avoid spawning duplicate background threads.
    fn refresh_chapter_list(&mut self, source_id: i64) -> Result<()> {
        let conn = self.open_db()?;
        let source = models::get_web_source_by_id(&conn, source_id)?
            .ok_or_else(|| anyhow::anyhow!("Source not found: {}", source_id))?;
        let chapters = models::list_chapters_by_source(&conn, source_id)?;
        let (total, imported, skipped) = models::get_source_chapter_counts(&conn, source_id)?;

        // Build reading state map for chapters with text_ids
        let mut chapter_read_states = HashMap::new();
        for ch in &chapters {
            if ch.is_skipped {
                continue;
            }
            if let Some(text_id) = ch.text_id {
                let text = models::get_text_by_id(&conn, text_id)?;
                let state = match text {
                    Some(t) => {
                        if t.total_sentences == 0 {
                            ChapterReadState::Unread
                        } else if t.last_sentence_index >= t.total_sentences - 1 {
                            ChapterReadState::Finished
                        } else if t.last_sentence_index > 0 {
                            ChapterReadState::InProgress
                        } else {
                            ChapterReadState::Unread
                        }
                    }
                    None => ChapterReadState::NotImported,
                };
                chapter_read_states.insert(ch.id, state);
            } else {
                chapter_read_states.insert(ch.id, ChapterReadState::NotImported);
            }
        }

        let page_size = chapter_page_size_for_terminal();
        let selected = self
            .chapter_select_state
            .as_ref()
            .filter(|s| s.source.id == source_id)
            .map(|s| s.selected.min(chapters.len().saturating_sub(1)))
            .unwrap_or(0);
        let page_start = (selected / page_size) * page_size;

        self.chapter_select_state = Some(ChapterSelectState {
            source,
            chapters,
            selected,
            page_start,
            page_size,
            total_chapters: total,
            total_imported: imported,
            total_skipped: skipped,
            loading: false,
            chapter_read_states,
        });
        self.screen = Screen::ChapterSelect { source_id };
        Ok(())
    }

    /// Load the chapter select screen for any multi-chapter source.
    pub fn load_chapter_select(&mut self, source_id: i64) -> Result<()> {
        let conn = self.open_db()?;
        let source = models::get_web_source_by_id(&conn, source_id)?
            .ok_or_else(|| anyhow::anyhow!("Source not found: {}", source_id))?;
        let chapters = models::list_chapters_by_source(&conn, source_id)?;
        let (total, imported, skipped) = models::get_source_chapter_counts(&conn, source_id)?;

        // Build reading state map for chapters with text_ids
        let mut chapter_read_states = HashMap::new();
        for ch in &chapters {
            if ch.is_skipped {
                continue; // Skipped state is handled separately
            }
            if let Some(text_id) = ch.text_id {
                // Look up reading progress for this text
                let text = models::get_text_by_id(&conn, text_id)?;
                let state = match text {
                    Some(t) => {
                        if t.total_sentences == 0 {
                            ChapterReadState::Unread
                        } else if t.last_sentence_index >= t.total_sentences - 1 {
                            ChapterReadState::Finished
                        } else if t.last_sentence_index > 0 {
                            ChapterReadState::InProgress
                        } else {
                            ChapterReadState::Unread
                        }
                    }
                    None => ChapterReadState::NotImported,
                };
                chapter_read_states.insert(ch.id, state);
            } else {
                chapter_read_states.insert(ch.id, ChapterReadState::NotImported);
            }
        }

        let page_size = chapter_page_size_for_terminal();
        let selected = self
            .chapter_select_state
            .as_ref()
            .filter(|s| s.source.id == source_id)
            .map(|s| s.selected.min(chapters.len().saturating_sub(1)))
            .unwrap_or(0);
        // Approximate page_start: snap to a page boundary
        let page_start = (selected / page_size) * page_size;

        let is_syosetu = source.source_type == "syosetu";
        let ncode = source.external_id.clone();
        let chapter_count = chapters.len();

        self.chapter_select_state = Some(ChapterSelectState {
            source,
            chapters,
            selected,
            page_start,
            page_size,
            total_chapters: total,
            total_imported: imported,
            total_skipped: skipped,
            loading: false,
            chapter_read_states,
        });
        self.screen = Screen::ChapterSelect { source_id };

        // Start eager preprocessing
        self.start_preprocessing();

        // Auto-refresh: check for new chapters in the background for Syosetu sources
        if is_syosetu {
            if let Some(ref importer) = self.background_importer {
                importer.refresh_novel_chapters(
                    source_id,
                    ncode,
                    chapter_count,
                    self.db_path.clone(),
                );
            }
        }

        Ok(())
    }

    /// Save current reading progress to the database.
    pub fn save_reading_progress(&self) -> Result<()> {
        if let Some(ref state) = self.reader_state {
            let conn = self.open_db()?;
            models::save_reading_progress(&conn, state.text_id, state.sentence_index)?;
        }
        Ok(())
    }

    /// Try to advance to the next chapter in the same source.
    /// Returns Ok(true) if we navigated to a new chapter, Ok(false) if there is no next chapter.
    pub fn advance_to_next_chapter(&mut self) -> Result<bool> {
        let text_id = match self.reader_state {
            Some(ref s) => s.text_id,
            None => return Ok(false),
        };

        let conn = self.open_db()?;
        let info = models::find_next_chapter_for_text(&conn, text_id)?;
        drop(conn);

        let (_current, next, source_id) = match info {
            Some(v) => v,
            None => return Ok(false), // not a chapter-based text
        };

        let next_ch = match next {
            Some(ch) => ch,
            None => return Ok(false), // no next chapter
        };

        if let Some(next_text_id) = next_ch.text_id {
            // Already imported — open it directly
            self.previous_screen = Some(Screen::ChapterSelect { source_id });
            self.load_text(next_text_id)?;
            // Update chapter select selected index to point at the new chapter
            if let Some(ref mut cs) = self.chapter_select_state {
                if let Some(idx) = cs.chapters.iter().position(|c| c.id == next_ch.id) {
                    cs.selected = idx;
                }
            }
            self.set_message(format!(
                "Chapter {}: {}",
                next_ch.chapter_number, next_ch.title
            ));
            Ok(true)
        } else {
            // Not yet imported — queue for background import if syosetu
            let source_type = self
                .chapter_select_state
                .as_ref()
                .map(|s| s.source.source_type.clone());

            if source_type.as_deref() == Some("syosetu") {
                self.pending_open_chapter = Some(next_ch.id);
                if !self.preprocessing_chapters.contains(&next_ch.id) {
                    if let Some(ref mut importer) = self.background_importer {
                        if let Some(ref cs) = self.chapter_select_state {
                            importer.queue_single(
                                cs.source.id,
                                &cs.source.source_type,
                                &cs.source.external_id,
                                next_ch.id,
                                next_ch.chapter_number,
                                &self.db_path,
                            );
                        }
                    }
                    self.preprocessing_chapters.insert(next_ch.id);
                }
                self.set_message(format!(
                    "Importing chapter {}... will open when ready",
                    next_ch.chapter_number
                ));
                // Go to chapter select to wait
                self.screen = Screen::ChapterSelect { source_id };
                if let Some(ref mut cs) = self.chapter_select_state {
                    if let Some(idx) = cs.chapters.iter().position(|c| c.id == next_ch.id) {
                        cs.selected = idx;
                    }
                }
                let _ = self.load_chapter_select(source_id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }

    /// Go back from reader to the previous screen, saving progress.
    pub fn back_from_reader(&mut self) -> Result<()> {
        let _ = self.save_reading_progress();
        let target = self.previous_screen.take().unwrap_or(Screen::Home);
        self.screen = target.clone();
        match target {
            Screen::Library => {
                let _ = self.refresh_library();
            }
            Screen::Home => {
                let _ = self.refresh_home();
            }
            Screen::ChapterSelect { source_id } => {
                let _ = self.load_chapter_select(source_id);
            }
            _ => {}
        }
        Ok(())
    }

    /// Search the library by title.
    pub fn search_library(&mut self, query: &str) -> Result<()> {
        let conn = self.open_db()?;
        let texts = models::search_texts(&conn, query)?;
        let mut items: Vec<LibraryItem> = Vec::new();
        let mut stats = HashMap::new();
        for t in texts {
            if let Ok(s) = models::get_text_stats(&conn, t.id) {
                stats.insert(t.id, s);
            }
            items.push(LibraryItem::Text(t));
        }
        // Also search web sources by title
        let web_sources = models::list_web_sources(&conn)?;
        let query_lower = query.to_lowercase();
        let mut source_chapter_counts = HashMap::new();
        for ws in web_sources {
            if ws.title.to_lowercase().contains(&query_lower) {
                if let Ok(counts) = models::get_source_chapter_counts(&conn, ws.id) {
                    source_chapter_counts.insert(ws.id, counts);
                }
                items.push(LibraryItem::Source(ws));
            }
        }
        let source_types = self
            .library_state
            .as_ref()
            .map(|s| s.source_types.clone())
            .unwrap_or_default();
        self.library_state = Some(LibraryState {
            items,
            stats,
            source_chapter_counts,
            selected: 0,
            sort: LibrarySort::DateDesc,
            filter_source: None,
            source_types,
        });
        Ok(())
    }

    /// Save note from the note editor.
    pub fn save_note(&mut self) -> Result<()> {
        if let Some(PopupState::NoteEditor {
            vocabulary_id,
            ref text,
        }) = self.popup
        {
            let conn = self.open_db()?;
            let notes: Option<&str> = if text.is_empty() {
                None
            } else {
                Some(text.as_str())
            };
            conn.execute(
                "UPDATE vocabulary SET notes = ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![notes, vocabulary_id],
            )?;

            if let Some(vocab) = models::get_vocabulary_by_id(&conn, vocabulary_id)? {
                if let Some(ref mut state) = self.reader_state {
                    state
                        .vocabulary_cache
                        .insert((vocab.base_form.clone(), vocab.reading.clone()), vocab);
                }
            }
            self.set_message("Note saved");
        }
        self.popup = None;
        Ok(())
    }

    /// Begin saving an expression: capture the marked range, look up JMdict,
    /// then open the translation prompt popup so the user can confirm/edit the gloss.
    pub fn save_expression_mark(&mut self) -> Result<()> {
        let surface = {
            let state = match self.reader_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            let (start, end) = match state.expression_mark {
                Some(range) => range,
                None => return Ok(()),
            };
            let sentence = &state.sentences[state.sentence_index];
            sentence.tokens[start..=end]
                .iter()
                .map(|t| t.surface.as_str())
                .collect::<String>()
        };

        if surface.is_empty() {
            self.set_message("Empty expression");
            if let Some(ref mut state) = self.reader_state {
                state.expression_mark = None;
            }
            return Ok(());
        }

        // Clear expression mark immediately (visual range highlighting)
        if let Some(ref mut state) = self.reader_state {
            state.expression_mark = None;
        }

        let conn = self.open_db()?;

        // Look up gloss from JMdict if available — pre-fill for the user
        let (reading, gloss) = dictionary::lookup_mwe_info(&conn, &surface)
            .unwrap_or_else(|| (String::new(), String::new()));

        // Open translation prompt popup instead of saving immediately
        self.popup = Some(PopupState::ExpressionTranslation {
            surface,
            reading,
            gloss,
        });

        Ok(())
    }

    /// Save a user expression that was confirmed via the translation prompt popup.
    /// Called when the user presses Enter in the ExpressionTranslation popup.
    pub fn save_expression_with_translation(&mut self) -> Result<()> {
        let (surface, reading, gloss) = match self.popup {
            Some(PopupState::ExpressionTranslation {
                ref surface,
                ref reading,
                ref gloss,
                ..
            }) => (surface.clone(), reading.clone(), gloss.clone()),
            _ => return Ok(()),
        };
        self.popup = None;

        let conn = self.open_db()?;
        models::upsert_user_expression(&conn, &surface, &reading, &gloss)?;

        // Re-detect MWE matches for all sentences
        let user_expressions = models::list_user_expressions(&conn).unwrap_or_default();
        if let Some(ref mut state) = self.reader_state {
            state.mwe_matches = detect_all_mwe_matches(&conn, &state.sentences, &user_expressions);
            // Rebuild sentences and re-apply
            state.sentences = build_sentences(
                &state.paragraphs,
                &state.vocabulary_cache,
                &state.gloss_cache,
            );
            for (idx, matches) in state.mwe_matches.iter().enumerate() {
                if idx < state.sentences.len() {
                    apply_mwe_matches(&mut state.sentences[idx].tokens, matches);
                }
            }
        }

        self.set_message(format!("Expression saved: {}", surface));
        Ok(())
    }

    // ─── SRS / Review Methods ────────────────────────────────────────────

    /// Start a review session: load due cards and switch to Review screen.
    pub fn start_review_session(&mut self) -> Result<()> {
        let conn = self.open_db()?;
        let srs = match self.srs_engine.as_ref() {
            Some(s) => s,
            None => {
                self.set_message("SRS engine not initialized");
                return Ok(());
            }
        };

        let (total_due, total_new) = models::get_due_card_counts(&conn)?;
        let limit = if self.config.srs.max_reviews_per_session > 0 {
            self.config.srs.max_reviews_per_session as usize
        } else {
            200 // reasonable batch limit
        };

        let due_cards = srs.get_due_cards(
            &conn,
            limit,
            self.config.srs.review_word_cards,
            self.config.srs.review_sentence_cloze_cards,
            self.config.srs.review_sentence_full_cards,
            self.config.srs.new_cards_per_day,
        )?;

        // Build vocabulary cache for sentence context rendering
        let mut vocabulary_cache = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT * FROM vocabulary")?;
            let rows = stmt.query_map([], Vocabulary::from_row)?;
            for row in rows {
                if let Ok(v) = row {
                    vocabulary_cache.insert((v.base_form.clone(), v.reading.clone()), v);
                }
            }
        }

        // Build a gloss cache for sentence context tokens
        let mut gloss_cache: HashMap<String, String> = HashMap::new();

        // Build card data for each due card
        let user_expressions = models::list_user_expressions(&conn).unwrap_or_default();
        let mut queue: Vec<ReviewCardData> = Vec::new();
        for card in due_cards {
            let answer_mode = AnswerMode::from_str(&card.answer_mode);

            // Handle SentenceFull cards (no vocabulary, linked via sentence_translation_id)
            if answer_mode == AnswerMode::SentenceFull {
                if let Some(st_id) = card.sentence_translation_id {
                    if let Ok(Some(st)) = models::get_sentence_translation_by_id(&conn, st_id) {
                        queue.push(ReviewCardData {
                            card,
                            vocabulary: Vocabulary {
                                id: 0,
                                base_form: String::new(),
                                reading: String::new(),
                                pos: String::new(),
                                status: VocabularyStatus::New,
                                notes: None,
                                translation: None,
                                first_seen_at: String::new(),
                                updated_at: String::new(),
                            },
                            definitions: vec![],
                            sentence_tokens: vec![],
                            sentence_text: st.sentence_text.clone(),
                            answer_mode,
                            display_surface: String::new(),
                            display_reading: String::new(),
                            target_group_id: None,
                            sentence_translation_text: Some(st.translation.clone()),
                        });
                    }
                }
                continue;
            }

            let vocab_id = match card.vocabulary_id {
                Some(id) => id,
                None => continue,
            };
            let vocabulary = match models::get_vocabulary_by_id(&conn, vocab_id)? {
                Some(v) => v,
                None => continue,
            };

            let definitions =
                dictionary::lookup(&conn, &vocabulary.base_form, None).unwrap_or_default();

            // Get sentence context tokens
            let db_tokens = models::get_sentence_tokens_for_vocab(&conn, vocab_id)?;
            let sentence_text: String = db_tokens.iter().map(|t| t.surface.as_str()).collect();

            // Build TokenDisplay for sentence context
            let mut sentence_tokens: Vec<TokenDisplay> = db_tokens
                .iter()
                .map(|t| {
                    let key = (t.base_form.clone(), t.reading.clone());
                    let status = vocabulary_cache
                        .get(&key)
                        .map(|v| v.status)
                        .unwrap_or(VocabularyStatus::New);
                    let is_trivial =
                        is_trivial_pos(&t.pos, &t.surface) || status == VocabularyStatus::Ignored;

                    // Populate short_gloss from cache or JMdict lookup
                    let short_gloss = if !is_trivial {
                        if let Some(cached) = gloss_cache.get(&t.base_form) {
                            cached.clone()
                        } else {
                            let gloss = dictionary::lookup(&conn, &t.base_form, None)
                                .ok()
                                .and_then(|entries| entries.first().map(|e| e.short_gloss()))
                                .unwrap_or_default();
                            gloss_cache.insert(t.base_form.clone(), gloss.clone());
                            gloss
                        }
                    } else {
                        String::new()
                    };

                    TokenDisplay {
                        surface: t.surface.clone(),
                        base_form: t.base_form.clone(),
                        reading: t.reading.clone(),
                        surface_reading: t.surface_reading.clone(),
                        pos: t.pos.clone(),
                        vocabulary_status: status,
                        is_selected: false,
                        short_gloss,
                        conjugation_form: translate_conjugation_form(&t.conjugation_form),
                        conjugation_type: translate_conjugation_type(&t.conjugation_type),
                        is_trivial,
                        group_id: None,
                        is_group_head: false,
                        conjugation_desc: String::new(),
                        mwe_gloss: String::new(),
                    }
                })
                .collect();

            // Apply conjugation grouping (same logic as reader's build_sentences)
            apply_conjugation_groups(&mut sentence_tokens);

            // Detect and apply MWE matches
            let mwe_matches = detect_sentence_mwes(&conn, &sentence_tokens, &user_expressions, 12);
            apply_mwe_matches(&mut sentence_tokens, &mwe_matches);

            // Detect compound context: find the target word in sentence tokens
            // and aggregate group surface/reading if it belongs to a group
            let target_token = sentence_tokens
                .iter()
                .find(|t| t.base_form == vocabulary.base_form && t.reading == vocabulary.reading);

            let (display_surface, display_reading, target_group_id) =
                if let Some(token) = target_token {
                    if let Some(gid) = token.group_id {
                        // Compound word — aggregate all group members
                        let surface: String = sentence_tokens
                            .iter()
                            .filter(|t| t.group_id == Some(gid))
                            .map(|t| t.surface.as_str())
                            .collect();
                        let reading: String = sentence_tokens
                            .iter()
                            .filter(|t| t.group_id == Some(gid))
                            .map(|t| {
                                if !t.surface_reading.is_empty() {
                                    t.surface_reading.as_str()
                                } else if !t.reading.is_empty() {
                                    t.reading.as_str()
                                } else {
                                    t.surface.as_str()
                                }
                            })
                            .collect();
                        (surface, reading, Some(gid))
                    } else {
                        (
                            vocabulary.base_form.clone(),
                            vocabulary.reading.clone(),
                            None,
                        )
                    }
                } else {
                    (
                        vocabulary.base_form.clone(),
                        vocabulary.reading.clone(),
                        None,
                    )
                };

            // For MWE compounds, try looking up the full expression in JMdict
            let definitions =
                if target_group_id.is_some() && display_surface != vocabulary.base_form {
                    let mwe_defs =
                        dictionary::lookup(&conn, &display_surface, None).unwrap_or_default();
                    if !mwe_defs.is_empty() {
                        mwe_defs
                    } else {
                        definitions
                    }
                } else {
                    definitions
                };

            queue.push(ReviewCardData {
                card,
                vocabulary,
                definitions,
                sentence_tokens,
                sentence_text,
                answer_mode,
                display_surface,
                display_reading,
                target_group_id,
                sentence_translation_text: None,
            });
        }

        // Shuffle the queue so cards don't always appear in due_date order
        {
            use rand::seq::SliceRandom;
            let mut rng = rand::thread_rng();
            queue.shuffle(&mut rng);
        }

        let phase = if queue.is_empty() {
            ReviewPhase::SessionSummary
        } else {
            ReviewPhase::PreSession
        };

        self.review_state = Some(ReviewState {
            queue,
            current_index: 0,
            phase,
            typed_input: String::new(),
            card_shown_at: Instant::now(),
            context_word_index: None,
            total_reviewed: 0,
            correct_count: 0,
            total_due,
            total_new,
            vocabulary_cache,
            auto_rating: None,
            typed_result: None,
        });

        self.previous_screen = Some(self.screen.clone());
        self.screen = Screen::Review;
        Ok(())
    }

    /// Begin showing the first card (transition from PreSession to ShowFront).
    pub fn begin_review(&mut self) {
        let require_typed = self.config.srs.require_typed_input;
        if let Some(ref mut state) = self.review_state {
            if !state.queue.is_empty() {
                state.phase = ReviewPhase::ShowFront;
                state.card_shown_at = Instant::now();
                // For typed reading mode (or word cards when require_typed_input is on), go directly to typing
                if let Some(card_data) = state.queue.get(state.current_index) {
                    if card_data.answer_mode == AnswerMode::TypedReading
                        || (require_typed && card_data.card.card_type == "word")
                    {
                        state.phase = ReviewPhase::TypingAnswer;
                    }
                }
            }
        }
    }

    /// Reveal the answer (transition from ShowFront to ShowBack).
    pub fn reveal_answer(&mut self) {
        if let Some(ref mut state) = self.review_state {
            state.phase = ReviewPhase::ShowBack;
        }
    }

    /// Submit a typed answer (typed reading mode).
    /// Normalizes input: accepts romaji (converted to hiragana), katakana (converted to hiragana),
    /// hiragana, or kanji (compared against surface form). Uses fuzzy matching for typo tolerance.
    pub fn submit_typed_answer(&mut self) {
        if let Some(ref mut state) = self.review_state {
            if let Some(card_data) = state.queue.get(state.current_index) {
                let expected_reading = &card_data.display_reading;
                let expected_surface = &card_data.display_surface;
                let raw_input = state.typed_input.clone();

                // Normalize input: romaji→hiragana, katakana→hiragana
                let normalized = tokenizer::normalize_reading_input(&raw_input);

                // Check against both reading (hiragana) and surface form (kanji)
                let dist_reading = strsim::levenshtein(&normalized, expected_reading);
                let dist_surface = strsim::levenshtein(&normalized, expected_surface);

                // Also check raw input against surface (user might type kanji directly)
                let dist_raw_surface = strsim::levenshtein(&raw_input, expected_surface);

                // Use the best (smallest) distance
                let distance = dist_reading.min(dist_surface).min(dist_raw_surface);

                // Scale tolerance by expected length: allow ~1 error per 3 chars
                let expected_len = expected_reading.chars().count().max(1);
                let is_correct = distance == 0;

                // Auto-rate based on edit distance, scaled by word length
                let auto_rating = if distance == 0 {
                    Rating::Easy
                } else if distance == 1 {
                    Rating::Good
                } else if distance <= expected_len / 3 + 1 {
                    Rating::Hard
                } else {
                    Rating::Again
                };

                state.auto_rating = Some(auto_rating);
                state.typed_result = Some((normalized, expected_reading.clone(), is_correct));
                state.phase = ReviewPhase::ShowResult;
            }
        }
    }

    /// Rate the current card and advance to the next one.
    pub fn rate_card(&mut self, rating: Rating) -> Result<()> {
        let require_typed = self.config.srs.require_typed_input;
        let (card_id, elapsed_ms, typed_answer, answer_correct) = {
            let state = match self.review_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            let card_data = match state.queue.get(state.current_index) {
                Some(c) => c,
                None => return Ok(()),
            };
            let elapsed = state.card_shown_at.elapsed().as_millis() as u64;
            let correct = rating != Rating::Again;
            let typed = state
                .typed_result
                .as_ref()
                .map(|(input, _, _)| input.clone());
            (card_data.card.id, elapsed, typed, correct)
        };

        // Record the review via SRS engine
        let conn = self.open_db()?;
        if let Some(ref srs) = self.srs_engine {
            srs.record_review(
                &conn,
                card_id,
                rating,
                elapsed_ms,
                typed_answer.as_deref(),
                answer_correct,
            )?;
        }

        // Update session stats
        if let Some(ref mut state) = self.review_state {
            state.total_reviewed += 1;
            if answer_correct {
                state.correct_count += 1;
            }

            // Advance to next card
            state.current_index += 1;
            state.typed_input.clear();
            state.auto_rating = None;
            state.typed_result = None;
            state.context_word_index = None;

            if state.current_index >= state.queue.len() {
                // All cards done
                state.phase = ReviewPhase::SessionSummary;
            } else {
                // Show next card
                state.card_shown_at = Instant::now();
                let next_card = &state.queue[state.current_index];
                let next_mode = next_card.answer_mode.clone();
                let is_word_card = next_card.card.card_type == "word";
                if next_mode == AnswerMode::TypedReading || (require_typed && is_word_card) {
                    state.phase = ReviewPhase::TypingAnswer;
                } else {
                    state.phase = ReviewPhase::ShowFront;
                }
            }
        }

        Ok(())
    }

    /// Open word detail popup for the selected word in review sentence context.
    pub fn open_review_word_detail(&mut self) -> Result<()> {
        let (base_form, reading) = {
            let state = match self.review_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            let word_idx = match state.context_word_index {
                Some(i) => i,
                None => return Ok(()),
            };
            let card_data = match state.queue.get(state.current_index) {
                Some(c) => c,
                None => return Ok(()),
            };
            if word_idx >= card_data.sentence_tokens.len() {
                return Ok(());
            }
            let token = &card_data.sentence_tokens[word_idx];
            if token.is_trivial {
                self.set_message("No dictionary entry for punctuation");
                return Ok(());
            }
            (token.base_form.clone(), token.reading.clone())
        };

        let conn = self.open_db()?;
        let entries = dictionary::lookup(&conn, &base_form, None)?;

        let vocab_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM vocabulary WHERE base_form = ?1 AND reading = ?2",
                rusqlite::params![base_form, reading],
                |row| row.get(0),
            )
            .ok();

        let conjugations = if let Some(vid) = vocab_id {
            let mut stmt = conn.prepare(
                "SELECT surface, encounter_count FROM conjugation_encounters WHERE vocabulary_id = ?1 ORDER BY encounter_count DESC"
            )?;
            let rows = stmt.query_map(rusqlite::params![vid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
            })?;
            rows.filter_map(|r| r.ok()).collect()
        } else {
            vec![]
        };

        let notes = vocab_id
            .and_then(|vid| models::get_vocabulary_by_id(&conn, vid).ok().flatten())
            .and_then(|v| v.notes);

        self.popup = Some(PopupState::WordDetail {
            base_form,
            reading,
            entries,
            conjugations,
            notes,
            scroll: 0,
        });

        Ok(())
    }

    /// Copy the currently selected word (or its group surface) to the system clipboard.
    pub fn copy_word_to_clipboard(&mut self) -> Result<()> {
        let text = {
            let state = match self.reader_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            let wi = match state.word_index {
                Some(i) => i,
                None => {
                    self.set_message("Select a word first (←/→), then press 'c' to copy");
                    return Ok(());
                }
            };
            let sentence = &state.sentences[state.sentence_index];
            if wi >= sentence.tokens.len() {
                return Ok(());
            }
            let token = &sentence.tokens[wi];
            // If this token belongs to a group (conjugation or MWE), copy the whole group surface.
            if let Some(gid) = token.group_id {
                sentence
                    .tokens
                    .iter()
                    .filter(|t| t.group_id == Some(gid))
                    .map(|t| t.surface.as_str())
                    .collect::<String>()
            } else {
                token.surface.clone()
            }
        };

        self.set_clipboard(&text)?;
        self.set_message(format!("Copied: {}", text));
        Ok(())
    }

    /// Copy the full current sentence text to the system clipboard.
    pub fn copy_sentence_to_clipboard(&mut self) -> Result<()> {
        let text = {
            let state = match self.reader_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            if state.sentences.is_empty() {
                return Ok(());
            }
            let sentence = &state.sentences[state.sentence_index];
            sentence
                .tokens
                .iter()
                .map(|t| t.surface.as_str())
                .collect::<String>()
        };

        self.set_clipboard(&text)?;
        self.set_message(format!("Copied sentence: {}", text));
        Ok(())
    }

    /// Write text to the system clipboard using the persistent handle.
    fn set_clipboard(&mut self, text: &str) -> Result<()> {
        let cb = self.clipboard.as_mut().context("Clipboard not available")?;
        cb.set_text(text).context("Failed to write to clipboard")?;
        Ok(())
    }

    // ─── Word Translation ────────────────────────────────────────────────

    /// Open word translation editor for the currently selected word.
    pub fn open_translation_editor(&mut self) -> Result<()> {
        let (vocab_id, base_form, reading, existing_translation) = {
            let state = match self.reader_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            let word_idx = match state.word_index {
                Some(i) => i,
                None => {
                    self.set_message("No word selected");
                    return Ok(());
                }
            };
            let sentence = &state.sentences[state.sentence_index];
            if word_idx >= sentence.tokens.len() {
                return Ok(());
            }
            let token = &sentence.tokens[word_idx];
            if token.is_trivial {
                self.set_message("Cannot set translation on punctuation/whitespace");
                return Ok(());
            }
            let key = (token.base_form.clone(), token.reading.clone());
            match state.vocabulary_cache.get(&key) {
                Some(v) => (
                    v.id,
                    v.base_form.clone(),
                    v.reading.clone(),
                    v.translation.clone().unwrap_or_default(),
                ),
                None => {
                    // Word not in vocab yet — create it
                    let conn = self.open_db()?;
                    let vid = models::upsert_vocabulary(
                        &conn,
                        &token.base_form,
                        &token.reading,
                        &token.pos,
                    )?;
                    (
                        vid,
                        token.base_form.clone(),
                        token.reading.clone(),
                        String::new(),
                    )
                }
            }
        };

        self.popup = Some(PopupState::TranslationEditor {
            vocabulary_id: vocab_id,
            base_form,
            reading,
            text: existing_translation,
        });
        Ok(())
    }

    /// Save word translation from the translation editor popup.
    pub fn save_word_translation(&mut self) -> Result<()> {
        if let Some(PopupState::TranslationEditor {
            vocabulary_id,
            ref text,
            ..
        }) = self.popup
        {
            let conn = self.open_db()?;
            let translation: Option<&str> = if text.is_empty() {
                None
            } else {
                Some(text.as_str())
            };
            models::update_vocabulary_translation(&conn, vocabulary_id, translation)?;

            // Update in-memory cache
            if let Some(vocab) = models::get_vocabulary_by_id(&conn, vocabulary_id)? {
                if let Some(ref mut state) = self.reader_state {
                    state
                        .vocabulary_cache
                        .insert((vocab.base_form.clone(), vocab.reading.clone()), vocab);
                }
            }
            self.set_message("Translation saved");
        }
        self.popup = None;
        Ok(())
    }

    // ─── Sentence Translation ────────────────────────────────────────────

    /// Open sentence translation editor for the current sentence.
    pub fn open_sentence_translation_editor(&mut self) -> Result<()> {
        let (text_id, sentence_index, sentence_text, existing) = {
            let state = match self.reader_state.as_ref() {
                Some(s) => s,
                None => return Ok(()),
            };
            if state.sentences.is_empty() {
                return Ok(());
            }
            let sentence = &state.sentences[state.sentence_index];
            let existing = state
                .sentence_translations
                .get(&state.sentence_index)
                .map(|t| t.translation.clone())
                .unwrap_or_default();
            (
                state.text_id,
                state.sentence_index,
                sentence.text.clone(),
                existing,
            )
        };

        self.popup = Some(PopupState::SentenceTranslationEditor {
            text_id,
            sentence_index,
            sentence_text,
            translation: existing,
        });
        Ok(())
    }

    /// Save sentence translation and create SRS card.
    pub fn save_sentence_translation(&mut self) -> Result<()> {
        let (text_id, sentence_index, sentence_text, translation) = match self.popup {
            Some(PopupState::SentenceTranslationEditor {
                text_id,
                sentence_index,
                ref sentence_text,
                ref translation,
            }) => (
                text_id,
                sentence_index,
                sentence_text.clone(),
                translation.clone(),
            ),
            _ => return Ok(()),
        };
        self.popup = None;

        if translation.is_empty() {
            self.set_message("Translation is empty, not saved");
            return Ok(());
        }

        let conn = self.open_db()?;
        let trans_id = models::upsert_sentence_translation(
            &conn,
            text_id,
            sentence_index as i64,
            &sentence_text,
            &translation,
            None,
            "user",
            None,
        )?;

        // Create SRS sentence_full card if not exists
        if !models::has_sentence_full_card(&conn, trans_id)? {
            models::insert_sentence_full_card(&conn, trans_id)?;
            self.set_message("Sentence translation saved + SRS card created");
        } else {
            self.set_message("Sentence translation updated");
        }

        // Update in-memory cache
        if let Some(ref mut state) = self.reader_state {
            if let Ok(Some(st)) =
                models::get_sentence_translation(&conn, text_id, sentence_index as i64)
            {
                state.sentence_translations.insert(sentence_index, st);
            }
        }

        Ok(())
    }

    // ─── Card Browser ────────────────────────────────────────────────────

    /// Load the card browser screen.
    pub fn load_card_browser(&mut self) -> Result<()> {
        let conn = self.open_db()?;
        let cards = models::get_all_srs_cards(&conn, 1000)?;
        let now = chrono::Utc::now().naive_utc();

        let mut entries: Vec<CardBrowserEntry> = Vec::new();
        for card in cards {
            let vocabulary = card
                .vocabulary_id
                .and_then(|vid| models::get_vocabulary_by_id(&conn, vid).ok().flatten());

            let sentence_translation = card.sentence_translation_id.and_then(|stid| {
                models::get_sentence_translation_by_id(&conn, stid)
                    .ok()
                    .flatten()
            });

            let display_front = if let Some(ref v) = vocabulary {
                format!("{} ({})", v.base_form, v.reading)
            } else if let Some(ref st) = sentence_translation {
                let truncated: String = st.sentence_text.chars().take(40).collect();
                if st.sentence_text.len() > 40 {
                    format!("{}...", truncated)
                } else {
                    truncated
                }
            } else {
                format!("Card #{}", card.id)
            };

            let display_back = if let Some(ref st) = sentence_translation {
                st.translation.clone()
            } else if let Some(ref v) = vocabulary {
                v.translation.clone().unwrap_or_default()
            } else {
                String::new()
            };

            let due_label = {
                let due =
                    chrono::NaiveDateTime::parse_from_str(&card.due_date, "%Y-%m-%d %H:%M:%S")
                        .unwrap_or(now);
                if due <= now {
                    "due now".to_string()
                } else {
                    let diff = due.signed_duration_since(now);
                    let days = diff.num_days();
                    let hours = diff.num_hours();
                    if days > 0 {
                        format!("in {}d", days)
                    } else if hours > 0 {
                        format!("in {}h", hours)
                    } else {
                        format!("in {}m", diff.num_minutes().max(1))
                    }
                }
            };

            entries.push(CardBrowserEntry {
                card,
                vocabulary,
                sentence_translation,
                display_front,
                display_back,
                due_label,
            });
        }

        let selected = self
            .card_browser_state
            .as_ref()
            .map(|s| s.selected.min(entries.len().saturating_sub(1)))
            .unwrap_or(0);
        let filter = self
            .card_browser_state
            .as_ref()
            .map(|s| s.filter.clone())
            .unwrap_or(CardBrowserFilter::All);
        let sort = self
            .card_browser_state
            .as_ref()
            .map(|s| s.sort.clone())
            .unwrap_or(CardBrowserSort::DueDate);

        self.card_browser_state = Some(CardBrowserState {
            entries,
            selected,
            scroll_offset: 0,
            filter,
            sort,
        });
        self.previous_screen = Some(self.screen.clone());
        self.screen = Screen::CardBrowser;
        Ok(())
    }

    /// Get filtered entries for the card browser.
    pub fn card_browser_filtered_entries(&self) -> Vec<usize> {
        let state = match self.card_browser_state.as_ref() {
            Some(s) => s,
            None => return vec![],
        };
        state
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| match &state.filter {
                CardBrowserFilter::All => true,
                CardBrowserFilter::DueNow => {
                    let now = chrono::Utc::now().naive_utc();
                    chrono::NaiveDateTime::parse_from_str(&entry.card.due_date, "%Y-%m-%d %H:%M:%S")
                        .map(|d| d <= now)
                        .unwrap_or(true)
                        && entry.card.state != "retired"
                }
                CardBrowserFilter::WordCards => entry.card.card_type == "word",
                CardBrowserFilter::SentenceCards => entry.card.card_type == "sentence",
                CardBrowserFilter::New => entry.card.state == "new",
                CardBrowserFilter::Learning => entry.card.state == "learning",
                CardBrowserFilter::Review => entry.card.state == "review",
                CardBrowserFilter::Retired => entry.card.state == "retired",
            })
            .map(|(i, _)| i)
            .collect()
    }

    // ─── Settings ────────────────────────────────────────────────────────

    /// Load the settings screen.
    pub fn load_settings(&mut self) {
        let srs = &self.config.srs;
        let reader = &self.config.reader;

        let categories = vec![
            SettingsCategory {
                name: "Reader".to_string(),
                items: vec![
                    SettingsItem {
                        key: "reader.sidebar_width".to_string(),
                        label: "Sidebar width (%)".to_string(),
                        description: "Percentage of screen for sidebar".to_string(),
                        value: SettingsValue::Integer(reader.sidebar_width as i64),
                    },
                    SettingsItem {
                        key: "reader.furigana".to_string(),
                        label: "Show furigana".to_string(),
                        description: "Display furigana above kanji".to_string(),
                        value: SettingsValue::Bool(reader.furigana),
                    },
                    SettingsItem {
                        key: "reader.preprocess_ahead".to_string(),
                        label: "Preprocess ahead".to_string(),
                        description: "Chapters to preprocess ahead".to_string(),
                        value: SettingsValue::Integer(reader.preprocess_ahead as i64),
                    },
                ],
            },
            SettingsCategory {
                name: "SRS".to_string(),
                items: vec![
                    SettingsItem {
                        key: "srs.default_answer_mode".to_string(),
                        label: "Default answer mode".to_string(),
                        description: "Default review mode for new cards".to_string(),
                        value: SettingsValue::Choice(
                            srs.default_answer_mode.clone(),
                            vec![
                                "meaning_recall".to_string(),
                                "reading_recall".to_string(),
                                "typed_reading".to_string(),
                                "sentence_cloze".to_string(),
                                "sentence_full".to_string(),
                            ],
                        ),
                    },
                    SettingsItem {
                        key: "srs.new_cards_per_day".to_string(),
                        label: "New cards per day".to_string(),
                        description: "Max new cards introduced per day (0 = unlimited)".to_string(),
                        value: SettingsValue::Integer(srs.new_cards_per_day as i64),
                    },
                    SettingsItem {
                        key: "srs.max_reviews_per_session".to_string(),
                        label: "Max reviews per session".to_string(),
                        description: "0 = unlimited".to_string(),
                        value: SettingsValue::Integer(srs.max_reviews_per_session as i64),
                    },
                    SettingsItem {
                        key: "srs.review_order".to_string(),
                        label: "Review order".to_string(),
                        description: "Order of cards in review session".to_string(),
                        value: SettingsValue::Choice(
                            srs.review_order.clone(),
                            vec!["due_first".to_string(), "random".to_string()],
                        ),
                    },
                    SettingsItem {
                        key: "srs.review_word_cards".to_string(),
                        label: "Review word cards".to_string(),
                        description: "Include word cards in review".to_string(),
                        value: SettingsValue::Bool(self.config.srs.review_word_cards),
                    },
                    SettingsItem {
                        key: "srs.review_sentence_cloze_cards".to_string(),
                        label: "Review sentence cloze cards".to_string(),
                        description: "Include sentence cloze in review".to_string(),
                        value: SettingsValue::Bool(self.config.srs.review_sentence_cloze_cards),
                    },
                    SettingsItem {
                        key: "srs.review_sentence_full_cards".to_string(),
                        label: "Review sentence full cards".to_string(),
                        description: "Include sentence full in review".to_string(),
                        value: SettingsValue::Bool(self.config.srs.review_sentence_full_cards),
                    },
                    SettingsItem {
                        key: "srs.require_typed_input".to_string(),
                        label: "Require typed reading".to_string(),
                        description: "Type reading for word cards (accepts hiragana/romaji/kanji)"
                            .to_string(),
                        value: SettingsValue::Bool(self.config.srs.require_typed_input),
                    },
                ],
            },
        ];

        self.settings_state = Some(SettingsState {
            categories,
            selected_category: 0,
            selected_item: 0,
            editing: false,
            edit_buffer: String::new(),
            dirty: false,
        });
        self.previous_screen = Some(self.screen.clone());
        self.screen = Screen::Settings;
    }

    /// Apply settings changes back to the config and optionally save to file.
    pub fn apply_settings(&mut self) -> Result<()> {
        let state = match self.settings_state.as_ref() {
            Some(s) => s,
            None => return Ok(()),
        };

        for cat in &state.categories {
            for item in &cat.items {
                match item.key.as_str() {
                    "reader.sidebar_width" => {
                        if let SettingsValue::Integer(v) = &item.value {
                            self.config.reader.sidebar_width = (*v).max(10).min(80) as u16;
                        }
                    }
                    "reader.furigana" => {
                        if let SettingsValue::Bool(v) = &item.value {
                            self.config.reader.furigana = *v;
                        }
                    }
                    "reader.preprocess_ahead" => {
                        if let SettingsValue::Integer(v) = &item.value {
                            self.config.reader.preprocess_ahead = (*v).max(0).min(20) as usize;
                        }
                    }
                    "srs.default_answer_mode" => {
                        if let SettingsValue::Choice(v, _) = &item.value {
                            self.config.srs.default_answer_mode = v.clone();
                        } else if let SettingsValue::Text(v) = &item.value {
                            self.config.srs.default_answer_mode = v.clone();
                        }
                    }
                    "srs.new_cards_per_day" => {
                        if let SettingsValue::Integer(v) = &item.value {
                            self.config.srs.new_cards_per_day = (*v).max(0) as u32;
                        }
                    }
                    "srs.max_reviews_per_session" => {
                        if let SettingsValue::Integer(v) = &item.value {
                            self.config.srs.max_reviews_per_session = (*v).max(0) as u32;
                        }
                    }
                    "srs.review_order" => {
                        if let SettingsValue::Choice(v, _) = &item.value {
                            self.config.srs.review_order = v.clone();
                        } else if let SettingsValue::Text(v) = &item.value {
                            self.config.srs.review_order = v.clone();
                        }
                    }
                    "srs.review_word_cards" => {
                        if let SettingsValue::Bool(v) = &item.value {
                            self.config.srs.review_word_cards = *v;
                        }
                    }
                    "srs.review_sentence_cloze_cards" => {
                        if let SettingsValue::Bool(v) = &item.value {
                            self.config.srs.review_sentence_cloze_cards = *v;
                        }
                    }
                    "srs.review_sentence_full_cards" => {
                        if let SettingsValue::Bool(v) = &item.value {
                            self.config.srs.review_sentence_full_cards = *v;
                        }
                    }
                    "srs.require_typed_input" => {
                        if let SettingsValue::Bool(v) = &item.value {
                            self.config.srs.require_typed_input = *v;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Save config to TOML file
        crate::config::save_config(&self.config)?;
        self.set_message("Settings saved");
        Ok(())
    }
}

/// Build SentenceData from paragraphs using sentence_index stored in DB tokens.
/// No re-tokenization needed — sentence boundaries come from the DB.
fn build_sentences(
    paragraphs: &[ParagraphData],
    vocab_cache: &HashMap<(String, String), Vocabulary>,
    gloss_cache: &HashMap<String, String>,
) -> Vec<SentenceData> {
    let mut sentences = Vec::new();

    for (para_idx, para) in paragraphs.iter().enumerate() {
        if para.db_tokens.is_empty() {
            continue;
        }

        // Group tokens by sentence_index
        let mut current_sent_idx = para.db_tokens[0].sentence_index;
        let mut current_tokens: Vec<TokenDisplay> = Vec::new();
        let mut start_token = 0usize;

        for (i, db_tok) in para.db_tokens.iter().enumerate() {
            if db_tok.sentence_index != current_sent_idx {
                // Flush current sentence
                let text = current_tokens
                    .iter()
                    .map(|t| t.surface.as_str())
                    .collect::<String>();
                sentences.push(SentenceData {
                    paragraph_idx: para_idx,
                    start_token,
                    end_token: i,
                    tokens: current_tokens,
                    text,
                });
                current_tokens = Vec::new();
                start_token = i;
                current_sent_idx = db_tok.sentence_index;
            }

            let key = (db_tok.base_form.clone(), db_tok.reading.clone());
            let vocab = vocab_cache.get(&key);
            let status = vocab.map(|v| v.status).unwrap_or(VocabularyStatus::New);

            // A token is trivial if its POS says so, OR if its vocabulary status is Ignored
            let is_trivial =
                is_trivial_pos(&db_tok.pos, &db_tok.surface) || status == VocabularyStatus::Ignored;

            let short_gloss = if !is_trivial {
                gloss_cache
                    .get(&db_tok.base_form)
                    .cloned()
                    .unwrap_or_default()
            } else {
                String::new()
            };

            current_tokens.push(TokenDisplay {
                surface: db_tok.surface.clone(),
                base_form: db_tok.base_form.clone(),
                reading: db_tok.reading.clone(),
                surface_reading: db_tok.surface_reading.clone(),
                pos: db_tok.pos.clone(),
                vocabulary_status: status,
                is_selected: false,
                short_gloss,
                conjugation_form: translate_conjugation_form(&db_tok.conjugation_form),
                conjugation_type: translate_conjugation_type(&db_tok.conjugation_type),
                is_trivial,
                group_id: None,
                is_group_head: false,
                conjugation_desc: String::new(),
                mwe_gloss: String::new(),
            });
        }

        // Flush last sentence
        if !current_tokens.is_empty() {
            let text = current_tokens
                .iter()
                .map(|t| t.surface.as_str())
                .collect::<String>();
            sentences.push(SentenceData {
                paragraph_idx: para_idx,
                start_token,
                end_token: para.db_tokens.len(),
                tokens: current_tokens,
                text,
            });
        }
    }

    // Apply conjugation grouping to each sentence
    for sentence in &mut sentences {
        apply_conjugation_groups(&mut sentence.tokens);
    }

    sentences
}

/// Apply conjugation grouping to a sentence's tokens.
/// Groups verb/adjective heads with following auxiliaries, assigns group IDs,
/// propagates the head's vocabulary_status to group members, and sets
/// the conjugation description on the head token.
fn apply_conjugation_groups(tokens: &mut [TokenDisplay]) {
    // Build lightweight GroupToken references for the grouping algorithm
    let group_tokens: Vec<GroupToken> = tokens
        .iter()
        .map(|t| GroupToken {
            pos: &t.pos,
            base_form: &t.base_form,
            conjugation_form: &t.conjugation_form,
        })
        .collect();

    let groups = tokenizer::assign_conjugation_groups(&group_tokens);

    for group in &groups {
        let head_status = tokens[group.head_index].vocabulary_status;

        for &idx in &group.member_indices {
            tokens[idx].group_id = Some(group.group_id);
            // Propagate head's vocabulary status to all group members
            tokens[idx].vocabulary_status = head_status;
            // Clear is_trivial so the renderer highlights group members
            // (auxiliaries like ない, ます, た are normally trivial, but when
            // part of a conjugation group they should highlight with the head)
            tokens[idx].is_trivial = false;
        }

        // Mark head and set description
        tokens[group.head_index].is_group_head = true;
        tokens[group.head_index].conjugation_desc = group.description.clone();
    }
}

/// Detect MWE matches across all sentences using JMdict and user expressions.
/// Returns a Vec with one Vec<MweMatch> per sentence.
fn detect_all_mwe_matches(
    conn: &rusqlite::Connection,
    sentences: &[SentenceData],
    user_expressions: &[models::UserExpression],
) -> Vec<Vec<MweMatch>> {
    let max_window = 12; // Max tokens to combine in a sliding window

    sentences
        .iter()
        .map(|sentence| detect_sentence_mwes(conn, &sentence.tokens, user_expressions, max_window))
        .collect()
}

/// Detect MWE matches in a single sentence using a sliding window.
/// User expressions take priority over JMdict matches.
fn detect_sentence_mwes(
    conn: &rusqlite::Connection,
    tokens: &[TokenDisplay],
    user_expressions: &[models::UserExpression],
    max_window: usize,
) -> Vec<MweMatch> {
    let mut matches: Vec<MweMatch> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let mut best_match: Option<MweMatch> = None;
        let mut combined = String::new();

        let end = tokens.len().min(i + max_window);
        for j in i..end {
            combined.push_str(&tokens[j].surface);

            // Skip single-token matches (already handled by normal vocabulary)
            if j <= i {
                continue;
            }

            // Check user expressions first (highest priority)
            if let Some(ue) = user_expressions.iter().find(|ue| ue.surface == combined) {
                best_match = Some(MweMatch {
                    start: i,
                    end: j + 1,
                    surface: combined.clone(),
                    reading: ue.reading.clone(),
                    gloss: ue.gloss.clone(),
                });
                continue; // keep looking for longer matches
            }

            // Check JMdict
            if dictionary::has_jmdict_kanji_entry(conn, &combined) {
                if let Some((reading, gloss)) = dictionary::lookup_mwe_info(conn, &combined) {
                    best_match = Some(MweMatch {
                        start: i,
                        end: j + 1,
                        surface: combined.clone(),
                        reading,
                        gloss,
                    });
                    // keep looking for longer matches (greedy)
                }
            }
        }

        if let Some(m) = best_match {
            let skip_to = m.end;
            matches.push(m);
            i = skip_to; // advance past the match
        } else {
            i += 1;
        }
    }

    matches
}

/// Apply MWE matches to tokens in a sentence.
/// MWE groups override conjugation groups for overlapping tokens.
/// The group_id space for MWEs starts at 10000 to avoid collisions with
/// conjugation group IDs.
fn apply_mwe_matches(tokens: &mut [TokenDisplay], matches: &[MweMatch]) {
    for (match_idx, m) in matches.iter().enumerate() {
        let mwe_group_id = 10000 + match_idx;

        // Find the first non-trivial token in the match to be the head
        let head_idx = (m.start..m.end)
            .find(|&idx| !tokens[idx].is_trivial)
            .unwrap_or(m.start);

        // Get the head's vocabulary status to propagate to all members
        let head_status = tokens[head_idx].vocabulary_status;

        for idx in m.start..m.end {
            if idx >= tokens.len() {
                break;
            }
            // Override any existing conjugation group assignment
            tokens[idx].group_id = Some(mwe_group_id);
            tokens[idx].is_group_head = idx == head_idx;
            tokens[idx].conjugation_desc = String::new();
            tokens[idx].mwe_gloss = m.gloss.clone();
            // Propagate head's vocabulary status so all members highlight uniformly
            // (particles like の, も normally have Ignored status → DarkGray,
            // but inside an MWE they should match the head's highlight color)
            tokens[idx].vocabulary_status = head_status;
            // Clear is_trivial so the renderer uses status_style() for coloring
            tokens[idx].is_trivial = false;
        }

        // Set the head's description
        if head_idx < tokens.len() {
            tokens[head_idx].conjugation_desc = format!("expression");
        }
    }
}

/// Check if a POS tag represents a trivial token (not worth tracking as vocabulary).
fn is_trivial_pos(pos: &str, surface: &str) -> bool {
    matches!(
        pos,
        "Symbol"
            | "Punctuation"
            | "Whitespace"
            | "BOS/EOS"
            | ""
            | "Particle"
            | "Auxiliary"
            | "Conjunction"
            | "Prefix"
    ) || surface.trim().is_empty()
        || is_numeric(surface)
        || is_ascii_only(surface)
}

/// Check if a string is purely numeric.
fn is_numeric(s: &str) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',' || ('０'..='９').contains(&c))
}

/// Check if a string contains only ASCII characters (English text, etc.).
fn is_ascii_only(s: &str) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii())
}

/// Translate UniDic conjugation form names (Japanese) to English.
fn translate_conjugation_form(form: &str) -> String {
    // UniDic conjugation forms are compound: "連用形-一般", "終止形-一般", etc.
    // We translate the main part and the sub-part separately.
    if form.is_empty() {
        return String::new();
    }

    let parts: Vec<&str> = form.splitn(2, '-').collect();
    let main = match parts[0] {
        "未然形" => "irrealis",       // negative/volitional stem
        "連用形" => "continuative",   // masu-stem / te-form stem
        "終止形" => "terminal",       // dictionary/plain ending
        "連体形" => "attributive",    // modifies noun
        "仮定形" => "conditional",    // ba-conditional
        "命令形" => "imperative",     // command form
        "意志推量形" => "volitional", // let's / probably
        "語幹" => "stem",
        other => other,
    };

    if parts.len() > 1 {
        let sub = match parts[1] {
            "一般" => "general",
            "促音便" => "geminate", // っ sound change (e.g. 食べたかった)
            "撥音便" => "nasal",    // ん sound change (e.g. 読んだ)
            "イ音便" => "i-onbin",  // い sound change (e.g. 書いた)
            "ウ音便" => "u-onbin",  // う sound change
            "基本形" => "basic",
            "縮約形" => "contracted",
            other => other,
        };
        format!("{} ({})", main, sub)
    } else {
        main.to_string()
    }
}

/// Translate UniDic conjugation type names (Japanese) to English.
fn translate_conjugation_type(ctype: &str) -> String {
    if ctype.is_empty() {
        return String::new();
    }

    let parts: Vec<&str> = ctype.splitn(2, '-').collect();
    let main = match parts[0] {
        "五段" => "godan",            // u-verbs
        "上一段" => "ichidan-upper",  // ru-verbs (i-stem)
        "下一段" => "ichidan-lower",  // ru-verbs (e-stem)
        "カ行変格" => "ka-irregular", // 来る
        "サ行変格" => "sa-irregular", // する
        "形容詞" => "i-adjective",
        "助動詞" => "auxiliary",
        "文語" => "classical",
        other => other,
    };

    if parts.len() > 1 {
        let sub = match parts[1] {
            "カ行" => "ka-row",
            "ガ行" => "ga-row",
            "サ行" => "sa-row",
            "タ行" => "ta-row",
            "ナ行" => "na-row",
            "バ行" => "ba-row",
            "マ行" => "ma-row",
            "ラ行" => "ra-row",
            "ワ行" => "wa-row",
            "タ" => "ta",
            "ダ" => "da",
            "デス" => "desu",
            "マス" => "masu",
            "タイ" => "tai",
            "ナイ" => "nai",
            "ヌ" => "nu",
            "レル" => "reru",     // passive/potential
            "ラレル" => "rareru", // passive/potential (ichidan)
            "セル" => "seru",     // causative
            "サセル" => "saseru", // causative (ichidan)
            other => other,
        };
        format!("{} ({})", main, sub)
    } else {
        main.to_string()
    }
}
