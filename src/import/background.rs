//! Background chapter preprocessing system.
//!
//! Runs import tasks (HTTP fetch + tokenization) in a thread pool so the TUI
//! stays responsive. Communicates results back via mpsc channel.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::db::{connection, models};

/// Events sent from background workers to the TUI event loop.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ImportEvent {
    /// A chapter started preprocessing.
    Started {
        source_id: i64,
        chapter_id: i64,
        chapter_number: i32,
    },
    /// A chapter finished preprocessing successfully.
    Completed {
        source_id: i64,
        chapter_id: i64,
        chapter_number: i32,
        text_id: i64,
    },
    /// A chapter failed to import.
    Failed {
        source_id: i64,
        chapter_id: i64,
        chapter_number: i32,
        error: String,
    },
    /// Progress update during preprocessing.
    Progress {
        chapter_id: i64,
        /// Which phase: "fetch", "tokenize", "write"
        phase: &'static str,
        /// 0..100
        percent: u8,
    },
    /// A chapter's import was cancelled (e.g., marked as skipped).
    Cancelled { source_id: i64, chapter_id: i64 },
    /// A page of chapters has been loaded and saved (incremental Syosetu loading).
    ChaptersPageLoaded {
        source_id: i64,
        page: usize,
        total_so_far: usize,
        has_next: bool,
    },
    /// Novel metadata finished loading (all pages fetched).
    NovelInfoLoaded { source_id: i64, title: String },
    /// Novel metadata fetch failed.
    NovelInfoFailed { ncode: String, error: String },
    /// A standalone text import (file, clipboard, URL, subtitle, epub) has started.
    TextImportStarted { label: String },
    /// A standalone text import completed successfully.
    TextImportCompleted { title: String },
    /// A standalone text import failed.
    TextImportFailed { label: String, error: String },
}

/// A task to import a single chapter.
#[derive(Debug, Clone)]
struct ImportTask {
    source_id: i64,
    chapter_id: i64,
    chapter_number: i32,
    source_type: String,
    /// For syosetu: the ncode. For epub: not used (already imported).
    external_id: String,
    db_path: PathBuf,
}

/// Manages background import workers and task queue.
pub struct BackgroundImporter {
    /// Channel to send import events back to the TUI.
    event_tx: mpsc::Sender<ImportEvent>,
    /// Task queue shared with workers.
    task_queue: Arc<Mutex<Vec<ImportTask>>>,
    /// Set of chapter_ids that have been cancelled.
    cancelled: Arc<Mutex<HashSet<i64>>>,
    /// Set of chapter_ids currently in-flight or completed.
    in_flight: Arc<Mutex<HashSet<i64>>>,
    /// Number of worker threads.
    num_workers: usize,
    /// Whether workers have been spawned.
    workers_spawned: bool,
}

