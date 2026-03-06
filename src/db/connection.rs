use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

/// Open an existing database or create a new one at the given path.
/// Ensures the parent directory exists and enables WAL mode.
pub fn open_or_create(path: &Path) -> Result<Connection> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("Failed to open database: {}", path.display()))?;

    // Enable WAL mode for better concurrent read performance
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    // Enable foreign keys
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    // Set busy timeout so concurrent writers retry instead of failing immediately
    conn.execute_batch("PRAGMA busy_timeout=5000;")?;

    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}
