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
    /// The driver's version string (wgpu's `driver_info`, e.g. "595.91.07"): a
    /// driver change moves the CPU/GPU crossover, so a calibration keys on it.
    pub driver: String,
}

impl AdapterInfo {
    fn of(info: &wgpu::AdapterInfo) -> Self {
        Self {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: info.driver_info.clone(),
        }
    }
}

/// How long a readback waits for the GPU before giving up on it (FND-196). The
/// largest deck the device solves takes about 1.3 s on a GTX 1080 Ti, so this
/// only ever ends a wait that would not have ended: on a driver fault that is not
/// reported as a lost device — an Xid 13 on the compute class, say — the work's
/// fence never signals, and an unbounded wait held `fnec` for good.
const GPU_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Wait for submitted GPU work and the buffer map to complete, for at most
/// [`GPU_WAIT_TIMEOUT`]. `Err` names why not: the device was lost, the wait timed
/// out, or the map failed — callers then degrade to the CPU path instead of
/// panicking, so a mid-run GPU fault does not abort the process (G9,
/// review-260821), and a fault that never resolves does not hang it (FND-196).
fn await_map<E: std::fmt::Display>(
    device: &wgpu::Device,
    rx: &std::sync::mpsc::Receiver<Result<(), E>>,
) -> Result<(), String> {
    let poll = |t| device.poll(wait_for(t)).map(|_| ());
    wait_readback(poll, rx, GPU_WAIT_TIMEOUT).map_err(|fault| {
        // A lost, hung or unmappable device cannot be trusted. Drop the shared
        // context so the next solve builds a fresh one instead of being pinned
        // to the CPU fallback for the rest of the process. `device` may belong
        // to one of the uncached callers, in which case this discards a healthy
        // context — that costs one rebuild and is cheaper than the alternative.
        if fault.device_suspect {
            invalidate_gpu_context();
        }
        fault.why
    })
}

/// The device wait for everything submitted, bounded by `t`. The bound is what
/// FND-196 is about: `timeout: None` waited for good on a fence that never signals.
fn wait_for(t: std::time::Duration) -> wgpu::PollType {
    wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(t),
    }
}

/// Why a readback did not complete, and whether the device is to blame.
#[derive(Debug)]
struct ReadbackFault {
    why: String,
    device_suspect: bool,
}

/// The bounded wait itself, touching no process-wide state: `poll` (the device's
/// wait for the submitted work, given the time it may take), then the map
/// callback, together within `timeout`. The poll is a parameter so each outcome —
/// a timeout above all — can be tested without racing a real GPU.
fn wait_readback<E: std::fmt::Display>(
    poll: impl FnOnce(std::time::Duration) -> Result<(), wgpu::PollError>,
    rx: &std::sync::mpsc::Receiver<Result<(), E>>,
    timeout: std::time::Duration,
) -> Result<(), ReadbackFault> {
    let start = std::time::Instant::now();
    if let Err(e) = poll(timeout) {
        return Err(ReadbackFault {
            why: match e {
                wgpu::PollError::Timeout => format!(
                    "the GPU did not finish within {} s — a device fault the driver did \
                     not report as a lost device (FND-196)",
                    timeout.as_secs_f64()
                ),
                other => format!("the device was lost ({other})"),
            },
            device_suspect: true,
        });
    }
    // The poll returned, so the map callback has run or is about to; bounded all
    // the same, since a callback that never arrives would block here instead.
    let left = timeout
        .saturating_sub(start.elapsed())
        .max(std::time::Duration::from_secs(1));
    match rx.recv_timeout(left) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(ReadbackFault {
            why: format!("the buffer map failed ({e})"),
            device_suspect: false,
        }),
        Err(_) => Err(ReadbackFault {
            why: "the buffer map never completed (FND-196)".to_string(),
            device_suspect: true,
        }),
    }
}

/// Relative-residual ceiling for the GPU-resident f32 solve, above which the
/// answer is discarded and the caller falls back to the f64 CPU solve.
///
/// **Re-measured on the rebuilt solve (FND-185, direct LU of the column-scaled M),
/// NVIDIA GTX 1080 Ti, 20 runs each, identical every run:** 2.0e-7 (21 segments),
/// 2.3e-7 (42), 6.3e-7 (51), 1.2e-6 (301) — at least 80x inside the ceiling, and
/// the 301-segment deck that failed before now solves. The table below is the
/// original derivation, on the single-workgroup normal-equations shader, whose
/// marginal cases the ceiling was placed between; it is kept because it is why
/// the value is 1e-4.
///
/// Derived from measurement, not picked. `||y - Mx|| / ||y||` against the f64 CPU
/// solve on a λ/2 dipole, three frequency points each:
///
/// | segments | rel residual   | impedance error |
/// |---------:|---------------:|----------------:|
/// |       51 | 2e-6 .. 1e-5   | none            |
/// |      101 | 3e-6 .. 7e-5   | ≤ 0.5 %         |
/// |      151 | 4e-6 .. 3e-5   | ≤ 0.1 %         |
/// |      151 | 8.9e-4         | **7 %**         |
/// |      201 | 5e-6 .. 4e-5   | ≤ 0.4 %         |
/// |      301 | 7e-5           | 0.7 %           |
/// |      301 | 2.8e-3         | **34 %**        |
/// |      301 | 4.3e-1         | **negative R**  |
///
/// Every good solve sits at or below 7e-5; the smallest bad one is 8.9e-4, an
/// order of magnitude clear. `1e-4` splits them, and corresponds to well under the
/// 2 Ω the GPU path is contracted to. Erring high only costs a CPU fallback, so a
/// different GPU landing slightly noisier fails safe.
const GPU_SOLVE_MAX_REL_RESIDUAL: f64 = 1.0e-4;

/// One `Instance` + `Adapter` + `Device` + `Queue`, shared by every solve-path
/// kernel for the life of the process.
///
/// Each kernel entry point used to build its own from scratch, so a frequency
/// sweep paid full adapter enumeration and device creation **twice per frequency
/// point** — 20 device initialisations for a 10-point sweep, which on an
/// integrated GPU cost more than the solve itself (`--exec gpu` ran ~3x slower
/// than `--exec cpu` on a 301-segment 10-point sweep purely from this).
struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// The adapter the shared device was built on — the one that solves, which on
    /// a host with an integrated and a discrete GPU is not simply the first listed.
    adapter: AdapterInfo,
}

/// What an acquisition attempt produced. The two failure variants are kept
/// separate so each caller can keep emitting its own distinct diagnostic.
/// The failures carry the driver's own words, which used to be discarded — so
/// an intermittent `request_device` failure (FND-186) could not be read.
enum GpuAcquire {
    Ready(std::sync::Arc<GpuContext>),
    NoAdapter(String),
    DeviceFailed(String),
}

/// Cache state. `Unavailable` is remembered too — on a host with no adapter,
/// retrying the probe on every call is the same waste this cache exists to
/// remove.
enum GpuCache {
    Untried,
    Ready(std::sync::Arc<GpuContext>),
    NoAdapter(String),
    DeviceFailed(String),
}

