// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-227 / FND-230 — a source, load or port on the segment at a free wire end.
//!
//! Hallén's delta gap enters as `sin(k·|s − s_f|)` at the segment midpoints. On a
//! free-end segment every other midpoint lies on one side of the gap, so the
//! source lay in the span of the homogeneous columns and the currents solved to
//! zero: a λ/2 wire fed on segment 1 read 8.17×10¹⁰ Ω, exit 0 (refused since
//! #562). A load stamped there was inert the same way (FND-230). Every free wire
//! end is now refined into three thirds (`build_geometry`); the deck's segment is
//! the centre third, whose midpoint is the deck segment's, with a test point on
//! each side of it.

use std::process::Command;

fn run(args: &[&str], deck_text: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!(
        "fnec-end-segment-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("deck.nec");
    std::fs::write(&path, deck_text).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A straight λ/2 wire at 29.98 MHz (λ = 10 m) with `drive` on it.
fn wire(segs: usize, drive: &str) -> String {
    format!("CE\nGW 1 {segs} 0 0 -2.5 0 0 2.5 0.001\nGE 0\n{drive}\nFR 0 1 0 0 29.98 0\nEN\n")
}

/// The feedpoint impedance of the FEEDPOINTS row for `(tag, seg)`.
fn z_at(stdout: &str, tag: u32, seg: u32) -> (f64, f64) {
    let row = stdout
        .lines()
        .find(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            c.len() == 8 && c[0] == tag.to_string() && c[1] == seg.to_string()
        })
        .unwrap_or_else(|| panic!("no feedpoint row for {tag}/{seg}:\n{stdout}"));
    let f: Vec<f64> = row
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    (f[6], f[7])
}

fn rel(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1) / b.0.hypot(b.1)
}

const REFUSAL: &str = "lies wholly in the homogeneous solution";

/// A stem-fed Y: stem from z = 0 to the junction at z = 2, two arms to the tips.
fn y_junction(tag: u32, seg: u32) -> String {
    format!(
        "CE\nGW 1 11 0 0 0 0 0 2 0.001\nGW 2 11 0 0 2 1.5 0 3.5 0.001\n\
         GW 3 11 0 0 2 -1.5 0 3.5 0.001\nGE 0\nEX 0 {tag} {seg} 0 1 0\nFR 0 1 0 0 29.98 0\nEN\n"
    )
}

/// What a deck is called, its extra arguments, the deck, and its feedpoint.
type Case<'a> = (&'a str, &'a [&'a str], String, (u32, u32));

/// Every Hallén route answers a source or port at a free end — none is the
/// null solution, and none is the #562 refusal any more. The bound separates an
/// answer from the 10¹⁰ Ω null; accuracy is gated below.
#[test]
fn a_source_at_a_free_wire_end_solves_on_every_hallen_route() {
    let bent = "CE\nGW 1 11 -2 0 0 0 0 1.5 0.001\nGW 2 11 0 0 1.5 2 0 0 0.001\nGE 0\n\
                EX 0 1 1 0 1 0\nFR 0 1 0 0 29.98 0\nEN\n";
    let tl_port = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 2 21 1 0 -2.5 1 0 2.5 0.001\nGE 0\n\
                   EX 0 1 11 0 1 0\nTL 1 1 2 21 50 0 0 0 0 0\nFR 0 1 0 0 29.98 0\nEN\n";
    let cases: [Case; 8] = [
        ("first segment", &[], wire(21, "EX 0 1 1 0 1 0"), (1, 1)),
        ("last segment", &[], wire(21, "EX 0 1 21 0 1 0"), (1, 21)),
        ("finer mesh", &[], wire(81, "EX 0 1 1 0 1 0"), (1, 1)),
        ("current source", &[], wire(21, "EX 4 1 1 0 1 0"), (1, 1)),
        (
            "sinusoidal basis",
            &["--solver", "sinusoidal"],
            wire(21, "EX 0 1 1 0 1 0"),
            (1, 1),
        ),
        ("bent path end", &[], bent.to_string(), (1, 1)),
        ("TL port at an end", &[], tl_port.to_string(), (1, 11)),
        ("Y arm tip (section graph)", &[], y_junction(2, 11), (2, 11)),
    ];
    for (what, args, deck, (tag, seg)) in cases {
        let (ok, stdout, stderr) = run(args, &deck);
        assert!(ok, "{what}: must solve:\n{stderr}");
        assert!(!stderr.contains(REFUSAL), "{what}: refused:\n{stderr}");
        let z = z_at(&stdout, tag, seg);
        assert!(
            z.0.hypot(z.1) < 1e5,
            "{what}: |Z| = {:.3e} is the null",
            z.0.hypot(z.1)
        );
    }
}

