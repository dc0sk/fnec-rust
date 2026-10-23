// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Contract tests for `fnec converge`, the convergence-study helper (GAP-012,
//! BL-IMPR-017): a deck solved at increasing segmentation, every feedpoint's
//! impedance per mesh, and whether its series has settled.
//!
//! The oracle for the refinement is fnec itself on a hand-written finer deck,
//! and `scripts/nec2c-fine-mesh.py`, the script that captures the corpus's
//! converged nec2c references: the two mapping rules are a twin on purpose (the
//! script writes text for nec2c), and C8 holds them to one answer on every
//! converged corpus deck. Impedances are compared in full precision, through
//! both commands' JSON, to 1e-9 Ω. The oracle for the verdict is the arithmetic
//! recomputed from the meshes. A series that does not settle is a result, exit 0.
//!
//! The tests run `fnec converge` through the binary, as a user does.

// VERIFIES: FR-010

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

mod common;

/// Two solves of the same deck by the same binary: the same doubles (1e-9 Ω is
/// the design's bound).
const SAME: f64 = 1e-9;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A deck written to the temp dir under a name unique to this test binary's run.
fn temp_deck(prefix: &str, body: &str) -> common::TempDeck {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let n = RUN.fetch_add(1, Ordering::Relaxed);
    common::TempDeck::new(
        &format!("fnec-converge-{prefix}-{}-{n}.nec", std::process::id()),
        body,
    )
}

fn fnec(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .output()
        .expect("runs fnec")
}

fn converge(deck: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["converge"];
    args.extend_from_slice(extra);
    args.push(deck.to_str().expect("utf-8 path"));
    fnec(&args)
}

fn text(out: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The study as JSON; the run must succeed.
fn study(deck: &Path, extra: &[&str]) -> Value {
    let mut args = extra.to_vec();
    args.extend_from_slice(&["--output-format", "json"]);
    let out = converge(deck, &args);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{}: {stderr}", deck.display());
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"))
}

/// Row `row`'s impedance at factor `factor`, from a JSON study.
fn z_at(v: &Value, row: usize, factor: u64) -> (f64, f64) {
    let m = v["rows"][row]["meshes"]
        .as_array()
        .unwrap_or_else(|| panic!("no row {row}: {v}"))
        .iter()
        .find(|m| m["factor"] == factor)
        .unwrap_or_else(|| panic!("no {factor}x mesh in row {row}: {v}"));
    (m["z_re"].as_f64().unwrap(), m["z_im"].as_f64().unwrap())
}

/// The value after `key ` on the first line starting with it.
fn field<'a>(stdout: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key} ");
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(prefix.as_str()))
        .map(str::trim)
}

/// Every `MESH` line as `(factor, segments, route)`.
fn mesh_lines(stdout: &str) -> Vec<(u32, usize, String)> {
    stdout
        .lines()
        .filter(|l| l.starts_with("MESH "))
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            assert_eq!(
                (f[2], f[4], f[6], f[8]),
                ("SEGMENTS", "Z_RE", "Z_IM", "ROUTE"),
                "{l}"
            );
            (
                f[1].parse().unwrap(),
                f[3].parse().unwrap(),
                f[9].to_string(),
            )
        })
        .collect()
}

/// The main command's feedpoint impedance per frequency (its JSON carries the
/// first source of each), in full precision.
fn main_z(deck: &Path) -> Vec<(f64, f64)> {
    let out = fnec(&["--output-format", "json", deck.to_str().unwrap()]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| (p["z_re"].as_f64().unwrap(), p["z_im"].as_f64().unwrap()))
        .collect()
}

