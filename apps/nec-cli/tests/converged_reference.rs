// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-232 — the corpus's nec2c references are nec2c's own converged value.
//!
//! A reference captured on the deck's own mesh is as unconverged as the subject,
//! and two codes erring the same way agree by cancellation: the split-V read
//! 0.15 Ω in X from same-mesh nec2c, and 6.3 Ω from the converged value it and
//! fnec both reach when refined. These references are nec2c on a 9× mesh
//! (`scripts/nec2c-fine-mesh.py`: every segment reference at the same point),
//! each with its 1 / 3 / 9 × series stored beside it. The gates here: the series
//! contracts, the reference is its last point, and fnec moves toward it when its
//! own mesh is refined — the error is measured at two resolutions and shrinks.

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

/// Every case whose reference is a 9× capture. A case that gains or loses one
/// changes this list on purpose.
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

/// The (1×, 3×, 9×) series of each point of `case`, in feedpoint order.
fn series(case: &Value) -> Vec<(Z, Z, Z)> {
    case["external_reference_candidate"]["convergence"]
        .as_array()
        .expect("convergence rows")
        .iter()
        .map(|row| (pair(&row["1x"]), pair(&row["3x"]), pair(&row["9x"])))
        .collect()
}

#[test]
fn the_converged_references_are_exactly_these() {
    let d = corpus();
    let mut found: Vec<&str> = d["cases"]
        .as_object()
        .expect("cases")
        .iter()
        .filter(|(_, c)| c["external_reference_candidate"]["mesh_multiple"].as_u64() == Some(9))
        .map(|(k, _)| k.as_str())
        .collect();
    found.sort_unstable();
    assert_eq!(found, CONVERGED);
}

/// Each step of the series moves less than the one before, and the stored
/// reference is the 9× point, not a value copied from elsewhere.
#[test]
fn every_stored_series_contracts_to_its_reference() {
    let d = corpus();
    for name in CONVERGED {
        let case = &d["cases"][name];
        let ext = &case["external_reference_candidate"];
        let rows = series(case);
        assert!(!rows.is_empty(), "{name}: no series");
        let points: Vec<&Value> = ext["convergence"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| match r["point"].as_str() {
                Some(key) => &ext[key],
                None => ext,
            })
            .collect();
        for ((one, three, nine), stored) in rows.iter().zip(points) {
            assert!(
                dist(*nine, *three) < dist(*three, *one),
                "{name}: 1x {one:?} 3x {three:?} 9x {nine:?} does not contract"
            );
            let reference = (
                stored["real_ohm"].as_f64().expect("real_ohm"),
                stored["imag_ohm"].as_f64().expect("imag_ohm"),
            );
            assert_eq!(
                reference, *nine,
                "{name}: the reference is not the 9x point"
            );
        }
    }
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
        let refs: Vec<(f64, f64)> = series(case).iter().map(|r| r.2).collect();
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
