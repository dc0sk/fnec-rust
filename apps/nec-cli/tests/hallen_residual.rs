// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-302 and FND-306: the `rel_res` on the diag line describes the system the
// route solved, and says `n/a` where the route computes none.
//
// FND-302: on a bent conductor path the Hallén solve adds a corner term to its
// tangential rows; the residual left it out, so it measured the corner
// correction, not the solve — the 3× U read `rel_res = 0.678` and the corpus
// split-V 0.96 while both solves were exact to ~1e-10. FND-306: a route without a
// residual printed `rel_res=0.000000e0`, which reads as a perfect solve.
//
// Every route that computes a residual has a control here — a direct solve whose
// residual must be tiny — so a residual computed against the wrong system for any
// one of them fails a test, not only the bent one.

use std::process::{Command, Output};

mod common;
use common::{diag_residual, TempDeck};

const FND302: &str = "FND-302";

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fnec(args: &[&str], deck: &std::path::Path) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(deck)
        .output()
        .expect("fnec runs");
    assert!(
        out.status.success(),
        "fnec {args:?} {} failed:\n{}",
        deck.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `(R, X)` of the first FEEDPOINTS row.
fn z_in(out: &Output) -> (f64, f64) {
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == "FEEDPOINTS")
        .expect("FEEDPOINTS");
    let c: Vec<f64> = lines[at + 2]
        .split_whitespace()
        .map(|v| v.parse().expect("number"))
        .collect();
    (c[6], c[7])
}

/// The deck's `rel_res`, asserting the route computed one, that it is below
/// `bound`, and that no FND-302 caveat was printed.
fn assert_tiny_residual(err: &str, what: &str, bound: f64) -> f64 {
    let rel = diag_residual(err, "rel_res")
        .unwrap_or_else(|| panic!("{what}: the route must compute a residual:\n{err}"));
    assert!(
        rel < bound,
        "{what}: rel_res {rel:e} not below {bound:e}:\n{err}"
    );
    assert!(
        diag_residual(err, "abs_res").is_some(),
        "{what}: abs_res must be a number where rel_res is:\n{err}"
    );
    assert!(
        !err.contains(FND302),
        "{what}: no FND-302 caveat expected:\n{err}"
    );
    rel
}

/// The 3× U (committed fixture with its provenance): the conductor-path route
/// with sections and both bends. Its residual is the solve's, ~1e-11, and its
/// impedance is nec2c's at the same mesh (2.2719 − j448.82) within 1.5 % / 1 %.
#[test]
fn the_bent_u_reports_the_residual_of_the_system_it_solved() {
    let deck = root().join("apps/nec-cli/tests/fixtures/fnd302/u-one-segment-negative-r-3x.nec");
    let out = fnec(&[], &deck);
    let err = stderr(&out);
    common::assert_diag_field(&err, "route", "paths");
    assert_tiny_residual(&err, "3x U", 1e-8);
    let (r, x) = z_in(&out);
    let (nr, nx) = (2.2719, -448.82);
    assert!(
        ((r - nr) / nr).abs() < 0.015 && ((x - nx) / nx).abs() < 0.01,
        "3x U: {r} {x:+} against nec2c {nr} {nx:+}"
    );
}

/// The corpus split-V: a valid deck that read `rel_res = 0.96` before FND-302,
/// so any guard on the old number would have tripped on it.
#[test]
fn the_corpus_split_v_reports_a_tiny_residual() {
    let out = fnec(
        &[],
        &root().join("corpus/split-v-conductor-path-freesp.nec"),
    );
    let err = stderr(&out);
    common::assert_diag_field(&err, "route", "paths");
    assert_tiny_residual(&err, "split-V", 1e-8);
}

