use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A sense (meaning) of a dictionary entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sense {
    pub pos: Vec<String>,
    pub glosses: Vec<String>,
    pub misc: Vec<String>,
}

/// A parsed JMdict entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictEntry {
    pub ent_seq: i64,
    pub kanji_forms: Vec<String>,
    pub readings: Vec<String>,
    pub senses: Vec<Sense>,
}

impl DictEntry {
    /// Returns the first English gloss of the first sense, for sidebar display.
    pub fn short_gloss(&self) -> String {
        self.senses
            .first()
            .and_then(|s| s.glosses.first())
            .cloned()
            .unwrap_or_default()
    }
}

/// Import JMdict XML file into the database.
pub fn import_jmdict(path: &Path, conn: &Connection) -> Result<()> {
    // Estimate entry count for progress bar (~190k entries)
    let file_size = std::fs::metadata(path)
        .with_context(|| format!("Cannot read file: {}", path.display()))?
        .len();

    let pb = ProgressBar::new(file_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} bytes ({eta})")
            .unwrap()
            .progress_chars("##-"),
    );

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read JMdict file: {}", path.display()))?;

    let mut reader = Reader::from_str(&content);
    reader.trim_text(true);

    let tx = conn.execute_batch("BEGIN TRANSACTION;")?;
    let _ = tx;

    let mut entry_count = 0u64;
    let mut current_entry: Option<EntryBuilder> = None;
    let mut current_element = String::new();
    let mut in_sense = false;
    let mut current_sense = SenseBuilder::new();
    let mut buf_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = name.clone();

                match name.as_str() {
                    "entry" => {
                        current_entry = Some(EntryBuilder::new());
                    }
                    "sense" => {
                        in_sense = true;
                        current_sense = SenseBuilder::new();
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match name.as_str() {
                    "entry" => {
                        if let Some(builder) = current_entry.take() {
                            let entry = builder.build();
                            insert_dict_entry(conn, &entry)?;
                            entry_count += 1;

                            if entry_count % 1000 == 0 {
                                pb.set_position(reader.buffer_position() as u64);
                            }
                        }
                    }
                    "sense" => {
                        if let Some(ref mut builder) = current_entry {
                            builder.senses.push(current_sense.build());
                        }
                        in_sense = false;
                    }
                    _ => {}
                }
                current_element.clear();
            }
            Ok(Event::Text(ref e)) => {
                buf_text = e.unescape().unwrap_or_default().to_string();

                if let Some(ref mut builder) = current_entry {
                    match current_element.as_str() {
                        "ent_seq" => {
                            builder.ent_seq = buf_text.parse().unwrap_or(0);
                        }
                        "keb" => {
                            builder.kanji_forms.push(buf_text.clone());
                        }
                        "reb" => {
                            builder.readings.push(buf_text.clone());
                        }
                        "gloss" => {
                            if in_sense {
                                current_sense.glosses.push(buf_text.clone());
                            }
                        }
                        "pos" => {
                            if in_sense {
                                current_sense.pos.push(buf_text.clone());
                            }
                        }
                        "misc" => {
                            if in_sense {
                                current_sense.misc.push(buf_text.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error at position {}: {:?}", reader.buffer_position(), e)),
            _ => {}
        }
    }

    conn.execute_batch("COMMIT;")?;

    pb.finish_with_message(format!("Imported {} entries", entry_count));
    println!("Imported {} JMdict entries", entry_count);

    Ok(())
}

fn insert_dict_entry(conn: &Connection, entry: &DictEntry) -> Result<()> {
    let json_blob = serde_json::to_string(entry)?;

    conn.execute(
        "INSERT OR REPLACE INTO jmdict_entries (ent_seq, json_blob) VALUES (?1, ?2)",
        params![entry.ent_seq, json_blob],
    )?;

    for kanji in &entry.kanji_forms {
        conn.execute(
            "INSERT INTO jmdict_kanji (entry_id, kanji_element) VALUES (?1, ?2)",
            params![entry.ent_seq, kanji],
        )?;
    }

    for reading in &entry.readings {
        conn.execute(
            "INSERT INTO jmdict_readings (entry_id, reading_element) VALUES (?1, ?2)",
            params![entry.ent_seq, reading],
        )?;
    }

    Ok(())
}

/// Look up a word in the JMdict database.
/// First searches kanji forms, then reading forms as fallback.
pub fn lookup(conn: &Connection, base_form: &str, reading: Option<&str>) -> Result<Vec<DictEntry>> {
    let mut entries = Vec::new();

    // Search by kanji form first
    let mut stmt = conn.prepare(
        "SELECT e.json_blob FROM jmdict_entries e
         JOIN jmdict_kanji k ON k.entry_id = e.ent_seq
         WHERE k.kanji_element = ?1"
    )?;

    let rows = stmt.query_map(params![base_form], |row| {
        let json: String = row.get(0)?;
        Ok(json)
    })?;

    for row in rows {
        if let Ok(json) = row {
            if let Ok(entry) = serde_json::from_str::<DictEntry>(&json) {
                // If reading is specified, filter by it
                if let Some(r) = reading {
                    if entry.readings.iter().any(|er| er == r) {
                        entries.push(entry);
                    }
                } else {
                    entries.push(entry);
                }
            }
        }
    }

    // If no kanji match, try reading match
    if entries.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT e.json_blob FROM jmdict_entries e
             JOIN jmdict_readings r ON r.entry_id = e.ent_seq
             WHERE r.reading_element = ?1"
        )?;

        let rows = stmt.query_map(params![base_form], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        })?;

        for row in rows {
            if let Ok(json) = row {
                if let Ok(entry) = serde_json::from_str::<DictEntry>(&json) {
                    entries.push(entry);
                }
            }
        }
    }

    Ok(entries)
}

// ─── Builder helpers ─────────────────────────────────────────────────

struct EntryBuilder {
    ent_seq: i64,
    kanji_forms: Vec<String>,
    readings: Vec<String>,
    senses: Vec<Sense>,
}

impl EntryBuilder {
    fn new() -> Self {
        Self {
            ent_seq: 0,
            kanji_forms: Vec::new(),
            readings: Vec::new(),
            senses: Vec::new(),
        }
    }

    fn build(self) -> DictEntry {
        DictEntry {
            ent_seq: self.ent_seq,
            kanji_forms: self.kanji_forms,
            readings: self.readings,
            senses: self.senses,
        }
    }
}

struct SenseBuilder {
    pos: Vec<String>,
    glosses: Vec<String>,
    misc: Vec<String>,
}

impl SenseBuilder {
    fn new() -> Self {
        Self {
            pos: Vec::new(),
            glosses: Vec::new(),
            misc: Vec::new(),
        }
    }

    fn build(&self) -> Sense {
        Sense {
            pos: self.pos.clone(),
            glosses: self.glosses.clone(),
            misc: self.misc.clone(),
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
    fn test_insert_and_lookup() {
        let conn = setup();

        let entry = DictEntry {
            ent_seq: 1234,
            kanji_forms: vec!["食べる".to_string()],
            readings: vec!["たべる".to_string()],
            senses: vec![Sense {
                pos: vec!["verb".to_string()],
                glosses: vec!["to eat".to_string(), "to consume".to_string()],
                misc: vec![],
            }],
        };

        insert_dict_entry(&conn, &entry).unwrap();

        // Lookup by kanji
        let results = lookup(&conn, "食べる", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].short_gloss(), "to eat");

        // Lookup by reading
        let results = lookup(&conn, "たべる", None).unwrap();
        assert_eq!(results.len(), 1);

        // Lookup with reading filter
        let results = lookup(&conn, "食べる", Some("たべる")).unwrap();
        assert_eq!(results.len(), 1);

        // Lookup with wrong reading
        let results = lookup(&conn, "食べる", Some("のべる")).unwrap();
        assert_eq!(results.len(), 0);

        // Lookup missing entry
        let results = lookup(&conn, "存在しない", None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_short_gloss() {
        let entry = DictEntry {
            ent_seq: 1,
            kanji_forms: vec![],
            readings: vec!["あ".to_string()],
            senses: vec![
                Sense {
                    pos: vec![],
                    glosses: vec!["first meaning".to_string()],
                    misc: vec![],
                },
                Sense {
                    pos: vec![],
                    glosses: vec!["second meaning".to_string()],
                    misc: vec![],
                },
            ],
        };
        assert_eq!(entry.short_gloss(), "first meaning");
    }
}
