---
project: fnec-rust
doc: docs/worker-deployment.md
status: living
last_updated: 2026-09-25
---

# Worker Node Deployment Guide

This guide covers how to install and configure `fnec` worker nodes for the
distributed frequency-sweep feature introduced in PH6-CHK-006.  Workers are
invoked over SSH; the controller spawns `fnec worker --stdio` on each remote
node and exchanges JSON-lines messages over the SSH stdio pipe.

---

## Prerequisites

Each worker node needs:

| Requirement | Minimum |
|---|---|
| CPU architecture | x86-64 or aarch64 |
| Rust toolchain (for build-from-source) | stable ≥ 1.78 |
| Pre-built `fnec` binary | PH6 release |
| SSH daemon | OpenSSH ≥ 8.0 |
| Free RAM | 256 MiB per simultaneous task |

Optional: a Vulkan-capable GPU (for GPU-accelerated Z-matrix assembly via
`nec_accel`).

---

## Installation

### Option A — Binary drop

Copy the pre-built `fnec` binary to the target node and mark it executable:

```sh
scp target/release/fnec worker1.example.com:/usr/local/bin/fnec
ssh worker1.example.com chmod +x /usr/local/bin/fnec
```

### Option B — Build from source on the node

```sh
ssh worker1.example.com
git clone https://github.com/yourusername/fnec-rust.git
cd fnec-rust
cargo build --release -p nec-cli
sudo install -m 755 target/release/fnec /usr/local/bin/fnec
```

---

## SSH Key Setup

The controller authenticates with each worker node over SSH using an
**ed25519** key pair.  Generate one dedicated key for the fnec service
account:

```sh
ssh-keygen -t ed25519 -C "fnec-worker-controller" -f ~/.ssh/fnec_worker_ed25519 -N ""
```

Deploy the public key to each worker node:

```sh
ssh-copy-id -i ~/.ssh/fnec_worker_ed25519.pub dc0sk@worker1.example.com
```

Verify the connection is non-interactive (no passphrase prompt):

```sh
ssh -i ~/.ssh/fnec_worker_ed25519 dc0sk@worker1.example.com fnec --version
```

---

## `hosts.toml` Configuration

The controller reads worker addresses from a TOML file (default path:
`~/.config/fnec/hosts.toml`).

### Minimal example

```toml
[[worker]]
hostname = "worker1.example.com"
ssh_user = "dc0sk"

[[worker]]
hostname = "worker2.example.com"
ssh_user = "dc0sk"
```

### Full field reference

```toml
[[worker]]
# Required: hostname or IP address of the worker node.
hostname = "worker1.example.com"

# Optional: SSH login user.  Defaults to the controller's current user.
ssh_user = "dc0sk"

# Optional: absolute path to fnec on the worker.  Defaults to "fnec"
# (resolved via $PATH on the worker).
binary_path = "/usr/local/bin/fnec"

```

Two further keys, `cpu_threads_override` and `gpu_weight_override`, are
**accepted and ignored**. They were documented here until v0.18.0 as a way to
cap task assignment on a shared node and to make a node attract more tasks, and
nothing ever read them (FND-104). They could not have worked under the scheduler
that shipped, either: each worker runs **one task at a time**, pulled from a
shared queue, so there is no lower concurrency to cap to, and a faster node
already takes more work simply by finishing sooner. Setting either one now
prints a `warning: [hosts]` line naming the host; existing files still parse.

---

## Worker Capability Cache

`nec_worker` provides a capability probe (`SshWorkerHandle::probe_capability`),
a cache for its results (`CapabilityCache`) and a helper that probes a whole
`hosts.toml` (`connect_all`). **The CLI uses none of them.** `fnec --hosts`
connects to each worker without probing and lets the workers pull tasks from a
shared queue, so nothing it does depends on a node's detected CPU or GPU. The
probe is still useful as a diagnostic from Rust code; it is not part of
scheduling.

---

## Distributed GPU execution (PH7-CHK-004)

