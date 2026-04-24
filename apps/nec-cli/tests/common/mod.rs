pub fn diag_mode(stderr: &str) -> Option<&str> {
    for line in stderr.lines() {
        if !line.starts_with("diag: ") {
            continue;
        }
        for field in line.split_whitespace() {
            if let Some(mode) = field.strip_prefix("mode=") {
                return Some(mode);
            }
        }
    }
    None
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
