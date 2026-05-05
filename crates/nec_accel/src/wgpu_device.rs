// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)
//
// wgpu device enumeration and no-op compute pipeline (milestone gate G2).
//
// This module provides two public async functions:
//   - `enumerate_compute_adapters` — list every wgpu adapter the runtime can see
//   - `run_noop_compute_pipeline`   — compile + dispatch a trivial WGSL shader to
//     confirm the compute stack is functional; safe to call with no real GPU present
//     (returns `NoAdapterAvailable` rather than panicking when no adapter is found)

/// Summary of a single enumerated wgpu adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInfo {
    pub name: String,
    pub backend: String,
    pub device_type: String,
}

/// Returns every compute-capable adapter visible to wgpu on this system.
///
/// The list may be empty on headless CI hosts without a software rasterizer;
/// that is not an error.
pub async fn enumerate_compute_adapters() -> Vec<AdapterInfo> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

    instance
        .enumerate_adapters(wgpu::Backends::all())
        .await
        .into_iter()
        .map(|adapter| {
            let info = adapter.get_info();
            AdapterInfo {
                name: info.name.clone(),
                backend: format!("{:?}", info.backend),
                device_type: format!("{:?}", info.device_type),
            }
        })
        .collect()
}

/// Result of `run_noop_compute_pipeline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoOpPipelineResult {
    /// The no-op compute pipeline was compiled, dispatched, and submitted successfully.
    Success,
    /// No suitable adapter (or device) could be acquired — expected on bare CI hosts
    /// without a software rasterizer.  Not an error; callers should skip, not fail.
    NoAdapterAvailable,
}

/// Minimal WGSL compute shader — one thread group, no I/O, no-op body.
const NOOP_WGSL: &str = r#"
@compute @workgroup_size(1)
fn cs_main() {}
"#;

/// Compile and dispatch a trivial WGSL compute shader to verify the wgpu compute
/// stack is operational end-to-end (gate G2 of the Phase 5 milestone sequence).
///
/// Behaviour on hosts without a real GPU:
/// - `force_fallback_adapter: true` causes wgpu to select a software rasterizer
///   (e.g. Mesa llvmpipe / Lavapipe on Linux) if available.
/// - If even that fails, `NoAdapterAvailable` is returned — the pipeline test is
///   not mandatory in bare-metal CI; gate G2 only requires the *code path* to
///   exist without panics.
pub async fn run_noop_compute_pipeline() -> NoOpPipelineResult {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

    let adapter = match instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: None,
            force_fallback_adapter: true,
        })
        .await
    {
        Ok(a) => a,
        Err(_) => return NoOpPipelineResult::NoAdapterAvailable,
    };

    let (device, queue) = match adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("fnec-noop"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })
        .await
    {
        Ok(dq) => dq,
        Err(_) => return NoOpPipelineResult::NoAdapterAvailable,
    };

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fnec-noop-shader"),
        source: wgpu::ShaderSource::Wgsl(NOOP_WGSL.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fnec-noop-layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("fnec-noop-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("fnec-noop-encoder"),
    });
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("fnec-noop-pass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&pipeline);
        // Zero-workgroup dispatch — verifies the pipeline is usable without doing work.
        cpass.dispatch_workgroups(0, 0, 0);
    }
    queue.submit(std::iter::once(encoder.finish()));

    NoOpPipelineResult::Success
}

// ---------------------------------------------------------------------------
// RP far-field wgpu kernel — milestone gate G3
// ---------------------------------------------------------------------------

/// Result of a single RP far-field GPU computation.
///
/// Radiation intensity components are returned as f64 (upcast from f32 shader output).
/// Gain values are derived by the host using the `total_radiated` normalisation.
#[derive(Debug, Clone, Copy)]
pub struct RpGpuResult {
    pub u_theta: f64,
    pub u_phi: f64,
    pub gain_total_dbi: f64,
    pub gain_theta_dbi: f64,
    pub gain_phi_dbi: f64,
    pub axial_ratio: f64,
    pub theta_deg: f64,
    pub phi_deg: f64,
}