static GPU_CACHE: std::sync::Mutex<GpuCache> = std::sync::Mutex::new(GpuCache::Untried);

/// Every wgpu instance, adapter enumeration, adapter request and device request in
/// this process happens under this lock (FND-186).
///
/// Four threads each creating an `Instance` and enumerating adapters segfaulted
/// the process in 5 of 5 runs on an NVIDIA GTX 1080 Ti (driver 580.178.04,
/// Vulkan, wgpu 29); the same calls one after another, 0 of 5. Test binaries run
/// their GPU tests on parallel threads, which is how `nec_accel`'s unit tests
/// failed intermittently with the name lost — a segfault takes the whole binary.
/// The crash is in the driver's first initialisation: the same four threads did
/// not crash once another test had created a device (0 of 10). Creation is rare
/// and cheap next to the work, so serialising all of it costs nothing.
static WGPU_INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Every wgpu instance in this crate, built from the environment, so the
/// standard wgpu switches work on every route: `WGPU_BACKEND` (`vulkan`, `gl`,
/// …), `WGPU_VALIDATION=1` (the Vulkan validation layer, when installed) and
/// `WGPU_DEBUG` (and the other `WGPU_*` switches wgpu-types reads). Unset, they
/// leave wgpu's defaults. `new_without_display_handle()`
/// reads none of them — `WGPU_BACKEND=gl` silently ran Vulkan (FND-190).
///
/// Also installs [`EnvLogger`] when `RUST_LOG` is set, so wgpu's own messages —
/// validation errors among them — reach stderr; nothing in the workspace
/// installed a logger, so they were dropped.
///
/// The Vulkan validation layer is on only when `WGPU_VALIDATION` asks for it.
/// wgpu's own default turns it on in every debug build, so once the layer was
/// installed on the development host every GPU test loaded it — and the loader
/// crashed inside it when one test named a buffer through `VK_EXT_debug_utils`
/// while another test's `vkEnumeratePhysicalDevices` was `dlclose`-ing a
/// library: a segfault in `libvulkan` that killed the whole test binary, 2 of
/// 30 suite loops on 2026-10-05 (backtrace from the core; FND-214). With the
/// layer off, 0 of 300 runs. Release builds never enabled it.
fn new_instance() -> wgpu::Instance {
    install_env_logger();
    wgpu::Instance::new(instance_descriptor(
        std::env::var_os("WGPU_VALIDATION").is_some(),
    ))
}

/// The descriptor [`new_instance`] builds: wgpu's, from the environment, with
/// the validation layer withdrawn unless `WGPU_VALIDATION` was set. Split out
/// so the decision is testable without touching the process environment.
fn instance_descriptor(validation_requested: bool) -> wgpu::InstanceDescriptor {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    if !validation_requested {
        descriptor
            .flags
            .remove(wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::GPU_BASED_VALIDATION);
    }
    descriptor
}

/// A minimal stderr logger for wgpu's diagnostics, filtered by `RUST_LOG`:
/// comma-separated items, each a level (`warn`) or `target=level`
/// (`wgpu_hal=debug`); the longest matching target prefix wins.
struct EnvLogger {
    default: log::LevelFilter,
    targets: Vec<(String, log::LevelFilter)>,
}

impl EnvLogger {
    fn parse(spec: &str) -> (Self, Vec<String>) {
        let mut logger = EnvLogger {
            default: log::LevelFilter::Off,
            targets: Vec::new(),
        };
        let mut rejected = Vec::new();
        for item in spec.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            let (target, level) = match item.split_once('=') {
                Some((t, l)) => (Some(t.trim()), l.trim()),
                None => (None, item),
            };
            match (target, level.parse::<log::LevelFilter>()) {
                (Some(t), Ok(l)) => logger.targets.push((t.to_owned(), l)),
                (None, Ok(l)) => logger.default = l,
                (_, Err(_)) => rejected.push(item.to_owned()),
            }
        }
        (logger, rejected)
    }

    fn level_for(&self, target: &str) -> log::LevelFilter {
        self.targets
            .iter()
            .filter(|(t, _)| {
                target == t
                    || target
                        .strip_prefix(t.as_str())
                        .is_some_and(|rest| rest.starts_with("::"))
            })
            .max_by_key(|(t, _)| t.len())
            .map_or(self.default, |&(_, l)| l)
    }

    fn max_level(&self) -> log::LevelFilter {
        self.targets
            .iter()
            .map(|&(_, l)| l)
            .fold(self.default, std::cmp::max)
    }
}

impl log::Log for EnvLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level_for(metadata.target())
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            // Not `eprintln!`, which panics when stderr is closed — a worker
            // whose parent has gone would abort inside wgpu's own logging.
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr().lock(),
                "[{} {}] {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

fn install_env_logger() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Ok(spec) = std::env::var("RUST_LOG") else {
            return;
        };
        let (logger, rejected) = EnvLogger::parse(&spec);
        for item in rejected {
            eprintln!("warning: RUST_LOG item '{item}' is not a level or target=level; ignored");
        }
        let max = logger.max_level();
        // Fails only if the host program installed a logger first; then it is used.
        if log::set_logger(Box::leak(Box::new(logger))).is_ok() {
            log::set_max_level(max);
        }
    });
}

fn wgpu_init_guard() -> std::sync::MutexGuard<'static, ()> {
    WGPU_INIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// How many times a device has actually been created. The whole point of the
