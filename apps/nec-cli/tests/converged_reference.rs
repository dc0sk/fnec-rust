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

/// The settle rule of `nec_solver::convergence`, on a series read from nec2c's
/// five-significant-digit prints: the last step is widened by one printed unit.
fn remaining_drift(values: &[f64]) -> f64 {
    let n = values.len();
    assert!(n >= 3, "a tail needs three points");
    let ulp = values[n - 3..]
        .iter()
        .map(|v| printed_ulp(*v))
        .fold(0.0, f64::max);
    nec_solver::convergence::remaining_drift(values, ulp)
}

// How a nec2c mesh series behaves (FND-256): the shared rule.
use nec_solver::convergence::{classify, Trend as Series};

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

/// nec2c's own series at the junction decks FND-256 found unsettled, as data:
/// a later "re-point at nec2c's converged value" on these decks is refused here,
/// because nec2c has no converged value to give on them. Each at 1 / 3 / 9 / 27 ×
/// (`scripts/nec2c-converged-series.py`, captured 2026-10-16), X in ohms.
/// The corpus gate above admits a settling series only.
#[test]
fn nec2c_has_no_converged_value_at_these_junctions() {
    // The H, `corpus/series/h-top-hat-41-25.nec` at 7.1 MHz, `EK 0` (`--ek --to 27`,
    // reproduced 2026-10-17): the steps grow, toward fnec's settled −49.78 (FND-264).
    assert_eq!(
        classify(&[-57.427, -56.402, -55.187, -53.32]),
        Series::Diverging
    );
    // The T through a bar (`interior_joins.rs`), 41 per wire, `EK 0`: a constant step.
    assert_eq!(
        classify(&[-103.76, -103.21, -102.78, -102.35]),
        Series::LogDrift
    );
    // The tee fed at its node (`junction_feedpoint.rs`), 13 per wire.
    assert_eq!(
        classify(&[13.773, 13.553, 13.782, 13.999]),
        Series::LogDrift
    );
    // FND-252's 5:1 radius step drifts at a near-constant 9.5 Ω a tripling too.
    assert_eq!(
        classify(&[73.803, 65.116, 55.638, 45.771]),
        Series::LogDrift
    );
    // Against them, the stem-fed Y at 41 per wire settles — slowly, a 0.71 % tail.
    assert_eq!(
        classify(&[-1639.4, -1612.6, -1599.4, -1591.6]),
        Series::Settling
    );
}

/// A receive series written by `scripts/nec2c-receive-series.py` (FND-265).
fn receive_series(name: &str) -> Value {
    let path = root().join("corpus/series").join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).expect("series"))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Im of the current at deck segment `seg` (1-based), at each multiplier given.
fn receive_im(series: &Value, seg: usize, mults: &[&str]) -> Vec<f64> {
    mults
        .iter()
        .map(|m| series[m]["I"][seg - 1][1].as_f64().expect("current"))
        .collect()
}

/// The largest change over the whole current table between two multipliers, as a
/// share of the finer one's peak.
fn table_step(series: &Value, a: &str, b: &str) -> f64 {
    let (ia, ib) = (
        series[a]["I"].as_array().unwrap(),
        series[b]["I"].as_array().unwrap(),
    );
    let c = |v: &Value| (v[0].as_f64().unwrap(), v[1].as_f64().unwrap());
    let worst = ia
        .iter()
        .zip(ib)
        .map(|(x, y)| {
            let ((xr, xi), (yr, yi)) = (c(x), c(y));
            ((xr - yr).powi(2) + (xi - yi).powi(2)).sqrt()
        })
        .fold(0.0, f64::max);
    worst / series[b]["peak"].as_f64().unwrap()
}

/// FND-265: at the T's node nec2c's receive current drifts at a near-constant
/// ratio — toward fnec, the H's class — so `graph_receive_nec2c.rs` gates those
/// pins as a band and by fnec's own convergence, not against nec2c's last point.
/// Read from the checked-in series, the node segment's Im at 1 / 3 / 9 / 27 ×.
#[test]
fn nec2c_drifts_at_the_t_node_on_receive() {
    let m = ["1", "3", "9", "27"];
    for deck in ["loaded-t-21.json", "t45-21.json"] {
        let s = receive_series(deck);
        assert_eq!(s["kernel"], "extended", "{deck}");
        assert_eq!(
            classify(&receive_im(&s, 21, &m)),
            Series::LogDrift,
            "{deck}"
        );
    }
}

