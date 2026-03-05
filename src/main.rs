use anyhow::Context;
use atty::Stream;
use clap::Parser;
use std::io::Read;

mod cache;
mod config;
mod handlers;
mod llm;
mod shell;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "AI shell assistant: генерирует команды для оболочки по запросу"
)]
struct Args {
    /// Вопрос на естественном языке (если не указан, читается из stdin)
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
    if args.history.is_some() {
        return handlers::handle_history(&args, &config);
    }

    // Режим тестирования
    if args.test {
        return handlers::handle_test(&config);
    }

    // Режим --last (игнорирует stdin и аргументы вопроса)
    if args.last {
        return handlers::handle_last(&config);
    }

    // Определяем источник ввода: приоритет у аргументов
    let input = if !args.question.is_empty() {
        // Если есть аргументы, используем их
        if !atty::is(Stream::Stdin) {
            // Предупреждаем, что stdin игнорируется
            eprintln!("⚠️ Вопрос передан как аргумент, данные из stdin игнорируются.");
        }
        Some(args.question.join(" "))
    } else if !atty::is(Stream::Stdin) {
        // Если нет аргументов, но есть stdin
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            eprintln!("Пустой ввод в stdin.");
            std::process::exit(1);
        }
        Some(trimmed.to_string())
    } else {
        None
    };

    match input {
        Some(content) => handlers::handle_input(&content, &args, &config),
        None => handlers::handle_no_input(&config),
    }
}
