// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! The parity sweep — the standing check for "a decision keyed on one axis while the run
//! has another" (eight of thirteen findings of the FND-197/198 audit;
//! `docs/dev/parity-sweep-design.md`).
//!
//! Every cell runs the CLI twice where the two runs MUST agree, and compares their whole
//! outcome: exit status, refusal, the run's decision record (the `diag:` line's
//! `route= … nf_exec=` fields), and every printed number. A cell ends `Holds` (both ran
//! and agree), `Refused` (both refused, for the same reason class), or it fails. The
//! outcome of every cell is pinned in `parity_manifest.txt`, together with the cell
//! count per relation, so a new refusal or a shrinking product is a reviewed diff — and
//! the decision values reached in `Holds` cells are pinned too, so coverage is read from
//! what the runs did, not from a list typed here.
//!
//! Bless after a reviewed change: `PARITY_BLESS=1 cargo test -p nec-cli --test parity_sweep`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FREQ_MHZ: f64 = 14.2;

/// Every `key=value` decision reached by a run in a `Holds` cell: coverage read from what
/// the runs did, pinned in the manifest, so a route, drive or load source the sweep stops
/// reaching — or a new one it starts reaching — is a reviewed diff.
static REACHED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Stages a device actually ran, across the sweep (`solve_exec=gpu`, `rp_exec=gpu`).
static DEVICE_STAGES: Mutex<usize> = Mutex::new(0);

fn record_reached(o: &Outcome) {
    let mut r = REACHED.lock().unwrap();
    for (k, v) in &o.decisions {
        r.insert(format!("{k}={v}"));
    }
    let on_device = ["solve_exec", "rp_exec"]
        .iter()
        .filter(|k| o.decisions.get(**k).map(String::as_str) == Some("gpu"))
        .count();
    *DEVICE_STAGES.lock().unwrap() += on_device;
}
const RUN_TIMEOUT: Duration = Duration::from_secs(120);

// ---------------------------------------------------------------------------------
// Running fnec
// ---------------------------------------------------------------------------------

/// One run's outcome, parsed.
#[derive(Debug, Clone)]
struct Outcome {
    ok: bool,
    /// The first `error:` line with digits removed: the reason class of a refusal.
    refusal: Option<String>,
    /// `key=value` from the `diag:` line's decision fields (first frequency point).
    decisions: BTreeMap<String, String>,
    /// Every numeric data section of the report, by name.
    sections: BTreeMap<String, Vec<Vec<f64>>>,
    stderr: String,
    stdout_text: String,
}