/// An inverted-V fed at the apex: one bend, on the paths basis.
#[test]
fn an_inverted_v_reports_a_tiny_residual() {
    let deck = TempDeck::new(
        "fnec-fnd302-invv.nec",
        "CE\nGW 1 11 0 0 10 -3.5 0 6.5 .001\nGW 2 11 0 0 10 3.5 0 6.5 .001\nGE 0\n\
         EX 0 1 1 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    let err = stderr(&fnec(&[], &deck));
    common::assert_diag_field(&err, "route", "paths");
    assert_tiny_residual(&err, "inverted-V", 1e-8);
}

/// Negative controls, one per f64 route whose solved system differs: the plain
/// merged-conductor basis, stamped loads, and the network superposition. Each
/// must read tiny; a residual against the wrong system (loads unstamped, the
/// network's unit-gap responses not superposed) would not. `--exec cpu`, so a GPU
/// host does not move these onto the f32 device solve.
#[test]
fn every_route_with_a_residual_reports_a_tiny_one() {
    for (what, deck) in [
        ("plain dipole", "corpus/dipole-freesp-51seg.nec"),
        ("loaded dipole", "corpus/dipole-ld-loaded-51seg.nec"),
        ("TL network", "corpus/tl-two-dipoles-linked.nec"),
        ("NT network", "corpus/dipole-nt-freesp-51seg.nec"),
    ] {
        let err = stderr(&fnec(&["--exec", "cpu"], &root().join(deck)));
        assert_tiny_residual(&err, what, 1e-8);
    }
}

/// The sinusoidal basis is a Galerkin projection tested at pulse points, so its
/// collocation residual is the projection's (6.6e-4 here), judged by
/// `--sin-fallback-rel-max`, not by the FND-302 threshold: a number, below the
/// fallback bound, and no FND-302 caveat. A residual of the wrong system (no
/// homogeneous term, say) is O(1) and fails the bound.
#[test]
fn the_sinusoidal_residual_is_its_projection_s_and_earns_no_caveat() {
    let out = fnec(
        &["--solver", "sinusoidal", "--exec", "cpu"],
        &root().join("corpus/dipole-sinusoidal-51seg.nec"),
    );
    let err = stderr(&out);
    common::assert_diag_mode(&err, "sinusoidal");
    let rel = diag_residual(&err, "rel_res").expect("sinusoidal computes a residual");
    assert_eq!(diag_residual(&err, "sin_rel_res"), Some(rel), "{err}");
    assert!(
        rel > 1e-5 && rel < 2e-3,
        "sinusoidal projection residual {rel:e}:\n{err}"
    );
    assert!(
        !err.contains(FND302),
        "no FND-302 caveat on the sinusoidal basis:\n{err}"
    );
}

/// The 5-element yagi meshed 3× (153 segments per element): on the f64 CPU
/// solve its residual is ~3e-8; on the GPU-resident f32 solve it is 1.43e-5 —
/// above the f64 threshold, correct to f32, and inside the device's own gate
/// (1e-4, above which it declines to the CPU). The f64 caveat must not fire on
/// the device's answer. The device half needs an adapter: CI-dormant without one.
#[test]
fn a_device_solve_is_not_judged_by_the_f64_threshold() {
    let mut text = String::from("CE\n");
    for (t, x, h) in [
        (1, -3.80, 5.388),
        (2, 0.0, 5.282),
        (3, 2.64, 5.176),
        (4, 5.28, 5.092),
        (5, 8.45, 4.965),
    ] {
        text.push_str(&format!("GW {t} 153 {x} 0 -{h} {x} 0 {h} 0.001\n"));
    }
    text.push_str("GE 0\nEX 0 2 77 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n");
    let deck = TempDeck::new("fnec-fnd302-yagi3x.nec", &text);
    let cpu = stderr(&fnec(&["--exec", "cpu"], &deck));
    assert_tiny_residual(&cpu, "yagi 3x, cpu", 1e-7);
    let gpu = stderr(&fnec(&["--exec", "gpu"], &deck));
    if common::diag_field(&gpu, "solve_exec") != Some("gpu") {
        eprintln!("no GPU-resident solve on this host; the device half is dormant");
        return;
    }
    let rel = diag_residual(&gpu, "rel_res").expect("the resident solve computes a residual");
    assert!(rel < 1e-4, "inside the device's own gate: {rel:e}");
    assert!(
        !gpu.contains(FND302),
        "no f64 caveat on an f32 solve:\n{gpu}"
    );
}

/// A bent path carrying a lumped load: the paths basis stamps the load columns
/// into the matrix in place, and the residual must be against that matrix.
#[test]
fn a_loaded_bent_path_reports_a_tiny_residual() {
    let deck = TempDeck::new(
        "fnec-fnd302-bent-ld.nec",
        "CE\nGW 1 21 0 0 0 0 0 5 .001\nGW 2 21 0 0 5 5 0 5 .001\nGE 0\n\
         LD 0 2 11 11 50 1e-6 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    let err = stderr(&fnec(&[], &deck));
    common::assert_diag_field(&err, "route", "paths");
    common::assert_diag_field(&err, "loads", "deck");
    assert_tiny_residual(&err, "loaded L", 1e-8);
}

/// FND-306: routes that compute no residual print `n/a`, never `0`, and earn
/// no FND-302 caveat: the PEC-contact image, the section graph, a current
/// source, a plane wave, and the MPIE.
#[test]
fn routes_without_a_residual_print_not_computed() {
    for (what, deck, args) in [
        (
            "PEC-contact monopole",
            "corpus/monopole-pec-26seg.nec",
            &[][..],
        ),
        (
            "current source",
            "corpus/dipole-ex4-freesp-51seg.nec",
            &[][..],
        ),
        ("plane wave", "corpus/dipole-ex1-freesp-51seg.nec", &[][..]),
        (
            "mpie",
            "corpus/dipole-freesp-51seg.nec",
            &["--solver", "mpie"][..],
        ),
    ] {
        let err = stderr(&fnec(args, &root().join(deck)));
        common::assert_diag_field_not_computed(&err, "abs_res");
        common::assert_diag_field_not_computed(&err, "rel_res");
        common::assert_diag_field_not_computed(&err, "sin_rel_res");
        assert!(!err.contains(FND302), "{what}: no FND-302 caveat:\n{err}");
    }
    // The section graph: a closed square loop.
    let deck = TempDeck::new(
        "fnec-fnd306-loop.nec",
        "CE\nGW 1 9 0 0 0 1.8 0 0 .001\nGW 2 9 1.8 0 0 1.8 1.8 0 .001\n\
         GW 3 9 1.8 1.8 0 0 1.8 0 .001\nGW 4 9 0 1.8 0 0 0 0 .001\nGE 0\n\
         EX 0 1 5 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    let err = stderr(&fnec(&[], &deck));
    common::assert_diag_field_not_computed(&err, "rel_res");
    assert!(!err.contains(FND302), "loop: no FND-302 caveat:\n{err}");
}

/// The bench records carry the same rule: `n/a` in CSV, `null` in JSON.
#[test]
fn bench_records_say_not_computed_rather_than_zero() {
    let deck = root().join("corpus/monopole-pec-26seg.nec");
    let json = stderr(&fnec(&["--bench", "--bench-format", "json"], &deck));
    let line = json
        .lines()
        .find(|l| l.starts_with("bench_json:"))
        .expect("bench_json line");
    for key in ["abs_res", "rel_res", "sin_rel_res"] {
        assert!(
            line.contains(&format!("\"{key}\":null")),
            "{key} null in {line}"
        );
    }
    let csv = stderr(&fnec(&["--bench", "--bench-format", "csv"], &deck));
    let row = csv
        .lines()
        .filter(|l| l.starts_with("bench_csv:"))
        .nth(1)
        .expect("bench_csv data row");
    let f: Vec<&str> = row.split(',').collect();
    // abs_res, rel_res, diag_spread, sin_rel_res are the last four columns.
    let n = f.len();
    assert_eq!(
        (f[n - 4], f[n - 3], f[n - 1]),
        ("n/a", "n/a", "n/a"),
        "{row}"
    );
    assert!(
        f[n - 2].parse::<f64>().is_ok(),
        "diag_spread is computed: {row}"
    );
}
