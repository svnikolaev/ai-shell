pub fn is_dangerous(command: &str, stop_list: &[String]) -> bool {
    let cmd_lower = command.to_lowercase();
    for pattern in stop_list {
        if cmd_lower.contains(&pattern.to_lowercase()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_dangerous() {
        let stop_list = vec![
            "rm -rf /".to_string(),
            "mkfs".to_string(),
        ];
        assert!(is_dangerous("rm -rf /", &stop_list));
        assert!(is_dangerous("sudo rm -rf /", &stop_list));
        assert!(!is_dangerous("ls -la", &stop_list));
        assert!(is_dangerous("/sbin/mkfs.ext4 /dev/sda1", &stop_list));
        assert!(is_dangerous("mkfs", &stop_list));
    }
}
