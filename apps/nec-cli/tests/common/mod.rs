pub fn diag_field<'a>(stderr: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}=");
    for line in stderr.lines() {
        if !line.starts_with("diag: ") {
            continue;
        }
        for field in line.split_whitespace() {
            if let Some(value) = field.strip_prefix(&prefix) {
                return Some(value);
            }
        }
    }
    None
}

pub fn diag_mode(stderr: &str) -> Option<&str> {
    diag_field(stderr, "mode")
}

pub fn assert_diag_mode(stderr: &str, expected_diag_mode: &str) {
    let actual = diag_mode(stderr);
    assert_eq!(
        actual,
        Some(expected_diag_mode),
        "expected diag mode '{expected_diag_mode}', got {:?} in stderr:\n{stderr}",
        actual
    );
}

pub fn assert_diag_field(stderr: &str, key: &str, expected_value: &str) {
    let actual = diag_field(stderr, key);
    assert_eq!(
        actual,
        Some(expected_value),
        "expected diag field '{key}={expected_value}', got {:?} in stderr:\n{stderr}",
        actual
    );
}
