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
    /// Number of chapters to keep preprocessed ahead of the reader.
    #[serde(default = "default_preprocess_ahead")]
    pub preprocess_ahead: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrsConfig {
    #[serde(default = "default_answer_mode")]
    pub default_answer_mode: String,
    #[serde(default = "default_new_cards")]
    pub new_cards_per_day: u32,
    #[serde(default)]
    pub max_reviews_per_session: u32,
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
fn default_answer_mode() -> String {
    "meaning_recall".into()
}
fn default_new_cards() -> u32 {
    20
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
            preprocess_ahead: default_preprocess_ahead(),
        }
    }
}

impl Default for SrsConfig {
    fn default() -> Self {
        Self {
            default_answer_mode: default_answer_mode(),
            new_cards_per_day: default_new_cards(),
            max_reviews_per_session: 0,
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

    /// Ensure the data directory exists (creates it if needed).
    pub fn ensure_data_dir(&self) -> Result<()> {
        let db_dir = self.db_path().parent().map(|p| p.to_path_buf());
        if let Some(dir) = db_dir {
            if !dir.exists() {
                std::fs::create_dir_all(&dir).with_context(|| {
                    format!("Failed to create data directory: {}", dir.display())
                })?;
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