/// The refined end is a mesh like any other: the end-fed answer agrees with the
/// same wire meshed three times finer, the gap on segment 2 (the same point),
/// and the difference closes as the mesh does — slowly, to about a percent. An
/// impedance this near a free tip converges slowly in every code (held at one
/// gap position, Hallén, the MPIE and nec2c all still move at 735 segments), and
/// the two meshes differ near the tip by a factor of three. Measured, the split
/// N-segment wire against the uniform 3N one, at 11 / 21 / 41 / 81 segments:
/// 0.5 λ, 1 mm 3.40 / 1.97 / 1.44 / 1.23 %; the worst 0.5 λ (5 mm) 3.55 / 2.29 /
/// 1.76 / 1.11 %; 1 λ 6.5 / 2.9–3.1 / 1.5–2.1 / 1.0–1.6 %; 0.3 λ 0.2–0.9 %.
#[test]
fn an_end_fed_wire_agrees_with_the_mesh_three_times_finer() {
    let err = |n: usize| {
        let (ok, split, e) = run(&["--exec", "cpu"], &wire(n, "EX 0 1 1 0 1 0"));
        assert!(ok, "{e}");
        let (ok, fine, e) = run(&["--exec", "cpu"], &wire(3 * n, "EX 0 1 2 0 1 0"));
        assert!(ok, "{e}");
        rel(z_at(&split, 1, 1), z_at(&fine, 1, 2))
    };
    let (e21, e41) = (err(21), err(41));
    println!(
        "end-fed vs 3N: {:.3} % at 21, {:.3} % at 41",
        e21 * 100.0,
        e41 * 100.0
    );
    assert!(e21 < 0.025, "h = λ/42: {:.3} %", e21 * 100.0);
    assert!(e41 < 0.02, "h = λ/82: {:.3} %", e41 * 100.0);
    assert!(e41 < e21, "must close with the mesh: {e21:.4} -> {e41:.4}");
}

/// The device and the hybrid lane solve the refined mesh too, within the f32
/// device's tolerance of the CPU.
#[test]
fn an_end_fed_wire_solves_alike_on_every_exec() {
    let deck = wire(201, "EX 0 1 1 0 1 0");
    let (ok, cpu, e) = run(&["--exec", "cpu"], &deck);
    assert!(ok, "{e}");
    let z_cpu = z_at(&cpu, 1, 1);
    for exec in ["gpu", "hybrid"] {
        let (ok, out, e) = run(&["--exec", exec], &deck);
        assert!(ok, "--exec {exec}: {e}");
        let z = z_at(&out, 1, 1);
        assert!(
            rel(z, z_cpu) < 2e-3,
            "--exec {exec}: {z:?} against the CPU's {z_cpu:?}"
        );
    }
}

/// The worker (remote hosts, the GUI, fnec_py) answers what the CLI answers.
#[test]
fn the_worker_solves_an_end_fed_wire_as_the_cli_does() {
    let deck = wire(21, "EX 0 1 1 0 1 0");
    let (ok, out, e) = run(&["--exec", "cpu"], &deck);
    assert!(ok, "{e}");
    let cli = z_at(&out, 1, 1);
    let r = nec_worker::solve::solve_deck_at_frequency_with_exec(&deck, 29.98e6, "hallen", "cpu")
        .expect("the worker solves it");
    assert!(
        rel((r.impedance_re, r.impedance_im), cli) < 1e-6,
        "worker {} + j{} against the CLI's {cli:?}",
        r.impedance_re,
        r.impedance_im
    );
}