/// The main command's every feedpoint row (every frequency, every source) from
/// its text `FEEDPOINTS` tables, printed to six decimals.
fn main_rows_text(deck: &Path) -> Vec<(f64, f64)> {
    let out = fnec(&[deck.to_str().unwrap()]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in stdout.lines() {
        if line.starts_with("FEEDPOINTS") {
            in_table = true;
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let numeric = f.len() == 8 && f[0].parse::<u32>().is_ok() && f[1].parse::<u32>().is_ok();
        if in_table && numeric {
            rows.push((f[6].parse().unwrap(), f[7].parse().unwrap()));
        } else if in_table && !rows.is_empty() && f.is_empty() {
            in_table = false;
        }
    }
    assert!(!rows.is_empty(), "no feedpoint rows:\n{stdout}");
    rows
}

/// `deck` meshed `s` times finer by the script that made the corpus references.
fn remeshed(deck: &Path, s: u32) -> String {
    let out = Command::new("python3")
        .arg(root().join("scripts/nec2c-fine-mesh.py"))
        .arg(deck)
        .arg(s.to_string())
        .output()
        .expect("python3 runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn same(a: (f64, f64), b: (f64, f64), tol: f64, what: &str) {
    assert!(
        (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol,
        "{what}: {a:?} vs {b:?}"
    );
}

const DIPOLE: &str = "corpus/dipole-freesp-51seg.nec";

// ---------------------------------------------------------------------------
// C1: the worked example — three meshes, the deck itself at 1×, the script's
// mesh at 3×, the verdict recomputed from the series.
// ---------------------------------------------------------------------------

#[test]
fn the_worked_example_settles_and_matches_the_main_command() {
    let deck = root().join(DIPOLE);
    let out = converge(&deck, &[]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}\n{stdout}");
    assert!(stdout.starts_with("CONVERGENCE_STUDY\n"), "{stdout}");
    assert_eq!(field(&stdout, "SOLVER"), Some("hallen"));
    assert_eq!(field(&stdout, "FACTORS"), Some("1,3,9"));
    assert_eq!(field(&stdout, "ENDS"), Some("keep"));
    assert_eq!(field(&stdout, "TOL_OHM"), Some("0.500000 TOL_PCT 1.000000"));
    assert_eq!(
        field(&stdout, "ROW"),
        Some("1 FREQ_MHZ 14.200000 TAG 1 SEG 26")
    );
    assert_eq!(
        field(&stdout, "REF"),
        Some("EX 1 26 -> 1x 26, 3x 77, 9x 230"),
        "{stdout}"
    );
    let m = mesh_lines(&stdout);
    assert_eq!(
        m.iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(),
        [(1, 51), (3, 153), (9, 459)],
        "{stdout}"
    );
    assert!(m.iter().all(|x| x.2 == "plain"), "{stdout}");
    assert_eq!(field(&stdout, "CLASS"), Some("settling"));
    assert_eq!(field(&stdout, "SETTLED"), Some("true"));
    assert_eq!(field(&stdout, "ALL_SETTLED"), Some("true"));
    // The study is stdout; each mesh's info and diag lines go to stderr under
    // its own prefix.
    assert!(stderr.contains("mesh 9x: info: 459 segments"), "{stderr}");
    assert!(stderr.contains("mesh 3x: diag: mode=hallen"), "{stderr}");
    assert!(!stderr.contains("warning: converge"), "{stderr}");

    let v = study(&deck, &[]);
    // 1×: the deck itself, the main command's doubles.
    same(
        z_at(&v, 0, 1),
        main_z(&deck)[0],
        SAME,
        "1x vs the main command",
    );
    // 3×: the script's mesh.
    let fine = temp_deck("c1-fine", &remeshed(&deck, 3));
    same(
        z_at(&v, 0, 3),
        main_z(&fine)[0],
        SAME,
        "3x vs the script's deck",
    );
    // Hallén's own series on this deck, measured 2026-10-22 on main 39e0fa5
    // (79.3315 + j45.0368, 79.5431 + j45.9348, 79.6473 + j46.3620): a settling
    // series, against nec2c's 27× EK value 79.710 + j46.719.
    same(z_at(&v, 0, 9), (79.6473, 46.3620), 1e-3, "9x as measured");

    // The verdict, recomputed from the series.
    let row = &v["rows"][0];
    let zs: Vec<(f64, f64)> = [1, 3, 9].iter().map(|&f| z_at(&v, 0, f)).collect();
    let r: Vec<f64> = zs.iter().map(|z| z.0).collect();
    let x: Vec<f64> = zs.iter().map(|z| z.1).collect();
    let steps = |v: &[f64]| [(v[1] - v[0]).abs(), (v[2] - v[1]).abs()];
    let tail = |v: &[f64]| {
        let [d1, d2] = steps(v);
        let q = d2 / d1;
        d2 * q / (1.0 - q)
    };
    for (axis, values) in [("r", &r), ("x", &x)] {
        let printed: Vec<f64> = row["steps"][axis]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_f64().unwrap())
            .collect();
        let want = steps(values);
        assert!(
            (printed[0] - want[0]).abs() < 1e-12 && (printed[1] - want[1]).abs() < 1e-12,
            "steps {axis}: {printed:?} vs {want:?}"
        );
        let t = row["tail"][axis].as_f64().unwrap();
        assert!(
            (t - tail(values)).abs() < 1e-12,
            "tail {axis}: {t} vs {}",
            tail(values)
        );
    }
    // The worked example's numbers: R steps 0.2117, 0.1042 (ratio 0.49), tail
    // 0.101; X tail 0.388; the tolerance max(0.5, 1 % of |Z| = 0.92) holds both.
    assert!((tail(&r) - 0.101).abs() < 1e-3 && (tail(&x) - 0.388).abs() < 1e-3);
    let sign = |v: &[f64]| (v[2] - v[1]).signum();
    let ex = (
        row["extrapolated"]["z_re"].as_f64().unwrap(),
        row["extrapolated"]["z_im"].as_f64().unwrap(),
    );
    same(
        ex,
        (r[2] + sign(&r) * tail(&r), x[2] + sign(&x) * tail(&x)),
        1e-12,
        "Z_EXTRAPOLATED",
    );
    assert_eq!(row["settled"], true);
    // The text prints the same numbers to six decimals.
    let printed_tail: Vec<&str> = field(&stdout, "TAIL_R")
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(printed_tail[1], "TAIL_X");
    assert_eq!(printed_tail[0], format!("{:.6}", tail(&r)));
    assert_eq!(printed_tail[2], format!("{:.6}", tail(&x)));
}

// ---------------------------------------------------------------------------
// C2: an end reference keeps its end under the default and takes the centre
// under `--ends centre`; the refined deck is the hand-written one.
// ---------------------------------------------------------------------------

#[test]
fn an_end_reference_keeps_its_end_by_default() {
    let mono = root().join("corpus/monopole-pec-26seg.nec");
    let out = converge(&mono, &["--factors", "1,3"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(
        field(&stdout, "REF"),
        Some("EX 1 1 -> 1x 1, 3x 1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("NOTE `EX` on tag 1 segment 1 touches a wire end")
            && stdout.contains("--ends centre"),
        "{stdout}"
    );
    let base = temp_deck(
        "c2-base",
        "CE\nGW 1 78 0 0 0 0 0 5.282 0.001\nGE 1\nGN 1\nEX 0 1 1 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    let keep = study(&mono, &["--factors", "1,3"]);
    same(
        z_at(&keep, 0, 3),
        main_z(&base)[0],
        SAME,
        "keep vs the base-fed 78-segment deck",
    );

    let out = converge(&mono, &["--factors", "1,3", "--ends", "centre"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(field(&stdout, "ENDS"), Some("centre"));
    assert_eq!(
        field(&stdout, "REF"),
        Some("EX 1 1 -> 1x 1, 3x 2"),
        "{stdout}"
    );
    let script = temp_deck("c2-script", &remeshed(&mono, 3));
    let centre = study(&mono, &["--factors", "1,3", "--ends", "centre"]);
    same(
        z_at(&centre, 0, 3),
        main_z(&script)[0],
        SAME,
        "centre vs the script's deck",
    );
    // The two rules are two antennas (measured 39.70 against 39.62 Ω at 3×).
    assert!(
        (z_at(&keep, 0, 3).0 - z_at(&centre, 0, 3).0).abs() > 0.02,
        "keep and centre must differ: {keep} {centre}"
    );
}

// ---------------------------------------------------------------------------
// C3: a series that does not settle is a result, exit 0 — the FND-270 dipole
// fed beside its join, whose gap moves toward the node as the mesh refines.
// ---------------------------------------------------------------------------

#[test]
fn a_series_that_does_not_settle_is_a_result() {
    let deck = temp_deck(
        "c3-node",
        "CE\nGW 1 10 0 0 -2.64 0 0 0 .001\nGW 2 10 0 0 0 0 0 2.64 .001\nGE 0\nEX 0 1 10 0 1 0\n\
         FR 0 1 0 0 28.4 0\nEN\n",
    );
    let out = converge(&deck, &[]);
    let (stdout, stderr) = text(&out);
    assert!(
        out.status.success(),
        "a drifting series is a result: {stderr}"
    );
    assert_eq!(
        field(&stdout, "REF"),
        Some("EX 1 10 -> 1x 10, 3x 30, 9x 90"),
        "{stdout}"
    );
    // The premise, measured 2026-10-22 (Hallén R 80.057, 80.140, 80.335): the
    // steps grow.
    let v = study(&deck, &[]);
    let r: Vec<f64> = [1, 3, 9].iter().map(|&f| z_at(&v, 0, f).0).collect();
    assert!(
        (r[1] - r[0]).abs() < (r[2] - r[1]).abs(),
        "a growing step: {r:?}"
    );
    assert_eq!(field(&stdout, "CLASS"), Some("diverging"), "{stdout}");
    assert_eq!(field(&stdout, "SETTLED"), Some("false"));
    assert_eq!(field(&stdout, "ALL_SETTLED"), Some("false"));
    assert!(field(&stdout, "Z_EXTRAPOLATED").is_none(), "{stdout}");
    assert!(
        stdout.contains("NOTE `EX` on tag 1 segment 10 touches a wire end")
            && stdout.contains("may have no limit (FND-277, FND-278)"),
        "{stdout}"
    );
    assert!(
        stderr.contains("warning: converge: row 1") && stderr.contains("has not settled"),
        "{stderr}"
    );
}

// ---------------------------------------------------------------------------
// C4: two meshes give a step and no verdict.
// ---------------------------------------------------------------------------

#[test]
fn two_meshes_give_a_step_and_no_verdict() {
    let out = converge(&root().join(DIPOLE), &["--factors", "1,3"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(mesh_lines(&stdout).len(), 2);
    assert_eq!(field(&stdout, "SETTLED"), Some("undecided"), "{stdout}");
    assert_eq!(field(&stdout, "ALL_SETTLED"), Some("undecided"));
    assert!(field(&stdout, "STEPS_R").is_some());
    for absent in ["TAIL_R", "CLASS", "Z_EXTRAPOLATED"] {
        assert!(
            field(&stdout, absent).is_none(),
            "{absent} printed on two meshes:\n{stdout}"
        );
    }
    assert!(stderr.contains("no verdict"), "{stderr}");
}

// ---------------------------------------------------------------------------
// C5: refusals, each by name and exit code, and the dipole as the control.
// ---------------------------------------------------------------------------

#[test]
fn the_study_refuses_what_it_cannot_do_and_says_why() {
    let dipole = root().join(DIPOLE);
    let code = |out: &Output| out.status.code();
    // A refusal before any solve: exit 1, the sentence, nothing on stdout.
    let refused = |deck: &Path, extra: &[&str], want: &str| {
        let out = converge(deck, extra);
        let (stdout, stderr) = text(&out);
        assert_eq!(code(&out), Some(1), "{}: {stderr}", deck.display());
        assert!(stderr.contains(want), "want `{want}` in:\n{stderr}");
        assert!(
            stdout.is_empty(),
            "nothing solved, nothing printed:\n{stdout}"
        );
    };

    // A receiving antenna: no feedpoint to converge.
    let rx = temp_deck(
        "c5-rx",
        "CE\nGW 1 21 0 0 -5 0 0 5 .001\nGE\nEX 1 1 1 0 90 0 0\nFR 0 1 0 0 14.2 0\nEN\n",
    );
    refused(
        &rx,
        &[],
        "error: converge: this deck is a receiving antenna (EX type 1); there is no feedpoint \
         impedance to converge",
    );

    // Tag 0 with a segment number: an absolute number fnec does not map (FND-282).
    let abs = temp_deck(
        "c5-abs",
        "CE\nGW 1 21 0 0 -5 0 0 5 .001\nGE\nEX 0 1 11 0 1 0\nLD 4 0 5 5 50 0 0\n\
         FR 0 1 0 0 14.2 0\nEN\n",
    );
    refused(
        &abs,
        &[],
        "error: converge: `LD 4 0 5 5` names an absolute segment number (tag 0), which \
         refinement does not map yet — give the wire a tag",
    );

    // A one-segment wire's reference touches both ends under `keep`.
    let one = temp_deck(
        "c5-one",
        "CE\nGW 1 1 0 0 0 0 0 1 .001\nGW 2 11 0 0 1 0 0 2 .001\nGE\nEX 0 1 1 0 1 0\n\
         FR 0 1 0 0 100 0\nEN\n",
    );
    refused(
        &one,
        &[],
        "tag 1 has one segment; its `EX` reference touches both ends — give the wire two \
         segments or use `--ends centre`",
    );

    // The factor list: odd, increasing, geometric, two or more — a usage error.
    for bad in [
        "1,2,4", "2,6,18", "1,3,5", "3,1", "1", "1,3,9,15", "0,3", "1,x",
    ] {
        let out = converge(&dipole, &["--factors", bad]);
        let (stdout, stderr) = text(&out);
        assert_eq!(code(&out), Some(2), "--factors {bad}: {stderr}");
        assert!(
            stderr.contains("invalid --factors value"),
            "{bad}: {stderr}"
        );
        assert!(stdout.is_empty());
    }
    let (_, err) = text(&converge(&dipole, &["--factors", "1,3,5"]));
    assert!(
        err.contains("the settle rule prices a geometric tail; 1,3,5 has none"),
        "{err}"
    );
    let (_, err) = text(&converge(&dipole, &["--factors", "2,6,18"]));
    assert!(err.contains("2 is not odd"), "{err}");

    // A parasitic wire the finest mesh takes under the thin-wire kernel's
    // validity — every segment counts, not the fed one's: radius 0.03 m on
    // 0.207 m segments is 6.9 radii at 1×, 2.3 at 3×, 0.77 at 9×.
    let fat = temp_deck(
        "c5-fat",
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGW 2 51 1 0 -5.282 1 0 5.282 0.03\nGE\n\
         EX 0 1 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
    );
    refused(
        &fat,
        &[],
        "error: converge: at 9x tag 2's segments are 0.77 radii long, under the thin-wire \
         kernel's 2; the largest usable factor is 3 (`--factors 1,3`)",
    );
    // The same structure at 1,3 runs, and notes the mesh under nec2c's limit.
    let out = converge(&fat, &["--factors", "1,3"]);
    let (stdout, err) = text(&out);
    assert!(out.status.success(), "{err}");
    assert!(
        stdout.contains("NOTE at 3x the shortest segment is 2.3 radii (tag 2)")
            && stdout.contains("nec2c's thin-kernel limit of 8"),
        "{stdout}"
    );

    // The segment budget: 1201 segments is 10 809 at 9×, past MAX_SEGMENTS.
    let big = temp_deck(
        "c5-big",
        "CE\nGW 1 1201 0 0 -5.282 0 0 5.282 0.0001\nGE\nEX 0 1 601 0 1.0 0.0\n\
         FR 0 1 0 0 14.2 0.0\nEN\n",
    );
    refused(
        &big,
        &[],
        "error: converge: at 9x: GW card: the deck reaches 10809 segments",
    );
    refused(
        &big,
        &[],
        "the largest usable factor is 3 (`--factors 1,3`)",
    );

    // What the main command refuses, with its sentence (FND-173) and the factor.
    let ld9 = temp_deck(
        "c5-ld9",
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGE\nLD 9 1 26 26 50 0 0\nEX 0 1 26 0 1.0 0.0\n\
         FR 0 1 0 0 14.2 0.0\nEN\n",
    );
    let main_err = text(&fnec(&[ld9.path().to_str().unwrap()])).1;
    let sentence = main_err
        .lines()
        .find_map(|l| l.strip_prefix("error: "))
        .unwrap_or_else(|| panic!("the main command refuses LD 9:\n{main_err}"))
        .to_string();
    assert!(sentence.contains("load type 9"), "{sentence}");
    refused(&ld9, &[], &format!("error: converge: at 1x: {sentence}"));
    // ... on a refined mesh too, before any mesh is solved: a base 0.1 mm below
    // z = 0 is on the ground at 1× (within 1e-3 of the 0.203 m segment) and
    // buried at 3× (1e-3 of 0.068 m).
    let buried = temp_deck(
        "c5-buried",
        "CE\nGW 1 26 0 0 -0.0001 0 0 5.282 0.001\nGE 1\nGN 1\nEX 0 1 1 0 1.0 0.0\n\
         FR 0 1 0 0 14.2 0\nEN\n",
    );
    let out = fnec(&[buried.path().to_str().unwrap()]);
    assert!(
        out.status.success(),
        "the premise: 1x solves: {}",
        text(&out).1
    );
    refused(
        &buried,
        &[],
        "error: converge: at 3x: tag 1 seg 1 lies below the ground plane",
    );

    // Options this command does not take, the pulse bases without the flag, and
    // no deck: usage errors.
    for bad in [
        &["--loads-config", "x.toml"][..],
        &["--sweep-config", "x.toml"],
        &["--bogus"],
        &["--solver", "pulse"],
        &["--ends", "middle"],
    ] {
        let out = converge(&dipole, bad);
        assert_eq!(code(&out), Some(2), "{bad:?}: {}", text(&out).1);
    }
    let out = fnec(&["converge"]);
    assert_eq!(code(&out), Some(2));
    assert!(text(&out).1.contains("Usage: fnec converge"));
    assert!(text(&out).1.contains("missing deck path"));

    // The control.
    assert!(converge(&dipole, &["--factors", "1,3"]).status.success());
}

// ---------------------------------------------------------------------------
// C6: loads and ports — a lumped range becomes points, a per-metre range a
// length, a port's 0 is resolved before it is mapped.
// ---------------------------------------------------------------------------

#[test]
fn loads_and_ports_map_to_the_hand_written_finer_deck() {
    let z3 = |body: &str, name: &str| -> (f64, f64) {
        let deck = temp_deck(name, body);
        z_at(&study(&deck, &["--factors", "1,3"]), 0, 3)
    };
    let hand = |body: &str, name: &str| -> (f64, f64) {
        let deck = temp_deck(name, body);
        main_z(&deck)[0]
    };

    // A lumped load on segments 3..7 of 21: five loads on 8, 11, 14, 17, 20 of 63.
    same(
        z3(
            "CE\nGW 1 21 0 0 -5 0 0 5 .001\nGE\nEX 0 1 11 0 1 0\nLD 0 1 3 7 10 1e-6 0\n\
             FR 0 1 0 0 14.2 0\nEN\n",
            "c6-ld0",
        ),
        hand(
            "CE\nGW 1 63 0 0 -5 0 0 5 .001\nGE\nEX 0 1 32 0 1 0\nLD 0 1 8 8 10 1e-6 0\n\
             LD 0 1 11 11 10 1e-6 0\nLD 0 1 14 14 10 1e-6 0\nLD 0 1 17 17 10 1e-6 0\n\
             LD 0 1 20 20 10 1e-6 0\nFR 0 1 0 0 14.2 0\nEN\n",
            "c6-ld0-hand",
        ),
        SAME,
        "lumped range",
    );
    // A conductivity load over the same length: 7..21 of 63.
    same(
        z3(
            "CE\nGW 1 21 0 0 -5 0 0 5 .001\nGE\nEX 0 1 11 0 1 0\nLD 5 1 3 7 5.8e7 0 0\n\
             FR 0 1 0 0 14.2 0\nEN\n",
            "c6-ld5",
        ),
        hand(
            "CE\nGW 1 63 0 0 -5 0 0 5 .001\nGE\nEX 0 1 32 0 1 0\nLD 5 1 7 21 5.8e7 0 0\n\
             FR 0 1 0 0 14.2 0\nEN\n",
            "c6-ld5-hand",
        ),
        SAME,
        "per-metre range",
    );
    // Ports at 0 on even wires: the lower centre, 10 of 20, then 29 of 60 (the
    // refined wire's own centre would be 30).
    same(
        z3(
            "CE\nGW 1 20 0 0 -5 0 0 5 .001\nGW 2 20 1 0 -5 1 0 5 .001\nGE\nTL 1 0 2 0 50 0.1\n\
             EX 0 1 10 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
            "c6-tl0",
        ),
        hand(
            "CE\nGW 1 60 0 0 -5 0 0 5 .001\nGW 2 60 1 0 -5 1 0 5 .001\nGE\nTL 1 29 2 29 50 0.1\n\
             EX 0 1 29 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
            "c6-tl0-hand",
        ),
        SAME,
        "port 0",
    );
}

// ---------------------------------------------------------------------------
// C7: a GM copy fed in the copy maps inside the copy.
// ---------------------------------------------------------------------------

#[test]
fn a_gm_copy_maps_inside_the_copy() {
    let gm = temp_deck(
        "c7-gm",
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 0.001\nGM 10 1 0 0 0 1 0 0 0\nGE\nEX 0 11 26 0 1.0 0.0\n\
         FR 0 1 0 0 14.2 0.0\nEN\n",
    );
    let out = converge(&gm, &["--factors", "1,3"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(
        field(&stdout, "REF"),
        Some("EX 11 26 -> 1x 26, 3x 77"),
        "{stdout}"
    );
    let explicit = temp_deck(
        "c7-explicit",
        "CE\nGW 1 153 0 0 -5.282 0 0 5.282 0.001\nGW 11 153 1 0 -5.282 1 0 5.282 0.001\nGE\n\
         EX 0 11 77 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
    );
    let v = study(&gm, &["--factors", "1,3"]);
    same(
        z_at(&v, 0, 3),
        main_z(&explicit)[0],
        SAME,
        "the copy vs two explicit wires",
    );
}

// ---------------------------------------------------------------------------
// C8: the twin gate — on every converged corpus deck, `--ends centre` at 3× is
// the script's 3× deck, every frequency and source.
// ---------------------------------------------------------------------------

#[test]
fn the_centre_rule_is_the_scripts_on_every_converged_corpus_deck() {
    let corpus: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("corpus/reference-results.json")).unwrap(),
    )
    .unwrap();
    let mut checked = 0;
    for (name, case) in corpus["cases"].as_object().unwrap() {
        if case["external_reference_candidate"]
            .get("convergence")
            .is_none()
        {
            continue;
        }
        let deck = root()
            .join("corpus")
            .join(case["deck_file"].as_str().unwrap());
        let v = study(&deck, &["--factors", "1,3", "--ends", "centre"]);
        let fine = temp_deck(&format!("c8-{name}"), &remeshed(&deck, 3));
        let rows = v["rows"].as_array().unwrap();
        // Every row, to the main command's six printed decimals ...
        let script_rows = main_rows_text(&fine);
        assert_eq!(rows.len(), script_rows.len(), "{name}: rows\n{v}");
        for (i, theirs) in script_rows.iter().enumerate() {
            same(z_at(&v, i, 3), *theirs, 5e-7, &format!("{name} row {i}"));
        }
        // ... and each frequency's first source in full precision.
        let firsts = main_z(&fine);
        let mut f = 0;
        for (i, row) in rows.iter().enumerate() {
            let first_of_freq = i == 0 || row["freq_mhz"] != rows[i - 1]["freq_mhz"];
            if first_of_freq {
                same(z_at(&v, i, 3), firsts[f], SAME, &format!("{name} row {i}"));
                f += 1;
            }
        }
        assert_eq!(f, firsts.len(), "{name}: frequencies");
        checked += 1;
    }
    assert_eq!(checked, 11, "the converged corpus cases");
}

// ---------------------------------------------------------------------------
// C9: JSON — valid, the documented shape, stable across runs.
// ---------------------------------------------------------------------------

#[test]
fn json_is_valid_and_stable() {
    let deck = root().join(DIPOLE);
    let out = converge(&deck, &["--output-format", "json"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["schema"], "fnec-converge/1");
    assert_eq!(v["solver"], "hallen");
    assert_eq!(v["factors"], serde_json::json!([1, 3, 9]));
    assert_eq!(v["ends"], "keep");
    assert_eq!(v["tol"], serde_json::json!({"ohm": 0.5, "pct": 1.0}));
    assert_eq!(v["all_settled"], true);
    let row = &v["rows"][0];
    assert_eq!(row["freq_mhz"], 14.2);
    assert_eq!(
        (row["tag"].as_u64(), row["seg"].as_u64()),
        (Some(1), Some(26))
    );
    assert_eq!(
        row["refs"],
        serde_json::json!([{"card": "EX", "tag": 1, "seg": 26, "touches_end": false,
                            "mapped": [26, 77, 230]}])
    );
    assert_eq!(row["meshes"].as_array().unwrap().len(), 3);
    assert_eq!(row["meshes"][1]["segments"], 153);
    assert_eq!(row["meshes"][1]["route"], "plain");
    assert_eq!(row["steps"]["r"].as_array().unwrap().len(), 2);
    assert_eq!(row["class"], "settling");
    assert_eq!(row["settled"], true);
    assert!(row["extrapolated"]["z_re"].as_f64().is_some());
    assert_eq!(row["notes"], serde_json::json!([]));
    let again = text(&converge(&deck, &["--output-format", "json"])).0;
    assert_eq!(stdout, again, "two runs differ");
    // Two meshes: an undecided verdict is a string, the tail, class and
    // extrapolation null.
    let v = study(&deck, &["--factors", "1,3"]);
    let row = &v["rows"][0];
    assert_eq!(row["settled"], "undecided");
    for absent in ["tail", "class", "extrapolated"] {
        assert!(row[absent].is_null(), "{absent}: {row}");
    }
    assert_eq!(v["all_settled"], "undecided");
}

// ---------------------------------------------------------------------------
// C10: the caveats are each mesh's own, printed under each mesh's prefix.
// ---------------------------------------------------------------------------

#[test]
fn caveats_are_each_meshs_own() {
    // The bent conductor with a one-segment run at 1×: three segments at 3×, so
    // the caveat is the 1× mesh's alone.
    let deck = root().join("corpus/u-one-segment-negative-r-freesp.nec");
    let out = converge(&deck, &["--factors", "1,3"]);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let caveat = |mesh: &str| {
        let prefix = format!("mesh {mesh}: warning:");
        stderr
            .lines()
            .filter(|l| l.starts_with(&prefix) && l.contains("one segment long"))
            .count()
    };
    assert_eq!(caveat("1x"), 1, "{stderr}");
    assert_eq!(caveat("3x"), 0, "{stderr}");
    // The route each mesh solved is printed with it. Measured: both meshes of
    // this deck take the junction-paths route.
    let routes: Vec<String> = mesh_lines(&stdout).into_iter().map(|m| m.2).collect();
    assert_eq!(routes, ["paths", "paths"], "{stdout}");

    // A caveat true at every mesh — the antenna 0.05 λ over finite ground — is
    // printed under every mesh, not once per study.
    let low = temp_deck(
        "c10-low",
        "CE\nGW 1 21 -5.278 0 1.056 5.278 0 1.056 0.001\nGE 0\nGN 2 0 0 0 13 0.005\n\
         EX 0 1 11 0 1.0 0.0\nFR 0 1 0 0 14.2 0.0\nEN\n",
    );
    let out = converge(&low, &["--factors", "1,3"]);
    let (_, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    for mesh in ["1x", "3x"] {
        let prefix = format!("mesh {mesh}: warning: antenna is 0.050 λ");
        let n = stderr.lines().filter(|l| l.starts_with(&prefix)).count();
        assert_eq!(n, 1, "mesh {mesh}:\n{stderr}");
    }
}
