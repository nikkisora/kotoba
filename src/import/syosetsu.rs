use anyhow::{Context, Result};
use rusqlite::Connection;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use super::text;
use crate::db::models;

/// Metadata for a Syosetsu novel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyosetsuNovel {
    pub ncode: String,
    pub title: String,
    pub author: String,
    pub total_chapters: usize,
    pub chapters: Vec<SyosetsuChapter>,
}

/// A single chapter from a Syosetsu novel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyosetsuChapter {
    pub number: usize,
    pub title: String,
    pub text_id: Option<i64>,
    pub word_count: usize,
}

/// Extract the ncode from a Syosetsu URL.
/// Handles URLs like:
///   https://ncode.syosetsu.com/n1234ab/
///   https://ncode.syosetsu.com/n1234ab/1/
///   n1234ab
pub fn parse_ncode(input: &str) -> Result<String> {
    let input = input.trim().trim_end_matches('/');

    // If it looks like a bare ncode
    if input.chars().all(|c| c.is_alphanumeric()) && input.to_lowercase().starts_with('n') {
        return Ok(input.to_lowercase());
    }

    // Try to extract from URL
    if let Some(idx) = input.find("ncode.syosetsu.com/") {
        let after = &input[idx + "ncode.syosetsu.com/".len()..];
        let ncode = after.split('/').next().unwrap_or("");
        if !ncode.is_empty() {
            return Ok(ncode.to_lowercase());
        }
    }

    anyhow::bail!("Could not parse ncode from: {}", input)
}

/// Fetch novel metadata (title, author, chapter list) from Syosetsu.
pub fn fetch_novel_info(ncode: &str) -> Result<SyosetsuNovel> {
    let toc_url = format!("https://ncode.syosetsu.com/{}/", ncode);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; kotoba/0.1)")
        .build()?;

    let html = client
        .get(&toc_url)
        .send()
        .with_context(|| format!("Failed to fetch {}", toc_url))?
        .text()
        .context("Failed to read response")?;

    let document = Html::parse_document(&html);

    // Extract title
    let title = if let Ok(sel) = Selector::parse(".novel_title") {
        document.select(&sel).next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("Novel {}", ncode))
    } else {
        format!("Novel {}", ncode)
    };

    // Extract author
    let author = if let Ok(sel) = Selector::parse(".novel_writername a") {
        document.select(&sel).next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Extract chapters from table of contents
    let mut chapters = Vec::new();

    // Syosetsu chapter links are in .novel_sublist2 > .subtitle > a
    if let Ok(sel) = Selector::parse(".subtitle a") {
        for (i, elem) in document.select(&sel).enumerate() {
            let chapter_title = elem.text().collect::<String>().trim().to_string();
            chapters.push(SyosetsuChapter {
                number: i + 1,
                title: chapter_title,
                text_id: None,
                word_count: 0,
            });
        }
    }

    // If no chapters found, it might be a single-page novel (oneshot)
    if chapters.is_empty() {
        // Check if there's novel content directly on this page
        if let Ok(sel) = Selector::parse("#novel_honbun") {
            if document.select(&sel).next().is_some() {
                chapters.push(SyosetsuChapter {
                    number: 1,
                    title: title.clone(),
                    text_id: None,
                    word_count: 0,
                });
            }
        }
    }

    let total_chapters = chapters.len();

    Ok(SyosetsuNovel {
        ncode: ncode.to_string(),
        title,
        author,
        total_chapters,
        chapters,
    })
}

/// Fetch and import a single chapter from a Syosetsu novel.
/// Returns the text_id.
pub fn import_chapter(ncode: &str, chapter: usize, conn: &Connection) -> Result<i64> {
    let url = if chapter == 0 {
        // Oneshot novel - content is on main page
        format!("https://ncode.syosetsu.com/{}/", ncode)
    } else {
        format!("https://ncode.syosetsu.com/{}/{}/", ncode, chapter)
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

    // Extract chapter title
    let chapter_title = if let Ok(sel) = Selector::parse(".novel_subtitle") {
        document.select(&sel).next()
            .map(|e| e.text().collect::<String>().trim().to_string())
    } else {
        None
    }.unwrap_or_else(|| format!("Chapter {}", chapter));

    // Extract novel body text
    let mut content = String::new();

    // Preface
    if let Ok(sel) = Selector::parse("#novel_p") {
        if let Some(elem) = document.select(&sel).next() {
            let preface = extract_novel_text(&elem);
            if !preface.is_empty() {
                content.push_str(&preface);
                content.push_str("\n\n");
            }
        }
    }

    // Main body
    if let Ok(sel) = Selector::parse("#novel_honbun") {
        if let Some(elem) = document.select(&sel).next() {
            content.push_str(&extract_novel_text(&elem));
        }
    }

    // Afterword
    if let Ok(sel) = Selector::parse("#novel_a") {
        if let Some(elem) = document.select(&sel).next() {
            let afterword = extract_novel_text(&elem);
            if !afterword.is_empty() {
                content.push_str("\n\n");
                content.push_str(&afterword);
            }
        }
    }

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract chapter text from {}", url);
    }

    let title = format!("{} — Ch.{}", chapter_title, chapter);
    text::import_text(&title, &content, "syosetsu", Some(&url), conn)
}

/// Import a chapter quietly (for TUI use).
pub fn import_chapter_quiet(ncode: &str, chapter: usize, conn: &Connection) -> Result<(i64, String)> {
    let url = if chapter == 0 {
        format!("https://ncode.syosetsu.com/{}/", ncode)
    } else {
        format!("https://ncode.syosetsu.com/{}/{}/", ncode, chapter)
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; kotoba/0.1)")
        .build()?;

    let html = client.get(&url).send()?.text()?;
    let document = Html::parse_document(&html);

    let chapter_title = if let Ok(sel) = Selector::parse(".novel_subtitle") {
        document.select(&sel).next()
            .map(|e| e.text().collect::<String>().trim().to_string())
    } else {
        None
    }.unwrap_or_else(|| format!("Chapter {}", chapter));

    let mut content = String::new();

    if let Ok(sel) = Selector::parse("#novel_p") {
        if let Some(elem) = document.select(&sel).next() {
            let t = extract_novel_text(&elem);
            if !t.is_empty() {
                content.push_str(&t);
                content.push_str("\n\n");
            }
        }
    }

    if let Ok(sel) = Selector::parse("#novel_honbun") {
        if let Some(elem) = document.select(&sel).next() {
            content.push_str(&extract_novel_text(&elem));
        }
    }

    if let Ok(sel) = Selector::parse("#novel_a") {
        if let Some(elem) = document.select(&sel).next() {
            let t = extract_novel_text(&elem);
            if !t.is_empty() {
                content.push_str("\n\n");
                content.push_str(&t);
            }
        }
    }

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract chapter text");
    }

    let title = format!("{} — Ch.{}", chapter_title, chapter);
    let text_id = text::import_text_quiet(&title, &content, "syosetsu", Some(&url), conn)?;
    Ok((text_id, title))
}

/// Store novel metadata in the web_sources table.
pub fn save_novel_metadata(novel: &SyosetsuNovel, conn: &Connection) -> Result<i64> {
    let metadata = serde_json::to_string(novel)?;
    let ws_id = models::upsert_web_source(
        conn,
        "syosetsu",
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

/// Extract text from Syosetsu novel HTML elements.
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
            parse_ncode("https://ncode.syosetsu.com/n1234ab/").unwrap(),
            "n1234ab"
        );
        assert_eq!(
            parse_ncode("https://ncode.syosetsu.com/n1234ab/5/").unwrap(),
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