/// cache is that this stays at 1 no matter how many kernels run, so it is exposed
/// rather than inferred — a cache with no way to observe a miss cannot be tested.
static GPU_BUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Number of wgpu devices this process has created for the shared context.
///
/// Stays at `0` on a host with no adapter and at `1` once any kernel has run,
/// rising only if a device was lost and rebuilt.
pub fn gpu_context_build_count() -> usize {
    GPU_BUILDS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Drop the cached context so the next caller builds a fresh one.
///
/// Called when a kernel sees the device fail mid-run. Without this a transient
/// device loss (a driver reset, say) would pin every later solve to the CPU
/// fallback for the rest of the process, which is worse than what the
/// build-every-time code did.
fn invalidate_gpu_context() {
    if let Ok(mut c) = GPU_CACHE.lock() {
        *c = GpuCache::Untried;
    }
}

/// The shared compute context, building it on first use.
///
/// Not used by [`microbench_zmatrix_dispatch`], which **times** device
/// acquisition as one of its reported metrics — handing it a cached device would
/// corrupt the measurement it exists to produce. Nor by the two
/// `force_fallback_adapter: true` probes, which deliberately select the software
/// adapter rather than the best one.
async fn shared_gpu_context() -> GpuAcquire {
    // Decided and built under one guard, so the first callers of a sweep's
    // parallel points build ONE device instead of racing to build several
    // (FND-186). The wgpu futures resolve synchronously on native backends, and
    // `pollster` is a park loop, not an executor, so blocking here inside a
    // caller's own `block_on` cannot deadlock; nothing below re-enters this lock.
    let mut cache = GPU_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match &*cache {
        GpuCache::Ready(ctx) => return GpuAcquire::Ready(ctx.clone()),
        GpuCache::NoAdapter(why) => return GpuAcquire::NoAdapter(why.clone()),
        GpuCache::DeviceFailed(why) => return GpuAcquire::DeviceFailed(why.clone()),
        GpuCache::Untried => {}
    }

    // Always taken after GPU_CACHE, never before it, so the two cannot deadlock.
    let _init = wgpu_init_guard();
    let instance = new_instance();
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    })) {
        Ok(a) => a,
        Err(e) => {
            let why = e.to_string();
            *cache = GpuCache::NoAdapter(why.clone());
            return GpuAcquire::NoAdapter(why);
        }
    };
    let info = adapter.get_info();
    log::info!(
        target: "nec_accel",
        "GPU adapter: {} ({:?}, {:?}, driver {} {})",
        info.name,
        info.backend,
        info.device_type,
        info.driver,
        info.driver_info
    );
    // The downlevel defaults cap a storage binding at 128 MiB, which is what
    // bounds the dense solve's matrix; take what the adapter offers for the two
    // limits the size checks read (2 GiB / 4 GiB on a GTX 1080 Ti).
    let offered = adapter.limits();
    let required_limits = wgpu::Limits {
        max_storage_buffer_binding_size: offered.max_storage_buffer_binding_size,
        max_buffer_size: offered.max_buffer_size,
        ..wgpu::Limits::downlevel_defaults()
    };
    let (device, queue) =
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fnec-shared"),
            required_limits,
            ..Default::default()
        })) {
            Ok(dq) => dq,
            Err(e) => {
                let why = e.to_string();
                *cache = GpuCache::DeviceFailed(why.clone());
                return GpuAcquire::DeviceFailed(why);
            }
        };

    GPU_BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ctx = std::sync::Arc::new(GpuContext {
        device,
        queue,
        adapter: AdapterInfo::of(&info),
    });
    *cache = GpuCache::Ready(ctx.clone());
    GpuAcquire::Ready(ctx)
}

