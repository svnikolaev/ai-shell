use anyhow::Context;
use atty::Stream;
use chrono::{DateTime, Local, TimeZone};
use clap::CommandFactory;
use clap::Parser;
use std::io::Read;
use std::path::PathBuf;

mod cache;
mod config;
mod llm;
mod shell;
#[derive(Parser)]
#[command(
    author,
    version,
    about = "AI shell assistant: генерирует bash-команды по запросу"
)]
struct Args {
    /// Вопрос на естественном языке
    question: Vec<String>,

    /// Показать объяснение команды
    #[arg(short, long)]
    explain: bool,

    /// Показать последнюю сгенерированную команду (из кэша)
    #[arg(short = 'l', long)]
    last: bool,

    /// Игнорировать кэш
    #[arg(short, long)]
    no_cache: bool,

    /// Проверить конфигурацию и доступность бэкендов
    #[arg(long)]
    test: bool,

    /// Показать последние запросы (можно указать число, по умолчанию 10)
    #[arg(short = 'i', long, num_args = 0..=1, default_missing_value = "10")]
    history: Option<usize>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = config::Config::load()?;

    // Режим истории
    if let Some(n) = args.history {
        let entries = cache::get_history(n, &config)?;
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
        return Ok(());
    }

    // Режим объяснения команды из stdin (если нет аргументов и есть данные в пайпе)
    if !args.test && !args.last && !args.explain && !args.no_cache && args.question.is_empty() {
        if !atty::is(Stream::Stdin) {
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            let command = buffer.trim();
            if command.is_empty() {
                eprintln!("Пустой ввод в stdin.");
                std::process::exit(1);
            }
            // Получаем объяснение
            let explanation = llm::explain_command(command, &config)?;
            // Выводим команду и объяснение (как при -e)
            println!("{}", command);
            println!("\nОбъяснение: {}", explanation);
            return Ok(());
        }

        // Если нет stdin, показываем последнюю команду или help
        match cache::read_last(&config) {
            Ok(cmd) => {
                if shell::is_dangerous(&cmd, &config.stop_list) {
                    eprintln!("❌ Последняя команда заблокирована стоп-листом. Вывод отменён.");
                    std::process::exit(1);
                }
                println!("{}", cmd);
                return Ok(());
            }
            Err(_) => {
                Args::command().print_help()?;
                println!();
                return Ok(());
            }
        }
    }

    // Режим тестирования
    if args.test {
        return run_test(&config);
    }

    // Режим --last
    if args.last {
        let last_cmd = cache::read_last(&config)
            .context("Нет сохранённой последней команды (файл last_command отсутствует)")?;
        if shell::is_dangerous(&last_cmd, &config.stop_list) {
            eprintln!("❌ Команда заблокирована стоп-листом. Вывод отменён.");
            std::process::exit(1);
        }
        println!("{}", last_cmd);
        return Ok(());
    }

    // Проверка наличия вопроса
    if args.question.is_empty() {
        anyhow::bail!("Вопрос не указан. Используйте -l для вывода последней команды.");
    }
    let question = args.question.join(" ");

    // Получаем команду и объяснение (из кэша или через LLM)
    let (command, explanation) = if !args.no_cache {
        if let Some(cached) = cache::get(&question, &config)? {
            (cached.command, cached.explanation)
        } else {
            let (cmd, exp) = llm::ask(&question, &config)?;
            cache::put(&question, &cmd, &exp, &config)?;
            (cmd, exp)
        }
    } else {
        let (cmd, exp) = llm::ask(&question, &config)?;
        cache::put(&question, &cmd, &exp, &config)?;
        (cmd, exp)
    };

    // Проверка стоп-листа
    if shell::is_dangerous(&command, &config.stop_list) {
        eprintln!("❌ Команда заблокирована стоп-листом. Она не будет сохранена как последняя.");
        std::process::exit(1);
    }

    // Сохраняем в last_command
    cache::save_last(&command, &config)?;
    // Добавляем в историю
    if let Err(e) = cache::add_to_history(&question, &command, &explanation, &config) {
        eprintln!("Не удалось сохранить историю: {}", e);
    }

    // Вывод результата
    if args.explain {
        println!("{}", command);
        println!("\nОбъяснение: {}", explanation);
    } else {
        println!("{}", command);
    }

    Ok(())
}

fn run_test(config: &config::Config) -> anyhow::Result<()> {
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
        match llm::try_backend(
            "скажи 'test' одной командой echo",
            backend,
            &config.explain_language,
        ) {
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
