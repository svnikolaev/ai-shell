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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_with_stop_list() {
        let toml_str = r#"
            explain_language = "ru"
            stop_list = ["rm -rf /", "mkfs", "dd if=/dev/zero"]

            [[backends]]
            api_url = "https://openrouter.ai/api/v1/chat/completions"
            api_key = "test-key"
            model = "test-model"
            timeout_secs = 30
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.explain_language, "ru");
        assert_eq!(config.stop_list.len(), 3);
        assert!(config.stop_list.contains(&"rm -rf /".to_string()));
        assert!(config.stop_list.contains(&"mkfs".to_string()));
        assert!(config.stop_list.contains(&"dd if=/dev/zero".to_string()));
        assert_eq!(config.backends.len(), 1);
        assert_eq!(
            config.backends[0].api_url,
            "https://openrouter.ai/api/v1/chat/completions"
        );
        assert_eq!(config.backends[0].api_key, Some("test-key".to_string()));
        assert_eq!(config.backends[0].model, "test-model");
        assert_eq!(config.backends[0].timeout_secs, 30);
    }

    #[test]
    fn test_parse_config_empty_stop_list() {
        let toml_str = r#"
            explain_language = "en"
            stop_list = []

            [[backends]]
            api_url = "https://example.com"
            api_key = "key"
            model = "model"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.stop_list.is_empty());
    }

    #[test]
    fn test_parse_config_missing_stop_list() {
        let toml_str = r#"
            explain_language = "en"

            [[backends]]
            api_url = "https://example.com"
            api_key = "key"
            model = "model"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        // Должно быть значение по умолчанию — пустой вектор
        assert!(config.stop_list.is_empty());
    }

    #[test]
    fn test_parse_config_missing_backends() {
        let toml_str = r#"
        explain_language = "ru"
        stop_list = ["rm -rf"]
        "#;
        let result: Result<Config, toml::de::Error> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("missing field `backends`"));
    }

    #[test]
    fn test_parse_config_with_multiple_backends() {
        let toml_str = r#"
            explain_language = "ru"

            [[backends]]
            api_url = "https://openrouter.ai/api/v1/chat/completions"
            api_key = "key1"
            model = "model1"

            [[backends]]
            api_url = "http://localhost:11434/v1/chat/completions"
            api_key = ""
            model = "model2"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.backends.len(), 2);
        assert_eq!(
            config.backends[0].api_url,
            "https://openrouter.ai/api/v1/chat/completions"
        );
        assert_eq!(
            config.backends[1].api_url,
            "http://localhost:11434/v1/chat/completions"
        );
    }
}
