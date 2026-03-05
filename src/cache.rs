use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
pub struct CacheEntry {
    pub command: String,
    pub explanation: String,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HistoryEntry {
    pub question: String,
    pub command: String,
    pub explanation: String,
    pub timestamp: u64,
}

const HISTORY_MAX: usize = 1000; // максимальное количество записей в истории

/// Добавить запись в историю
pub fn add_to_history(
    question: &str,
    command: &str,
    explanation: &str,
    config: &crate::config::Config,
) -> anyhow::Result<()> {
    let history_file = config.cache_dir.join("history.json");
    let mut history: VecDeque<HistoryEntry> = if history_file.exists() {
        let content = std::fs::read_to_string(&history_file)?;
        serde_json::from_str(&content)?
    } else {
        VecDeque::new()
    };

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let entry = HistoryEntry {
        question: question.to_string(),
        command: command.to_string(),
        explanation: explanation.to_string(),
        timestamp,
    };

    history.push_front(entry); // новые записи добавляются в начало
    if history.len() > HISTORY_MAX {
        history.pop_back();
    }

    if let Some(parent) = history_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(&history)?;
    std::fs::write(history_file, content)?;
    Ok(())
}

/// Получить последние N записей истории
pub fn get_history(n: usize, config: &crate::config::Config) -> anyhow::Result<Vec<HistoryEntry>> {
    let history_file = config.cache_dir.join("history.json");
    if !history_file.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(history_file)?;
    let history: VecDeque<HistoryEntry> = serde_json::from_str(&content)?;
    Ok(history.into_iter().take(n).collect())
}

pub fn get(question: &str, config: &crate::config::Config) -> anyhow::Result<Option<CacheEntry>> {
    let cache_file = config.cache_dir.join("cache.json");
    if !cache_file.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(cache_file)?;
    let mut cache: HashMap<String, CacheEntry> = serde_json::from_str(&content)?;
    let key = blake3::hash(question.as_bytes()).to_hex().to_string();
    Ok(cache.remove(&key))
}

pub fn put(
    question: &str,
    command: &str,
    explanation: &str,
    config: &crate::config::Config,
) -> anyhow::Result<()> {
    let cache_file = config.cache_dir.join("cache.json");
    let mut cache: HashMap<String, CacheEntry> = if cache_file.exists() {
        let content = std::fs::read_to_string(&cache_file)?;
        serde_json::from_str(&content)?
    } else {
        HashMap::new()
    };
    let key = blake3::hash(question.as_bytes()).to_hex().to_string();
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    cache.insert(
        key,
        CacheEntry {
            command: command.to_string(),
            explanation: explanation.to_string(),
            timestamp,
        },
    );
    if let Some(parent) = cache_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(&cache)?;
    std::fs::write(cache_file, content)?;
    Ok(())
}

pub fn save_last(command: &str, config: &crate::config::Config) -> anyhow::Result<()> {
    let last_file = config.cache_dir.join("last_command");
    if let Some(parent) = last_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(last_file, command)?;
    Ok(())
}

pub fn read_last(config: &crate::config::Config) -> anyhow::Result<String> {
    let last_file = config.cache_dir.join("last_command");
    let cmd = std::fs::read_to_string(last_file)?;
    Ok(cmd.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir; // потребуется добавить tempfile в dev-dependencies

    #[test]
    fn test_cache_put_get() {
        let dir = tempdir().unwrap();
        let config = crate::config::Config {
            cache_dir: dir.path().to_path_buf(),
            backends: vec![],
            explain_language: "ru".to_string(),
            stop_list: vec![],
            os: None,
        };
        let question = "как распаковать tar.gz";
        let cmd = "tar -xzf archive.tar.gz";
        let exp = "распаковывает архив";

        put(question, cmd, exp, &config).unwrap();
        let entry = get(question, &config).unwrap().unwrap();
        assert_eq!(entry.command, cmd);
        assert_eq!(entry.explanation, exp);
    }
}
