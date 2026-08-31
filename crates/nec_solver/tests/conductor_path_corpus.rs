// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-132 — the corpus must reach the conductor-path basis.
//!
//! PH9-CHK-002's path solver handles geometry the plain per-wire basis cannot
//! express. What routes a deck to it is needing a traversal **reversal** to walk
//! the wires as one conductor. Measured across the four ways two wires can meet:
//! start-to-start, end-to-end, and start-of-first-meets-end-of-second all route
//! here; only `end of 1 meets start of 2` — wires listed in the order you would
//! walk them — reduces to trivial paths and takes the plain basis.
//!
//! That single exception is the one people write by hand without thinking about
//! it, which is a large part of why no corpus deck reached this code.
//!
//! Probed on 2026-08-30: of every deck then in `corpus/`, **zero** routed here.
//! That is worse than a coverage gap. FND-121 was a CRITICAL finding about this
//! exact basis being wired to one frontend of four — the worker answered a bent
//! deck 29x wrong in R with the reactance sign flipped — and it was found by an
//! audit rather than by the corpus, because the corpus could not reach the code
//! at all. A regression that re-broke the path basis would have passed every
//! corpus gate.

use nec_parser::parse;
use nec_solver::{build_geometry, hallen_route};

fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("corpus")
}

fn routes_to_paths(path: &std::path::Path) -> Option<bool> {
    let src = std::fs::read_to_string(path).ok()?;
    let deck = parse(&src).ok()?.deck;
    let segs = build_geometry(&deck).ok()?;
    Some(hallen_route(&deck, &segs).paths)
}

/// The fixture itself takes the path basis.
///
/// Its pinned impedance would already catch a routing change — the plain basis
/// answers this deck 9.15 - j767.60 against the path basis's 264.88 + j410.86 —
/// but an impedance mismatch is a confusing way to learn that the routing moved.
/// This says so directly.
#[test]
fn the_split_v_fixture_takes_the_conductor_path_basis() {
    let deck = corpus_dir().join("split-v-conductor-path-freesp.nec");
    assert_eq!(
        routes_to_paths(&deck),
        Some(true),
        "split-v-conductor-path-freesp.nec exists to exercise the conductor-path \
         basis; if it stops routing there it is no longer doing its job"
    );
}

/// The standing check, and the point of the finding: the corpus must keep at
/// least one deck that reaches this basis.
///
/// Pinning only the one fixture would leave the gap re-openable by deleting it.
/// This fails if the corpus ever again contains nothing that routes here —
/// which is the state the audit found.
#[test]
fn the_corpus_still_reaches_the_conductor_path_basis() {
    let mut routed: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(corpus_dir()).expect("corpus/ is readable") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("nec") {
            continue;
        }
        // Decks that do not parse or build are other fixtures' business —
        // several exist to be refused.
        if routes_to_paths(&path) == Some(true) {
            routed.push(path.file_name().expect("name").to_string_lossy().into());
        }
    }
    assert!(
        !routed.is_empty(),
        "no deck in corpus/ routes to the conductor-path basis, so PH9-CHK-002 \
         is exercised only by in-source unit tests — the exact state in which a \
         CRITICAL finding (FND-121) about that basis went uncaught by the corpus"
    );
}

/// The endpoint claim FND-128's consolidation rests on.
///
/// The CLI's receive sweep used to pass the caller's raw
/// `wire_endpoints_from_segs` into the non-path plane-wave solve, where
/// `solve_hallen_routed` has always used `merge_collinear_wire_endpoints`.
/// Folding the sweep onto the shared seam adopts the merged list, so the two
/// must be indistinguishable *wherever the non-path branch can actually run*.
///
/// They are, and not by luck: the two lists differ only when consecutive wires
/// meet end-to-start, collinearly, at the same radius — and that connection is a
/// junction, which `build_planewave_hallen` refuses outright
/// (`planewave.rs`: `JunctionedGeometryNotSupported`). So a geometry that could
/// tell the two lists apart never reaches the code that would use either.
///
/// This is the implication, gated: merged != raw implies a junction exists.
#[test]
fn merged_and_raw_endpoints_differ_only_on_junctioned_geometry() {
    let mut differing = 0usize;
    let mut checked = 0usize;

    let mut check = |segs: &[nec_solver::Segment], name: &str| {
        let raw = nec_solver::wire_endpoints_from_segs(segs);
        let merged = nec_solver::merge_collinear_wire_endpoints(segs);
        checked += 1;
        if raw != merged {
            differing += 1;
            assert!(
                !nec_solver::detect_wire_junctions(segs, &raw, 1e-6).is_empty(),
                "{name}: merged endpoints differ from raw, but no junction was detected — \
                 the non-path plane-wave branch would then see two different endpoint lists \
                 depending on which caller reached it"
            );
        }
    };

    for entry in std::fs::read_dir(corpus_dir()).expect("corpus/ is readable") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("nec") {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Decks that do not parse or build are other fixtures' business.
        let Some(segs) = parse(&src).ok().and_then(|p| build_geometry(&p.deck).ok()) else {
            continue;
        };
        check(&segs, &path.file_name().expect("name").to_string_lossy());
    }

    // A corpus that happened to contain no such geometry would make the loop
    // above vacuous, so pin the discriminating case explicitly: two collinear
    // wires meeting end-to-start is exactly where the two lists diverge.
    let collinear = parse(
        "GW 1 4 0 0 -1 0 0 0 0.001\nGW 2 4 0 0 0 0 0 1 0.001\nGE\nEX 0 1 2 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n",
    )
    .expect("collinear deck parses");
    let segs = build_geometry(&collinear.deck).expect("collinear deck builds");
    assert_ne!(
        nec_solver::wire_endpoints_from_segs(&segs),
        nec_solver::merge_collinear_wire_endpoints(&segs),
        "the constructed collinear split must be a case where the two lists differ, \
         or this test proves nothing"
    );
    check(&segs, "constructed collinear split");

    assert!(
        checked > 10 && differing > 0,
        "expected to have checked many decks ({checked}) and found at least one differing \
         geometry ({differing})"
    );
}