/// Returns every compute-capable adapter visible to wgpu on this system.
///
/// The list may be empty on headless CI hosts without a software rasterizer;
/// that is not an error.
pub async fn enumerate_compute_adapters() -> Vec<AdapterInfo> {
    let _init = wgpu_init_guard();
    let instance = new_instance();

    pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()))
        .into_iter()
        .map(|adapter| AdapterInfo::of(&adapter.get_info()))
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
    let init = wgpu_init_guard();
    let instance = new_instance();

    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::None,
        compatible_surface: None,
        force_fallback_adapter: true,
    })) {
        Ok(a) => a,
        Err(_) => return NoOpPipelineResult::NoAdapterAvailable,
    };

    let (device, queue) =
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fnec-noop"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })) {
            Ok(dq) => dq,
            Err(_) => return NoOpPipelineResult::NoAdapterAvailable,
        };
    drop(init);

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
/// * `segments`  — GPU-ready segment list from `nec_accel::kernel_reference::GpuSegment`
/// * `currents`  — solved current vector (complex128 on CPU, downcast to f32 pairs for GPU)
/// * `k`         — wavenumber 2πf/c
/// * `theta_deg` — zenith angle in degrees
/// * `phi_deg`   — azimuth angle in degrees
///
/// Returns `RpPipelineResult::NoAdapterAvailable` when no wgpu adapter can be
/// obtained (headless CI without software rasterizer).
pub async fn run_rp_farfield_wgpu(
    segments: &[crate::kernel_reference::GpuSegment],
    currents: &[num_complex::Complex64],
    k: f64,
    theta_deg: f64,
    phi_deg: f64,
) -> RpPipelineResult {
    // ---- device setup -------------------------------------------------------
    let init = wgpu_init_guard();
    let instance = new_instance();

    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::None,
        compatible_surface: None,
        force_fallback_adapter: true,
    })) {
        Ok(a) => a,
        Err(_) => return RpPipelineResult::NoAdapterAvailable,
    };

    let (device, queue) =
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("fnec-rp"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })) {
            Ok(dq) => dq,
            Err(_) => return RpPipelineResult::NoAdapterAvailable,
        };
    drop(init);

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
    if await_map(&device, &rx).is_err() {
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
    segments: &[crate::kernel_reference::GpuSegment],
    currents: &[num_complex::Complex64],
    k: f64,
    total_radiated: f64,
    points: &[(f64, f64)],
) -> Option<Vec<RpGpuResult>> {
    if points.is_empty() {
        return Some(Vec::new());
    }

    // ---- device setup (shared, built once per process) ----------------------
    let ctx = match shared_gpu_context().await {
        GpuAcquire::Ready(c) => c,
        GpuAcquire::NoAdapter(_) | GpuAcquire::DeviceFailed(_) => return None,
    };
    let (device, queue) = (&ctx.device, &ctx.queue);

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
    // Four f32 per direction: Fθ and Fφ, complex (FND-216).
    let out_size = n_points as u64 * 16;
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
    // No RP card reaches the per-dimension limit (361 × 181 points is 1022
    // workgroups), but every dispatch the shaders index linearly goes through the
    // one grid rule.
    let (gx, gy) = linear_dispatch_grid(
        n_points,
        device.limits().max_compute_workgroups_per_dimension,
    );
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
        cpass.dispatch_workgroups(gx, gy, 1);
    }
    encoder.copy_buffer_to_buffer(&out_buf, 0, &readback_buf, 0, out_size);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback_buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    if await_map(device, &rx).is_err() {
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
            let (ft_re, ft_im) = (vals[i * 4] as f64, vals[i * 4 + 1] as f64);
            let (fp_re, fp_im) = (vals[i * 4 + 2] as f64, vals[i * 4 + 3] as f64);
            let u_theta = ft_re * ft_re + ft_im * ft_im;
            let u_phi = fp_re * fp_re + fp_im * fp_im;
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
            // The polarisation ellipse's axial ratio, signed by the sense of
            // rotation — the CPU's `polarization_axial_ratio` (Stokes S3) term for
            // term. This was √Uθ/√Uφ, an amplitude ratio: a tilted dipole, linearly
            // polarised everywhere, printed 2.2161 where the CPU prints 0 (FND-216).
            let s0 = u_theta + u_phi;
            let axial_ratio = if s0 <= 1e-30 {
                0.0
            } else {
                // 2·Im(Fθ·conj(Fφ))
                let s3 = 2.0 * (ft_im * fp_re - ft_re * fp_im);
                (0.5 * (s3 / s0).clamp(-1.0, 1.0).asin()).tan()
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

/// The WGSL GPU-resident Hallén solve (direct LU of the column-scaled M, one
/// dispatch per phase — FND-185).
const HALLEN_SOLVE_WGSL: &str = include_str!("shaders/hallen_lu_solve.wgsl");

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
/// Each thread computes one element Z[i,j].  Returns `Err` with the reason when
/// the device cannot do it — no adapter, a failed device, or a matrix larger than
/// its storage binding — and the caller falls back to the CPU path and says why.
/// It used to return `None` for every one of those, and the CLI reported each as
/// "no wgpu adapter".
///
/// The returned `Vec<ZElem>` has length `n*n`, stored row-major so that
/// `result[i * n + j]` gives Z[i,j].
pub async fn fill_zmatrix_wgpu(
    segments: &[ZSegmentInput],
    freq_hz: f64,
) -> Result<Vec<ZElem>, String> {
    use wgpu::util::DeviceExt;

    if segments.is_empty() {
        return Ok(Vec::new());
    }

    let n = segments.len();
    let k = (2.0 * std::f64::consts::PI * freq_hz / 299_792_458.0) as f32;

    // ---- adapter + device (shared, built once per process) -----------------
    let ctx = match shared_gpu_context().await {
        GpuAcquire::Ready(c) => c,
        GpuAcquire::NoAdapter(why) => return Err(format!("no wgpu adapter available ({why})")),
        GpuAcquire::DeviceFailed(why) => return Err(format!("device request failed ({why})")),
    };
    let (device, queue) = (&ctx.device, &ctx.queue);
    let capacity = dense_matrix_capacity(&device.limits());
    if n > capacity {
        return Err(format!(
            "{n} segments is more than the {capacity} a {} MiB storage binding holds",
            device.limits().max_storage_buffer_binding_size >> 20
        ));
    }

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
    let (gx, gy) = linear_dispatch_grid(
        (n * n) as u32,
        device.limits().max_compute_workgroups_per_dimension,
    );

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
        cpass.dispatch_workgroups(gx, gy, 1);
    }
    encoder.copy_buffer_to_buffer(&output_buf, 0, &readback_buf, 0, output_size);

    queue.submit(std::iter::once(encoder.finish()));

    // ---- readback ----------------------------------------------------------
    let slice = readback_buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    if let Err(why) = await_map(device, &rx) {
        return Err(format!("the device failed during the readback: {why}"));
    }
    let raw = slice.get_mapped_range();
    let floats: &[f32] = bytemuck::cast_slice(&raw);

    let results: Vec<ZElem> = floats
        .chunks_exact(2)
        .map(|c| ZElem { re: c[0], im: c[1] })
        .collect();

    drop(raw);
    readback_buf.unmap();

    Ok(results)
}

// ---------------------------------------------------------------------------
// In-process GPU microbenchmark (PH7-CHK-002)
// ---------------------------------------------------------------------------

/// Result of [`microbench_zmatrix_dispatch`].
///
/// Separates the one-time wgpu **device-initialization** cost from the per-kernel
/// **dispatch** cost. The across-process G5 gate
/// (`apps/nec-cli/tests/gpu_benchmark_gate.rs`) cannot make this split because
/// each of its samples is a fresh process that re-pays device-init; this
/// in-process measurement pays it once and times many reused dispatches.
#[derive(Debug, Clone, Copy)]
pub struct GpuMicrobench {
    /// Problem size (segments) the microbenchmark filled.
    pub n_segments: usize,
    /// Number of timed dispatches.
    pub n_dispatches: usize,
    /// One-time wgpu device acquisition (instance + adapter + device), µs.
    pub device_init_us: u64,
    /// Best (minimum) per-dispatch submit→complete time, µs (device-init excluded).
    pub dispatch_min_us: u64,
    /// Median per-dispatch submit→complete time, µs.
    pub dispatch_median_us: u64,
}

/// In-process GPU microbenchmark of the Z-matrix-fill kernel (PH7-CHK-002).
///
/// Acquires the wgpu device **once** (timed as `device_init_us`), builds the
/// pipeline and buffers once, then runs warm-up plus `reps` timed dispatches that
/// reuse all resources — so `dispatch_min_us` / `dispatch_median_us` measure pure
/// kernel dispatch + execution with no device-init contamination. Uses the
/// minimum over `reps` as the headline figure to reject positive-only wall-clock
/// noise (the source of the across-process gate's flakiness).
///
/// Returns `None` when no wgpu adapter is available.
pub async fn microbench_zmatrix_dispatch(
    segments: &[ZSegmentInput],
    freq_hz: f64,
    reps: usize,
) -> Option<GpuMicrobench> {
    use std::time::Instant;
    use wgpu::util::DeviceExt;

    if segments.is_empty() || reps == 0 {
        return None;
    }
    let n = segments.len();
    let k = (2.0 * std::f64::consts::PI * freq_hz / 299_792_458.0) as f32;

    // ---- timed device acquisition -----------------------------------------
    let t_dev = Instant::now();
    let init = wgpu_init_guard();
    let instance = new_instance();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("fnec-microbench"),
        required_limits: wgpu::Limits::downlevel_defaults(),
        ..Default::default()
    }))
    .ok()?;
    drop(init);
    let device_init_us = t_dev.elapsed().as_micros() as u64;

    // ---- build pipeline + buffers once ------------------------------------
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
    let seg_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("microbench-segs"),
        contents: bytemuck::cast_slice(&seg_data),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("microbench-uniforms"),
        contents: bytemuck::bytes_of(&ZUniforms {
            k,
            n: n as u32,
            _p0: 0,
            _p1: 0,
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("microbench-output"),
        size: (2 * n * n * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("microbench-shader"),
        source: wgpu::ShaderSource::Wgsl(ZMATRIX_WGSL.into()),
    });
    let entry = |binding: u32, read_only: bool, uniform: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: if uniform {
                wgpu::BufferBindingType::Uniform
            } else {
                wgpu::BufferBindingType::Storage { read_only }
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("microbench-bgl"),
        entries: &[
            entry(0, true, false),
            entry(1, false, true),
            entry(2, false, false),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("microbench-layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("microbench-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_zmatrix_fill"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("microbench-bg"),
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
    let (gx, gy) = linear_dispatch_grid(
        (n * n) as u32,
        device.limits().max_compute_workgroups_per_dimension,
    );

    let one_dispatch = || {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("microbench-encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("microbench-pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(gx, gy, 1);
        }
        queue.submit(std::iter::once(encoder.finish()));
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
    };

    // ---- warm-up (shader compile / lazy init), then timed reps ------------
    for _ in 0..2 {
        one_dispatch();
    }
    let mut times: Vec<u64> = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t = Instant::now();
        one_dispatch();
        times.push(t.elapsed().as_micros() as u64);
    }
    times.sort_unstable();

    Some(GpuMicrobench {
        n_segments: n,
        n_dispatches: reps,
        device_init_us,
        dispatch_min_us: times[0],
        dispatch_median_us: times[times.len() / 2],
    })
}

// ---------------------------------------------------------------------------
// GPU-resident Hallén solve (gate PH7-CHK-003)
// ---------------------------------------------------------------------------

/// Uniform block for the Hallén solve shader: n, s, nc (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SolveParams {
    n: u32,
    s: u32,
    nc: u32,
    _pad: u32,
}

/// Per-dispatch parameters of the solve (column, mode), one per dynamic offset.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SolveStep {
    col: u32,
    mode: u32,
    _p0: u32,
    _p1: u32,
}

/// Refinement steps after the direct LU solve. The design review's f32 replica
/// measured a contraction of about ε·cond(M) ≈ 1e-4 per step, so one step is
/// margin rather than necessity; two keep a noisier device inside the gate.
const REFINE_STEPS: u32 = 2;

