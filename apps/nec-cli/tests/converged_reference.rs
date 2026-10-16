// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-232 — the corpus's nec2c references are nec2c's own converged value.
//!
//! A reference captured on the deck's own mesh is as unconverged as the subject,
//! and two codes erring the same way agree by cancellation: the split-V read
//! 0.15 Ω in X from same-mesh nec2c, and 6.3 Ω from the converged value it and
//! fnec both reach when refined. These references are nec2c on a finer mesh
//! (`scripts/nec2c-fine-mesh.py`: every segment reference at the same point),
//! each with its 1 / 3 / 9 (/ 27) × series stored beside it
//! (`scripts/nec2c-converged-series.py`). The gates here: the series has settled
//! within the case's own gate, the reference is its last point, and fnec moves
//! toward it when its own mesh is refined — the error is measured at two
//! resolutions and shrinks.
//!
//! FND-255: "settled" was "the last step is shorter than the one before", which a
//! series still drifting at a constant rate passes. Now the remaining drift past
//! the last point — the geometric tail of the last two steps — must be within a
//! third of the tolerance the corpus gate holds fnec to against this reference, so
//! a pass there means fnec is within about [2/3, 4/3] of that tolerance of nec2c's
//! limit, not of a point nec2c itself would still move away from.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus() -> Value {
    let text = std::fs::read_to_string(root().join("corpus/reference-results.json"))
        .expect("reference-results.json");
    serde_json::from_str(&text).expect("parses")
}

/// Every case whose reference is a converged capture (it carries a mesh series).
/// A case that gains or loses one changes this list on purpose.
const CONVERGED: [&str; 11] = [
    "dipole-freesp-51seg",
    "dipole-ground-51seg",
    "dipole-ld-loaded-51seg",
    "dipole-nt-freesp-51seg",
    "dipole-nt-tl-equiv-freesp-51seg",
    "frequency-sweep-dipole",
    "monopole-pec-26seg",
    "multi-source",
    "split-v-conductor-path-freesp",
    "tl-two-dipoles-linked",
    "tl-two-dipoles-linked-nseg0",
];

