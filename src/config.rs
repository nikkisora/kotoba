use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub general: GeneralConfig,
    #[serde(default)]
    pub reader: ReaderConfig,
    #[serde(default)]
    pub srs: SrsConfig,
    #[serde(default)]
    pub llm: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Path to the SQLite database. Defaults to XDG data dir (~/.local/share/kotoba/kotoba.db).
    pub db_path: Option<PathBuf>,
    /// Path to JMdict XML file. Defaults to XDG data dir (~/.local/share/kotoba/JMdict_e.xml).
    pub jmdict_path: Option<PathBuf>,
    #[serde(default = "default_theme")]
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderConfig {
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
    #[serde(default = "default_true")]
    pub furigana: bool,
    /// Add a 1-row gap between sentences for readability.
    #[serde(default = "default_true")]
    pub sentence_gaps: bool,
    /// Number of chapters to keep preprocessed ahead of the reader.
    #[serde(default = "default_preprocess_ahead")]
    pub preprocess_ahead: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrsConfig {
    #[serde(default = "default_new_cards")]
    pub new_cards_per_day: u32,
    #[serde(default)]
    pub max_reviews_per_session: u32,
    /// Review order: "due_first" (default) or "random".
    #[serde(default = "default_review_order")]
    pub review_order: String,
    /// Require typed reading input for word cards during review.
    /// When enabled, word cards ask the user to type the reading (accepts hiragana, romaji, or kanji).
    #[serde(default)]
    pub require_typed_input: bool,
    /// Enable sentence cloze variant during word card review.
    /// When enabled, word cards randomly show either the normal word front
    /// or a sentence cloze front (word blanked in the sentence).
    #[serde(default)]
    pub enable_sentence_cloze: bool,
    /// Probability (0-100) of showing a sentence cloze variant instead of
    /// the normal word front. Only used when enable_sentence_cloze is true.
    #[serde(default = "default_cloze_ratio")]
    pub sentence_cloze_ratio: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_llm_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
}

fn default_theme() -> String {
    "tokyo-night".into()
}
fn default_sidebar_width() -> u16 {
    30
}
fn default_true() -> bool {
    true
}
fn default_preprocess_ahead() -> usize {
    3
}
fn default_cloze_ratio() -> u32 {
    50
}
fn default_new_cards() -> u32 {
    20
}
fn default_review_order() -> String {
    "due_first".into()
}
fn default_llm_endpoint() -> String {
    "https://api.openai.com/v1".into()
}
fn default_model() -> String {
    "gpt-4o".into()
}
fn default_max_tokens() -> usize {
    2048
}

impl Default for ReaderConfig {
    fn default() -> Self {
        Self {
            sidebar_width: default_sidebar_width(),
            furigana: true,
            sentence_gaps: true,
            preprocess_ahead: default_preprocess_ahead(),
        }
    }
}

impl Default for SrsConfig {
    fn default() -> Self {
        Self {
            new_cards_per_day: 20,
            max_reviews_per_session: 0,
            review_order: default_review_order(),
            require_typed_input: false,
            enable_sentence_cloze: false,
            sentence_cloze_ratio: default_cloze_ratio(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            endpoint: default_llm_endpoint(),
            api_key: String::new(),
            model: default_model(),
            max_tokens: default_max_tokens(),
        }
    }
}

impl AppConfig {
    /// Default data directory: ~/.local/share/kotoba (or platform equivalent).
    fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kotoba")
    }

    /// Resolved DB path — uses config override or defaults to XDG data dir.
    pub fn db_path(&self) -> PathBuf {
        self.general
            .db_path
            .clone()
            .unwrap_or_else(|| Self::default_data_dir().join("kotoba.db"))
    }

    /// Resolved JMdict path — uses config override or defaults to XDG data dir.
    pub fn jmdict_path(&self) -> PathBuf {
        self.general
            .jmdict_path
            .clone()
            .unwrap_or_else(|| Self::default_data_dir().join("JMdict_e.xml"))
    }

    /// Ensure the data directory and themes sub-directory exist (creates them if needed).
    pub fn ensure_data_dir(&self) -> Result<()> {
        let db_dir = self.db_path().parent().map(|p| p.to_path_buf());
        if let Some(dir) = db_dir {
            if !dir.exists() {
                std::fs::create_dir_all(&dir).with_context(|| {
                    format!("Failed to create data directory: {}", dir.display())
                })?;
            }
            // Also create the themes sub-directory
            let themes_dir = dir.join("themes");
            if !themes_dir.exists() {
                let _ = std::fs::create_dir_all(&themes_dir);
            }
        }
        Ok(())
    }

    pub fn load(path: Option<&Path>) -> Result<Self> {
        // Try explicit path, then XDG config, then local, then defaults
        let config: Self = if let Some(p) = path {
            let content = std::fs::read_to_string(p)
                .with_context(|| format!("Failed to read config file: {}", p.display()))?;
            toml::from_str(&content).context("Failed to parse config file")?
        } else if let Some(cfg_path) = dirs::config_dir()
            .map(|d| d.join("kotoba").join("kotoba.toml"))
            .filter(|p| p.exists())
        {
            let content = std::fs::read_to_string(&cfg_path)?;
            toml::from_str(&content).context("Failed to parse config file")?
        } else {
            let local = Path::new("kotoba.toml");
            if local.exists() {
                let content = std::fs::read_to_string(local)?;
                toml::from_str(&content).context("Failed to parse config file")?
            } else {
                Self::default()
            }
        };

        config.ensure_data_dir()?;
        Ok(config)
    }
}

/// Save the current config to a TOML file.
/// Writes to the XDG config location or the local kotoba.toml.
pub fn save_config(config: &AppConfig) -> Result<()> {
    let content = toml::to_string_pretty(config).context("Failed to serialize config")?;

    // Try XDG config dir first, then local
    let config_path = if let Some(cfg_dir) = dirs::config_dir() {
        let dir = cfg_dir.join("kotoba");
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        dir.join("kotoba.toml")
    } else {
        PathBuf::from("kotoba.toml")
    };

    std::fs::write(&config_path, content)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;
    Ok(())
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                db_path: None,
                jmdict_path: None,
                theme: default_theme(),
            },
            reader: ReaderConfig::default(),
            srs: SrsConfig::default(),
            llm: LlmConfig::default(),
        }
    }
}