/// FND-230: a lumped load on a free-end segment is seen. It did nothing — 1000 Ω
/// on the end of a centre-fed 21-segment wire left 78.016 + j36.530 unchanged,
/// where nec2c moves 79.66 + j45.15 → 87.91 + j42.05 (ΔZ = 8.25 − j3.10).
#[test]
fn a_load_on_a_free_end_segment_is_seen() {
    let z = |ld: &str| {
        let (ok, out, e) = run(
            &["--exec", "cpu"],
            &wire(21, &format!("{ld}EX 0 1 11 0 1 0")),
        );
        assert!(ok, "{e}");
        z_at(&out, 1, 11)
    };
    let (bare, loaded) = (z(""), z("LD 0 1 1 1 1000 0\n"));
    let dz = (loaded.0 - bare.0, loaded.1 - bare.1);
    let nec2c = (8.25, -3.10);
    println!("ΔZ = {:.3} {:+.3}j, nec2c {nec2c:?}", dz.0, dz.1);
    assert!(rel(dz, nec2c) < 0.15, "ΔZ {dz:?} against nec2c's {nec2c:?}");
}

/// A refined end's outer thirds carry segment number 0, which no card can name:
/// `EX 0 1 0` stays the refusal it always was, not a feed on a flank.
#[test]
fn segment_zero_names_nothing_on_a_refined_wire() {
    let (ok, stdout, stderr) = run(&[], &wire(21, "EX 0 1 0 0 1 0"));
    assert!(!ok, "EX on segment 0 must not solve:\n{stdout}");
    assert!(stderr.contains("index 0"), "{stderr}");
}

/// A feed on a short, fat end segment is judged by the deck's segment, not its
/// centre third: length/radius 5 passes the 2.0 source-risk floor, a third of it
/// (1.67) would not.
#[test]
fn a_fat_end_segment_is_judged_by_its_deck_length() {
    // 5 segments of 0.1 m on a 0.02 m radius: h/r = 5.
    let deck = "CE\nGW 1 5 0 0 0 0 0 0.5 0.02\nGE 0\nEX 0 1 1 0 1 0\nFR 0 1 0 0 150 0\nEN\n";
    let (ok, stdout, stderr) = run(&[], deck);
    assert!(ok, "a feed the deck allows must not be refused:\n{stderr}");
    let z = z_at(&stdout, 1, 1);
    assert!(z.0.is_finite() && z.0 > 0.0, "{z:?}");
}

/// The MPIE does not take the refinement (`SolverMode::geometry`): its end-feed
/// refusal (FND-228) stands.
#[test]
fn the_mpie_still_refuses_a_feed_at_a_free_wire_end() {
    let (ok, stdout, stderr) = run(&["--solver", "mpie"], &wire(21, "EX 0 1 1 0 1 0"));
    assert!(!ok, "{stdout}");
    assert!(stderr.contains("free wire end"), "{stderr}");
}

/// The CURRENTS block lists the deck's segments — one row each, numbered 1 to N
/// — and not the thirds a refined end is solved on: a flank is numbered 0 and is
/// no segment the deck has. The end row's current is the centre third's, at the
/// deck segment's midpoint.
#[test]
fn the_currents_block_lists_the_decks_segments() {
    let (ok, stdout, stderr) = run(&[], &wire(21, "EX 0 1 11 0 1 0"));
    assert!(ok, "{stderr}");
    let block: Vec<&str> = stdout
        .lines()
        .skip_while(|l| *l != "CURRENTS")
        .skip(2)
        .take_while(|l| !l.trim().is_empty())
        .collect();
    let segs: Vec<u32> = block
        .iter()
        .map(|l| {
            l.split_whitespace()
                .nth(1)
                .expect("SEG")
                .parse()
                .expect("number")
        })
        .collect();
    assert_eq!(segs, (1..=21).collect::<Vec<_>>(), "{}", block.join("\n"));
}

/// The FND-294 caveat's own words, unique to it.
const FREE_END_CAVEAT: &str = "at a free wire end; a feed there is singular";

/// The audit's end-fed wire (`ef5_*`): 5 m of 1 mm wire at 29.9 MHz, `n` segments,
/// `drive` on it.
fn ef5(n: usize, drive: &str) -> String {
    format!("CE\nGW 1 {n} 0 0 0 0 0 5 0.001\nGE 0\n{drive}\nFR 0 1 0 0 29.9 0\nEN\n")
}

