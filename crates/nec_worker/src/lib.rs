// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! Distributed worker protocol, controller, and solve dispatch for fnec-rust.
//!
//! See `docs/distributed-execution-design.md` for the full design spec.

pub mod capability;
pub mod controller;
pub mod hosts;
pub mod protocol;
pub mod result_cache;
pub mod solve;
pub mod ssh_worker;
pub mod worker;

pub use capability::{Capability, CapabilityCache};
pub use controller::LocalWorkerHandle;
pub use hosts::{HostEntry, HostsConfig, HostsConfigError};
pub use protocol::{ErrorCode, Impedance, TaskMessage, TaskResult, WorkerSolverConfig};
pub use result_cache::{cache_key, ResultCache};
pub use ssh_worker::{connect_all, SshWorkerHandle};
pub use worker::run_worker_stdio;

/// Encode a NEC deck string as base64 for inclusion in a [`TaskMessage`].
pub fn encode_deck(deck: &str) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode(deck.as_bytes())
}