/// The smallest deck the CLI and the worker send to [`solve_hallen_gpu_resident`].
///
/// One value for both callers (FND-078: it was two independent `16`s). It is a
/// floor, not a crossover. The single-workgroup solve measured 0.04x-0.48x the CPU
/// at every size (PH7-CHK-003); the rebuilt one crosses over near 500 segments for
/// one point on a GTX 1080 Ti (FND-185). Where the CLI's automatic pick sends a
/// deck is the host's own crossover, measured by `fnec calibrate` (FND-225). Below
/// this floor the dispatch is pure overhead; between it and the crossover the path
/// is taken only for an explicit `--exec gpu`.
/// The value itself has no recorded measurement behind it.
pub const MIN_GPU_RESIDENT_SEGS: usize = 16;

/// Why [`solve_hallen_gpu_resident`] returned no solution.
///
/// It was `None` for all of these, and every GPU gate read `None` as "no adapter,
/// skip" — so a shader that had stopped converging passed as a skip on a machine
/// that HAS a GPU (FND-163: zeroing the sin column in the shader failed nothing).
/// A caller that falls back to the CPU treats them alike; a test must not.
#[derive(Debug, Clone, PartialEq)]
pub enum GpuSolveDeclined {
    /// No wgpu adapter. The only reason a gate may skip on.
    NoAdapter,
    /// An adapter exists but the device request, the device, or the readback
    /// failed.
    DeviceFailed,
    /// The input is outside what the device solve takes (a size or shape the
    /// shader does not handle); the CPU solve is the right answer, not a fault.
    OutOfClass(&'static str),
    /// The f32 solve ran and its residual check rejected the answer.
    NotConverged { rel_residual: f64 },
}

/// The dispatch grid for `total` invocations of a 64-wide kernel whose shader
/// derives its linear index as `gid.x + gid.y * num_workgroups.x * 64`.
///
/// One dimension holds at most `max_per_dim` workgroups (65535 on common
/// hardware), and a 1-D dispatch of an N×N fill passed it at N = 2048: wgpu
/// panicked, `dispatch group size [65536,1,1] must be ≤ 65535`, and the process
/// exited 101. The grid is (x, y) with x ≤ `max_per_dim` and x·y·64 ≥ `total`;
/// the shaders' `idx < total` guards drop the overhang.
pub fn linear_dispatch_grid(total: u32, max_per_dim: u32) -> (u32, u32) {
    let groups = total.div_ceil(64);
    if groups <= max_per_dim {
        return (groups, 1);
    }
    let y = groups.div_ceil(max_per_dim);
    (groups.div_ceil(y), y)
}

/// The largest system the device holds as one dense f32-complex matrix: the
/// biggest `S` with `8·S²` bytes within both the storage-binding and the buffer
/// size limit. It replaces a fixed `MAX_S = 1024`, whose reason (fixed-size
/// workgroup arrays) the rebuilt solve shader no longer has; its only arrays are
/// 64-wide. 4096 at the downlevel defaults' 128 MiB binding, 16384 at 2 GiB.
pub fn dense_matrix_capacity(limits: &wgpu::Limits) -> usize {
    let bytes = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size);
    let mut s = ((bytes / 8) as f64).sqrt() as u64;
    while 8 * (s + 1) * (s + 1) <= bytes {
        s += 1;
    }
    while s > 0 && 8 * s * s > bytes {
        s -= 1;
    }
    s as usize
}

/// [`dense_matrix_capacity`] of the shared device — the ceiling the dense solve
/// checks — or `None` where there is no device.
pub fn shared_device_dense_capacity() -> Option<usize> {
    match pollster::block_on(shared_gpu_context()) {
        GpuAcquire::Ready(ctx) => Some(dense_matrix_capacity(&ctx.device.limits())),
        GpuAcquire::NoAdapter(_) | GpuAcquire::DeviceFailed(_) => None,
    }
}

/// The adapter the shared device solves on, or `None` where there is no device.
/// The one a CPU/GPU calibration must name: `hardware_adapter_present` accepts
/// any non-CPU adapter and enumeration lists them all, but the device is built
/// on the high-performance one.
pub fn shared_adapter_info() -> Option<AdapterInfo> {
    match pollster::block_on(shared_gpu_context()) {
        GpuAcquire::Ready(ctx) => Some(ctx.adapter.clone()),
        GpuAcquire::NoAdapter(_) | GpuAcquire::DeviceFailed(_) => None,
    }
}

/// Print a GPU decline once per process for each distinct reason. A GPU sweep
/// asks at every point, and the reason — a deck too large for the device — does
/// not change between them.
fn warn_once(message: String) {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let mut seen = SEEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !seen.contains(&message) {
        eprintln!("{message}");
        seen.push(message);
    }
}

/// Whether a hardware (non-CPU) wgpu adapter is present — for tests that must
/// know whether a missing GPU result is a skip or a failure (FND-163).
pub async fn hardware_adapter_present() -> bool {
    enumerate_compute_adapters()
        .await
        .iter()
        .any(|a| a.device_type != "Cpu")
}

