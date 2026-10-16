// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PAR-011: `--output-format nec2d` prints the layout NEC-2's kernel prints.
//!
//! 4nec2 rejects any output but its kernel's NEC-2D layout — nec2c's valid output
//! for the same deck draws "NEC error ?" — and reads its results back by position
//! (the evidence memo's ablations). So for every fixture pair 4nec2 and its kernel
//! wrote (`tests/fixtures/par011/`), fnec's output for the same `.inp` has the
//! kernel's layout, line for line:
//! - the same number of lines;
//! - each line's text, with every number blanked, byte for byte the same;
//! - each number ending in the same column (Fortran right-justifies a field, so
//!   the column a number ends in is its format's, whatever its value);
//! - every integer the same: NEC-2's bookkeeping (segment and tag numbers,
//!   connection data, the card echo's integer fields) is not physics;
//! - every number the same shape: its decimals and its exponent's width are the
//!   FORMAT descriptor's, whatever the value.
//!
//! The numbers themselves are fnec's own and are not compared here: fnec's
//! physics is gated against converged nec2c elsewhere, and this gate proves only
//! that the layout carries them where 4nec2 reads. fnec's one added line — the
//! note before `RUN TIME` naming fnec — is the only line the kernel did not print
//! (4nec2 tolerates free text there: the insertion ablation).
//!
//! The decks the NEC-2D output cannot print yet are refused before solving, with
//! a non-zero exit, and that set is pinned too.

use std::path::PathBuf;
use std::process::Command;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/par011")
}

/// The spans of a line's numbers: a digit, or a sign or point before one, then
/// digits, points and an exponent; a sign after a complete number starts the next
/// (NEC-2's `1PE12.5` fields run together when negative). A run of `*` is an
/// overflowed field.
fn numbers(line: &str) -> Vec<(usize, usize)> {
    let b = line.as_bytes();
    let digit = |i: usize| i < b.len() && b[i].is_ascii_digit();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let starts = digit(i)
            || (matches!(b[i], b'-' | b'+' | b'.')
                && (digit(i + 1) || (b[i] != b'.' && b.get(i + 1) == Some(&b'.') && digit(i + 2))));
        if b[i] == b'*' {
            let s = i;
            while i < b.len() && b[i] == b'*' {
                i += 1;
            }
            out.push((s, i));
            continue;
        }
        if !starts {
            i += 1;
            continue;
        }
        let s = i;
        if matches!(b[i], b'-' | b'+') {
            i += 1;
        }
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        if i < b.len()
            && b[i] == b'E'
            && i + 1 < b.len()
            && matches!(b[i + 1], b'+' | b'-')
            && digit(i + 2)
        {
            i += 2;
            while digit(i) {
                i += 1;
            }
        }
        out.push((s, i));
    }
    out
}

/// A line with its numbers blanked, and the column each number ends in.
fn skeleton(line: &str) -> (String, Vec<usize>) {
    let mut chars: Vec<u8> = line.as_bytes().to_vec();
    let spans = numbers(line);
    for &(s, e) in &spans {
        for c in &mut chars[s..e] {
            *c = b' ';
        }
    }
    let text = String::from_utf8(chars)
        .expect("ascii")
        .trim_end()
        .to_string();
    (text, spans.iter().map(|&(_, e)| e).collect())
}

fn lines(text: &str) -> Vec<String> {
    text.replace('\r', "").lines().map(str::to_string).collect()
}

