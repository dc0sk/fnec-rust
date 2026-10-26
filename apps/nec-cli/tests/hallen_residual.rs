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

// ---------------------------------------------------------------------------
// The caveat's positive side, through the production entry points (C3). No
// corpus deck reaches the 1e-5 default, so the threshold is overridden —
// `--hallen-residual-caveat` / `FNEC_HALLEN_REL_RESIDUAL_CAVEAT`, on the
// pattern of `--sin-fallback-rel-max` — and at 1e-30 every judged residual is
// above it. Each wiring site has a test here that goes red without it.
// ---------------------------------------------------------------------------

const ENV: &str = "FNEC_HALLEN_REL_RESIDUAL_CAVEAT";

fn run_raw(args: &[&str], env: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.args(args).env_remove(ENV);
    if let Some(v) = env {
        cmd.env(ENV, v);
    }
    cmd.output().expect("fnec runs")
}

fn caveats(out: &Output) -> usize {
    stderr(out)
        .lines()
        .filter(|l| l.contains("warning:") && l.contains(FND302))
        .count()
}

fn corpus(name: &str) -> String {
    root().join("corpus").join(name).display().to_string()
}

/// Once per run, single point and sweep: the five-point sweep prints one line
/// at its worst point, not five.
#[test]
fn the_caveat_fires_once_per_run_when_the_threshold_is_crossed() {
    for deck in ["dipole-freesp-51seg.nec", "frequency-sweep-dipole.nec"] {
        let d = corpus(deck);
        let out = run_raw(
            &["--exec", "cpu", "--hallen-residual-caveat", "1e-30", &d],
            None,
        );
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(caveats(&out), 1, "{deck}:\n{}", stderr(&out));
        let quiet = run_raw(&["--exec", "cpu", &d], None);
        assert_eq!(
            caveats(&quiet),
            0,
            "{deck} at the default:\n{}",
            stderr(&quiet)
        );
    }
}

/// The environment variable, and the flag over it — the precedence and the
/// refusals of `--sin-fallback-rel-max` / `FNEC_SIN_FALLBACK_REL_MAX`.
#[test]
fn the_threshold_override_follows_the_sin_fallback_precedent() {
    let d = corpus("dipole-freesp-51seg.nec");
    assert_eq!(caveats(&run_raw(&["--exec", "cpu", &d], Some("1e-30"))), 1);
    let flag_wins = run_raw(
        &["--exec", "cpu", "--hallen-residual-caveat", "1", &d],
        Some("1e-30"),
    );
    assert_eq!(caveats(&flag_wins), 0, "{}", stderr(&flag_wins));
    let bad_env = run_raw(&[&d], Some("x"));
    assert_eq!(bad_env.status.code(), Some(2), "{}", stderr(&bad_env));
    assert!(stderr(&bad_env).contains("invalid FNEC_HALLEN_REL_RESIDUAL_CAVEAT='x'"));
    let bad_flag = run_raw(&["--hallen-residual-caveat", "0", &d], None);
    assert_eq!(bad_flag.status.code(), Some(2), "{}", stderr(&bad_flag));
    assert!(stderr(&bad_flag).contains("invalid --hallen-residual-caveat value '0'"));
}

/// The probe (`fnec converge` runs one per mesh): once per probe, through the
/// environment, since a probe takes the main command's flags only in part.
#[test]
fn the_caveat_fires_once_per_probe() {
    let d = corpus("dipole-freesp-51seg.nec");
    let out = run_raw(
        &["converge", "--factors", "1,3", "--exec", "cpu", &d],
        Some("1e-30"),
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(caveats(&out), 2, "one per mesh:\n{}", stderr(&out));
    let quiet = run_raw(&["converge", "--factors", "1,3", "--exec", "cpu", &d], None);
    assert_eq!(caveats(&quiet), 0, "{}", stderr(&quiet));
}

/// The sinusoidal basis's fallback to Hallén is an f64 Hallén solve and is
/// judged; the sinusoidal projection itself is not, even at 1e-30.
#[test]
fn the_sinusoidal_fallback_is_judged_and_the_projection_is_not() {
    let d = corpus("dipole-sinusoidal-51seg.nec");
    let fallback = run_raw(
        &[
            "--solver",
            "sinusoidal",
            "--exec",
            "cpu",
            "--sin-fallback-rel-max",
            "1e-30",
            "--hallen-residual-caveat",
            "1e-30",
            &d,
        ],
        None,
    );
    common::assert_diag_mode(&stderr(&fallback), "sinusoidal->hallen(residual)");
    assert_eq!(caveats(&fallback), 1, "{}", stderr(&fallback));
    let projection = run_raw(
        &[
            "--solver",
            "sinusoidal",
            "--exec",
            "cpu",
            "--hallen-residual-caveat",
            "1e-30",
            &d,
        ],
        None,
    );
    common::assert_diag_mode(&stderr(&projection), "sinusoidal");
    assert_eq!(caveats(&projection), 0, "{}", stderr(&projection));
}

/// The pulse basis's residual is printed, not judged.
#[test]
fn the_pulse_residual_is_not_judged() {
    let d = corpus("dipole-freesp-51seg.nec");
    let out = run_raw(
        &[
            "--solver",
            "pulse",
            "--experimental-solver",
            "--hallen-residual-caveat",
            "1e-30",
            &d,
        ],
        None,
    );
    assert!(
        diag_residual(&stderr(&out), "rel_res").is_some(),
        "{}",
        stderr(&out)
    );
    assert_eq!(caveats(&out), 0, "{}", stderr(&out));
}

/// The device exclusion's positive side: a `solve_exec=gpu` point is not
/// judged, even at 1e-30. Without an adapter (CI) the point falls back to the
/// f64 CPU solve, which is judged — so the same run checks the other direction.
#[test]
fn a_device_point_is_not_judged_and_its_cpu_fallback_is() {
    let mut text = String::from("CE\nGW 1 201 0 0 -5.282 0 0 5.282 0.001\nGE 0\n");
    text.push_str("EX 0 1 101 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n");
    let deck = TempDeck::new("fnec-fnd302-gpu-override.nec", &text);
    let path = deck.path().display().to_string();
    let attempt = nec_accel::gpu_test::gpu_attempt("FND-302 device exclusion", || {
        let out = run_raw(
            &["--exec", "gpu", "--hallen-residual-caveat", "1e-30", &path],
            None,
        );
        if common::diag_field(&stderr(&out), "solve_exec") == Some("gpu") {
            Ok(out)
        } else {
            Err(stderr(&out))
        }
    });
    match attempt {
        Ok(out) => assert_eq!(caveats(&out), 0, "{}", stderr(&out)),
        Err(err) => {
            eprintln!("no GPU-resident solve on this host; the device half is dormant");
            assert_eq!(
                err.lines().filter(|l| l.contains(FND302)).count(),
                1,
                "the CPU fallback is judged:\n{err}"
            );
        }
    }
}
