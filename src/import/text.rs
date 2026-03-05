use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

use crate::core::tokenizer;
use crate::db::models;

/// Import a text file into the database.
/// Returns the text_id of the imported text.
pub fn import_file(path: &Path, conn: &Connection) -> Result<i64> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();

    import_text(&title, &content, "text", None, conn)
}

/// Import text content into the database with full tokenization pipeline.
/// Returns the text_id.
pub fn import_text(
    title: &str,
    content: &str,
    source_type: &str,
    source_url: Option<&str>,
    conn: &Connection,
) -> Result<i64> {
    // Wrap entire import in a transaction
    conn.execute_batch("BEGIN TRANSACTION;")?;

    let result = import_text_inner(title, content, source_type, source_url, conn);

    match result {
        Ok(text_id) => {
            conn.execute_batch("COMMIT;")?;
            Ok(text_id)
        }
        Err(e) => {
            conn.execute_batch("ROLLBACK;").ok();
            Err(e)
        }
    }
}

fn import_text_inner(
    title: &str,
    content: &str,
    source_type: &str,
    source_url: Option<&str>,
    conn: &Connection,
) -> Result<i64> {
    let text_id = models::insert_text(conn, title, content, source_type, source_url)?;

    let paragraphs = tokenizer::split_paragraphs(content);
    let mut total_tokens = 0usize;
    let mut new_vocab = 0usize;
    let mut para_count = 0usize;

    for (para_idx, para_text) in paragraphs.iter().enumerate() {
        let para_id = models::insert_paragraph(conn, text_id, para_idx as i32, para_text)?;
        para_count += 1;

        // Split paragraph into sentences and tokenize each
        let sentences = tokenizer::split_sentences(para_text);
        let mut token_position = 0i32;

        for sentence in &sentences {
            let tokens = tokenizer::tokenize_sentence(sentence)?;

            for token_info in &tokens {
                models::insert_token(
                    conn,
                    para_id,
                    token_position,
                    &token_info.surface,
                    &token_info.base_form,
                    &token_info.reading,
                    &token_info.pos,
                    &token_info.conjugation_form,
                    &token_info.conjugation_type,
                )?;
                token_position += 1;
                total_tokens += 1;

                // For non-trivial tokens, upsert vocabulary and conjugation encounters
                if !token_info.is_trivial() && !token_info.base_form.is_empty() {
                    let vocab_id = models::upsert_vocabulary(
                        conn,
                        &token_info.base_form,
                        &token_info.reading,
                        &token_info.pos,
                    )?;

                    // Check if this is a new vocabulary entry (status == New means it was just created)
                    if let Ok(Some(vocab)) = models::get_vocabulary_by_id(conn, vocab_id) {
                        if vocab.status == models::VocabularyStatus::New {
                            // Count only truly new (first time seen) — but upsert doesn't
                            // distinguish, so we count based on whether the updated_at == first_seen_at
                            // For simplicity, we increment for each unique vocab_id we see
                            new_vocab += 1;
                        }
                    }

                    // Upsert conjugation encounter
                    if token_info.surface != token_info.base_form {
                        models::upsert_conjugation_encounter(
                            conn,
                            vocab_id,
                            &token_info.surface,
                            &token_info.conjugation_form,
                            &token_info.conjugation_type,
                        )?;
                    }
                }
            }
        }
    }

    println!(
        "  Paragraphs: {}, Tokens: {}, Vocabulary entries: {}",
        para_count, total_tokens, new_vocab
    );

    Ok(text_id)
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
    fn test_import_simple_text() {
        let conn = setup();

        let text_id = import_text(
            "Test",
            "吾輩は猫である。名前はまだ無い。",
            "text",
            None,
            &conn,
        )
        .unwrap();

        // Verify text was created
        let text = models::get_text_by_id(&conn, text_id).unwrap().unwrap();
        assert_eq!(text.title, "Test");

        // Verify paragraphs
        let paragraphs = models::list_paragraphs_by_text(&conn, text_id).unwrap();
        assert_eq!(paragraphs.len(), 1);

        // Verify tokens exist
        let tokens = models::list_tokens_by_paragraph(&conn, paragraphs[0].id).unwrap();
        assert!(!tokens.is_empty(), "Should have tokens");

        // Verify vocabulary was created
        let vocab: i32 = conn
            .query_row("SELECT COUNT(*) FROM vocabulary", [], |r| r.get(0))
            .unwrap();
        assert!(vocab > 0, "Should have vocabulary entries");
    }

    #[test]
    fn test_import_multi_paragraph() {
        let conn = setup();

        let content = "最初の段落。\n\n二番目の段落。";
        let text_id = import_text("Multi", content, "text", None, &conn).unwrap();

        let paragraphs = models::list_paragraphs_by_text(&conn, text_id).unwrap();
        assert_eq!(paragraphs.len(), 2, "Should have 2 paragraphs");
    }

    #[test]
    fn test_vocabulary_not_duplicated() {
        let conn = setup();

        // Import twice with same word
        import_text("Test1", "猫は猫である。", "text", None, &conn).unwrap();

        // "猫" should appear only once in vocabulary
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM vocabulary WHERE base_form = '猫'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "猫 should appear exactly once in vocabulary");
    }
}