Running a distributed sweep with `--exec gpu` asks every worker to solve on its
GPU:

```bash
fnec --exec gpu --hosts hosts.toml antenna.nec
```

Each worker honours the request only if it actually has a usable wgpu adapter
**and** the deck is in the GPU-resident supported class (Hallén solver,
free-space ground, no `LD`/`TL` cards). Otherwise it transparently falls back to
the f64 CPU solve — so a heterogeneous pool (some GPU nodes, some CPU-only)
returns correct impedance on every node. The worker reports which path it took
via the `exec_used` field (`"cpu"` | `"gpu"`) in each result. The GPU path is the
f32 GPU-resident solve from PH7-CHK-003; the f64 CPU solve remains the accuracy
reference for tolerance-gated work.

---

## Running the Worker Manually (Debugging)

To test the worker protocol by hand:

```sh
# On the worker node:
fnec worker --stdio
```

The worker reads newline-delimited JSON task messages from stdin and writes
result messages to stdout.  Send a shutdown command to exit cleanly:

```json
{"cmd":"shutdown"}
```

### Example task message

```json
{
  "task_id": "t001",
  "deck_hash": "sha256:aabbcc...",
  "deck_b64": "<base64-encoded NEC deck>",
  "solver_config": {
    "basis": "hallen",
    "ground_model": "none"
  },
  "frequency_hz": 14175000.0
}
```

### Example result message (success)

```json
{
  "status": "ok",
  "task_id": "t001",
  "frequency_hz": 14175000.0,
  "impedance": { "re_ohm": 71.8, "im_ohm": 0.3 },
  "vswr_50": 1.44,
  "feedpoint_current_mag": 0.01394,
  "feedpoint_current_phase_deg": -0.24
}
```

### Example result message (error)

```json
{
  "status": "error",
  "task_id": "t001",
  "frequency_hz": 14175000.0,
  "error_code": "singular_matrix",
  "error_message": "factorization failed: matrix is singular at this frequency"
}
```

---

## Programmatic Usage (Rust API)

The `nec_worker` crate exposes two worker handle types:

- **`LocalWorkerHandle`** — spawns `fnec worker --stdio` as a subprocess on
  the local machine. Not used by the CLI: a run without `--hosts` solves
  in-process, and there is no `--workers` flag.
- **`SshWorkerHandle`** — spawns `ssh <user>@<host> <binary> worker --stdio`
  as a subprocess. This is what `fnec --hosts` uses.

Both implement `dispatch(&TaskMessage) -> Result<TaskResult, DispatchError>`.
`DispatchError` distinguishes a worker that could not be reached (the task is
retried on another worker) from one that answered with an unusable result (the
task is not retried, since the fault is in the task).

### Connecting to a single remote worker

```rust
use nec_worker::{HostEntry, SshWorkerHandle, WorkerSolverConfig, TaskMessage};

let entry = HostEntry {
    hostname: "worker1.example.com".to_string(),
    ssh_user: Some("dc0sk".to_string()),
    binary_path: Some("/usr/local/bin/fnec".to_string()),
    cpu_threads_override: None,
    gpu_weight_override: None,
};

let mut handle = SshWorkerHandle::connect(&entry)?;

let result = handle.dispatch(&TaskMessage {
    task_id: "t001".into(),
    deck_hash: "abc".into(),
    deck_b64: /* base64-encoded NEC deck */,
    solver_config: WorkerSolverConfig {
        basis: "hallen".into(),
        ground_model: "none".into(),
        exec: "cpu".into(), // or "gpu" to request the GPU-resident solve
    },
    frequency_hz: 14.175e6,
})?;

handle.shutdown()?;
```

### Batch discovery with capability probing

```rust
use nec_worker::{HostsConfig, connect_all};

let cfg = HostsConfig::from_file("hosts.toml")?;
let (handles, cache) = connect_all(&cfg);

// `cache` maps hostname -> Capability{cpu_threads, gpu_available, ...}
// Handles that fail to connect or probe are skipped with a warning.
```

