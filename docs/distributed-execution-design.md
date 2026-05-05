---
project: fnec-rust
doc: docs/distributed-execution-design.md
status: living
last_updated: 2026-05-05
---

# Distributed Execution Design

## Purpose and Scope

This document is the gating prerequisite for PH6-CHK-006 (SSH-backed worker
deployment) and PH6-CHK-007 (result-cache layer).  No cluster code ships until
all five design sections here carry an explicit, non-TBD decision.

Distributed execution targets large frequency-sweep and parameter-sweep
workloads where a single workstation CPU/GPU is the throughput bottleneck.
The design scope is trusted-LAN nodes under operator control (home lab,
university cluster, shared office network).  Public-internet adversarial
networks are out of scope for Phase 6; they are addressed in a later
hardening milestone.

---

## 1. Transport Protocol

**Decision: SSH, with JSON-framed messages over a persistent stdio channel.**

### Rationale

SSH is the only transport that satisfies all three constraints simultaneously:

1. **Zero-infrastructure requirement** — Every Linux/macOS/Windows node already
   has an SSH daemon.  No message broker (NATS, RabbitMQ, gRPC server) needs to
   be installed or maintained on worker nodes.

2. **Proven auth model** — SSH public-key authentication is mature, audited, and
   understood by the operator community that fnec-rust targets.  Adding a
   separate token infrastructure would duplicate identity management.

3. **Portable-Linux support** — Raspberry Pi 4/5 runs `sshd` from Raspberry Pi
   OS out of the box.  The transport requirement imposes no additional
   dependencies on constrained ARM nodes.

### Channel model

The controller opens one SSH connection per worker node.  Over that connection
it spawns the remote worker binary in a single long-lived subprocess:

```
ssh <worker-host> fnec worker --stdio
```

The worker reads newline-delimited JSON task messages from `stdin` and writes
newline-delimited JSON result messages to `stdout`.  Errors and diagnostics are
written to `stderr` (captured separately by the SSH channel and forwarded to the
controller log).

This design avoids any custom port or daemon; the SSH connection itself is the
only network socket.

### Message framing

Each message is a single UTF-8 JSON object followed by a newline (`\n`).
Messages are never split across lines.  The maximum message size is 4 MiB;
larger payloads (e.g. bulk far-field patterns) are chunked into multiple result
messages tagged with a `chunk_seq` field.

### Reconnect policy

If an SSH connection drops mid-sweep, the controller marks all in-flight tasks
for that node as failed and redistributes them to remaining nodes or to the
local CPU.  Reconnect is attempted once after a 5-second back-off; if it fails,
the node is removed from the active pool for the duration of the run and a
warning is logged.

---

## 2. AuthN / AuthZ Model

**Decision: SSH public-key authentication (ed25519).  No token or password
paths are supported in the worker protocol.**

### Identity model

Each worker node is identified by its hostname/IP and an ed25519 key pair.  The
controller authenticates to the worker using a key from the operator's SSH agent
or from `~/.ssh/id_ed25519` (standard OpenSSH key discovery order).  No
fnec-specific keyring or certificate authority is introduced.

The worker node must have the controller's public key in its
`~/.ssh/authorized_keys` before any distributed run.  This is a one-time
operator setup step documented in `docs/worker-deployment.md` (PH6-CHK-006
artifact).

### Authorisation scope

The worker node authenticates the *operator*, not individual fnec jobs.  Once
the SSH connection is established, any task submitted over that connection is
accepted.  Job-level authorisation (per-deck ACLs) is out of scope for Phase 6.

### Key management constraints

- **Password auth is disabled** in the worker's `sshd_config`
  (`PasswordAuthentication no`).  The deployment guide enforces this.
- **Agent forwarding is not required** — the controller connects outbound to
  workers; workers never need to reach back to the controller.
- **Host key verification is required** — `StrictHostKeyChecking yes` in the
  controller's SSH invocation.  Operators add worker host keys to
  `~/.ssh/known_hosts` during initial setup.  `StrictHostKeyChecking accept-new`
  is permitted during first-time provisioning only.

### Future path

If fnec-rust gains a web-facing coordinator role (out of scope for Phase 6), a
short-lived HMAC token layer can be layered on top of the SSH channel without
changing the worker stdio protocol.

---

## 3. Worker Contract

### 3.1 Input format

Each task message is a JSON object with the following mandatory fields:

