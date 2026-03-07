use anyhow::{Context, Result};
use rusqlite::Connection;
use scraper::{Html, Selector};
use std::io::Read;
use std::path::Path;

use super::text;
use crate::db::models;

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

    // Parse content.opf to get spine order and TOC titles
    let opf_data = parse_opf(&mut archive)?;
    let opf_dir = opf_data
        .opf_path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{}/", dir))
        .unwrap_or_default();

    if opf_data.spine_items.is_empty() {
        anyhow::bail!("EPUB has no chapters in spine");
    }

    println!(
        "EPUB: \"{}\" — {} chapters found",
        book_title,
        opf_data.spine_items.len()
    );

    let pb = indicatif::ProgressBar::new(opf_data.spine_items.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::with_template(
            "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} chapters — {msg}",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );

    let mut imported = Vec::new();
    let source_url = path.to_string_lossy().to_string();
    let mut chapter_num = 0;

    for item_href in opf_data.spine_items.iter() {
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

        let (heading_title, chapter_text) = extract_chapter_text(&content);

        if chapter_text.trim().is_empty() || chapter_text.trim().chars().count() < 10 {
            pb.inc(1);
            continue;
        }

        chapter_num += 1;

        // Try TOC title first, then heading from content, then fallback
        let chapter_title = resolve_chapter_title(
            item_href,
            &opf_data.toc_titles,
            heading_title.as_deref(),
            chapter_num,
        );
        let title = format!("{} — {}", book_title, chapter_title);

        pb.set_message(format!("\"{}\"", title));

        match text::import_text_with_progress(
            &title,
            &chapter_text,
            "epub",
            Some(&source_url),
            conn,
            None,
        ) {
            Ok(text_id) => {
                imported.push((text_id, title));
            }
            Err(e) => {
                pb.set_message(format!("Error importing chapter {}: {}", chapter_num, e));
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
/// Also creates a web_source and web_source_chapters for grouped display.
pub fn import_epub_quiet(path: &Path, conn: &Connection) -> Result<Vec<(i64, String)>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open EPUB: {}", path.display()))?;

    let mut archive = zip::ZipArchive::new(file)?;
    let opf_data = parse_opf(&mut archive)?;
    let opf_dir = opf_data
        .opf_path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{}/", dir))
        .unwrap_or_default();

    let book_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled EPUB")
        .to_string();

    let source_url = path.to_string_lossy().to_string();

    // Create web_source for this EPUB
    let source_id = models::upsert_web_source(conn, "epub", &source_url, &book_title, "{}")?;

    let mut imported = Vec::new();
    let mut chapter_num = 0;

    for item_href in opf_data.spine_items.iter() {
        let full_path = format!("{}{}", opf_dir, item_href);
        let content = read_zip_entry(&mut archive, &full_path)
            .or_else(|_| read_zip_entry(&mut archive, item_href));

        let content = match content {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (heading_title, chapter_text) = extract_chapter_text(&content);

        if chapter_text.trim().is_empty() || chapter_text.trim().chars().count() < 10 {
            continue;
        }

        chapter_num += 1;

        // Try TOC title first, then heading from content, then fallback
        let chapter_title = resolve_chapter_title(
            item_href,
            &opf_data.toc_titles,
            heading_title.as_deref(),
            chapter_num,
        );
        let title = format!("{} — {}", book_title, chapter_title);

        if let Ok(text_id) =
            text::import_text_quiet(&title, &chapter_text, "epub", Some(&source_url), conn)
        {
            // Create chapter entry linking to the text
            let _ = models::insert_web_source_chapter(
                conn,
                source_id,
                chapter_num as i32,
                &chapter_title,
                Some(text_id),
                0,
                "",
            );
            imported.push((text_id, title));
        }
    }

    Ok(imported)
}

/// Result of parsing the OPF: path to OPF, spine items, and TOC title map (href -> title).
struct OpfData {
    opf_path: String,
    spine_items: Vec<String>,
    /// Map from content file href (relative to OPF dir) to chapter title from TOC.
    toc_titles: std::collections::HashMap<String, String>,
}

/// Parse the OPF manifest/spine to get ordered list of content files,
/// and extract chapter titles from the TOC (NCX or NAV).
fn parse_opf(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<OpfData> {
    // First, find the OPF file from META-INF/container.xml
    let container_xml = read_zip_entry(archive, "META-INF/container.xml")
        .context("EPUB missing META-INF/container.xml")?;

    let container = Html::parse_document(&container_xml);
    let opf_path = if let Ok(sel) = Selector::parse("rootfile") {
        container
            .select(&sel)
            .next()
            .and_then(|e| e.value().attr("full-path"))
            .map(|s| s.to_string())
    } else {
        None
    }
    .unwrap_or_else(|| "content.opf".to_string());

    let opf_dir = opf_path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{}/", dir))
        .unwrap_or_default();

    // Read the OPF file
    let opf_xml = read_zip_entry(archive, &opf_path)
        .with_context(|| format!("Failed to read OPF: {}", opf_path))?;

    let opf = Html::parse_document(&opf_xml);

    // Build manifest map: id -> (href, media_type)
    let mut manifest: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut toc_ncx_href: Option<String> = None;
    let mut nav_href: Option<String> = None;

    if let Ok(sel) = Selector::parse("item") {
        for elem in opf.select(&sel) {
            if let (Some(id), Some(href)) = (elem.value().attr("id"), elem.value().attr("href")) {
                let media_type = elem.value().attr("media-type").unwrap_or("");
                let properties = elem.value().attr("properties").unwrap_or("");

                // Detect TOC NCX
                if media_type == "application/x-dtbncx+xml" {
                    toc_ncx_href = Some(href.to_string());
                }
                // Detect EPUB3 NAV document
                if properties.contains("nav") {
                    nav_href = Some(href.to_string());
                }

                // Only include XHTML/HTML content in manifest
                if media_type.contains("html") || media_type.contains("xml") {
                    manifest.insert(id.to_string(), href.to_string());
                }
            }
        }
    }

    // Read spine order
    let mut spine_items: Vec<String> = Vec::new();
    // Check for toc attribute on <spine> element for NCX reference
    if toc_ncx_href.is_none() {
        if let Ok(sel) = Selector::parse("spine") {
            if let Some(spine_elem) = opf.select(&sel).next() {
                if let Some(toc_id) = spine_elem.value().attr("toc") {
                    // Look up in full manifest (including non-html items)
                    // Re-parse to find the NCX href by id
                    if let Ok(item_sel) = Selector::parse("item") {
                        for elem in opf.select(&item_sel) {
                            if elem.value().attr("id") == Some(toc_id) {
                                toc_ncx_href = elem.value().attr("href").map(|s| s.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

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

    // Extract TOC titles: try EPUB3 NAV first, then fall back to NCX
    let mut toc_titles = std::collections::HashMap::new();

    if let Some(ref nav) = nav_href {
        let nav_path = format!("{}{}", opf_dir, nav);
        if let Ok(nav_xml) =
            read_zip_entry(archive, &nav_path).or_else(|_| read_zip_entry(archive, nav))
        {
            toc_titles = parse_nav_toc(&nav_xml);
        }
    }

    if toc_titles.is_empty() {
        if let Some(ref ncx) = toc_ncx_href {
            let ncx_path = format!("{}{}", opf_dir, ncx);
            if let Ok(ncx_xml) =
                read_zip_entry(archive, &ncx_path).or_else(|_| read_zip_entry(archive, ncx))
            {
                toc_titles = parse_ncx_toc(&ncx_xml);
            }
        }
    }

    Ok(OpfData {
        opf_path,
        spine_items,
        toc_titles,
    })
}

/// Parse an EPUB3 NAV document (nav.xhtml) to extract href -> title mapping.
fn parse_nav_toc(nav_xml: &str) -> std::collections::HashMap<String, String> {
    let mut titles = std::collections::HashMap::new();
    let doc = Html::parse_document(nav_xml);

    // The TOC nav is <nav epub:type="toc"> containing <ol> with <li><a href="...">Title</a></li>
    // We look for all <a> elements inside <nav> that have href attributes
    if let Ok(sel) = Selector::parse("nav a[href]") {
        for elem in doc.select(&sel) {
            if let Some(href) = elem.value().attr("href") {
                let title: String = elem.text().collect::<String>().trim().to_string();
                if !title.is_empty() {
                    // Strip fragment (#...) from href for matching
                    let href_base = href.split('#').next().unwrap_or(href).to_string();
                    if !href_base.is_empty() {
                        titles.insert(href_base, title);
                    }
                }
            }
        }
    }

    titles
}

/// Parse an NCX TOC (toc.ncx) to extract href -> title mapping.
fn parse_ncx_toc(ncx_xml: &str) -> std::collections::HashMap<String, String> {
    let mut titles = std::collections::HashMap::new();
    let doc = Html::parse_document(ncx_xml);

    // NCX has <navPoint> elements with <navLabel><text>Title</text></navLabel>
    // and <content src="chapter1.xhtml"/>
    if let (Ok(navpoint_sel), Ok(text_sel), Ok(content_sel)) = (
        Selector::parse("navPoint"),
        Selector::parse("navLabel text"),
        Selector::parse("content"),
    ) {
        for navpoint in doc.select(&navpoint_sel) {
            let title = navpoint
                .select(&text_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string());
            let src = navpoint
                .select(&content_sel)
                .next()
                .and_then(|e| e.value().attr("src"))
                .map(|s| s.to_string());

            if let (Some(title), Some(src)) = (title, src) {
                if !title.is_empty() {
                    // Strip fragment (#...) from src for matching
                    let src_base = src.split('#').next().unwrap_or(&src).to_string();
                    if !src_base.is_empty() {
                        titles.insert(src_base, title);
                    }
                }
            }
        }
    }

    titles
}

/// Read a file from the ZIP archive.
fn read_zip_entry(archive: &mut zip::ZipArchive<std::fs::File>, path: &str) -> Result<String> {
    let mut entry = archive
        .by_name(path)
        .with_context(|| format!("Entry not found in EPUB: {}", path))?;
    let mut content = String::new();
    entry.read_to_string(&mut content)?;
    Ok(content)
}

/// Resolve the best chapter title from available sources.
/// Priority: TOC title > heading from content > fallback "Chapter N".
fn resolve_chapter_title(
    item_href: &str,
    toc_titles: &std::collections::HashMap<String, String>,
    heading_title: Option<&str>,
    chapter_num: usize,
) -> String {
    // Try TOC title first (strip fragment from href for matching)
    let href_base = item_href.split('#').next().unwrap_or(item_href);
    if let Some(toc_title) = toc_titles.get(href_base) {
        if !toc_title.is_empty() {
            return toc_title.clone();
        }
    }

    // Try heading extracted from the XHTML content
    if let Some(heading) = heading_title {
        if !heading.is_empty() {
            return heading.to_string();
        }
    }

    // Fallback: sequential chapter number
    format!("Chapter {}", chapter_num)
}

/// Extract chapter title (from headings) and text from an XHTML chapter file.
/// Returns (Option<heading_title>, body_text).
fn extract_chapter_text(html_content: &str) -> (Option<String>, String) {
    let document = Html::parse_document(html_content);

    // Try to find a chapter title from headings
    let title = ["h1", "h2", "h3"]
        .iter()
        .find_map(|tag| {
            Selector::parse(tag).ok().and_then(|sel| {
                document
                    .select(&sel)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
            })
        })
        .filter(|t| !t.is_empty());

    // Extract text from body
    let body_selector =
        Selector::parse("body").unwrap_or_else(|_| Selector::parse("html").unwrap());

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
        let (title, content) = extract_chapter_text(html);
        assert_eq!(title, Some("第一章".to_string()));
        assert!(content.contains("最初の段落です。"));
        assert!(content.contains("二番目の段落です。"));
    }

    #[test]
    fn test_extract_chapter_text_no_heading() {
        let html = "<html><body><p>Some text</p></body></html>";
        let (title, _content) = extract_chapter_text(html);
        assert_eq!(title, None);
    }

    #[test]
    fn test_resolve_chapter_title_from_toc() {
        let mut toc = std::collections::HashMap::new();
        toc.insert("chapter1.xhtml".to_string(), "第一章 始まり".to_string());
        // TOC title should take priority over heading
        let result = resolve_chapter_title("chapter1.xhtml", &toc, Some("Heading"), 1);
        assert_eq!(result, "第一章 始まり");
    }

    #[test]
    fn test_resolve_chapter_title_from_heading() {
        let toc = std::collections::HashMap::new();
        let result = resolve_chapter_title("chapter1.xhtml", &toc, Some("第二章"), 2);
        assert_eq!(result, "第二章");
    }

    #[test]
    fn test_resolve_chapter_title_fallback() {
        let toc = std::collections::HashMap::new();
        let result = resolve_chapter_title("chapter1.xhtml", &toc, None, 3);
        assert_eq!(result, "Chapter 3");
    }

    #[test]
    fn test_resolve_chapter_title_toc_with_fragment() {
        let mut toc = std::collections::HashMap::new();
        toc.insert("content.xhtml".to_string(), "プロローグ".to_string());
        // href with fragment should match base href in TOC
        let result = resolve_chapter_title("content.xhtml#part1", &toc, None, 1);
        assert_eq!(result, "プロローグ");
    }

    #[test]
    fn test_parse_ncx_toc() {
        let ncx = r#"<?xml version="1.0" encoding="UTF-8"?>
        <ncx xmlns="http://www.daisy.org/z3986/2005/ncx/">
            <navMap>
                <navPoint id="np1">
                    <navLabel><text>第一章</text></navLabel>
                    <content src="chapter1.xhtml"/>
                </navPoint>
                <navPoint id="np2">
                    <navLabel><text>第二章</text></navLabel>
                    <content src="chapter2.xhtml#start"/>
                </navPoint>
            </navMap>
        </ncx>"#;
        let titles = parse_ncx_toc(ncx);
        assert_eq!(titles.get("chapter1.xhtml"), Some(&"第一章".to_string()));
        assert_eq!(titles.get("chapter2.xhtml"), Some(&"第二章".to_string()));
    }

    #[test]
    fn test_parse_nav_toc() {
        let nav = r#"<html xmlns:epub="http://www.idpf.org/2007/ops">
        <body>
            <nav epub:type="toc">
                <ol>
                    <li><a href="chapter1.xhtml">第一章</a></li>
                    <li><a href="chapter2.xhtml#sec1">第二章</a></li>
                </ol>
            </nav>
        </body>
        </html>"#;
        let titles = parse_nav_toc(nav);
        assert_eq!(titles.get("chapter1.xhtml"), Some(&"第一章".to_string()));
        assert_eq!(titles.get("chapter2.xhtml"), Some(&"第二章".to_string()));
    }
}
