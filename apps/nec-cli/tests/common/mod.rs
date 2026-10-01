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

/// Assert the `exec` label of an `--exec gpu` run of a deck the device solves:
/// `gpu` where a hardware adapter is present, `gpu(cpu-fallback)` where none is.
/// The label used to read `gpu(cpu-fallback)` on every point, including the ones
/// the device solved — so tests that asserted it were false on a GPU host.
///
/// One fallback is accepted on a GPU host, and only with its reason on stderr:
/// the driver losing the device under the process (FND-190). On the GTX 1080 Ti
/// host the NVIDIA driver raises Xid 13 during GPU initialisation — logged even
/// for a probe that only enumerated adapters — and a process using the GPU at that
/// moment gets "Parent device is lost". fnec falls back to the CPU and says so,
/// which is the behaviour to test; any OTHER decline still fails.
pub fn assert_gpu_exec_label(stderr: &str) {
    if !pollster::block_on(nec_accel::hardware_adapter_present()) {
        assert_diag_field(stderr, "exec", "gpu(cpu-fallback)");
        return;
    }
    if diag_field(stderr, "exec") == Some("gpu(cpu-fallback)") && stderr.contains("device is lost")
    {
        eprintln!(
            "NOTE: the driver lost the GPU device under this run (FND-190); fallback accepted"
        );
        return;
    }
    assert_diag_field(stderr, "exec", "gpu");
}

/// Every `exec=` field of a hybrid run names what solved its point — `cpu`, or
/// `gpu` / `gpu(cpu-fallback)` from the GPU lane — never the requested `hybrid`.
pub fn assert_points_say_what_ran(stderr: &str) {
    let labels: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("diag:"))
        .filter_map(|l| l.split_whitespace().find_map(|f| f.strip_prefix("exec=")))
        .collect();
    assert!(!labels.is_empty(), "no diag lines in:\n{stderr}");
    for l in labels {
        assert!(
            matches!(l, "cpu" | "gpu" | "gpu(cpu-fallback)"),
            "exec={l}: a point must say what ran\n{stderr}"
        );
    }
}

/// A multi-point `--exec hybrid` run on a deck the GPU solves: with a hardware
/// GPU the lane runs and reports how many points it solved; without one the run
/// says it is CPU-only, and why.
pub fn assert_hybrid_lane_reported(stderr: &str, points: usize) {
    if pollster::block_on(nec_accel::hardware_adapter_present()) {
        assert!(
            stderr.contains(&format!("of {points} sweep points solved on the GPU")),
            "a GPU is present and the hybrid run reported no lane:\n{stderr}"
        );
    } else {
        assert!(
            stderr.contains("--exec hybrid runs on the CPU only: no hardware GPU"),
            "no GPU, and the hybrid run did not say it is CPU-only:\n{stderr}"
        );
    }
}