/// FND-294: a feed on a free-end segment is answered (exit 0), with a caveat that
/// the answer is the mesh's, not the antenna's. Fed on segment 1 the impedance
/// does not converge in either code — fnec Hallén 1839.26 − j1838.77 /
/// 1244.24 − j5690.26 / 746.32 − j11638.39 at 11 / 33 / 99 segments, nec2c 1.3.1
/// 3078.4 − j1554.2 / 2793.4 − j8345.2 / 1700.8 − j17519 — while a feed at a
/// fixed interior point 0.68 m in (segment 2 / 5 / 14) converges in both: fnec
/// 490.85 + j66.84 / 501.49 + j76.16 / 507.01 + j79.61, nec2c 493.96 + j107.45 /
/// 503.41 + j93.65 / 508.14 + j87.75. The test asserts the caveat, not the numbers.
#[test]
fn a_feed_at_a_free_wire_end_is_caveated() {
    let cases: [(&str, String, (u32, u32)); 3] = [
        ("first segment", ef5(11, "EX 0 1 1 0 1 0"), (1, 1)),
        ("last segment", ef5(11, "EX 0 1 11 0 1 0"), (1, 11)),
        ("current source", ef5(11, "EX 4 1 1 0 1 0"), (1, 1)),
    ];
    for (what, deck, (tag, seg)) in cases {
        let (ok, stdout, stderr) = run(&[], &deck);
        assert!(ok, "{what}: a caveat, not a refusal (D10):\n{stderr}");
        z_at(&stdout, tag, seg);
        let want = format!("feedpoint tag {tag} segment {seg} is on a segment {FREE_END_CAVEAT}");
        assert!(stderr.contains(&want), "{what}: no caveat:\n{stderr}");
        assert_eq!(
            stderr.matches(FREE_END_CAVEAT).count(),
            1,
            "{what}: {stderr}"
        );
    }
}

/// Negative controls for the caveat, one per way a feed can sit near an end that
/// is not free. Each must also solve and report its feedpoint, so the absence is
/// not that of a run that never got there.
#[test]
fn a_feed_in_from_a_free_end_is_not_caveated() {
    // A base-fed monopole: the end on the ground joins its image. Finite ground
    // joins it too (`ground_joins_wire_ends`, unit-tested in `geometry.rs`), but a
    // wire touching GN 0/2 is refused before any caveat (FND-240).
    let monopole = |gn: &str| {
        format!(
            "CE\nGW 1 26 0 0 0 0 0 2.5 0.001\nGE 1\n{gn}\nEX 0 1 1 0 1 0\nFR 0 1 0 0 29.98 0\nEN\n"
        )
    };
    let invv = "CE\nGW 1 11 -2 0 8 0 0 10 0.001\nGW 2 11 0 0 10 2 0 8 0.001\nGE 1\nGN 1\n\
                EX 0 1 11 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";
    let tee = "CE\nGW 1 11 0 0 0 0 0 3 0.001\nGW 2 11 0 0 3 -2 0 3 0.001\n\
               GW 3 11 0 0 3 2 0 3 0.001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";
    // A passive port: the line joins wire 2's end to wire 1's end segment, and no
    // feed is on the network (FND-311 judges only a port the network drives).
    let tl_port = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 2 21 1 0 -2.5 1 0 2.5 0.001\nGE 0\n\
                   EX 0 1 11 0 1 0\nTL 1 1 2 21 50 0 0 0 0 0\nFR 0 1 0 0 29.98 0\nEN\n";
    let cases: Vec<(&str, String, (u32, u32))> = vec![
        ("interior feed 0.68 m in", ef5(11, "EX 0 1 2 0 1 0"), (1, 2)),
        ("centre-fed dipole", wire(21, "EX 0 1 11 0 1 0"), (1, 11)),
        ("monopole over GN 1", monopole("GN 1"), (1, 1)),
        ("inverted-V apex (bend)", invv.to_string(), (1, 11)),
        ("tee stem top (junction)", tee.to_string(), (1, 11)),
        ("TL port at a free end", tl_port.to_string(), (1, 11)),
    ];
    for (what, deck, (tag, seg)) in cases {
        let (ok, stdout, stderr) = run(&[], &deck);
        assert!(ok, "{what}: must solve:\n{stderr}");
        z_at(&stdout, tag, seg);
        assert!(
            !stderr.contains(FREE_END_CAVEAT) && !stderr.contains(DRIVEN_PORT_CAVEAT),
            "{what}: caveated:\n{stderr}"
        );
    }
}

/// The FND-311 caveat's own words, unique to it.
const DRIVEN_PORT_CAVEAT: &str = "carries the drive from feedpoint";

