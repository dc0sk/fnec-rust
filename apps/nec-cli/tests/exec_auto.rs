// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Without `--exec`, the deck picks the CPU or the GPU: a supported deck goes to
//! the GPU at ≥ 600 segments for one point, ≥ 550 for a sweep, when a hardware
//! GPU is present. Everything here runs on any host; on one without a GPU the
//! pick must stay on the CPU and the answer must be the CPU's to the byte.

mod common;

use std::process::Command;

fn dipole(n: usize, points: usize) -> String {
    format!(
        "CE\nGW 1 {n} 0 0 -5.282 0 0 5.282 .001\nGE 0\nEX 0 1 {} 0 1 0\nFR 0 {points} 0 0 14.2 0.1\nEN\n",
        n / 2 + 1
    )
}

/// Run fnec on `deck` with `args`; (stdout, stderr).
fn run(name: &str, deck: &str, args: &[&str]) -> (String, String) {
    let path =
        std::env::temp_dir().join(format!("fnec-exec-auto-{name}-{}.nec", std::process::id()));
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "fnec {args:?} failed:\n{stderr}");
    (String::from_utf8_lossy(&out.stdout).into_owned(), stderr)
}

fn selected(stderr: &str) -> &str {
    let line = stderr
        .lines()
        .find(|l| l.starts_with("info: exec auto: selected_exec="))
        .unwrap_or_else(|| panic!("no exec auto line:\n{stderr}"));
    line["info: exec auto: selected_exec=".len()..]
        .split_whitespace()
        .next()
        .unwrap()
}

fn feed_z(stdout: &str) -> (f64, f64) {
    let row = stdout
        .lines()
        .skip_while(|l| *l != "FEEDPOINTS")
        .nth(2)
        .expect("feedpoint row");
    let f: Vec<f64> = row
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    (f[6], f[7])
}

fn gpu_present() -> bool {
    pollster::block_on(nec_accel::hardware_adapter_present())
}

#[test]
fn below_the_crossover_the_pick_is_the_cpu_to_the_byte() {
    let deck = dipole(599, 1);
    let (auto_out, auto_err) = run("599", &deck, &[]);
    assert_eq!(selected(&auto_err), "cpu", "{auto_err}");
    let (cpu_out, _) = run("599cpu", &deck, &["--exec", "cpu"]);
    assert_eq!(auto_out, cpu_out);
}

#[test]
fn at_the_crossover_the_pick_is_the_gpu_where_there_is_one() {
    let deck = dipole(601, 1);
    let (auto_out, auto_err) = run("601", &deck, &[]);
    let (cpu_out, _) = run("601cpu", &deck, &["--exec", "cpu"]);
    if !gpu_present() {
        assert_eq!(selected(&auto_err), "cpu", "{auto_err}");
        assert!(auto_err.contains("no hardware GPU"), "{auto_err}");
        assert_eq!(auto_out, cpu_out, "no GPU: the CPU's answer to the byte");
        return;
    }
    assert_eq!(selected(&auto_err), "gpu", "{auto_err}");
    common::assert_gpu_exec_label(&auto_err);
    let (g, c) = (feed_z(&auto_out), feed_z(&cpu_out));
    assert!(
        (g.0 - c.0).abs() < 2.0 && (g.1 - c.1).abs() < 2.0,
        "GPU {g:?} against CPU {c:?}"
    );
}

/// A deck the device does not solve stays on the CPU however large: a 90° L is a
/// bent conductor, which takes the path basis.
#[test]
fn a_large_deck_the_gpu_does_not_solve_stays_on_the_cpu() {
    let deck = "CE\nGW 1 400 0 0 0 5 0 0 .001\nGW 2 400 5 0 0 5 0 5 .001\nGE 0\nEX 0 1 200 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let (_, err) = run("bent", deck, &[]);
    assert_eq!(selected(&err), "cpu", "{err}");
    assert!(err.contains("not a deck the GPU solves"), "{err}");
}

/// A sweep has its own crossover: 520 segments over two points stays on the
/// CPU, below the sweep threshold (550).
#[test]
fn a_sweep_below_its_crossover_stays_on_the_cpu() {
    let (_, err) = run("sweep520", &dipole(520, 2), &[]);
    assert_eq!(selected(&err), "cpu", "{err}");
    assert!(err.contains("crossover for a sweep (550)"), "{err}");
}

/// Past the sweep crossover, every point goes to the device, and the run says
/// how many did.
#[test]
fn a_sweep_past_its_crossover_runs_on_the_gpu_and_counts_its_points() {
    let (_, err) = run("sweep551", &dipole(551, 2), &[]);
    if !gpu_present() {
        assert_eq!(selected(&err), "cpu", "{err}");
        return;
    }
    assert_eq!(selected(&err), "gpu", "{err}");
    let summary = err
        .lines()
        .find(|l| l.contains("sweep points solved on the GPU"))
        .unwrap_or_else(|| panic!("no device count:\n{err}"));
    // Two of two, unless the driver lost the device under a point (FND-190).
    assert!(
        summary.contains("2 of 2") || err.contains("device is lost"),
        "{summary}"
    );
}

/// The slower-than-the-CPU warning for an explicit `--exec gpu` on a small deck
/// is said once per run. It printed once per sweep point.
#[test]
fn the_small_deck_gpu_warning_is_said_once_per_run() {
    let (_, err) = run("small-sweep", &dipole(51, 5), &["--exec", "gpu"]);
    let n = err
        .lines()
        .filter(|l| l.contains("is slower than the CPU below about"))
        .count();
    assert_eq!(n, 1, "expected one warning, got {n}:\n{err}");
}

/// A deck with a load card stays on the CPU: the device re-solves from raw
/// segments and would not see the stamp. Checked before any GPU is asked about,
/// so the reason is the same on every host.
#[test]
fn a_loaded_deck_stays_on_the_cpu() {
    let deck = "CE\nGW 1 601 0 0 -5.282 0 0 5.282 .001\nGE 0\nLD 4 1 301 301 50 0\nEX 0 1 301 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    let (_, err) = run("loaded", deck, &[]);
    assert_eq!(selected(&err), "cpu", "{err}");
    assert!(err.contains("stamps loads or networks"), "{err}");
}