fn pair(v: &Value) -> (f64, f64) {
    let a = v.as_array().expect("[re, im]");
    (a[0].as_f64().expect("re"), a[1].as_f64().expect("im"))
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// An impedance, (Re, Im) in ohms.
type Z = (f64, f64);

/// The multipliers of a case's series, `[1, 3, 9, …]`, read from its first row's
/// keys. Refused: fewer than three, or a gap — the tail estimate assumes each step
/// refines by the same factor of 3.
fn multipliers(case: &Value, name: &str) -> Vec<u64> {
    let row = &case["external_reference_candidate"]["convergence"][0];
    let mut found: Vec<u64> = row
        .as_object()
        .expect("a convergence row")
        .keys()
        .filter_map(|k| k.strip_suffix('x')?.parse().ok())
        .collect();
    found.sort_unstable();
    let expected: Vec<u64> = (0..found.len() as u32).map(|i| 3u64.pow(i)).collect();
    assert!(
        found.len() >= 3 && found == expected,
        "{name}: the series is at {found:?}; it must be 1, 3, 9, … with three points or more"
    );
    found
}

/// The series of each point of `case`, in feedpoint order: one impedance per
/// multiplier, coarsest first.
fn series(case: &Value, name: &str) -> Vec<Vec<Z>> {
    let ms = multipliers(case, name);
    case["external_reference_candidate"]["convergence"]
        .as_array()
        .expect("convergence rows")
        .iter()
        .map(|row| ms.iter().map(|m| pair(&row[format!("{m}x")])).collect())
        .collect()
}

/// One unit in the last place nec2c prints `v` to: it prints five significant
/// digits.
fn printed_ulp(v: f64) -> f64 {
    10f64.powf(v.abs().max(1e-30).log10().floor() - 4.0)
}

/// How far `values` (one axis of a series, ×3 between points) has still to move
/// past its last point: the geometric tail of the last two steps, `d₂ r / (1 − r)`
/// with `r = d₂ / d₁`. Infinite when the series is not contracting (`r ≥ 1`): it
/// drifts, and no tail can be priced. The last step is widened by one printed unit
/// (it is a difference of two rounded prints), the ratio is not: widened both
/// ways, a series that has settled to within a print of its limit — the loaded
/// dipole's R, 1129.3 / 1129.6 / 1129.7 — reads `r = 1` and an infinite tail,
/// which is the rounding speaking, not the series.
fn remaining_drift(values: &[f64]) -> f64 {
    let n = values.len();
    assert!(n >= 3, "a tail needs three points");
    let (a, b, c) = (values[n - 3], values[n - 2], values[n - 1]);
    let ulp = printed_ulp(a).max(printed_ulp(b)).max(printed_ulp(c));
    let (d1, d2) = ((b - a).abs(), (c - b).abs());
    if d2 == 0.0 {
        return ulp;
    }
    let r = d2 / d1;
    if r >= 1.0 {
        f64::INFINITY
    } else {
        (d2 + ulp) * r / (1.0 - r)
    }
}

/// The tolerance the corpus gate (`corpus_validation.rs`) holds fnec to against
/// this reference on `axis` ("R" or "X"): `max(absolute, percent × |reference|)`,
/// each External figure falling back to the case's own; `None` when the case
/// declares no External figure for the axis, which a converged case may not do.
fn external_tolerance(case: &Value, axis: &str, reference: f64) -> Option<f64> {
    let g = &case["tolerance_gates"];
    let ext_abs = g[format!("External{axis}_absolute_ohm")].as_f64();
    let ext_rel = g[format!("External{axis}_percent_rel")].as_f64();
    if ext_abs.is_none() && ext_rel.is_none() {
        return None;
    }
    let abs = ext_abs.or(g[format!("{axis}_absolute_ohm")].as_f64())?;
    let rel = ext_rel.or(g[format!("{axis}_percent_rel")].as_f64())?;
    Some(abs.max(rel / 100.0 * reference.abs()))
}

#[test]
fn the_converged_references_are_exactly_these() {
    let d = corpus();
    let mut found: Vec<&str> = d["cases"]
        .as_object()
        .expect("cases")
        .iter()
        .filter(|(_, c)| {
            c["external_reference_candidate"]
                .get("convergence")
                .is_some()
        })
        .map(|(k, _)| k.as_str())
        .collect();
    found.sort_unstable();
    assert_eq!(found, CONVERGED);
}

/// Every stored series has settled within its case's gate: the drift left past
/// the last point is at most a third of the tolerance fnec is held to, on R and
/// on X, at every point. The stored reference is that last point, the series is
/// on one recorded kernel, and an extended-kernel series agrees with the thin
/// kernel at 9×, where both apply, within a tenth of the tolerance.
#[test]
fn every_stored_series_settles_within_its_gate() {
    let d = corpus();
    for name in CONVERGED {
        let case = &d["cases"][name];
        let ext = &case["external_reference_candidate"];
        let ms = multipliers(case, name);
        let last = *ms.last().unwrap();
        assert_eq!(
            ext["mesh_multiple"].as_u64(),
            Some(last),
            "{name}: mesh_multiple is not the series' last multiplier"
        );
        let kernel = ext["kernel"].as_str();
        assert!(
            matches!(kernel, Some("thin" | "extended")),
            "{name}: the series records no kernel ({kernel:?})"
        );
        let rows = series(case, name);
        assert!(!rows.is_empty(), "{name}: no series");
        let raw = ext["convergence"].as_array().unwrap();
        for (row, (points, raw)) in rows.iter().zip(raw).enumerate() {
            let stored = match raw["point"].as_str() {
                Some(key) => &ext[key],
                None => ext,
            };
            let reference = (
                stored["real_ohm"].as_f64().expect("real_ohm"),
                stored["imag_ohm"].as_f64().expect("imag_ohm"),
            );
            assert_eq!(
                reference,
                *points.last().unwrap(),
                "{name} row {row}: the reference is not the {last}x point"
            );
            for (axis, pick) in [("R", 0usize), ("X", 1)] {
                let value = |z: &Z| if pick == 0 { z.0 } else { z.1 };
                let at = value(&reference);
                let tol = external_tolerance(case, axis, at)
                    .unwrap_or_else(|| panic!("{name}: no External {axis} gate"));
                let values: Vec<f64> = points.iter().map(value).collect();
                let drift = remaining_drift(&values);
                assert!(
                    drift <= tol / 3.0,
                    "{name} row {row} {axis}: {values:?} has {drift:.3} Ω still to move, \
                     more than a third of its {tol:.3} Ω gate"
                );
                if kernel == Some("extended") {
                    let thin = value(&pair(&raw["9x_thin"]));
                    let nine = values[2];
                    assert!(
                        (nine - thin).abs() <= tol / 10.0,
                        "{name} row {row} {axis}: 9x {nine} on the extended kernel, \
                         {thin} on the thin"
                    );
                }
            }
        }
    }
}

/// The settle rule refuses a series that is still moving. Two known failures:
/// - the FND-252 shape, a dipole thinned 5:1 within 1.26 m of its feed (8 + 3 + 8
///   segments of 5 / 1 / 5 mm, 14.2 MHz, `EK 0`), nec2c at 3 / 9 / 27×, captured
///   2026-10-16: X moves 9.5 then 9.9 Ω a tripling, R 2.0 then 1.8 — no settling;
/// - the split-V's 1 / 3 / 9× series, the corpus reference before FND-255, against
///   its 8.359 Ω R gate: it contracts (r ≈ 0.47), and fails on its tail alone,
///   2.85 Ω against 2.79. Its 27× extension passes.
#[test]
fn the_settle_rule_refuses_a_drifting_series() {
    let step_r = [86.881, 88.896, 90.659];
    let step_x = [65.116, 55.638, 45.771];
    assert!(remaining_drift(&step_x).is_infinite(), "{step_x:?}");
    assert!(remaining_drift(&step_r) > 10.0, "{step_r:?}");
    let split_v_r = [268.56, 275.43, 278.64];
    let drift = remaining_drift(&split_v_r);
    assert!(drift.is_finite() && drift > 8.359 / 3.0, "{drift}");
    let extended = [268.56, 275.43, 278.64, 280.30];
    assert!(remaining_drift(&extended) < 8.359 / 3.0);
}

/// The feedpoint impedances of `deck`, in output order (every source, every
/// frequency).
fn fnec_z(deck: &str) -> Vec<(f64, f64)> {
    let dir = std::env::temp_dir().join(format!("fnec-converged-ref-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let path = dir.join(format!("{}.nec", RUN.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
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
        } else if in_table && !f.is_empty() && f[0].parse::<u32>().is_ok() {
            panic!("unreadable feedpoint row: {line}");
        } else if in_table && !rows.is_empty() && f.is_empty() {
            in_table = false;
        }
    }
    rows
}

/// The remesh script on `deck` at `s`: (success, stdout, stderr).
fn remesh(deck: &std::path::Path, s: u32) -> (bool, String, String) {
    let out = Command::new("python3")
        .arg(root().join("scripts/nec2c-fine-mesh.py"))
        .arg(deck)
        .arg(s.to_string())
        .output()
        .expect("python3 runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `deck_file` meshed `s` times finer by the same script that made the references.
fn remeshed(deck_file: &str, s: u32) -> String {
    let (ok, stdout, stderr) = remesh(&root().join("corpus").join(deck_file), s);
    assert!(ok, "{stderr}");
    stdout
}

/// fnec on the deck, and on the deck meshed 3× finer: the finer answer is nearer
/// the converged reference at every feedpoint and frequency. A same-mesh
/// reference cannot make this assertion — fnec refined moves away from it.
#[test]
fn fnec_refined_moves_toward_the_converged_reference() {
    let d = corpus();
    for name in CONVERGED {
        let case = &d["cases"][name];
        let deck_file = case["deck_file"].as_str().expect("deck_file");
        let text = std::fs::read_to_string(root().join("corpus").join(deck_file)).unwrap();
        let coarse = fnec_z(&text);
        let fine = fnec_z(&remeshed(deck_file, 3));
        let refs: Vec<(f64, f64)> = series(case, name)
            .iter()
            .map(|r| *r.last().unwrap())
            .collect();
        assert_eq!(
            coarse.len(),
            refs.len(),
            "{name}: feedpoint rows {coarse:?}"
        );
        assert_eq!(fine.len(), refs.len(), "{name}: feedpoint rows {fine:?}");
        for ((c, f), r) in coarse.iter().zip(&fine).zip(&refs) {
            assert!(
                dist(*f, *r) < dist(*c, *r),
                "{name}: fnec {c:?} -> {f:?} at 3x does not approach nec2c's {r:?}"
            );
        }
    }
}

/// The script's mapping, checked by hand once: segment 26 of 51 is the middle
/// third, 77 of 153; and a card it cannot map is refused, not passed through.
#[test]
fn the_remesh_maps_a_segment_to_the_same_point_and_refuses_what_it_cannot() {
    let fine = remeshed("dipole-freesp-51seg.nec", 3);
    assert!(
        fine.lines()
            .any(|l| l == "GW 1 153 0 0 -5.282 0 0 5.282 0.001"),
        "{fine}"
    );
    assert!(fine.lines().any(|l| l == "EX 0 1 77 0 1.0 0.0"), "{fine}");
    // Below nec2c's thin-wire limit without EK: refused.
    let (ok, _, err) = remesh(&root().join("corpus/dipole-freesp-51seg.nec"), 27);
    assert!(!ok && err.contains("segment/radius"), "{err}");
    // A card it does not map (a GM copy would renumber the segments): refused.
    let dir = std::env::temp_dir().join(format!("fnec-converged-gm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let gm = dir.join("gm.nec");
    std::fs::write(
        &gm,
        "CE\nGW 1 5 0 0 0 0 0 1 .001\nGM 0 1 0 0 0 0 0 1 0\nGE\nEN\n",
    )
    .unwrap();
    let (ok, _, err) = remesh(&gm, 3);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!ok && err.contains("'GM' is not mapped"), "{err}");
}
