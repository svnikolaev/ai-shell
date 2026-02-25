use clap::Parser;
use anyhow::Context;

mod config;
mod llm;
mod cache;
mod shell;

#[derive(Parser)]
#[command(author, version, about = "AI shell assistant: генерирует bash-команды по запросу")]
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
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = config::Config::load()?;

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

    // Проверка директории кэша
    println!("📁 Кэш-директория: {}", config.cache_dir.display());
    if config.cache_dir.exists() {
        println!("   ✅ доступна");
    } else {
        println!("   ⚠️  не существует (будет создана при первом запросе)");
    }

    // Проверка стоп-листа
    println!("\n🛑 Стоп-лист: {} записей", config.stop_list.len());

    // Проверка бэкендов
    println!("\n🌐 Проверка бэкендов:");
    for (i, backend) in config.backends.iter().enumerate() {
        print!("   {}. {} ... ", i+1, backend.api_url);
        match test_backend(backend) {
            Ok((cmd, exp)) => {
                println!("✅ работает");
                println!("      Пример ответа: команда = \"{}\", объяснение = \"{}\"", cmd, exp);
            }
            Err(e) => {
                println!("❌ ошибка: {}", e);
            }
        }
    }

    Ok(())
}

fn test_backend(backend: &config::Backend) -> anyhow::Result<(String, String)> {
    // Используем простой запрос, который точно должен сработать
    let test_question = "скажи 'test' одной командой echo";
    // Можно переиспользовать llm::try_backend, но она требует explain_language.
    // Для теста создадим временный конфиг с explain_language = "ru"
    let dummy_config = config::Config {
        backends: vec![backend.clone()],
        explain_language: "ru".to_string(),
        cache_dir: Default::default(),
        stop_list: vec![],
    };
    llm::ask(test_question, &dummy_config)
}
