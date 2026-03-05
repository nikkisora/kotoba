use anyhow::{Context, Result};
use rusqlite::Connection;
use scraper::{Html, Selector};

use super::text;

/// Import a web page by URL. Fetches HTML, extracts article text, imports.
/// Returns the text_id.
pub fn import_url(url: &str, conn: &Connection) -> Result<i64> {
    println!("Fetching {}...", url);

    let html = fetch_html(url)?;
    let (title, content) = extract_article(&html, url)?;

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract any text content from {}", url);
    }

    println!("Title: \"{}\"", title);
    println!("Extracted {} characters", content.chars().count());

    text::import_text(&title, &content, "web", Some(url), conn)
}

/// Import URL without CLI output (for TUI use).
pub fn import_url_quiet(url: &str, conn: &Connection) -> Result<(i64, String)> {
    let html = fetch_html(url)?;
    let (title, content) = extract_article(&html, url)?;

    if content.trim().is_empty() {
        anyhow::bail!("Could not extract text content from {}", url);
    }

    let text_id = text::import_text_quiet(&title, &content, "web", Some(url), conn)?;
    Ok((text_id, title))
}

/// Fetch HTML content from a URL using reqwest (blocking).
fn fetch_html(url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; kotoba/0.1)")
        .build()
        .context("Failed to create HTTP client")?;

    let response = client
        .get(url)
        .send()
        .with_context(|| format!("Failed to fetch {}", url))?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {} for {}", response.status(), url);
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v: &reqwest::header::HeaderValue| v.to_str().ok())
        .unwrap_or("");

    if !content_type.is_empty() && !content_type.contains("html") && !content_type.contains("text/") {
        anyhow::bail!("URL returned non-HTML content: {}", content_type);
    }

    response.text().context("Failed to read response body")
}

/// Extract article title and main text content from HTML.
/// Uses a readability-style heuristic: find the element with the most text.
fn extract_article(html: &str, _url: &str) -> Result<(String, String)> {
    let document = Html::parse_document(html);

    // Extract title
    let title = extract_title(&document);

    // Try common article selectors first
    let article_selectors = [
        "article",
        "main",
        "[role='main']",
        ".entry-content",
        ".article-body",
        ".post-content",
        ".content",
        "#novel_honbun",    // Syosetsu novel body
        "#novel_p",         // Syosetsu preface
        "#novel_a",         // Syosetsu afterword
        ".novel_view",      // Alternative syosetsu
    ];

    for selector_str in &article_selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            let elements: Vec<_> = document.select(&selector).collect();
            if let Some(elem) = elements.first() {
                let text = extract_text_from_element(elem);
                if text.chars().count() > 100 {
                    return Ok((title, clean_text(&text)));
                }
            }
        }
    }

    // Fallback: extract text from body, skipping nav/header/footer/script/style
    let body_text = extract_body_text(&document);
    if body_text.trim().is_empty() {
        anyhow::bail!("Could not extract meaningful text content from HTML");
    }

    Ok((title, clean_text(&body_text)))
}

/// Extract the page title from HTML.
fn extract_title(document: &Html) -> String {
    if let Ok(selector) = Selector::parse("title") {
        if let Some(elem) = document.select(&selector).next() {
            let title = elem.text().collect::<String>().trim().to_string();
            if !title.is_empty() {
                return title;
            }
        }
    }

    // Try og:title
    if let Ok(selector) = Selector::parse("meta[property='og:title']") {
        if let Some(elem) = document.select(&selector).next() {
            if let Some(content) = elem.value().attr("content") {
                let title = content.trim().to_string();
                if !title.is_empty() {
                    return title;
                }
            }
        }
    }

    // Try h1
    if let Ok(selector) = Selector::parse("h1") {
        if let Some(elem) = document.select(&selector).next() {
            let title = elem.text().collect::<String>().trim().to_string();
            if !title.is_empty() {
                return title;
            }
        }
    }

    "Untitled Web Import".to_string()
}

/// Extract text content from a specific HTML element.
fn extract_text_from_element(element: &scraper::ElementRef) -> String {
    let mut text = String::new();
    for node in element.text() {
        text.push_str(node);
    }
    text
}

/// Extract text from <body>, skipping unwanted elements.
fn extract_body_text(document: &Html) -> String {
    let body_selector = Selector::parse("body").unwrap();
    let mut text = String::new();

    if let Some(body) = document.select(&body_selector).next() {
        // Simple approach: get all text, but it's from body which is better than nothing
        for node in body.text() {
            let trimmed = node.trim();
            if !trimmed.is_empty() {
                text.push_str(trimmed);
                text.push('\n');
            }
        }
    }

    // Note: The simple .text() approach doesn't let us skip specific elements easily.
    // For a more sophisticated approach we'd need to walk the DOM tree.
    // This is good enough for most Japanese content sites.
    text
}

/// Clean up extracted text: normalize whitespace, merge blank lines.
fn clean_text(text: &str) -> String {
    let mut result = String::new();
    let mut blank_count = 0;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_count += 1;
        } else {
            if !result.is_empty() {
                if blank_count > 0 {
                    result.push_str("\n\n"); // Paragraph break (at most one blank line)
                } else {
                    result.push('\n');
                }
            }
            result.push_str(trimmed);
            blank_count = 0;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_article_basic() {
        let html = r#"
        <html>
        <head><title>テスト記事</title></head>
        <body>
            <nav>Navigation</nav>
            <article>
                <p>これはテスト記事です。日本語のコンテンツを含んでいます。</p>
                <p>二番目の段落。もっと日本語のテキストがここにあります。とても長い文章を作成して百文字以上のテキストになるようにします。</p>
            </article>
            <footer>Footer</footer>
        </body>
        </html>
        "#;
        let (title, content) = extract_article(html, "http://example.com").unwrap();
        assert_eq!(title, "テスト記事");
        assert!(content.contains("テスト記事です"));
    }

    #[test]
    fn test_clean_text() {
        let input = "  First line  \n\n\n\n  Second line  \n  Third line  ";
        let cleaned = clean_text(input);
        assert_eq!(cleaned, "First line\n\nSecond line\nThird line");
    }

    #[test]
    fn test_extract_title_fallback() {
        let html = "<html><body><h1>見出し</h1><p>Content</p></body></html>";
        let document = Html::parse_document(html);
        let title = extract_title(&document);
        assert_eq!(title, "見出し");
    }
}
