use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::time::Instant;

use crate::config::AppConfig;
use crate::core::dictionary::{self, DictEntry};
use crate::db::models::{self, TextStats, Vocabulary, VocabularyStatus};
use crate::import::syosetu::SyosetuNovel;

/// Which screen is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Library,
    Reader,
    Syosetu,
    Review,
    Stats,
}

impl Screen {
    pub fn next(self) -> Self {
        match self {
            Screen::Library => Screen::Reader,
            Screen::Reader => Screen::Syosetu,
            Screen::Syosetu => Screen::Review,
            Screen::Review => Screen::Stats,
            Screen::Stats => Screen::Library,
        }
    }
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
    Help,
    /// Note editor for a word.
    NoteEditor {
        vocabulary_id: i64,
        text: String,
    },
    /// Quit confirmation.
    QuitConfirm,
    /// Delete text confirmation.
    DeleteConfirm {
        text_id: i64,
        title: String,
    },
    /// Import sub-menu (clipboard / URL / file).
    ImportMenu,
    /// URL text input for web import.
    UrlInput {
        text: String,
    },
    /// Search/filter input for library.
    SearchInput {
        text: String,
    },
    /// File path input for file/epub/subtitle import.
    FilePathInput {
        text: String,
    },
    /// Syosetu ncode input.
    SyosetuInput {
        text: String,
    },
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

/// Library screen state.
pub struct LibraryState {
    pub texts: Vec<models::Text>,
    pub stats: HashMap<i64, TextStats>,
    pub selected: usize,
    pub sort: LibrarySort,
    pub filter_source: Option<String>,
    /// All unique source types present in the DB.
    pub source_types: Vec<String>,
}

/// Syosetu novel browser state.
pub struct SyosetuState {
    pub novel: SyosetuNovel,
    pub selected_chapter: usize,
}

/// Central application state.
pub struct App {
    pub screen: Screen,
    pub config: AppConfig,
    pub reader_state: Option<ReaderState>,
    pub library_state: Option<LibraryState>,
    pub syosetu_state: Option<SyosetuState>,
    pub popup: Option<PopupState>,
    pub message: Option<(String, Instant)>,
    pub should_quit: bool,
    pub db_path: std::path::PathBuf,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        let db_path = config.db_path();
        Self {
            screen: Screen::Library,
            config,
            reader_state: None,
            library_state: None,
            syosetu_state: None,
            popup: None,
            message: None,
            should_quit: false,
            db_path,
        }
    }

    pub fn open_db(&self) -> Result<Connection> {
        crate::db::connection::open_or_create(&self.db_path)
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
    }

    /// Clear expired messages (older than 3 seconds).
    pub fn tick(&mut self) {
        if let Some((_, when)) = &self.message {
            if when.elapsed().as_secs() >= 3 {
                self.message = None;
            }
        }
    }

