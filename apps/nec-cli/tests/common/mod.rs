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
/// What an `--exec gpu` run's stderr says the device did: the one reading of
/// fnec's GPU fallback messages that every CLI GPU test shares (FND-215).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuRun {
    /// No fallback message: the device did the work.
    OnDevice,
    /// A transient miss — no adapter seen, the device not created or lost, a
    /// readback that failed. Worth one re-run where a GPU is expected.
    Missed(String),
    /// A deterministic decline — a system past the device's storage binding, or an
    /// f32 solve that failed its residual check. It fails the same way every time.
    Declined(String),
}

/// A run whose device work missed, kept so a test without a GPU can still read
/// its fallback output; prints as its reason only.
pub struct GpuMiss {
    pub why: String,
    pub stdout: String,
    pub stderr: String,
}

impl std::fmt::Debug for GpuMiss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

pub fn classify_gpu_stderr(stderr: &str) -> GpuRun {
    let fallbacks: Vec<&str> = stderr
        .lines()
        .filter(|l| {
            l.contains("solve_hallen_gpu_resident:")
                || l.contains("falling back to CPU Z-matrix fill")
                || l.contains("falling back to CPU RP")
        })
        .collect();
    let deterministic =
        |l: &&str| l.contains("storage binding holds") || l.contains("did not converge");
    if let Some(l) = fallbacks.iter().find(|l| !deterministic(l)) {
        return GpuRun::Missed(l.to_string());
    }
    match fallbacks.first() {
        Some(l) => GpuRun::Declined(l.to_string()),
        None => GpuRun::OnDevice,
    }
}

/// The run's `exec=` label: `gpu` where a GPU is expected, `gpu(cpu-fallback)`
/// where none is. The label used to read `gpu(cpu-fallback)` on every point,
/// including the ones the device solved — so tests that asserted it were false
/// on a GPU host.
///
/// `first` is the run's stderr; `rerun` runs it again and returns its stderr. A
/// fallback where a GPU is expected — the driver losing the device or hiding the
/// adapter (FND-190) — earns exactly one re-run under the shared policy
/// (FND-215); a second fails. It used to be accepted on its reason with no
/// re-run, so a GPU-host run passed with the device solving nothing.
pub fn assert_gpu_exec_label(first: &str, mut rerun: impl FnMut() -> String) {
    let mut given = Some(first.to_string());
    let attempt = nec_accel::gpu_test::gpu_attempt("the --exec gpu label", || {
        let s = given.take().unwrap_or_else(&mut rerun);
        if diag_field(&s, "exec") == Some("gpu") {
            Ok(s)
        } else {
            Err(s)
        }
    });
    if let Err(s) = attempt {
        assert_diag_field(&s, "exec", "gpu(cpu-fallback)");
    }
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

/// A multi-point `--exec hybrid` run on a deck the GPU solves: where a GPU is
/// expected the lane runs and reports how many points it solved; where none is,
/// the run says it is CPU-only, and why.
///
/// FND-190: this host's driver has momentarily hidden the GPU from adapter
/// enumeration — the run said "no hardware GPU" while a probe found one. A run
/// without its lane where a GPU is expected earns one re-run under the shared
/// policy (`rerun` returns its stderr; FND-215): a transient passes on it, while
/// a hybrid that cannot find a present GPU fails both.
pub fn assert_hybrid_lane_reported(stderr: &str, points: usize, mut rerun: impl FnMut() -> String) {
    const CPU_ONLY: &str = "--exec hybrid runs on the CPU only: no hardware GPU";
    let lane = format!("of {points} sweep points solved on the GPU");
    let mut given = Some(stderr.to_string());
    let attempt = nec_accel::gpu_test::gpu_attempt("the --exec hybrid lane", || {
        let s = given.take().unwrap_or_else(&mut rerun);
        if s.contains(&lane) {
            Ok(s)
        } else {
            Err(s)
        }
    });
    if let Err(s) = attempt {
        assert!(
            s.contains(CPU_ONLY),
            "no GPU, and the hybrid run did not say it is CPU-only:\n{s}"
        );
    }
}
