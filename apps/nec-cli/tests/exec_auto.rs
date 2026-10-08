// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Without `--exec`, the deck picks the CPU or the GPU from this host's calibration
//! (`fnec calibrate`): a supported deck goes to the GPU at or above the calibrated
//! crossover, when the calibration was measured on this host's device. Without a
//! calibration the pick is the CPU and says how to get one. Everything here runs
//! on any host — each run is given its own calibration file through
//! `FNEC_EXEC_CALIBRATION`, never the user's — and on one without a GPU the pick
//! must stay on the CPU and the answer must be the CPU's to the byte. The
//! measuring path of `fnec calibrate` itself needs a GPU and minutes; it is run by
//! hand on a GPU host (CI has no adapter).

mod common;

use std::path::PathBuf;
use std::process::Command;

fn dipole(n: usize, points: usize) -> String {
    format!(
        "CE\nGW 1 {n} 0 0 -5.282 0 0 5.282 .001\nGE 0\nEX 0 1 {} 0 1 0\nFR 0 {points} 0 0 14.2 0.1\nEN\n",
        n / 2 + 1
    )
}

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("fnec-exec-auto-{name}-{}", std::process::id()))
}

/// Run fnec on `deck` with `args` and the calibration at `calibration` (a path that
/// need not exist); (stdout, stderr). Four sweep threads, so a sweep calibration
/// can name them.
fn run_with(
    name: &str,
    deck: &str,
    args: &[&str],
    calibration: &std::path::Path,
) -> (String, String) {
    let path = scratch(name).with_extension("nec");
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .env("FNEC_EXEC_CALIBRATION", calibration)
        .env("RAYON_NUM_THREADS", "4")
        .args(args)
        .arg(&path)
        .output()
        .expect("run fnec");
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "fnec {args:?} failed:\n{stderr}");
    (String::from_utf8_lossy(&out.stdout).into_owned(), stderr)
}

/// An uncalibrated run.
fn run(name: &str, deck: &str, args: &[&str]) -> (String, String) {
    run_with(
        name,
        deck,
        args,
        &scratch("no-calibration").with_extension("toml"),
    )
}

/// This host's key as `fnec calibrate --print-key` reports it, or `None` without a
/// GPU device.
fn host_key() -> Option<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["calibrate", "--print-key"])
        .output()
        .expect("run fnec calibrate");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A calibration file for this host (or a foreign one): 550 segments for one point