    /// Refresh the library text list with stats, sorting, and filtering.
    pub fn refresh_library(&mut self) -> Result<()> {
        let conn = self.open_db()?;

        // Preserve current sort/filter from existing state
        let (sort, filter_source) = self.library_state.as_ref()
            .map(|s| (s.sort, s.filter_source.clone()))
            .unwrap_or((LibrarySort::DateDesc, None));

        // Get all texts (or filtered)
        let mut texts = if let Some(ref src) = filter_source {
            models::list_texts_by_source_type(&conn, src)?
        } else {
            models::list_all_texts(&conn)?
        };

        // Collect unique source types
        let source_types = {
            let all = models::list_all_texts(&conn)?;
            let mut types: Vec<String> = all.iter().map(|t| t.source_type.clone()).collect();
            types.sort();
            types.dedup();
            types
        };

        // Load per-text stats
        let mut stats = HashMap::new();
        for t in &texts {
            if let Ok(s) = models::get_text_stats(&conn, t.id) {
                stats.insert(t.id, s);
            }
        }

        // Apply sort
        match sort {
            LibrarySort::DateDesc => texts.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            LibrarySort::DateAsc => texts.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
            LibrarySort::TitleAsc => texts.sort_by(|a, b| a.title.cmp(&b.title)),
            LibrarySort::Completion => {
                texts.sort_by(|a, b| {
                    let pct_a = stats.get(&a.id).map(|s| {
                        if s.unique_vocab == 0 { 0.0 } else { s.known_count as f64 / s.unique_vocab as f64 }
                    }).unwrap_or(0.0);
                    let pct_b = stats.get(&b.id).map(|s| {
                        if s.unique_vocab == 0 { 0.0 } else { s.known_count as f64 / s.unique_vocab as f64 }
                    }).unwrap_or(0.0);
                    pct_b.partial_cmp(&pct_a).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        let selected = self.library_state.as_ref()
            .map(|s| s.selected.min(texts.len().saturating_sub(1)))
            .unwrap_or(0);

        self.library_state = Some(LibraryState {
            texts,
            stats,
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
                    if !is_trivial_pos(&tok.pos, &tok.surface) && seen.insert(tok.base_form.clone()) {
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

        let sentences = build_sentences(&paragraphs, &vocabulary_cache, &gloss_cache);

        // Restore saved reading progress
        let saved_index = models::get_reading_progress(&conn, text_id).unwrap_or(0);
        let sentence_index = if saved_index < sentences.len() { saved_index } else { 0 };

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
        });

        self.screen = Screen::Reader;
        self.set_message(format!("Loaded: {}", text.title));
        Ok(())
    }

    /// Refresh token display after vocabulary changes.
    pub fn refresh_reader_display(&mut self) -> Result<()> {
        if let Some(ref mut state) = self.reader_state {
            state.sentences = build_sentences(&state.paragraphs, &state.vocabulary_cache, &state.gloss_cache);
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

        // Update cache & patch affected tokens in-place (no full rebuild)
        if let Some(ref mut state) = self.reader_state {
            if let Some(vocab) = models::get_vocabulary_by_id(&conn, vid)? {
                state.vocabulary_cache.insert(
                    (base_form.clone(), reading.clone()),
                    vocab,
                );
            }
            // Patch all tokens matching this base_form+reading
            for sentence in &mut state.sentences {
                for token in &mut sentence.tokens {
                    if token.base_form == base_form && token.reading == reading {
                        token.vocabulary_status = status;
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

    /// Open word detail popup for the currently selected word.
    pub fn open_word_detail(&mut self) -> Result<()> {
        let (base_form, reading, notes, vocab_id) = {
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
            let key = (token.base_form.clone(), token.reading.clone());
            let vocab = state.vocabulary_cache.get(&key);
            let notes = vocab.and_then(|v| v.notes.clone());
            let vocab_id = vocab.map(|v| v.id);
            (token.base_form.clone(), token.reading.clone(), notes, vocab_id)
        };

        let conn = self.open_db()?;
        let entries = dictionary::lookup(&conn, &base_form, None)?;

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
        let ext = path.extension()
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
                    path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled"),
                    &std::fs::read_to_string(&path)?,
                    "text",
                    None,
                    &conn,
                )?;
                let text = models::get_text_by_id(&conn, text_id)?.map(|t| t.title).unwrap_or_default();
                text
            }
        };
        self.refresh_library()?;
        Ok(result)
    }

    /// Load a Syosetu novel by ncode into the Syosetu TUI screen.
    pub fn load_syosetu(&mut self, ncode_input: &str) -> Result<()> {
        let ncode = crate::import::syosetu::parse_ncode(ncode_input)?;
        let novel = crate::import::syosetu::fetch_novel_info(&ncode)?;
        self.syosetu_state = Some(SyosetuState {
            novel,
            selected_chapter: 0,
        });
        self.screen = Screen::Syosetu;
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

    /// Go back to library from reader, saving progress.
    pub fn back_to_library(&mut self) -> Result<()> {
        let _ = self.save_reading_progress();
        self.screen = Screen::Library;
        self.refresh_library()?;
        Ok(())
    }

    /// Search the library by title.
    pub fn search_library(&mut self, query: &str) -> Result<()> {
        let conn = self.open_db()?;
        let texts = models::search_texts(&conn, query)?;
        let mut stats = HashMap::new();
        for t in &texts {
            if let Ok(s) = models::get_text_stats(&conn, t.id) {
                stats.insert(t.id, s);
            }
        }
        let source_types = self.library_state.as_ref()
            .map(|s| s.source_types.clone())
            .unwrap_or_default();
        self.library_state = Some(LibraryState {
            texts,
            stats,
            selected: 0,
            sort: LibrarySort::DateDesc,
            filter_source: None,
            source_types,
        });
        Ok(())
    }

    /// Save note from the note editor.
    pub fn save_note(&mut self) -> Result<()> {
        if let Some(PopupState::NoteEditor { vocabulary_id, ref text }) = self.popup {
            let conn = self.open_db()?;
            let notes: Option<&str> = if text.is_empty() { None } else { Some(text.as_str()) };
            conn.execute(
                "UPDATE vocabulary SET notes = ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![notes, vocabulary_id],
            )?;

            if let Some(vocab) = models::get_vocabulary_by_id(&conn, vocabulary_id)? {
                if let Some(ref mut state) = self.reader_state {
                    state.vocabulary_cache.insert(
                        (vocab.base_form.clone(), vocab.reading.clone()),
                        vocab,
                    );
                }
            }
            self.set_message("Note saved");
        }
        self.popup = None;
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
                let text = current_tokens.iter().map(|t| t.surface.as_str()).collect::<String>();
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
            let is_trivial = is_trivial_pos(&db_tok.pos, &db_tok.surface)
                || status == VocabularyStatus::Ignored;

            let short_gloss = if !is_trivial {
                gloss_cache.get(&db_tok.base_form).cloned().unwrap_or_default()
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
            });
        }

        // Flush last sentence
        if !current_tokens.is_empty() {
            let text = current_tokens.iter().map(|t| t.surface.as_str()).collect::<String>();
            sentences.push(SentenceData {
                paragraph_idx: para_idx,
                start_token,
                end_token: para.db_tokens.len(),
                tokens: current_tokens,
                text,
            });
        }
    }

    sentences
}

/// Check if a POS tag represents a trivial token (not worth tracking as vocabulary).
fn is_trivial_pos(pos: &str, surface: &str) -> bool {
    matches!(
        pos,
        "Symbol" | "Punctuation" | "Whitespace" | "BOS/EOS" | ""
            | "Particle" | "Auxiliary" | "Conjunction" | "Prefix" | "Suffix"
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
        "未然形" => "irrealis",         // negative/volitional stem
        "連用形" => "continuative",     // masu-stem / te-form stem
        "終止形" => "terminal",         // dictionary/plain ending
        "連体形" => "attributive",      // modifies noun
        "仮定形" => "conditional",      // ba-conditional
        "命令形" => "imperative",       // command form
        "意志推量形" => "volitional",   // let's / probably
        "語幹" => "stem",
        other => other,
    };

    if parts.len() > 1 {
        let sub = match parts[1] {
            "一般" => "general",
            "促音便" => "geminate",     // っ sound change (e.g. 食べたかった)
            "撥音便" => "nasal",        // ん sound change (e.g. 読んだ)
            "イ音便" => "i-onbin",      // い sound change (e.g. 書いた)
            "ウ音便" => "u-onbin",      // う sound change
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
        "五段" => "godan",             // u-verbs
        "上一段" => "ichidan-upper",    // ru-verbs (i-stem)
        "下一段" => "ichidan-lower",    // ru-verbs (e-stem)
        "カ行変格" => "ka-irregular",   // 来る
        "サ行変格" => "sa-irregular",   // する
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
            "レル" => "reru",       // passive/potential
            "ラレル" => "rareru",   // passive/potential (ichidan)
            "セル" => "seru",       // causative
            "サセル" => "saseru",   // causative (ichidan)
            other => other,
        };
        format!("{} ({})", main, sub)
    } else {
        main.to_string()
    }
}
