use anyhow::{Context, Result};
use regex::Regex;
use rusqlite::Connection;
use std::path::Path;

use super::text;

/// Import a subtitle file (.srt or .ass/.ssa) into the database.
/// Returns the text_id.
pub fn import_subtitle(path: &Path, conn: &Connection) -> Result<i64> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read subtitle file: {}", path.display()))?;

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled Subtitle")
        .to_string();

    let extracted = match extension.as_str() {
        "srt" => parse_srt(&content)?,
        "ass" | "ssa" => parse_ass(&content)?,
        _ => anyhow::bail!("Unsupported subtitle format: .{}", extension),
    };

    if extracted.trim().is_empty() {
        anyhow::bail!("No text content found in subtitle file");
    }

    println!(
        "Extracted {} characters from subtitle",
        extracted.chars().count()
    );

    text::import_text(&title, &extracted, "subtitle", None, conn)
}

/// Import subtitle file quietly (for TUI use).
pub fn import_subtitle_quiet(path: &Path, conn: &Connection) -> Result<(i64, String)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read: {}", path.display()))?;

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled Subtitle")
        .to_string();

    let extracted = match extension.as_str() {
        "srt" => parse_srt(&content)?,
        "ass" | "ssa" => parse_ass(&content)?,
        _ => anyhow::bail!("Unsupported subtitle format: .{}", extension),
    };

    if extracted.trim().is_empty() {
        anyhow::bail!("No text content found in subtitle file");
    }

    let text_id = text::import_text_quiet(&title, &extracted, "subtitle", None, conn)?;
    Ok((text_id, title))
}

/// Parse an SRT subtitle file and extract text content.
/// Groups subtitle blocks into paragraphs based on timing gaps.
fn parse_srt(content: &str) -> Result<String> {
    let timing_re =
        Regex::new(r"^\d{2}:\d{2}:\d{2}[,\.]\d{3}\s*-->\s*\d{2}:\d{2}:\d{2}[,\.]\d{3}").unwrap();
    let index_re = Regex::new(r"^\d+$").unwrap();
    let html_tag_re = Regex::new(r"<[^>]+>").unwrap();

    let mut paragraphs: Vec<Vec<String>> = Vec::new();
    let mut current_group: Vec<String> = Vec::new();
    let mut line_count = 0;

    let mut in_text = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            in_text = false;
            continue;
        }

        if index_re.is_match(trimmed) && !in_text {
            continue;
        }

        if timing_re.is_match(trimmed) {
            in_text = true;
            continue;
        }

        if in_text {
            // Strip HTML tags (like <i>, <b>, <font>)
            let clean = html_tag_re.replace_all(trimmed, "").trim().to_string();
            if !clean.is_empty() {
                current_group.push(clean);
                line_count += 1;

                // Group every ~10 subtitle lines into a paragraph
                if line_count % 10 == 0 && !current_group.is_empty() {
                    paragraphs.push(current_group);
                    current_group = Vec::new();
                }
            }
        }
    }

    if !current_group.is_empty() {
        paragraphs.push(current_group);
    }

    // Join: each line becomes a sentence, paragraphs separated by blank lines
    let result: String = paragraphs
        .iter()
        .map(|group| group.join("\n"))
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(result)
}

/// Parse an ASS/SSA subtitle file and extract dialogue text.
fn parse_ass(content: &str) -> Result<String> {
    let style_tag_re = Regex::new(r"\{[^}]*\}").unwrap();

    let mut lines: Vec<String> = Vec::new();
    let mut in_events = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("[Events]") {
            in_events = true;
            continue;
        }

        if trimmed.starts_with('[') && in_events {
            break; // New section, stop
        }

        if !in_events {
            continue;
        }

        // Dialogue lines: Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
        if trimmed.starts_with("Dialogue:") || trimmed.starts_with("Comment:") {
            // Skip Comment lines
            if trimmed.starts_with("Comment:") {
                continue;
            }

            // Find the text part (everything after the 9th comma)
            let parts: Vec<&str> = trimmed.splitn(10, ',').collect();
            if parts.len() >= 10 {
                let text_part = parts[9].trim();

                // Strip ASS style tags like {\i1}, {\b1}, {\pos(x,y)}, etc.
                let clean = style_tag_re.replace_all(text_part, "");

                // Replace \N and \n with actual newlines
                let clean = clean.replace("\\N", "\n").replace("\\n", "\n");

                let clean = clean.trim().to_string();
                if !clean.is_empty() {
                    lines.push(clean);
                }
            }
        }
    }

    // Group into paragraphs (every 10 lines)
    let mut paragraphs: Vec<String> = Vec::new();
    for chunk in lines.chunks(10) {
        paragraphs.push(chunk.join("\n"));
    }

    Ok(paragraphs.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_srt() {
        let srt = "\
1
00:00:01,000 --> 00:00:03,000
こんにちは。

2
00:00:04,000 --> 00:00:06,000
<i>お元気ですか。</i>

3
00:00:07,000 --> 00:00:09,000
はい、元気です。
";
        let result = parse_srt(srt).unwrap();
        assert!(result.contains("こんにちは。"));
        assert!(result.contains("お元気ですか。")); // HTML stripped
        assert!(result.contains("元気です。"));
    }

    #[test]
    fn test_parse_ass() {
        let ass = "\
[Script Info]
Title: Test

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,こんにちは。
Dialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,{\\i1}お元気ですか。{\\i0}
Comment: 0,0:00:07.00,0:00:09.00,Default,,0,0,0,,これはコメントです。
Dialogue: 0,0:00:10.00,0:00:12.00,Default,,0,0,0,,はい\\N元気です。
";
        let result = parse_ass(ass).unwrap();
        assert!(result.contains("こんにちは。"));
        assert!(result.contains("お元気ですか。")); // Style tags stripped
        assert!(!result.contains("コメントです")); // Comment skipped
        assert!(result.contains("はい\n元気です。")); // \N converted
    }

    #[test]
    fn test_parse_srt_empty() {
        let result = parse_srt("").unwrap();
        assert!(result.is_empty());
    }
}
