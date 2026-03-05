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
    pub db_path: PathBuf,
    pub jmdict_path: PathBuf,
    #[serde(default = "default_theme")]
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderConfig {
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
    #[serde(default = "default_true")]
    pub furigana: bool,
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

fn default_theme() -> String { "tokyo-night".into() }
fn default_sidebar_width() -> u16 { 30 }
fn default_true() -> bool { true }
fn default_answer_mode() -> String { "meaning_recall".into() }
fn default_new_cards() -> u32 { 20 }
fn default_llm_endpoint() -> String { "https://api.openai.com/v1".into() }
fn default_model() -> String { "gpt-4o".into() }
fn default_max_tokens() -> usize { 2048 }

impl Default for ReaderConfig {
    fn default() -> Self {
        Self { sidebar_width: default_sidebar_width(), furigana: true }
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
    /// Resolved DB path for convenience
    pub fn db_path(&self) -> &Path {
        &self.general.db_path
    }

    pub fn load(path: Option<&Path>) -> Result<Self> {
        // Try explicit path, then XDG config, then default
        if let Some(p) = path {
            let content = std::fs::read_to_string(p)
                .with_context(|| format!("Failed to read config file: {}", p.display()))?;
            return toml::from_str(&content).context("Failed to parse config file");
        }

        // Try XDG config path
        if let Some(config_dir) = dirs::config_dir() {
            let cfg_path = config_dir.join("kotoba").join("kotoba.toml");
            if cfg_path.exists() {
                let content = std::fs::read_to_string(&cfg_path)?;
                return toml::from_str(&content).context("Failed to parse config file");
            }
        }

        // Try local kotoba.toml
        let local = Path::new("kotoba.toml");
        if local.exists() {
            let content = std::fs::read_to_string(local)?;
            return toml::from_str(&content).context("Failed to parse config file");
        }

        // Return defaults
        Ok(Self::default())
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kotoba");

        Self {
            general: GeneralConfig {
                db_path: data_dir.join("kotoba.db"),
                jmdict_path: data_dir.join("JMdict_e.xml"),
                theme: default_theme(),
            },
            reader: ReaderConfig::default(),
            srs: SrsConfig::default(),
            llm: LlmConfig::default(),
        }
    }
}