### SSH connection options

The `SshWorkerHandle::connect` method passes these options to the `ssh`
binary by default:

| Option | Value | Purpose |
|---|---|---|
| `BatchMode` | `yes` | Disable interactive password prompts |
| `StrictHostKeyChecking` | `no` | **Accept any host key without checking** — see Security Notes |
| `UserKnownHostsFile` | `/dev/null` | **Never record or compare host keys** — see Security Notes |
| `ConnectTimeout` | `5` | Abort after 5 seconds if host is unreachable |

This table used to list only the first and last rows, omitting the two options
that disable host-key verification.

---

## Solver Limitations

| Feature | Status |
|---|---|
| Hallén basis (`"hallen"`) | ✓ Supported |
| Other basis strings | ✗ Returns `unsupported_config` |
| `EX 0` / `EX 5` voltage source | ✓ Supported — both are delta gaps |
| `EX 4` current source | ✓ Supported — priced from the solved port voltage |
| `EX 1` / `2` / `3` plane wave | ✗ Solves, then returns `unsupported_config`: a receive deck has no driven port, and this API reports only a feedpoint impedance |
| No `EX` card at all | ✗ Refused before solving, `unsupported_config` naming the missing card (FND-145) |
| Several delta gaps | Superposed; the **first** one's impedance is reported |
| A plane wave beside a driven source, or two `EX 4` | ✗ Refused before solving |
| Ground | Taken from the deck's `GN` card. A task whose `solver_config.ground_model` is anything but `"none"` is **refused** with `unsupported_config`, rather than having the field silently ignored |

Until v0.18.0 this table said every excitation but `EX 0` was rejected and that
ground came from the `GE` card. `EX 4` and `EX 5` have been supported since
FND-051 and FND-031. The basis, `EX 4`, `EX 5`, plane-wave and no-`EX` rows are
pinned by tests in `crates/nec_worker/src/solve.rs`; the two-current-source and
ground rows by tests in `crates/nec_worker/src/worker.rs`. The superposition and
mixed-excitation rows come from `nec_solver` code the worker shares with every
other frontend (`build_hallen_rhs`, `validate::pre_solve_error`) and are tested
there rather than through the worker.

---

## Troubleshooting

### Worker exits immediately

- Confirm `fnec` is installed at the correct path on the worker node.
- Run `fnec worker --stdio < /dev/null` — it should exit cleanly with code 0.

### SSH connection refused

- Verify the SSH daemon is running on the worker.
- Ensure the public key is in `~/.ssh/authorized_keys` on the worker node.
- Check that the SSH port is open in the node's firewall.

### High latency on first task

The first task on each node incurs extra latency for SSH connection setup.
Subsequent tasks reuse the same subprocess and connection.

### `singular_matrix` errors at isolated frequencies

This can occur near antenna resonance at certain discretisation lengths.
Use a finer segment count (e.g. 101 instead of 51) or avoid the exact
resonant frequency.

---

## Security Notes

- **Host keys are not verified.** Every connection passes
  `StrictHostKeyChecking=no` and `UserKnownHostsFile=/dev/null`, so the
  controller accepts whatever machine answers at a worker's hostname, and
  nothing about that host key is ever stored or compared. Anyone who can
  intercept or redirect that connection can pose as a worker: they receive
  every deck sent to it and can return any impedance they like, and the
  controller reports it as a solved result. Run `--hosts` only on a network
  you control. This is recorded as FND-154, and has not been changed yet
  because tightening it would make new, never-seen hosts fail to connect
  until their keys are known.
- The worker subprocess runs with the SSH user's full privileges on the
  worker node.  Use a dedicated service account with minimal filesystem
  permissions.
- Only send deck files you trust.  The worker does not sandbox deck
  execution.
- The `deck_hash` field in `TaskMessage` is informational only in this
  release; signature verification is planned for a future milestone.
