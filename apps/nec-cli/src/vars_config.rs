// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Variable map loader for `--vars <file>` CLI flag.
//!
//! Accepts both JSON (`{"KEY": "value", ...}`) and TOML
//! (`KEY = "value"`) flat key-value files.  The file format is
//! detected by extension (`.json` → JSON, anything else → TOML).

use std::collections::HashMap;
use std::path::Path;

/// Error returned when a vars file cannot be read or parsed.
#[derive(Debug)]
pub struct VarsError(pub String);

impl std::fmt::Display for VarsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vars error: {}", self.0)
    }
}

/// Load a flat string-to-string variable map from `path`.
///
/// JSON files must be a top-level object with string values.
/// TOML files must be a flat table with string values.
/// The format is detected by the `.json` extension; all other extensions
/// are tried as TOML.
pub fn load_vars(path: &Path) -> Result<HashMap<String, String>, VarsError> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| VarsError(format!("cannot read '{}': {e}", path.display())))?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "json" {
        parse_json(&src, path)
    } else {
        parse_toml(&src, path)
    }
}

fn parse_json(src: &str, path: &Path) -> Result<HashMap<String, String>, VarsError> {
    // Minimal JSON object parser — no external dep needed; serde_json is only
    // in dev-deps.  We use the toml crate for TOML (already in workspace) and
    // implement a small JSON object parser here.
    //
    // For safety we parse only the subset: a flat object where all values are
    // JSON strings.  Nested objects / arrays / numbers are rejected.
    let s = src.trim();
    if !s.starts_with('{') || !s.ends_with('}') {
        return Err(VarsError(format!(
            "'{}': JSON vars file must be a top-level object",
            path.display()
        )));
    }

    // Use toml Value as a JSON stand-in via a tiny hand-rolled parser.
    // Actually — serde_json is a dev-dep only.  Use a simple regex-free
    // approach: delegate to the toml crate after stripping JSON-ness.
    // The cleanest safe option here is to parse the JSON manually for this
    // restricted subset.
    parse_json_object(s, path)
}

fn parse_json_object(s: &str, path: &Path) -> Result<HashMap<String, String>, VarsError> {
    let inner = s[1..s.len() - 1].trim();
    let mut map = HashMap::new();

    if inner.is_empty() {
        return Ok(map);
    }

    // Split on commas (top-level only) and parse "key": "value" pairs.
    // This is intentionally minimal — handles the flat-string-map use case.
    for raw_pair in split_json_top_level_commas(inner) {
        let pair = raw_pair.trim();
        if pair.is_empty() {
            continue;
        }
        let colon = pair.find(':').ok_or_else(|| {
            VarsError(format!(
                "'{}': malformed JSON pair (missing ':'): {pair}",
                path.display()
            ))
        })?;
        let raw_key = pair[..colon].trim();
        let raw_val = pair[colon + 1..].trim();
        let key = unquote_json_string(raw_key, path)?;
        let val = unquote_json_string(raw_val, path)?;
        map.insert(key, val);
    }

    Ok(map)
}

fn split_json_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut in_string = false;
    let mut escape = false;

    for (idx, ch) in &chars {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if *ch == '\\' {
                escape = true;
            } else if *ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&s[start..*idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

fn unquote_json_string(s: &str, path: &Path) -> Result<String, VarsError> {
    let s = s.trim();
    if !s.starts_with('"') || !s.ends_with('"') || s.len() < 2 {
        return Err(VarsError(format!(
            "'{}': expected a JSON string (got: {s})",
            path.display()
        )));
    }
    // Unescape common escape sequences.
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

fn parse_toml(src: &str, path: &Path) -> Result<HashMap<String, String>, VarsError> {
    let table: toml::Table = toml::from_str(src)
        .map_err(|e| VarsError(format!("'{}': TOML parse error: {e}", path.display())))?;

    let mut map = HashMap::new();
    for (k, v) in table {
        match v {
            toml::Value::String(s) => {
                map.insert(k, s);
            }
            toml::Value::Integer(i) => {
                map.insert(k, i.to_string());
            }
            toml::Value::Float(f) => {
                map.insert(k, format!("{f}"));
            }
            other => {
                return Err(VarsError(format!(
                    "'{}': variable '{k}' has unsupported type {} — vars must be strings or numbers",
                    path.display(),
                    other.type_str()
                )));
            }
        }
    }
    Ok(map)
}
