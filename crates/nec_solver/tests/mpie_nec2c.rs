// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-157 — the MPIE solver against nec2c, through the session every frontend
//! uses.
//!
//! MPIE integrated the reduced kernel's self and adjacent terms with a 6-point
//! rule, which cannot resolve the kernel's peak (width ~ the wire radius, 1 mm on
//! a 200 mm segment). It read 6% low on a dipole, 35 Ω off in R on a Yagi, and
//! 260 Ω off in X on a Y-junction — the topology MPIE exists for. The static part
//! is now integrated exactly. nec2c 1.3.1, captured 2026-09-26.

use nec_solver::{build_geometry, ground_model_from_deck, solve_mpie_session};
use num_complex::Complex64;

const F: f64 = 14.2e6;

fn z_mpie(text: &str, feed: (u32, u32)) -> Complex64 {
    let deck = nec_parser::parse(text).expect("parses").deck;
    let segs = build_geometry(&deck).expect("geometry");
    let currents =
        solve_mpie_session(&deck, &segs, &ground_model_from_deck(&deck), F).expect("solves");
    let idx = segs
        .iter()
        .position(|s| s.tag == feed.0 && s.tag_index == feed.1)
        .expect("feed");
    Complex64::new(1.0, 0.0) / currents[idx]
}

fn assert_near(what: &str, z: Complex64, want: (f64, f64), tol: (f64, f64)) {
    assert!(
        (z.re - want.0).abs() < tol.0 && (z.im - want.1).abs() < tol.1,
        "{what}: MPIE {z:.3}, nec2c {} + j{}",
        want.0,
        want.1
    );
}

/// Was 74.44 + j41.75.
#[test]
fn a_half_wave_dipole_tracks_nec2c() {
    let d = "CE\nGW 1 41 0 0 -5.2782 0 0 5.2782 0.001\nGE 0\nEX 0 1 21 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near("dipole", z_mpie(d, (1, 21)), (79.107, 45.047), (0.8, 1.0));
}

/// `corpus/yagi-5elm-51seg.nec`. Was 43.47 + j43.77 — R 5× the truth.
#[test]
fn a_five_element_yagi_tracks_nec2c() {
    let d = "CE\nGW 1 51 -3.80 0 -5.388 -3.80 0 5.388 0.001\nGW 2 51  0.00 0 -5.282  0.00 0 5.282 0.001\n\
             GW 3 51  2.64 0 -5.176  2.64 0 5.176 0.001\nGW 4 51  5.28 0 -5.092  5.28 0 5.092 0.001\n\
             GW 5 51  8.45 0 -4.965  8.45 0 4.965 0.001\nGE 0\nEX 0 2 26 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near("Yagi", z_mpie(d, (2, 26)), (8.1749, 56.538), (0.5, 1.5));
}

/// A degree-3 Y-junction — the case `--solver mpie` is recommended for. Was
/// 63.67 − j322.20: 260 Ω of reactance off.
#[test]
fn a_y_junction_tracks_nec2c() {
    let d = "CE\nGW 1 20 0 0 0 5 0 0 0.001\nGW 2 20 0 0 0 -2.5 4.330127 0 0.001\n\
             GW 3 20 0 0 0 -2.5 -4.330127 0 0.001\nGE 0\nEX 0 1 10 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n";
    assert_near(
        "Y-junction",
        z_mpie(d, (1, 10)),
        (67.215, -63.033),
        (2.5, 2.5),
    );
}

/// The MPIE's feed is NEC's: a gap on a segment drives both triangle bases that
/// touch it, V/2 each (FND-224). It drove one end node, half a segment from where
/// the current is read — first-order in the mesh, and mirrored ports unequal.
mod centred_feed {
    use super::*;

    fn dipole(n: u32, feeds: &[u32]) -> String {
        let ex: String = feeds
            .iter()
            .map(|s| format!("EX 0 1 {s} 0 1 0\n"))
            .collect();
        format!("CE\nGW 1 {n} 0 0 -5.28 0 0 5.28 .001\nGE 0\n{ex}FR 0 1 0 0 14.2 0\nEN\n")
    }