/// GPU-resident Hallén dense solve (PH7-CHK-003).
///
/// Fills the N×N Hallén Z-matrix on the GPU and then solves the square augmented
/// system **on the device** by LU of its column-scaled form, returning only the
/// length-`S` solution vector (`S = N + W`): `x[..N]` are the segment currents
/// and `x[N..]` the per-wire homogeneous constants. The full matrix never
/// leaves the GPU.
///
/// `rhs` and `cos_vec` are the Hallén RHS / projection vectors (length `N`),
/// `wire_endpoints` the per-wire `(first, last)` segment indices, and
/// `constraint_rows` the augmented rows `(col_a, col_b, val_a, val_b)` — pass
/// `nec_solver::hallen_constraint_rows(wire_endpoints, junctions)`, so the
/// free-end extrapolation (FND-156) and the junction rows are built in one
/// place and cannot drift from the CPU solve this reproduces.
///
/// Returns [`GpuSolveDeclined`] when there is no solution — no adapter, a device
/// fault, an input the shader does not take, or an f32 answer that failed its
/// residual check. A caller falls back to the f64 CPU solve on any of them; a
/// gate may skip only on `NoAdapter`. All GPU arithmetic is f32; the result is
/// intended to be validated to the 2 Ω GPU-path tolerance, not the f64 corpus gate.
#[allow(clippy::too_many_arguments)] // the CPU solve's inputs, one for one
pub async fn solve_hallen_gpu_resident(
    segments: &[ZSegmentInput],
    rhs: &[num_complex::Complex64],
    cos_vec: &[f64],
    sin_vec: &[f64],
    wire_endpoints: &[(usize, usize)],
    sin_eligible: &[bool],
    constraint_rows: &[(usize, Option<usize>, f64, f64)],
    freq_hz: f64,
) -> Result<Vec<num_complex::Complex64>, GpuSolveDeclined> {
    use wgpu::util::DeviceExt;

    let n = segments.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    if rhs.len() != n || cos_vec.len() != n || sin_vec.len() != n {
        return Err(GpuSolveDeclined::OutOfClass(
            "vector lengths differ from the segment count",
        ));
    }

    // ---- host-side augmented-system metadata (mirrors solve_hallen) --------
    // The constraint rows come from the caller, built over these same
    // endpoints; with no wires there is nothing consistent to build them from,
    // so leave that case to the CPU solve.
    if wire_endpoints.is_empty() {
        return Err(GpuSolveDeclined::OutOfClass("no wires"));
    }
    let endpoints = wire_endpoints;

    // Constraint rows encoded as [col_a, col_b_or_-1, val_a, val_b].
    let constraints: Vec<[f32; 4]> = constraint_rows
        .iter()
        .map(|&(a, b, va, vb)| [a as f32, b.map_or(-1.0, |b| b as f32), va as f32, vb as f32])
        .collect();

    let w = endpoints.len();
    if sin_eligible.len() != w {
        return Err(GpuSolveDeclined::OutOfClass(
            "sin_eligible has the wrong length",
        ));
    }
    // The sin homogeneous column (FND-158) for the wires `sin_eligible` marks —
    // `nec_solver::sin_eligible`, the rule the CPU solve uses.
    let mut sin_col = vec![-1.0f32; w];
    let mut next = n + w;
    for (wi, &e) in sin_eligible.iter().enumerate() {
        if e {
            sin_col[wi] = next as f32;
            next += 1;
        }
    }
    let s = next;
    let nc = constraints.len();
    // The shader factors M itself, which needs it square. Straight wires with two
    // free ends each give R = N + 2W = S; anything else goes to the CPU.
    if n + nc != s {
        return Err(GpuSolveDeclined::OutOfClass(
            "the augmented system is not square",
        ));
    }

    let mut row_wire = vec![0u32; n];
    for (wi, &(first, last)) in endpoints.iter().enumerate() {
        for rw in row_wire.iter_mut().take(last + 1).skip(first) {
            *rw = wi as u32;
        }
    }

    // meta buffer: per-seg [cos, sin, rhs_re, rhs_im, cos_col, sin_col (-1 = none)]
    // then constraint rows.
    let mut meta: Vec<f32> = Vec::with_capacity(6 * n + 4 * nc);
    for r in 0..n {
        meta.push(cos_vec[r] as f32);
        meta.push(sin_vec[r] as f32);
        meta.push(rhs[r].re as f32);
        meta.push(rhs[r].im as f32);
        meta.push((n as u32 + row_wire[r]) as f32);
        meta.push(sin_col[row_wire[r] as usize]);
    }
    for c in &constraints {
        meta.extend_from_slice(c);
    }

    let k = (2.0 * std::f64::consts::PI * freq_hz / 299_792_458.0) as f32;

    // ---- adapter + device (shared, built once per process) -----------------
    let ctx = match shared_gpu_context().await {
        GpuAcquire::Ready(c) => c,
        GpuAcquire::NoAdapter(why) => {
            eprintln!(
                "warning: solve_hallen_gpu_resident: no wgpu adapter available ({why}) — falling back to CPU"
            );
            return Err(GpuSolveDeclined::NoAdapter);
        }
        GpuAcquire::DeviceFailed(why) => {
            eprintln!(
                "warning: solve_hallen_gpu_resident: device request failed ({why}) — falling back to CPU"
            );
            return Err(GpuSolveDeclined::DeviceFailed);
        }
    };
    let (device, queue) = (&ctx.device, &ctx.queue);
    let grid = |total: u32| {
        linear_dispatch_grid(total, device.limits().max_compute_workgroups_per_dimension)
    };

    // The one size ceiling: the dense matrix must fit one storage binding. It was
    // a fixed MAX_S = 1024, declined without a word — the diag label read
    // `gpu(cpu-fallback)` either way — while the solve shader's only arrays are
    // 64 wide. The serial triangular solve is the remaining cost at large S
    // (a 2049-segment dipole solves in about 1.3 s on a GTX 1080 Ti), not a limit.
    let capacity = dense_matrix_capacity(&device.limits());
    if s > capacity {
        warn_once(format!(
            "warning: solve_hallen_gpu_resident: a {s}-unknown system is more than the \
             {capacity} a {} MiB storage binding holds — falling back to CPU",
            device.limits().max_storage_buffer_binding_size >> 20
        ));
        return Err(GpuSolveDeclined::OutOfClass(
            "system larger than the device's storage binding",
        ));
    }

    // ---- segment data for the fill kernel ---------------------------------
    let seg_data: Vec<GpuSegmentZ> = segments
        .iter()
        .map(|sg| GpuSegmentZ {
            mid_x: sg.midpoint[0] as f32,
            mid_y: sg.midpoint[1] as f32,
            mid_z: sg.midpoint[2] as f32,
            dir_x: sg.direction[0] as f32,
            dir_y: sg.direction[1] as f32,
            dir_z: sg.direction[2] as f32,
            length: sg.length as f32,
            radius: sg.radius as f32,
            _pad0: 0.0,
            _pad1: 0.0,
        })
        .collect();
    let seg_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("hallen-segs"),
        contents: bytemuck::cast_slice(&seg_data),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let zfill_uniforms = ZUniforms {
        k,
        n: n as u32,
        _p0: 0,
        _p1: 0,
    };
    let zfill_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("hallen-zfill-uniforms"),
        contents: bytemuck::bytes_of(&zfill_uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // ---- device-resident Z buffer (never copied back) ----------------------
    let z_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hallen-z"),
        size: (2 * n * n * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    // ---- solve buffers -----------------------------------------------------
    let solve_params = SolveParams {
        n: n as u32,
        s: s as u32,
        nc: nc as u32,
        _pad: 0,
    };
    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("hallen-params"),
        contents: bytemuck::bytes_of(&solve_params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let meta_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("hallen-meta"),
        contents: bytemuck::cast_slice(&meta),
        usage: wgpu::BufferUsages::STORAGE,
    });
    // lu: one S×S complex matrix (the column-scaled M, factored in place).
    let mat_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hallen-lu"),
        size: (2 * s * s * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    // vec_: 7 complex vectors of stride R + 1 (x, b, w, t, out, d, piv); index R
    // of slot t carries the residual norms, where no invocation reads.
    let rows = n + nc;
    let stride = rows + 1;
    let vec_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hallen-vec"),
        size: (2 * 7 * stride * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    // Read back slots 3 and 4 in one copy: SLOT_T[R] carries the residual scalars
    // the accuracy gate needs (.x = ||y - Mx||^2, .y = ||y||^2), and SLOT_OUT is the
    // full S-element solution (currents = x[..n], per-wire homogeneous constants =
    // x[n..]).
    let out_slot_byte_offset = (2 * 3 * stride * std::mem::size_of::<f32>()) as u64;
    let readback_size = (2 * 2 * stride * std::mem::size_of::<f32>()) as u64;

    // Per-dispatch steps behind dynamic uniform offsets: index 0 is (0, set),
    // 1 is (0, add), then one entry per column.
    let align = device.limits().min_uniform_buffer_offset_alignment as usize;
    let step_size = std::mem::size_of::<SolveStep>();
    let slot = step_size.div_ceil(align) * align;
    let mut steps = vec![0u8; slot * (s + 2)];
    let put = |buf: &mut [u8], i: usize, st: SolveStep| {
        buf[i * slot..i * slot + step_size].copy_from_slice(bytemuck::bytes_of(&st));
    };
    put(
        &mut steps,
        0,
        SolveStep {
            col: 0,
            mode: 0,
            _p0: 0,
            _p1: 0,
        },
    );
    put(
        &mut steps,
        1,
        SolveStep {
            col: 0,
            mode: 1,
            _p0: 0,
            _p1: 0,
        },
    );
    for c in 0..s {
        put(
            &mut steps,
            c + 2,
            SolveStep {
                col: c as u32,
                mode: 0,
                _p0: 0,
                _p1: 0,
            },
        );
    }
    let step_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("hallen-steps"),
        contents: &steps,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let step_at = |i: usize| (i * slot) as u32;
    let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hallen-readback"),
        size: readback_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // ---- bind-group-layout entry helpers ----------------------------------
    let storage_entry = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };

    // ---- fill pipeline (reuses zmatrix_fill.wgsl) --------------------------
    let fill_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("hallen-zfill-shader"),
        source: wgpu::ShaderSource::Wgsl(ZMATRIX_WGSL.into()),
    });
    let fill_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hallen-zfill-bgl"),
        entries: &[
            storage_entry(0, true),
            uniform_entry(1),
            storage_entry(2, false),
        ],
    });
    let fill_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hallen-zfill-layout"),
        bind_group_layouts: &[Some(&fill_bgl)],
        immediate_size: 0,
    });
    let fill_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("hallen-zfill-pipeline"),
        layout: Some(&fill_layout),
        module: &fill_shader,
        entry_point: Some("cs_zmatrix_fill"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let fill_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("hallen-zfill-bg"),
        layout: &fill_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: seg_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: zfill_uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: z_buf.as_entire_binding(),
            },
        ],
    });

    // ---- solve pipelines: one per phase (FND-185) ---------------------------
    // Every hand-off between invocations is a dispatch boundary; see the header
    // of hallen_lu_solve.wgsl for why a storage barrier inside one dispatch was
    // not enough on an NVIDIA device.
    let solve_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("hallen-solve-shader"),
        source: wgpu::ShaderSource::Wgsl(HALLEN_SOLVE_WGSL.into()),
    });
    let step_entry = wgpu::BindGroupLayoutEntry {
        binding: 5,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: std::num::NonZeroU64::new(step_size as u64),
        },
        count: None,
    };
    let solve_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hallen-solve-bgl"),
        entries: &[
            storage_entry(0, true),
            uniform_entry(1),
            storage_entry(2, true),
            storage_entry(3, false),
            storage_entry(4, false),
            step_entry,
        ],
    });
    let solve_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hallen-solve-layout"),
        bind_group_layouts: &[Some(&solve_bgl)],
        immediate_size: 0,
    });
    let pipe = |entry: &'static str| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: Some(&solve_layout),
            module: &solve_shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    };
    let p_col_scale = pipe("cs_col_scale");
    let p_build = pipe("cs_build");
    let p_pivot_search = pipe("cs_pivot_search");
    let p_pivot_swap = pipe("cs_pivot_swap");
    let p_eliminate = pipe("cs_eliminate");
    let p_rhs = pipe("cs_rhs");
    let p_permute = pipe("cs_permute");
    let p_forward = pipe("cs_forward");
    let p_backward = pipe("cs_backward");
    let p_divide_diag = pipe("cs_divide_diag");
    let p_update_x = pipe("cs_update_x");
    let p_residual = pipe("cs_residual");
    let p_norms = pipe("cs_norms");
    let p_copy_out = pipe("cs_copy_out");
    let solve_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("hallen-solve-bg"),
        layout: &solve_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: z_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: meta_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: mat_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: vec_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &step_buf,
                    offset: 0,
                    size: std::num::NonZeroU64::new(step_size as u64),
                }),
            },
        ],
    });

    // ---- encode both passes (Z stays on device between them) ---------------
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("hallen-encoder"),
    });
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("hallen-zfill-pass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&fill_pipeline);
        cpass.set_bind_group(0, &fill_bg, &[]);
        let (gx, gy) = grid((n * n) as u32);
        cpass.dispatch_workgroups(gx, gy, 1);
    }
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("hallen-solve-pass"),
            timestamp_writes: None,
        });
        let groups = |count: usize| grid(count as u32);
        let mut run = |p: &wgpu::ComputePipeline, step: usize, (gx, gy): (u32, u32)| {
            cpass.set_pipeline(p);
            cpass.set_bind_group(0, &solve_bg, &[step_at(step)]);
            cpass.dispatch_workgroups(gx, gy, 1);
        };
        // W = (LU)⁻¹ P B, one dispatch per column each way: every hand-off between
        // the columns is a dispatch boundary (FND-185). It was one invocation
        // doing all of it — 0.73 s per pass at S = 2048, three passes a solve; a
        // 2049-segment dipole went from 3.5 s to 1.3 s.
        macro_rules! triangular_solve {
            () => {
                run(&p_permute, 0, (1, 1));
                for c in 0..s.saturating_sub(1) {
                    run(&p_forward, c + 2, groups(s - c - 1));
                }
                for c in (1..s).rev() {
                    run(&p_backward, c + 2, groups(c));
                }
                run(&p_divide_diag, 0, groups(s));
            };
        }
        // Scale and build M D⁻¹.
        run(&p_col_scale, 0, groups(s));
        run(&p_build, 0, groups(s * s));
        // LU with partial pivoting: search, swap, eliminate per column.
        for c in 0..s {
            run(&p_pivot_search, c + 2, (1, 1));
            run(&p_pivot_swap, c + 2, groups(s));
            if c + 1 < s {
                run(&p_eliminate, c + 2, groups(s - c - 1));
            }
        }
        // x = D⁻¹ (LU)⁻¹ P y.
        run(&p_rhs, 0, groups(rows));
        triangular_solve!();
        run(&p_update_x, 0, groups(s));
        // Refinement in M-space: t = y − Mx, x += D⁻¹ (LU)⁻¹ P t.
        for _ in 0..REFINE_STEPS {
            run(&p_residual, 0, groups(rows));
            triangular_solve!();
            run(&p_update_x, 1, groups(s));
        }
        // Final residual for the host's gate, then the solution.
        run(&p_residual, 0, groups(rows));
        run(&p_norms, 0, (1, 1));
        run(&p_copy_out, 0, groups(s));
    }
    encoder.copy_buffer_to_buffer(
        &vec_buf,
        out_slot_byte_offset,
        &readback_buf,
        0,
        readback_size,
    );
    queue.submit(std::iter::once(encoder.finish()));

    // ---- readback (solution vector only) -----------------------------------
    let slice = readback_buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    if let Err(why) = await_map(device, &rx) {
        eprintln!("warning: solve_hallen_gpu_resident: {why} — falling back to CPU");
        return Err(GpuSolveDeclined::DeviceFailed);
    }
    let raw = slice.get_mapped_range();
    let floats: &[f32] = bytemuck::cast_slice(&raw);
    // Slot 3 first (its entry R holds the norms), then slot 4.
    let res_sq = floats[2 * rows] as f64;
    let rhs_sq = floats[2 * rows + 1] as f64;
    let out = &floats[2 * stride..];
    let solution: Vec<num_complex::Complex64> = (0..s)
        .map(|i| num_complex::Complex64::new(out[2 * i] as f64, out[2 * i + 1] as f64))
        .collect();
    drop(raw);
    readback_buf.unmap();

    // ---- accuracy gate -----------------------------------------------------
    // An f32 solve that went wrong (a device fault, or a regression of FND-185) is
    // otherwise indistinguishable from a good one
    // once it reaches the report. See GPU_SOLVE_MAX_REL_RESIDUAL for the measured
    // split between the two. Reject the bad ones so the caller takes the f64 CPU
    // path instead of reporting a wrong number.
    let rel = if rhs_sq > 0.0 {
        (res_sq / rhs_sq).sqrt()
    } else {
        0.0
    };
    if !rel.is_finite() || rel > GPU_SOLVE_MAX_REL_RESIDUAL {
        eprintln!(
            "warning: solve_hallen_gpu_resident: f32 solve did not converge \
             (relative residual {rel:.3e} > {GPU_SOLVE_MAX_REL_RESIDUAL:.0e}) — \
             falling back to the f64 CPU solve"
        );
        return Err(GpuSolveDeclined::NotConverged { rel_residual: rel });
    }

    Ok(solution)
}

