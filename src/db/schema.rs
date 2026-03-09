use anyhow::Result;
use rusqlite::Connection;

/// All migrations in order. Each is (version, description, SQL).
const MIGRATIONS: &[(i32, &str, &str)] = &[
    (
        1,
        "Create texts, paragraphs, tokens tables",
        r#"
        CREATE TABLE IF NOT EXISTS texts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            source_url TEXT,
            source_type TEXT NOT NULL DEFAULT 'text',
            content TEXT NOT NULL,
            language TEXT NOT NULL DEFAULT 'ja',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS paragraphs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            text_id INTEGER NOT NULL REFERENCES texts(id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            content TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_paragraphs_text_id ON paragraphs(text_id);

        CREATE TABLE IF NOT EXISTS tokens (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            paragraph_id INTEGER NOT NULL REFERENCES paragraphs(id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            surface TEXT NOT NULL,
            base_form TEXT NOT NULL,
            reading TEXT NOT NULL DEFAULT '',
            pos TEXT NOT NULL DEFAULT '',
            conjugation_form TEXT NOT NULL DEFAULT '',
            conjugation_type TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_tokens_paragraph_position ON tokens(paragraph_id, position);
    "#,
    ),
    (
        2,
        "Create vocabulary table",
        r#"
        CREATE TABLE IF NOT EXISTS vocabulary (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            base_form TEXT NOT NULL,
            reading TEXT NOT NULL DEFAULT '',
            pos TEXT NOT NULL DEFAULT '',
            status INTEGER NOT NULL DEFAULT 0,
            notes TEXT,
            first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_vocabulary_base_reading ON vocabulary(base_form, reading);
    "#,
    ),
    (
        3,
        "Create conjugation_encounters table",
        r#"
        CREATE TABLE IF NOT EXISTS conjugation_encounters (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            vocabulary_id INTEGER NOT NULL REFERENCES vocabulary(id) ON DELETE CASCADE,
            surface TEXT NOT NULL,
            conjugation_form TEXT NOT NULL DEFAULT '',
            conjugation_type TEXT NOT NULL DEFAULT '',
            encounter_count INTEGER NOT NULL DEFAULT 1,
            status INTEGER NOT NULL DEFAULT 0,
            first_seen TEXT NOT NULL DEFAULT (datetime('now')),
            updated TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_conj_vocab_id ON conjugation_encounters(vocabulary_id);
    "#,
    ),
    (
        4,
        "Create srs_cards table",
        r#"
        CREATE TABLE IF NOT EXISTS srs_cards (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            vocabulary_id INTEGER REFERENCES vocabulary(id) ON DELETE CASCADE,
            conjugation_id INTEGER REFERENCES conjugation_encounters(id) ON DELETE SET NULL,
            card_type TEXT NOT NULL DEFAULT 'word',
            answer_mode TEXT NOT NULL DEFAULT 'meaning_recall',
            due_date TEXT NOT NULL DEFAULT (datetime('now')),
            stability REAL NOT NULL DEFAULT 0.0,
            difficulty REAL NOT NULL DEFAULT 0.0,
            reps INTEGER NOT NULL DEFAULT 0,
            lapses INTEGER NOT NULL DEFAULT 0,
            state TEXT NOT NULL DEFAULT 'new',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_srs_cards_due ON srs_cards(due_date);
    "#,
    ),
    (
        5,
        "Create srs_reviews table",
        r#"
        CREATE TABLE IF NOT EXISTS srs_reviews (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            card_id INTEGER NOT NULL REFERENCES srs_cards(id) ON DELETE CASCADE,
            reviewed_at TEXT NOT NULL DEFAULT (datetime('now')),
            rating INTEGER NOT NULL,
            elapsed_ms INTEGER NOT NULL DEFAULT 0,
            typed_answer TEXT,
            answer_correct INTEGER NOT NULL DEFAULT 0
        );
    "#,
    ),
    (
        6,
        "Create llm_cache table",
        r#"
        CREATE TABLE IF NOT EXISTS llm_cache (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            request_type TEXT NOT NULL DEFAULT '',
            request_hash TEXT NOT NULL,
            request_body TEXT NOT NULL DEFAULT '',
            response TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            tokens_used INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_llm_cache_hash ON llm_cache(request_hash);
    "#,
    ),
    (
        7,
        "Create JMdict tables",
        r#"
        CREATE TABLE IF NOT EXISTS jmdict_entries (
            ent_seq INTEGER PRIMARY KEY,
            json_blob TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS jmdict_kanji (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entry_id INTEGER NOT NULL REFERENCES jmdict_entries(ent_seq) ON DELETE CASCADE,
            kanji_element TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_jmdict_kanji ON jmdict_kanji(kanji_element);

        CREATE TABLE IF NOT EXISTS jmdict_readings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entry_id INTEGER NOT NULL REFERENCES jmdict_entries(ent_seq) ON DELETE CASCADE,
            reading_element TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_jmdict_readings ON jmdict_readings(reading_element);
    "#,
    ),
    (
        8,
        "Add surface_reading and sentence_index to tokens",
        r#"
        ALTER TABLE tokens ADD COLUMN surface_reading TEXT NOT NULL DEFAULT '';
        ALTER TABLE tokens ADD COLUMN sentence_index INTEGER NOT NULL DEFAULT 0;
    "#,
    ),
    (
        9,
        "Create web_sources and web_source_chapters tables",
        r#"
        CREATE TABLE IF NOT EXISTS web_sources (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_type TEXT NOT NULL,
            external_id TEXT NOT NULL,
            title TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            last_synced TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_web_sources_type_extid ON web_sources(source_type, external_id);

        CREATE TABLE IF NOT EXISTS web_source_chapters (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            web_source_id INTEGER NOT NULL REFERENCES web_sources(id) ON DELETE CASCADE,
            chapter_number INTEGER NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            text_id INTEGER REFERENCES texts(id) ON DELETE SET NULL,
            word_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_wsc_source_id ON web_source_chapters(web_source_id);
    "#,
    ),
    (
        10,
        "Add reading progress to texts",
        r#"
        ALTER TABLE texts ADD COLUMN last_sentence_index INTEGER NOT NULL DEFAULT 0;
    "#,
    ),
    (
        11,
        "Add last_read_at to texts, is_skipped to chapters, total_sentences to texts",
        r#"
        ALTER TABLE texts ADD COLUMN last_read_at TEXT;
        ALTER TABLE texts ADD COLUMN total_sentences INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE web_source_chapters ADD COLUMN is_skipped INTEGER NOT NULL DEFAULT 0;
    "#,
    ),
    (
        12,
        "Add chapter_group to web_source_chapters",
        r#"
        ALTER TABLE web_source_chapters ADD COLUMN chapter_group TEXT NOT NULL DEFAULT '';
    "#,
    ),
    (
        13,
        "Fix syosetu chapter titles to include novel name",
        r#"
        UPDATE texts SET title = ws.title || ' — ' || wsc.title
        FROM web_source_chapters wsc
        JOIN web_sources ws ON wsc.web_source_id = ws.id
        WHERE texts.id = wsc.text_id
          AND ws.source_type = 'syosetu'
          AND wsc.title != ''
          AND texts.source_type = 'syosetu';
    "#,
    ),
    (
        14,
        "Deduplicate web_source_chapters and add unique constraint",
        r#"
        -- Delete duplicate chapter rows, keeping the one with the lowest id
        -- (which preserves text_id and is_skipped from the original insert)
        DELETE FROM web_source_chapters
        WHERE id NOT IN (
            SELECT MIN(id) FROM web_source_chapters
            GROUP BY web_source_id, chapter_number
        );

        -- Add unique constraint to prevent future duplicates
        CREATE UNIQUE INDEX IF NOT EXISTS idx_wsc_unique_chapter
            ON web_source_chapters(web_source_id, chapter_number);
        "#,
    ),
    (
        15,
        "Reset Suffix vocabulary from Ignored to New",
        r#"
        -- Suffixes were previously auto-ignored during import but carry real meaning
        -- (e.g. 族, 的, 者, 性). Reset them so they appear as normal vocabulary.
        UPDATE vocabulary SET status = 0, updated_at = datetime('now')
        WHERE status = -1
          AND id IN (
            SELECT DISTINCT v.id FROM vocabulary v
            JOIN tokens t ON t.base_form = v.base_form AND t.reading = v.reading
            WHERE t.pos = 'Suffix'
          );
        "#,
    ),
    (
        16,
        "Create user_expressions table for manual MWE entries",
        r#"
        CREATE TABLE IF NOT EXISTS user_expressions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            surface TEXT NOT NULL,
            reading TEXT NOT NULL DEFAULT '',
            gloss TEXT NOT NULL DEFAULT '',
            status INTEGER NOT NULL DEFAULT 0,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_user_expr_surface ON user_expressions(surface);
        "#,
    ),
    (
        17,
        "Add translation column to vocabulary for user-provided word translations",
        r#"
        ALTER TABLE vocabulary ADD COLUMN translation TEXT DEFAULT NULL;
        "#,
    ),
    (
        18,
        "Create sentence_translations table and add sentence_translation_id to srs_cards",
        r#"
        CREATE TABLE IF NOT EXISTS sentence_translations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            text_id INTEGER NOT NULL REFERENCES texts(id) ON DELETE CASCADE,
            sentence_index INTEGER NOT NULL,
            sentence_text TEXT NOT NULL,
            translation TEXT NOT NULL DEFAULT '',
            explanation TEXT,
            source TEXT NOT NULL DEFAULT 'user',
            model TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_sentence_trans
            ON sentence_translations(text_id, sentence_index);

        ALTER TABLE srs_cards ADD COLUMN sentence_translation_id INTEGER DEFAULT NULL
            REFERENCES sentence_translations(id);
        "#,
    ),
];

/// Run all pending migrations.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // Create migrations tracking table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current_version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for &(version, description, sql) in MIGRATIONS {
        if version > current_version {
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT INTO schema_migrations (version, description) VALUES (?1, ?2)",
                rusqlite::params![version, description],
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_run_cleanly() {
        let conn = crate::db::connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        // Running again should be idempotent
        run_migrations(&conn).unwrap();

        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 18);
    }
}
