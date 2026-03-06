use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
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
/// Returns the text_id. Shows a progress bar in the terminal.
pub fn import_text(
    title: &str,
    content: &str,
    source_type: &str,
    source_url: Option<&str>,
    conn: &Connection,
) -> Result<i64> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} paragraphs — {msg}",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message(format!("Importing \"{}\"...", title));

    let result =
        import_text_with_progress(title, content, source_type, source_url, conn, Some(&pb));

    match &result {
        Ok(_) => pb.finish_with_message(format!("\"{}\" imported successfully", title)),
        Err(e) => pb.finish_with_message(format!("Import failed: {}", e)),
    }

    result
}

/// Import text without any progress bar (for use from TUI or tests).
pub fn import_text_quiet(
    title: &str,
    content: &str,
    source_type: &str,
    source_url: Option<&str>,
    conn: &Connection,
) -> Result<i64> {
    import_text_with_progress(title, content, source_type, source_url, conn, None)
}

/// Core import function with optional progress bar.
pub fn import_text_with_progress(
    title: &str,
    content: &str,
    source_type: &str,
    source_url: Option<&str>,
    conn: &Connection,
    progress: Option<&ProgressBar>,
) -> Result<i64> {
    // Wrap entire import in a transaction
    conn.execute_batch("BEGIN TRANSACTION;")?;

    let result = import_text_inner(title, content, source_type, source_url, conn, progress);

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
    progress: Option<&ProgressBar>,
) -> Result<i64> {
    let text_id = models::insert_text(conn, title, content, source_type, source_url)?;
    let tok = tokenizer::create_tokenizer()?;

    let paragraphs = tokenizer::split_paragraphs(content);
    let mut total_tokens = 0usize;
    let mut new_vocab = 0usize;
    let mut para_count = 0usize;

    if let Some(pb) = progress {
        pb.set_length(paragraphs.len() as u64);
        pb.set_position(0);
    }

    for (para_idx, para_text) in paragraphs.iter().enumerate() {
        let para_id = models::insert_paragraph(conn, text_id, para_idx as i32, para_text)?;
        para_count += 1;

        // Split paragraph into sentences and tokenize each
        let sentences = tokenizer::split_sentences(para_text);
        let mut token_position = 0i32;

        for (sent_idx, sentence) in sentences.iter().enumerate() {
            let tokens = tokenizer::tokenize_with(&tok, sentence)?;

            for token_info in &tokens {
                models::insert_token(
                    conn,
                    para_id,
                    token_position,
                    &token_info.surface,
                    &token_info.base_form,
                    &token_info.reading,
                    &token_info.surface_reading,
                    &token_info.pos,
                    &token_info.conjugation_form,
                    &token_info.conjugation_type,
                    sent_idx as i32,
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

                    // Auto-ignore particles, auxiliaries, conjunctions, prefixes, suffixes,
                    // numbers, and ASCII-only tokens (English text, punctuation)
                    let is_numeric = token_info.surface.trim().chars().all(|c| {
                        c.is_ascii_digit() || c == '.' || c == ',' || ('０'..='９').contains(&c)
                    }) && !token_info.surface.trim().is_empty();
                    let is_ascii = !token_info.surface.trim().is_empty()
                        && token_info.surface.trim().chars().all(|c| c.is_ascii());
                    let auto_ignore = matches!(
                        token_info.pos.as_str(),
                        "Particle" | "Auxiliary" | "Conjunction" | "Prefix"
                    ) || is_numeric
                        || is_ascii;
                    if auto_ignore {
                        // Only set to Ignored if still New (don't override user choices)
                        if let Ok(Some(vocab)) = models::get_vocabulary_by_id(conn, vocab_id) {
                            if vocab.status == models::VocabularyStatus::New {
                                models::update_vocabulary_status(
                                    conn,
                                    vocab_id,
                                    models::VocabularyStatus::Ignored,
                                )?;
                            }
                        }
                    }

                    // Check if this is a new vocabulary entry
                    if let Ok(Some(vocab)) = models::get_vocabulary_by_id(conn, vocab_id) {
                        if vocab.status == models::VocabularyStatus::New {
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

        if let Some(pb) = progress {
            pb.set_position((para_idx + 1) as u64);
            pb.set_message(format!(
                "\"{}\" — {} tokens, {} new vocab",
                title, total_tokens, new_vocab
            ));
        }
    }

    // Note: don't println here — it corrupts the TUI. CLI callers use
    // import_text() which has its own progress bar with finish message.

    Ok(text_id)
}

/// Pre-tokenized paragraph data for two-phase import.
pub struct PreTokenizedParagraph {
    pub position: i32,
    pub content: String,
    pub sentences: Vec<PreTokenizedSentence>,
}

/// Pre-tokenized sentence data.
pub struct PreTokenizedSentence {
    pub sentence_index: i32,
    pub tokens: Vec<tokenizer::TokenInfo>,
}

/// Phase 1: Tokenize text in memory without any DB access.
/// Returns pre-tokenized data that can be written to DB later.
/// This is safe to call from multiple threads simultaneously.
pub fn pretokenize_text(content: &str) -> Result<Vec<PreTokenizedParagraph>> {
    pretokenize_text_with_progress(content, &|_, _| {})
}

/// Phase 1 with progress callback: `on_progress(paragraphs_done, paragraphs_total)`.
pub fn pretokenize_text_with_progress(
    content: &str,
    on_progress: &dyn Fn(usize, usize),
) -> Result<Vec<PreTokenizedParagraph>> {
    let tok = tokenizer::create_tokenizer()?;
    let paragraphs = tokenizer::split_paragraphs(content);
    let total = paragraphs.len();
    let mut result = Vec::with_capacity(total);

    for (para_idx, para_text) in paragraphs.iter().enumerate() {
        let sentences_text = tokenizer::split_sentences(para_text);
        let mut sentences = Vec::with_capacity(sentences_text.len());

        for (sent_idx, sentence) in sentences_text.iter().enumerate() {
            let tokens = tokenizer::tokenize_with(&tok, sentence)?;
            sentences.push(PreTokenizedSentence {
                sentence_index: sent_idx as i32,
                tokens,
            });
        }

        result.push(PreTokenizedParagraph {
            position: para_idx as i32,
            content: para_text.clone(),
            sentences,
        });

        on_progress(para_idx + 1, total);
    }

    Ok(result)
}

/// Phase 2: Write pre-tokenized data to DB in a single short transaction.
/// This holds the DB write lock only for the insert phase (no tokenization).
pub fn write_pretokenized(
    title: &str,
    content: &str,
    source_type: &str,
    source_url: Option<&str>,
    paragraphs: &[PreTokenizedParagraph],
    conn: &Connection,
) -> Result<i64> {
    conn.execute_batch("BEGIN TRANSACTION;")?;

    let result = (|| -> Result<i64> {
        let text_id = models::insert_text(conn, title, content, source_type, source_url)?;

        for para in paragraphs {
            let para_id = models::insert_paragraph(conn, text_id, para.position, &para.content)?;
            let mut token_position = 0i32;

            for sentence in &para.sentences {
                for token_info in &sentence.tokens {
                    models::insert_token(
                        conn,
                        para_id,
                        token_position,
                        &token_info.surface,
                        &token_info.base_form,
                        &token_info.reading,
                        &token_info.surface_reading,
                        &token_info.pos,
                        &token_info.conjugation_form,
                        &token_info.conjugation_type,
                        sentence.sentence_index,
                    )?;
                    token_position += 1;

                    if !token_info.is_trivial() && !token_info.base_form.is_empty() {
                        let vocab_id = models::upsert_vocabulary(
                            conn,
                            &token_info.base_form,
                            &token_info.reading,
                            &token_info.pos,
                        )?;

                        let is_numeric = token_info.surface.trim().chars().all(|c| {
                            c.is_ascii_digit() || c == '.' || c == ',' || ('０'..='９').contains(&c)
                        }) && !token_info.surface.trim().is_empty();
                        let is_ascii = !token_info.surface.trim().is_empty()
                            && token_info.surface.trim().chars().all(|c| c.is_ascii());
                        let auto_ignore = matches!(
                            token_info.pos.as_str(),
                            "Particle" | "Auxiliary" | "Conjunction" | "Prefix"
                        ) || is_numeric
                            || is_ascii;
                        if auto_ignore {
                            if let Ok(Some(vocab)) = models::get_vocabulary_by_id(conn, vocab_id) {
                                if vocab.status == models::VocabularyStatus::New {
                                    models::update_vocabulary_status(
                                        conn,
                                        vocab_id,
                                        models::VocabularyStatus::Ignored,
                                    )?;
                                }
                            }
                        }

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

        Ok(text_id)
    })();

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

        let text_id = import_text_quiet(
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
        let text_id = import_text_quiet("Multi", content, "text", None, &conn).unwrap();

        let paragraphs = models::list_paragraphs_by_text(&conn, text_id).unwrap();
        assert_eq!(paragraphs.len(), 2, "Should have 2 paragraphs");
    }

    #[test]
    fn test_vocabulary_not_duplicated() {
        let conn = setup();

        // Import twice with same word
        import_text_quiet("Test1", "猫は猫である。", "text", None, &conn).unwrap();

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