/// Result of `run_rp_farfield_wgpu`.
#[derive(Debug, Clone)]
pub enum RpPipelineResult {
    Success(RpGpuResult),
    NoAdapterAvailable,
}

/// Segment layout expected by the WGSL shader (AoS, f32, 8 floats).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSegmentF32 {
    mid_x: f32,
    mid_y: f32,
    mid_z: f32,
    dir_x: f32,
    dir_y: f32,
    dir_z: f32,
    length: f32,
    _pad: f32,
}

/// Uniform block for the RP shader (4 × 4 bytes = 16 bytes, aligns to 16).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RpUniforms {
    k: f32,
    theta_deg: f32,
    phi_deg: f32,
    n_segs: u32,
}

/// Uniform block for the batch RP shader (k, n_segs, n_points, pad — 16 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RpBatchUniforms {
    k: f32,
    n_segs: u32,
    n_points: u32,
    _pad: u32,
}

/// The compiled WGSL RP far-field shader source.
const RP_WGSL: &str = include_str!("shaders/rp_farfield.wgsl");

/// The compiled WGSL RP far-field batch shader source (all N points, single dispatch).
const RP_BATCH_WGSL: &str = include_str!("shaders/rp_farfield_batch.wgsl");

/// Dispatch the RP far-field WGSL shader for one (θ, φ) observation direction.
///
/// # Arguments
/// * `segments`  — GPU-ready segment list from `nec_accel::gpu_kernels::GpuSegment`
/// * `currents`  — solved current vector (complex128 on CPU, downcast to f32 pairs for GPU)
/// * `k`         — wavenumber 2πf/c
/// * `theta_deg` — zenith angle in degrees
/// * `phi_deg`   — azimuth angle in degrees
///
/// Returns `RpPipelineResult::NoAdapterAvailable` when no wgpu adapter can be
/// obtained (headless CI without software rasterizer).
pub async fn run_rp_farfield_wgpu(
    segments: &[crate::gpu_kernels::GpuSegment],
    currents: &[num_complex::Complex64],
    k: f64,
    theta_deg: f64,
    phi_deg: f64,
) -> RpPipelineResult {
    // ---- device setup -------------------------------------------------------
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

    let adapter = match instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: None,
            force_fallback_adapter: true,
        })
        .await
    {
        Ok(a) => a,
        Err(_) => return RpPipelineResult::NoAdapterAvailable,
    };

    let (device, queue) = match adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("fnec-rp"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })
        .await
    {
        Ok(dq) => dq,
        Err(_) => return RpPipelineResult::NoAdapterAvailable,
    };

    let n = segments.len() as u32;

    // ---- pack segment data (f64 → f32) --------------------------------------
    let seg_data: Vec<GpuSegmentF32> = segments
        .iter()
        .map(|s| GpuSegmentF32 {
            mid_x: s.midpoint[0] as f32,
            mid_y: s.midpoint[1] as f32,
            mid_z: s.midpoint[2] as f32,
            dir_x: s.direction[0] as f32,
            dir_y: s.direction[1] as f32,
            dir_z: s.direction[2] as f32,
            length: s.length as f32,
            _pad: 0.0,
        })
        .collect();

    // ---- pack current data (Complex64 → f32 pairs) --------------------------
    let cur_data: Vec<f32> = currents
        .iter()
        .flat_map(|c| [c.re as f32, c.im as f32])
        .collect();

    // ---- create GPU buffers -------------------------------------------------
    use wgpu::util::DeviceExt;

    let seg_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rp-segs"),
        contents: bytemuck::cast_slice(&seg_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let cur_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rp-currents"),
        contents: bytemuck::cast_slice(&cur_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let uniforms = RpUniforms {
        k: k as f32,
        theta_deg: theta_deg as f32,
        phi_deg: phi_deg as f32,
        n_segs: n,
    };
    let uni_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rp-uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // Output: [u_theta_f32, u_phi_f32]
    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rp-output"),
        size: 8,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rp-readback"),
        size: 8,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // ---- bind group layout --------------------------------------------------
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rp-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rp-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: seg_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: cur_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uni_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: out_buf.as_entire_binding(),
            },
        ],
    });

    // ---- pipeline -----------------------------------------------------------
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rp-shader"),
        source: wgpu::ShaderSource::Wgsl(RP_WGSL.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rp-layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("rp-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_rp_farfield"),
        compilation_options: Default::default(),
        cache: None,
    });

    // ---- dispatch + readback ------------------------------------------------
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("rp-encoder"),
    });
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rp-pass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        cpass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&out_buf, 0, &readback_buf, 0, 8);
    queue.submit(std::iter::once(encoder.finish()));

    // Map readback buffer and read results.
    let slice = readback_buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    if rx.recv().unwrap().is_err() {
        return RpPipelineResult::NoAdapterAvailable;
    }
    let raw = slice.get_mapped_range();
    let vals: &[f32] = bytemuck::cast_slice(&raw[..8]);
    let u_theta = vals[0] as f64;
    let u_phi = vals[1] as f64;
    drop(raw);
    readback_buf.unmap();

    let result = RpGpuResult {
        u_theta,
        u_phi,
        gain_total_dbi: -999.99,
        gain_theta_dbi: -999.99,
        gain_phi_dbi: -999.99,
        axial_ratio: 0.0,
        theta_deg,
        phi_deg,
    };

    RpPipelineResult::Success(result)
}

