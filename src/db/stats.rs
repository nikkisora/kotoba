use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;

use super::models::{DailyActivity, VocabularyStatus};

// ─── Vocabulary Stats ────────────────────────────────────────────────

/// Count of words grouped by every VocabularyStatus level.
pub fn get_words_by_status(conn: &Connection) -> Result<HashMap<VocabularyStatus, usize>> {
    let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM vocabulary GROUP BY status")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, usize>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows.flatten() {
        let (status_val, count) = row;
        map.insert(VocabularyStatus::from_i32(status_val), count);
    }
    Ok(map)
}

/// Known word count over time — returns (date, cumulative_known) pairs.
/// Uses vocabulary.updated_at for words that reached Known status (5).
/// We build a cumulative sum: for each day a word became Known, add 1 to the running total.
pub fn get_known_words_over_time(conn: &Connection, days: usize) -> Result<Vec<(String, usize)>> {
    // Get all words that are currently Known (status=5), grouped by the date they were updated.
    // This gives us the daily *new* Known words count.
    let mut stmt = conn.prepare(
        "SELECT date(updated_at) as d, COUNT(*) as c
         FROM vocabulary
         WHERE status = 5 AND updated_at >= date('now', ?1)
         GROUP BY d
         ORDER BY d ASC",
    )?;
    let offset = format!("-{} days", days);
    let daily: Vec<(String, usize)> = stmt
        .query_map(params![offset], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Get the count of Known words *before* the window (the baseline)
    let baseline: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM vocabulary WHERE status = 5 AND updated_at < date('now', ?1)",
            params![offset],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Build cumulative series
    let mut result = Vec::new();
    let mut cumulative = baseline;
    for (date, count) in daily {
        cumulative += count;
        result.push((date, cumulative));
    }

    Ok(result)
}

// ─── Reading Activity ────────────────────────────────────────────────

/// Reading activity: sentences read per day for the last N days.
/// Re-uses the daily_activity table from Phase 5.5.
pub fn get_reading_activity(conn: &Connection, days: usize) -> Result<Vec<DailyActivity>> {
    super::models::get_daily_activity(conn, days)
}

// ─── SRS Stats ───────────────────────────────────────────────────────

/// Comprehensive SRS statistics.
#[derive(Debug, Clone, Default)]
pub struct SrsStats {
    pub due_today: usize,
    pub due_tomorrow: usize,
    pub total_reviews: usize,
    pub avg_accuracy_7d: u8,
    pub avg_accuracy_30d: u8,
    pub retention_rate: f64,
    pub total_cards: usize,
    pub cards_new: usize,
    pub cards_learning: usize,
    pub cards_review: usize,
    pub cards_retired: usize,
    pub reviews_today: usize,
}

pub fn get_srs_stats(conn: &Connection) -> Result<SrsStats> {
    let due_today: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM srs_cards WHERE due_date <= datetime('now') AND state != 'retired'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let due_tomorrow: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM srs_cards
             WHERE due_date > datetime('now') AND due_date <= datetime('now', '+1 day')
             AND state != 'retired'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_reviews: usize = conn
        .query_row("SELECT COUNT(*) FROM srs_reviews", [], |r| r.get(0))
        .unwrap_or(0);

    let reviews_today: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM srs_reviews WHERE date(reviewed_at) = date('now')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // 7-day accuracy
    let avg_accuracy_7d = accuracy_for_period(conn, 7);

    // 30-day accuracy
    let avg_accuracy_30d = accuracy_for_period(conn, 30);

    // Retention rate: proportion of review-state cards (matured) that were answered correctly
    // on their most recent review.
    let retention_rate = compute_retention_rate(conn);

    // Card counts by state
    let mut cards_new = 0usize;
    let mut cards_learning = 0usize;
    let mut cards_review = 0usize;
    let mut cards_retired = 0usize;
    let mut total_cards = 0usize;
    {
        let mut stmt = conn.prepare("SELECT state, COUNT(*) FROM srs_cards GROUP BY state")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })?;
        for row in rows.flatten() {
            let (state, count) = row;
            total_cards += count;
            match state.as_str() {
                "new" => cards_new = count,
                "learning" | "relearning" => cards_learning += count,
                "review" => cards_review = count,
                "retired" => cards_retired = count,
                _ => {}
            }
        }
    }

    Ok(SrsStats {
        due_today,
        due_tomorrow,
        total_reviews,
        avg_accuracy_7d,
        avg_accuracy_30d,
        retention_rate,
        total_cards,
        cards_new,
        cards_learning,
        cards_review,
        cards_retired,
        reviews_today,
    })
}

