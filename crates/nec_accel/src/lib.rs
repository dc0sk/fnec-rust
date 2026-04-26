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

pub fn dispatch_frequency_point(_request: AccelRequestKind, _freq_hz: f64) -> DispatchDecision {
    DispatchDecision::FallbackToCpu {
        reason: "GPU kernels are not yet wired",
    }
}

#[cfg(test)]
mod tests {
    use super::{dispatch_frequency_point, AccelRequestKind, DispatchDecision};

    #[test]
    fn hybrid_gpu_candidate_dispatch_falls_back_to_cpu_for_now() {
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
        let decision = dispatch_frequency_point(AccelRequestKind::GpuOnly, 14.2e6);
        assert!(matches!(
            decision,
            DispatchDecision::FallbackToCpu {
                reason: "GPU kernels are not yet wired"
            }
        ));
    }
}
