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
