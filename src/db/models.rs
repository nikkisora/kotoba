use anyhow::Result;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

// ─── Enums ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum VocabularyStatus {
    Ignored = -1,
    New = 0,
    Learning1 = 1,
    Learning2 = 2,
    Learning3 = 3,
    Learning4 = 4,
    Known = 5,
}

impl VocabularyStatus {
    pub fn from_i32(v: i32) -> Self {
        match v {
            -1 => Self::Ignored,
            0 => Self::New,
            1 => Self::Learning1,
            2 => Self::Learning2,
            3 => Self::Learning3,
            4 => Self::Learning4,
            5 => Self::Known,
            _ => Self::New,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardType {
    Word,
    Sentence,
}

/// Review presentation mode — determined at review time by card_type + settings.
/// Word cards use WordReview (show word + context, recall reading + meaning).
/// Sentence cards use SentenceFull (show sentence, recall translation).
/// SentenceCloze is a random variant of word review when enabled in settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnswerMode {
    /// Unified word review: show word + context sentence, recall reading + meaning.
    WordReview,
    /// Sentence cloze variant of word review: sentence with word blanked.
    SentenceCloze,
    /// Full sentence review: show sentence, recall translation.
    SentenceFull,
}

// ─── Models ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Text {
    pub id: i64,
    pub title: String,
    pub source_url: Option<String>,
    pub source_type: String,
    pub content: String,
    pub language: String,
    pub created_at: String,
    pub last_read_at: Option<String>,
    pub last_sentence_index: i64,
    pub total_sentences: i64,
}

impl Text {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            title: row.get("title")?,
            source_url: row.get("source_url")?,
            source_type: row.get("source_type")?,
            content: row.get("content")?,
            language: row.get("language")?,
            created_at: row.get("created_at")?,
            last_read_at: row.get("last_read_at").unwrap_or(None),
            last_sentence_index: row.get("last_sentence_index").unwrap_or(0),
            total_sentences: row.get("total_sentences").unwrap_or(0),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub id: i64,
    pub text_id: i64,
    pub position: i32,
    pub content: String,
}

impl Paragraph {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            text_id: row.get("text_id")?,
            position: row.get("position")?,
            content: row.get("content")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub id: i64,
    pub paragraph_id: i64,
    pub position: i32,
    pub surface: String,
    pub base_form: String,
    pub reading: String,
    pub surface_reading: String,
    pub pos: String,
    pub conjugation_form: String,
    pub conjugation_type: String,
    pub sentence_index: i32,
}

impl Token {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            paragraph_id: row.get("paragraph_id")?,
            position: row.get("position")?,
            surface: row.get("surface")?,
            base_form: row.get("base_form")?,
            reading: row.get("reading")?,
            surface_reading: row.get("surface_reading").unwrap_or_default(),
            pos: row.get("pos")?,
            conjugation_form: row.get("conjugation_form")?,
            conjugation_type: row.get("conjugation_type")?,
            sentence_index: row.get("sentence_index").unwrap_or(0),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Vocabulary {
    pub id: i64,
    pub base_form: String,
    pub reading: String,
    pub pos: String,
    pub status: VocabularyStatus,
    pub notes: Option<String>,
    pub translation: Option<String>,
    pub first_seen_at: String,
    pub updated_at: String,
}

impl Vocabulary {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let status_val: i32 = row.get("status")?;
        Ok(Self {
            id: row.get("id")?,
            base_form: row.get("base_form")?,
            reading: row.get("reading")?,
            pos: row.get("pos")?,
            status: VocabularyStatus::from_i32(status_val),
            notes: row.get("notes")?,
            translation: row.get("translation").unwrap_or(None),
            first_seen_at: row.get("first_seen_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ConjugationEncounter {
    pub id: i64,
    pub vocabulary_id: i64,
    pub surface: String,
    pub conjugation_form: String,
    pub conjugation_type: String,
    pub encounter_count: i32,
    pub status: i32,
    pub first_seen: String,
    pub updated: String,
}

#[derive(Debug, Clone)]
pub struct SrsCard {
    pub id: i64,
    pub vocabulary_id: Option<i64>,
    pub conjugation_id: Option<i64>,
    pub card_type: String,
    pub answer_mode: String,
    pub due_date: String,
    pub stability: f64,
    pub difficulty: f64,
    pub reps: i32,
    pub lapses: i32,
    pub state: String,
    pub created_at: String,
    pub sentence_translation_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SrsReview {
    pub id: i64,
    pub card_id: i64,
    pub reviewed_at: String,
    pub rating: i32,
    pub elapsed_ms: i64,
    pub typed_answer: Option<String>,
    pub answer_correct: bool,
}

#[derive(Debug, Clone)]
pub struct LlmCacheEntry {
    pub id: i64,
    pub request_type: String,
    pub request_hash: String,
    pub request_body: String,
    pub response: String,
    pub model: String,
    pub tokens_used: i64,
    pub created_at: String,
}

/// A user-created multi-word expression.
#[derive(Debug, Clone)]
pub struct UserExpression {
    pub id: i64,
    pub surface: String,
    pub reading: String,
    pub gloss: String,
    pub status: VocabularyStatus,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl UserExpression {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let status_val: i32 = row.get("status")?;
        Ok(Self {
            id: row.get("id")?,
            surface: row.get("surface")?,
            reading: row.get("reading")?,
            gloss: row.get("gloss")?,
            status: VocabularyStatus::from_i32(status_val),
            notes: row.get("notes")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

// ─── CRUD Operations ─────────────────────────────────────────────────

/// Insert a new text and return its id.
pub fn insert_text(
    conn: &Connection,
    title: &str,
    content: &str,
    source_type: &str,
    source_url: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO texts (title, content, source_type, source_url) VALUES (?1, ?2, ?3, ?4)",
        params![title, content, source_type, source_url],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_text_by_id(conn: &Connection, id: i64) -> Result<Option<Text>> {
    let mut stmt = conn.prepare("SELECT * FROM texts WHERE id = ?1")?;
    let mut rows = stmt.query_map(params![id], Text::from_row)?;
    Ok(rows.next().transpose()?)
}

/// Insert a paragraph and return its id.
pub fn insert_paragraph(
    conn: &Connection,
    text_id: i64,
    position: i32,
    content: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO paragraphs (text_id, position, content) VALUES (?1, ?2, ?3)",
        params![text_id, position, content],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_paragraphs_by_text(conn: &Connection, text_id: i64) -> Result<Vec<Paragraph>> {
    let mut stmt = conn.prepare("SELECT * FROM paragraphs WHERE text_id = ?1 ORDER BY position")?;
    let rows = stmt.query_map(params![text_id], Paragraph::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Insert a token and return its id.
pub fn insert_token(
    conn: &Connection,
    paragraph_id: i64,
    position: i32,
    surface: &str,
    base_form: &str,
    reading: &str,
    surface_reading: &str,
    pos: &str,
    conjugation_form: &str,
    conjugation_type: &str,
    sentence_index: i32,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO tokens (paragraph_id, position, surface, base_form, reading, surface_reading, pos, conjugation_form, conjugation_type, sentence_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![paragraph_id, position, surface, base_form, reading, surface_reading, pos, conjugation_form, conjugation_type, sentence_index],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_tokens_by_paragraph(conn: &Connection, paragraph_id: i64) -> Result<Vec<Token>> {
    let mut stmt =
        conn.prepare("SELECT * FROM tokens WHERE paragraph_id = ?1 ORDER BY position")?;
    let rows = stmt.query_map(params![paragraph_id], Token::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Upsert vocabulary: insert if not exists, leave status unchanged if exists.
/// Returns the vocabulary id.
pub fn upsert_vocabulary(
    conn: &Connection,
    base_form: &str,
    reading: &str,
    pos: &str,
) -> Result<i64> {
    // Try to find existing
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM vocabulary WHERE base_form = ?1 AND reading = ?2",
            params![base_form, reading],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        return Ok(id);
    }

    conn.execute(
        "INSERT INTO vocabulary (base_form, reading, pos) VALUES (?1, ?2, ?3)",
        params![base_form, reading, pos],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_vocabulary_by_id(conn: &Connection, id: i64) -> Result<Option<Vocabulary>> {
    let mut stmt = conn.prepare("SELECT * FROM vocabulary WHERE id = ?1")?;
    let mut rows = stmt.query_map(params![id], Vocabulary::from_row)?;
    Ok(rows.next().transpose()?)
}

pub fn update_vocabulary_status(
    conn: &Connection,
    id: i64,
    status: VocabularyStatus,
) -> Result<()> {
    conn.execute(
        "UPDATE vocabulary SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
        params![status as i32, id],
    )?;
    Ok(())
}

pub fn list_vocabulary_by_status(
    conn: &Connection,
    status: VocabularyStatus,
) -> Result<Vec<Vocabulary>> {
    let mut stmt = conn.prepare("SELECT * FROM vocabulary WHERE status = ?1 ORDER BY base_form")?;
    let rows = stmt.query_map(params![status as i32], Vocabulary::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Upsert conjugation encounter: increment count if exists, insert if not.
pub fn upsert_conjugation_encounter(
    conn: &Connection,
    vocabulary_id: i64,
    surface: &str,
    conjugation_form: &str,
    conjugation_type: &str,
) -> Result<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM conjugation_encounters WHERE vocabulary_id = ?1 AND surface = ?2",
            params![vocabulary_id, surface],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        conn.execute(
            "UPDATE conjugation_encounters SET encounter_count = encounter_count + 1, updated = datetime('now') WHERE id = ?1",
            params![id],
        )?;
        return Ok(id);
    }

    conn.execute(
        "INSERT INTO conjugation_encounters (vocabulary_id, surface, conjugation_form, conjugation_type)
         VALUES (?1, ?2, ?3, ?4)",
        params![vocabulary_id, surface, conjugation_form, conjugation_type],
    )?;
    Ok(conn.last_insert_rowid())
}

// ─── Web Source Models ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WebSource {
    pub id: i64,
    pub source_type: String,
    pub external_id: String,
    pub title: String,
    pub metadata_json: String,
    pub last_synced: String,
}

impl WebSource {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            source_type: row.get("source_type")?,
            external_id: row.get("external_id")?,
            title: row.get("title")?,
            metadata_json: row.get("metadata_json")?,
            last_synced: row.get("last_synced")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WebSourceChapter {
    pub id: i64,
    pub web_source_id: i64,
    pub chapter_number: i32,
    pub title: String,
    pub text_id: Option<i64>,
    pub word_count: i32,
    pub created_at: String,
    pub is_skipped: bool,
    /// Group/arc name this chapter belongs to (e.g. "クマさん、異世界に来る").
    pub chapter_group: String,
}

impl WebSourceChapter {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            web_source_id: row.get("web_source_id")?,
            chapter_number: row.get("chapter_number")?,
            title: row.get("title")?,
            text_id: row.get("text_id")?,
            word_count: row.get("word_count")?,
            created_at: row.get("created_at")?,
            is_skipped: row.get::<_, i32>("is_skipped").unwrap_or(0) != 0,
            chapter_group: row.get::<_, String>("chapter_group").unwrap_or_default(),
        })
    }
}

/// Per-text statistics for library display.
#[derive(Debug, Clone, Default)]
pub struct TextStats {
    pub total_tokens: usize,
    pub unique_vocab: usize,
    pub known_count: usize,
    pub learning_count: usize,
    pub new_count: usize,
}

// ─── Additional CRUD Operations ──────────────────────────────────────

/// Delete a text and all its cascaded data (paragraphs, tokens).
pub fn delete_text(conn: &Connection, text_id: i64) -> Result<()> {
    conn.execute("DELETE FROM texts WHERE id = ?1", params![text_id])?;
    Ok(())
}

/// Delete a web source, all its chapters, and all associated texts.
pub fn delete_web_source(conn: &Connection, source_id: i64) -> Result<()> {
    // First delete all texts that belong to this source's chapters
    let chapter_text_ids: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT text_id FROM web_source_chapters WHERE web_source_id = ?1 AND text_id IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![source_id], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    for text_id in chapter_text_ids {
        conn.execute("DELETE FROM texts WHERE id = ?1", params![text_id])?;
    }
    // Delete chapters
    conn.execute(
        "DELETE FROM web_source_chapters WHERE web_source_id = ?1",
        params![source_id],
    )?;
    // Delete the source itself
    conn.execute("DELETE FROM web_sources WHERE id = ?1", params![source_id])?;
    Ok(())
}

/// List all texts, ordered by created_at descending.
pub fn list_all_texts(conn: &Connection) -> Result<Vec<Text>> {
    let mut stmt = conn.prepare("SELECT * FROM texts ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], Text::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get per-text vocabulary statistics.
pub fn get_text_stats(conn: &Connection, text_id: i64) -> Result<TextStats> {
    // Total non-trivial tokens
    let total_tokens: usize = conn.query_row(
        "SELECT COUNT(*) FROM tokens t
         JOIN paragraphs p ON t.paragraph_id = p.id
         WHERE p.text_id = ?1 AND t.pos NOT IN ('Symbol','Punctuation','Whitespace','BOS/EOS','','Particle','Auxiliary','Conjunction','Prefix')",
        params![text_id],
        |r| r.get(0),
    ).unwrap_or(0);

    // Unique vocabulary with status breakdown
    let mut stmt = conn.prepare(
        "SELECT v.status, COUNT(DISTINCT v.id) FROM vocabulary v
         JOIN tokens t ON t.base_form = v.base_form AND t.reading = v.reading
         JOIN paragraphs p ON t.paragraph_id = p.id
         WHERE p.text_id = ?1
         GROUP BY v.status",
    )?;
    let rows = stmt.query_map(params![text_id], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, usize>(1)?))
    })?;

    let mut stats = TextStats {
        total_tokens,
        ..Default::default()
    };
    for row in rows.flatten() {
        let (status_val, count) = row;
        stats.unique_vocab += count;
        match VocabularyStatus::from_i32(status_val) {
            VocabularyStatus::Known => stats.known_count += count,
            VocabularyStatus::Learning1
            | VocabularyStatus::Learning2
            | VocabularyStatus::Learning3
            | VocabularyStatus::Learning4 => stats.learning_count += count,
            VocabularyStatus::New => stats.new_count += count,
            _ => {}
        }
    }

    Ok(stats)
}

/// Upsert a web source. Returns the id.
pub fn upsert_web_source(
    conn: &Connection,
    source_type: &str,
    external_id: &str,
    title: &str,
    metadata_json: &str,
) -> Result<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM web_sources WHERE source_type = ?1 AND external_id = ?2",
            params![source_type, external_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        conn.execute(
            "UPDATE web_sources SET title = ?1, metadata_json = ?2, last_synced = datetime('now') WHERE id = ?3",
            params![title, metadata_json, id],
        )?;
        return Ok(id);
    }

    conn.execute(
        "INSERT INTO web_sources (source_type, external_id, title, metadata_json) VALUES (?1, ?2, ?3, ?4)",
        params![source_type, external_id, title, metadata_json],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert a web source chapter.
pub fn insert_web_source_chapter(
    conn: &Connection,
    web_source_id: i64,
    chapter_number: i32,
    title: &str,
    text_id: Option<i64>,
    word_count: i32,
    chapter_group: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO web_source_chapters (web_source_id, chapter_number, title, text_id, word_count, chapter_group)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(web_source_id, chapter_number) DO UPDATE SET
           title = excluded.title,
           chapter_group = excluded.chapter_group",
        params![web_source_id, chapter_number, title, text_id, word_count, chapter_group],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List chapters for a web source.
pub fn list_chapters_by_source(
    conn: &Connection,
    web_source_id: i64,
) -> Result<Vec<WebSourceChapter>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM web_source_chapters WHERE web_source_id = ?1 ORDER BY chapter_number",
    )?;
    let rows = stmt.query_map(params![web_source_id], WebSourceChapter::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Search texts by title (case-insensitive LIKE).
pub fn search_texts(conn: &Connection, query: &str) -> Result<Vec<Text>> {
    let pattern = format!("%{}%", query);
    let mut stmt =
        conn.prepare("SELECT * FROM texts WHERE title LIKE ?1 ORDER BY created_at DESC")?;
    let rows = stmt.query_map(params![pattern], Text::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// List texts filtered by source_type.
pub fn list_texts_by_source_type(conn: &Connection, source_type: &str) -> Result<Vec<Text>> {
    let mut stmt =
        conn.prepare("SELECT * FROM texts WHERE source_type = ?1 ORDER BY created_at DESC")?;
    let rows = stmt.query_map(params![source_type], Text::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Save reading progress (sentence index) for a text, updating last_read_at.
pub fn save_reading_progress(conn: &Connection, text_id: i64, sentence_index: usize) -> Result<()> {
    conn.execute(
        "UPDATE texts SET last_sentence_index = ?1, last_read_at = datetime('now') WHERE id = ?2",
        params![sentence_index as i64, text_id],
    )?;
    Ok(())
}

/// Update total_sentences for a text.
pub fn update_total_sentences(conn: &Connection, text_id: i64, total: usize) -> Result<()> {
    conn.execute(
        "UPDATE texts SET total_sentences = ?1 WHERE id = ?2",
        params![total as i64, text_id],
    )?;
    Ok(())
}

/// Touch last_read_at for a text.
pub fn touch_last_read(conn: &Connection, text_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE texts SET last_read_at = datetime('now') WHERE id = ?1",
        params![text_id],
    )?;
    Ok(())
}

/// List recently read texts (with last_read_at set, unfinished).
pub fn list_recent_texts(conn: &Connection, limit: usize) -> Result<Vec<Text>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM texts WHERE last_read_at IS NOT NULL ORDER BY last_read_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], Text::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// List all web sources.
pub fn list_web_sources(conn: &Connection) -> Result<Vec<WebSource>> {
    let mut stmt = conn.prepare("SELECT * FROM web_sources ORDER BY last_synced DESC")?;
    let rows = stmt.query_map([], WebSource::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get a web source by id.
pub fn get_web_source_by_id(conn: &Connection, id: i64) -> Result<Option<WebSource>> {
    let mut stmt = conn.prepare("SELECT * FROM web_sources WHERE id = ?1")?;
    let mut rows = stmt.query_map(params![id], WebSource::from_row)?;
    Ok(rows.next().transpose()?)
}

/// Toggle is_skipped on a chapter.
pub fn toggle_chapter_skip(conn: &Connection, chapter_id: i64) -> Result<bool> {
    let current: i32 = conn
        .query_row(
            "SELECT is_skipped FROM web_source_chapters WHERE id = ?1",
            params![chapter_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let new_val = if current == 0 { 1 } else { 0 };
    conn.execute(
        "UPDATE web_source_chapters SET is_skipped = ?1 WHERE id = ?2",
        params![new_val, chapter_id],
    )?;
    Ok(new_val != 0)
}

/// Update the text_id of a web source chapter.
pub fn update_chapter_text_id(conn: &Connection, chapter_id: i64, text_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE web_source_chapters SET text_id = ?1 WHERE id = ?2",
        params![text_id, chapter_id],
    )?;
    Ok(())
}

/// Get chapter count and imported count for a web source.
pub fn get_source_chapter_counts(
    conn: &Connection,
    source_id: i64,
) -> Result<(usize, usize, usize)> {
    let total: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM web_source_chapters WHERE web_source_id = ?1",
            params![source_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let imported: usize = conn.query_row(
        "SELECT COUNT(*) FROM web_source_chapters WHERE web_source_id = ?1 AND text_id IS NOT NULL",
        params![source_id], |r| r.get(0),
    ).unwrap_or(0);
    let skipped: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM web_source_chapters WHERE web_source_id = ?1 AND is_skipped = 1",
            params![source_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok((total, imported, skipped))
}

/// List chapters for a web source with pagination.
pub fn list_chapters_paginated(
    conn: &Connection,
    source_id: i64,
    offset: usize,
    limit: usize,
) -> Result<Vec<WebSourceChapter>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM web_source_chapters WHERE web_source_id = ?1 ORDER BY chapter_number LIMIT ?2 OFFSET ?3"
    )?;
    let rows = stmt.query_map(
        params![source_id, limit as i64, offset as i64],
        WebSourceChapter::from_row,
    )?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Find a web source by source_type and external_id.
pub fn find_web_source(
    conn: &Connection,
    source_type: &str,
    external_id: &str,
) -> Result<Option<WebSource>> {
    let mut stmt =
        conn.prepare("SELECT * FROM web_sources WHERE source_type = ?1 AND external_id = ?2")?;
    let mut rows = stmt.query_map(params![source_type, external_id], WebSource::from_row)?;
    Ok(rows.next().transpose()?)
}

/// Get saved reading progress for a text.
pub fn get_reading_progress(conn: &Connection, text_id: i64) -> Result<usize> {
    let idx: i64 = conn
        .query_row(
            "SELECT last_sentence_index FROM texts WHERE id = ?1",
            params![text_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(idx as usize)
}

/// Find the chapter that owns a given text_id, along with the next non-skipped chapter.
/// Returns (current_chapter, next_chapter, source_id).
pub fn find_next_chapter_for_text(
    conn: &Connection,
    text_id: i64,
) -> Result<Option<(WebSourceChapter, Option<WebSourceChapter>, i64)>> {
    // Find the chapter that references this text
    let current: Option<WebSourceChapter> = conn
        .query_row(
            "SELECT * FROM web_source_chapters WHERE text_id = ?1",
            params![text_id],
            WebSourceChapter::from_row,
        )
        .ok();

    let current = match current {
        Some(c) => c,
        None => return Ok(None),
    };

    let source_id = current.web_source_id;
    let current_number = current.chapter_number;

    // Find the next non-skipped chapter by chapter_number
    let next: Option<WebSourceChapter> = conn
        .query_row(
            "SELECT * FROM web_source_chapters
             WHERE web_source_id = ?1 AND chapter_number > ?2 AND is_skipped = 0
             ORDER BY chapter_number ASC LIMIT 1",
            params![source_id, current_number],
            WebSourceChapter::from_row,
        )
        .ok();

    Ok(Some((current, next, source_id)))
}

/// List texts that don't belong to any web_source (standalone texts).
pub fn list_standalone_texts(conn: &Connection) -> Result<Vec<Text>> {
    // Texts whose id is NOT referenced by any web_source_chapters.text_id
    let mut stmt = conn.prepare(
        "SELECT t.* FROM texts t
         WHERE t.id NOT IN (SELECT wsc.text_id FROM web_source_chapters wsc WHERE wsc.text_id IS NOT NULL)
         ORDER BY t.created_at DESC"
    )?;
    let rows = stmt.query_map([], Text::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

// ─── SRS Card/Review CRUD ────────────────────────────────────────────

impl SrsCard {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            vocabulary_id: row.get("vocabulary_id")?,
            conjugation_id: row.get("conjugation_id")?,
            card_type: row.get("card_type")?,
            answer_mode: row.get("answer_mode")?,
            due_date: row.get("due_date")?,
            stability: row.get("stability")?,
            difficulty: row.get("difficulty")?,
            reps: row.get("reps")?,
            lapses: row.get("lapses")?,
            state: row.get("state")?,
            created_at: row.get("created_at")?,
            sentence_translation_id: row.get("sentence_translation_id").unwrap_or(None),
        })
    }
}

impl SrsReview {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            card_id: row.get("card_id")?,
            reviewed_at: row.get("reviewed_at")?,
            rating: row.get("rating")?,
            elapsed_ms: row.get("elapsed_ms")?,
            typed_answer: row.get("typed_answer")?,
            answer_correct: row.get::<_, i32>("answer_correct")? != 0,
        })
    }
}

impl CardType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CardType::Word => "word",
            CardType::Sentence => "sentence",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "sentence" => CardType::Sentence,
            _ => CardType::Word,
        }
    }
}

impl AnswerMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnswerMode::WordReview => "word_review",
            AnswerMode::SentenceCloze => "sentence_cloze",
            AnswerMode::SentenceFull => "sentence_full",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "sentence_cloze" => AnswerMode::SentenceCloze,
            "sentence_full" => AnswerMode::SentenceFull,
            // Map old modes to WordReview for backward compat
            _ => AnswerMode::WordReview,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            AnswerMode::WordReview => "Word Review",
            AnswerMode::SentenceCloze => "Sentence Cloze",
            AnswerMode::SentenceFull => "Sentence Full",
        }
    }
}

/// Insert a new SRS card. Returns the card id.
pub fn insert_srs_card(conn: &Connection, vocabulary_id: i64, card_type: &CardType) -> Result<i64> {
    let answer_mode = match card_type {
        CardType::Word => AnswerMode::WordReview,
        CardType::Sentence => AnswerMode::SentenceFull,
    };
    conn.execute(
        "INSERT INTO srs_cards (vocabulary_id, card_type, answer_mode, due_date, state)
         VALUES (?1, ?2, ?3, datetime('now'), 'new')",
        params![vocabulary_id, card_type.as_str(), answer_mode.as_str()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Check if a word card already exists for a vocabulary item.
pub fn has_word_card(conn: &Connection, vocabulary_id: i64) -> Result<bool> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM srs_cards WHERE vocabulary_id = ?1 AND card_type = 'word' AND state != 'retired'",
        params![vocabulary_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Check if a sentence card exists for a vocabulary item with a given sentence text hash.
pub fn has_sentence_card_for_vocab(conn: &Connection, vocabulary_id: i64) -> Result<bool> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM srs_cards WHERE vocabulary_id = ?1 AND card_type = 'sentence' AND state != 'retired'",
        params![vocabulary_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Get all due SRS cards (due_date <= now, not retired), ordered by due date.
pub fn get_due_cards(conn: &Connection, limit: usize) -> Result<Vec<SrsCard>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM srs_cards
         WHERE due_date <= datetime('now') AND state != 'retired'
         ORDER BY due_date ASC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], SrsCard::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get count of due cards and new cards.
pub fn get_due_card_counts(conn: &Connection) -> Result<(usize, usize)> {
    let due: usize = conn.query_row(
        "SELECT COUNT(*) FROM srs_cards WHERE due_date <= datetime('now') AND state != 'retired'",
        [],
        |r| r.get(0),
    )?;
    let new: usize = conn.query_row(
        "SELECT COUNT(*) FROM srs_cards WHERE state = 'new' AND due_date <= datetime('now')",
        [],
        |r| r.get(0),
    )?;
    Ok((due, new))
}

/// Count how many new cards were introduced (first reviewed) today.
/// A card is "introduced" when it receives its first-ever review.
pub fn get_new_cards_introduced_today(conn: &Connection) -> Result<usize> {
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT card_id
             FROM srs_reviews
             GROUP BY card_id
             HAVING MIN(reviewed_at) >= date('now')
         )",
        [],
        |r| r.get(0),
    )?;
    Ok(count)
}

/// Get an SRS card by id.
pub fn get_srs_card(conn: &Connection, id: i64) -> Result<Option<SrsCard>> {
    let mut stmt = conn.prepare("SELECT * FROM srs_cards WHERE id = ?1")?;
    let mut rows = stmt.query_map(params![id], SrsCard::from_row)?;
    Ok(rows.next().transpose()?)
}

/// Update an SRS card's FSRS state after a review.
pub fn update_srs_card_state(
    conn: &Connection,
    card_id: i64,
    stability: f64,
    difficulty: f64,
    due_date: &str,
    reps: i32,
    lapses: i32,
    state: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE srs_cards SET stability = ?1, difficulty = ?2, due_date = ?3, reps = ?4, lapses = ?5, state = ?6
         WHERE id = ?7",
        params![stability, difficulty, due_date, reps, lapses, state, card_id],
    )?;
    Ok(())
}

/// Retire an SRS card (set state to 'retired').
pub fn retire_srs_card(conn: &Connection, card_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE srs_cards SET state = 'retired' WHERE id = ?1",
        params![card_id],
    )?;
    Ok(())
}

/// Retire all active SRS cards for a vocabulary item.
pub fn retire_cards_for_vocabulary(conn: &Connection, vocabulary_id: i64) -> Result<usize> {
    let count = conn.execute(
        "UPDATE srs_cards SET state = 'retired' WHERE vocabulary_id = ?1 AND state != 'retired'",
        params![vocabulary_id],
    )?;
    Ok(count)
}

/// Insert a review log entry.
pub fn insert_srs_review(
    conn: &Connection,
    card_id: i64,
    rating: i32,
    elapsed_ms: i64,
    typed_answer: Option<&str>,
    answer_correct: bool,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO srs_reviews (card_id, rating, elapsed_ms, typed_answer, answer_correct)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            card_id,
            rating,
            elapsed_ms,
            typed_answer,
            answer_correct as i32
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get review history for a card (for FSRS state computation).
pub fn get_card_reviews(conn: &Connection, card_id: i64) -> Result<Vec<SrsReview>> {
    let mut stmt =
        conn.prepare("SELECT * FROM srs_reviews WHERE card_id = ?1 ORDER BY reviewed_at ASC")?;
    let rows = stmt.query_map(params![card_id], SrsReview::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get total review count and accuracy for session summary.
pub fn get_review_session_stats(conn: &Connection) -> Result<(usize, usize)> {
    // Today's reviews
    let total: usize = conn.query_row(
        "SELECT COUNT(*) FROM srs_reviews WHERE reviewed_at >= date('now')",
        [],
        |r| r.get(0),
    )?;
    let correct: usize = conn.query_row(
        "SELECT COUNT(*) FROM srs_reviews WHERE reviewed_at >= date('now') AND answer_correct = 1",
        [],
        |r| r.get(0),
    )?;
    Ok((total, correct))
}

/// Get the sentence text for a card (reconstructed from vocabulary context).
/// Returns the sentence from the first text where this vocabulary appears.
pub fn get_card_sentence_context(conn: &Connection, vocabulary_id: i64) -> Result<Option<String>> {
    // Find a sentence containing this vocabulary item
    let result: Option<String> = conn
        .query_row(
            "SELECT GROUP_CONCAT(t2.surface, '')
             FROM tokens t1
             JOIN paragraphs p ON t1.paragraph_id = p.id
             JOIN tokens t2 ON t2.paragraph_id = t1.paragraph_id AND t2.sentence_index = t1.sentence_index
             JOIN vocabulary v ON t1.base_form = v.base_form AND t1.reading = v.reading
             WHERE v.id = ?1
             GROUP BY t1.paragraph_id, t1.sentence_index
             ORDER BY t1.id ASC
             LIMIT 1",
            params![vocabulary_id],
            |row| row.get(0),
        )
        .ok();
    Ok(result)
}

/// Get sentence tokens for a vocabulary item (for rendering sentence context in review).
/// Returns tokens from the first sentence containing this vocabulary item.
pub fn get_sentence_tokens_for_vocab(conn: &Connection, vocabulary_id: i64) -> Result<Vec<Token>> {
    // Find the first sentence containing this vocabulary
    let sentence_info: Option<(i64, i32)> = conn
        .query_row(
            "SELECT t.paragraph_id, t.sentence_index
         FROM tokens t
         JOIN vocabulary v ON t.base_form = v.base_form AND t.reading = v.reading
         WHERE v.id = ?1
         ORDER BY t.id ASC
         LIMIT 1",
            params![vocabulary_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    match sentence_info {
        Some((paragraph_id, sentence_index)) => {
            let mut stmt = conn.prepare(
                "SELECT * FROM tokens WHERE paragraph_id = ?1 AND sentence_index = ?2 ORDER BY position",
            )?;
            let rows = stmt.query_map(params![paragraph_id, sentence_index], Token::from_row)?;
            Ok(rows.filter_map(|r| r.ok()).collect())
        }
        None => Ok(vec![]),
    }
}

// ─── User Expression CRUD ────────────────────────────────────────────

/// Insert or update a user expression. Returns the expression id.
pub fn upsert_user_expression(
    conn: &Connection,
    surface: &str,
    reading: &str,
    gloss: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO user_expressions (surface, reading, gloss)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(surface) DO UPDATE SET
             reading = excluded.reading,
             gloss = excluded.gloss,
             updated_at = datetime('now')",
        params![surface, reading, gloss],
    )?;
    let id = conn.query_row(
        "SELECT id FROM user_expressions WHERE surface = ?1",
        params![surface],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// List all user expressions (for MWE detection cache).
pub fn list_user_expressions(conn: &Connection) -> Result<Vec<UserExpression>> {
    let mut stmt = conn.prepare("SELECT * FROM user_expressions ORDER BY length(surface) DESC")?;
    let rows = stmt.query_map([], UserExpression::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Delete a user expression by id.
pub fn delete_user_expression(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM user_expressions WHERE id = ?1", params![id])?;
    Ok(())
}

// ─── Sentence Translation Model & CRUD ───────────────────────────────

/// A sentence translation (user-provided, LLM, or API).
#[derive(Debug, Clone)]
pub struct SentenceTranslation {
    pub id: i64,
    pub text_id: i64,
    pub sentence_index: i64,
    pub sentence_text: String,
    pub translation: String,
    pub explanation: Option<String>,
    pub source: String,
    pub model: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl SentenceTranslation {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            text_id: row.get("text_id")?,
            sentence_index: row.get("sentence_index")?,
            sentence_text: row.get("sentence_text")?,
            translation: row.get("translation")?,
            explanation: row.get("explanation")?,
            source: row.get("source")?,
            model: row.get("model")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

/// Upsert a sentence translation. Returns the id.
pub fn upsert_sentence_translation(
    conn: &Connection,
    text_id: i64,
    sentence_index: i64,
    sentence_text: &str,
    translation: &str,
    explanation: Option<&str>,
    source: &str,
    model: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO sentence_translations (text_id, sentence_index, sentence_text, translation, explanation, source, model)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(text_id, sentence_index) DO UPDATE SET
             translation = excluded.translation,
             explanation = excluded.explanation,
             source = excluded.source,
             model = excluded.model,
             updated_at = datetime('now')",
        params![text_id, sentence_index, sentence_text, translation, explanation, source, model],
    )?;
    let id = conn.query_row(
        "SELECT id FROM sentence_translations WHERE text_id = ?1 AND sentence_index = ?2",
        params![text_id, sentence_index],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// Get all sentence translations for a text.
pub fn get_sentence_translations(
    conn: &Connection,
    text_id: i64,
) -> Result<Vec<SentenceTranslation>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM sentence_translations WHERE text_id = ?1 ORDER BY sentence_index",
    )?;
    let rows = stmt.query_map(params![text_id], SentenceTranslation::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get a sentence translation by text_id and sentence_index.
pub fn get_sentence_translation(
    conn: &Connection,
    text_id: i64,
    sentence_index: i64,
) -> Result<Option<SentenceTranslation>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM sentence_translations WHERE text_id = ?1 AND sentence_index = ?2",
    )?;
    let mut rows = stmt.query_map(
        params![text_id, sentence_index],
        SentenceTranslation::from_row,
    )?;
    Ok(rows.next().transpose()?)
}

/// Update vocabulary translation.
pub fn update_vocabulary_translation(
    conn: &Connection,
    id: i64,
    translation: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE vocabulary SET translation = ?1, updated_at = datetime('now') WHERE id = ?2",
        params![translation, id],
    )?;
    Ok(())
}

/// Insert an SRS card for a sentence translation. Returns the card id.
pub fn insert_sentence_full_card(conn: &Connection, sentence_translation_id: i64) -> Result<i64> {
    conn.execute(
        "INSERT INTO srs_cards (vocabulary_id, card_type, answer_mode, sentence_translation_id, due_date, state)
         VALUES (NULL, 'sentence', 'sentence_full', ?1, datetime('now'), 'new')",
        params![sentence_translation_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Check if a sentence_full card already exists for a sentence translation.
pub fn has_sentence_full_card(conn: &Connection, sentence_translation_id: i64) -> Result<bool> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM srs_cards WHERE sentence_translation_id = ?1 AND answer_mode = 'sentence_full' AND state != 'retired'",
        params![sentence_translation_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Get a sentence translation by id.
pub fn get_sentence_translation_by_id(
    conn: &Connection,
    id: i64,
) -> Result<Option<SentenceTranslation>> {
    let mut stmt = conn.prepare("SELECT * FROM sentence_translations WHERE id = ?1")?;
    let mut rows = stmt.query_map(params![id], SentenceTranslation::from_row)?;
    Ok(rows.next().transpose()?)
}

/// Get all SRS cards (for card browser). Includes retired cards.
pub fn get_all_srs_cards(conn: &Connection, limit: usize) -> Result<Vec<SrsCard>> {
    let mut stmt = conn.prepare("SELECT * FROM srs_cards ORDER BY due_date ASC LIMIT ?1")?;
    let rows = stmt.query_map(params![limit as i64], SrsCard::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Update the answer_mode of an SRS card.
pub fn update_card_answer_mode(conn: &Connection, card_id: i64, answer_mode: &str) -> Result<()> {
    conn.execute(
        "UPDATE srs_cards SET answer_mode = ?1 WHERE id = ?2",
        params![answer_mode, card_id],
    )?;
    Ok(())
}

/// Reset an SRS card to new state with due_date = now.
pub fn reset_srs_card(conn: &Connection, card_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE srs_cards SET state = 'new', stability = 0.0, difficulty = 0.0, reps = 0, lapses = 0, due_date = datetime('now') WHERE id = ?1",
        params![card_id],
    )?;
    Ok(())
}

/// Delete an SRS card permanently.
pub fn delete_srs_card(conn: &Connection, card_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM srs_reviews WHERE card_id = ?1",
        params![card_id],
    )?;
    conn.execute("DELETE FROM srs_cards WHERE id = ?1", params![card_id])?;
    Ok(())
}

// ─── Vocabulary Sentences CRUD ────────────────────────────────────────

/// Add a sentence context to a vocabulary item (deduplicates automatically via UNIQUE index).
pub fn add_vocabulary_sentence(
    conn: &Connection,
    vocabulary_id: i64,
    paragraph_id: i64,
    sentence_index: i32,
) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO vocabulary_sentences (vocabulary_id, paragraph_id, sentence_index)
         VALUES (?1, ?2, ?3)",
        params![vocabulary_id, paragraph_id, sentence_index],
    )?;
    Ok(())
}

/// Get a random sentence's tokens for a vocabulary item (from all collected sentences).
/// Returns tokens from a randomly chosen sentence where the word appears.
pub fn get_random_sentence_tokens_for_vocab(
    conn: &Connection,
    vocabulary_id: i64,
) -> Result<Vec<Token>> {
    // Pick a random sentence from the vocabulary_sentences table
    let sentence_info: Option<(i64, i32)> = conn
        .query_row(
            "SELECT paragraph_id, sentence_index
             FROM vocabulary_sentences
             WHERE vocabulary_id = ?1
             ORDER BY RANDOM()
             LIMIT 1",
            params![vocabulary_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    // Fall back to the old method if no collected sentences
    let (paragraph_id, sentence_index) = match sentence_info {
        Some(info) => info,
        None => {
            // Fallback: find from tokens table directly
            let fallback: Option<(i64, i32)> = conn
                .query_row(
                    "SELECT t.paragraph_id, t.sentence_index
                     FROM tokens t
                     JOIN vocabulary v ON t.base_form = v.base_form AND t.reading = v.reading
                     WHERE v.id = ?1
                     ORDER BY t.id ASC
                     LIMIT 1",
                    params![vocabulary_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();
            match fallback {
                Some(info) => info,
                None => return Ok(vec![]),
            }
        }
    };

    let mut stmt = conn.prepare(
        "SELECT * FROM tokens WHERE paragraph_id = ?1 AND sentence_index = ?2 ORDER BY position",
    )?;
    let rows = stmt.query_map(params![paragraph_id, sentence_index], Token::from_row)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get the count of collected sentences for a vocabulary item.
pub fn get_vocabulary_sentence_count(conn: &Connection, vocabulary_id: i64) -> Result<usize> {
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM vocabulary_sentences WHERE vocabulary_id = ?1",
        params![vocabulary_id],
        |r| r.get(0),
    )?;
    Ok(count)
}

/// Get all collected sentence texts for a vocabulary item.
/// Returns the surface text of each sentence (concatenated token surfaces).
pub fn get_all_vocabulary_sentence_texts(
    conn: &Connection,
    vocabulary_id: i64,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT paragraph_id, sentence_index FROM vocabulary_sentences WHERE vocabulary_id = ?1",
    )?;
    let rows: Vec<(i64, i32)> = stmt
        .query_map(params![vocabulary_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut sentences = Vec::new();
    for (para_id, sent_idx) in rows {
        let mut tok_stmt = conn.prepare(
            "SELECT surface FROM tokens WHERE paragraph_id = ?1 AND sentence_index = ?2 ORDER BY position",
        )?;
        let text: String = tok_stmt
            .query_map(params![para_id, sent_idx], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        if !text.is_empty() {
            sentences.push(text);
        }
    }
    Ok(sentences)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection, schema};

    fn setup() -> Connection {
        let conn = connection::open_in_memory().unwrap();
        schema::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_text_crud() {
        let conn = setup();
        let id = insert_text(&conn, "Test Title", "Some content", "text", None).unwrap();
        let text = get_text_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(text.title, "Test Title");
        assert_eq!(text.content, "Some content");
    }

    #[test]
    fn test_vocabulary_upsert() {
        let conn = setup();
        let id1 = upsert_vocabulary(&conn, "食べる", "たべる", "verb").unwrap();
        let id2 = upsert_vocabulary(&conn, "食べる", "たべる", "verb").unwrap();
        assert_eq!(id1, id2, "Upsert should return same id for duplicate");

        let vocab = get_vocabulary_by_id(&conn, id1).unwrap().unwrap();
        assert_eq!(vocab.status, VocabularyStatus::New);

        update_vocabulary_status(&conn, id1, VocabularyStatus::Learning1).unwrap();
        let vocab = get_vocabulary_by_id(&conn, id1).unwrap().unwrap();
        assert_eq!(vocab.status, VocabularyStatus::Learning1);
    }

    #[test]
    fn test_conjugation_encounter_upsert() {
        let conn = setup();
        let vocab_id = upsert_vocabulary(&conn, "食べる", "たべる", "verb").unwrap();
        let ce1 =
            upsert_conjugation_encounter(&conn, vocab_id, "食べた", "past", "ta-form").unwrap();
        let ce2 =
            upsert_conjugation_encounter(&conn, vocab_id, "食べた", "past", "ta-form").unwrap();
        assert_eq!(ce1, ce2);

        // Check count was incremented
        let count: i32 = conn
            .query_row(
                "SELECT encounter_count FROM conjugation_encounters WHERE id = ?1",
                params![ce1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
}
