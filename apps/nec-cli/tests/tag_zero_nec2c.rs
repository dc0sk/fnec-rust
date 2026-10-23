// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-282 — tag 0 on `EX`, `LD`, `TL`, `NT` and `PT` is an absolute segment
//! number, as NEC-2 defines it and nec2c's `isegno`/`load()` read it. fnec read it
//! as "the wire tagged 0" (a point reference) or "every tag" (a range), so
//! `EX 0 0 32` on two 21-segment dipoles was refused where nec2c feeds segment 32,
//! `LD 4 0 11 11` loaded segment 11 of both wires, and a `GW 0` wire took the feed
//! meant for the first wire's 11th segment.
//!
//! The strongest gate needs no tolerance: a tag-0 reference must give the same
//! answer as the tagged reference naming the same segment, to the last digit,
//! on every solver and through the PEC image problem. nec2c 1.3.1 (measured
//! 2026-10-23, 29.9 MHz) agrees on the identities and gives the values quoted in
//! each test; those are checked within the Hallén band the tagged decks already
//! hold against nec2c (a few per cent), so a wrong *segment* — the only failure
//! this gate exists for — is caught, not a solver drift.
//!
//! FND-299 (`PT`'s range through the same resolver) rides along, being the same
//! rule; FND-292 (negative `LD` types) is pinned in `ld_loads.rs`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Run {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run(deck: &str, name: &str, args: &[&str]) -> Run {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("fnec-tag0-{name}-{now}.nec"));
    fs::write(&path, deck).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .current_dir(&root)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    Run {
        status: out.status,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// The first FEEDPOINTS row's `(tag, seg, Z)`.
fn feed(deck: &str, name: &str, args: &[&str]) -> (u32, u32, f64, f64) {
    let r = run(deck, name, args);
    assert!(r.status.success(), "{name}: {}", r.stderr);
    let mut lines = r.stdout.lines();
    lines
        .find(|l| l.trim() == "FEEDPOINTS")
        .expect("FEEDPOINTS");
    lines.next();
    let cols: Vec<&str> = lines.next().expect("a row").split_whitespace().collect();
    (
        cols[0].parse().unwrap(),
        cols[1].parse().unwrap(),
        cols[6].parse().unwrap(),
        cols[7].parse().unwrap(),
    )
}

fn refused(deck: &str, name: &str) -> String {
    let r = run(deck, name, &["--exec", "cpu"]);
    assert!(!r.status.success(), "{name} solved:\n{}", r.stdout);
    r.stderr
}

fn same(a: (u32, u32, f64, f64), b: (u32, u32, f64, f64), what: &str) {
    assert_eq!((a.0, a.1), (b.0, b.1), "{what}: different segment");
    assert!(
        (a.2 - b.2).abs() < 1e-9 && (a.3 - b.3).abs() < 1e-9,
        "{what}: {} + j{} vs {} + j{}",
        a.2,
        a.3,
        b.2,
        b.3
    );
}

/// Within the band the tagged deck already holds against nec2c: a wrong segment
/// moves Z by tens of ohms or more on every deck here.
fn near_nec2c(z: (f64, f64), nec2c: (f64, f64), what: &str) {
    let d = ((z.0 - nec2c.0).powi(2) + (z.1 - nec2c.1).powi(2)).sqrt();
    let scale = (nec2c.0.powi(2) + nec2c.1.powi(2)).sqrt();
    assert!(
        d <= 0.08 * scale + 0.5,
        "{what}: {} + j{} against nec2c {} + j{}",
        z.0,
        z.1,
        nec2c.0,
        nec2c.1
    );
}

const CPU: &[&str] = &["--exec", "cpu"];
const TWO: &str = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 2 21 1 0 -2.5 1 0 2.5 0.001\nGE 0\n";
const TAIL: &str = "FR 0 1 0 0 29.9 0\nEN\n";

fn two(cards: &str) -> String {
    format!("{TWO}{cards}{TAIL}")
}

/// `EX 0 0 32` is the second wire's 11th segment (nec2c: tag 2 seg 32,
/// 22.282 + j60.142), on Hallén, the sinusoidal basis and the MPIE alike.
/// Segment 43 does not exist, and the message says what tag 0 means.
#[test]
fn ex_tag_zero_feeds_the_absolute_segment() {
    for solver in ["hallen", "sinusoidal", "mpie"] {
        let args = ["--solver", solver, "--exec", "cpu"];
        let abs = feed(&two("EX 0 0 32 0 1 0\n"), "ex-abs", &args);
        let tagged = feed(&two("EX 0 2 11 0 1 0\n"), "ex-tagged", &args);
        same(abs, tagged, &format!("{solver}: EX 0 0 32 vs EX 0 2 11"));
        assert_eq!((abs.0, abs.1), (2, 11));
    }
    let abs = feed(&two("EX 0 0 32 0 1 0\n"), "ex-abs", CPU);
    near_nec2c((abs.2, abs.3), (22.282, 60.142), "EX 0 0 32");
    let e = refused(&two("EX 0 0 43 0 1 0\n"), "ex-43");
    assert!(
        e.contains("no segment numbered 43") && e.contains("absolute segment number"),
        "{e}"
    );
    let e = refused(&two("EX 0 0 0 0 1 0\n"), "ex-0-0");
    assert!(e.contains("segment 0 names none"), "{e}");
}

/// With a wire tagged 0 present, `EX 0 0 11` is still the 11th segment built —
/// the first wire's — not the tag-0 wire's. nec2c: tag 1 seg 11, 22.282 + j60.142
/// on the symmetric pair; on the asymmetric one 76.728 + j28.276 where fnec's old
/// reading gave 13.265 − j219.94 (the 4 m wire).
#[test]
fn a_wire_tagged_zero_does_not_capture_a_tag_zero_reference() {
    let sym = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 0 21 1 0 -2.5 1 0 2.5 0.001\nGE 0\n";
    let a = feed(&format!("{sym}EX 0 0 11 0 1 0\n{TAIL}"), "zero-sym", CPU);
    let b = feed(
        &format!("{sym}EX 0 1 11 0 1 0\n{TAIL}"),
        "zero-sym-tagged",
        CPU,
    );
    same(a, b, "GW 0 present: EX 0 0 11 vs EX 0 1 11");
    assert_eq!((a.0, a.1), (1, 11));
    let asym = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGW 0 21 1 0 -2.0 1 0 2.0 0.001\nGE 0\n";
    let a = feed(&format!("{asym}EX 0 0 11 0 1 0\n{TAIL}"), "zero-asym", CPU);
    near_nec2c((a.2, a.3), (76.728, 28.276), "asymmetric pair, EX 0 0 11");
    // The tag-0 wire's own 11th is absolute 32.
    let c = feed(&format!("{sym}EX 0 0 32 0 1 0\n{TAIL}"), "zero-32", CPU);
    assert_eq!((c.0, c.1), (0, 11));
}

/// `LD` with tag 0: a single absolute segment, a range spanning two wires, a
/// type-5 range, and the controls NEC-2 refuses. nec2c: `LD 4 0 11 11 0 1000`
/// 22.282 + j1060.1; `LD 4 0 32 32 0 1000` 79.088 + j44.105; `LD 4 0 20 24 10 0`
/// 24.030 + j59.051 (= its `LD 4 1 20 21` + `LD 4 2 1 3`); `LD 5 0 20 24 1e4`
/// 23.462 + j60.407; `LD 4 1 0 5 10 0` is all of tag 1, 134.81 + j53.094.
#[test]
fn ld_tag_zero_loads_absolute_segments() {
    let fed = |ld: &str| two(&format!("EX 0 1 11 0 1 0\n{ld}\n"));
    for solver in ["hallen", "sinusoidal", "mpie"] {
        let args = ["--solver", solver, "--exec", "cpu"];
        same(
            feed(&fed("LD 4 0 11 11 0 1000"), "ld-11", &args),
            feed(&fed("LD 4 1 11 11 0 1000"), "ld-11-tagged", &args),
            &format!("{solver}: LD 4 0 11 11"),
        );
        same(
            feed(&fed("LD 4 0 32 32 0 1000"), "ld-32", &args),
            feed(&fed("LD 4 2 11 11 0 1000"), "ld-32-tagged", &args),
            &format!("{solver}: LD 4 0 32 32"),
        );
        same(
            feed(&fed("LD 4 0 20 24 10 0"), "ld-span", &args),
            feed(
                &fed("LD 4 1 20 21 10 0\nLD 4 2 1 3 10 0"),
                "ld-span-split",
                &args,
            ),
            &format!("{solver}: LD 4 0 20 24"),
        );
        same(
            feed(&fed("LD 5 0 20 24 1e4"), "ld5-span", &args),
            feed(
                &fed("LD 5 1 20 21 1e4\nLD 5 2 1 3 1e4"),
                "ld5-span-split",
                &args,
            ),
            &format!("{solver}: LD 5 0 20 24"),
        );
    }
    let z = feed(&fed("LD 4 0 11 11 0 1000"), "ld-11", CPU);
    near_nec2c((z.2, z.3), (22.282, 1060.1), "LD 4 0 11 11 0 1000");
    let z = feed(&fed("LD 4 0 32 32 0 1000"), "ld-32", CPU);
    near_nec2c((z.2, z.3), (79.088, 44.105), "LD 4 0 32 32 0 1000");
    let z = feed(&fed("LD 4 0 20 24 10 0"), "ld-span", CPU);
    near_nec2c((z.2, z.3), (24.030, 59.051), "LD 4 0 20 24 10 0");
    // A tagged card with seg_first 0 is every segment of the tag, last unread.
    same(
        feed(&fed("LD 4 1 0 5 10 0"), "ld-tag-all", CPU),
        feed(&fed("LD 4 1 0 0 10 0"), "ld-tag-all-0", CPU),
        "LD 4 1 0 5 vs LD 4 1 0 0",
    );
    let z = feed(&fed("LD 4 1 0 5 10 0"), "ld-tag-all", CPU);
    near_nec2c((z.2, z.3), (134.81, 53.094), "LD 4 1 0 5 10 0");
    // Negative controls: segment 0 with tag 0, a reversed range (nec2c's DATA
    // FAULT), a load past the last segment.
    let e = refused(&fed("LD 4 0 0 5 10 0"), "ld-0-5");
    assert!(
        e.contains("LD 4 0 0 5:") && e.contains("segment 0 names none"),
        "{e}"
    );
    let e = refused(&fed("LD 4 0 24 20 10 0"), "ld-rev");
    assert!(e.contains("DATA FAULT"), "{e}");
    let e = refused(&fed("LD 4 0 43 43 10 0"), "ld-43");
    assert!(e.contains("names no segment"), "{e}");
}

/// `TL`/`NT` ports with tag 0 (nec2c: `TL 0 11 0 32 …` 50.991 − j34.802 and
/// `NT 0 11 0 32 …` 28.371 + j18.630, each equal to its tagged twin). `(0, 0)` is
/// refused: tag 0 has no centre for fnec's segment-0 shorthand, and nec2c stops.
#[test]
fn tl_and_nt_ports_with_tag_zero_are_absolute() {
    let fed = |net: &str| two(&format!("EX 0 1 11 0 1 0\n{net}\n"));
    let a = feed(&fed("TL 0 11 0 32 50 1 0 0 0 0"), "tl-abs", CPU);
    let tl_tagged = feed(&fed("TL 1 11 2 11 50 1 0 0 0 0"), "tl-tagged", CPU);
    same(a, tl_tagged, "TL 0 11 0 32 vs TL 1 11 2 11");
    near_nec2c((a.2, a.3), (50.991, -34.802), "TL 0 11 0 32");
    let a = feed(&fed("NT 0 11 0 32 .01 -.02 0 .015 .01 -.02"), "nt-abs", CPU);
    let b = feed(
        &fed("NT 1 11 2 11 .01 -.02 0 .015 .01 -.02"),
        "nt-tagged",
        CPU,
    );
    same(a, b, "NT 0 11 0 32 vs NT 1 11 2 11");
    near_nec2c((a.2, a.3), (28.371, 18.630), "NT 0 11 0 32");
    let e = refused(&fed("TL 0 0 2 11 50 1 0 0 0 0"), "tl-0-0");
    assert!(e.contains("segment 0 names none"), "{e}");
    let e = refused(&fed("NT 0 0 2 11 .01 -.02 0 .015 .01 -.02"), "nt-0-0");
    assert!(e.contains("segment 0 names none"), "{e}");
    // The tagged shorthand still names the centre.
    let c = feed(&fed("TL 1 0 2 11 50 1 0 0 0 0"), "tl-centre", CPU);
    same(
        c,
        tl_tagged,
        "TL 1 0 2 11 (centre shorthand) vs TL 1 11 2 11",
    );
}

/// NEC's absolute numbering is the build order, through `GM`, `GR` and `GX` copies
/// (nec2c: `GM 0 1 … 1` + `EX 0 0 32` 22.282 + j60.142 = `EX 0 1 32`; `GR 0 4` on an
/// 11-segment wire, `EX 0 0 17` 13.415 − j758.82 = `EX 0 1 17`; `GX 10 100`,
/// `EX 0 0 32` 68.205 + j79.178 = `EX 0 11 11`).
#[test]
fn absolute_numbers_follow_the_build_order_through_copies() {
    let cases = [
        (
            "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGM 0 1 0 0 0 1 0 0 1\nGE 0\n",
            "EX 0 0 32 0 1 0\n",
            "EX 0 1 32 0 1 0\n",
            (22.282, 60.142),
            "GM copy",
        ),
        (
            "CE\nGW 1 11 1 0 -1.25 1 0 1.25 0.001\nGR 0 4\nGE 0\n",
            "EX 0 0 17 0 1 0\n",
            "EX 0 1 17 0 1 0\n",
            (13.415, -758.82),
            "GR ring",
        ),
        (
            "CE\nGW 1 21 1 0 -2.5 1 0 2.5 0.001\nGX 10 100\nGE 0\n",
            "EX 0 0 32 0 1 0\n",
            "EX 0 11 11 0 1 0\n",
            (68.205, 79.178),
            "GX mirror",
        ),
    ];
    for (geometry, abs, tagged, nec2c, what) in cases {
        let a = feed(&format!("{geometry}{abs}{TAIL}"), "copy-abs", CPU);
        let b = feed(&format!("{geometry}{tagged}{TAIL}"), "copy-tagged", CPU);
        same(a, b, what);
        near_nec2c((a.2, a.3), nec2c, what);
    }
}

/// Hallén refines a free end into three (FND-227); the flanks are not the deck's
/// and take no absolute number, so `EX 0 0 1` is the end segment itself and
/// `LD 4 0 21 21` / `LD 4 0 22 22` the last segment of wire 1 and the first of
/// wire 2 — not a flank, and not a number shifted by the flanks before it.
#[test]
fn absolute_numbers_skip_the_free_end_flanks() {
    let one = "CE\nGW 1 21 0 0 -2.5 0 0 2.5 0.001\nGE 0\n";
    same(
        feed(&format!("{one}EX 0 0 1 0 1 0\n{TAIL}"), "end-abs", CPU),
        feed(&format!("{one}EX 0 1 1 0 1 0\n{TAIL}"), "end-tagged", CPU),
        "EX 0 0 1 vs EX 0 1 1 (refined end)",
    );
    let fed = |ld: &str| two(&format!("EX 0 1 11 0 1 0\n{ld}\n"));
    same(
        feed(&fed("LD 4 0 21 21 100 0"), "end-ld21", CPU),
        feed(&fed("LD 4 1 21 21 100 0"), "end-ld21-tagged", CPU),
        "LD 4 0 21 21 vs LD 4 1 21 21",
    );
    same(
        feed(&fed("LD 4 0 22 22 100 0"), "end-ld22", CPU),
        feed(&fed("LD 4 2 1 1 100 0"), "end-ld22-tagged", CPU),
        "LD 4 0 22 22 vs LD 4 2 1 1",
    );
}

/// A tag-0 reference crosses into the PEC image problem (nec2c: a λ/4 monopole
/// `EX 0 0 1` + `LD 4 0 5 5 100 0` on `GN 1` is 130.81 + j8.644, = the tagged deck).
#[test]
fn tag_zero_references_cross_the_pec_image() {
    let mono = "CE\nGW 1 21 0 0 0 0 0 2.5 0.001\nGE 1\nGN 1\n";
    let a = feed(
        &format!("{mono}EX 0 0 1 0 1 0\nLD 4 0 5 5 100 0\n{TAIL}"),
        "pec-abs",
        CPU,
    );
    let b = feed(
        &format!("{mono}EX 0 1 1 0 1 0\nLD 4 1 5 5 100 0\n{TAIL}"),
        "pec-tagged",
        CPU,
    );
    same(a, b, "PEC monopole: tag 0 vs tagged");
    near_nec2c(
        (a.2, a.3),
        (130.81, 8.644),
        "PEC monopole EX 0 0 1 + LD 4 0 5 5",
    );
    // A tag-0 TL on contact: both ports absolute, mirrored onto the images.
    let pair = "CE\nGW 1 21 0 0 0 0 0 2.5 0.001\nGW 2 21 1 0 0 1 0 2.5 0.001\nGE 1\nGN 1\n";
    let a = feed(
        &format!("{pair}EX 0 1 3 0 1 0\nTL 0 3 0 24 50 0 0 0 0 0\n{TAIL}"),
        "pec-tl-abs",
        CPU,
    );
    let b = feed(
        &format!("{pair}EX 0 1 3 0 1 0\nTL 1 3 2 3 50 0 0 0 0 0\n{TAIL}"),
        "pec-tl-tagged",
        CPU,
    );
    same(a, b, "PEC pair: TL 0 3 0 24 vs TL 1 3 2 3");
    // Loads either side of an inverted L's bend: the image deck's own `LD` cards
    // place the gaps at the image's bend (FND-270), so a tag-0 load must be
    // mirrored there as a tagged one is, not only into the doubled load vector.
    let ell = "CE\nGW 1 10 0 0 0 0 0 2.5 0.001\nGW 2 10 0 0 2.5 2.5 0 2.5 0.001\nGE 1\nGN 1\n";
    let a = feed(
        &format!("{ell}EX 0 1 1 0 1 0\nLD 4 0 10 11 50 0\n{TAIL}"),
        "pec-ell-abs",
        CPU,
    );
    let b = feed(
        &format!("{ell}EX 0 1 1 0 1 0\nLD 4 1 10 10 50 0\nLD 4 2 1 1 50 0\n{TAIL}"),
        "pec-ell-tagged",
        CPU,
    );
    same(a, b, "PEC inverted L: LD 4 0 10 11 vs the tagged pair");
    // The image deck's `LD` cards are what the FND-278 caveat reads: on a slanted
    // wire from the ground, a load on segment 1 and its image sit either side of
    // the ground node's bend. A tag-0 load must be mirrored there as the tagged
    // one is, or the caveat goes silent for it alone.
    let slant = "CE\nGW 1 11 0 0 0 1.5 0 2.0 0.001\nGE 1\nGN 1\nEX 0 1 6 0 1 0\n";
    let caveats = |ld: &str| -> Vec<String> {
        let r = run(
            &format!("{slant}{ld}FR 0 1 0 0 29.9 0\nEN\n"),
            "pec-slant",
            CPU,
        );
        assert!(r.status.success(), "{}", r.stderr);
        r.stderr
            .lines()
            .filter(|l| l.starts_with("warning:"))
            .map(str::to_string)
            .collect()
    };
    let tagged = caveats("LD 4 1 1 1 50 0\n");
    assert!(
        tagged.iter().any(|w| w.contains("FND-278")),
        "the control fires: {tagged:?}"
    );
    assert_eq!(caveats("LD 4 0 1 1 50 0\n"), tagged);
}

/// NEC-2D's "loaded twice" note is decided on the segments the cards resolve to
/// (nec2c 1.3.1: `LD 4 0 11 11` + `LD 4 2 11 11` load absolute 11 and 32, no
/// note; `LD 4 0 32 32` + `LD 4 2 11 11` both load 32, the note). fnec read
/// tag 0 as every tag and printed it for the first pair.
#[test]
fn the_loaded_twice_note_follows_the_resolved_segments() {
    let note = |lds: &str| {
        let r = run(
            &format!("{TWO}EX 0 1 11 0 1 0\n{lds}FR 0 1 0 0 29.9 0\nXQ\nEN\n"),
            "twice",
            &["--exec", "cpu", "--output-format", "nec2d"],
        );
        assert!(r.status.success(), "{}", r.stderr);
        r.stdout.contains("LOADED TWICE")
    };
    assert!(!note("LD 4 0 11 11 0 1000\nLD 4 2 11 11 0 1000\n"));
    assert!(note("LD 4 0 32 32 0 1000\nLD 4 2 11 11 0 1000\n"));
}

/// `PT` selects the printed currents by NEC's rule (FND-299): `PT 0 0 20 24` is
/// absolute segments 20-24 (nec2c prints rows 20, 21 of tag 1 and 22-24 of tag 2),
/// `PT 0 2 1 3` tag 2's first three; `I3 = 0` prints everything whatever `I1`.
#[test]
fn pt_ranges_resolve_like_ld() {
    let rows = |pt: &str| -> Vec<(u32, u32)> {
        let r = run(&two(&format!("EX 0 1 11 0 1 0\n{pt}\n")), "pt", CPU);
        assert!(r.status.success(), "{}", r.stderr);
        let mut out = Vec::new();
        let mut in_sec = false;
        for line in r.stdout.lines() {
            if line.trim() == "CURRENTS" {
                in_sec = true;
                continue;
            }
            if in_sec {
                let c: Vec<&str> = line.split_whitespace().collect();
                match (
                    c.first().and_then(|t| t.parse::<u32>().ok()),
                    c.get(1).and_then(|t| t.parse::<u32>().ok()),
                ) {
                    (Some(t), Some(s)) => out.push((t, s)),
                    _ if c.first() == Some(&"TAG") => {}
                    _ => break,
                }
            }
        }
        out
    };
    assert_eq!(
        rows("PT 0 0 20 24"),
        [(1, 20), (1, 21), (2, 1), (2, 2), (2, 3)]
    );
    assert_eq!(rows("PT 0 2 1 3"), [(2, 1), (2, 2), (2, 3)]);
    assert_eq!(rows("PT 0 0 32 0"), [(2, 11)]);
    assert_eq!(rows("PT 1 2 0 0").len(), 42, "I3 = 0 prints all, as nec2c");
    assert_eq!(rows("PT 0 0 0 0").len(), 42);
}

/// Every frontend resolves a tag-0 reference through the same resolver: the
/// worker (the distributed path, Hallén only) gives the tagged deck's answer for
/// `LD 4 0 11 11` and `EX 0 0 32`, and the CLI's own. The
/// GUI and the Python bindings have their twins (`gui_smoke.rs`, `fnec_py`).
#[test]
fn the_worker_reads_tag_zero_as_the_cli_does() {
    let ld = |tag: u32| two(&format!("EX 0 1 11 0 1 0\nLD 4 {tag} 11 11 0 1000\n"));
    {
        let solver = "hallen";
        let w = |deck: &str| {
            let r =
                nec_worker::solve::solve_deck_at_frequency_with_exec(deck, 29.9e6, solver, "cpu")
                    .unwrap_or_else(|e| panic!("{solver}: {e:?}"));
            (r.impedance_re, r.impedance_im)
        };
        let (abs, tagged) = (w(&ld(0)), w(&ld(1)));
        assert!(
            (abs.0 - tagged.0).abs() < 1e-9 && (abs.1 - tagged.1).abs() < 1e-9,
            "{solver}: worker LD 4 0 11 11 {abs:?} vs LD 4 1 11 11 {tagged:?}"
        );
        let cli = feed(&ld(0), "worker-cli", &["--solver", solver, "--exec", "cpu"]);
        assert!(
            (abs.0 - cli.2).abs() < 1e-6 && (abs.1 - cli.3).abs() < 1e-6,
            "{solver}: worker {abs:?} vs CLI {cli:?}"
        );
        let (abs, tagged) = (w(&two("EX 0 0 32 0 1 0\n")), w(&two("EX 0 2 11 0 1 0\n")));
        assert!(
            (abs.0 - tagged.0).abs() < 1e-9 && (abs.1 - tagged.1).abs() < 1e-9,
            "{solver}: worker EX 0 0 32 {abs:?} vs EX 0 2 11 {tagged:?}"
        );
    }
}
