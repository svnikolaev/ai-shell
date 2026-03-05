use crate::cache;
use crate::config::Config;
use crate::llm;
use crate::shell;
use anyhow::Context;
use chrono::{DateTime, Local, TimeZone};
use clap::CommandFactory;
use std::path::PathBuf;

use super::Args; // Импортируем Args из main (или можно вынести в отдельный модуль, но пока оставим так)

/// Обработка режима истории
pub fn handle_history(args: &Args, config: &Config) -> anyhow::Result<()> {
    let n = args.history.unwrap_or(10);
    let entries = cache::get_history(n, config)?;
    if entries.is_empty() {
        println!("История пуста.");
    } else {
        for (i, entry) in entries.iter().enumerate() {
            let datetime: DateTime<Local> =
                Local.timestamp_opt(entry.timestamp as i64, 0).unwrap();
            println!(
                "{}. [{}] {}",
                i + 1,
                datetime.format("%Y-%m-%d %H:%M:%S"),
                entry.question
            );
            println!("   → {}", entry.command);
            if args.explain {
                println!("   Объяснение: {}", entry.explanation);
            }
            println!();
        }
    }
    Ok(())
}

/// Обработка режима тестирования
pub fn handle_test(config: &Config) -> anyhow::Result<()> {
    println!("🔍 Тестирование конфигурации ai-shell\n");

    // ---- Проверка конфигурационного файла ----
    let config_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("ai-shell")
        .join("config.toml");
    println!("📄 Конфиг: {}", config_path.display());

    if config_path.exists() {
        println!("   ✅ файл найден");
        let metadata = std::fs::metadata(&config_path)?;
        println!("   📏 размер: {} байт", metadata.len());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            println!("   🔐 права: {:o}", metadata.permissions().mode() & 0o777);
        }

        let content = std::fs::read(&config_path)?;
        if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
            println!("   ⚠️  файл содержит BOM — это может помешать парсингу");
            println!("      Рекомендуется сохранить файл без BOM");
        }
        if content.iter().any(|&b| b == b'\r') {
            println!("   ⚠️  файл содержит символы возврата каретки (CR) — возможно, DOS-формат");
            println!("      Выполните: dos2unix {}", config_path.display());
        }
    } else {
        println!("   ❌ файл не найден (используются значения по умолчанию?)");
    }

    // ---- Проверка директории кэша ----
    println!("\n📁 Кэш-директория: {}", config.cache_dir.display());
    if config.cache_dir.exists() {
        println!("   ✅ доступна");
    } else {
        println!("   ⚠️  не существует (будет создана при первом запросе)");
    }

    // ---- Информация об ОС ----
    println!("\n🖥️  Целевая ОС: {}", config.target_os());

    // ---- Проверка стоп-листа ----
    println!("\n🛑 Стоп-лист: {} записей", config.stop_list.len());
    if config.stop_list.is_empty() {
        println!("   ⚠️  стоп-лист пуст — это снижает безопасность!");
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if content.contains("stop_list") {
                println!(
                    "   ⚠️  В файле присутствует поле stop_list, но после парсинга оно пусто."
                );
                println!(
                    "       Возможно, синтаксическая ошибка в массиве (невидимые символы, неверные кавычки)."
                );
                println!(
                    "       Проверьте строку с stop_list вручную, например, с помощью hexdump."
                );
            }
        }
    } else {
        for pattern in &config.stop_list {
            println!("   - {}", pattern);
        }
    }

    // ---- Проверка бэкендов ----
    println!("\n🌐 Проверка бэкендов:");
    if config.backends.is_empty() {
        println!("   ❌ нет ни одного бэкенда в конфиге");
        return Ok(());
    }

    for (i, backend) in config.backends.iter().enumerate() {
        println!("   {}. {}", i + 1, backend.api_url);
        println!("      Модель: {}", backend.model);
        if backend.api_key.is_some() {
            println!("      Ключ: ✅ задан");
        } else {
            println!(
                "      Ключ: ❌ не задан (возможно, используется переменная окружения AI_API_KEY)"
            );
        }
        print!("      Тестовый запрос ... ");
        match llm::try_backend("скажи 'test' одной командой echo", backend, config) {
            Ok((cmd, exp)) => {
                println!("✅ успех");
                println!("         команда = \"{}\"", cmd);
                println!("         объяснение = \"{}\"", exp);
            }
            Err(e) => {
                println!("❌ ошибка: {}", e);
            }
        }
    }

    Ok(())
}

/// Обработка режима --last
pub fn handle_last(config: &Config) -> anyhow::Result<()> {
    let last_cmd = cache::read_last(config)
        .context("Нет сохранённой последней команды (файл last_command отсутствует)")?;
    if shell::is_dangerous(&last_cmd, &config.stop_list) {
        eprintln!("❌ Команда заблокирована стоп-листом. Вывод отменён.");
        std::process::exit(1);
    }
    println!("{}", last_cmd);
    Ok(())
}

/// Обработка основного ввода (вопрос или команда)
pub fn handle_input(input: &str, args: &Args, config: &Config) -> anyhow::Result<()> {
    if args.explain {
        // Режим объяснения: input — это команда
        let explanation = llm::explain_command(input, config)?;
        println!("{}", input);
        println!("\nОбъяснение: {}", explanation);
    } else {
        // Режим генерации: input — это вопрос
        let (command, explanation) = if !args.no_cache {
            if let Some(cached) = cache::get(input, config)? {
                (cached.command, cached.explanation)
            } else {
                let (cmd, exp) = llm::ask(input, config)?;
                cache::put(input, &cmd, &exp, config)?;
                (cmd, exp)
            }
        } else {
            let (cmd, exp) = llm::ask(input, config)?;
            cache::put(input, &cmd, &exp, config)?;
            (cmd, exp)
        };

        // Проверка стоп-листа
        if shell::is_dangerous(&command, &config.stop_list) {
            eprintln!("❌ Команда заблокирована стоп-листом. Она не будет сохранена как последняя.");
            std::process::exit(1);
        }

        // Сохраняем в last_command
        cache::save_last(&command, config)?;
        // Добавляем в историю
        if let Err(e) = cache::add_to_history(input, &command, &explanation, config) {
            eprintln!("Не удалось сохранить историю: {}", e);
        }

        // Вывод результата
        if args.explain {
            println!("{}", command);
            println!("\nОбъяснение: {}", explanation);
        } else {
            println!("{}", command);
        }
    }
    Ok(())
}

/// Обработка случая, когда нет ни аргументов, ни stdin
pub fn handle_no_input(config: &Config) -> anyhow::Result<()> {
    match cache::read_last(config) {
        Ok(cmd) => {
            if shell::is_dangerous(&cmd, &config.stop_list) {
                eprintln!("❌ Последняя команда заблокирована стоп-листом. Вывод отменён.");
                std::process::exit(1);
            }
            println!("{}", cmd);
        }
        Err(_) => {
            Args::command().print_help()?;
            println!();
        }
    }
    Ok(())
}
