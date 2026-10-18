// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Names the commit fnec is built from, for `--version` and the report header:
//! `FNEC_BUILD_COMMIT` is twelve hex digits of `HEAD`, `-dirty` when fnec's own
//! inputs differ from that commit, or `unknown` where git cannot say (a source
//! tarball). A figure quoted from a binary can then be traced to its build — a
//! stale binary in a shared target dir once carried a 3 % discrepancy (FND-264).
//!
//! The release and Windows builds run in a container that cannot read `.git`; the
//! host computes the same value (`scripts/build-commit.sh`) and passes it in
//! `FNEC_BUILD_COMMIT`, which wins here.
//!
//! "fnec's inputs" is one path set, used both for the dirty flag and for the
//! rerun triggers, so the two cannot disagree: `scripts/build-commit.sh` holds the
//! same list. An untracked file under it counts — an untracked `mod` file is a
//! binary the commit cannot reproduce; so does an untracked scratch deck under
//! `apps/nec-cli/tests/`. Docs, the GUI and the Python binding do not.

use std::path::Path;
use std::process::Command;

/// fnec's inputs, as top-level-relative pathspecs. Keep in step with
/// `scripts/build-commit.sh`.
const INPUTS: [&str; 5] = [
    "crates",
    "apps/nec-cli",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
];

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        // The release container mounts the tree read-only: never write the index.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn valid(commit: &str) -> bool {
    let hash = commit.strip_suffix("-dirty").unwrap_or(commit);
    commit == "unknown"
        || (hash.len() == 12 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

fn main() {
    println!("cargo:rerun-if-env-changed=FNEC_BUILD_COMMIT");
    println!("cargo:rerun-if-changed=build.rs");

    let commit = match std::env::var("FNEC_BUILD_COMMIT") {
        Ok(v) if !v.is_empty() => v,
        _ => from_git(),
    };
    assert!(
        valid(&commit),
        "FNEC_BUILD_COMMIT must be 12 hex digits, optionally -dirty, or unknown: {commit:?}"
    );
    println!("cargo:rustc-env=FNEC_BUILD_COMMIT={commit}");
}

fn from_git() -> String {
    let Some(top) = git(&["rev-parse", "--show-toplevel"]) else {
        return "unknown".into();
    };
    let top = top.trim();
    // Rerun when HEAD moves (the reflog is appended on every commit, amend,
    // checkout, reset, pull and rebase, loose or packed ref alike), when the index
    // changes, and when any input changes on disk. A listed path that does not
    // exist would rerun on every build, so only existing ones are listed.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        let dir = Path::new(dir.trim());
        for p in [dir.join("HEAD"), dir.join("logs/HEAD"), dir.join("index")] {
            if p.exists() {
                println!("cargo:rerun-if-changed={}", p.display());
            }
        }
    }
    for input in INPUTS {
        let p = Path::new(top).join(input);
        if p.exists() {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }

    // Full hash, cut here: `--short=N` lengthens on ambiguity.
    let Some(head) = git(&["rev-parse", "HEAD"]) else {
        return "unknown".into();
    };
    let head = head.trim();
    if head.len() < 12 {
        return "unknown".into();
    }
    let mut args = vec!["status", "--porcelain", "--untracked-files=normal", "--"];
    let specs: Vec<String> = INPUTS.iter().map(|i| format!(":/{i}")).collect();
    args.extend(specs.iter().map(String::as_str));
    match git(&args) {
        Some(s) if s.trim().is_empty() => head[..12].to_string(),
        Some(_) => format!("{}-dirty", &head[..12]),
        None => "unknown".into(),
    }
}
