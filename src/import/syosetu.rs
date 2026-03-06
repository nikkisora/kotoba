use anyhow::{Context, Result};
use rusqlite::Connection;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use super::text;
use crate::db::models;

/// Metadata for a Syosetu novel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyosetuNovel {
    pub ncode: String,
    pub title: String,
    pub author: String,
    pub total_chapters: usize,
    pub chapters: Vec<SyosetuChapter>,
}

/// A single chapter from a Syosetu novel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyosetuChapter {
    pub number: usize,
    pub title: String,
    pub text_id: Option<i64>,
    pub word_count: usize,
}

/// Extract the ncode from a Syosetu URL.
/// Handles URLs like:
///   https://ncode.syosetu.com/n1234ab/
///   https://ncode.syosetu.com/n1234ab/1/
///   n1234ab
pub fn parse_ncode(input: &str) -> Result<String> {
    let input = input.trim().trim_end_matches('/');

    // If it looks like a bare ncode
    if input.chars().all(|c| c.is_alphanumeric()) && input.to_lowercase().starts_with('n') {
        return Ok(input.to_lowercase());
    }

    // Try to extract from URL (accept both syosetu.com and syosetu.com)
    for domain in &["ncode.syosetu.com/", "ncode.syosetu.com/"] {
        if let Some(idx) = input.find(domain) {
            let after = &input[idx + domain.len()..];
            let ncode = after.split('/').next().unwrap_or("");
            if !ncode.is_empty() {
                return Ok(ncode.to_lowercase());
            }
        }
    }

    anyhow::bail!("Could not parse ncode from: {}", input)
}

/// Fetch novel metadata (title, author, chapter list) from Syosetu.
pub fn fetch_novel_info(ncode: &str) -> Result<SyosetuNovel> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; kotoba/0.1)")
        .build()?;

    let mut all_chapters: Vec<SyosetuChapter> = Vec::new();
    let mut title = format!("Novel {}", ncode);
    let mut author = String::new();
    let mut page = 1;

    loop {
        let toc_url = if page == 1 {
            format!("https://ncode.syosetu.com/{}/", ncode)
        } else {
            format!("https://ncode.syosetu.com/{}/?p={}", ncode, page)
        };

        let html = client
            .get(&toc_url)
            .send()
            .with_context(|| format!("Failed to fetch {}", toc_url))?
            .text()
            .context("Failed to read response")?;

        let document = Html::parse_document(&html);

        // Extract title and author from the first page only
        if page == 1 {
            // Try new layout first, then legacy
            title = try_select_text(&document, &[".p-novel__title", ".novel_title"])
                .unwrap_or_else(|| format!("Novel {}", ncode));

            author = try_select_text(&document, &[".p-novel__author a", ".novel_writername a"])
                .unwrap_or_default();
        }

        // Extract chapters — try new layout selectors, then legacy
        let chapter_count_before = all_chapters.len();

        // New layout: .p-eplist__subtitle links
        if let Ok(sel) = Selector::parse(".p-eplist__subtitle") {
            for elem in document.select(&sel) {
                let chapter_title = elem.text().collect::<String>().trim().to_string();
                if !chapter_title.is_empty() {
                    all_chapters.push(SyosetuChapter {
                        number: all_chapters.len() + 1,
                        title: chapter_title,
                        text_id: None,
                        word_count: 0,
                    });
                }
            }
        }

        // Legacy layout fallback: .subtitle a
        if all_chapters.len() == chapter_count_before {
            if let Ok(sel) = Selector::parse(".subtitle a") {
                for elem in document.select(&sel) {
                    let chapter_title = elem.text().collect::<String>().trim().to_string();
                    if !chapter_title.is_empty() {
                        all_chapters.push(SyosetuChapter {
                            number: all_chapters.len() + 1,
                            title: chapter_title,
                            text_id: None,
                            word_count: 0,
                        });
                    }
                }
            }
        }

        // Check if there's a next page
        let has_next = if let Ok(sel) = Selector::parse(".c-pager__item--next") {
            // Only follow if it's an <a> tag (not a <span>)
            document.select(&sel).next()
                .map(|e| e.value().name() == "a")
                .unwrap_or(false)
        } else {
            false
        };

        if has_next && all_chapters.len() > chapter_count_before {
            page += 1;
            // Be polite to the server
            std::thread::sleep(std::time::Duration::from_millis(500));
        } else {
            break;
        }
    }

    // If no chapters found, it might be a single-page novel (oneshot)
    if all_chapters.is_empty() {
        let toc_url = format!("https://ncode.syosetu.com/{}/", ncode);
        let html = client.get(&toc_url).send()?.text()?;
        let document = Html::parse_document(&html);
        if let Ok(sel) = Selector::parse("#novel_honbun") {
            if document.select(&sel).next().is_some() {
                all_chapters.push(SyosetuChapter {
                    number: 1,
                    title: title.clone(),
                    text_id: None,
                    word_count: 0,
                });
            }
        }
    }

    let total_chapters = all_chapters.len();

    Ok(SyosetuNovel {
        ncode: ncode.to_string(),
        title,
        author,
        total_chapters,
        chapters: all_chapters,
    })
}

