use crate::config::{Backend, Config};
use anyhow::{anyhow, Context};
use std::time::Duration;
use ureq::{Agent, AgentBuilder};

// Общая функция для вызова одного бэкенда с заданным системным промптом
fn call_backend(
    user_content: &str,
    system_prompt: &str,
    backend: &Backend,
) -> anyhow::Result<(String, String)> {
    let agent: Agent = AgentBuilder::new()
        .timeout(Duration::from_secs(backend.timeout_secs))
        .build();

    let body = serde_json::json!({
        "model": backend.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_content}
        ],
        "temperature": 0,
        "max_tokens": 300,
        "response_format": {"type": "json_object"}
    });

    let mut request = agent.post(&backend.api_url).set("Content-Type", "application/json");
    if let Some(key) = &backend.api_key {
        request = request.set("Authorization", &format!("Bearer {}", key));
    }

    let response = match request.send_json(body) {
        Ok(resp) => resp,
        Err(ureq::Error::Transport(err)) => {
            return Err(anyhow!("Ошибка соединения: {}", err.to_string()));
        }
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            return Err(anyhow!("HTTP {}: {}", code, text));
        }
    };

    let status = response.status();
    if status != 200 {
        let text = response.into_string().unwrap_or_default();
        anyhow::bail!("HTTP {}: {}", status, text);
    }

    let json: serde_json::Value = response.into_json()?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow!("Не удалось извлечь content из ответа"))?;

    let parsed: serde_json::Value = serde_json::from_str(content)
        .with_context(|| "Ответ LLM не является валидным JSON")?;

    let command = parsed["command"]
        .as_str()
        .ok_or_else(|| anyhow!("Поле command не найдено или не строка"))?
        .to_string();
    let explanation = parsed["explanation"]
        .as_str()
        .ok_or_else(|| anyhow!("Поле explanation не найдено или не строка"))?
        .to_string();

    Ok((command, explanation))
}

/// Запрос к бэкенду для генерации команды по описанию задачи
pub fn try_backend(question: &str, backend: &Backend, explain_lang: &str) -> anyhow::Result<(String, String)> {
    let system_prompt = format!(
        "Ты — терминальный ассистент. Пользователь описывает задачу на русском языке.\n\
         Ответь ТОЛЬКО валидным JSON объектом с двумя полями:\n\
         - \"command\": строка с bash-командой (одна строка, несколько команд через && или |)\n\
         - \"explanation\": краткое объяснение команды на русском языке (язык объяснения: {})\n\
         Не используй markdown, обратные кавычки или пояснения вне JSON.\n\
         ОС: Linux.",
        explain_lang
    );
    call_backend(question, &system_prompt, backend)
}

/// Получить объяснение для готовой команды (переданной пользователем)
pub fn explain_command(command: &str, config: &Config) -> anyhow::Result<String> {
    let system_prompt = format!(
        "Ты — терминальный ассистент. Пользователь передаёт bash-команду.\n\
         Ответь ТОЛЬКО валидным JSON объектом с двумя полями:\n\
         - \"command\": строка с исходной командой (без изменений)\n\
         - \"explanation\": краткое объяснение команды на русском языке (язык объяснения: {})\n\
         Не используй markdown, обратные кавычки или пояснения вне JSON.\n\
         ОС: Linux.",
        config.explain_language
    );

    let mut last_error = None;
    for (idx, backend) in config.backends.iter().enumerate() {
        match call_backend(command, &system_prompt, backend) {
            Ok((cmd, exp)) => {
                // Проверяем, не изменила ли модель команду
                if cmd.trim() != command.trim() {
                    eprintln!("⚠️ Модель изменила команду. Используем оригинал.");
                }
                return Ok(exp);
            }
            Err(e) => {
                eprintln!("⚠️ Бэкенд {} ({}) не сработал: {}", idx + 1, backend.api_url, e);
                last_error = Some(e);
            }
        }
    }
    Err(anyhow!("Все бэкенды недоступны. Последняя ошибка: {:?}", last_error))
}

/// Последовательный опрос бэкендов для получения команды по вопросу (используется в main)
pub fn ask(question: &str, config: &Config) -> anyhow::Result<(String, String)> {
    let mut last_error = None;
    for (idx, backend) in config.backends.iter().enumerate() {
        match try_backend(question, backend, &config.explain_language) {
            Ok(res) => return Ok(res),
            Err(e) => {
                eprintln!("⚠️ Бэкенд {} ({}) не сработал: {}", idx + 1, backend.api_url, e);
                last_error = Some(e);
            }
        }
    }
    Err(anyhow!("Все бэкенды недоступны или вернули ошибку. Последняя ошибка: {:?}", last_error))
}