const DECISION_KEYS: [&str; 9] = [
    "route",
    "contact",
    "drive",
    "ground",
    "gsolver",
    "loads",
    "solve_exec",
    "rp_exec",
    "nf_exec",
];

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("fnec-parity-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// Run fnec with `args` on `deck`, plus an optional `--loads-config` / `--sweep-config`
/// file body. A run that outlives [`RUN_TIMEOUT`] has its state captured, then is killed,
/// and fails the cell by name — never a stalled gate.
fn run(cell: &str, deck: &str, args: &[&str], files: &[(&str, &str)]) -> Outcome {
    let dir = scratch().join(cell.replace(['/', ' '], "_"));
    std::fs::create_dir_all(&dir).expect("cell dir");
    let deck_path = dir.join("d.nec");
    std::fs::write(&deck_path, deck).expect("write deck");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.args(args);
    for (flag, body) in files {
        let p = dir.join(format!("{}.toml", flag.trim_start_matches('-')));
        std::fs::write(&p, body).expect("write config");
        cmd.arg(flag).arg(&p);
    }
    cmd.arg(&deck_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn fnec");
    let mut out = child.stdout.take().expect("stdout");
    let mut err = child.stderr.take().expect("stderr");
    let out_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break st;
        }
        if start.elapsed() > RUN_TIMEOUT {
            let pid = child.id();
            let state = ["status", "wchan", "stack"]
                .iter()
                .map(|f| {
                    format!(
                        "{f}: {}",
                        std::fs::read_to_string(format!("/proc/{pid}/{f}"))
                            .unwrap_or_else(|e| e.to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let _ = child.kill();
            panic!("{cell}: fnec ran past {RUN_TIMEOUT:?}; state before the kill:\n{state}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stdout = out_t.join().expect("stdout reader");
    let stderr = err_t.join().expect("stderr reader");
    let _ = std::fs::remove_dir_all(&dir);
    parse(status.success(), &stdout, &stderr)
}

fn parse(ok: bool, stdout: &str, stderr: &str) -> Outcome {
    let refusal = stderr.lines().find(|l| l.starts_with("error:")).map(|l| {
        l.chars()
            .filter(|c| !c.is_ascii_digit())
            .collect::<String>()
    });
    let mut decisions = BTreeMap::new();
    if let Some(diag) = stderr.lines().find(|l| l.starts_with("diag: mode=")) {
        for tok in diag.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                if DECISION_KEYS.contains(&k) {
                    decisions.insert(k.to_string(), v.to_string());
                }
            }
        }
    }
    // Report sections: an upper-case name line, then rows; keep the numeric rows.
    let mut sections: BTreeMap<String, Vec<Vec<f64>>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in stdout.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            current = Some(t.to_string());
            continue;
        }
        let nums: Vec<f64> = t
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        let all_numeric = nums.len() == t.split_whitespace().count();
        if let (Some(name), true, false) = (&current, all_numeric, nums.is_empty()) {
            sections.entry(name.clone()).or_default().push(nums);
        } else if let Some(rest) = t.strip_prefix("AVERAGE_POWER_GAIN ") {
            if let Ok(v) = rest.trim().parse::<f64>() {
                sections.insert("AVERAGE_POWER_GAIN".into(), vec![vec![v]]);
            }
        }
    }
    // Rows with a non-numeric token (`N_POINTS <n>`, column headers) are skipped; a
    // different row count still shows as a different number of numeric rows.
    Outcome {
        ok,
        refusal,
        decisions,
        sections,
        stderr: stderr.to_string(),
        stdout_text: stdout.to_string(),
    }
}

// ---------------------------------------------------------------------------------
// Comparing outcomes
// ---------------------------------------------------------------------------------

/// Whether two numbers agree: relative `tol` against the section's largest magnitude
/// (so a near-zero entry is not held to a relative bound it cannot meet), with angles
/// equal modulo 360 and the `-999.99` gain sentinel equal only to itself.
fn close(a: f64, b: f64, scale: f64, tol: f64) -> bool {
    if a == b {
        return true;
    }
    if (a - b).abs() > 300.0 && ((a - b).abs() - 360.0).abs() < 1e-3 {
        return true;
    }
    (a - b).abs() <= tol * scale.max(1e-12)
}

/// Compare two outcomes' numbers; `Err` names the first disagreement.
fn same_numbers(a: &Outcome, b: &Outcome, tol: f64) -> Result<(), String> {
    same_numbers_except(a, b, tol, &[])
}

fn same_numbers_except(a: &Outcome, b: &Outcome, tol: f64, skip: &[&str]) -> Result<(), String> {
    let names_a: BTreeSet<_> = a
        .sections
        .keys()
        .filter(|k| !skip.contains(&k.as_str()))
        .collect();
    let names_b: BTreeSet<_> = b
        .sections
        .keys()
        .filter(|k| !skip.contains(&k.as_str()))
        .collect();
    if names_a != names_b {
        return Err(format!("sections differ: {names_a:?} vs {names_b:?}"));
    }
    for (name, rows_a) in a
        .sections
        .iter()
        .filter(|(k, _)| !skip.contains(&k.as_str()))
    {
        let rows_b = &b.sections[name];
        if name == "RADIATION_PATTERN" {
            same_pattern(rows_a, rows_b, tol).map_err(|e| format!("{name}: {e}"))?;
            continue;
        }
        if rows_a.len() != rows_b.len() {
            return Err(format!("{name}: {} rows vs {}", rows_a.len(), rows_b.len()));
        }
        let scale = rows_a
            .iter()
            .chain(rows_b)
            .flatten()
            .filter(|v| v.abs() < 900.0)
            .fold(0.0_f64, |m, v| m.max(v.abs()));
        for (i, (ra, rb)) in rows_a.iter().zip(rows_b).enumerate() {
            if ra.len() != rb.len() {
                return Err(format!(
                    "{name} row {i}: width {} vs {}",
                    ra.len(),
                    rb.len()
                ));
            }
            for (j, (x, y)) in ra.iter().zip(rb).enumerate() {
                if !close(*x, *y, scale, tol) {
                    return Err(format!("{name} row {i} col {j}: {x} vs {y} (tol {tol})"));
                }
            }
        }
    }
    Ok(())
}

/// Pattern rows `THETA PHI GAIN GAIN_V GAIN_H AXIAL_RATIO`, compared as **power relative
/// to the pattern's peak**: a polarisation component at -160 dB against the -999.99
/// sentinel is the same nothing (f32 on the device, f64 on the CPU, in a null), while a
/// main-lobe error of a few dB — FND-205's 5-15 dB — is a large fraction of the peak and
/// fails. The axial ratio is compared only where both components are within 60 dB of the
/// peak; in a null it is the ratio of two roundings.
fn same_pattern(a: &[Vec<f64>], b: &[Vec<f64>], tol: f64) -> Result<(), String> {
    if a.len() != b.len() {
        return Err(format!("{} rows vs {}", a.len(), b.len()));
    }
    let lin = |db: f64| {
        if db < -900.0 {
            0.0
        } else {
            10f64.powf(db / 10.0)
        }
    };
    let peak = a.iter().chain(b).map(|r| lin(r[2])).fold(0.0_f64, f64::max);
    for (i, (ra, rb)) in a.iter().zip(b).enumerate() {
        if ra[0] != rb[0] || ra[1] != rb[1] {
            return Err(format!(
                "row {i}: direction {:?} vs {:?}",
                &ra[..2],
                &rb[..2]
            ));
        }
        for c in 2..5 {
            let (pa, pb) = (lin(ra[c]), lin(rb[c]));
            if (pa - pb).abs() > tol.max(1e-9) * peak {
                return Err(format!(
                    "row {i} col {c}: {} dB vs {} dB (peak-relative tol {tol})",
                    ra[c], rb[c]
                ));
            }
        }
        let floor = peak * 1e-6; // 60 dB below the peak
        if lin(ra[3]) > floor
            && lin(ra[4]) > floor
            && (ra[5] - rb[5]).abs() > tol.max(1e-9) * ra[5].abs().max(1.0)
        {
            return Err(format!("row {i}: axial ratio {} vs {}", ra[5], rb[5]));
        }
    }
    Ok(())
}

/// The decision record minus the keys a relation legitimately changes.
fn decisions_without(o: &Outcome, keys: &[&str]) -> BTreeMap<String, String> {
    o.decisions
        .iter()
        .filter(|(k, _)| !keys.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// The outcome of a cell that compares two runs which must agree.
fn pair_outcome(
    a: &Outcome,
    b: &Outcome,
    may_differ: &[&str],
    tol: f64,
    refusal_class: impl Fn(&str) -> String,
) -> Result<String, String> {
    pair_outcome_except(a, b, may_differ, tol, &[], refusal_class)
}

fn pair_outcome_except(
    a: &Outcome,
    b: &Outcome,
    may_differ: &[&str],
    tol: f64,
    skip_sections: &[&str],
    refusal_class: impl Fn(&str) -> String,
) -> Result<String, String> {
    match (a.ok, b.ok) {
        (true, true) => {
            let (da, db) = (
                decisions_without(a, may_differ),
                decisions_without(b, may_differ),
            );
            if da != db {
                return Err(format!("decisions differ: {da:?} vs {db:?}"));
            }
            same_numbers_except(a, b, tol, skip_sections)?;
            record_reached(a);
            record_reached(b);
            Ok("Holds".into())
        }
        (false, false) => {
            let (ra, rb) = (
                refusal_class(a.refusal.as_deref().unwrap_or("")),
                refusal_class(b.refusal.as_deref().unwrap_or("")),
            );
            if ra.is_empty() || rb.is_empty() {
                return Err(format!(
                    "a refusal without an error line:\n{}\n{}",
                    a.stderr, b.stderr
                ));
            }
            if ra != rb {
                return Err(format!("refused for different reasons: {ra:?} vs {rb:?}"));
            }
            Ok(format!("Refused({ra})"))
        }
        _ => Err(format!(
            "one run refused, the other solved:\n--- a (ok={}):\n{}\n--- b (ok={}):\n{}",
            a.ok, a.stderr, b.ok, b.stderr
        )),
    }
}

/// The refusal text itself is the class, by default.
fn verbatim(r: &str) -> String {
    r.trim().to_string()
}

// ---------------------------------------------------------------------------------
// Decks
// ---------------------------------------------------------------------------------

/// A geometry: GW cards (with `{Z}` for the height offset) and its feed `(tag, seg)`.
#[derive(Clone, Copy)]
struct Geometry {
    name: &'static str,
    wires: &'static str,
    feed: (u32, u32),
    /// A second feed, for the multi-source array.
    feed2: Option<(u32, u32)>,
    /// A segment to carry an off-feed load.
    load: (u32, u32),
    /// Touches z = 0 by construction: only meaningful over PEC, never raised.
    contact: bool,
}

const GEOMETRIES: [Geometry; 9] = [
    Geometry {
        name: "dipole",
        wires: "GW 1 21 0 0 {-5.28} 0 0 {5.28} .001\n",
        feed: (1, 11),
        feed2: None,
        load: (1, 5),
        contact: false,
    },
    Geometry {
        // Tilted and off-centre fed: no symmetry for a sign error to hide behind.
        name: "tilted-offcentre",
        wires: "GW 1 21 -3 1 {-4} 4 -1 {5} .001\n",
        feed: (1, 7),
        feed2: None,
        load: (1, 15),
        contact: false,
    },
    Geometry {
        name: "collinear-split",
        wires: "GW 1 10 0 0 {-5.28} 0 0 {0} .001\nGW 2 11 0 0 {0} 0 0 {5.28} .001\n",
        feed: (1, 10),
        feed2: None,
        load: (2, 6),
        contact: false,
    },
    Geometry {
        name: "inverted-v",
        wires: "GW 1 21 -4 0 {-2} 0 0 {2} .001\nGW 2 21 0 0 {2} 4 0 {-2} .001\n",
        feed: (1, 21),
        feed2: None,
        load: (2, 10),
        contact: false,
    },
    Geometry {
        name: "tee",
        wires: "GW 1 21 0 0 {-4} 0 0 {0} .001\nGW 2 21 -3 0 {0} 0 0 {0} .001\nGW 3 21 0 0 {0} 3 0 {0} .001\n",
        feed: (1, 4),
        feed2: None,
        load: (2, 5),
        contact: false,
    },
    Geometry {
        name: "square-loop",
        wires: "GW 1 11 0 0 {0} 3 0 {0} .001\nGW 2 11 3 0 {0} 3 0 {3} .001\nGW 3 11 3 0 {3} 0 0 {3} .001\nGW 4 11 0 0 {3} 0 0 {0} .001\n",
        feed: (1, 6),
        feed2: None,
        load: (3, 6),
        contact: false,
    },
    Geometry {
        name: "two-element-array",
        wires: "GW 1 21 0 0 {-5.28} 0 0 {5.28} .001\nGW 2 21 3 1 {-5.28} 3 1 {5.28} .001\n",
        feed: (1, 11),
        feed2: Some((2, 11)),
        load: (2, 4),
        contact: false,
    },
    Geometry {
        name: "monopole",
        wires: "GW 1 20 0 0 0 0 0 5.28 .001\n",
        feed: (1, 1),
        feed2: None,
        load: (1, 10),
        contact: true,
    },
    Geometry {
        // Its horizontal arm needs the image's reversed horizontal current.
        name: "inverted-l",
        wires: "GW 1 10 0 0 0 0 0 3 .001\nGW 2 10 0 0 3 4 1 3 .001\n",
        feed: (1, 1),
        feed2: None,
        load: (2, 5),
        contact: true,
    },
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Ground {
    Free,
    Pec,
    Finite,
}

const GROUNDS: [Ground; 3] = [Ground::Free, Ground::Pec, Ground::Finite];

impl Ground {
    fn name(self) -> &'static str {
        match self {
            Ground::Free => "free",
            Ground::Pec => "pec",
            Ground::Finite => "gn2",
        }
    }
    fn cards(self) -> &'static str {
        match self {
            Ground::Free => "GE 0\n",
            Ground::Pec => "GE 1\nGN 1\n",
            Ground::Finite => "GE 1\nGN 2 0 0 0 13 0.005\n",
        }
    }
}

/// Which geometries exist over which ground: raised decks over every ground, contact
/// decks over PEC only.
fn placements() -> Vec<(Geometry, Ground)> {
    let mut v = Vec::new();
    for g in GEOMETRIES {
        for gr in GROUNDS {
            if g.contact && gr != Ground::Pec {
                continue;
            }
            v.push((g, gr));
        }
    }
    v
}

/// The GW cards with every `{z}` shifted for the ground: free space as written, a
/// raised deck 8 m up (contact decks are written at z = 0 and never shifted).
fn wires(g: &Geometry, gr: Ground) -> String {
    let dz = if gr == Ground::Free || g.contact {
        0.0
    } else {
        8.0
    };
    let mut out = String::new();
    let mut rest = g.wires;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let j = rest[i..].find('}').expect("closing brace") + i;
        let z: f64 = rest[i + 1..j].parse().expect("z");
        out.push_str(&format!("{}", z + dz));
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drive {
    Voltage,
    PlaneWave,
}

/// A deck: geometry, ground, optional load card, drive, and the outputs every cell reads
/// (a small pattern with the average-power-gain digit, and near E and H fields).
fn deck(g: &Geometry, gr: Ground, ld: &str, drive: Drive) -> String {
    let ex = match drive {
        Drive::Voltage => {
            let mut e = format!("EX 0 {} {} 0 1 0\n", g.feed.0, g.feed.1);
            if let Some((t, s)) = g.feed2 {
                e.push_str(&format!("EX 0 {t} {s} 0 1 0\n"));
            }
            e
        }
        Drive::PlaneWave => "EX 1 1 1 0 60 30 0\n".to_string(),
    };
    let z0 = if gr == Ground::Free || g.contact {
        0.0
    } else {
        8.0
    };
    format!(
        "CE\n{}{}{ld}{ex}FR 0 1 0 0 {FREQ_MHZ} 0\nRP 0 3 2 1001 15 0 45 90\nNE 0 1 1 2 7 2 {} 0 0 3\nNH 0 1 1 2 7 2 {} 0 0 3\nEN\n",
        wires(g, gr),
        gr.cards(),
        z0 + 1.0,
        z0 + 1.0
    )
}

/// The same series R + L load, as an `LD 0` card and as a `--loads-config` entry.
fn load_spellings(tag: u32, seg: u32) -> (String, String) {
    (
        format!("LD 0 {tag} {seg} {seg} 75 1.5e-6 0\n"),
        format!(
            "[[laplace_load]]\ntag = {tag}\nseg_first = {seg}\nnumerator = [75.0, 1.5e-6]\ndenominator = [1.0]\n"
        ),
    )
}

const SOLVERS: [&str; 3] = ["hallen", "sinusoidal", "mpie"];

// ---------------------------------------------------------------------------------
// The sweep
// ---------------------------------------------------------------------------------

/// One cell: its id (relation/geometry/ground/…), and the closure that produces its
/// outcome — `Ok(label)` or `Err(why it failed)`.
/// A deck chosen to draw caveats: its name, its text, and any config files it runs with.
type CaveatDeck = (&'static str, String, Vec<(&'static str, String)>);

type Cell = (
    String,
    Box<dyn Fn() -> Result<String, String> + Send + Sync>,
);

fn cells() -> Vec<Cell> {
    let mut v: Vec<Cell> = Vec::new();

    // S-load: the same load as an LD card and from --loads-config. Every solver, every
    // placement, voltage and plane-wave drive. FND-197, FND-209.
    for (g, gr) in placements() {
        for solver in SOLVERS {
            for drive in [Drive::Voltage, Drive::PlaneWave] {
                let id = format!("S-load/{}/{}/{solver}/{drive:?}", g.name, gr.name());
                let (card, toml) = load_spellings(g.load.0, g.load.1);
                let with_card = deck(&g, gr, &card, drive);
                let bare = deck(&g, gr, "", drive);
                let cid = id.clone();
                v.push((
                    id,
                    Box::new(move || {
                        let args = ["--solver", solver, "--exec", "cpu"];
                        let a = run(&format!("{cid}/card"), &with_card, &args, &[]);
                        let b = run(
                            &format!("{cid}/config"),
                            &bare,
                            &args,
                            &[("--loads-config", &toml)],
                        );
                        // Both spellings refused: the class is "loads are refused here",
                        // whatever each message says about its own spelling. `LOADS`
                        // echoes LD cards by contract (cli-guide), so only the card run
                        // has it.
                        pair_outcome_except(&a, &b, &["loads"], 1e-9, &["LOADS"], |r| {
                            if r.contains("LD") || r.contains("oad") {
                                "loads refused".into()
                            } else {
                                verbatim(r)
                            }
                        })
                    }),
                ));
            }
        }
    }

    // S-exec: the device route against the CPU, every placement, on Hallén (the only
    // solver with a device path) — and auto, which may pick either. FND-205, FND-199's
    // local twin. Exact when every stage ran on the CPU; f32 when one ran on the device.
    for (g, gr) in placements() {
        for exec in ["gpu", "hybrid", "auto"] {
            let id = format!("S-exec/{}/{}/{exec}", g.name, gr.name());
            let text = deck(&g, gr, "", Drive::Voltage);
            let cid = id.clone();
            v.push((
                id,
                Box::new(move || {
                    let cpu = run(&format!("{cid}/cpu"), &text, &["--exec", "cpu"], &[]);
                    let args: Vec<&str> = if exec == "auto" {
                        vec![]
                    } else {
                        vec!["--exec", exec]
                    };
                    let dev = run(&format!("{cid}/dev"), &text, &args, &[]);
                    let on_device = dev.decisions.get("solve_exec").map(String::as_str)
                        == Some("gpu")
                        || dev.decisions.get("rp_exec").map(String::as_str) == Some("gpu");
                    let tol = if on_device { 2e-3 } else { 1e-12 };
                    pair_outcome(&cpu, &dev, &["solve_exec", "rp_exec"], tol, verbatim)
                }),
            ));
        }
    }

    // S-drive: a current source of the voltage drive's own feed current must price as the
    // voltage source — Z equal, currents equal. Single-feed voltage placements; with and
    // without a load (FND-198 was a loaded T).
    for (g, gr) in placements() {
        if g.feed2.is_some() {
            continue;
        }
        for loaded in [false, true] {
            let id = format!(
                "S-drive/{}/{}/{}",
                g.name,
                gr.name(),
                if loaded { "loaded" } else { "bare" }
            );
            let ld = if loaded {
                load_spellings(g.load.0, g.load.1).0
            } else {
                String::new()
            };
            let volt = deck(&g, gr, &ld, Drive::Voltage);
            let cid = id.clone();
            v.push((
                id,
                Box::new(move || {
                    let a = run(&format!("{cid}/ex0"), &volt, &["--exec", "cpu"], &[]);
                    if !a.ok {
                        return Ok(format!(
                            "Refused({})",
                            verbatim(a.refusal.as_deref().unwrap_or(""))
                        ));
                    }
                    let feed = a
                        .sections
                        .get("FEEDPOINTS")
                        .and_then(|r| r.first())
                        .cloned();
                    let Some(f) = feed else {
                        return Err("no feedpoint row".into());
                    };
                    // TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM. The feed current is printed
                    // with six fixed decimals — four significant digits at 1 mA — so i0 is
                    // taken as 1/Z from the impedance (eight), not from I.
                    let (zr, zi) = (f[6], f[7]);
                    let d = zr * zr + zi * zi;
                    let (ire, iim) = (zr / d, -zi / d);
                    let cur = volt.replacen(
                        &format!("EX 0 {} {} 0 1 0", g.feed.0, g.feed.1),
                        &format!("EX 4 {} {} 0 {ire:e} {iim:e}", g.feed.0, g.feed.1),
                        1,
                    );
                    let b = run(&format!("{cid}/ex4"), &cur, &["--exec", "cpu"], &[]);
                    if !b.ok {
                        return Err(format!("EX 0 solved, EX 4 refused: {:?}", b.refusal));
                    }
                    let (za, zb) = (feed_z(&a), feed_z(&b));
                    if (za.0 - zb.0).hypot(za.1 - zb.1) > 1e-6 * za.0.hypot(za.1) {
                        return Err(format!("EX 0 Z {za:?} vs EX 4 Z {zb:?}"));
                    }
                    record_reached(&a);
                    record_reached(&b);
                    let (ca, cb) = (&a.sections["CURRENTS"], &b.sections["CURRENTS"]);
                    for (ra, rb) in ca.iter().zip(cb) {
                        // TAG SEG I_RE I_IM — the complex current, not the printed phase.
                        let (x, y) = ((ra[2], ra[3]), (rb[2], rb[3]));
                        if (x.0 - y.0).hypot(x.1 - y.1) > 1e-6 * (ire.hypot(iim)) {
                            return Err(format!("current {ra:?} vs {rb:?}"));
                        }
                    }
                    Ok("Holds".into())
                }),
            ));
        }
    }

    // S-fr: the deck's own FR against the same frequency from --sweep-config, which
    // replaces it — and the sweep's frequency in place of a different deck FR. Every
    // solver, every placement, unloaded (so the MPIE solves here, where S-load's loads
    // refuse it). FND-210.
    for (g, gr) in placements() {
        for solver in SOLVERS {
            let id = format!("S-fr/{}/{}/{solver}", g.name, gr.name());
            let text = deck(&g, gr, "", Drive::Voltage);
            // The same deck asking for another frequency: the sweep file must replace it.
            let other = text.replacen(&format!("FR 0 1 0 0 {FREQ_MHZ} 0"), "FR 0 1 0 0 7.1 0", 1);
            let sweep = format!("[frequency]\npoints_mhz = [{FREQ_MHZ}]\n");
            let cid = id.clone();
            v.push((
                id,
                Box::new(move || {
                    let args = ["--solver", solver, "--exec", "cpu"];
                    let a = run(&format!("{cid}/fr"), &text, &args, &[]);
                    let b = run(
                        &format!("{cid}/sweep"),
                        &other,
                        &args,
                        &[("--sweep-config", &sweep)],
                    );
                    pair_outcome(&a, &b, &[], 1e-9, verbatim)
                }),
            ));
        }
    }

    // ---- Stage 2 -------------------------------------------------------------

    // S-worker: the distributed path's own solve (`fnec worker --stdio`, what a
    // `--hosts` controller sends) against the local CLI, bare and with an LD load, on
    // the CPU and on the device. FND-199 was the worker's device fallback alone. Where
    // the worker's protocol cannot carry what the local run solves, the refusal is
    // pinned as `WorkerRefused(...)` — a reviewed limit, never a silent skip.
    for (g, gr) in placements() {
        for exec in ["cpu", "gpu"] {
            for loaded in [false, true] {
                let id = format!(
                    "S-worker/{}/{}/{exec}/{}",
                    g.name,
                    gr.name(),
                    if loaded { "loaded" } else { "bare" }
                );
                let ld = if loaded {
                    load_spellings(g.load.0, g.load.1).0
                } else {
                    String::new()
                };
                let text = deck(&g, gr, &ld, Drive::Voltage);
                let cid = id.clone();
                v.push((
                    id,
                    Box::new(move || {
                        let local = run(&format!("{cid}/local"), &text, &["--exec", exec], &[]);
                        let remote = worker(&text, exec);
                        match (local.ok, remote) {
                            (true, Ok((re, im, used))) => {
                                let z = feed_z(&local);
                                let dev = used == "gpu"
                                    || local.decisions.get("solve_exec").map(String::as_str)
                                        == Some("gpu");
                                // The CLI prints six decimals; the worker sends full precision.
                                let tol = if dev { 2e-3 } else { 1e-7 };
                                if (z.0 - re).hypot(z.1 - im) > tol * z.0.hypot(z.1) {
                                    return Err(format!(
                                        "local Z {z:?} vs worker {re} + j{im} (exec_used {used})"
                                    ));
                                }
                                record_reached(&local);
                                Ok("Holds".into())
                            }
                            (true, Err(why)) => Ok(format!(
                                "WorkerRefused({})",
                                why.chars()
                                    .filter(|c| !c.is_ascii_digit())
                                    .take(90)
                                    .collect::<String>()
                            )),
                            (false, Err(_)) => Ok(format!(
                                "Refused({})",
                                verbatim(local.refusal.as_deref().unwrap_or(""))
                            )),
                            (false, Ok(z)) => Err(format!(
                                "the local CLI refused what the worker solved ({z:?}): {:?}",
                                local.refusal
                            )),
                        }
                    }),
                ));
            }
        }
    }

    // S-load-invalid: a load the geometry cannot take, spelled both ways, is refused
    // both ways, for the same reason. FND-204 (no segment), FND-207's twin (a range
    // written backwards).
    for g in GEOMETRIES.iter().filter(|g| !g.contact) {
        for (what, card, toml) in [
            (
                "no-segment",
                "LD 0 9 3 3 75 1.5e-6 0\n".to_string(),
                "[[laplace_load]]\ntag = 9\nseg_first = 3\nnumerator = [75.0, 1.5e-6]\ndenominator = [1.0]\n".to_string(),
            ),
            (
                "reversed-range",
                format!("LD 0 {} 8 3 75 1.5e-6 0\n", g.load.0),
                format!("[[laplace_load]]\ntag = {}\nseg_first = 8\nseg_last = 3\nnumerator = [75.0, 1.5e-6]\ndenominator = [1.0]\n", g.load.0),
            ),
        ] {
            let id = format!("S-load-invalid/{}/{what}", g.name);
            let with_card = deck(g, Ground::Free, &card, Drive::Voltage);
            let bare = deck(g, Ground::Free, "", Drive::Voltage);
            let cid = id.clone();
            v.push((
                id,
                Box::new(move || {
                    let args = ["--exec", "cpu"];
                    let a = run(&format!("{cid}/card"), &with_card, &args, &[]);
                    let b = run(&format!("{cid}/config"), &bare, &args, &[("--loads-config", &toml)]);
                    // A backwards range matches no segment: the LD check words it that
                    // way, the loads-file check names the order — one refusal class.
                    pair_outcome_except(&a, &b, &["loads"], 1e-9, &["LOADS"], |r| {
                        if r.contains("no segment")
                            || r.contains("before")
                            || r.contains("first to last")
                        {
                            "the load cannot be applied".into()
                        } else {
                            verbatim(r)
                        }
                    })
                }),
            ));
        }
    }

    // R-network: a pure shunt admittance at the feed (an NT to a distant, otherwise
    // unconnected stub, with only Y11) leaves the antenna's currents alone, so the
    // input impedance is exactly the antenna's in parallel with it. FND-208's shape.
    for (g, gr) in placements() {
        if g.feed2.is_some() || g.contact {
            continue;
        }
        let id = format!("R-network/{}/{}", g.name, gr.name());
        let z0 = if gr == Ground::Free { 0.0 } else { 8.0 };
        let stub = format!("GW 9 3 40 40 {} 41 40 {} .001\n", z0, z0);
        let bare = deck(&g, gr, "", Drive::Voltage).replacen("GE ", &format!("{stub}GE "), 1);
        let with_nt = bare.replacen(
            "EX ",
            &format!("NT {} {} 9 2 0 -0.01 0 0 0 0\nEX ", g.feed.0, g.feed.1),
            1,
        );
        let cid = id.clone();
        v.push((
            id,
            Box::new(move || {
                let a = run(&format!("{cid}/bare"), &bare, &["--exec", "cpu"], &[]);
                let b = run(&format!("{cid}/nt"), &with_nt, &["--exec", "cpu"], &[]);
                match (a.ok, b.ok) {
                    (true, true) => {
                        let (za, zb) = (feed_z(&a), feed_z(&b));
                        // Z_in = 1 / (1/Z_ant + Y), Y = -j0.01 S
                        let d = za.0 * za.0 + za.1 * za.1;
                        let (yr, yi) = (za.0 / d, -za.1 / d - 0.01);
                        let dy = yr * yr + yi * yi;
                        let want = (yr / dy, -yi / dy);
                        if (want.0 - zb.0).hypot(want.1 - zb.1) > 1e-6 * want.0.hypot(want.1) {
                            return Err(format!("Z with the shunt {zb:?}, Z_ant ∥ shunt {want:?}"));
                        }
                        record_reached(&b);
                        Ok("Holds".into())
                    }
                    (true, false) => Ok(format!(
                        "Refused({})",
                        verbatim(b.refusal.as_deref().unwrap_or(""))
                    )),
                    _ => Err(format!("the bare deck failed:\n{}", a.stderr)),
                }
            }),
        ));
    }

    // R-remedy: a remedy a caveat names (`--solver mpie`, `--ground-solver
    // sommerfeld`, …) must be one the run can take — rerunning with it is not refused.
    // FND-209: the negative-resistance caveat sent a --loads-config run to an MPIE that
    // refuses it. Decks chosen to draw caveats: a T with a one-segment arm, a low
    // dipole over finite ground, a loaded loop, the same with a load from a file.
    let caveat_decks: Vec<CaveatDeck> = vec![
        (
            "tee-one-segment-arm",
            "CE\nGW 1 21 0 0 0 0 0 5 .001\nGW 2 21 -5 0 5 0 0 5 .001\nGW 3 1 0 0 5 0.5 0 5 .001\nGE 0\nEX 0 1 5 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n".into(),
            vec![],
        ),
        (
            "low-dipole-gn2",
            "CE\nGW 1 21 -5.28 0 0.6 5.28 0 0.6 .001\nGE 1\nGN 2 0 0 0 13 0.005\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n".into(),
            vec![],
        ),
        (
            "negative-load-config",
            "CE\nGW 1 21 0 0 -5.28 0 0 5.28 .001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n".into(),
            vec![("--loads-config", "[[laplace_load]]\ntag = 1\nseg_first = 11\nnumerator = [-200.0]\ndenominator = [1.0]\n".into())],
        ),
        (
            "negative-load-card",
            "CE\nGW 1 21 0 0 -5.28 0 0 5.28 .001\nGE 0\nLD 4 1 11 11 -200 0\nEX 0 1 11 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n".into(),
            vec![],
        ),
    ];
    for (name, text, files) in caveat_decks {
        let id = format!("R-remedy/{name}");
        let cid = id.clone();
        v.push((
            id,
            Box::new(move || {
                let f: Vec<(&str, &str)> = files.iter().map(|(a, b)| (*a, b.as_str())).collect();
                let first = run(&format!("{cid}/first"), &text, &["--exec", "cpu"], &f);
                if !first.ok {
                    return Err(format!("the caveat deck itself failed:\n{}", first.stderr));
                }
                let named = remedies(&first.stderr);
                for r in &named {
                    let mut args: Vec<&str> = r.iter().map(String::as_str).collect();
                    if !args.contains(&"--exec") {
                        args.extend(["--exec", "cpu"]);
                    }
                    let again = run(&format!("{cid}/{}", r.join("-")), &text, &args, &f);
                    if !again.ok {
                        return Err(format!(
                            "a caveat named `{}`, and that run was refused: {:?}",
                            r.join(" "),
                            again.refusal
                        ));
                    }
                }
                Ok(format!(
                    "Holds[{}]",
                    named
                        .iter()
                        .map(|r| r.join(" "))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }),
        ));
    }

    // S-point: one point of a frequency sweep against that frequency solved alone —
    // every output and the decision record. Every solver and placement, voltage drive.
    for (g, gr) in placements() {
        for solver in SOLVERS {
            // The MPIE over ground costs 7-10 s a run in a debug build, and three
            // frequencies here; S-fr already compares its ground solves. Free space only.
            if solver == "mpie" && gr != Ground::Free {
                continue;
            }
            let id = format!("S-point/{}/{}/{solver}", g.name, gr.name());
            let single = deck(&g, gr, "", Drive::Voltage);
            let swept = single.replacen(
                &format!("FR 0 1 0 0 {FREQ_MHZ} 0"),
                &format!("FR 0 3 0 0 {} 0.2", FREQ_MHZ - 0.2),
                1,
            );
            let cid = id.clone();
            v.push((
                id,
                Box::new(move || {
                    let args = ["--solver", solver, "--exec", "cpu"];
                    let a = run_raw(&format!("{cid}/single"), &single, &args, &[]);
                    let b = run_raw(&format!("{cid}/sweep"), &swept, &args, &[]);
                    let (pa, pb) = (point(&a, FREQ_MHZ), point(&b, FREQ_MHZ));
                    pair_outcome(&pa, &pb, &[], 1e-9, verbatim)
                }),
            ));
        }
    }

    // R-image: a deck touching PEC against its explicit free-space double — the wires
    // mirrored in z = 0 with the horizontal current reversed, i.e. the same wires written
    // downward from the plane, fed antiphase where the image of a horizontal source would
    // be. Z halves, currents of the upper half equal, near fields above the plane equal.
    // FND-201.
    for g in GEOMETRIES.iter().filter(|g| g.contact) {
        let id = format!("R-image/{}", g.name);
        let g = *g;
        v.push((
            id.clone(),
            Box::new(move || {
                let pec = deck(&g, Ground::Pec, "", Drive::Voltage);
                let a = run(&format!("{id}/pec"), &pec, &["--exec", "cpu"], &[]);
                // The double: original wires, plus mirrored copies with tags +10; the base
                // segment and its image form one fed gap, driven across both halves.
                let mut double = String::from("CE\n");
                double.push_str(g.wires);
                for line in g.wires.lines() {
                    let f: Vec<&str> = line.split_whitespace().collect();
                    let n = |i: usize| -> f64 { f[i].parse().unwrap() };
                    double.push_str(&format!(
                        "GW {} {} {} {} {} {} {} {} {}\n",
                        n(1) as u32 + 10,
                        f[2],
                        n(3),
                        n(4),
                        -n(5),
                        n(6),
                        n(7),
                        -n(8),
                        f[9]
                    ));
                }
                double.push_str(&format!(
                    "GE 0\nEX 0 {} {} 0 1 0\nEX 0 {} {} 0 -1 0\nFR 0 1 0 0 {FREQ_MHZ} 0\nNE 0 1 1 2 7 2 1 0 0 3\nNH 0 1 1 2 7 2 1 0 0 3\nEN\n",
                    g.feed.0,
                    g.feed.1,
                    g.feed.0 + 10,
                    g.feed.1
                ));
                let b = run(&format!("{id}/double"), &double, &["--exec", "cpu"], &[]);
                if !(a.ok && b.ok) {
                    return Err(format!("pec ok={} double ok={}:\n{}\n{}", a.ok, b.ok, a.stderr, b.stderr));
                }
                // The image wire is written from the plane DOWN, so its current reads with
                // the opposite sign along its own direction; its EX is -1 for that reason.
                for name in ["NEAR_FIELD", "NEAR_H_FIELD"] {
                    let x = Outcome {
                        sections: [(name.to_string(), a.sections[name].clone())].into(),
                        ..a.clone()
                    };
                    let y = Outcome {
                        sections: [(name.to_string(), b.sections[name].clone())].into(),
                        ..b.clone()
                    };
                    same_numbers(&x, &y, 1e-6).map_err(|e| format!("{name}: {e}"))?;
                }
                let (zp, zd) = (feed_z(&a), feed_z(&b));
                // The double's two gaps in series carry the full 2 V across 2 Z_pec.
                if (zp.0 - zd.0).hypot(zp.1 - zd.1) > 1e-6 * zp.0.hypot(zp.1) {
                    return Err(format!("Z over PEC {zp:?} vs the double's per-gap Z {zd:?}"));
                }
                Ok("Holds".into())
            }),
        ));
    }

    // R-loss: a resistive load at the feed costs exactly its share of the input power:
    // gain_loaded - gain_unloaded = 10·log10(1 - R_L / Re Z_in,loaded), from printed
    // values only. Every raised placement, free space and PEC (FND-200) and finite ground.
    for (g, gr) in placements() {
        if g.feed2.is_some() || g.contact {
            continue;
        }
        let id = format!("R-loss/{}/{}", g.name, gr.name());
        v.push((
            id.clone(),
            Box::new(move || {
                let r_l = 50.0;
                let ld = format!("LD 4 {} {} {} {r_l} 0\n", g.feed.0, g.feed.1, g.feed.1);
                let a = run(
                    &format!("{id}/bare"),
                    &deck(&g, gr, "", Drive::Voltage),
                    &["--exec", "cpu"],
                    &[],
                );
                let b = run(
                    &format!("{id}/loaded"),
                    &deck(&g, gr, &ld, Drive::Voltage),
                    &["--exec", "cpu"],
                    &[],
                );
                if !(a.ok && b.ok) {
                    return Err(format!("bare ok={} loaded ok={}", a.ok, b.ok));
                }
                let z = feed_z(&b);
                let want = 10.0 * (1.0 - r_l / z.0).log10();
                let (pa, pb) = (
                    &a.sections["RADIATION_PATTERN"],
                    &b.sections["RADIATION_PATTERN"],
                );
                for (ra, rb) in pa.iter().zip(pb) {
                    // THETA PHI GAIN_DB …
                    if ra.len() < 3 || ra[2] < -900.0 || rb[2] < -900.0 {
                        continue;
                    }
                    let got = rb[2] - ra[2];
                    if (got - want).abs() > 0.05 {
                        return Err(format!(
                            "θ {} φ {}: gain moved {got:.4} dB, the load's share is {want:.4} dB",
                            ra[0], ra[1]
                        ));
                    }
                }
                Ok("Holds".into())
            }),
        ));
    }

    v
}

/// One frequency's block of a report: from its `FREQ_MHZ` line to the next block, with
/// the `diag:` line for that frequency — so a sweep point compares with a single run.
fn point(o: &RawRun, freq_mhz: f64) -> Outcome {
    let tag = format!("FREQ_MHZ {freq_mhz:.6}");
    let mut block = String::new();
    let mut on = false;
    for line in o.stdout.lines() {
        if line.starts_with("FREQ_MHZ ") {
            on = line.trim() == tag;
        } else if line == "SWEEP_POINTS" {
            on = false;
        }
        if on {
            block.push_str(line);
            block.push('\n');
        }
    }
    let diag_tag = format!("freq_mhz={freq_mhz:.6}");
    let stderr: String = o
        .stderr
        .lines()
        .filter(|l| !l.starts_with("diag:") || l.contains(&diag_tag))
        .map(|l| format!("{l}\n"))
        .collect();
    parse(o.ok, &block, &stderr)
}

/// A run's raw output, for relations that cut it up themselves.
struct RawRun {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn run_raw(cell: &str, deck: &str, args: &[&str], files: &[(&str, &str)]) -> RawRun {
    let o = run(cell, deck, args, files);
    RawRun {
        ok: o.ok,
        stdout: o.stdout_text.clone(),
        stderr: o.stderr.clone(),
    }
}

/// One task through `fnec worker --stdio`, the distributed path's own solve, as the
/// controller would send it. `Ok((re, im, exec_used))` or `Err(the worker's message)`.
fn worker(deck: &str, exec: &str) -> Result<(f64, f64, String), String> {
    use base64::Engine;
    use std::io::Write;
    let task = serde_json::json!({
        "task_id": "parity",
        "deck_hash": "-",
        "deck_b64": base64::engine::general_purpose::STANDARD.encode(deck),
        "solver_config": {"basis": "hallen", "ground_model": "none", "exec": exec},
        "frequency_hz": FREQ_MHZ * 1e6,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["worker", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn worker");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        writeln!(stdin, "{task}").expect("send task");
        writeln!(stdin, r#"{{"cmd":"shutdown"}}"#).expect("send shutdown");
    }
    let out = child.wait_with_output().expect("worker output");
    let line = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(line.lines().next().unwrap_or(""))
        .map_err(|e| format!("unreadable worker line {line:?}: {e}"))?;
    if v["status"] == "ok" {
        Ok((
            v["impedance"]["re_ohm"].as_f64().unwrap_or(f64::NAN),
            v["impedance"]["im_ohm"].as_f64().unwrap_or(f64::NAN),
            v["exec_used"].as_str().unwrap_or("?").to_string(),
        ))
    } else {
        Err(v["error_message"].as_str().unwrap_or("?").to_string())
    }
}

/// The `--solver`/`--ground-solver`/`--exec` remedies a run's warnings name.
fn remedies(stderr: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    for line in stderr.lines().filter(|l| l.starts_with("warning:")) {
        let toks: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == '`')
            .filter(|t| !t.is_empty())
            .collect();
        for w in toks.windows(2) {
            if ["--solver", "--ground-solver", "--exec"].contains(&w[0]) {
                let val = w[1].trim_end_matches([',', '.', ';', ')']);
                let r = vec![w[0].to_string(), val.to_string()];
                if !out.contains(&r) {
                    out.push(r);
                }
            }
        }
    }
    out
}

fn feed_z(o: &Outcome) -> (f64, f64) {
    let f = &o.sections["FEEDPOINTS"][0];
    (f[6], f[7])
}

fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity_manifest.txt")
}

#[test]
fn parity_sweep() {
    let all = cells();
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().min(6))
        .unwrap_or(2);
    let results: Mutex<BTreeMap<String, Result<String, String>>> = Mutex::new(BTreeMap::new());
    let next = Mutex::new(0usize);
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let i = {
                    let mut n = next.lock().unwrap();
                    let i = *n;
                    *n += 1;
                    i
                };
                let Some((id, f)) = all.get(i) else { break };
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|p| {
                        Err(p
                            .downcast_ref::<String>()
                            .cloned()
                            .unwrap_or_else(|| "panicked".into()))
                    });
                results.lock().unwrap().insert(id.clone(), r);
            });
        }
    });
    let results = results.into_inner().unwrap();
    let _ = std::fs::remove_dir_all(scratch());

    let failures: Vec<_> = results
        .iter()
        .filter_map(|(id, r)| r.as_ref().err().map(|e| (id, e)))
        .collect();

    // The manifest: every cell's outcome, and the cell count per relation.
    let mut manifest = String::from(
        "# parity_sweep — every cell's outcome. A diff here is a behaviour change: review it,\n\
         # then bless with PARITY_BLESS=1. Generated; do not edit by hand.\n",
    );
    let mut per_relation: BTreeMap<String, usize> = BTreeMap::new();
    for (id, r) in &results {
        *per_relation
            .entry(id.split('/').next().unwrap().to_string())
            .or_default() += 1;
        if let Ok(label) = r {
            manifest.push_str(&format!("{id} {label}\n"));
        }
    }
    for (rel, n) in &per_relation {
        manifest.push_str(&format!("count {rel} {n}\n"));
    }
    // Which executor ran a stage depends on the host; everything else reached is pinned.
    for kv in REACHED.lock().unwrap().iter() {
        if !kv.starts_with("solve_exec=") && !kv.starts_with("rp_exec=") {
            manifest.push_str(&format!("reached {kv}\n"));
        }
    }
    let device_stages = *DEVICE_STAGES.lock().unwrap();
    if pollster::block_on(nec_accel::hardware_adapter_present()) {
        assert!(
            device_stages > 0,
            "a hardware GPU is present, but no stage of any Holds cell ran on it: S-exec \
             compared the CPU with the CPU"
        );
        eprintln!("parity_sweep: {device_stages} stages ran on the device");
    } else {
        eprintln!("parity_sweep: no hardware GPU — S-exec compared the CPU with its fallback");
    }

    assert!(
        failures.is_empty(),
        "{} of {} parity cells failed:\n{}",
        failures.len(),
        results.len(),
        failures
            .iter()
            .map(|(id, e)| format!("--- {id}\n{e}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let path = manifest_path();
    if std::env::var_os("PARITY_BLESS").is_some() {
        std::fs::write(&path, &manifest).expect("write manifest");
        eprintln!("blessed {} cells into {}", results.len(), path.display());
        return;
    }
    let pinned = std::fs::read_to_string(&path).unwrap_or_default();
    // The exec relation's labels depend on the host's GPU; the manifest pins the rest.
    let strip = |m: &str| -> String {
        m.lines()
            .filter(|l| !l.starts_with("S-exec/"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    if strip(&pinned) != strip(&manifest) {
        let old: BTreeSet<&str> = pinned.lines().collect();
        let new: BTreeSet<&str> = manifest.lines().collect();
        panic!(
            "the parity manifest changed — review, then bless with PARITY_BLESS=1:\n\
             removed:\n{}\nadded:\n{}",
            old.difference(&new)
                .filter(|l| !l.starts_with("S-exec/"))
                .copied()
                .collect::<Vec<_>>()
                .join("\n"),
            new.difference(&old)
                .filter(|l| !l.starts_with("S-exec/"))
                .copied()
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}