```jsonc
{
  "task_id": "<uuid-v4>",          // opaque controller-assigned task identifier
  "deck_hash": "<sha256-hex>",     // SHA-256 of the canonical deck bytes
  "deck_bytes": "<base64>",        // full deck, base64-encoded
  "solver_config": {               // serialised SolverConfig struct
    "basis": "sinusoidal",         // "piecewise_linear" | "sinusoidal"
    "ground_model": "sommerfeld",  // "perfect" | "sommerfeld" | "none"
    "excitation_mode": "voltage_source"
  },
  "frequency_hz": 14.175e6         // single frequency point for this task
}
```

The controller always sends the full deck bytes even on cache-miss paths, so the
worker is stateless between tasks and needs no shared filesystem with the
controller.

### 3.2 Output format

A successful result message:

```jsonc
{
  "task_id": "<uuid-v4>",          // echoed from the input task
  "status": "ok",
  "frequency_hz": 14.175e6,
  "impedance": {
    "re_ohm": 72.3,
    "im_ohm": -1.7
  },
  "vswr_50": 1.46,
  "feedpoint_current_mag": 0.01384,
  "feedpoint_current_phase_deg": -1.34
}
```

A failed result message:

```jsonc
{
  "task_id": "<uuid-v4>",
  "status": "error",
  "frequency_hz": 14.175e6,
  "error_code": "singular_matrix",    // machine-readable code
  "error_message": "Z-matrix is singular at 14.175 MHz — check geometry"
}
```

Defined `error_code` values:

| Code | Meaning |
|:-----|:--------|
| `singular_matrix` | Z-matrix factorisation failed (NaN/Inf or zero pivot) |
| `parse_error` | Deck failed to parse on the worker |
| `unsupported_config` | Solver config requests a feature not compiled in |
| `resource_exhausted` | Worker ran out of memory |
| `internal` | Catch-all for unexpected panics |

### 3.3 Failure semantics

- **Hard failure** (`status: "error"`): The worker emits the error result and
  then continues waiting for the next task.  It does **not** exit.  The
  controller logs the failure, does not cache the result, and optionally retries
  the task once on a different node (configurable; default: retry once).

- **Worker crash** (SSH channel closes unexpectedly): The controller treats all
  in-flight tasks for that node as failed.  They are redistributed.  The node is
  removed from the pool; reconnect is attempted once per §1.

- **Controller abort** (`Ctrl-C` / `SIGTERM` to the controller): The controller
  sends a `{"cmd": "shutdown"}` message to each worker over the SSH channel,
  then waits up to 2 seconds for graceful exit before closing the SSH connection
  forcibly.  Partially completed tasks are not written to the cache.

- **Result ordering**: Results may arrive out-of-order relative to submission
  order.  The controller matches results to tasks via `task_id`.

---

## 4. Work-Split Strategy

**Decision: frequency-point-parallel decomposition.  Each frequency point is
one independent unit of work.  The controller uses a weighted round-robin
assignment based on node capacity.**

### Decomposition unit

A single frequency solve (one value of `frequency_hz`) is the smallest unit of
work because:

1. The MoM Z-matrix depends on frequency; there is no sub-frequency
   parallelism within a single solve.
2. A typical sweep has 10–1000 frequency points; that is enough parallelism
   to fill a small cluster without finer decomposition.
3. The deck is small (≤ a few KiB) and the result is small; per-task
   message overhead is negligible relative to solve time above 50 segments.

Geometry parallelism (splitting the segment matrix across nodes) is
architecturally possible but deferred: it is only beneficial above ~5000
segments and adds complex aggregation logic.  It is tracked as a future
extension (not a Phase 6 target).

### Assignment algorithm

The controller maintains a capacity-weighted task queue:

```
weight(node) = cpu_threads(node) + 4 × gpu_available(node)
```

(The GPU multiplier of 4 reflects expected speedup over a single CPU thread for
the RP far-field kernel; this constant is configurable in `hosts.toml`.)

Task assignment proceeds as a weighted round-robin: the node with the lowest
ratio of `in_flight_tasks / weight` receives the next task.  This is computed
locally on the controller; no distributed coordination is required.

### Chunk size

The controller sends tasks one at a time (chunk size = 1).  A small pipeline
depth of 2 tasks per node is maintained (send the next task as soon as a result
arrives, keeping one task in flight and one pre-loaded).  This avoids head-of-
line blocking on slow nodes without over-committing work.

