// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelRequestKind {
    HybridGpuCandidate,
    GpuOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchDecision {
    RunOnGpu,
    FallbackToCpu { reason: &'static str },
}

const ACCEL_STUB_GPU_ENV: &str = "FNEC_ACCEL_STUB_GPU";

fn stub_gpu_enabled() -> bool {
    std::env::var_os(ACCEL_STUB_GPU_ENV)
        .and_then(|v| v.into_string().ok())
        .is_some_and(|v| v == "1")
}

pub fn dispatch_frequency_point(_request: AccelRequestKind, _freq_hz: f64) -> DispatchDecision {
    if stub_gpu_enabled() {
        return DispatchDecision::RunOnGpu;
    }

    DispatchDecision::FallbackToCpu {
        reason: "GPU kernels are not yet wired",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{dispatch_frequency_point, AccelRequestKind, DispatchDecision};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn hybrid_gpu_candidate_dispatch_falls_back_to_cpu_for_now() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        std::env::remove_var("FNEC_ACCEL_STUB_GPU");
        let decision = dispatch_frequency_point(AccelRequestKind::HybridGpuCandidate, 14.2e6);
        assert!(matches!(
            decision,
            DispatchDecision::FallbackToCpu {
                reason: "GPU kernels are not yet wired"
            }
        ));
    }

    #[test]
    fn gpu_only_dispatch_falls_back_to_cpu_for_now() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        std::env::remove_var("FNEC_ACCEL_STUB_GPU");
        let decision = dispatch_frequency_point(AccelRequestKind::GpuOnly, 14.2e6);
        assert!(matches!(
            decision,
            DispatchDecision::FallbackToCpu {
                reason: "GPU kernels are not yet wired"
            }
        ));
    }

    #[test]
    fn stub_gpu_env_enables_run_on_gpu_dispatch() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        std::env::set_var("FNEC_ACCEL_STUB_GPU", "1");

        let hybrid = dispatch_frequency_point(AccelRequestKind::HybridGpuCandidate, 14.2e6);
        let gpu_only = dispatch_frequency_point(AccelRequestKind::GpuOnly, 14.2e6);

        std::env::remove_var("FNEC_ACCEL_STUB_GPU");

        assert!(matches!(hybrid, DispatchDecision::RunOnGpu));
        assert!(matches!(gpu_only, DispatchDecision::RunOnGpu));
    }
}
