use std::process::Command;

#[test]
fn test_help() {
    let output = Command::new("target/debug/ai-shell")
        .arg("--help")
        .output()
        .expect("failed to execute process");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage"));
}

#[test]
fn test_test_flag() {
    let output = Command::new("target/debug/ai-shell")
        .arg("--test")
        .output()
        .expect("failed to execute process");
    // Должен завершиться успешно (код 0), даже если бэкенды недоступны
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Тестирование конфигурации"));
}
