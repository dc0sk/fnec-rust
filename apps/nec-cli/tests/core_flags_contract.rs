use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_deck(name: &str) -> PathBuf {
    workspace_root().join("corpus").join(name)
}

fn run_fnec(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("Failed to run fnec: {e}"))
}

#[test]
fn missing_solver_value_reports_contract_error_and_usage() {
    let output = run_fnec(&["--solver"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: fnec [--solver <hallen|pulse|continuity|sinusoidal|mpie>]"),
        "missing usage contract in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("missing value after --solver"),
        "missing parse error detail in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_solver_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--solver",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --solver value 'bogus'"),
        "missing invalid solver detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: hallen|pulse|continuity|sinusoidal"),
        "missing expected solver values in stderr:\n{stderr}"
    );
}

#[test]
fn missing_pulse_rhs_value_reports_contract_error_and_usage() {
    let output = run_fnec(&["--pulse-rhs"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing value after --pulse-rhs"),
        "missing pulse-rhs parse error in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: raw|nec2"),
        "missing expected pulse-rhs values in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_pulse_rhs_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--pulse-rhs",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --pulse-rhs value 'bogus'"),
        "missing invalid pulse-rhs detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: raw|nec2"),
        "missing expected pulse-rhs values in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_exec_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--exec",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --exec value 'bogus'"),
        "missing invalid-value detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: cpu|hybrid|gpu"),
        "missing expected-value hint in stderr:\n{stderr}"
    );
}

#[test]
fn missing_sin_fallback_rel_max_value_reports_contract_error_and_usage() {
    let output = run_fnec(&["--sin-fallback-rel-max"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing value after --sin-fallback-rel-max"),
        "missing sin-fallback parse error in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: positive number"),
        "missing expected positive-number hint in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_sin_fallback_rel_max_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--sin-fallback-rel-max",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --sin-fallback-rel-max value 'bogus'"),
        "missing invalid sin-fallback detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: positive number"),
        "missing expected positive-number hint in stderr:\n{stderr}"
    );
}

#[test]
fn missing_bench_format_value_reports_contract_error_and_usage() {
    let output = run_fnec(&["--bench-format"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing value after --bench-format"),
        "missing bench-format parse error in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: human|csv|json"),
        "missing expected bench-format values in stderr:\n{stderr}"
    );
}

#[test]
fn invalid_bench_format_value_reports_contract_error_and_usage() {
    let output = run_fnec(&[
        "--bench-format",
        "bogus",
        fixture_deck("dipole-freesp-51seg.nec").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --bench-format value 'bogus'"),
        "missing invalid bench-format detail in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: human|csv|json"),
        "missing expected bench-format values in stderr:\n{stderr}"
    );
}

