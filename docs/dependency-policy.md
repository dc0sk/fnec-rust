---
project: fnec-rust
doc: docs/dependency-policy.md
status: living
last_updated: 2026-05-02
---

# Dependency Policy

## Purpose

This document defines the dependency license policy for fnec-rust, the exception
request process, and the GPLv2 compatibility rules.  It resolves blocker
**BLK-005** (GPLv2 dependency policy thresholds and exception process documented).

This policy is enforced by `cargo deny` using the configuration in `deny.toml`
at the workspace root.  Check it with:

```bash
cargo deny check licenses
```

---

## License allowlist

The following SPDX identifiers are unconditionally allowed for dependencies:

| SPDX identifier | Notes |
|:----------------|:------|
| `GPL-3.0-only` | All workspace-internal crates |
| `MIT` | Permissive |
| `Apache-2.0` | Permissive; compatible with GPL-3.0 |
| `Apache-2.0 WITH LLVM-exception` | Apache-2.0 with LLVM runtime exception; compatible |
| `BSD-2-Clause` | Permissive |
| `BSD-3-Clause` | Permissive |
| `ISC` | Permissive |
| `CC0-1.0` | Public domain dedication |
| `0BSD` | Permissive (0-clause BSD) |
| `BSL-1.0` | Boost Software License; permissive |
| `Zlib` | Permissive |
| `Unicode-3.0` | Unicode Data License; permissive for data files |
| `Unlicense` | Public domain |
| `LGPL-2.1-or-later` | Allowed only when the crate is also offered under a permissive option (e.g. `MIT OR Apache-2.0 OR LGPL-2.1-or-later`) and we select the permissive option. |

### Deny-list

The following license identifiers are **denied** unless an explicit exception
is granted (see below):

| SPDX identifier | Reason |
|:----------------|:-------|
| `GPL-2.0-only` | Not compatible with GPL-3.0-only; cannot be combined without permission |
| `GPL-2.0-or-later` | Allowed only under the "or later" option exercised as GPL-3.0; requires explicit exception |
| `AGPL-3.0-only` | Viral network-copyleft; incompatible distribution model for a library |
| `AGPL-3.0-or-later` | Same as above |
| `SSPL-1.0` | Not OSI approved; business-source restriction |
| `BUSL-1.1` | Business Source License; not a free/open-source license |
| Proprietary / no license | Never permitted |

---

## GPLv2 compatibility rules

fnec-rust is licensed as **GPL-3.0-only**.  GPLv2 is **not** upward-compatible
with GPLv3 in either direction without explicit "or later" permission from the
copyright holder.

| Scenario | Policy |
|:---------|:-------|
| Dependency is `GPL-2.0-only` | **Denied.** Cannot be linked into GPL-3.0-only code without an exception. |
| Dependency is `GPL-2.0-or-later` | Allowed only if we exercise the "or later" option (i.e. we treat it as GPL-3.0+). Must be documented in the exception table below. |
| Dependency is `LGPL-2.1-or-later` and also `MIT OR Apache-2.0` | Allowed; we select `MIT` or `Apache-2.0`. `cargo deny` must confirm this choice. |
| Dependency is `LGPL-2.1-only` (no `or later`) | **Denied** without explicit architectural review (dynamic linking may make it acceptable; static linking would require a policy exception). |

---

## Exception process

When a desired dependency has a license not in the allowlist, or falls into a
deny-list category, an exception may be granted by:

1. Opening a PR that adds the crate to the `[[licenses.exceptions]]` table in
   `deny.toml`.
2. Adding a comment to the exception entry explaining:
   - Which license option is being used (for multi-license crates).
   - Why it is compatible with GPL-3.0-only in our use case.
   - The review date and who approved it.
3. The PR description must link to the crate's license file on crates.io or
   source repository.
4. A maintainer review is required before merging.

Exceptions are re-reviewed whenever the affected crate's version is bumped.

### Current exceptions

| Crate | Version | Allowed license(s) | Rationale | Review date |
|:------|:--------|:-------------------|:----------|:------------|
| `self_cell` | `*` | `Apache-2.0 OR GPL-2.0-only` | Multi-license; we select Apache-2.0. Transitive dep of iced/winit. | 2026-05-02 |

---

## Duplicate-version policy

`cargo deny` is configured to **warn** (not error) on duplicate transitive
dependency versions.  Duplicates are expected when external crates pin
different versions of the same library; they do not block CI but are tracked
as technical debt.

When a new **direct** workspace dependency is added that introduces a
duplicate, the PR author should attempt to align versions with existing
dependencies before merging.

---

## Source policy

Only crates published to **crates.io** and local `path = "..."` workspace
crates are permitted.  Git dependencies and unknown private registries are
denied (`deny.toml` `[sources]` section).

---

## Tooling

| Tool | Version | Purpose |
|:-----|:--------|:--------|
| `cargo deny` | ≥ 0.14 | License, advisory, ban, and source checks |
| `cargo audit` | latest | CVE advisory check (pre-push hook; see `docs/steering.md`) |

Install cargo-deny:
```bash
cargo install cargo-deny --locked
```

Run all checks:
```bash
cargo deny check
```

Run only license check:
```bash
cargo deny check licenses
```
