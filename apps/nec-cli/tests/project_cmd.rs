// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// GAP-015's acceptance criterion names "explicit CLI/API entry points" for
// Markdown project import and export. The library half shipped with round-trip
// tests and the criterion was marked Done citing only that half (FND-006), while
// `nec_project` sat in the CLI's manifest as a dependency no source file
// imported (FND-016). These exercise the entry point that closes both.

use std::process::Command;

mod common;

const PROJECT_TOML: &str = "version = 1\nname = \"Test dipole project\"\n\
                            deck_path = \"corpus/dipole-freesp-51seg.nec\"\n\n\
                            [solver]\nmode = \"hallen\"\npulse_rhs = \"auto\"\n";

fn fnec(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .output()
        .expect("run fnec")
}

/// The round trip is the property GAP-015 asks for, through the CLI rather than
/// the library: a project that goes out as Markdown and back must be the same
/// project, byte for byte.
#[test]
fn a_project_round_trips_through_markdown_via_the_cli() {
    let src = common::TempDeck::new("project-in.toml", PROJECT_TOML);
    // The CLI writes these two; they live in a directory that removes them.
    let dir = common::TempDir::new("project-round-trip");
    let md = dir.join("mid.md");
    let back = dir.join("out.toml");

    let out = fnec(&[
        "project",
        "convert",
        src.to_str().unwrap(),
        md.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let markdown = std::fs::read_to_string(&md).expect("markdown written");
    assert!(
        markdown.contains("fnec-project-markdown"),
        "not the documented Markdown schema: {markdown}"
    );

    let out = fnec(&[
        "project",
        "convert",
        md.to_str().unwrap(),
        back.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&back).expect("toml written"),
        PROJECT_TOML,
        "the round trip must return the same project"
    );
}

/// With no output path the document goes to stdout, so the converter composes
/// with a pipeline rather than only with a filename.
#[test]
fn conversion_without_an_output_path_writes_to_stdout() {
    let src = common::TempDeck::new("project-stdout.toml", PROJECT_TOML);
    let out = fnec(&["project", "convert", src.to_str().unwrap()]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Test dipole project"), "{stdout}");
}

/// A malformed project is refused with the offending file named, and **nothing
/// is written** — the output is rendered before the destination is touched, so a
/// conversion that cannot be produced does not truncate an existing file.
#[test]
fn a_malformed_project_is_refused_without_writing_the_output() {
    let src = common::TempDeck::new("project-bad.toml", "version = 1\ndeck_path = \"x.nec\"\n");
    let dst = common::TempDeck::new("project-untouched.md", "PRE-EXISTING");

    let out = fnec(&[
        "project",
        "convert",
        src.to_str().unwrap(),
        dst.to_str().unwrap(),
    ]);
    assert!(
        !out.status.success(),
        "a project missing `name` must be refused"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("bad.toml"),
        "the refusal must name the file: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(&dst).expect("read"),
        "PRE-EXISTING",
        "the destination must not be touched when the conversion fails"
    );
}

#[test]
fn an_unknown_project_subcommand_is_refused_with_usage() {
    let out = fnec(&["project", "explode"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("explode") && err.contains("Usage"), "{err}");
}

/// The CLI must still behave as before for everything else — a subcommand is a
/// new door, not a change to the existing one.
#[test]
fn the_ordinary_solve_path_is_unaffected() {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/dipole-freesp-51seg.nec"
    );
    let out = fnec(&[root]);
    assert!(
        out.status.success(),
        "{:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("FEEDPOINTS"));
}