impl BackgroundImporter {
    /// Create a new background importer.
    pub fn new(event_tx: mpsc::Sender<ImportEvent>, num_workers: usize) -> Self {
        Self {
            event_tx,
            task_queue: Arc::new(Mutex::new(Vec::new())),
            cancelled: Arc::new(Mutex::new(HashSet::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            num_workers,
            workers_spawned: false,
        }
    }

    /// Spawn worker threads if not already spawned.
    fn ensure_workers(&mut self) {
        if self.workers_spawned {
            return;
        }
        self.workers_spawned = true;

        for _ in 0..self.num_workers {
            let queue = Arc::clone(&self.task_queue);
            let cancelled = Arc::clone(&self.cancelled);
            let in_flight = Arc::clone(&self.in_flight);
            let tx = self.event_tx.clone();

            thread::spawn(move || {
                loop {
                    // Try to get a task from the queue
                    let task = {
                        let mut q = queue.lock().unwrap();
                        if q.is_empty() {
                            None
                        } else {
                            Some(q.remove(0))
                        }
                    };

                    let task = match task {
                        Some(t) => t,
                        None => {
                            // No tasks — sleep briefly and retry
                            thread::sleep(std::time::Duration::from_millis(200));
                            continue;
                        }
                    };

                    // Check if cancelled before starting
                    {
                        let c = cancelled.lock().unwrap();
                        if c.contains(&task.chapter_id) {
                            let _ = tx.send(ImportEvent::Cancelled {
                                source_id: task.source_id,
                                chapter_id: task.chapter_id,
                            });
                            continue;
                        }
                    }

                    // Mark as in-flight
                    {
                        in_flight.lock().unwrap().insert(task.chapter_id);
                    }

                    // Send started event
                    let _ = tx.send(ImportEvent::Started {
                        source_id: task.source_id,
                        chapter_id: task.chapter_id,
                        chapter_number: task.chapter_number,
                    });

                    // Do the actual import
                    let result = run_import_task(&task, &cancelled, &tx);

                    // Remove from in-flight
                    {
                        in_flight.lock().unwrap().remove(&task.chapter_id);
                    }

                    match result {
                        Ok(text_id) => {
                            let _ = tx.send(ImportEvent::Completed {
                                source_id: task.source_id,
                                chapter_id: task.chapter_id,
                                chapter_number: task.chapter_number,
                                text_id,
                            });
                        }
                        Err(e) => {
                            if cancelled.lock().unwrap().contains(&task.chapter_id) {
                                let _ = tx.send(ImportEvent::Cancelled {
                                    source_id: task.source_id,
                                    chapter_id: task.chapter_id,
                                });
                            } else {
                                let _ = tx.send(ImportEvent::Failed {
                                    source_id: task.source_id,
                                    chapter_id: task.chapter_id,
                                    chapter_number: task.chapter_number,
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                }
            });
        }
    }

    /// Queue a single specific chapter (e.g., when user presses Enter on it).
    /// This bypasses the count limit — it's a user-initiated request.
    pub fn queue_single(
        &mut self,
        source_id: i64,
        source_type: &str,
        external_id: &str,
        chapter_id: i64,
        chapter_number: i32,
        db_path: &std::path::Path,
    ) {
        self.ensure_workers();

        let in_flight = self.in_flight.lock().unwrap();
        let mut queue = self.task_queue.lock().unwrap();

        // Don't queue if already in-flight or queued
        if in_flight.contains(&chapter_id) || queue.iter().any(|t| t.chapter_id == chapter_id) {
            return;
        }

        // Insert at the front of the queue for priority
        queue.insert(
            0,
            ImportTask {
                source_id,
                chapter_id,
                chapter_number,
                source_type: source_type.to_string(),
                external_id: external_id.to_string(),
                db_path: db_path.to_path_buf(),
            },
        );
    }

    /// Cancel preprocessing for a specific chapter.
    pub fn cancel_chapter(&self, chapter_id: i64) {
        // Add to cancelled set
        self.cancelled.lock().unwrap().insert(chapter_id);
        // Also remove from queue if not yet started
        let mut queue = self.task_queue.lock().unwrap();
        queue.retain(|t| t.chapter_id != chapter_id);
    }

    /// Check if a chapter is currently being preprocessed.
    #[allow(dead_code)]
    pub fn is_in_flight(&self, chapter_id: i64) -> bool {
        self.in_flight.lock().unwrap().contains(&chapter_id)
    }

    /// Check if a chapter is in the queue (waiting to start).
    #[allow(dead_code)]
    pub fn is_queued(&self, chapter_id: i64) -> bool {
        self.task_queue
            .lock()
            .unwrap()
            .iter()
            .any(|t| t.chapter_id == chapter_id)
    }

    /// Fetch novel info in the background. Spawns a single thread.
    /// Sends `ChaptersPageLoaded` events incrementally as pages are fetched,
    /// then a final `NovelInfoLoaded` when all pages are done.
    pub fn fetch_novel_info(&self, ncode: String, db_path: PathBuf) {
        let tx = self.event_tx.clone();
        thread::spawn(
            move || match fetch_novel_info_blocking(&ncode, &db_path, &tx) {
                Ok((source_id, title)) => {
                    let _ = tx.send(ImportEvent::NovelInfoLoaded { source_id, title });
                }
                Err(e) => {
                    let _ = tx.send(ImportEvent::NovelInfoFailed {
                        ncode,
                        error: e.to_string(),
                    });
                }
            },
        );
    }

    /// Refresh an existing Syosetu novel's chapter list in the background.
    /// Checks page 1 to see if the total chapter count has changed, and if so,
    /// fetches only the new pages (those beyond what we already have in DB).
    pub fn refresh_novel_chapters(
        &self,
        source_id: i64,
        ncode: String,
        existing_chapter_count: usize,
        db_path: PathBuf,
    ) {
        let tx = self.event_tx.clone();
        thread::spawn(move || {
            if let Err(e) =
                refresh_novel_blocking(source_id, &ncode, existing_chapter_count, &db_path, &tx)
            {
                // Silently log — refresh failures shouldn't disrupt the user
                let _ = tx.send(ImportEvent::NovelInfoFailed {
                    ncode,
                    error: format!("Refresh failed: {}", e),
                });
            }
        });
    }

    /// Import a file in the background (text, subtitle, epub).
    pub fn import_file(&self, path: std::path::PathBuf, db_path: PathBuf) {
        let tx = self.event_tx.clone();
        let label = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("file")
            .to_string();
        let _ = tx.send(ImportEvent::TextImportStarted {
            label: label.clone(),
        });
        thread::spawn(move || {
            let result = (|| -> anyhow::Result<String> {
                let conn = connection::open_or_create(&db_path)?;
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                match ext.as_str() {
                    "srt" | "ass" | "ssa" => {
                        let (_, title) =
                            crate::import::subtitle::import_subtitle_quiet(&path, &conn)?;
                        Ok(title)
                    }
                    "epub" => {
                        let chapters = crate::import::epub::import_epub_quiet(&path, &conn)?;
                        Ok(format!("{} chapters imported", chapters.len()))
                    }
                    _ => {
                        let title = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Untitled")
                            .to_string();
                        let content = std::fs::read_to_string(&path)?;
                        crate::import::text::import_text_quiet(
                            &title, &content, "text", None, &conn,
                        )?;
                        Ok(title)
                    }
                }
            })();
            match result {
                Ok(title) => {
                    let _ = tx.send(ImportEvent::TextImportCompleted { title });
                }
                Err(e) => {
                    let _ = tx.send(ImportEvent::TextImportFailed {
                        label,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    /// Import clipboard content in the background.
    pub fn import_clipboard(&self, db_path: PathBuf) {
        let tx = self.event_tx.clone();
        let _ = tx.send(ImportEvent::TextImportStarted {
            label: "clipboard".to_string(),
        });
        thread::spawn(move || {
            let result = (|| -> anyhow::Result<String> {
                let conn = connection::open_or_create(&db_path)?;
                let (_, title) = crate::import::clipboard::import_clipboard_quiet(&conn)?;
                Ok(title)
            })();
            match result {
                Ok(title) => {
                    let _ = tx.send(ImportEvent::TextImportCompleted { title });
                }
                Err(e) => {
                    let _ = tx.send(ImportEvent::TextImportFailed {
                        label: "clipboard".to_string(),
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    /// Import from URL in the background.
    pub fn import_url(&self, url: String, db_path: PathBuf) {
        let tx = self.event_tx.clone();
        let label = url.clone();
        let _ = tx.send(ImportEvent::TextImportStarted {
            label: label.clone(),
        });
        thread::spawn(move || {
            let result = (|| -> anyhow::Result<String> {
                let conn = connection::open_or_create(&db_path)?;
                let (_, title) = crate::import::web::import_url_quiet(&url, &conn)?;
                Ok(title)
            })();
            match result {
                Ok(title) => {
                    let _ = tx.send(ImportEvent::TextImportCompleted { title });
                }
                Err(e) => {
                    let _ = tx.send(ImportEvent::TextImportFailed {
                        label,
                        error: e.to_string(),
                    });
                }
            }
        });
    }
}

/// Actually perform the import using two-phase approach:
/// Phase 1: HTTP fetch + tokenize in memory (no DB lock needed, parallelizable)
/// Phase 2: Short DB transaction to write results
fn run_import_task(
    task: &ImportTask,
    cancelled: &Arc<Mutex<HashSet<i64>>>,
    tx: &mpsc::Sender<ImportEvent>,
) -> anyhow::Result<i64> {
    match task.source_type.as_str() {
        "syosetu" => {
            // Check cancellation before HTTP
            if cancelled.lock().unwrap().contains(&task.chapter_id) {
                anyhow::bail!("Cancelled");
            }

            let _ = tx.send(ImportEvent::Progress {
                chapter_id: task.chapter_id,
                phase: "fetch",
                percent: 0,
            });

            // Phase 1a: HTTP fetch
            let (title, content, url) = crate::import::syosetu::fetch_chapter_content(
                &task.external_id,
                task.chapter_number as usize,
            )?;

            // Check cancellation after HTTP fetch
            if cancelled.lock().unwrap().contains(&task.chapter_id) {
                anyhow::bail!("Cancelled");
            }

            let _ = tx.send(ImportEvent::Progress {
                chapter_id: task.chapter_id,
                phase: "tokenize",
                percent: 0,
            });

            // Phase 1b: Tokenize in memory (CPU-bound, no DB)
            let chapter_id = task.chapter_id;
            let tx_clone = tx.clone();
            let pretokenized = crate::import::text::pretokenize_text_with_progress(
                &content,
                &move |done, total| {
                    let pct = if total > 0 {
                        ((done as f64 / total as f64) * 100.0) as u8
                    } else {
                        0
                    };
                    let _ = tx_clone.send(ImportEvent::Progress {
                        chapter_id,
                        phase: "tokenize",
                        percent: pct,
                    });
                },
            )?;

            // Check cancellation after tokenization
            if cancelled.lock().unwrap().contains(&task.chapter_id) {
                anyhow::bail!("Cancelled");
            }

            let _ = tx.send(ImportEvent::Progress {
                chapter_id: task.chapter_id,
                phase: "write",
                percent: 90,
            });

            // Phase 2: Short DB transaction for writes only
            let conn = connection::open_or_create(&task.db_path)?;

            // Always prefer DB-sourced novel title and chapter subtitle (from ToC)
            // over the per-chapter-page scrape, which can miss selectors.
            let title = {
                let novel_title = models::get_web_source_by_id(&conn, task.source_id)
                    .ok()
                    .flatten()
                    .map(|ws| ws.title)
                    .unwrap_or_default();
                let chapter_subtitle = models::list_chapters_by_source(&conn, task.source_id)
                    .ok()
                    .and_then(|chs| {
                        chs.into_iter()
                            .find(|c| c.id == task.chapter_id)
                            .map(|c| c.title)
                    })
                    .unwrap_or_default();
                if !novel_title.is_empty() && !chapter_subtitle.is_empty() {
                    crate::import::syosetu::format_chapter_title(
                        &novel_title,
                        task.chapter_number as usize,
                        &chapter_subtitle,
                    )
                } else {
                    // Fall back to the title scraped from the chapter page
                    title
                }
            };

            let text_id = crate::import::text::write_pretokenized(
                &title,
                &content,
                "syosetu",
                Some(&url),
                &pretokenized,
                &conn,
            )?;

            // Update the chapter's text_id in the DB
            models::update_chapter_text_id(&conn, task.chapter_id, text_id)?;

            Ok(text_id)
        }
        _ => {
            anyhow::bail!(
                "Background import not supported for source type: {}",
                task.source_type
            )
        }
    }
}

/// Fetch novel info incrementally, page by page. Saves each page to DB and
/// sends `ChaptersPageLoaded` events so the UI can update as chapters arrive.
#[allow(unused_assignments)]
fn fetch_novel_info_blocking(
    ncode: &str,
    db_path: &Path,
    tx: &mpsc::Sender<ImportEvent>,
) -> anyhow::Result<(i64, String)> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; kotoba/0.1)")
        .build()?;

    let conn = connection::open_or_create(db_path)?;
    let mut page = 1;
    let mut total_so_far = 0usize;
    let mut title = format!("Novel {}", ncode);
    let mut author = String::new();
    let mut source_id: Option<i64> = None;

    loop {
        let result = crate::import::syosetu::fetch_novel_page(&client, ncode, page, total_so_far)?;

        if page == 1 {
            title = result.title;
            author = result.author;
        }

        let got_chapters = !result.chapters.is_empty();
        total_so_far += result.chapters.len();

        // Save this page's chapters to DB
        let ws_id = crate::import::syosetu::save_novel_metadata_page(
            ncode,
            &title,
            &author,
            &result.chapters,
            &conn,
        )?;
        source_id = Some(ws_id);

        // Notify UI that a page of chapters has arrived
        let _ = tx.send(ImportEvent::ChaptersPageLoaded {
            source_id: ws_id,
            page,
            total_so_far,
            has_next: result.has_next_page,
        });

        if result.has_next_page && got_chapters {
            page += 1;
            std::thread::sleep(std::time::Duration::from_millis(500));
        } else {
            break;
        }
    }

    // Handle oneshot novels (no chapters found on any page)
    if total_so_far == 0 {
        let toc_url = format!("https://ncode.syosetu.com/{}/", ncode);
        let html = client.get(&toc_url).send()?.text()?;
        let document = scraper::Html::parse_document(&html);
        if let Ok(sel) = scraper::Selector::parse("#novel_honbun") {
            if document.select(&sel).next().is_some() {
                let ch = crate::import::syosetu::SyosetuChapter {
                    number: 1,
                    title: title.clone(),
                    text_id: None,
                    word_count: 0,
                    group: String::new(),
                };
                let ws_id = crate::import::syosetu::save_novel_metadata_page(
                    ncode,
                    &title,
                    &author,
                    &[ch],
                    &conn,
                )?;
                source_id = Some(ws_id);
                total_so_far = 1;

                let _ = tx.send(ImportEvent::ChaptersPageLoaded {
                    source_id: ws_id,
                    page: 1,
                    total_so_far: 1,
                    has_next: false,
                });
            }
        }
    }

    let ws_id = source_id.ok_or_else(|| anyhow::anyhow!("No chapters found for {}", ncode))?;
    Ok((ws_id, title))
}

/// Refresh an existing novel by checking for new chapters.
/// Fetches page 1 to count total chapters, then fetches only pages
/// that contain chapters beyond `existing_count`.
fn refresh_novel_blocking(
    source_id: i64,
    ncode: &str,
    existing_count: usize,
    db_path: &Path,
    tx: &mpsc::Sender<ImportEvent>,
) -> anyhow::Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; kotoba/0.1)")
        .build()?;

    // Fetch page 1 to get the total chapter count
    let first_page = crate::import::syosetu::fetch_novel_page(&client, ncode, 1, 0)?;
    let chapters_per_page = first_page.chapters.len();

    if chapters_per_page == 0 {
        return Ok(()); // Oneshot or empty — nothing to refresh
    }

    // Estimate total pages: walk until no more pages
    // But first, check if page 1 already shows more chapters than we have
    // The total count across all pages is unknown until we walk them all,
    // but we can skip pages we've already fetched by using chapter_offset.

    // Strategy: if existing_count < chapters on page 1, we need to re-fetch
    // everything (chapters may have been reordered). For simplicity, only
    // fetch pages whose chapters are beyond existing_count.
    let conn = connection::open_or_create(db_path)?;
    let title = first_page.title;
    let author = first_page.author;

    // Calculate which page the new chapters start on
    // Syosetu has ~100 chapters per page
    if chapters_per_page == 0 {
        return Ok(());
    }
    let start_page = (existing_count / chapters_per_page) + 1;
    let mut page = start_page;
    let mut total_new = 0usize;

    loop {
        let result = crate::import::syosetu::fetch_novel_page(
            &client,
            ncode,
            page,
            existing_count + total_new,
        )?;

        // Only save chapters that are truly new (number > existing_count)
        let new_chapters: Vec<_> = result
            .chapters
            .into_iter()
            .filter(|ch| ch.number > existing_count)
            .collect();

        if !new_chapters.is_empty() {
            total_new += new_chapters.len();
            crate::import::syosetu::save_novel_metadata_page(
                ncode,
                &title,
                &author,
                &new_chapters,
                &conn,
            )?;

            let _ = tx.send(ImportEvent::ChaptersPageLoaded {
                source_id,
                page,
                total_so_far: existing_count + total_new,
                has_next: result.has_next_page,
            });
        }

        if result.has_next_page && !new_chapters.is_empty() {
            page += 1;
            std::thread::sleep(std::time::Duration::from_millis(500));
        } else {
            break;
        }
    }

    if total_new > 0 {
        let _ = tx.send(ImportEvent::NovelInfoLoaded { source_id, title });
    }

    Ok(())
}