/// and for a sweep of 4 threads.
fn calibration(name: &str, key: Option<&str>) -> PathBuf {
    let key = key.map(str::to_string).unwrap_or_else(|| {
        "adapter = \"Another GPU\"\nbackend = \"Vulkan\"\ndevice_type = \"DiscreteGpu\"\ndriver = \"0\"\ncpu = \"Another CPU\"\n".to_string()
    });
    let text = format!(
        "schema = 1\nepoch = 1\none_point_min_segs = 550\nsweep_min_segs = 550\nsweep_threads = 4\nmeasured_up_to = 2000\n\n[key]\n{key}"
    );
    let p = scratch(name).with_extension("toml");
    std::fs::write(&p, text).expect("write calibration");
    p
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

/// Without a calibration for this host the pick is the CPU, however large the
/// deck, and it says how to get one — the answer the CPU's to the byte.
#[test]
fn an_uncalibrated_host_stays_on_the_cpu_and_says_how_to_calibrate() {
    let deck = dipole(901, 1);
    let (auto_out, auto_err) = run("uncal", &deck, &[]);
    assert_eq!(selected(&auto_err), "cpu", "{auto_err}");
    assert!(auto_err.contains("run `fnec calibrate`"), "{auto_err}");
    let (cpu_out, _) = run("uncal-cpu", &deck, &["--exec", "cpu"]);
    assert_eq!(auto_out, cpu_out);
}

/// A calibration file that cannot be read is said, with its path — never silently
/// treated as absent.
#[test]
fn an_unreadable_calibration_is_named() {
    let bad = scratch("bad").with_extension("toml");
    std::fs::write(&bad, "this is not toml = = =").unwrap();
    let (_, err) = run_with("bad", &dipole(901, 1), &[], &bad);
    let _ = std::fs::remove_file(&bad);
    assert_eq!(selected(&err), "cpu", "{err}");
    assert!(
        err.contains("could not be read") && err.contains("bad"),
        "{err}"
    );
}

#[test]
fn below_the_calibrated_crossover_the_pick_is_the_cpu_to_the_byte() {
    let cal = calibration("below", host_key().as_deref());
    let deck = dipole(549, 1);
    let (auto_out, auto_err) = run_with("549", &deck, &[], &cal);
    assert_eq!(selected(&auto_err), "cpu", "{auto_err}");
    assert!(
        auto_err.contains("below this host's GPU crossover for one point (550)"),
        "{auto_err}"
    );
    let (cpu_out, _) = run_with("549cpu", &deck, &["--exec", "cpu"], &cal);
    let _ = std::fs::remove_file(&cal);
    assert_eq!(auto_out, cpu_out);
}

#[test]
fn at_the_calibrated_crossover_the_pick_is_the_gpu_where_there_is_one() {
    let key = host_key();
    let cal = calibration("at", key.as_deref());
    let deck = dipole(551, 1);
    let (auto_out, auto_err) = run_with("551", &deck, &[], &cal);
    let (cpu_out, _) = run_with("551cpu", &deck, &["--exec", "cpu"], &cal);
    let _ = std::fs::remove_file(&cal);
    if key.is_none() {
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

/// A calibration measured on another device or CPU does not apply here.
#[test]
fn a_calibration_from_another_host_does_not_apply() {
    let cal = calibration("foreign", None);
    let (_, err) = run_with("foreign", &dipole(901, 1), &[], &cal);
    let _ = std::fs::remove_file(&cal);
    assert_eq!(selected(&err), "cpu", "{err}");
    if host_key().is_some() {
        assert!(
            err.contains("Another GPU") && err.contains("run `fnec calibrate` again"),
            "{err}"
        );
    } else {
        assert!(err.contains("no hardware GPU"), "{err}");
    }
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

/// A sweep has its own calibrated crossover.
#[test]
fn a_sweep_below_its_crossover_stays_on_the_cpu() {
    let cal = calibration("sweep520", host_key().as_deref());
    let (_, err) = run_with("sweep520", &dipole(520, 2), &[], &cal);
    let _ = std::fs::remove_file(&cal);
    assert_eq!(selected(&err), "cpu", "{err}");
    assert!(err.contains("crossover for a sweep (550)"), "{err}");
}

/// Past the sweep crossover, every point goes to the device, and the run says
/// how many did.
#[test]
fn a_sweep_past_its_crossover_runs_on_the_gpu_and_counts_its_points() {
    let key = host_key();
    let cal = calibration("sweep551", key.as_deref());
    let (_, err) = run_with("sweep551", &dipole(551, 2), &[], &cal);
    let _ = std::fs::remove_file(&cal);
    if key.is_none() {
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
/// quotes this host's calibration, once per run (it printed once per sweep point);
/// an uncalibrated host has no measurement to quote and says nothing.
#[test]
fn the_small_deck_gpu_warning_quotes_the_calibration_once_per_run() {
    let cal = calibration("small", host_key().as_deref());
    let (_, err) = run_with("small-sweep", &dipole(51, 5), &["--exec", "gpu"], &cal);
    let _ = std::fs::remove_file(&cal);
    let n = err
        .lines()
        .filter(|l| l.contains("slower than the CPU below 550 segments"))
        .count();
    assert_eq!(n, 1, "expected one warning, got {n}:\n{err}");
    let (_, uncal) = run("small-uncal", &dipole(51, 5), &["--exec", "gpu"]);
    assert!(!uncal.contains("slower than the CPU"), "{uncal}");
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

/// `fnec calibrate --print-key` names this host's device, or says there is none.
#[test]
fn calibrate_prints_this_hosts_key_or_says_there_is_no_device() {
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(["calibrate", "--print-key"])
        .output()
        .expect("run fnec calibrate");
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    if out.status.success() {
        assert!(
            stdout.contains("adapter = ") && stdout.contains("cpu = "),
            "{stdout}"
        );
    } else {
        assert_eq!(out.status.code(), Some(3), "{stderr}");
        assert!(stderr.contains("no GPU device"), "{stderr}");
    }
}