#[test]
fn the_nec2d_output_has_the_kernels_layout() {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .expect("fixtures")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "inp").then(|| p.file_stem()?.to_str().map(String::from))?
        })
        .collect();
    names.sort();
    assert_eq!(names.len(), 15, "{names:?}");
    let mut refused = Vec::new();
    let mut compared = 0;
    for name in &names {
        let inp = fixtures().join(format!("{name}.inp"));
        let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--exec", "cpu", "--output-format", "nec2d"])
            .arg(&inp)
            .output()
            .expect("runs fnec");
        if !out.status.success() {
            refused.push(name.clone());
            assert!(
                out.stdout.is_empty(),
                "{name}: a refused deck printed a partial report"
            );
            continue;
        }
        let ours = lines(&String::from_utf8_lossy(&out.stdout));
        let ours: Vec<String> = ours
            .into_iter()
            .filter(|l| !l.starts_with(" SOLVED BY FNEC"))
            .collect();
        let kernel =
            lines(&std::fs::read_to_string(fixtures().join(format!("{name}.out"))).unwrap());
        for (n, (a, k)) in ours.iter().zip(&kernel).enumerate() {
            let (sa, ea) = skeleton(a);
            let (sk, ek) = skeleton(k);
            assert!(
                sa == sk && ea == ek,
                "{name}, line {}:\n  fnec:   {a:?}\n  kernel: {k:?}",
                n + 1
            );
            // NEC-2's bookkeeping is not physics: segment and tag numbers,
            // connection data, the card echo's integer fields, segment counts.
            // Every integer is the kernel's own.
            let ints = |l: &str| -> Vec<String> {
                numbers(l)
                    .iter()
                    .map(|&(s, e)| l[s..e].to_string())
                    .filter(|t| !t.contains('.') && !t.contains('*'))
                    .collect()
            };
            assert_eq!(
                ints(a),
                ints(k),
                "{name}, line {}: integers\n  fnec:   {a:?}\n  kernel: {k:?}",
                n + 1
            );
            // And each number has its format's shape: the digits after the point
            // and the exponent's width are the descriptor's (F9.4, 1PE12.5), not
            // the value's, so they match the kernel's wherever the values differ.
            let shapes = |l: &str| -> Vec<(usize, usize)> {
                numbers(l)
                    .iter()
                    .map(|&(s, e)| {
                        let t = &l[s..e];
                        let (mantissa, exponent) = t.split_once('E').unwrap_or((t, ""));
                        let decimals = mantissa
                            .split_once('.')
                            .map_or(usize::MAX, |(_, d)| d.len());
                        (decimals, exponent.len())
                    })
                    .collect()
            };
            assert_eq!(
                shapes(a),
                shapes(k),
                "{name}, line {}: number shapes\n  fnec:   {a:?}\n  kernel: {k:?}",
                n + 1
            );
        }
        assert_eq!(
            ours.len(),
            kernel.len(),
            "{name}: {} lines, the kernel printed {}",
            ours.len(),
            kernel.len()
        );
        compared += 1;
    }
    // What the NEC-2D output cannot print yet, refused before solving: a surface
    // patch (refused by the parser), a network and a line (their loss is not
    // separated), Sommerfeld ground, two FR/XQ groups, and the kernel's own error
    // deck (a source on a segment that does not exist).
    assert_eq!(
        refused,
        [
            "err_seg",
            "ex3_gn2_sy",
            "nt_net",
            "sp_patch",
            "tl_pair",
            "two_fr"
        ],
        "refused"
    );
    assert_eq!(compared, 9);
}

/// The tokenizer's own cases: fields that run together, an exponent sign, a
/// sign that starts the next field, and an overflow.
#[test]
fn numbers_are_split_as_fortran_prints_them() {
    let s = "     1    11 1.00000E+00 0.00000E+00 9.46615E-03-5.49233E-03 ***";
    let spans: Vec<&str> = numbers(s).iter().map(|&(a, b)| &s[a..b]).collect();
    assert_eq!(
        spans,
        [
            "1",
            "11",
            "1.00000E+00",
            "0.00000E+00",
            "9.46615E-03",
            "-5.49233E-03",
            "***"
        ]
    );
    let (text, ends) = skeleton("   TAG  -12.50  X");
    assert_eq!(text, "   TAG          X");
    assert_eq!(ends, [14]);
}

/// A section of fnec's text report: the rows after its header line, up to the
/// next blank line.
fn text_section<'a>(report: &'a str, name: &str) -> Vec<Vec<&'a str>> {
    let mut rows = Vec::new();
    let mut lines = report.lines().skip_while(|l| *l != name).skip(1);
    for l in lines.by_ref() {
        if l.trim().is_empty() {
            break;
        }
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.first().is_some_and(|t| t.parse::<f64>().is_ok()) {
            rows.push(f);
        }
    }
    rows
}

