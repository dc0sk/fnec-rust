// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Variable-substitution engine for NEC deck templates.
//!
//! Tokens of the form `$VARNAME` (ASCII alphanumeric characters and
//! underscores, case-sensitive) are replaced with their values from
//! the supplied variable map.  A `$$` escape sequence produces a
//! literal `$` without attempting substitution.
//!
//! # Example
//!
//! ```
//! use nec_parser::template::{substitute, TemplateError};
//! use std::collections::HashMap;
//!
//! let mut vars = HashMap::new();
//! vars.insert("HALF_LEN".to_string(), "5.282".to_string());
//! vars.insert("FREQ".to_string(), "14.2".to_string());
//!
//! let tmpl = "GW 1 51 0 0 -$HALF_LEN 0 0 $HALF_LEN 0.001\nFR 0 1 0 0 $FREQ 0\n";
//! let result = substitute(tmpl, &vars).unwrap();
//! assert!(result.contains("5.282"));
//! assert!(result.contains("14.2"));
//! ```

use std::collections::HashMap;

/// Error returned when a template token references an undefined variable.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateError {
    /// The variable name that was referenced but not found in the map.
    pub undefined_var: String,
    /// 1-based line number where the undefined token appeared.
    pub line: usize,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}: undefined template variable '${}'; provide it via --vars",
            self.line, self.undefined_var
        )
    }
}

/// Substitute `$VARNAME` tokens in `input` using the values in `vars`.
///
/// - `$$` is replaced with a literal `$`.
/// - Variable names consist of ASCII letters, digits, and underscores;
///   the longest match starting immediately after `$` is used.
/// - Returns [`TemplateError`] on the first undefined variable encountered.
pub fn substitute(input: &str, vars: &HashMap<String, String>) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut line = 1usize;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '\n' {
            out.push(ch);
            line += 1;
            i += 1;
            continue;
        }

        if ch != '$' {
            out.push(ch);
            i += 1;
            continue;
        }

        // ch == '$'
        i += 1;

        // $$ escape → literal '$'
        if i < chars.len() && chars[i] == '$' {
            out.push('$');
            i += 1;
            continue;
        }

        // Collect identifier characters (letters, digits, '_').
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
        }

        let name: String = chars[start..i].iter().collect();

        if name.is_empty() {
            // Bare '$' not followed by identifier — pass through literally.
            out.push('$');
            continue;
        }

        match vars.get(&name) {
            Some(val) => out.push_str(val),
            None => {
                return Err(TemplateError {
                    undefined_var: name,
                    line,
                })
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn single_token_replaced() {
        let vars = map(&[("LEN", "5.282")]);
        assert_eq!(substitute("$LEN", &vars).unwrap(), "5.282");
    }

    #[test]
    fn multiple_tokens_in_line() {
        let vars = map(&[("A", "1.0"), ("B", "2.0")]);
        assert_eq!(substitute("$A $B", &vars).unwrap(), "1.0 2.0");
    }

    #[test]
    fn dollar_dollar_escape() {
        let vars = map(&[]);
        assert_eq!(substitute("$$", &vars).unwrap(), "$");
    }

    #[test]
    fn bare_dollar_passes_through() {
        let vars = map(&[]);
        assert_eq!(substitute("$ ", &vars).unwrap(), "$ ");
    }

    #[test]
    fn undefined_variable_returns_error() {
        let vars = map(&[]);
        let err = substitute("$MISSING", &vars).unwrap_err();
        assert_eq!(err.undefined_var, "MISSING");
        assert_eq!(err.line, 1);
    }

    #[test]
    fn error_reports_correct_line_number() {
        let vars = map(&[("A", "x")]);
        let err = substitute("$A\n$MISSING\n", &vars).unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn multiline_substitution() {
        let vars = map(&[("HALF", "5.282"), ("FREQ", "14.2")]);
        let tmpl = "GW 1 51 0 0 -$HALF 0 0 $HALF 0.001\nFR 0 1 0 0 $FREQ 0\n";
        let result = substitute(tmpl, &vars).unwrap();
        assert!(result.contains("-5.282"));
        assert!(result.contains("14.2"));
    }
}
