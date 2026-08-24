#![allow(dead_code)]

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

pub fn assert_diag_field_is_finite_nonnegative(stderr: &str, key: &str) {
    let raw = diag_field(stderr, key)
        .unwrap_or_else(|| panic!("missing diag field '{key}' in stderr:\n{stderr}"));
    let value = raw
        .parse::<f64>()
        .unwrap_or_else(|e| panic!("failed to parse diag field '{key}={raw}' as f64: {e}"));
    assert!(
        value.is_finite(),
        "expected diag field '{key}' to be finite, got {value} from stderr:\n{stderr}"
    );
    assert!(
        value >= 0.0,
        "expected diag field '{key}' to be non-negative, got {value} from stderr:\n{stderr}"
    );
}

/// A deck written to the system temp directory that deletes itself when the test
/// ends — **including when the test panics**, which a trailing
/// `fs::remove_file(&path)` does not.
///
/// Most CLI test files already clean up after themselves; six did not, and one
/// session's repeated `cargo test --workspace` runs left 437 stray decks in
/// `/tmp`. That is not only untidy: enough of them broke the sandbox this agent
/// runs its shell in.
///
/// Derefs to `Path`, so a call site that had a `PathBuf` needs no change beyond
/// binding the guard to a variable that outlives its use.
pub struct TempDeck {
    path: std::path::PathBuf,
}

impl TempDeck {
    /// Write `body` to `<temp dir>/<file_name>`.
    ///
    /// `file_name` must already be unique per test — the guard does not
    /// disambiguate, because tests within one binary run in parallel and two
    /// sharing a name would delete each other's file mid-run.
    pub fn new(file_name: &str, body: &str) -> Self {
        let path = std::env::temp_dir().join(file_name);
        std::fs::write(&path, body)
            .unwrap_or_else(|e| panic!("failed to write temp deck {}: {e}", path.display()));
        Self { path }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl std::ops::Deref for TempDeck {
    type Target = std::path::Path;
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl AsRef<std::path::Path> for TempDeck {
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

// `Command::arg` wants `AsRef<OsStr>`, not `AsRef<Path>`, so call sites that
// passed a `PathBuf` keep working without change.
impl AsRef<std::ffi::OsStr> for TempDeck {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.path.as_os_str()
    }
}

impl Drop for TempDeck {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
