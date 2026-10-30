// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-307 — a GPU-resident solve's currents are checked against the f64
//! problem, and declined to the f64 CPU solve when they miss it.
//!
//! The device fills and solves in f32 and judges itself against its own fill;
//! the CLI's residual was computed against the same f32 fill. Both were blind
//! to a defect in the fill: on an 805-segment dipole with the device's matrix
//! perturbed (diagonal × (1 + 1e-4)), the device-against-itself residual read
//! 8e-9 and the f64 one 3.4e-4. The host matrix is now the CPU's f64 fill, and
//! a residual above `DEVICE_REL_RESIDUAL_MAX` (1e-4) declines the point.
//!
//! - the printed `rel_res` of a resident point is the f64 residual of the
//!   device's currents, recomputed here from the production entry point. The
//!   f32-fill residual is 1.4× it on this deck (5.92e-6 against 8.42e-6), so
//!   reverting the host fill to the device turns this red;
//! - `FNEC_DEVICE_REL_RESIDUAL_MAX=1e-30` declines a healthy point: the warning,
//!   `solve_exec=cpu`, and a report byte-identical to `--exec cpu` — so a
//!   declined point is solved from the f64 matrix, not the f32 one (FND-293's
//!   run-time residue);
//! - the control: the override at 1 leaves the point on the device.
//!
//! The device half needs a hardware adapter, so it is dormant in CI and says
//! SKIP where it does not run.

use std::process::Command;

mod common;

use common::{classify_gpu_stderr, GpuRun, TempDeck, DEVICE_RESIDUAL_DECLINE};
use nec_accel::gpu_test::gpu_attempt;

const FREQ_HZ: f64 = 14.2e6;

/// An 801-segment λ/2 dipole at 14.2 MHz, centre fed (805 after the free-end
/// refinement).
fn dipole_801() -> String {
    "CE\nGW 1 801 0 0 -5.282 0 0 5.282 0.001\nGE 0\nEX 0 1 401 0 1 0\nFR 0 1 0 0 14.2 0\nEN\n"
        .to_string()
}

fn run(name: &str, deck: &str, exec: &str, max: Option<&str>) -> (String, String) {
    let file = TempDeck::new(&format!("fnec-devres-{name}.nec"), deck);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.args(["--solver", "hallen", "--exec", exec]).arg(&file);
    match max {
        Some(m) => cmd.env(nec_solver::validate::DEVICE_REL_RESIDUAL_MAX_ENV, m),
        None => cmd.env_remove(nec_solver::validate::DEVICE_REL_RESIDUAL_MAX_ENV),
    };
    let out = cmd.output().expect("run fnec");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "fnec --exec {exec} {name} failed:\n{stderr}"
    );
    (String::from_utf8_lossy(&out.stdout).into_owned(), stderr)
}

/// A `--exec gpu` run the device answered, or `None` (SKIP) where no adapter is.
fn on_device(name: &str, deck: &str, max: Option<&str>) -> Option<(String, String)> {
    gpu_attempt(name, || {
        let (stdout, stderr) = run(name, deck, "gpu", max);
        match classify_gpu_stderr(&stderr) {
            GpuRun::OnDevice => Ok((stdout, stderr)),
            GpuRun::Missed(why) => Err(why),
            GpuRun::Declined(line) => panic!("{name}: the device declined: {line}"),
        }
    })
    .map_err(|why| eprintln!("SKIP: {name}: {why}"))
    .ok()
}

/// The f64 residual of the device's currents on `deck`, computed as the CLI
/// should: the production device solve, the CPU's f64 fill, the shared residual.
fn f64_residual_of_the_device_solve(deck: &str) -> f64 {
    let deck = nec_parser::parse(deck).expect("parse").deck;
    let segs = nec_solver::build_geometry(&deck).expect("geometry");
    let n = segs.len();
    let rhs = nec_solver::build_hallen_rhs(&deck, &segs, FREQ_HZ).expect("rhs");
    let (endpoints, junctions) = nec_solver::merged_grouping(&segs);
    let lengths: Vec<f64> = segs.iter().map(|s| s.length).collect();
    let inputs: Vec<nec_accel::ZSegmentInput> = segs
        .iter()
        .map(|s| nec_accel::ZSegmentInput {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
            radius: s.radius,
        })
        .collect();
    let x = pollster::block_on(nec_accel::solve_hallen_gpu_resident(
        &inputs,
        &rhs.rhs,
        &rhs.cos_vec,
        &rhs.sin_vec,
        &endpoints,
        &nec_solver::sin_eligible(&endpoints, &junctions),
        &nec_solver::hallen_constraint_rows(&endpoints, &junctions, &lengths),
        FREQ_HZ,
    ))
    .expect("the device solved it for the binary");
    let (currents, c_hom) = x.split_at(n);
    let h =
        nec_solver::hallen_homogeneous(&rhs.cos_vec, &rhs.sin_vec, c_hom, &endpoints, &junctions);
    let z = nec_solver::assemble_z_matrix(&segs, FREQ_HZ);
    nec_solver::ResidualInputs::plain(h, rhs.rhs.clone(), endpoints)
        .residual(&z, currents)
        .1
}

#[test]
fn the_printed_device_residual_is_against_the_f64_problem() {
    let deck = dipole_801();
    let Some((_, stderr)) = on_device("printed", &deck, None) else {
        return;
    };
    common::assert_diag_field(&stderr, "solve_exec", "gpu");
    common::assert_diag_field(&stderr, "fill_exec", "cpu");
    let printed = common::diag_residual(&stderr, "rel_res").expect("rel_res");
    let want = f64_residual_of_the_device_solve(&deck);
    assert!(
        (printed / want - 1.0).abs() <= 0.10,
        "printed rel_res {printed:e} is not the f64 residual {want:e} of the device's currents"
    );
    assert!(
        want <= nec_solver::validate::DEVICE_REL_RESIDUAL_MAX,
        "{want:e}"
    );
}

#[test]
fn a_residual_above_the_bound_declines_to_the_f64_cpu_solve() {
    let deck = dipole_801();
    let declined = gpu_attempt("decline", || {
        let (stdout, stderr) = run("decline-gpu", &deck, "gpu", Some("1e-30"));
        match classify_gpu_stderr(&stderr) {
            GpuRun::Declined(line) => Ok((stdout, stderr, line)),
            GpuRun::Missed(why) => Err(why),
            GpuRun::OnDevice => panic!("a residual above 1e-30 did not decline:\n{stderr}"),
        }
    });
    let Ok((gpu_out, stderr, line)) = declined else {
        eprintln!("SKIP: no wgpu adapter on this host — the decline not checked");
        return;
    };
    assert!(
        line.contains(DEVICE_RESIDUAL_DECLINE) && line.contains("FND-307"),
        "{line}"
    );
    common::assert_diag_field(&stderr, "solve_exec", "cpu");
    let (cpu_out, _) = run("decline-cpu", &deck, "cpu", None);
    // The whole report to the byte: the f64 matrix, solved by the f64 route.
    assert_eq!(gpu_out, cpu_out, "a declined point differs from --exec cpu");
}

#[test]
fn a_residual_inside_the_bound_stays_on_the_device() {
    let Some((_, stderr)) = on_device("control", &dipole_801(), Some("1")) else {
        return;
    };
    common::assert_diag_field(&stderr, "solve_exec", "gpu");
    assert!(!stderr.contains(DEVICE_RESIDUAL_DECLINE), "{stderr}");
}
