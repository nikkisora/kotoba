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
    /// Group/arc name this chapter belongs to (from `<div class="p-eplist__chapter-title">`).
    #[serde(default)]
    pub group: String,
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

/// Result of fetching a single ToC page from Syosetu.
pub struct SyosetuPageResult {
    pub title: String,
    pub author: String,
    pub chapters: Vec<SyosetuChapter>,
    pub has_next_page: bool,
}

/// Fetch a single page of the Syosetu novel table of contents.
/// Returns the chapters found on that page along with their group names,
/// plus whether there is a next page.
pub fn fetch_novel_page(
    client: &reqwest::blocking::Client,
    ncode: &str,
    page: usize,
    chapter_offset: usize,
) -> Result<SyosetuPageResult> {
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

    // Extract title and author (meaningful on page 1, but always available)
    let title = try_select_text(&document, &[".p-novel__title", ".novel_title"])
        .unwrap_or_else(|| format!("Novel {}", ncode));
    let author = try_select_text(&document, &[".p-novel__author a", ".novel_writername a"])
        .unwrap_or_default();

    let mut chapters: Vec<SyosetuChapter> = Vec::new();

    // New layout: walk the episode list and track chapter-title (group) headers.
    // The structure is: .p-eplist contains .p-eplist__chapter-title divs
    // interspersed with .p-eplist__subtitle links.
    let mut found_new_layout = false;
    if let (Ok(chapter_title_sel), Ok(subtitle_sel)) = (
        Selector::parse(".p-eplist__chapter-title"),
        Selector::parse(".p-eplist__subtitle"),
    ) {
        // Collect all chapter-title and subtitle elements in document order
        // by walking the eplist container.
        if let Ok(eplist_sel) = Selector::parse(".p-eplist") {
            if let Some(eplist) = document.select(&eplist_sel).next() {
                let mut current_group = String::new();
                // Walk direct children of the eplist
                for child in eplist.children() {
                    if let Some(elem_ref) = scraper::ElementRef::wrap(child) {
                        // Check if this is a chapter-title (group header)
                        if chapter_title_sel.matches(&elem_ref) {
                            current_group = elem_ref.text().collect::<String>().trim().to_string();
                        }
                        // Check if this is a subtitle (chapter entry) — or contains one
                        if subtitle_sel.matches(&elem_ref) {
                            let chapter_title =
                                elem_ref.text().collect::<String>().trim().to_string();
                            if !chapter_title.is_empty() {
                                found_new_layout = true;
                                chapters.push(SyosetuChapter {
                                    number: chapter_offset + chapters.len() + 1,
                                    title: chapter_title,
                                    text_id: None,
                                    word_count: 0,
                                    group: current_group.clone(),
                                });
                            }
                        }
                        // Also check children (the subtitle might be nested in a sublist)
                        for sub in elem_ref.select(&subtitle_sel) {
                            if !subtitle_sel.matches(&elem_ref) {
                                let chapter_title =
                                    sub.text().collect::<String>().trim().to_string();
                                if !chapter_title.is_empty() {
                                    found_new_layout = true;
                                    chapters.push(SyosetuChapter {
                                        number: chapter_offset + chapters.len() + 1,
                                        title: chapter_title,
                                        text_id: None,
                                        word_count: 0,
                                        group: current_group.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Legacy layout fallback: .subtitle a
    if !found_new_layout {
        if let Ok(sel) = Selector::parse(".subtitle a") {
            for elem in document.select(&sel) {
                let chapter_title = elem.text().collect::<String>().trim().to_string();
                if !chapter_title.is_empty() {
                    chapters.push(SyosetuChapter {
                        number: chapter_offset + chapters.len() + 1,
                        title: chapter_title,
                        text_id: None,
                        word_count: 0,
                        group: String::new(),
                    });
                }
            }
        }
    }

    // Check if there's a next page
    let has_next = if let Ok(sel) = Selector::parse(".c-pager__item--next") {
        document
            .select(&sel)
            .next()
            .map(|e| e.value().name() == "a")
            .unwrap_or(false)
    } else {
        false
    };

    let has_next_page = has_next && !chapters.is_empty();
    Ok(SyosetuPageResult {
        title,
        author,
        chapters,
        has_next_page,
    })
}

/// Fetch novel metadata (title, author, chapter list) from Syosetu.
/// Fetches all pages synchronously. For incremental loading, use
/// `fetch_novel_page` in a loop instead.
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
        let result = fetch_novel_page(&client, ncode, page, all_chapters.len())?;

        if page == 1 {
            title = result.title;
            author = result.author;
        }

        let got_chapters = !result.chapters.is_empty();
        all_chapters.extend(result.chapters);

        if result.has_next_page && got_chapters {
            page += 1;
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
                    group: String::new(),
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

/// Format the title for a syosetu chapter text stored in the DB.
/// Produces: "Novel Title — Chapter Subtitle" (with novel name for context).
/// If the novel title is empty/unavailable, falls back to "Ch.N: Chapter Subtitle".
pub fn format_chapter_title(
    novel_title: &str,
    chapter_number: usize,
    chapter_subtitle: &str,
) -> String {
    if novel_title.is_empty() {
        format!("Ch.{} — {}", chapter_number, chapter_subtitle)
    } else {
        format!("{} — {}", novel_title, chapter_subtitle)
    }
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

    // Extract novel title and chapter subtitle from the page
    let novel_title =
        try_select_text(&document, &[".p-novel__title a", ".novel_title"]).unwrap_or_default();
    let chapter_subtitle = try_select_text(&document, &[".p-novel__subtitle", ".novel_subtitle"])
        .unwrap_or_else(|| format!("Chapter {}", chapter));

    let content = extract_chapter_content(&document)?;

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract chapter text from {}", url);
    }

    let title = format_chapter_title(&novel_title, chapter, &chapter_subtitle);
    text::import_text(&title, &content, "syosetu", Some(&url), conn)
}

/// Fetch chapter content without any DB access (for two-phase background import).
/// Returns (title, content, url).
pub fn fetch_chapter_content(ncode: &str, chapter: usize) -> Result<(String, String, String)> {
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

    // Extract novel title and chapter subtitle from the page
    let novel_title =
        try_select_text(&document, &[".p-novel__title a", ".novel_title"]).unwrap_or_default();
    let chapter_subtitle = try_select_text(&document, &[".p-novel__subtitle", ".novel_subtitle"])
        .unwrap_or_else(|| format!("Chapter {}", chapter));

    let content = extract_chapter_content(&document)?;

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract chapter text");
    }

    let title = format_chapter_title(&novel_title, chapter, &chapter_subtitle);
    Ok((title, content, url))
}

/// Import a chapter quietly (for TUI use).
pub fn import_chapter_quiet(
    ncode: &str,
    chapter: usize,
    conn: &Connection,
) -> Result<(i64, String)> {
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

    // Extract novel title and chapter subtitle from the page
    let novel_title =
        try_select_text(&document, &[".p-novel__title a", ".novel_title"]).unwrap_or_default();
    let chapter_subtitle = try_select_text(&document, &[".p-novel__subtitle", ".novel_subtitle"])
        .unwrap_or_else(|| format!("Chapter {}", chapter));

    let content = extract_chapter_content(&document)?;

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract chapter text");
    }

    let title = format_chapter_title(&novel_title, chapter, &chapter_subtitle);
    let text_id = text::import_text_quiet(&title, &content, "syosetu", Some(&url), conn)?;
    Ok((text_id, title))
}

/// Store novel metadata in the web_sources table.
pub fn save_novel_metadata(novel: &SyosetuNovel, conn: &Connection) -> Result<i64> {
    let metadata = serde_json::to_string(novel)?;
    let ws_id = models::upsert_web_source(conn, "syosetu", &novel.ncode, &novel.title, &metadata)?;

    // Upsert chapters
    for ch in &novel.chapters {
        models::insert_web_source_chapter(
            conn,
            ws_id,
            ch.number as i32,
            &ch.title,
            ch.text_id,
            ch.word_count as i32,
            &ch.group,
        )?;
    }

    Ok(ws_id)
}

/// Save a single page of chapters to the DB. Used for incremental loading.
/// Creates/updates the web_source row and inserts chapters from this page.
pub fn save_novel_metadata_page(
    ncode: &str,
    title: &str,
    author: &str,
    chapters: &[SyosetuChapter],
    conn: &Connection,
) -> Result<i64> {
    // Build a minimal metadata JSON for the web_source row
    let metadata = serde_json::json!({
        "ncode": ncode,
        "title": title,
        "author": author,
    });
    let ws_id = models::upsert_web_source(conn, "syosetu", ncode, title, &metadata.to_string())?;

    for ch in chapters {
        models::insert_web_source_chapter(
            conn,
            ws_id,
            ch.number as i32,
            &ch.title,
            ch.text_id,
            ch.word_count as i32,
            &ch.group,
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

    #[test]
    fn test_format_chapter_title_with_novel() {
        let title = format_chapter_title("転生したらスライムだった件", 5, "リムルの決意");
        assert_eq!(title, "転生したらスライムだった件 — リムルの決意");
    }

    #[test]
    fn test_format_chapter_title_no_novel() {
        let title = format_chapter_title("", 5, "リムルの決意");
        assert_eq!(title, "Ch.5 — リムルの決意");
    }
}