#[test]
fn unknown_option_reports_contract_error() {
    let output = run_fnec(&["--definitely-not-a-flag"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown option: --definitely-not-a-flag"),
        "missing unknown-option error in stderr:\n{stderr}"
    );
}

#[test]
fn unexpected_extra_argument_reports_contract_error() {
    let deck = fixture_deck("dipole-freesp-51seg.nec");
    let output = run_fnec(&[
        "--solver",
        "hallen",
        deck.to_str().unwrap(),
        deck.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected extra argument:"),
        "missing extra-argument parse error in stderr:\n{stderr}"
    );
}

#[test]
fn missing_deck_path_reports_contract_error() {
    let output = run_fnec(&["--solver", "hallen", "--pulse-rhs", "nec2"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing deck path"),
        "missing deck-path parse error in stderr:\n{stderr}"
    );
}

#[test]
fn missing_hosts_value_reports_contract_error() {
    let output = run_fnec(&["--hosts"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing value after --hosts"),
        "missing --hosts parse error in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("expected: path to hosts.toml file"),
        "missing expected-path hint in stderr:\n{stderr}"
    );
}

#[test]
fn hosts_nonexistent_file_reports_error() {
    let deck = fixture_deck("dipole-freesp-51seg.nec");
    let output = run_fnec(&[
        "--hosts",
        "/tmp/definitely-does-not-exist.toml",
        deck.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:"),
        "expected error in stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("IO error reading hosts config"),
        "expected IO error message in stderr:\n{stderr}"
    );
}

/// FND-150: the hosts file is checked before the deck is refused, so a bad
/// `--hosts` path is reported even for a deck that also has no frequency —
/// it used to be skipped by the frequency refusal and never mentioned.
#[test]
fn a_bad_hosts_path_is_reported_even_when_the_deck_is_refused() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let deck = dir.join(format!("no-fr-{}.nec", std::process::id()));
    std::fs::write(
        &deck,
        "CE\nGW 1 21 0 0 -5 0 0 5 0.001\nGE 0\nEX 0 1 11 0 1 0\nEN\n",
    )
    .expect("write deck");
    let output = run_fnec(&[
        "--hosts",
        "/tmp/definitely-does-not-exist.toml",
        deck.to_str().unwrap(),
    ]);
    let _ = std::fs::remove_file(&deck);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("IO error reading hosts config"),
        "the bad --hosts path must be reported:\n{stderr}"
    );
}

/// FND-181: ...and for a deck with no `EX` at all. The undriven-deck check ran
/// before the hosts file was read, so a bad path went unreported, contradicting
/// the comment at that read.
#[test]
fn a_bad_hosts_path_is_reported_even_for_an_undriven_deck() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let deck = dir.join(format!("no-ex-{}.nec", std::process::id()));
    std::fs::write(
        &deck,
        "CE\nGW 1 21 0 0 -5 0 0 5 0.001\nGE 0\nFR 0 1 0 0 14.2 0\nEN\n",
    )
    .expect("write deck");
    let output = run_fnec(&[
        "--hosts",
        "/tmp/definitely-does-not-exist.toml",
        deck.to_str().unwrap(),
    ]);
    let _ = std::fs::remove_file(&deck);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("IO error reading hosts config"),
        "the bad --hosts path must be reported:\n{stderr}"
    );
}

#[test]
fn all_core_flags_combination_runs_successfully() {
    let deck = fixture_deck("dipole-freesp-51seg.nec");
    let output = run_fnec(&[
        "--solver",
        "hallen",
        "--pulse-rhs",
        "raw",
        "--exec",
        "cpu",
        "--bench-format",
        "json",
        deck.to_str().unwrap(),
    ]);

    assert!(
        output.status.success(),
        "fnec failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("FNEC FEEDPOINT REPORT"),
        "expected report header in stdout, got:\n{stdout}"
    );
}

/// FND-169: `--version` used to print the usage text, whose first line reads
/// `fnec <version>`, and exit 2 with "unknown option". A release smoke test took
/// that first line as a version report. The version goes to stdout, alone, with
/// exit 0 — and a script can tell it from an error.
#[test]
fn version_flag_prints_the_version_alone_and_exits_zero() {
    for flag in ["--version", "-V"] {
        let output = run_fnec(&[flag]);
        assert_eq!(output.status.code(), Some(0), "{flag}: exit status");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("fnec {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}: stdout"
        );
        assert!(
            output.stderr.is_empty(),
            "{flag}: stderr {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Asked-for help is not an error: usage on stdout, exit 0. (It was "unknown
/// option", exit 2, with the usage on stderr — the same defect as FND-169.)
#[test]
fn help_flag_prints_usage_to_stdout_and_exits_zero() {
    for flag in ["--help", "-h"] {
        let output = run_fnec(&[flag]);
        assert_eq!(output.status.code(), Some(0), "{flag}: exit status");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with(&format!("fnec {}\n", env!("CARGO_PKG_VERSION"))),
            "{flag}: stdout must open with the version:\n{stdout}"
        );
        assert!(
            stdout.contains("Usage: fnec [--solver <hallen|pulse|continuity|sinusoidal|mpie>]"),
            "{flag}: usage missing from stdout:\n{stdout}"
        );
        assert!(
            output.stderr.is_empty(),
            "{flag}: stderr {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
