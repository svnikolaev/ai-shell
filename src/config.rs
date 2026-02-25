use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Backend {
    pub api_url: String,
    pub api_key: Option<String>,
    pub model: String,
    #[serde(rename = "type", default = "default_backend_type")]
    pub backend_type: String, // пока только "openai"
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_backend_type() -> String {
    "openai".to_string()
}

fn default_timeout() -> u64 {
    30
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub backends: Vec<Backend>,
    #[serde(default = "default_explain_language")]
    pub explain_language: String,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
    #[serde(default)]
    pub stop_list: Vec<String>,
}

fn default_explain_language() -> String {
    "ru".to_string()
}

fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("ai-shell")
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("ai-shell")
            .join("config.toml");

        if !config_path.exists() {
            anyhow::bail!(
                "Конфиг не найден: {}\nСоздайте файл с настройками.",
                config_path.display()
            );
        }

        let content = std::fs::read_to_string(config_path)?;
        let mut config: Config = toml::from_str(&content)?;

        // Переопределение api_key из переменной окружения для первого бэкенда (если не задан)
        if let Ok(env_key) = std::env::var("AI_API_KEY") {
            if !config.backends.is_empty() && config.backends[0].api_key.is_none() {
                config.backends[0].api_key = Some(env_key);
            }
        }

        Ok(config)
    }
}
