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

/// Читает содержимое stdin, обрезает пробелы и возвращает как String.
/// Если после обрезки строка пуста, возвращает ошибку.
fn read_stdin<R: Read>(reader: &mut R) -> anyhow::Result<String> {
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer)?;
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Пустой ввод в stdin.");
    }
    Ok(trimmed.to_string())
}

/// Чистая функция получения входного текста.
/// Принимает:
/// - аргументы вопроса (могут быть пустыми)
/// - флаг, является ли stdin терминалом (интерактивным)
/// - читатель для stdin (обычно `std::io::stdin()`, но для тестов можно подменить)
/// Возвращает `Ok(Some(text))` если ввод есть, `Ok(None)` если нет ввода,
/// или ошибку, если чтение stdin не удалось или он пуст.
fn get_input<R: Read>(
    question_args: &[String],
    is_stdin_terminal: bool,
    mut stdin_reader: R,
) -> anyhow::Result<Option<String>> {
    // Приоритет у аргументов командной строки
    if !question_args.is_empty() {
        // Если stdin не является терминалом (т.е. есть перенаправленные данные), предупреждаем
        if !is_stdin_terminal {
            eprintln!("⚠️ Вопрос передан как аргумент, данные из stdin игнорируются.");
        }
        return Ok(Some(question_args.join(" ")));
    }

    // Если аргументов нет, пробуем читать из stdin (когда он НЕ терминал)
    if !is_stdin_terminal {
        let content = read_stdin(&mut stdin_reader)?;
        return Ok(Some(content));
    }

    // Нет ни аргументов, ни данных в stdin
    Ok(None)
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

    // Получаем входные данные через чистую функцию
    let is_stdin_terminal = atty::is(Stream::Stdin);
    let input = get_input(&args.question, is_stdin_terminal, std::io::stdin())?;

    match input {
        Some(content) => handlers::handle_input(&content, &args, &config),
        None => handlers::handle_no_input(&config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    // Вспомогательная функция для создания Vec<String> из &str
    fn args(v: Vec<&str>) -> Vec<String> {
        v.into_iter().map(String::from).collect()
    }

    // --- Тесты для read_stdin ---

    #[test]
    fn read_stdin_ok() {
        let mut reader = Cursor::new("  hello world  ");
        assert_eq!(read_stdin(&mut reader).unwrap(), "hello world");
    }

    #[test]
    fn read_stdin_empty() {
        let mut reader = Cursor::new("   \n  ");
        let err = read_stdin(&mut reader).unwrap_err();
        assert_eq!(err.to_string(), "Пустой ввод в stdin.");
    }

    #[test]
    fn read_stdin_propagates_io_errors() {
        struct ErrorReader;
        impl Read for ErrorReader {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(std::io::ErrorKind::Other, "fake error"))
            }
        }
        let mut reader = ErrorReader;
        let err = read_stdin(&mut reader).unwrap_err();
        assert!(err.downcast_ref::<std::io::Error>().is_some());
    }

    #[test]
    fn read_stdin_trims_all_whitespace() {
        let cases = vec![
            ("hello", "hello"),
            ("  hello  ", "hello"),
            ("\nhello\n", "hello"),
            ("\t hello \t", "hello"),
            ("hello world", "hello world"),
            ("  leading and trailing  ", "leading and trailing"),
        ];
        for (input, expected) in cases {
            let mut reader = Cursor::new(input.as_bytes());
            assert_eq!(read_stdin(&mut reader).unwrap(), expected);
        }
    }

    // --- Тесты для get_input ---

    #[test]
    fn get_input_with_arguments() {
        let result = get_input(&args(vec!["hello", "world"]), false, Cursor::new("")).unwrap();
        assert_eq!(result, Some("hello world".to_string()));
    }

    #[test]
    fn get_input_with_arguments_and_stdin_data() {
        // Предупреждение выводится, но мы его не проверяем здесь
        let result = get_input(&args(vec!["test"]), false, Cursor::new("stdin data")).unwrap();
        assert_eq!(result, Some("test".to_string()));
    }

    #[test]
    fn get_input_from_stdin() {
        let result = get_input(&args(vec![]), false, Cursor::new("  stdin content  ")).unwrap();
        assert_eq!(result, Some("stdin content".to_string()));
    }

    #[test]
    fn get_input_stdin_empty_error() {
        let result = get_input(&args(vec![]), false, Cursor::new("   \n  "));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Пустой ввод в stdin.");
    }

    #[test]
    fn get_input_no_input() {
        let result = get_input(&args(vec![]), true, Cursor::new("")).unwrap();
        assert_eq!(result, None);
    }

    // Тест, который упадёт, если произойдёт чтение из stdin при наличии аргументов
    #[test]
    fn get_input_does_not_read_stdin_when_args_present() {
        struct PanicReader;
        impl Read for PanicReader {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("Не должно быть чтения из stdin при наличии аргументов");
            }
        }
        let result = get_input(&args(vec!["hello"]), false, PanicReader);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("hello".to_string()));
    }

    // Тест, который упадёт, если произойдёт чтение при is_stdin_terminal=true
    #[test]
    fn get_input_does_not_read_stdin_when_terminal() {
        struct PanicReader;
        impl Read for PanicReader {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("Не должно быть чтения, если stdin терминал");
            }
        }
        let result = get_input(&args(vec![]), true, PanicReader);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