/// Try multiple CSS selectors and return the text of the first match.
fn try_select_text(document: &Html, selectors: &[&str]) -> Option<String> {
    for sel_str in selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(elem) = document.select(&sel).next() {
                let text = elem.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

/// Fetch and import a single chapter from a Syosetu novel.
/// Returns the text_id.
pub fn import_chapter(ncode: &str, chapter: usize, conn: &Connection) -> Result<i64> {
    let url = if chapter == 0 {
        // Oneshot novel - content is on main page
        format!("https://ncode.syosetu.com/{}/", ncode)
    } else {
        format!("https://ncode.syosetu.com/{}/{}/", ncode, chapter)
    };

    println!("Fetching chapter {} from {}...", chapter, url);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; kotoba/0.1)")
        .build()?;

    let html = client
        .get(&url)
        .send()
        .with_context(|| format!("Failed to fetch {}", url))?
        .text()
        .context("Failed to read response")?;

    let document = Html::parse_document(&html);

    // Extract chapter title (try new layout, then legacy)
    let chapter_title = try_select_text(&document, &[".p-novel__subtitle", ".novel_subtitle"])
        .unwrap_or_else(|| format!("Chapter {}", chapter));

    let content = extract_chapter_content(&document)?;

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract chapter text from {}", url);
    }

    let title = format!("{} — Ch.{}", chapter_title, chapter);
    text::import_text(&title, &content, "syosetu", Some(&url), conn)
}

/// Import a chapter quietly (for TUI use).
pub fn import_chapter_quiet(ncode: &str, chapter: usize, conn: &Connection) -> Result<(i64, String)> {
    let url = if chapter == 0 {
        format!("https://ncode.syosetu.com/{}/", ncode)
    } else {
        format!("https://ncode.syosetu.com/{}/{}/", ncode, chapter)
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; kotoba/0.1)")
        .build()?;

    let html = client.get(&url).send()?.text()?;
    let document = Html::parse_document(&html);

    let chapter_title = try_select_text(&document, &[".p-novel__subtitle", ".novel_subtitle"])
        .unwrap_or_else(|| format!("Chapter {}", chapter));

    let content = extract_chapter_content(&document)?;

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract chapter text");
    }

    let title = format!("{} — Ch.{}", chapter_title, chapter);
    let text_id = text::import_text_quiet(&title, &content, "syosetu", Some(&url), conn)?;
    Ok((text_id, title))
}

/// Store novel metadata in the web_sources table.
pub fn save_novel_metadata(novel: &SyosetuNovel, conn: &Connection) -> Result<i64> {
    let metadata = serde_json::to_string(novel)?;
    let ws_id = models::upsert_web_source(
        conn,
        "syosetu",
        &novel.ncode,
        &novel.title,
        &metadata,
    )?;

    // Upsert chapters
    for ch in &novel.chapters {
        models::insert_web_source_chapter(
            conn,
            ws_id,
            ch.number as i32,
            &ch.title,
            ch.text_id,
            ch.word_count as i32,
        )?;
    }

    Ok(ws_id)
}

/// Extract the full chapter content (preface + body + afterword) from a parsed document.
/// Tries both new and legacy Syosetu selectors.
fn extract_chapter_content(document: &Html) -> Result<String> {
    let mut content = String::new();

    // Preface (new: .p-novel__preface, legacy: #novel_p)
    for sel_str in &[".p-novel__preface", "#novel_p"] {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(elem) = document.select(&sel).next() {
                let t = extract_novel_text(&elem);
                if !t.is_empty() {
                    content.push_str(&t);
                    content.push_str("\n\n");
                    break;
                }
            }
        }
    }

    // Main body (new: .p-novel__body, legacy: #novel_honbun)
    for sel_str in &[".p-novel__body", "#novel_honbun"] {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(elem) = document.select(&sel).next() {
                let t = extract_novel_text(&elem);
                if !t.is_empty() {
                    content.push_str(&t);
                    break;
                }
            }
        }
    }

    // Afterword (new: .p-novel__afterword, legacy: #novel_a)
    for sel_str in &[".p-novel__afterword", "#novel_a"] {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(elem) = document.select(&sel).next() {
                let t = extract_novel_text(&elem);
                if !t.is_empty() {
                    content.push_str("\n\n");
                    content.push_str(&t);
                    break;
                }
            }
        }
    }

    Ok(content)
}

/// Extract text from Syosetu novel HTML elements.
/// Converts <br> tags to newlines and strips other HTML.
fn extract_novel_text(element: &scraper::ElementRef) -> String {
    let inner_html = element.inner_html();

    // Replace <br> and <br/> with newlines
    let text = inner_html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("<p>", "");

    // Strip remaining HTML tags
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    let text = re.replace_all(&text, "");

    // Decode HTML entities
    let text = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Clean up whitespace
    let mut result = String::new();
    let mut prev_blank = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank && !result.is_empty() {
                result.push('\n');
                prev_blank = true;
            }
        } else {
            if prev_blank {
                result.push('\n');
            }
            result.push_str(trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ncode_bare() {
        assert_eq!(parse_ncode("n1234ab").unwrap(), "n1234ab");
        assert_eq!(parse_ncode("N1234AB").unwrap(), "n1234ab");
    }

    #[test]
    fn test_parse_ncode_url() {
        assert_eq!(
            parse_ncode("https://ncode.syosetu.com/n1234ab/").unwrap(),
            "n1234ab"
        );
        assert_eq!(
            parse_ncode("https://ncode.syosetu.com/n1234ab/5/").unwrap(),
            "n1234ab"
        );
        // Also accept common misspelling
        assert_eq!(
            parse_ncode("https://ncode.syosetu.com/n1234ab/").unwrap(),
            "n1234ab"
        );
    }

    #[test]
    fn test_parse_ncode_invalid() {
        assert!(parse_ncode("not-an-ncode").is_err());
    }

    #[test]
    fn test_extract_novel_text() {
        let html = r#"<div id="novel_honbun"><p>第一行です。<br>第二行です。</p><p>第三行です。</p></div>"#;
        let document = Html::parse_document(html);
        let sel = Selector::parse("#novel_honbun").unwrap();
        let elem = document.select(&sel).next().unwrap();
        let text = extract_novel_text(&elem);
        assert!(text.contains("第一行です。"));
        assert!(text.contains("第二行です。"));
    }
}
