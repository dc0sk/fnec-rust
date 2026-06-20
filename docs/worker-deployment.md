---
project: fnec-rust
doc: docs/worker-deployment.md
status: living
last_updated: 2026-06-20
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

# Optional: override the number of CPU threads reported by the worker.
# Useful when the node is shared and you want to cap task assignment.
cpu_threads_override = 8

# Optional: override the GPU capacity weight (default 4.0 when GPU is
# present).  Higher values attract more tasks to this node.
gpu_weight_override = 6.0
```

---

## Worker Capability Cache

The controller caches each node's capabilities (CPU thread count, GPU
availability, wgpu backend name) in memory after the first probe.  The
cache entry survives until:

- the worker process exits abnormally, or
- the controller explicitly invalidates it on reconnect.

A future release will add a configurable TTL (`capability_cache_ttl_secs`
in `hosts.toml`).

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
  the local machine.  Used by the CLI when `--workers` flag is absent.
- **`SshWorkerHandle`** — spawns `ssh <user>@<host> <binary> worker --stdio`
  as a subprocess.  Used by the controller when `hosts.toml` entries exist.

Both implement the same `dispatch(TaskMessage) -> Result<TaskResult, String>`
protocol.

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
| `ConnectTimeout` | `5` | Abort after 5 seconds if host is unreachable |

---

## Solver Limitations

| Feature | Status |
|---|---|
| Hallén basis (`"hallen"`) | ✓ Supported |
| Other basis strings | ✗ Returns `unsupported_config` |
| EX type-0 voltage source | ✓ Supported |
| EX type-1…5 excitations | ✗ Returns `unsupported_config` |
| Ground models | Deck-embedded GE card is used; `ground_model` field reserved |
| Multiple EX cards | First type-0 EX card is used for feedpoint impedance |

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

- The worker subprocess runs with the SSH user's full privileges on the
  worker node.  Use a dedicated service account with minimal filesystem
  permissions.
- Only send deck files you trust.  The worker does not sandbox deck
  execution.
- The `deck_hash` field in `TaskMessage` is informational only in this
  release; signature verification is planned for a future milestone.