/// FND-311's 10 m wire at 14.2 MHz, its far end on segment 1, beside a short
/// 5-segment source wire 100 m away (tag 3), with `cards` on them.
fn end_fed_through(cards: &str) -> String {
    format!(
        "CE\nGW 1 11 0 0 -5 0 0 5 0.001\nGW 3 5 100 0 -0.25 100 0 0.25 0.001\nGE 0\n\
         {cards}FR 0 1 0 0 14.2 0\nEN\n"
    )
}

/// FND-311: a `TL`/`NT` port on a free-end segment whose network carries a feed
/// to it is a free-end feed — an end-fed wire through a feedline or a matching
/// network. nec2c's impedance at that port equals the direct end feed's to four
/// digits at every mesh (10 m wire, 11 → 297 segments: 1273 − j1610 … 1954 −
/// j44452 Ω), and the source behind the `NT` below diverges in both codes: fnec
/// 673 − j1362 / 575 − j3686 / 218 − j6008 / 55 − j7549 Ω at 1× / 3× / 9× /
/// 27×, nec2c 1011 − j1525 / 904 − j4548 / 303 − j7029 / 69 − j8376 Ω
/// (`~/.cache/swap/fnd311-measurement.md`; Fable reproduced it). Caveated once
/// per port, by the port's own segment, and not as a feed. A chain counts: a line
/// to a port that a network joins to the feed.
#[test]
fn a_port_driven_through_a_network_at_a_free_end_is_caveated() {
    let cases: [(&str, String); 3] = [
        (
            "feedline (TL)",
            end_fed_through("EX 0 3 3 0 1 0\nTL 1 1 3 3 50 3.0 0 0 0 0\n"),
        ),
        (
            "matching network (NT)",
            end_fed_through("EX 0 3 3 0 1 0\nNT 1 1 3 3 1 0 -1 0 1 0\n"),
        ),
        (
            "line to a network on the feed",
            end_fed_through("EX 0 3 3 0 1 0\nTL 1 1 3 2 50 3.0 0 0 0 0\nNT 3 2 3 3 1 0 -1 0 1 0\n"),
        ),
    ];
    for (what, deck) in cases {
        let (ok, stdout, stderr) = run(&[], &deck);
        assert!(ok, "{what}: a caveat, not a refusal (D10):\n{stderr}");
        z_at(&stdout, 3, 3);
        let want = format!(
            "network port tag 1 segment 1 is on a segment at a free wire end and \
             {DRIVEN_PORT_CAVEAT} tag 3 segment 3"
        );
        assert!(stderr.contains(&want), "{what}: no caveat:\n{stderr}");
        assert_eq!(
            stderr.matches(DRIVEN_PORT_CAVEAT).count(),
            1,
            "{what}: {stderr}"
        );
        assert!(
            !stderr.contains(FREE_END_CAVEAT),
            "{what}: judged as a feed:\n{stderr}"
        );
    }
}

/// FND-311's negative controls: a port at a free end that no network drives, or a
/// driven network whose port is in from the end. Each solves and reports its
/// feedpoint beside the absence.
#[test]
fn a_port_no_network_drives_or_in_from_the_end_is_not_caveated() {
    let monopole_port = "CE\nGW 1 26 0 0 0 0 0 2.5 0.001\nGW 3 5 10 0 4.75 10 0 5.25 0.001\nGE 1\n\
                         GN 1\nEX 0 3 3 0 1 0\nTL 1 1 3 3 50 3.0 0 0 0 0\nFR 0 1 0 0 29.98 0\nEN\n";
    let cases: Vec<(&str, String)> = vec![
        // The driven feedline with its port moved to segment 3, 2.7 m in: settles
        // in both codes.
        (
            "driven port in from the end",
            end_fed_through("EX 0 3 3 0 1 0\nTL 1 3 3 3 50 3.0 0 0 0 0\n"),
        ),
        // An NT with Y12 = 0 is two one-ports: the free-end port is a shunt
        // admittance only, and carries nothing from the feed (FND-230's case).
        (
            "NT with Y12 = 0",
            end_fed_through("EX 0 3 3 0 1 0\nNT 1 1 3 3 1e-5 0 0 0 0 0\n"),
        ),
        // A line to a port that a coupling network joins to another port, the feed
        // on neither: passive.
        (
            "network chain without the feed",
            end_fed_through("EX 0 3 3 0 1 0\nTL 1 1 3 2 50 3.0 0 0 0 0\nNT 3 2 3 4 1 0 -1 0 1 0\n"),
        ),
        // The port on a monopole's base: that end joins its image over GN 1.
        ("driven port on a grounded base", monopole_port.to_string()),
    ];
    for (what, deck) in cases {
        let (ok, stdout, stderr) = run(&[], &deck);
        assert!(ok, "{what}: must solve:\n{stderr}");
        z_at(&stdout, 3, 3);
        assert!(
            !stderr.contains(FREE_END_CAVEAT) && !stderr.contains(DRIVEN_PORT_CAVEAT),
            "{what}: caveated:\n{stderr}"
        );
    }
}

