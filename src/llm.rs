use crate::config::{Backend, Config};
use anyhow::{anyhow, Context};
use std::time::Duration;
use ureq::{Agent, AgentBuilder};

/// Отправить запрос к конкретному бэкенду и вернуть (команда, объяснение)
pub fn try_backend(question: &str, backend: &Backend, explain_lang: &str) -> anyhow::Result<(String, String)> {
    let agent: Agent = AgentBuilder::new()
        .timeout(Duration::from_secs(backend.timeout_secs))
        .build();

    let system_prompt = format!(
        "Ты — терминальный ассистент. Пользователь описывает задачу на русском языке.\n\
         Ответь ТОЛЬКО валидным JSON объектом с двумя полями:\n\
         - \"command\": строка с bash-командой (одна строка, несколько команд через && или |)\n\
         - \"explanation\": краткое объяснение команды на русском языке (язык объяснения: {})\n\
         Не используй markdown, обратные кавычки или пояснения вне JSON.\n\
         ОС: Linux.",
        explain_lang
    );

    let body = serde_json::json!({
        "model": backend.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": question}
        ],
        "temperature": 0,
        "max_tokens": 300,
        "response_format": {"type": "json_object"}
    });

    let mut request = agent.post(&backend.api_url).set("Content-Type", "application/json");
    if let Some(key) = &backend.api_key {
        request = request.set("Authorization", &format!("Bearer {}", key));
    }

    // Отправляем запрос и обрабатываем ошибки соединения
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

/// Пытается последовательно опросить все бэкенды, пока один не вернёт ответ.
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