/// Dispatch the RP far-field WGSL shader for a **batch** of (θ, φ) observation
/// directions using a single GPU submission.
///
/// All N points are dispatched in one compute pass — the shader maps each
/// thread to one observation direction (workgroup_size=64, N/64 workgroups).
/// A single readback retrieves all results.  This eliminates the per-point
/// command-encoder and device.poll overhead of the earlier per-point loop.
///
/// Returns `None` when no wgpu adapter can be obtained; the caller should
/// fall back to the CPU path in that case.
pub async fn run_rp_farfield_batch_wgpu(
    segments: &[crate::gpu_kernels::GpuSegment],
    currents: &[num_complex::Complex64],
    k: f64,
    total_radiated: f64,
    points: &[(f64, f64)],
) -> Option<Vec<RpGpuResult>> {
    if points.is_empty() {
        return Some(Vec::new());
    }

    // ---- device setup -------------------------------------------------------
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

    let adapter = match instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
    {
        Ok(a) => a,
        Err(_) => return None,
    };

    let (device, queue) = match adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("fnec-rp-batch"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })
        .await
    {
        Ok(dq) => dq,
        Err(_) => return None,
    };

    let n_segs = segments.len() as u32;
    let n_points = points.len() as u32;

    // ---- pack input data (f64 → f32) ----------------------------------------
    let seg_data: Vec<GpuSegmentF32> = segments
        .iter()
        .map(|s| GpuSegmentF32 {
            mid_x: s.midpoint[0] as f32,
            mid_y: s.midpoint[1] as f32,
            mid_z: s.midpoint[2] as f32,
            dir_x: s.direction[0] as f32,
            dir_y: s.direction[1] as f32,
            dir_z: s.direction[2] as f32,
            length: s.length as f32,
            _pad: 0.0,
        })
        .collect();

    let cur_data: Vec<f32> = currents
        .iter()
        .flat_map(|c| [c.re as f32, c.im as f32])
        .collect();

    // obs_pts: flat [theta0, phi0, theta1, phi1, ...] in degrees
    let obs_data: Vec<f32> = points
        .iter()
        .flat_map(|&(theta, phi)| [theta as f32, phi as f32])
        .collect();

    // ---- create GPU buffers -------------------------------------------------
    use wgpu::util::DeviceExt;

    let seg_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rp-batch-segs"),
        contents: bytemuck::cast_slice(&seg_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let cur_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rp-batch-currents"),
        contents: bytemuck::cast_slice(&cur_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let uniforms = RpBatchUniforms {
        k: k as f32,
        n_segs,
        n_points,
        _pad: 0,
    };
    let uni_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rp-batch-uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let obs_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rp-batch-obs"),
        contents: bytemuck::cast_slice(&obs_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    // Output: n_points × [u_theta_f32, u_phi_f32] = n_points × 8 bytes
    let out_size = n_points as u64 * 8;
    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rp-batch-output"),
        size: out_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rp-batch-readback"),
        size: out_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // ---- bind group layout (5 bindings) -------------------------------------
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rp-batch-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rp-batch-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: seg_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: cur_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uni_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: obs_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: out_buf.as_entire_binding(),
            },
        ],
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rp-batch-shader"),
        source: wgpu::ShaderSource::Wgsl(RP_BATCH_WGSL.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rp-batch-layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("rp-batch-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_rp_farfield_batch"),
        compilation_options: Default::default(),
        cache: None,
    });

    // ---- single dispatch + single readback ----------------------------------
    let n_workgroups = n_points.div_ceil(64);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("rp-batch-encoder"),
    });
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rp-batch-pass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        cpass.dispatch_workgroups(n_workgroups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&out_buf, 0, &readback_buf, 0, out_size);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback_buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    if rx.recv().unwrap().is_err() {
        return None;
    }
    let raw = slice.get_mapped_range();
    let vals: &[f32] = bytemuck::cast_slice(&raw);

    let norm = if total_radiated > 0.0 {
        4.0 * std::f64::consts::PI / total_radiated
    } else {
        0.0
    };

    const DB_FACTOR: f64 = 10.0;
    const MIN_NORM: f64 = 1e-20;

    let results: Vec<RpGpuResult> = points
        .iter()
        .enumerate()
        .map(|(i, &(theta_deg, phi_deg))| {
            let u_theta = vals[i * 2] as f64;
            let u_phi = vals[i * 2 + 1] as f64;
            let u_total = u_theta + u_phi;
            let gain_total_dbi = if u_total * norm > MIN_NORM {
                DB_FACTOR * (u_total * norm).log10()
            } else {
                -999.99
            };
            let gain_theta_dbi = if u_theta * norm > MIN_NORM {
                DB_FACTOR * (u_theta * norm).log10()
            } else {
                -999.99
            };
            let gain_phi_dbi = if u_phi * norm > MIN_NORM {
                DB_FACTOR * (u_phi * norm).log10()
            } else {
                -999.99
            };
            let axial_ratio = if u_phi.sqrt() > 1e-30 {
                u_theta.sqrt() / u_phi.sqrt()
            } else {
                0.0
            };
            RpGpuResult {
                u_theta,
                u_phi,
                gain_total_dbi,
                gain_theta_dbi,
                gain_phi_dbi,
                axial_ratio,
                theta_deg,
                phi_deg,
            }
        })
        .collect();

    drop(raw);
    readback_buf.unmap();

    Some(results)
}

