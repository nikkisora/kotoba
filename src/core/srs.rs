use anyhow::Result;
use chrono::{NaiveDateTime, Utc};
use rusqlite::Connection;

use crate::db::models::{self, AnswerMode, CardType, SrsCard, VocabularyStatus};

/// FSRS-powered spaced repetition engine.
///
/// Wraps the `fsrs` crate to provide card lifecycle management:
/// creation, scheduling, review recording, and retirement.
pub struct SrsEngine {
    fsrs: fsrs::FSRS,
    desired_retention: f32,
}

/// Rating for a review (maps to FSRS ratings 1-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            1 => Some(Rating::Again),
            2 => Some(Rating::Hard),
            3 => Some(Rating::Good),
            4 => Some(Rating::Easy),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Rating::Again => "Again",
            Rating::Hard => "Hard",
            Rating::Good => "Good",
            Rating::Easy => "Easy",
        }
    }
}

impl SrsEngine {
    /// Create a new SRS engine with default FSRS parameters.
    pub fn new() -> Result<Self> {
        let fsrs = fsrs::FSRS::new(Some(&fsrs::DEFAULT_PARAMETERS))?;
        Ok(Self {
            fsrs,
            desired_retention: 0.9,
        })
    }

    /// Create a word card for a vocabulary item if one doesn't already exist.
    /// Returns Some(card_id) if created, None if already exists.
    pub fn create_word_card(
        &self,
        conn: &Connection,
        vocabulary_id: i64,
        answer_mode: &AnswerMode,
    ) -> Result<Option<i64>> {
        if models::has_word_card(conn, vocabulary_id)? {
            return Ok(None);
        }
        let card_id =
            models::insert_srs_card(conn, vocabulary_id, &CardType::Word, answer_mode, None)?;
        Ok(Some(card_id))
    }

    /// Create a sentence card for a vocabulary item in its sentence context.
    /// Returns Some(card_id) if created, None if already exists.
    pub fn create_sentence_card(
        &self,
        conn: &Connection,
        vocabulary_id: i64,
    ) -> Result<Option<i64>> {
        if models::has_sentence_card_for_vocab(conn, vocabulary_id)? {
            return Ok(None);
        }
        let card_id = models::insert_srs_card(
            conn,
            vocabulary_id,
            &CardType::Sentence,
            &AnswerMode::SentenceCloze,
            None,
        )?;
        Ok(Some(card_id))
    }

    /// Get due cards, filtering out sentence cards where all vocabulary is Known.
    pub fn get_due_cards(&self, conn: &Connection, limit: usize) -> Result<Vec<SrsCard>> {
        let cards = models::get_due_cards(conn, limit)?;
        // For sentence cards, check if target vocabulary is still Learning
        let filtered: Vec<SrsCard> = cards
            .into_iter()
            .filter(|card| {
                if card.card_type == "sentence" {
                    // Check if the vocabulary is still in Learning state
                    if let Some(vid) = card.vocabulary_id {
                        if let Ok(Some(vocab)) = models::get_vocabulary_by_id(conn, vid) {
                            return matches!(
                                vocab.status,
                                VocabularyStatus::Learning1
                                    | VocabularyStatus::Learning2
                                    | VocabularyStatus::Learning3
                                    | VocabularyStatus::Learning4
                            );
                        }
                    }
                    false
                } else {
                    true
                }
            })
            .collect();
        Ok(filtered)
    }

    /// Record a review and update the card's FSRS state.
    /// Returns the new due date as a string.
    pub fn record_review(
        &self,
        conn: &Connection,
        card_id: i64,
        rating: Rating,
        elapsed_ms: u64,
        typed_answer: Option<&str>,
        answer_correct: bool,
    ) -> Result<String> {
        let card = models::get_srs_card(conn, card_id)?
            .ok_or_else(|| anyhow::anyhow!("Card not found: {}", card_id))?;

        // Build current memory state from stored values
        let current_memory_state = if card.reps > 0 {
            Some(fsrs::MemoryState {
                stability: card.stability as f32,
                difficulty: card.difficulty as f32,
            })
        } else {
            None
        };

        // Calculate days elapsed since last review
        let days_elapsed = if card.reps > 0 {
            let due = NaiveDateTime::parse_from_str(&card.due_date, "%Y-%m-%d %H:%M:%S")
                .unwrap_or_else(|_| Utc::now().naive_utc());
            let now = Utc::now().naive_utc();
            let diff = now.signed_duration_since(due);
            diff.num_days().max(0) as u32
        } else {
            0
        };

        // Get next states from FSRS
        let next_states =
            self.fsrs
                .next_states(current_memory_state, self.desired_retention, days_elapsed)?;

        // Pick the state for the given rating
        let next_state = match rating {
            Rating::Again => &next_states.again,
            Rating::Hard => &next_states.hard,
            Rating::Good => &next_states.good,
            Rating::Easy => &next_states.easy,
        };

        // Calculate new due date
        let interval_days = next_state.interval.round().max(1.0) as i64;
        let new_due = Utc::now().naive_utc() + chrono::Duration::days(interval_days);
        let due_date_str = new_due.format("%Y-%m-%d %H:%M:%S").to_string();

        // Determine new card state
        let new_reps = card.reps + 1;
        let new_lapses = if rating == Rating::Again {
            card.lapses + 1
        } else {
            card.lapses
        };
        let new_state = if rating == Rating::Again && card.reps > 0 {
            "relearning"
        } else if card.state == "new" {
            "learning"
        } else {
            "review"
        };

        // Update card in DB
        models::update_srs_card_state(
            conn,
            card_id,
            next_state.memory.stability as f64,
            next_state.memory.difficulty as f64,
            &due_date_str,
            new_reps,
            new_lapses,
            new_state,
        )?;

        // Insert review log
        models::insert_srs_review(
            conn,
            card_id,
            rating as i32,
            elapsed_ms as i64,
            typed_answer,
            answer_correct,
        )?;

        Ok(due_date_str)
    }

    /// Retire a single card.
    pub fn retire_card(&self, conn: &Connection, card_id: i64) -> Result<()> {
        models::retire_srs_card(conn, card_id)?;
        Ok(())
    }

    /// Retire all active cards for a vocabulary item.
    pub fn retire_cards_for_vocab(&self, conn: &Connection, vocabulary_id: i64) -> Result<usize> {
        models::retire_cards_for_vocabulary(conn, vocabulary_id)
    }

    /// Handle vocabulary status change — create or retire cards as needed.
    /// Called from the Reader when user changes word status.
    pub fn on_status_change(
        &self,
        conn: &Connection,
        vocabulary_id: i64,
        new_status: VocabularyStatus,
        default_answer_mode: &str,
    ) -> Result<()> {
        let answer_mode = AnswerMode::from_str(default_answer_mode);

        match new_status {
            VocabularyStatus::Learning1
            | VocabularyStatus::Learning2
            | VocabularyStatus::Learning3
            | VocabularyStatus::Learning4 => {
                // Create word card if not exists
                self.create_word_card(conn, vocabulary_id, &answer_mode)?;
                // Create sentence card if not exists
                self.create_sentence_card(conn, vocabulary_id)?;
            }
            VocabularyStatus::Known | VocabularyStatus::Ignored => {
                // Retire all active cards for this vocabulary
                self.retire_cards_for_vocab(conn, vocabulary_id)?;
            }
            VocabularyStatus::New => {
                // No action needed — New words don't have cards
            }
        }

        Ok(())
    }
}
