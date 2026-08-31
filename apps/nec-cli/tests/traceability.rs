// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! G2 — machine-enforced requirements traceability (review-260821).
//!
//! Parses `docs/project/requirements.toml` (the requirements register, the
//! single source of truth) and scans the workspace source for `// VERIFIES:
//! <ID>` citations, then fails on:
//! - GAP: a `verification = "test"` requirement that no test cites.
//! - DANGLING: a `// VERIFIES:` citing an id absent from the register (renamed/removed).
//!
//! Because this runs inside `cargo test` (and therefore the CI `test` gate), a
//! test-verified requirement cannot be added without a citation, and a citation
//! cannot outlive the id it points at.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve workspace root")
}

/// `(id, verification)` for every registered requirement.
fn load_register(root: &Path) -> Vec<(String, String)> {
    let path = root.join("docs/project/requirements.toml");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let value: toml::Value = toml::from_str(&text).expect("requirements.toml is valid TOML");
    let reqs = value
        .get("requirement")
        .and_then(|v| v.as_array())
        .expect("requirements.toml has a [[requirement]] array");
    reqs.iter()
        .map(|r| {
            let id = r
                .get("id")
                .and_then(|v| v.as_str())
                .expect("each requirement has a string id")
                .to_string();
            let verification = r
                .get("verification")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("requirement {id} is missing `verification`"))
                .to_string();
            assert!(
                verification == "test" || verification == "review",
                "requirement {id}: verification must be \"test\" or \"review\", got {verification:?}"
            );
            (id, verification)
        })
        .collect()
}

/// Map of cited id -> the `file:line` sites where `// VERIFIES: <id>` appears.
fn collect_citations(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for top in ["crates", "apps"] {
        collect_dir(&root.join(top), root, &mut out);
    }
    out
}

fn collect_dir(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<String>>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().map(|n| n == "target").unwrap_or(false) {
                continue; // build artefacts
            }
            collect_dir(&path, root, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            if path.ends_with("traceability.rs") {
                continue; // the checker must not cite itself
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            // Only citations that are actually IN A TEST count.
            //
            // `requirements.toml` says `verification = "test"` means the
            // requirement is cited by a `// VERIFIES:` in a test, and this
            // collector used to accept the marker on any line of any `.rs` file
            // -- including production code, and including a string literal that
            // merely contained the text. So a requirement could read as verified
            // by a citation that no test ever ran (FND-091).
            //
            // A gate weaker than its own stated contract is worse than no gate,
            // because the contract is what people read.
            let in_tests_dir = path
                .components()
                .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "benches");
            let mut in_test_mod = false;
            let mut pending = false;
            let mut depth: i32 = 0;
            for (i, line) in text.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("#[cfg(test)]") {
                    pending = true;
                }
                if pending && line.contains('{') {
                    in_test_mod = true;
                    pending = false;
                    depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
                } else if in_test_mod {
                    depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                    if depth <= 0 {
                        in_test_mod = false;
                    }
                }
                if !(in_tests_dir || in_test_mod) {
                    continue;
                }
                // And in a COMMENT, not a string literal that happens to contain
                // the marker.
                if !trimmed.starts_with("//") {
                    continue;
                }
                if let Some(id) = parse_verifies(line) {
                    let rel = path.strip_prefix(root).unwrap_or(&path);
                    out.entry(id)
                        .or_default()
                        .push(format!("{}:{}", rel.display(), i + 1));
                }
            }
        }
    }
}

/// Extract `<ID>` from a line containing `VERIFIES: <ID>`. Ids are
/// `[A-Z][A-Z0-9-]*` (e.g. `FR-003`, `NFR-004`).
fn parse_verifies(line: &str) -> Option<String> {
    let marker = "VERIFIES:";
    let idx = line.find(marker)?;
    let rest = line[idx + marker.len()..].trim_start();
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '-')
        .collect();
    // require at least one leading letter to avoid matching stray punctuation
    if id.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        Some(id)
    } else {
        None
    }
}

#[test]
fn requirements_register_is_fully_traced() {
    let root = workspace_root();
    let register = load_register(&root);
    assert!(
        !register.is_empty(),
        "requirements.toml declares no requirements"
    );
    let declared: BTreeSet<&str> = register.iter().map(|(id, _)| id.as_str()).collect();
    let citations = collect_citations(&root);

    // GAP: verification="test" requirements with no citation.
    let mut gaps: Vec<&str> = register
        .iter()
        .filter(|(_, v)| v == "test")
        .filter(|(id, _)| !citations.contains_key(id))
        .map(|(id, _)| id.as_str())
        .collect();
    gaps.sort_unstable();

    // DANGLING: citations to ids not present in the register.
    let mut dangling: Vec<&str> = citations
        .keys()
        .map(String::as_str)
        .filter(|id| !declared.contains(id))
        .collect();
    dangling.sort_unstable();

    let mut msg = String::new();
    if !gaps.is_empty() {
        msg.push_str(&format!(
            "\nGAP — requirement(s) with verification=\"test\" and no `// VERIFIES:` citation:\n  {}\n",
            gaps.join("\n  ")
        ));
    }
    if !dangling.is_empty() {
        msg.push_str(
            "\nDANGLING — `// VERIFIES:` citing id(s) not in docs/project/requirements.toml:\n",
        );
        for id in dangling {
            msg.push_str(&format!("  {} (at {})\n", id, citations[id].join(", ")));
        }
    }
    assert!(msg.is_empty(), "requirements traceability failed:{msg}");
}