// ---------------------------------------------------------------------------
// Z-matrix fill (gate G6)
// ---------------------------------------------------------------------------

/// GPU segment layout for the Z-matrix shader (10 × f32, includes radius).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSegmentZ {
    mid_x: f32,
    mid_y: f32,
    mid_z: f32,
    dir_x: f32,
    dir_y: f32,
    dir_z: f32,
    length: f32,
    radius: f32,
    _pad0: f32,
    _pad1: f32,
}

/// Uniform block for the Z-matrix fill shader (k, n, pad, pad — 16 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ZUniforms {
    k: f32,
    n: u32,
    _p0: u32,
    _p1: u32,
}

/// The compiled WGSL Z-matrix fill shader source.
const ZMATRIX_WGSL: &str = include_str!("shaders/zmatrix_fill.wgsl");

/// Input segment data for [`fill_zmatrix_wgpu`].
///
/// Mirrors the fields of `nec_solver::geometry::Segment` needed for the
/// Z-matrix kernel.  Callers convert using `From<&Segment>` or manually.
#[derive(Debug, Clone, Copy)]
pub struct ZSegmentInput {
    pub midpoint: [f64; 3],
    pub direction: [f64; 3],
    pub length: f64,
    pub radius: f64,
}

/// A single Z-matrix element (real + imaginary parts).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZElem {
    pub re: f32,
    pub im: f32,
}