fn accuracy_for_period(conn: &Connection, days: i32) -> u8 {
    let offset = format!("-{} days", days);
    let total: f64 = conn
        .query_row(
            "SELECT COUNT(*) FROM srs_reviews WHERE reviewed_at >= datetime('now', ?1)",
            params![offset],
            |r| r.get(0),
        )
        .unwrap_or(0.0);
    if total == 0.0 {
        return 0;
    }
    let correct: f64 = conn
        .query_row(
            "SELECT COUNT(*) FROM srs_reviews WHERE reviewed_at >= datetime('now', ?1) AND answer_correct = 1",
            params![offset],
            |r| r.get(0),
        )
        .unwrap_or(0.0);
    (correct / total * 100.0).round() as u8
}

fn compute_retention_rate(conn: &Connection) -> f64 {
    // Retention = % of mature cards (state='review') where the most recent review was correct.
    let result: Result<(usize, usize), _> = conn.query_row(
        "SELECT
            COUNT(*),
            SUM(CASE WHEN answer_correct = 1 THEN 1 ELSE 0 END)
         FROM (
            SELECT card_id, answer_correct
            FROM srs_reviews r
            INNER JOIN srs_cards c ON r.card_id = c.id
            WHERE c.state = 'review'
            AND r.reviewed_at = (
                SELECT MAX(r2.reviewed_at) FROM srs_reviews r2 WHERE r2.card_id = r.card_id
            )
         )",
        [],
        |row| Ok((row.get::<_, usize>(0)?, row.get::<_, usize>(1)?)),
    );
    match result {
        Ok((total, correct)) if total > 0 => correct as f64 / total as f64 * 100.0,
        _ => 0.0,
    }
}

// ─── Text Coverage ───────────────────────────────────────────────────

/// Coverage statistics for a single text.
#[derive(Debug, Clone, Default)]
pub struct CoverageStats {
    pub text_id: i64,
    pub title: String,
    pub total_tokens: usize,
    pub known_tokens: usize,
    pub learning_tokens: usize,
    pub new_tokens: usize,
    pub ignored_tokens: usize,
    pub coverage_pct: f64,
}

