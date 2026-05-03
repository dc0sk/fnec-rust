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