### Local fallback

If all SSH connections fail before a run completes, the remaining tasks are
executed locally using the standard `rayon`-parallel CPU path.  The distributed
and local paths share the same `dispatch_frequency_point` API; the controller
simply uses the local dispatcher for those tasks.

---

## 5. Result-Cache Design

**Decision: SHA-256-keyed flat-file cache on the controller node.
Cache key = SHA-256(deck_bytes ‖ solver_config_canonical_json ‖ frequency_hz_ieee754_le).**

### Cache location

`$XDG_CACHE_HOME/fnec-rust/result-cache/` (default:
`~/.cache/fnec-rust/result-cache/`).

Each cache entry is a single JSON file named `<sha256hex>.json` containing the
full result message (identical schema to §3.2 output, plus a `cached_at`
timestamp field).

### Cache key construction

```
key = SHA-256(
    deck_bytes                        // raw NEC deck file bytes, no normalisation
  ‖ 0x00                             // separator byte
  ‖ solver_config_canonical_json     // keys sorted, no whitespace, UTF-8
  ‖ 0x00                             // separator byte
  ‖ frequency_hz_le                  // f64 little-endian IEEE 754, 8 bytes
)
```

Deck bytes are used verbatim (no AST normalisation).  Two decks that are
semantically identical but textually different produce different keys.  This is
intentional: normalisation adds complexity and the cache is a performance
optimisation, not a semantic equivalence oracle.

### Cache lookup

Before dispatching a task to a worker, the controller computes the key and
checks for the cache file.  On a hit, the cached result is used immediately; the
task is never sent to a worker.  On a miss, the task is dispatched and the result
is written to the cache after receipt (only for `status: "ok"` results — errors
are not cached).

### Cache invalidation

There is no TTL and no background eviction.  The cache is invalidated
naturally: if the deck changes, the deck bytes change, the key changes, and the
old entry is never referenced again (it becomes orphaned).

Manual eviction:

```
fnec cache clear              # delete all cache entries
fnec cache clear --deck <path>  # delete all entries for a specific deck
fnec cache stats              # print entry count and total size
```

Cache size is bounded by a configurable `max_cache_size_mb` setting in
`~/.config/fnec-rust/config.toml` (default: 512 MiB).  When the limit is
reached, the controller evicts the least-recently-used entries before writing a
new result.  LRU metadata is stored in a `lru.json` index file alongside the
cache entries.

### Integrity check

On read, the controller verifies that the cache file name matches
`SHA-256(file contents minus the cached_at field)`.  A mismatch causes the
entry to be treated as a miss and the corrupt file is deleted.  This guards
against partial writes and filesystem corruption without requiring a separate
checksum database.

### Error-result policy

`status: "error"` results are **not** cached.  If a solve fails at a given
frequency, the next run re-attempts it.  This avoids persisting transient
failures (singular matrices from slightly bad geometry that the user then fixes).

---

## Design Decision Summary

| Concern | Decision |
|:--------|:---------|
| Transport | SSH stdio; newline-delimited JSON messages |
| AuthN | SSH public-key (ed25519); no password/token path |
| AuthZ | Connection-level; per-job ACLs deferred |
| Worker lifecycle | Long-lived subprocess; stateless between tasks |
| Work decomposition | Frequency-point parallel; one point per task |
| Assignment | Capacity-weighted round-robin (CPU threads + 4×GPU) |
| Pipeline depth | 2 tasks per node (one in-flight, one pre-loaded) |
| Local fallback | `rayon` CPU path if all SSH connections fail |
| Cache key | SHA-256(deck bytes ‖ solver config ‖ frequency f64 LE) |
| Cache storage | Flat JSON files under `~/.cache/fnec-rust/result-cache/` |
| Cache invalidation | Key-based (natural on any change); no TTL |
| Cache eviction | LRU up to configurable size limit (default 512 MiB) |
| Error-result caching | Never cached |

---

## See Also

- [docs/architecture.md](architecture.md) — system-level architecture
- [docs/roadmap.md](roadmap.md) — PH6-CHK-005/006/007 checklist entries
- `docs/worker-deployment.md` — per-node setup guide (PH6-CHK-006 artifact, not yet written)
- `crates/nec_accel/src/wgpu_device.rs` — GPU dispatch reference for capacity weight rationale
