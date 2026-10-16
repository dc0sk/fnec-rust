// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `fnec nec2d-kernel`: 4nec2's external-kernel contract (PAR-011), as observed
//! with a real 4nec2 under Wine (`docs/par011-dropin-evidence-memo.md`).
//!
//! 4nec2 writes the deck to `..\out\NAME.inp`, writes two lines naming it and
//! the output file to `nec2d.tmp`, and runs its kernel with no arguments, its
//! working directory the `exe` folder and its stdin that file. It ignores the
//! kernel's exit status and reads the output file: success is the file's last line,
//! ` RUN TIME = …`; anything else is a failure, whose text 4nec2 shows the user.
//!
//! So this reads the two paths, solves the input with `--output-format nec2d`
//! through this same binary, and writes the result to the output file, CRLF as
//! the kernel writes it, with fnec's warnings inside it where 4nec2 shows them. A
//! deck fnec refuses, or cannot read, becomes the failure the kernel would have
//! written: its reason and no `RUN TIME`. The exit status is 0 whenever the output
//! file was written, as the kernel's is; it is 1 only when there is no output path
//! to write to.

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// The usage line for a person who runs the kernel at a terminal.
const KERNEL_USAGE: &str =
    "fnec nec2d-kernel reads two lines on stdin — the input deck's path, then \
     the output file's — as 4nec2 feeds its kernel (`nec2dxs*.exe < nec2d.tmp`), and writes the \
     NEC-2D report to the output file";

/// A path as 4nec2 writes it: relative to the working directory, with Windows
/// separators. Only the line end is stripped (a CR, then spaces); the path is
/// otherwise taken as it stands, since 4nec2 writes 8.3 names with no quoting.
fn kernel_path(line: &str) -> PathBuf {
    let trimmed = line.trim_end_matches(['\r', '\n']).trim_end();
    if cfg!(windows) {
        PathBuf::from(trimmed)
    } else {
        PathBuf::from(trimmed.replace('\\', "/"))
    }
}

/// The report 4nec2 receives: CRLF line ends, and fnec's warnings — which the
/// kernel has no other channel for — as text lines before `RUN TIME`, where 4nec2
/// tolerates free text (the insertion ablation) and shows it on F8.
fn with_warnings(report: &str, stderr: &str) -> String {
    let notes: Vec<String> = stderr
        .lines()
        .filter_map(|l| l.strip_prefix("warning: "))
        .map(|w| format!(" FNEC WARNING: {w}"))
        .collect();
    let mut out = String::new();
    for line in report.lines() {
        if line.starts_with(" RUN TIME") {
            for n in &notes {
                out.push_str(n);
                out.push_str("\r\n");
            }
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out
}

/// The failure the kernel would have written: the reason, and no `RUN TIME`.
fn failure(reason: &str) -> String {
    nec_report::nec2d::render_failure(None, &[], reason).replace('\n', "\r\n")
}

pub(crate) fn run() -> ExitCode {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        eprintln!("{KERNEL_USAGE}");
        return ExitCode::from(2);
    }
    // The kernel's prompts, which 4nec2 does not read.
    let mut stdout = std::io::stdout();
    let mut lines = stdin.lock().lines();
    let _ = write!(stdout, " ENTER NAME OF INPUT FILE > ");
    let input = lines.next().and_then(Result::ok).map(|l| kernel_path(&l));
    let _ = write!(stdout, "ENTER NAME OF OUTPUT FILE > ");
    let output = lines.next().and_then(Result::ok).map(|l| kernel_path(&l));
    let _ = writeln!(stdout);
    let (Some(input), Some(output)) = (input, output) else {
        eprintln!("error: nec2d-kernel: stdin did not name an input and an output file");
        return ExitCode::FAILURE;
    };
    let report = solve(&input);
    match std::fs::write(&output, report) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!(
                "error: nec2d-kernel: cannot write '{}': {e}",
                output.display()
            );
            ExitCode::FAILURE
        }
    }
}

/// The output file's content for `input`: the report, or the kernel's failure.
fn solve(input: &std::path::Path) -> String {
    if !input.is_file() {
        return failure(&format!(
            "FNEC: CANNOT READ THE INPUT FILE {}",
            input.display()
        ));
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return failure(&format!("FNEC: CANNOT FIND ITS OWN EXECUTABLE: {e}")),
    };
    // The same binary, as the CLI: a drop-in name with a deck argument is plain
    // fnec. The CPU path, until the host is calibrated for the device (P3).
    let child = Command::new(exe)
        .args(["--exec", "cpu", "--output-format", "nec2d"])
        .arg(input)
        .output();
    let child = match child {
        Ok(c) => c,
        Err(e) => return failure(&format!("FNEC: CANNOT RUN THE SOLVE: {e}")),
    };
    let stderr = String::from_utf8_lossy(&child.stderr);
    if child.status.success() {
        return with_warnings(&String::from_utf8_lossy(&child.stdout), &stderr);
    }
    let reasons: Vec<&str> = stderr
        .lines()
        .filter_map(|l| l.strip_prefix("error: "))
        .collect();
    let reason = if reasons.is_empty() {
        "the solve failed".to_string()
    } else {
        reasons.join("; ")
    };
    failure(&format!("FNEC: {reason}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kernel_path_is_the_line_without_its_end() {
        let p = kernel_path("..\\out\\dip_swee.inp\r\n");
        if cfg!(windows) {
            assert_eq!(p, PathBuf::from("..\\out\\dip_swee.inp"));
        } else {
            assert_eq!(p, PathBuf::from("../out/dip_swee.inp"));
        }
    }

    #[test]
    fn warnings_go_before_run_time_and_lines_end_in_crlf() {
        let out = with_warnings(
            "A\n\n RUN TIME =     0.010\n",
            "diag: x\nwarning: one\nwarning: two\n",
        );
        assert_eq!(
            out,
            "A\r\n\r\n FNEC WARNING: one\r\n FNEC WARNING: two\r\n RUN TIME =     0.010\r\n"
        );
    }

    #[test]
    fn a_failure_has_its_reason_and_no_run_time() {
        let out = failure("FNEC: NO");
        assert!(out.contains("FNEC: NO"), "{out}");
        assert!(!out.contains("RUN TIME"), "{out}");
    }
}
