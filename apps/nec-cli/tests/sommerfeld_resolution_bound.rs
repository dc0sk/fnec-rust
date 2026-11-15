// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// FND-325: `--ground-solver sommerfeld` samples its reaction sum once per segment
// pair, and over a ground closer than a segment's length the kernel's peak is kept
// and its cancelling lobe never sampled: a λ/2 dipole 0.02 m up on 0.238 m segments
// read 1185.64 + j132187.3 Ω, exit 0, against nec2c's 177.86 + j255.89 — and the
// low-ground warning named the flag as the remedy. The correction now declines a
// segment longer than its height (naming the MPIE), caveats itself below 0.01 λ,
// and the warning names it only where it applies inside both bounds.
//
// Every nec2c number below is nec2c 1.3.1 on the test's own deck text
// (`horizontal(h, n, feed)` or `TILT3` with `XQ`; one card per line), captured
// 2026-11-14 with `nec2c -i deck.nec -o deck.out`, ANTENNA INPUT PARAMETERS row.
// nec2c is the reference at σ = 0.005: outside FND-239's non-reference regime
// (σ ≥ 1e5), and it and the MPIE converge to each other under refinement on the
// 0.02 m deck (0.1 % apart at 441 segments, FND-325's row).

use std::process::{Command, Output};

mod common;

/// A 5 m horizontal wire `h` metres over `GN 2` 13 / 0.005 at 29.9 MHz (λ = 10.03 m),
/// `n` segments, fed at `feed` — the finding's deck, as nec2c ran it.
fn horizontal(h: f64, n: u32, feed: u32) -> String {
    format!(
        "CE\nGW 1 {n} -2.5 0 {h} 2.5 0 {h} 0.001\nGE 0\nEX 0 1 {feed} 0 1 0\n\
         GN 2 0 0 0 13 0.005\nFR 0 1 0 0 29.9 0\nXQ\nEN\n"
    )
}

/// The same 5 m wire tilted 3°, its low end 0.02 m up: the general (non-horizontal)
/// branch, whose grid has the same defect (lowest-midpoint Δ/z = 9.1). nec2c
/// 119.72 + j121.94; the shipped correction read 99.79 − j214.51, X sign-wrong.
const TILT3: &str = "CE\nGW 1 21 -2.4966 0 0.02 2.4966 0 0.28168 0.001\nGE 0\nEX 0 1 11 0 1 0\n\
                     GN 2 0 0 0 13 0.005\nFR 0 1 0 0 29.9 0\nXQ\nEN\n";

