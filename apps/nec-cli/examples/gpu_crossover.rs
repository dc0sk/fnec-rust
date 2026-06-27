// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! PH7-CHK-005 — real discrete-GPU benchmark harness.
//!
//! Measures CPU vs real-GPU performance for the two production GPU kernels and
//! locates the problem-size crossover where the GPU path beats the CPU:
//!
//!   * **Z-matrix fill** — kernel-only (device-init excluded, via the
//!     `microbench_zmatrix_dispatch` from PH7-CHK-002) vs the CPU
//!     `assemble_z_matrix`, across segment counts. CPU cost grows ~O(N²); the
//!     GPU dispatch is roughly flat, so they cross.
//!   * **RP far-field** — production wall-clock (`run_rp_farfield_batch_wgpu`,
//!     which re-acquires the device, so it *includes* the one-time device-init)
//!     vs the CPU `compute_radiation_pattern`, across observation-point counts.
//!
//! Run on a host with a real wgpu adapter:
//!
//! ```bash
//! cargo run --release -p nec-cli --example gpu_crossover > benchmarks/real-gpu-crossover.json
//! ```
//!
//! Human-readable tables go to stderr; the machine-readable JSON artifact goes to
//! stdout. Exits 0 with an empty `{}` if no adapter is present.

use nec_accel::gpu_kernels::GpuSegment;
use nec_accel::wgpu_device::{enumerate_compute_adapters, run_rp_farfield_batch_wgpu};
use nec_accel::{microbench_zmatrix_dispatch, ZSegmentInput};
use nec_solver::{assemble_z_matrix, build_geometry, compute_radiation_pattern, FarFieldPoint};
use num_complex::Complex64;
use std::time::Instant;

const FREQ_HZ: f64 = 14.2e6;
const C0: f64 = 299_792_458.0;

fn segments(n: usize) -> Vec<nec_solver::Segment> {
    let deck_str = format!("GW 1 {n} 0 0 -5.0 0 0 5.0 0.001\nGE 0\nEN\n");
    let deck = nec_parser::parse(&deck_str).expect("parse").deck;
    build_geometry(&deck).expect("geometry")
}

/// Best-of-`reps` wall time (µs) of `f`.
fn best_us(reps: usize, mut f: impl FnMut()) -> u64 {
    let mut best = u64::MAX;
    for _ in 0..reps {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed().as_micros() as u64);
    }
    best
}

