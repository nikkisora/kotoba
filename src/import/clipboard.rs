use anyhow::{Context, Result};
use rusqlite::Connection;

use super::text;

/// Import text from the system clipboard.
/// Returns the text_id of the imported text.
pub fn import_clipboard(conn: &Connection) -> Result<i64> {
    let mut clipboard = arboard::Clipboard::new().context("Failed to access clipboard")?;

    let content = clipboard.get_text().context(
        "Failed to read text from clipboard (clipboard may be empty or contain non-text data)",
    )?;

    if content.trim().is_empty() {
        anyhow::bail!("Clipboard is empty or contains only whitespace");
    }

    // Auto-generate title from first line or first N characters
    let title = generate_title(&content);

    // Show preview
    let preview: String = content.chars().take(200).collect();
    let ellipsis = if content.chars().count() > 200 {
        "..."
    } else {
        ""
    };
    println!("Clipboard preview:");
    println!("──────────────────────────────────────");
    println!("{}{}", preview, ellipsis);
    println!("──────────────────────────────────────");
    println!("Title: \"{}\"", title);
    println!("Length: {} characters", content.chars().count());
    println!();

    text::import_text(&title, &content, "clipboard", None, conn)
}

/// Import clipboard text without interactive output (for TUI use).
pub fn import_clipboard_quiet(conn: &Connection) -> Result<(i64, String)> {
    let mut clipboard = arboard::Clipboard::new().context("Failed to access clipboard")?;

    let content = clipboard
        .get_text()
        .context("Failed to read text from clipboard")?;

    if content.trim().is_empty() {
        anyhow::bail!("Clipboard is empty");
    }

    let title = generate_title(&content);
    let text_id = text::import_text_quiet(&title, &content, "clipboard", None, conn)?;
    Ok((text_id, title))
}

/// Get a preview of the clipboard content without importing.
pub fn get_clipboard_preview() -> Result<(String, usize)> {
    let mut clipboard = arboard::Clipboard::new().context("Failed to access clipboard")?;

    let content = clipboard
        .get_text()
        .context("Failed to read text from clipboard")?;

    let preview: String = content.chars().take(200).collect();
    let char_count = content.chars().count();
    Ok((preview, char_count))
}

/// Generate a title from text content.
fn generate_title(content: &str) -> String {
    // Use first line, trimmed
    let first_line = content.lines().next().unwrap_or("").trim();

    if first_line.is_empty() {
        return "Clipboard Import".to_string();
    }

    // Truncate to 50 chars
    if first_line.chars().count() > 50 {
        let truncated: String = first_line.chars().take(47).collect();
        format!("{}...", truncated)
    } else {
        first_line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_title_short() {
        assert_eq!(generate_title("吾輩は猫である"), "吾輩は猫である");
    }

    #[test]
    fn test_generate_title_long() {
        let long = "あ".repeat(100);
        let title = generate_title(&long);
        assert!(title.chars().count() <= 50);
        assert!(title.ends_with("..."));
    }

    #[test]
    fn test_generate_title_empty() {
        assert_eq!(generate_title(""), "Clipboard Import");
        // Lines with only whitespace: first line is "  " which trims to empty
        assert_eq!(generate_title("  \n  "), "Clipboard Import");
    }

    #[test]
    fn test_generate_title_multiline() {
        assert_eq!(generate_title("First line\nSecond line"), "First line");
    }
}
