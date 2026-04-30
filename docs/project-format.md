---
project: fnec-rust
doc: docs/project-format.md
status: living
last_updated: 2026-04-30
---

# Project File Format

fnec-rust project files are TOML documents with a fixed structure versioned by
the `version` field.  Version `1` is the only supported version as of
2026-04-30.

Project files are the primary mechanism for storing per-project solver
configuration and named run variants.  The CLI will load a project file when
invoked with a `.fnecproj` path instead of a `.nec` deck path.

## File extension

`.fnecproj`

## Top-level fields

| Field | Type | Required | Description |
|:------|:-----|:--------:|:------------|
| `version` | integer | yes | Format version. Must be `1`. |
| `name` | string | yes | Human-readable project name. |
| `deck_path` | string (path) | yes | Path to the NEC deck file, relative to the project file. |
| `solver` | table | yes | Default solver configuration (see below). |
| `runs` | array of tables | no | Named run variants. Omitted when empty. |

## `[solver]` table

| Field | Type | Required | Accepted values |
|:------|:-----|:--------:|:----------------|
| `mode` | string | yes | `"hallen"`, `"continuity"`, `"sinusoidal"`, `"auto"` |
| `pulse_rhs` | string | yes | `"auto"`, `"1"`, `"1/dl_lambda"` |

The `[solver]` table applies to all runs unless a run declares its own
`[runs.solver]` override.

## `[[runs]]` array

Each element represents a named run variant.

| Field | Type | Required | Description |
|:------|:-----|:--------:|:------------|
| `name` | string | yes | Short unique identifier (e.g. `"baseline"`, `"loaded-50ohm"`). |
| `description` | string | no | Free-form description shown in reports. Omitted when absent. |
| `solver` | table | no | Per-run solver override (same fields as `[solver]`). Inherits project-level config when absent. |

## Complete example

```toml
version = 1
name = "dipole-14mhz"
deck_path = "corpus/dipole-freesp-51seg.nec"

[solver]
mode = "hallen"
pulse_rhs = "auto"

[[runs]]
name = "baseline"
description = "Default Hallen solve at 14 MHz"

[[runs]]
name = "continuity-compare"
description = "Pulse-continuity solve for comparison"

[runs.solver]
mode = "continuity"
pulse_rhs = "1/dl_lambda"
```

## Minimal example (no runs)

```toml
version = 1
name = "quick-check"
deck_path = "examples/dipole_14mhz.nec"

[solver]
mode = "hallen"
pulse_rhs = "auto"
```

## Version compatibility

| Version | Status | Notes |
|:--------|:------:|:------|
| 1 | Current | Introduced in fnec-rust 0.3.0 (PH3-CHK-004) |

Loading a file with an unrecognised `version` value returns a
`ProjectError::UnsupportedVersion` error.  The CLI will print an actionable
message and exit with a non-zero status.

## Implementation

The structs and serialisation logic live in `crates/nec_project/src/lib.rs`.
The public API is:

- `ProjectFile::from_toml(s: &str) -> Result<ProjectFile, ProjectError>` — load from a TOML string.
- `ProjectFile::to_toml(&self) -> Result<String, ProjectError>` — serialise to a TOML string.

Integration tests are in `crates/nec_project/tests/project_roundtrip.rs`.