#[cfg(test)]
mod env_logger_tests {
    use super::EnvLogger;
    use log::LevelFilter;

    #[test]
    fn the_longest_matching_target_wins_and_a_prefix_must_end_at_a_path_boundary() {
        let (l, rejected) =
            EnvLogger::parse("warn, wgpu_core=debug,wgpu_hal=trace,wgpu_hal::vulkan=error");
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(l.level_for("nec_accel"), LevelFilter::Warn);
        assert_eq!(l.level_for("wgpu_core"), LevelFilter::Debug);
        assert_eq!(l.level_for("wgpu_core::device"), LevelFilter::Debug);
        assert_eq!(l.level_for("wgpu_hal::gles"), LevelFilter::Trace);
        assert_eq!(
            l.level_for("wgpu_hal::vulkan::instance"),
            LevelFilter::Error
        );
        // `wgpu_core_x` is not inside `wgpu_core`.
        assert_eq!(l.level_for("wgpu_core_x"), LevelFilter::Warn);
        assert_eq!(l.max_level(), LevelFilter::Trace);
    }

    #[test]
    fn an_unparseable_item_is_reported_not_dropped() {
        let (l, rejected) = EnvLogger::parse("wgpu_hal=loud,info");
        assert_eq!(rejected, vec!["wgpu_hal=loud".to_owned()]);
        assert_eq!(l.level_for("wgpu_hal"), LevelFilter::Info);
    }

