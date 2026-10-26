// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH7-CHK-003 CLI gate: `--exec gpu` (GPU-resident Hallén fill+solve) must
//! produce feedpoint impedance within 2 Ω of `--exec cpu` on free-space corpus
//! decks in the supported class.
//!
//! When no wgpu adapter is available the GPU path falls back to the CPU solve,
//! so the two runs are identical and the test still passes (Δ = 0).

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::{classify_gpu_stderr, GpuMiss, GpuRun};
use nec_accel::gpu_test::{gpu_attempt, gpu_expected};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Run `fnec --solver hallen --exec <mode> <deck>` and return (Z_RE, Z_IM) of
/// the first FEEDPOINTS row.
fn feedpoint_impedance(deck: &str, exec: &str) -> (f64, f64) {
    let deck_path = workspace_root().join("corpus").join(deck);
    let run = || {
        let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--solver", "hallen", "--exec", exec])
            .arg(&deck_path)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn fnec: {e}"));
        assert!(
            out.status.success(),
            "fnec --exec {exec} {deck} failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    // FND-163: at 51 segments the f32 solve is accurate, so where a GPU is
    // expected it must not decline at all. A fallback would make the comparison
    // in the test below the CPU against itself — it passed with the device solve
    // forced to fail. A lost device earns one re-run (FND-215); without a GPU,
    // "no adapter" is the one expected decline.
    let stdout = if exec == "gpu" {
        let attempt = gpu_attempt(deck, || {
            let (stdout, stderr) = run();
            match classify_gpu_stderr(&stderr) {
                GpuRun::OnDevice => Ok(stdout),
                GpuRun::Missed(why) => Err(GpuMiss {
                    why,
                    stdout,
                    stderr,
                }),
                GpuRun::Declined(line) => {
                    panic!("{deck}: the GPU-resident solve declined on this host:\n{line}")
                }
            }
        });
        match attempt {
            Ok(stdout) => stdout,
            Err(miss) => {
                assert!(
                    miss.why.contains("no wgpu adapter available"),
                    "{deck}: no GPU is expected, yet the solve declined: {}",
                    miss.why
                );
                miss.stdout
            }
        }
    } else {
        run().0
    };

    let mut lines = stdout.lines();
    // Find the FEEDPOINTS header, skip the column header, read the first data row.
    let row = loop {
        match lines.next() {
            Some("FEEDPOINTS") => {
                let _cols = lines.next(); // "TAG SEG V_RE ... Z_RE Z_IM"
                break lines.next().expect("missing feedpoint data row");
            }
            Some(_) => continue,
            None => panic!("no FEEDPOINTS section in output for {deck} ({exec})"),
        }
    };
    let f: Vec<&str> = row.split_whitespace().collect();
    // TAG SEG V_RE V_IM I_RE I_IM Z_RE Z_IM
    let z_re: f64 = f[6].parse().expect("Z_RE parse");
    let z_im: f64 = f[7].parse().expect("Z_IM parse");
    (z_re, z_im)
}

#[test]
fn gpu_resident_matches_cpu_within_2_ohm_on_corpus() {
    const DECKS: &[&str] = &[
        "dipole-freesp-51seg.nec",
        "dipole-freesp-rp-51seg.nec",
        "dipole-freesp-gm-inplace-shifted.nec",
    ];
    const TOL_OHM: f64 = 2.0;

    for deck in DECKS {
        let (cpu_r, cpu_x) = feedpoint_impedance(deck, "cpu");
        let (gpu_r, gpu_x) = feedpoint_impedance(deck, "gpu");
        let dr = (gpu_r - cpu_r).abs();
        let dx = (gpu_x - cpu_x).abs();
        eprintln!(
            "PH7-CHK-003 CLI: {deck}  Z_cpu=({cpu_r:.3}+j{cpu_x:.3})  Z_gpu=({gpu_r:.3}+j{gpu_x:.3})  ΔR={dr:.4}  ΔX={dx:.4}"
        );
        assert!(
            dr <= TOL_OHM && dx <= TOL_OHM,
            "{deck}: GPU-resident impedance differs from CPU by ΔR={dr:.4} ΔX={dx:.4} Ω (> {TOL_OHM} Ω)"
        );
    }
}

/// The GPU-resident f32 solve must never hand back an answer it got wrong.
///
/// `A = MᴴM` squares `cond(M)`, so the f32 solve degrades as the system grows. The
/// corpus decks above are 51 segments, where it is accurate; at 301 segments it
/// used to diverge and the result was reported anyway — one frequency point came
/// back at 101 Ω against the CPU's 75 Ω, and another at **−1.98 Ω**, a negative
/// resistance for a passive antenna. Nothing checked, so nothing warned.
///
/// The solve now reports its own relative residual and the host rejects a
/// non-converged one, falling back to the f64 CPU solve. This test drives the size
/// where that used to break.
#[test]
fn gpu_resident_never_reports_a_diverged_solve() {
    const TOL_OHM: f64 = 2.0;
    // 301 segments, three frequency points — two of which diverged before the gate.
    let deck = "\
GW 1 301 0 0 -5.282 0 0 5.282 0.001
GE
EX 0 1 151 0 1.0 0.0
FR 0 3 0 0 14.0 0.15
EN
";
    let path = std::env::temp_dir().join("fnec-gpu-resident-301seg.nec");
    std::fs::write(&path, deck).expect("write deck");

    let run = |exec: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--solver", "hallen", "--exec", exec])
            .arg(&path)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn fnec: {e}"));
        assert!(out.status.success(), "fnec --exec {exec} failed: {out:?}");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        // SWEEP_POINTS rows: FREQ_MHZ TAG SEG Z_RE Z_IM
        let z: Vec<(f64, f64)> = stdout
            .lines()
            .skip_while(|l| !l.starts_with("SWEEP_POINTS"))
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() == 5 {
                    Some((f[3].parse().ok()?, f[4].parse().ok()?))
                } else {
                    None
                }
            })
            .collect();
        (z, stderr)
    };

    let (cpu, _) = run("cpu");
    // The one decline this test expects is its subject: the f32 solve failing
    // its residual check at this size, and falling back. A device fault earns
    // one re-run (FND-215); any OTHER decline — an out-of-class input — means the
    // GPU path is broken and the comparison below would compare the CPU with
    // itself (FND-163).
    let attempt = gpu_attempt("the 301-segment sweep", || {
        let (z, stderr) = run("gpu");
        match classify_gpu_stderr(&stderr) {
            GpuRun::Missed(why) => Err(why),
            GpuRun::Declined(line) if !line.contains("did not converge") => panic!(
                "the GPU-resident solve declined for a reason other than its residual check:\n{line}"
            ),
            _ => Ok(z),
        }
    });
    let _ = std::fs::remove_file(&path);
    let Ok(gpu) = attempt else {
        eprintln!("SKIP: no wgpu adapter on this host — the GPU solve never ran");
        return;
    };

    assert_eq!(cpu.len(), 3, "expected 3 sweep points, got {cpu:?}");
    assert_eq!(
        gpu.len(),
        cpu.len(),
        "point count differs: {gpu:?} vs {cpu:?}"
    );

    for (i, ((gr, gx), (cr, cx))) in gpu.iter().zip(cpu.iter()).enumerate() {
        assert!(
            *gr > 0.0,
            "point {i}: GPU reported a negative resistance {gr:.3} Ω (CPU {cr:.3})"
        );
        let dr = (gr - cr).abs();
        let dx = (gx - cx).abs();
        assert!(
            dr <= TOL_OHM && dx <= TOL_OHM,
            "point {i}: GPU {gr:.3}+j{gx:.3} vs CPU {cr:.3}+j{cx:.3} \
             (ΔR={dr:.3} ΔX={dx:.3} Ω, tolerance {TOL_OHM})"
        );
    }
}

