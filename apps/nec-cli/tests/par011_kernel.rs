// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PAR-011 P2b: fnec as 4nec2's kernel, the way 4nec2 runs its kernel.
//!
//! Observed with a real 4nec2 under Wine (`docs/par011-dropin-evidence-memo.md`):
//! 4nec2 writes `..\out\NAME.inp`, runs `Nec2dXS1k5.exe < nec2d.tmp` from its
//! `exe` folder with no arguments — `nec2d.tmp` holding the two paths, CRLF,
//! backslashes — ignores the exit status, and reads `..\out\NAME.out`: success is
//! its ` RUN TIME` line, and anything else is a failure whose text it shows. Each
//! test builds that tree and runs fnec in it, by its subcommand and under the
//! kernel's own name.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/par011")
}

/// A 4nec2-like tree for one test: `exe/` (the kernel's working directory) and
/// `out/` holding the fixture's `.inp`.
fn tree(test: &str, fixture: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("par011-kernel-{test}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("exe")).unwrap();
    std::fs::create_dir_all(root.join("out")).unwrap();
    std::fs::copy(
        fixtures().join(format!("{fixture}.inp")),
        root.join(format!("out/{fixture}.inp")),
    )
    .unwrap();
    root
}

/// Run `program args` in `exe/` with `stdin`, as 4nec2's batch file does.
fn run(program: &Path, args: &[&str], cwd: &Path, stdin: &str) -> std::process::Output {
    use std::io::Write;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawns");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().expect("waits")
}

/// What `nec2d.tmp` holds for `NAME`.
fn tmp(name: &str) -> String {
    format!("..\\out\\{name}.inp\r\n..\\out\\{name}.out\r\n")
}

/// A report's lines without the run-dependent timing.
fn stable(text: &str) -> Vec<String> {
    text.replace('\r', "")
        .lines()
        .filter(|l| !l.starts_with(" RUN TIME") && !l.contains("FILL="))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_kernel_writes_the_nec2d_report_to_the_output_file() {
    let root = tree("subcommand", "dip_free");
    let out = run(
        Path::new(env!("CARGO_BIN_EXE_fnec")),
        &["nec2d-kernel"],
        &root.join("exe"),
        &tmp("dip_free"),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = std::fs::read_to_string(root.join("out/dip_free.out")).expect("the .out");
    assert!(report.contains("\r\n"), "the kernel writes CRLF");
    assert!(report
        .trim_end()
        .lines()
        .last()
        .unwrap()
        .starts_with(" RUN TIME ="));
    // The same report the CLI prints for the same deck.
    let cli = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["--exec", "cpu", "--output-format", "nec2d"])
        .arg(root.join("out/dip_free.inp"))
        .output()
        .unwrap();
    assert_eq!(
        stable(&report),
        stable(&String::from_utf8_lossy(&cli.stdout))
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Under the kernel's own name, with no arguments, fnec is the kernel.
#[test]
fn under_the_kernels_name_fnec_is_the_kernel() {
    let root = tree("alias", "dip_ld");
    let kernel = root.join("exe/Nec2dXS1k5");
    std::fs::copy(env!("CARGO_BIN_EXE_fnec"), &kernel).unwrap();
    let out = run(&kernel, &[], &root.join("exe"), &tmp("dip_ld"));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = std::fs::read_to_string(root.join("out/dip_ld.out")).expect("the .out");
    assert!(report.contains("STRUCTURE IMPEDANCE LOADING"), "{report}");
    assert!(report
        .trim_end()
        .lines()
        .last()
        .unwrap()
        .starts_with(" RUN TIME ="));
    let _ = std::fs::remove_dir_all(&root);
}

/// A deck the NEC-2D output cannot print: the kernel's failure — the reason, no
/// RUN TIME line — written where 4nec2 reads it, and exit status 0 as the kernel's.
#[test]
fn a_refused_deck_is_the_kernels_failure_in_the_output_file() {
    let root = tree("refused", "nt_net");
    let out = run(
        Path::new(env!("CARGO_BIN_EXE_fnec")),
        &["nec2d-kernel"],
        &root.join("exe"),
        &tmp("nt_net"),
    );
    assert!(out.status.success(), "the kernel exits 0 on a deck error");
    let report = std::fs::read_to_string(root.join("out/nt_net.out")).expect("the .out");
    assert!(
        report.contains("FNEC: ") && report.contains("NT"),
        "{report}"
    );
    assert!(!report.contains("RUN TIME"), "{report}");
    let _ = std::fs::remove_dir_all(&root);
}

/// An input that is not there: the failure is written, not a crash.
#[test]
fn a_missing_input_is_the_kernels_failure_in_the_output_file() {
    let root = tree("missing", "dip_free");
    let out = run(
        Path::new(env!("CARGO_BIN_EXE_fnec")),
        &["nec2d-kernel"],
        &root.join("exe"),
        &tmp("nothere"),
    );
    assert!(out.status.success());
    let report = std::fs::read_to_string(root.join("out/nothere.out")).expect("the .out");
    assert!(report.contains("CANNOT READ THE INPUT FILE"), "{report}");
    assert!(!report.contains("RUN TIME"), "{report}");
    let _ = std::fs::remove_dir_all(&root);
}

/// With no output path there is nowhere to write a failure: that alone exits 1.
#[test]
fn no_output_path_exits_non_zero() {
    let root = tree("nopath", "dip_free");
    let out = run(
        Path::new(env!("CARGO_BIN_EXE_fnec")),
        &["nec2d-kernel"],
        &root.join("exe"),
        "..\\out\\dip_free.inp\r\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("did not name an input and an output"));
    let _ = std::fs::remove_dir_all(&root);
}