    #[test]
    fn with_no_level_given_nothing_is_logged() {
        let (l, _) = EnvLogger::parse("wgpu_hal=debug");
        assert_eq!(l.level_for("nec_accel"), LevelFilter::Off);
    }
}

#[cfg(test)]
mod wait_timeout_tests {
    use super::{wait_readback, ReadbackFault, GPU_WAIT_TIMEOUT};
    use std::sync::mpsc;
    use std::time::Duration;

    /// Run `wait_readback` on a thread and give it `deadline` to return, so a
    /// regression to an unbounded wait fails the test instead of hanging it.
    fn within(
        deadline: Duration,
        f: impl FnOnce() -> Result<(), ReadbackFault> + Send + 'static,
    ) -> Result<(), ReadbackFault> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(deadline)
            .expect("the readback wait must return, not block")
    }

    /// FND-196: a poll that times out ends the wait with the reason, and marks the
    /// device suspect — the path a driver fault that never signals the fence
    /// takes after `GPU_WAIT_TIMEOUT`.
    #[test]
    fn a_timed_out_poll_ends_the_wait_with_the_reason() {
        let fault = within(Duration::from_secs(10), || {
            let (_tx, rx) = mpsc::channel::<Result<(), String>>();
            wait_readback(|_| Err(wgpu::PollError::Timeout), &rx, GPU_WAIT_TIMEOUT)
        })
        .expect_err("a timed-out poll is a fault");
        assert!(
            fault.device_suspect
                && fault.why.contains("did not finish within 120 s")
                && fault.why.contains("FND-196"),
            "{fault:?}"
        );
    }

    /// The device wait carries its bound — `timeout: None` was FND-196.
    /// FND-214: the Vulkan validation layer is loaded only on request. wgpu's
    /// debug-build default turned it on in every test, and the loader crashed
    /// inside it under concurrent instances (2 of 30 suite loops).
    #[test]
    fn the_validation_layer_is_opt_in() {
        let off = super::instance_descriptor(false).flags;
        assert!(
            !off.contains(wgpu::InstanceFlags::VALIDATION)
                && !off.contains(wgpu::InstanceFlags::GPU_BASED_VALIDATION),
            "validation must be off unless requested: {off:?}"
        );
        // Requested, it is whatever wgpu's environment reading gives — which, in
        // a debug build, includes the layer.
        if cfg!(debug_assertions) {
            assert!(super::instance_descriptor(true)
                .flags
                .contains(wgpu::InstanceFlags::VALIDATION));
        }
    }

    #[test]
    fn the_device_wait_is_bounded() {
        let t = Duration::from_secs(7);
        assert!(
            matches!(super::wait_for(t), wgpu::PollType::Wait { timeout: Some(x), .. } if x == t),
            "{:?}",
            super::wait_for(t)
        );
    }

    /// The poll gets the timeout it must honour: the production one.
    #[test]
    fn the_poll_is_given_the_production_timeout() {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        tx.send(Ok(())).unwrap();
        let mut seen = None;
        wait_readback(
            |t| {
                seen = Some(t);
                Ok(())
            },
            &rx,
            GPU_WAIT_TIMEOUT,
        )
        .unwrap();
        assert_eq!(seen, Some(GPU_WAIT_TIMEOUT));
    }

    /// A poll that returns but a map callback that never arrives is bounded too.
    #[test]
    fn a_map_callback_that_never_arrives_is_bounded() {
        let fault = within(Duration::from_secs(10), || {
            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            let r = wait_readback(|_| Ok(()), &rx, Duration::from_millis(10));
            drop(tx);
            r
        })
        .expect_err("a callback that never arrives is a fault");
        assert!(
            fault.device_suspect && fault.why.contains("never completed"),
            "{fault:?}"
        );
    }

    /// A map that fails is reported, but is not the device's fault.
    #[test]
    fn a_failed_map_is_not_blamed_on_the_device() {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        tx.send(Err("out of range".into())).unwrap();
        let fault = wait_readback(|_| Ok(()), &rx, GPU_WAIT_TIMEOUT).unwrap_err();
        assert!(
            !fault.device_suspect && fault.why.contains("out of range"),
            "{fault:?}"
        );
    }
}