/// Fill the N×N Hallén A-matrix on the GPU using a single compute dispatch.
///
/// Each thread computes one element Z[i,j].  Returns `None` when no wgpu
/// adapter is available; the caller should fall back to the CPU path.
///
/// The returned `Vec<ZElem>` has length `n*n`, stored row-major so that
/// `result[i * n + j]` gives Z[i,j].
pub async fn fill_zmatrix_wgpu(segments: &[ZSegmentInput], freq_hz: f64) -> Option<Vec<ZElem>> {
    use wgpu::util::DeviceExt;

    if segments.is_empty() {
        return Some(Vec::new());
    }

    let n = segments.len();
    let k = (2.0 * std::f64::consts::PI * freq_hz / 299_792_458.0) as f32;

    // ---- adapter + device --------------------------------------------------
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = match instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
    {
        Ok(a) => a,
        Err(_) => {
            eprintln!(
                "warning: fill_zmatrix_wgpu: no wgpu adapter available — falling back to CPU"
            );
            return None;
        }
    };

    let (device, queue) = match adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("fnec-zmatrix"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })
        .await
    {
        Ok(dq) => dq,
        Err(_) => {
            eprintln!("warning: fill_zmatrix_wgpu: device request failed — falling back to CPU");
            return None;
        }
    };

    // ---- pack segment data (f64 → f32) ------------------------------------
    let seg_data: Vec<GpuSegmentZ> = segments
        .iter()
        .map(|s| GpuSegmentZ {
            mid_x: s.midpoint[0] as f32,
            mid_y: s.midpoint[1] as f32,
            mid_z: s.midpoint[2] as f32,
            dir_x: s.direction[0] as f32,
            dir_y: s.direction[1] as f32,
            dir_z: s.direction[2] as f32,
            length: s.length as f32,
            radius: s.radius as f32,
            _pad0: 0.0,
            _pad1: 0.0,
        })
        .collect();

    // ---- buffers -----------------------------------------------------------
    let seg_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("zmatrix-segs"),
        contents: bytemuck::cast_slice(&seg_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let uniforms = ZUniforms {
        k,
        n: n as u32,
        _p0: 0,
        _p1: 0,
    };
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("zmatrix-uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // Output: 2 f32 per element (re, im), N*N elements.
    let output_size = (2 * n * n * std::mem::size_of::<f32>()) as u64;
    let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("zmatrix-output"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("zmatrix-readback"),
        size: output_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // ---- shader + pipeline -------------------------------------------------
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fnec-zmatrix-shader"),
        source: wgpu::ShaderSource::Wgsl(ZMATRIX_WGSL.into()),
    });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("zmatrix-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("zmatrix-layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("zmatrix-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_zmatrix_fill"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("zmatrix-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: seg_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buf.as_entire_binding(),
            },
        ],
    });

    // ---- dispatch ----------------------------------------------------------
    let total_threads = (n * n) as u32;
    let workgroups = total_threads.div_ceil(64);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("zmatrix-encoder"),
    });
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("zmatrix-pass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        cpass.dispatch_workgroups(workgroups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output_buf, 0, &readback_buf, 0, output_size);

    queue.submit(std::iter::once(encoder.finish()));

    // ---- readback ----------------------------------------------------------
    let slice = readback_buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    if rx.recv().unwrap().is_err() {
        return None;
    }
    let raw = slice.get_mapped_range();
    let floats: &[f32] = bytemuck::cast_slice(&raw);

    let results: Vec<ZElem> = floats
        .chunks_exact(2)
        .map(|c| ZElem { re: c[0], im: c[1] })
        .collect();

    drop(raw);
    readback_buf.unmap();

    Some(results)
}