/// Get coverage stats for a single text.
pub fn get_text_coverage(conn: &Connection, text_id: i64) -> Result<CoverageStats> {
    let title: String = conn
        .query_row(
            "SELECT title FROM texts WHERE id = ?1",
            params![text_id],
            |r| r.get(0),
        )
        .unwrap_or_default();

    // Count non-trivial tokens
    let total_tokens: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM tokens t
             JOIN paragraphs p ON t.paragraph_id = p.id
             WHERE p.text_id = ?1
             AND t.pos NOT IN ('Symbol','Punctuation','Whitespace','BOS/EOS','','Particle','Auxiliary','Conjunction','Prefix')",
            params![text_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Token counts by vocabulary status
    let mut stmt = conn.prepare(
        "SELECT
            COALESCE(v.status, 0) as status,
            COUNT(*) as cnt
         FROM tokens t
         JOIN paragraphs p ON t.paragraph_id = p.id
         LEFT JOIN vocabulary v ON t.base_form = v.base_form AND t.reading = v.reading
         WHERE p.text_id = ?1
         AND t.pos NOT IN ('Symbol','Punctuation','Whitespace','BOS/EOS','','Particle','Auxiliary','Conjunction','Prefix')
         GROUP BY status",
    )?;
    let rows = stmt.query_map(params![text_id], |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, usize>(1)?))
    })?;

    let mut known = 0usize;
    let mut learning = 0usize;
    let mut new = 0usize;
    let mut ignored = 0usize;
    for row in rows.flatten() {
        let (status_val, count) = row;
        match VocabularyStatus::from_i32(status_val) {
            VocabularyStatus::Known => known += count,
            VocabularyStatus::Learning1
            | VocabularyStatus::Learning2
            | VocabularyStatus::Learning3
            | VocabularyStatus::Learning4 => learning += count,
            VocabularyStatus::Ignored => ignored += count,
            VocabularyStatus::New => new += count,
        }
    }

    let coverage_pct = if total_tokens > 0 {
        (known + ignored) as f64 / total_tokens as f64 * 100.0
    } else {
        0.0
    };

    Ok(CoverageStats {
        text_id,
        title,
        total_tokens,
        known_tokens: known,
        learning_tokens: learning,
        new_tokens: new,
        ignored_tokens: ignored,
        coverage_pct,
    })
}

/// Get coverage stats for all imported texts (including unstarted ones).
pub fn get_all_text_coverages(conn: &Connection) -> Result<Vec<CoverageStats>> {
    let text_ids: Vec<i64> = {
        let mut stmt =
            conn.prepare("SELECT id FROM texts ORDER BY COALESCE(last_read_at, created_at) DESC")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut result = Vec::new();
    for text_id in text_ids {
        if let Ok(stats) = get_text_coverage(conn, text_id) {
            if stats.total_tokens > 0 {
                result.push(stats);
            }
        }
    }
    Ok(result)
}

// ─── Overview Stats ──────────────────────────────────────────────────

/// High-level overview statistics for the stats screen.
#[derive(Debug, Clone, Default)]
pub struct OverviewStats {
    pub texts_finished: usize,
    pub texts_reading: usize,
    pub total_vocabulary: usize,
    pub known_words: usize,
    pub learning_words: usize,
    pub new_words: usize,
    pub ignored_words: usize,
}

pub fn get_overview_stats(conn: &Connection) -> Result<OverviewStats> {
    let texts_finished: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM texts
             WHERE last_read_at IS NOT NULL
             AND total_sentences > 0
             AND last_sentence_index >= total_sentences - 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let texts_reading: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM texts
             WHERE last_read_at IS NOT NULL
             AND (total_sentences = 0 OR last_sentence_index < total_sentences - 1)",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let words_by_status = get_words_by_status(conn)?;

    let known = words_by_status
        .get(&VocabularyStatus::Known)
        .copied()
        .unwrap_or(0);
    let l1 = words_by_status
        .get(&VocabularyStatus::Learning1)
        .copied()
        .unwrap_or(0);
    let l2 = words_by_status
        .get(&VocabularyStatus::Learning2)
        .copied()
        .unwrap_or(0);
    let l3 = words_by_status
        .get(&VocabularyStatus::Learning3)
        .copied()
        .unwrap_or(0);
    let l4 = words_by_status
        .get(&VocabularyStatus::Learning4)
        .copied()
        .unwrap_or(0);
    let new = words_by_status
        .get(&VocabularyStatus::New)
        .copied()
        .unwrap_or(0);
    let ignored = words_by_status
        .get(&VocabularyStatus::Ignored)
        .copied()
        .unwrap_or(0);
    let learning = l1 + l2 + l3 + l4;
    let total = known + learning + new + ignored;

    Ok(OverviewStats {
        texts_finished,
        texts_reading,
        total_vocabulary: total,
        known_words: known,
        learning_words: learning,
        new_words: new,
        ignored_words: ignored,
    })
}