/// 2049 segments, one past the first size whose Z-fill panicked: one dimension of
/// its dispatch passed the 65535-workgroup limit, and `--exec gpu` exited 101 on
/// every deck this large. It also sat above the old `MAX_S = 1024`, where the
/// dense solve declined without a word. Both must now run on the device.
///
/// The CPU answer is pinned rather than re-run: it takes about 70 s here, the
/// device about 1.3 s (3.5 s before the triangular solves ran per column). Measured 2026-09-30, `--exec cpu`: 79.692336 + j46.433387.
#[test]
fn a_deck_past_the_old_gpu_ceilings_solves_on_the_device() {
    if !gpu_expected() {
        eprintln!("SKIP: no hardware GPU adapter");
        return;
    }
    let path = std::env::temp_dir().join(format!("fnec-gpu-2049seg-{}.nec", std::process::id()));
    std::fs::write(
        &path,
        "GW 1 2049 0 0 -5.282 0 0 5.282 0.001\nGE\nEX 0 1 1025 0 1.0 0.0\nFR 0 1 0 0 14.2 0\nEN\n",
    )
    .expect("write deck");
    // Any decline of its size is this test's failure; a lost device earns one
    // re-run (FND-215).
    let stdout = gpu_attempt("the 2049-segment solve", || {
        let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
            .args(["--solver", "hallen", "--exec", "gpu"])
            .arg(&path)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn fnec: {e}"));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "fnec exited {:?}:\n{stderr}",
            out.status.code()
        );
        match classify_gpu_stderr(&stderr) {
            GpuRun::OnDevice => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            GpuRun::Missed(why) => Err(why),
            GpuRun::Declined(line) => panic!("the device declined: {line}"),
        }
    })
    .expect("a GPU is expected");
    let _ = std::fs::remove_file(&path);
    let row = stdout
        .lines()
        .skip_while(|l| *l != "FEEDPOINTS")
        .nth(2)
        .expect("feedpoint row");
    let f: Vec<f64> = row
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    let (re, im) = (f[6], f[7]);
    assert!(
        (re - 79.692336).abs() < 2.0 && (im - 46.433387).abs() < 2.0,
        "device {re} + j{im} against the CPU's 79.692336 + j46.433387"
    );
}