/// The MPIE refuses a free-end feed (FND-228) by the same predicate; it does not
/// answer with the Hallén caveat instead.
#[test]
fn the_mpie_refuses_the_caveated_feed_rather_than_caveating_it() {
    let (ok, stdout, stderr) = run(&["--solver", "mpie"], &ef5(11, "EX 0 1 1 0 1 0"));
    assert!(!ok, "{stdout}");
    assert!(
        stderr.contains("which the MPIE's basis cannot drive"),
        "{stderr}"
    );
    assert!(!stderr.contains(FREE_END_CAVEAT), "{stderr}");
    // And takes the interior feed the caveat passes.
    let (ok, stdout, stderr) = run(&["--solver", "mpie"], &ef5(11, "EX 0 1 2 0 1 0"));
    assert!(ok, "{stderr}");
    z_at(&stdout, 1, 2);
}

/// `fnec converge` prints each mesh's caveats with the mesh's prefix; a free-end
/// feed is a free-end feed on every mesh.
#[test]
fn converge_caveats_a_free_end_feed_on_every_mesh() {
    let (ok, _stdout, stderr) = run(&["converge"], &ef5(11, "EX 0 1 1 0 1 0"));
    assert!(ok, "{stderr}");
    for mesh in ["mesh 3x: ", "mesh 9x: "] {
        assert!(
            stderr
                .lines()
                .any(|l| l.starts_with(mesh) && l.contains(FREE_END_CAVEAT)),
            "no {mesh:?} caveat:\n{stderr}"
        );
    }
}

/// FND-311 through `fnec converge`, which judges every mesh's own geometry: with
/// `--ends keep` the port stays on the end segment and every mesh is caveated;
/// with `--ends centre` it moves in from the end (1×: 1, 3×: 2, 9×: 5) and only
/// the 1× mesh is — while the source R settles, 1.141 → 1.215 → 1.252 Ω,
/// against keep's 1.141 → 0.166 → 0.024 Ω. A judge that read the 1× deck's port
/// for every mesh fails the second half.
#[test]
fn converge_judges_the_driven_port_on_each_mesh() {
    let deck = end_fed_through("EX 0 3 3 0 1 0\nTL 1 1 3 3 50 3.0 0 0 0 0\n");
    for (ends, meshes) in [("keep", vec!["1x", "3x", "9x"]), ("centre", vec!["1x"])] {
        let (ok, stdout, stderr) = run(&["converge", "--ends", ends], &deck);
        assert!(
            ok && stdout.contains("MESH 9 "),
            "{ends}:\n{stderr}\n{stdout}"
        );
        let caveated: Vec<&str> = stderr
            .lines()
            .filter(|l| l.contains(DRIVEN_PORT_CAVEAT))
            .map(|l| l.split(':').next().unwrap().trim_start_matches("mesh "))
            .collect();
        assert_eq!(caveated, meshes, "{ends}:\n{stderr}");
    }
}

/// A feed on the free-end port itself is FND-294's case, caveated once as a feed,
/// not a second time as a driven port.
#[test]
fn a_feed_on_the_free_end_port_is_caveated_once_as_a_feed() {
    let deck = end_fed_through("EX 0 1 1 0 1 0\nTL 1 1 3 3 50 3.0 0 0 0 0\n");
    let (ok, stdout, stderr) = run(&[], &deck);
    assert!(ok, "{stderr}");
    z_at(&stdout, 1, 1);
    assert_eq!(stderr.matches(FREE_END_CAVEAT).count(), 1, "{stderr}");
    assert!(!stderr.contains(DRIVEN_PORT_CAVEAT), "{stderr}");
}
