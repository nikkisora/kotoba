mod app;
mod config;
mod core;
mod db;
mod import;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kotoba", version, about = "Terminal-based Japanese language learning app")]
struct Cli {
    /// Path to config file
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Import a text file for reading
    Import {
        /// Path to the text file to import
        file: PathBuf,
    },
    /// Tokenize a Japanese text string
    Tokenize {
        /// The Japanese text to tokenize
        text: String,
    },
    /// Look up a word in JMdict
    Dict {
        /// The word to look up (kanji or kana)
        word: String,
    },
    /// Import JMdict XML dictionary into the database
    ImportDict {
        /// Path to JMdict_e.xml file
        path: PathBuf,
    },
    /// Launch the TUI reader
    Run,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cfg = config::AppConfig::load(cli.config.as_deref())?;
    let conn = db::connection::open_or_create(cfg.db_path())?;
    db::schema::run_migrations(&conn)?;

    match cli.command {
        Commands::Import { file } => {
            let text_id = import::text::import_file(&file, &conn)?;
            println!("Imported text with id: {text_id}");
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
                    t.surface, t.base_form, t.reading, t.pos, t.conjugation_form, t.conjugation_type
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
        Commands::Run => {
            println!("TUI mode not yet implemented (Phase 2)");
        }
    }

    Ok(())
}