/// Run fnec on a scratch copy of `deck` with `args`; (stdout, stderr).
fn run_deck(name: &str, deck: &str, args: &[&str]) -> (String, String) {
    let path = std::env::temp_dir().join(format!("fnec-fill-{name}-{}.nec", std::process::id()));
    std::fs::write(&path, deck).expect("write deck");
    let out = Command::new(env!("CARGO_BIN_EXE_fnec"))
        .args(args)
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn fnec: {e}"));
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "fnec {args:?} failed:\n{stderr}");
    (String::from_utf8_lossy(&out.stdout).into_owned(), stderr)
}

/// A 201-segment λ/2 dipole at 29.9 MHz, `extra` after `GE 0`.
fn dipole_201(extra: &str) -> String {
    format!(
        "CE\nGW 1 201 0 0 -2.5 0 0 2.5 0.001\nGE 0\n{extra}EX 0 1 101 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n"
    )
}

/// FND-293: a point the GPU-resident solve declines is filled on the CPU in f64,
/// so `--exec gpu` answers it exactly as `--exec cpu` does — and as the worker
/// does, which always fills in f64 — and the record says where the fill ran.
///
/// The device used to fill the matrix in f32 for every free-space `--exec gpu`
/// deck of 128 segments or more, and when the resident solve then declined, the
/// CPU solved the f32 matrix: on the loaded deck 106.383566 + j36.388674 against
/// 106.383595 + j36.388745, recorded `solve_exec=cpu` with no word of the f32
/// fill. The loaded deck and a bent one (path basis) are the two kinds of
/// decline. Runs on any host: without an adapter the device fill could not run
/// either, so on such a host the comparison cannot fail — where there is one it
/// turned red with the old unconditional fill.
#[test]
fn a_point_the_device_does_not_solve_is_filled_in_f64() {
    let bent = "CE\nGW 1 100 0 0 0 2.5 0 0 .001\nGW 2 100 2.5 0 0 2.5 0 2.5 .001\nGE 0\nEX 0 1 50 0 1 0\nFR 0 1 0 0 29.9 0\nEN\n";
    let loaded = dipole_201("LD 4 1 50 50 50 0\n");
    for (name, deck) in [("loaded", loaded.as_str()), ("bent", bent)] {
        let (cpu_out, cpu_err) = run_deck(&format!("{name}-cpu"), deck, &["--exec", "cpu"]);
        let (gpu_out, gpu_err) = run_deck(&format!("{name}-gpu"), deck, &["--exec", "gpu"]);
        common::assert_diag_field(&cpu_err, "fill_exec", "cpu");
        common::assert_diag_field(&gpu_err, "fill_exec", "cpu");
        common::assert_diag_field(&gpu_err, "solve_exec", "cpu");
        // The whole report — feedpoint, currents — to the byte: the same f64
        // arithmetic. The f32 fill moved the currents in their fifth digit.
        assert_eq!(
            gpu_out, cpu_out,
            "{name}: --exec gpu differs from --exec cpu"
        );
    }
}

/// The record's controls: a point the device takes is filled and solved there
/// (`fill_exec=gpu solve_exec=gpu`); a resident solve below the 128-segment fill
/// floor reads `fill_exec=cpu solve_exec=gpu`, because the device re-fills from
/// raw segment inputs for its own solve and the host matrix serves only the
/// residual check; `--exec cpu` and the MPIE fill on the CPU.
///
/// The device half needs a hardware adapter, so it is dormant in CI (which has
/// none) and runs on a GPU host; it says SKIP when it does not run.
#[test]
fn the_record_names_where_the_matrix_was_filled() {
    let big = dipole_201("");
    let (_, cpu_err) = run_deck("big-cpu", &big, &["--exec", "cpu"]);
    common::assert_diag_field(&cpu_err, "fill_exec", "cpu");
    common::assert_diag_field(&cpu_err, "solve_exec", "cpu");
    let (_, mpie_err) = run_deck("big-mpie", &big, &["--solver", "mpie", "--exec", "gpu"]);
    common::assert_diag_field(&mpie_err, "fill_exec", "cpu");

    let small =
        "CE\nGW 1 51 0 0 -5.282 0 0 5.282 .001\nGE 0\nEX 0 1 26 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n";
    for (name, deck, fill) in [("big", big.as_str(), "gpu"), ("small", small, "cpu")] {
        let attempt = gpu_attempt(name, || {
            let (_, stderr) = run_deck(&format!("{name}-gpu"), deck, &["--exec", "gpu"]);
            match classify_gpu_stderr(&stderr) {
                GpuRun::OnDevice => Ok(stderr),
                GpuRun::Missed(why) => Err(why),
                GpuRun::Declined(line) => panic!("{name}: the device declined: {line}"),
            }
        });
        let Ok(stderr) = attempt else {
            eprintln!("SKIP: no wgpu adapter on this host — {name}'s device record not checked");
            continue;
        };
        common::assert_diag_field(&stderr, "fill_exec", fill);
        common::assert_diag_field(&stderr, "solve_exec", "gpu");
    }
}