/// FND-267: nec2c's extended kernel does not carry its regime to an inverted-L's
/// corner. Below about 9 radii a corner segment the whole stem shifts at once: at
/// 1 mm the 9 → 27 × step over the table is over four times the 3 → 9 step, and the
/// series script flags 27× (5.3 radii); a 0.2 mm control stays smooth through 27×
/// (26 radii) and breaks at 81× (8.8). `planewave_ground_contact.rs` therefore takes
/// its reference at 9×, the last unflagged point. Not a ground effect: the doubled
/// structure in free space shifts the same (Fable's control, FND-265's review).
#[test]
fn nec2c_breaks_at_a_corner_below_about_nine_radii() {
    let l = receive_series("inverted-l-21.json");
    assert_eq!(l["9"]["corner_below_10"], false);
    assert_eq!(l["27"]["corner_below_10"], true);
    let (s3, s9, s27) = (
        table_step(&l, "1", "3"),
        table_step(&l, "3", "9"),
        table_step(&l, "9", "27"),
    );
    assert!(s9 < s3, "1 mm settles to 9x: {s3:.5} then {s9:.5}");
    assert!(s27 > 4.0 * s9, "1 mm breaks at 27x: {s9:.5} then {s27:.5}");

    let c = receive_series("inverted-l-21-r02.json");
    assert_eq!(c["27"]["corner_below_10"], false);
    assert_eq!(c["81"]["corner_below_10"], true);
    let (c9, c27, c81) = (
        table_step(&c, "3", "9"),
        table_step(&c, "9", "27"),
        table_step(&c, "27", "81"),
    );
    assert!(
        c27 < c9,
        "0.2 mm settles through 27x: {c9:.5} then {c27:.5}"
    );
    assert!(
        c81 > 4.0 * c27,
        "0.2 mm breaks at 81x: {c27:.5} then {c81:.5}"
    );

    // The T's 27x is flagged too (5.3 radii), and its 0.2 mm control keeps the
    // series' shape: steps shrinking at every multiplier, unflagged.
    let t = receive_series("t45-21-r02.json");
    assert_eq!(t["27"]["corner_below_10"], false);
    let (t3, t9, t27) = (
        table_step(&t, "1", "3"),
        table_step(&t, "3", "9"),
        table_step(&t, "9", "27"),
    );
    assert!(t27 < t9 && t9 < t3, "T control: {t3:.5}, {t9:.5}, {t27:.5}");
    let t1 = receive_series("t45-21.json");
    let (u3, u9, u27) = (
        table_step(&t1, "1", "3"),
        table_step(&t1, "3", "9"),
        table_step(&t1, "9", "27"),
    );
    assert!(
        u27 < u9 && u9 < u3,
        "T at 1 mm keeps shrinking to 27x: {u3:.5}, {u9:.5}, {u27:.5}"
    );
}

/// FND-289: `dipole-loaded` and `dipole-loaded-noncollinear-hallen` (one deck, the
/// dipole whose tip meets the middle of a top-hat loop's bottom side) are not in
/// `CONVERGED`, and cannot be: nec2c's own series on the deck does not settle
/// within a third of the case's 8 Ω External X gate. nec2c 1.3.1 at 1 / 3 / 9 /
/// 27 ×, `EK 0` (`scripts/nec2c-converged-series.py corpus/dipole-loaded.nec --ek
/// --to 27`, captured 2026-10-23; no `9x_thin`, the loop is at 6.17 radii a
/// segment there): R 13.463 / 13.243 / 13.153 / 13.13, X −896.03 / −886.97 /
/// −882.45 / −879.06. X's last step is three quarters of the one before, a tail of
/// about 10 Ω where 2.67 is allowed (4.5 Ω on the 1 / 3 / 9 × points the corpus
/// reference is taken from). fnec drifts the same way and meets it: −886.74 /
/// −883.22 / −880.96 / −878.83 at 1 / 3 / 9 / 27 × (measured 2026-10-23), 0.23 Ω
/// from nec2c at 27× (a data point here, not gated: a 27× fnec solve is too slow
/// for the suite). The corpus reference stays nec2c's 9× point; this test holds
/// the reason it is not a converged one.
#[test]
fn nec2c_has_no_converged_value_on_the_top_hat_dipole() {
    let r = [13.463, 13.243, 13.153, 13.13];
    let x = [-896.03, -886.97, -882.45, -879.06];
    let d = corpus();
    for name in ["dipole-loaded", "dipole-loaded-noncollinear-hallen"] {
        let case = &d["cases"][name];
        assert_eq!(case["deck_file"], "dipole-loaded.nec", "{name}");
        assert!(
            case["external_reference_candidate"]
                .get("convergence")
                .is_none(),
            "{name} carries a series: move it to CONVERGED and drop this test's claim"
        );
        let tol_x = external_tolerance(case, "X", x[3]).expect("External X gate");
        let tol_r = external_tolerance(case, "R", r[3]).expect("External R gate");
        let drift_x = remaining_drift(&x);
        assert!(
            drift_x > tol_x / 3.0,
            "{name}: X tail {drift_x:.2} Ω is within a third of {tol_x} Ω: it settles"
        );
        assert!(remaining_drift(&r) <= tol_r / 3.0, "{name}: R {r:?}");
    }
    assert_eq!(classify(&x), Series::Settling, "slowly: {x:?}");
}
