use anyhow::{Context, Result};
use rusqlite::Connection;
use scraper::{Html, Selector};
use std::io::Read;
use std::path::Path;

use super::text;

/// Import an EPUB file. Each chapter becomes a separate text entry.
/// Returns a Vec of (text_id, chapter_title) for all imported chapters.
pub fn import_epub(path: &Path, conn: &Connection) -> Result<Vec<(i64, String)>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open EPUB: {}", path.display()))?;

    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read EPUB as ZIP: {}", path.display()))?;

    let book_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled EPUB")
        .to_string();

    // Parse content.opf to get spine order
    let (opf_path, spine_items) = parse_opf(&mut archive)?;
    let opf_dir = opf_path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{}/", dir))
        .unwrap_or_default();

    if spine_items.is_empty() {
        anyhow::bail!("EPUB has no chapters in spine");
    }

    println!("EPUB: \"{}\" — {} chapters found", book_title, spine_items.len());

    let pb = indicatif::ProgressBar::new(spine_items.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::with_template(
            "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} chapters — {msg}",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );

    let mut imported = Vec::new();
    let source_url = path.to_string_lossy().to_string();

    for (i, item_href) in spine_items.iter().enumerate() {
        let full_path = format!("{}{}", opf_dir, item_href);

        let content = match read_zip_entry(&mut archive, &full_path) {
            Ok(c) => c,
            Err(_) => {
                // Try without opf_dir prefix
                match read_zip_entry(&mut archive, item_href) {
                    Ok(c) => c,
                    Err(_) => {
                        pb.set_message(format!("Skipping {} (not found)", item_href));
                        pb.inc(1);
                        continue;
                    }
                }
            }
        };

        let (chapter_title, chapter_text) = extract_chapter_text(&content, i + 1);

        if chapter_text.trim().is_empty() || chapter_text.trim().chars().count() < 10 {
            pb.inc(1);
            continue;
        }

        let title = if chapter_title != format!("Chapter {}", i + 1) {
            format!("{} — {}", book_title, chapter_title)
        } else {
            format!("{} — Chapter {}", book_title, i + 1)
        };

        pb.set_message(format!("\"{}\"", title));

        match text::import_text_with_progress(&title, &chapter_text, "epub", Some(&source_url), conn, None) {
            Ok(text_id) => {
                imported.push((text_id, title));
            }
            Err(e) => {
                pb.set_message(format!("Error importing chapter {}: {}", i + 1, e));
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "Imported {} chapters from \"{}\"",
        imported.len(),
        book_title
    ));

    if imported.is_empty() {
        anyhow::bail!("No chapters could be extracted from the EPUB");
    }

    Ok(imported)
}

/// Import EPUB quietly, returning imported chapters.
pub fn import_epub_quiet(path: &Path, conn: &Connection) -> Result<Vec<(i64, String)>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open EPUB: {}", path.display()))?;

    let mut archive = zip::ZipArchive::new(file)?;
    let (opf_path, spine_items) = parse_opf(&mut archive)?;
    let opf_dir = opf_path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{}/", dir))
        .unwrap_or_default();

    let book_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled EPUB")
        .to_string();

    let source_url = path.to_string_lossy().to_string();
    let mut imported = Vec::new();

    for (i, item_href) in spine_items.iter().enumerate() {
        let full_path = format!("{}{}", opf_dir, item_href);
        let content = read_zip_entry(&mut archive, &full_path)
            .or_else(|_| read_zip_entry(&mut archive, item_href));

        let content = match content {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (chapter_title, chapter_text) = extract_chapter_text(&content, i + 1);

        if chapter_text.trim().is_empty() || chapter_text.trim().chars().count() < 10 {
            continue;
        }

        let title = if chapter_title != format!("Chapter {}", i + 1) {
            format!("{} — {}", book_title, chapter_title)
        } else {
            format!("{} — Chapter {}", book_title, i + 1)
        };

        if let Ok(text_id) = text::import_text_quiet(&title, &chapter_text, "epub", Some(&source_url), conn) {
            imported.push((text_id, title));
        }
    }

    Ok(imported)
}

/// Parse the OPF manifest/spine to get ordered list of content files.
fn parse_opf(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<(String, Vec<String>)> {
    // First, find the OPF file from META-INF/container.xml
    let container_xml = read_zip_entry(archive, "META-INF/container.xml")
        .context("EPUB missing META-INF/container.xml")?;

    let container = Html::parse_document(&container_xml);
    let opf_path = if let Ok(sel) = Selector::parse("rootfile") {
        container.select(&sel).next()
            .and_then(|e| e.value().attr("full-path"))
            .map(|s| s.to_string())
    } else {
        None
    }.unwrap_or_else(|| "content.opf".to_string());

    // Read the OPF file
    let opf_xml = read_zip_entry(archive, &opf_path)
        .with_context(|| format!("Failed to read OPF: {}", opf_path))?;

    let opf = Html::parse_document(&opf_xml);

    // Build manifest map: id -> href
    let mut manifest: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Ok(sel) = Selector::parse("item") {
        for elem in opf.select(&sel) {
            if let (Some(id), Some(href)) = (elem.value().attr("id"), elem.value().attr("href")) {
                let media_type = elem.value().attr("media-type").unwrap_or("");
                // Only include XHTML/HTML content
                if media_type.contains("html") || media_type.contains("xml") {
                    manifest.insert(id.to_string(), href.to_string());
                }
            }
        }
    }

    // Read spine order
    let mut spine_items: Vec<String> = Vec::new();
    if let Ok(sel) = Selector::parse("itemref") {
        for elem in opf.select(&sel) {
            if let Some(idref) = elem.value().attr("idref") {
                if let Some(href) = manifest.get(idref) {
                    spine_items.push(href.clone());
                }
            }
        }
    }

    // Fallback: if no spine, use all manifest items
    if spine_items.is_empty() {
        spine_items = manifest.values().cloned().collect();
    }

    Ok((opf_path, spine_items))
}

/// Read a file from the ZIP archive.
fn read_zip_entry(archive: &mut zip::ZipArchive<std::fs::File>, path: &str) -> Result<String> {
    let mut entry = archive.by_name(path)
        .with_context(|| format!("Entry not found in EPUB: {}", path))?;
    let mut content = String::new();
    entry.read_to_string(&mut content)?;
    Ok(content)
}

/// Extract chapter title and text from an XHTML chapter file.
fn extract_chapter_text(html_content: &str, chapter_num: usize) -> (String, String) {
    let document = Html::parse_document(html_content);

    // Try to find a chapter title from headings
    let title = ["h1", "h2", "h3"]
        .iter()
        .find_map(|tag| {
            Selector::parse(tag).ok().and_then(|sel| {
                document.select(&sel).next().map(|e| {
                    e.text().collect::<String>().trim().to_string()
                })
            })
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| format!("Chapter {}", chapter_num));

    // Extract text from body
    let body_selector = Selector::parse("body").unwrap_or_else(|_| Selector::parse("html").unwrap());

    let mut text = String::new();
    if let Some(body) = document.select(&body_selector).next() {
        // Get text content, using <p> tags as paragraph boundaries
        if let Ok(p_sel) = Selector::parse("p") {
            let paragraphs: Vec<String> = body
                .select(&p_sel)
                .map(|p| p.text().collect::<String>().trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();

            if !paragraphs.is_empty() {
                text = paragraphs.join("\n\n");
            }
        }

        // Fallback: just get all text
        if text.is_empty() {
            text = body.text().collect::<Vec<_>>().join("\n");
        }
    }

    (title, text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_chapter_text() {
        let html = r#"
        <html>
        <body>
            <h1>第一章</h1>
            <p>最初の段落です。</p>
            <p>二番目の段落です。</p>
        </body>
        </html>
        "#;
        let (title, content) = extract_chapter_text(html, 1);
        assert_eq!(title, "第一章");
        assert!(content.contains("最初の段落です。"));
        assert!(content.contains("二番目の段落です。"));
    }

    #[test]
    fn test_extract_chapter_text_no_heading() {
        let html = "<html><body><p>Some text</p></body></html>";
        let (title, _content) = extract_chapter_text(html, 5);
        assert_eq!(title, "Chapter 5");
    }
}
