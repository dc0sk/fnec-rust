// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! `fnec project` — the CLI entry point for project files.
//!
//! GAP-015 asks for deterministic Markdown project import and export "with
//! documented schema, round-trip stability tests, **and explicit CLI/API entry
//! points**". The library half shipped and was tested; the entry points were not
//! written, and the item was marked Done citing only the half that existed
//! (FND-006). `nec_project` was meanwhile declared as a CLI dependency that no
//! source file imported, so the binary linked and the SBOM carried a crate it
//! never used (FND-016). One missing frontend, two findings.
//!
//! Format is chosen by extension, because a project file is a document the user
//! names: `.md` is Markdown, anything else is TOML. Conversion is
//! parse-then-render through `nec_project`, so a file that survives a round trip
//! here is one the library considers well-formed — the converter cannot be more
//! permissive than the loader it delegates to.

use std::path::Path;
use std::process::ExitCode;

use nec_project::ProjectFile;

pub const PROJECT_USAGE: &str = "Usage: fnec project convert <in.toml|in.md> [out.md|out.toml]\n\
                                 \n\
                                 Converts a project file between TOML and Markdown. The format is\n\
                                 taken from each path's extension (.md = Markdown, else TOML).\n\
                                 With no output path, the converted document is written to stdout.";

fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// Load a project file, taking its format from the path's extension.
pub fn load(path: &Path) -> Result<ProjectFile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
    if is_markdown(path) {
        ProjectFile::from_markdown(&text).map_err(|e| format!("{}: {e}", path.display()))
    } else {
        ProjectFile::from_toml(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Render a project file in the format the path's extension names.
pub fn render(project: &ProjectFile, path: Option<&Path>) -> Result<String, String> {
    let markdown = path.is_some_and(is_markdown);
    if markdown {
        project.to_markdown().map_err(|e| e.to_string())
    } else {
        project.to_toml().map_err(|e| e.to_string())
    }
}

pub fn run(args: &[String]) -> ExitCode {
    // args[0] = binary, args[1] = "project"
    match args.get(2).map(String::as_str) {
        Some("convert") => {}
        Some(other) => {
            eprintln!("error: unknown project subcommand '{other}'");
            eprintln!("{PROJECT_USAGE}");
            return ExitCode::from(2);
        }
        None => {
            eprintln!("{PROJECT_USAGE}");
            return ExitCode::from(2);
        }
    }

    let Some(input) = args.get(3) else {
        eprintln!("error: missing input path");
        eprintln!("{PROJECT_USAGE}");
        return ExitCode::from(2);
    };
    let input = Path::new(input);
    let output = args.get(4).map(Path::new);

    // Read before writing, so a conversion that cannot be produced does not
    // truncate the destination first.
    let project = match load(input) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let rendered = match render(&project, output) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &rendered) {
                eprintln!("error: cannot write '{}': {e}", path.display());
                return ExitCode::FAILURE;
            }
            eprintln!(
                "wrote {} ({} run definition(s), {} history record(s))",
                path.display(),
                project.runs.len(),
                project.history.run_count()
            );
        }
        None => print!("{rendered}"),
    }
    ExitCode::SUCCESS
}