fn fnec(args: &[&str], deck: &str) -> Output {
    let path = common::TempDeck::new("somm-bound.nec", deck);
    Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(path.path())
        .output()
        .expect("run fnec")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// Every FEEDPOINTS row, verbatim (TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM).
fn feed_rows(o: &Output) -> Vec<String> {
    stdout(o)
        .lines()
        .filter(|l| {
            let p: Vec<&str> = l.split_whitespace().collect();
            p.len() == 8 && p[0].parse::<u32>().is_ok() && p[1].parse::<u32>().is_ok()
        })
        .map(str::to_string)
        .collect()
}

fn feed_z(o: &Output) -> (f64, f64) {
    let rows = feed_rows(o);
    let p: Vec<&str> = rows
        .first()
        .expect("a feedpoint row")
        .split_whitespace()
        .collect();
    (p[6].parse().unwrap(), p[7].parse().unwrap())
}

/// |Z − Z_ref| / |Z_ref|.
fn rel(z: (f64, f64), r: (f64, f64)) -> f64 {
    (z.0 - r.0).hypot(z.1 - r.1) / r.0.hypot(r.1)
}

const DECLINED_RESOLUTION: &str = "--ground-solver sommerfeld declined this deck: tag";
const SOMMERFELD_REMEDY: &str = "`--ground-solver sommerfeld`";
const MPIE_REMEDY: &str = "re-run with `--solver mpie`";
const BAND_CAVEAT: &str = "below the 0.01 λ the correction is measured accurate to";

/// The run succeeded, declined for resolution, said so naming the MPIE, and
/// reported the plain run's impedance unchanged.
fn assert_declines(label: &str, deck: &str) {
    let somm = fnec(&["--ground-solver", "sommerfeld"], deck);
    let plain = fnec(&[], deck);
    let err = stderr(&somm);
    assert!(
        somm.status.success(),
        "{label}: exit {:?}\n{err}",
        somm.status
    );
    common::assert_diag_field(&err, "gsolver", "sommerfeld-declined-resolution");
    assert!(
        err.contains(DECLINED_RESOLUTION),
        "{label}: no decline:\n{err}"
    );
    let decline = err
        .lines()
        .find(|l| l.contains(DECLINED_RESOLUTION))
        .unwrap();
    assert!(
        decline.contains("Re-run with `--solver mpie`") && decline.contains("FND-325"),
        "{label}: the decline must name the MPIE: {decline}"
    );
    assert!(
        !err.contains("Z includes the surface-wave correction"),
        "{label}: a declined run claims the correction:\n{err}"
    );
    // Paired with the diag field: a decline that left the outcome `applied`
    // (or applied the delta) fails one or the other.
    assert_eq!(
        feed_rows(&somm),
        feed_rows(&plain),
        "{label}: a declined correction must report the rcm feedpoint row"
    );
}

/// G1: the finding's deck (Δ/H 11.9), the 3° tilt (general branch, Δ/z 9.1) and
/// the boundary deck (Δ/H 1.59, where the shipped correction read X +18 % from
/// nec2c's 110.13 + j88.807) all decline. A bound loosened to 2× passes the
/// first two and applies on the third.
#[test]
fn a_segment_longer_than_its_height_declines_the_correction() {
    let finding = horizontal(0.02, 21, 11);
    assert_declines("H 0.02 m, 21 segments", &finding);
    // The rcm row it keeps: 31.02 + j18.77 (measured with fnec 0.26.0), not the
    // corrected 1185.64 + j132187.3 the finding reported.
    let z = feed_z(&fnec(&["--ground-solver", "sommerfeld"], &finding));
    assert!(
        (z.0 - 31.02).abs() < 0.05 && (z.1 - 18.77).abs() < 0.05,
        "the finding deck: {z:?}"
    );
    assert_declines("3° tilt, low end 0.02 m", TILT3);
    assert_declines(
        "H 0.15 m, 21 segments (the boundary)",
        &horizontal(0.15, 21, 11),
    );
}

/// G1's negative controls: a mesh inside the bound still takes the correction
/// and lands within 3 % of nec2c. A bound tightened to 0.5× declines both.
#[test]
fn segments_no_longer_than_their_height_still_take_the_correction() {
    for (label, deck, nec2c) in [
        // Δ/H 0.79, 0.030 λ. nec2c 86.445 + j56.522; measured 85.71 + j55.23
        // (1.4 %), rcm 33.55 + j30.83.
        (
            "H 0.30 m at 29.9 MHz",
            horizontal(0.3, 21, 11),
            (86.445, 56.522),
        ),
        // `sommerfeld_ground_cli.rs`'s geometry over GN 2: Δ/H 0.95, 0.025 λ.
        // nec2c 87.797 + j68.626 on this deck text; measured 86.90 + j67.68 (1.2 %),
        // rcm 30.70 + j34.15.
        (
            "H 0.528 m at 14.2 MHz",
            "CE\nGW 1 21 -5.278 0 0.528 5.278 0 0.528 0.001\nGE 0\nEX 0 1 11 0 1 0\n\
             GN 2 0 0 0 13 0.005\nFR 0 1 0 0 14.2 0\nXQ\nEN\n"
                .to_string(),
            (87.797, 68.626),
        ),
    ] {
        let somm = fnec(&["--ground-solver", "sommerfeld"], &deck);
        let err = stderr(&somm);
        assert!(somm.status.success(), "{label}:\n{err}");
        common::assert_diag_field(&err, "gsolver", "sommerfeld-applied");
        assert!(!err.contains("declined"), "{label}: declined:\n{err}");
        assert!(
            !err.contains(BAND_CAVEAT),
            "{label}: inside the band:\n{err}"
        );
        let z = feed_z(&somm);
        let plain = feed_z(&fnec(&[], &deck));
        assert!(
            rel(z, nec2c) < 0.03 && rel(plain, nec2c) > 0.3,
            "{label}: corrected {z:?}, rcm {plain:?}, nec2c {nec2c:?}: {:.2} %",
            100.0 * rel(z, nec2c)
        );
    }
}

/// The low-ground warning line of a run's stderr.
fn low_ground_line(err: &str) -> String {
    err.lines()
        .find(|l| l.contains("above finite ground (below ~0.1 λ)"))
        .unwrap_or_else(|| panic!("no low-ground warning:\n{err}"))
        .to_string()
}

/// G2: advice equals cure. On a deck the correction declines, the low-ground
/// warning names the MPIE alone; inside the bound it names both — on the main
/// command, on an FR sweep and on `fnec converge`.
#[test]
fn the_low_ground_warning_names_the_correction_only_where_it_cures() {
    // 0.02 m is also below the 0.01 λ band, which withholds the advice on its own;
    // 0.15 m (0.015 λ, Δ/H 1.59) is above it, so only the mesh bound can.
    let low = horizontal(0.02, 21, 11);
    let coarse = horizontal(0.15, 21, 11);
    let ok = horizontal(0.3, 21, 11);
    let sweep = |d: &str| d.replace("FR 0 1 0 0 29.9 0", "FR 0 3 0 0 29.9 0.1");
    for (label, args, decks_declined, deck_ok) in [
        (
            "main command",
            vec![],
            vec![low.clone(), coarse.clone()],
            ok.clone(),
        ),
        (
            "FR sweep",
            vec![],
            vec![sweep(&low), sweep(&coarse)],
            sweep(&ok),
        ),
        (
            "converge",
            vec!["converge", "--factors", "1,3"],
            vec![low.clone(), coarse.clone()],
            ok.clone(),
        ),
    ] {
        for deck in &decks_declined {
            let declined = low_ground_line(&stderr(&fnec(&args, deck)));
            assert!(
                declined.contains(MPIE_REMEDY) && !declined.contains(SOMMERFELD_REMEDY),
                "{label}: a declined deck's warning: {declined}"
            );
        }
        let applies = low_ground_line(&stderr(&fnec(&args, &deck_ok)));
        assert!(
            applies.contains(MPIE_REMEDY) && applies.contains(SOMMERFELD_REMEDY),
            "{label}: a deck inside the bound: {applies}"
        );
    }
}

/// G3: `fnec converge` and `fnec match` carry the decline. Converge's rows are
/// the rcm study's and the decline is said once, naming the meshes it hit (1×:
/// Δ/H 11.9, 3×: 3.97); match prices the rcm impedance and the child solve's
/// decline reaches its stderr.
#[test]
fn converge_and_match_carry_the_decline() {
    let deck = horizontal(0.02, 21, 11);
    let args = ["converge", "--factors", "1,3"];
    let somm = fnec(
        &[&args[..], &["--ground-solver", "sommerfeld"]].concat(),
        &deck,
    );
    let plain = fnec(&args, &deck);
    let err = stderr(&somm);
    assert!(somm.status.success(), "converge:\n{err}");
    let mesh_rows = |o: &Output| -> Vec<String> {
        stdout(o)
            .lines()
            .filter(|l| l.trim_start().starts_with("MESH "))
            .map(str::to_string)
            .collect()
    };
    assert!(
        !mesh_rows(&plain).is_empty(),
        "no MESH rows:\n{}",
        stdout(&plain)
    );
    assert_eq!(
        mesh_rows(&somm),
        mesh_rows(&plain),
        "converge applied a declined correction"
    );
    let declines: Vec<&str> = err
        .lines()
        .filter(|l| l.contains("declined this deck"))
        .collect();
    assert_eq!(declines.len(), 1, "one decline per run:\n{err}");
    assert!(
        declines[0].starts_with("warning: converge: ")
            && declines[0].contains("(declined at mesh 1x, 3x;"),
        "{}",
        declines[0]
    );
    assert!(
        err.contains("gsolver=sommerfeld-declined-resolution"),
        "{err}"
    );

    let z_load = |o: &Output| {
        stdout(o)
            .lines()
            .find(|l| l.starts_with("Z_LOAD "))
            .map(str::to_string)
            .unwrap_or_else(|| panic!("no Z_LOAD:\n{}", stdout(o)))
    };
    let m_somm = fnec(&["match", "--ground-solver", "sommerfeld"], &deck);
    let m_plain = fnec(&["match"], &deck);
    assert!(m_somm.status.success(), "match:\n{}", stderr(&m_somm));
    assert_eq!(z_load(&m_somm), z_load(&m_plain));
    assert!(
        stderr(&m_somm).contains(DECLINED_RESOLUTION),
        "{}",
        stderr(&m_somm)
    );
}

/// G4: below 0.01 λ a resolved mesh takes the correction with a caveat, and the
/// low-ground warning no longer recommends it. λ/2 at 0.05 m (0.005 λ) on 189
/// segments (Δ/H 0.53): nec2c 148.71 + j170.92. The caveat says "R 5–20 % low";
/// the value is pinned within 15 % so the sentence is a measured band, not prose.
/// Measured 131.41 + j184.65 (2026-11-14): 9.7 % from nec2c, R 11.6 % low; rcm
/// 31.25 + j21.84.
#[test]
fn below_the_measured_band_the_correction_is_caveated_and_not_recommended() {
    let deck = horizontal(0.05, 189, 95);
    let somm = fnec(&["--ground-solver", "sommerfeld"], &deck);
    let err = stderr(&somm);
    assert!(somm.status.success(), "{err}");
    common::assert_diag_field(&err, "gsolver", "sommerfeld-applied");
    let info = err
        .lines()
        .find(|l| l.contains("Z includes the surface-wave correction"))
        .unwrap_or_else(|| panic!("no info line:\n{err}"));
    assert!(
        info.contains(BAND_CAVEAT) && info.contains("0.0050 λ") && info.contains("--solver mpie"),
        "{info}"
    );
    let nec2c = (148.71, 170.92);
    let z = feed_z(&somm);
    assert!(rel(z, nec2c) < 0.15, "{z:?} vs nec2c {nec2c:?}");
    let plain = low_ground_line(&stderr(&fnec(&[], &deck)));
    assert!(
        plain.contains(MPIE_REMEDY) && !plain.contains(SOMMERFELD_REMEDY),
        "below the band the warning must not recommend the correction: {plain}"
    );

    // 0.30 m (0.030 λ): no caveat (the recommendation there is G2's).
    let above = stderr(&fnec(
        &["--ground-solver", "sommerfeld"],
        &horizontal(0.3, 21, 11),
    ));
    assert!(
        above.contains("Z includes the surface-wave correction") && !above.contains(BAND_CAVEAT),
        "{above}"
    );
}

/// G6 (FND-240): the remedy the decline names is accurate where it names it. The
/// MPIE on the finding's deck against nec2c on the same mesh: 1× nec2c
/// 177.86 + j255.89, 3× (63 segments, fed at 32) 183.63 + j258.92; measured
/// 172.60 + j254.97 and 181.85 + j258.77 (2026-11-14, this branch): 1.7 % -> 0.6 %.
#[test]
fn the_mpie_the_decline_names_is_accurate_on_the_finding_deck() {
    let gaps: Vec<f64> = [
        (horizontal(0.02, 21, 11), (177.86, 255.89)),
        (horizontal(0.02, 63, 32), (183.63, 258.92)),
    ]
    .iter()
    .map(|(deck, nec2c)| {
        let o = fnec(&["--solver", "mpie"], deck);
        assert!(o.status.success(), "{}", stderr(&o));
        rel(feed_z(&o), *nec2c)
    })
    .collect();
    assert!(
        gaps[0] < 0.03 && gaps[1] < 0.015 && gaps[1] < gaps[0],
        "MPIE vs nec2c: {:.2} % at 1x, {:.2} % at 3x",
        100.0 * gaps[0],
        100.0 * gaps[1]
    );
}