fn main() {
    let adapters = pollster::block_on(enumerate_compute_adapters());
    let Some(adapter) = adapters.first().cloned() else {
        eprintln!("gpu_crossover: no wgpu adapter — nothing to measure");
        println!("{{}}");
        return;
    };
    eprintln!(
        "Adapter: {} ({} backend, {})",
        adapter.name, adapter.backend, adapter.device_type
    );

    // ---- Z-matrix fill: kernel-only crossover -----------------------------
    eprintln!("\n== Z-matrix fill: CPU assemble vs GPU dispatch (device-init excluded) ==");
    eprintln!(
        "{:>6}  {:>12}  {:>12}  {:>8}",
        "N", "cpu_us", "gpu_us", "speedup"
    );
    let z_sizes = [32usize, 64, 128, 256, 512, 768, 1024, 1536];
    let mut z_rows = Vec::new();
    let mut z_crossover: Option<usize> = None;
    for &n in &z_sizes {
        let segs = segments(n);
        let cpu_us = best_us(5, || {
            let _ = assemble_z_matrix(&segs, FREQ_HZ);
        });
        let z_inputs: Vec<ZSegmentInput> = segs
            .iter()
            .map(|s| ZSegmentInput {
                midpoint: s.midpoint,
                direction: s.direction,
                length: s.length,
                radius: s.radius,
            })
            .collect();
        let mb = pollster::block_on(microbench_zmatrix_dispatch(&z_inputs, FREQ_HZ, 9))
            .expect("microbench (adapter was present)");
        let gpu_us = mb.dispatch_min_us;
        let speedup = cpu_us as f64 / gpu_us as f64;
        if z_crossover.is_none() && gpu_us < cpu_us {
            z_crossover = Some(n);
        }
        eprintln!("{n:>6}  {cpu_us:>12}  {gpu_us:>12}  {speedup:>7.2}x");
        z_rows.push((n, cpu_us, gpu_us, mb.device_init_us));
    }

    // ---- RP far-field: production wall-clock (device-init included) --------
    eprintln!("\n== RP far-field: CPU vs GPU production wall-clock (GPU includes device-init) ==");
    eprintln!(
        "{:>8}  {:>12}  {:>12}  {:>8}",
        "points", "cpu_us", "gpu_us", "speedup"
    );
    let segs = segments(64);
    let i_vec = vec![Complex64::new(1.0, 0.0); segs.len()];
    let ground = nec_solver::GroundModel::FreeSpace;
    let gpu_segs: Vec<GpuSegment> = segs
        .iter()
        .map(|s| GpuSegment {
            midpoint: s.midpoint,
            direction: s.direction,
            length: s.length,
        })
        .collect();
    let k = 2.0 * std::f64::consts::PI * FREQ_HZ / C0;
    let point_counts = [181usize, 721, 2701, 8101, 16201];
    let mut rp_rows = Vec::new();
    for &p in &point_counts {
        let pts_ff: Vec<FarFieldPoint> = (0..p)
            .map(|i| FarFieldPoint {
                theta_deg: (i as f64 * 180.0 / p as f64),
                phi_deg: 0.0,
            })
            .collect();
        let pts_xy: Vec<(f64, f64)> = pts_ff.iter().map(|q| (q.theta_deg, q.phi_deg)).collect();
        let cpu_us = best_us(3, || {
            let _ = compute_radiation_pattern(&segs, &i_vec, FREQ_HZ, &pts_ff, &ground);
        });
        let gpu_us = best_us(3, || {
            let _ = pollster::block_on(run_rp_farfield_batch_wgpu(
                &gpu_segs, &i_vec, k, 1.0, &pts_xy,
            ));
        });
        let speedup = cpu_us as f64 / gpu_us as f64;
        eprintln!("{p:>8}  {cpu_us:>12}  {gpu_us:>12}  {speedup:>7.2}x");
        rp_rows.push((p, cpu_us, gpu_us));
    }

    // ---- JSON artifact ----------------------------------------------------
    let z_json: Vec<String> = z_rows
        .iter()
        .map(|(n, c, g, d)| {
            format!(
                r#"{{"n_segments":{n},"cpu_us":{c},"gpu_dispatch_us":{g},"gpu_device_init_us":{d}}}"#
            )
        })
        .collect();
    let rp_json: Vec<String> = rp_rows
        .iter()
        .map(|(p, c, g)| format!(r#"{{"n_points":{p},"cpu_us":{c},"gpu_wall_us":{g}}}"#))
        .collect();
    println!(
        r#"{{"schema_version":"1","kind":"gpu_crossover","adapter":{{"name":{name:?},"backend":{backend:?},"device_type":{dtype:?}}},"freq_hz":{FREQ_HZ},"zmatrix_fill_kernel_only":[{z}],"zmatrix_fill_crossover_n":{xover},"rp_farfield_wallclock":[{rp}]}}"#,
        name = adapter.name,
        backend = adapter.backend,
        dtype = adapter.device_type,
        z = z_json.join(","),
        xover = z_crossover
            .map(|n| n.to_string())
            .unwrap_or_else(|| "null".to_string()),
        rp = rp_json.join(","),
    );

    if let Some(n) = z_crossover {
        eprintln!("\nZ-fill kernel-only crossover: GPU beats CPU at N >= {n} segments.");
    } else {
        eprintln!("\nZ-fill: GPU did not beat CPU in the tested range.");
    }
}