/// The data rows of a NEC-2D section: the lines after `header` that start with a
/// number, up to the next blank line after the first of them.
fn nec2d_section(report: &str, header: &str) -> Vec<Vec<f64>> {
    let mut rows = Vec::new();
    let mut started = false;
    for l in report.lines().skip_while(|l| !l.contains(header)).skip(1) {
        let spans = numbers(l);
        let numeric = !spans.is_empty() && l[..spans[0].0].trim().is_empty();
        if numeric {
            started = true;
            rows.push(
                spans
                    .iter()
                    .map(|&(a, b)| l[a..b].parse().unwrap_or(f64::NAN))
                    .collect(),
            );
        } else if started && l.trim().is_empty() {
            break;
        }
    }
    rows
}

fn close(a: f64, b: f64, rel: f64, abs: f64) -> bool {
    (a - b).abs() <= abs.max(rel * a.abs().max(b.abs()))
}

/// The writer transcribes; it computes no answer of its own. For each deck, every
/// feedpoint impedance, segment current and pattern gain in the NEC-2D output is
/// fnec's own text report's, to the NEC-2D field's print precision. Both sides
/// are fnec — this proves the copy, not the physics, which is gated against
/// nec2c elsewhere.
#[test]
fn the_nec2d_output_carries_fnecs_own_numbers() {
    for name in [
        "dip_free", "dip_ld", "dip_gn0", "two_src", "gm_gx", "dip_pec",
    ] {
        let inp = fixtures().join(format!("{name}.inp"));
        let run = |format: &str| {
            let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
                .args(["--exec", "cpu", "--output-format", format])
                .arg(&inp)
                .output()
                .expect("runs fnec");
            assert!(out.status.success(), "{name} {format}");
            String::from_utf8(out.stdout).unwrap()
        };
        let (text, nec2d) = (run("text"), run("nec2d"));
        // Feedpoints: Z as E12.5 (six significant digits).
        let feeds = text_section(&text, "TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM");
        let inputs = nec2d_section(&nec2d, "NO.   NO.    REAL");
        assert_eq!(feeds.len(), inputs.len(), "{name}: feedpoints");
        assert!(!feeds.is_empty(), "{name}: no feedpoints");
        for (t, n) in feeds.iter().zip(&inputs) {
            let (zr, zi): (f64, f64) = (t[6].parse().unwrap(), t[7].parse().unwrap());
            assert_eq!(t[0].parse::<f64>().unwrap(), n[0], "{name}: tag");
            assert!(
                close(zr, n[6], 1e-5, 1e-9) && close(zi, n[7], 1e-5, 1e-9),
                "{name}: Z {zr} {zi} vs {n:?}"
            );
        }
        // Currents: E12.4 (five significant digits), in deck-segment order.
        let currents = text_section(&text, "TAG SEG I_RE I_IM I_MAG I_PHASE");
        let rows = nec2d_section(&nec2d, "NO.   NO.     X");
        assert_eq!(currents.len(), rows.len(), "{name}: currents");
        for (t, n) in currents.iter().zip(&rows) {
            let (re, im): (f64, f64) = (t[2].parse().unwrap(), t[3].parse().unwrap());
            assert!(
                close(re, n[6], 1e-4, 1e-12) && close(im, n[7], 1e-4, 1e-12),
                "{name}: I {re} {im} vs {n:?}"
            );
        }
        // Pattern: gains as F8.2.
        let pattern = text_section(&text, "THETA PHI GAIN_DB GAIN_V_DB GAIN_H_DB AXIAL_RATIO");
        let prows = nec2d_section(&nec2d, "DEGREES  DEGREES");
        assert_eq!(pattern.len(), prows.len(), "{name}: pattern");
        for (t, n) in pattern.iter().zip(&prows) {
            let g = |k: usize| t[k].parse::<f64>().unwrap();
            for (ours, theirs) in [(g(3), n[2]), (g(4), n[3]), (g(2), n[4])] {
                assert!(
                    (ours - theirs).abs() <= 0.0051,
                    "{name}: gain {ours} vs {theirs} at {} {}",
                    t[0],
                    t[1]
                );
            }
        }
    }
}