    fn rel(z: Complex64, want: (f64, f64)) -> f64 {
        let w = Complex64::new(want.0, want.1);
        (z - w).norm() / w.norm()
    }

    /// Two feeds mirrored about the centre see the same structure, so their ports
    /// must agree exactly. They differed by 49.5 % at 41 segments.
    #[test]
    fn mirrored_ports_report_the_same_impedance() {
        let text =
            "CE\nGW 1 41 -10 0 0 10 0 0 .001\nGE 0\nEX 0 1 11 0 1 0\nEX 0 1 31 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
        let (a, b) = (z_mpie(text, (1, 11)), z_mpie(text, (1, 31)));
        assert!((a - b).norm() < 1e-9 * a.norm(), "ports {a:.3} and {b:.3}");
    }

    /// A centre-fed dipole's current is mirror-symmetric. Its impedance came out
    /// right by symmetry even with the one-node feed, but the currents did not: the
    /// end segments differed by 0.75 % at 21. Proves the centred path runs on a
    /// deck whose impedance cannot show it.
    #[test]
    fn a_centre_fed_dipole_carries_mirror_symmetric_current() {
        for n in [21u32, 41] {
            let deck = nec_parser::parse(&dipole(n, &[n / 2 + 1])).unwrap().deck;
            let segs = build_geometry(&deck).unwrap();
            let i = solve_mpie_session(&deck, &segs, &ground_model_from_deck(&deck), F).unwrap();
            let (first, last) = (i[0].norm(), i[i.len() - 1].norm());
            assert!(
                (first - last).abs() < 1e-9 * first,
                "{n} segments: end currents {first:.6e} and {last:.6e}"
            );
        }
    }

    /// An off-centre feed against nec2c on the same mesh: the gap shrinks with the
    /// mesh and is small. With the one-node feed it was 6.48 % / 3.36 %; centred,
    /// 1.06 % / 0.50 %. nec2c 1.3.1, the same deck text.
    #[test]
    fn an_off_centre_feed_converges_on_nec2c() {
        let at21 = rel(z_mpie(&dipole(21, &[6]), (1, 6)), (152.23, 74.574));
        let at41 = rel(z_mpie(&dipole(41, &[11]), (1, 11)), (158.77, 76.743));
        assert!(
            at41 < at21,
            "the gap must shrink with the mesh: {at21:.4} -> {at41:.4}"
        );
        assert!(
            at41 < 0.01,
            "{:.2} % from nec2c at 41 segments",
            100.0 * at41
        );
    }

    /// The two nodes take their own reference signs: an inverted-V fed beside its
    /// apex, both wires written outward from it, so the apex node's basis runs
    /// against the segment. With one sign for both, the gap would cancel itself.
    #[test]
    fn an_apex_fed_inverted_v_takes_each_nodes_sign() {
        for (n, want) in [(21u32, (30.397, -380.31)), (41, (30.114, -378.08))] {
            let text = format!(
                "CE\nGW 1 {n} 0 0 1.6 -3.7 0 0.6 .001\nGW 2 {n} 0 0 1.6 3.7 0 0.6 .001\nGE 0\nEX 0 1 1 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n"
            );
            let gap = rel(z_mpie(&text, (1, 1)), want);
            assert!(gap < 0.01, "{n} per arm: {:.2} % from nec2c", 100.0 * gap);
        }
    }

    /// A feed on a segment at a free wire end is refused: there is no basis there,
    /// and the answer stayed 20-37 % from nec2c however fine the mesh (FND-228).
    #[test]
    fn a_feed_at_a_free_wire_end_is_refused() {
        let deck = nec_parser::parse(&dipole(21, &[1])).unwrap().deck;
        let segs = build_geometry(&deck).unwrap();
        let err = solve_mpie_session(&deck, &segs, &ground_model_from_deck(&deck), F)
            .expect_err("an end-segment feed must be refused");
        assert!(err.to_string().contains("free wire end"), "{err}");
    }
}
