---
project: fnec-rust
doc: docs/project-blueprint.md
status: living
last_updated: 2026-04-30
---

# Project Blueprint — Engineering Excellence Guide

A project-agnostic guide for establishing well-structured, documentation-first software projects with sophisticated requirements engineering, phased roadmap planning, tolerance-enforced testing, and disciplined AI-assisted development workflows.

---

## Table of Contents

1. [Principles](#1-principles)
2. [Project Structure](#2-project-structure)
3. [Documentation Architecture](#3-documentation-architecture)
4. [Requirements Engineering](#4-requirements-engineering)
5. [Roadmap Planning](#5-roadmap-planning)
6. [Backlog Management](#6-backlog-management)
7. [Versioning Scheme](#7-versioning-scheme)
8. [Target Definitions & Compatibility Contracts](#8-target-definitions--compatibility-contracts)
9. [Testing Concept](#9-testing-concept)
10. [Git Hooks](#10-git-hooks)
11. [CI/CD Workflows](#11-cicd-workflows)
12. [Release & Packaging](#12-release--packaging)
13. [Development Workflow](#13-development-workflow)
14. [Contract Rules for AI Agents](#14-contract-rules-for-ai-agents)
15. [Efficient Prompting Guide for Users](#15-efficient-prompting-guide-for-users)

---

## 1. Principles

These principles are invariants. Every section of this guide flows from them.

**Tolerance-first quality.** Every behavioral contract has a measurable acceptance criterion. "Works" is not a criterion. Define what works means, express it as a bound, and make a build fail when that bound is crossed.

**Documentation as a first-class artifact.** Requirements, architecture, and roadmap documents live in the repository alongside code. They are reviewed, versioned, and held to the same quality bar as code. Undocumented decisions accumulate as technical debt.

**Explicit scope boundaries.** Every project has a primary target, explicit deferred items, and explicitly out-of-scope items. Ambiguity creates scope creep. Scope decisions are numbered, recorded, and referenced.

**CLI is the reference implementation.** For projects with multiple frontends, the command-line interface defines reference behavior. GUIs, APIs, and libraries conform to CLI behavior, not the other way around.

**Traceability is mandatory.** Every test must trace to a requirement. Every requirement must trace to a test or have an explicit open gap. Gap items that are neither tested nor deferred are requirements debt.

**No tolerance creep.** When a behavioral contract is relaxed, that change is treated as a breaking change, documented explicitly, and reviewed deliberately. Tolerance loosening is never a silent fix for a failing test.

---

## 2. Project Structure

### 2.1 Recommended Directory Layout

```
project-root/
├── crates/ or lib/ or src/    # Core library modules (language-specific)
├── apps/ or cmd/ or bin/      # User-facing executables / frontends
├── docs/                      # All project documentation (see §3)
├── corpus/ or testdata/       # Golden reference fixtures and expected outputs
├── fuzz/                      # Fuzzing harnesses and seed corpora
├── scripts/                   # Automation scripts (validation, stamping, benchmarks)
├── .github/                   # CI workflows, issue/PR templates
├── .githooks/                 # Project-local git hooks (pre-commit, pre-push)
├── Makefile                   # At minimum: install-hooks target
├── CHANGELOG.md or docs/changelog.md
├── README.md                  # Project overview and quickstart
├── COPYING or LICENSE         # License text
└── SBOM.spdx.json             # Software Bill of Materials (generated, not edited)
```

### 2.2 Core Separation Rules

- **Libraries and frontends are always separate.** No solver, parser, or business logic belongs in a CLI main.rs or GUI handler. Frontend code is wiring only.
- **No cross-contamination between dialects or modes.** If a project supports multiple input formats or operational modes, their handling is isolated. One mode's quirks do not bleed into another's code path.
- **Golden fixtures are version-controlled.** Test inputs and reference outputs live in the repository, not in external storage.
- **Secrets are never committed.** Environment-specific configuration uses gitignored env files with a tracked `.example` counterpart.

---

## 3. Documentation Architecture

### 3.1 Document Frontmatter Standard

Every document in `docs/` carries YAML frontmatter:

```yaml
---
project: <project-name>
doc: docs/<filename>.md
status: living | completed | deprecated
last_updated: YYYY-MM-DD
---
```

- `status: living` — actively maintained; expected to change.
- `status: completed` — frozen; changes require explicit justification.
- `status: deprecated` — superseded; kept for historical reference.
- `last_updated` is stamped automatically by CI on PRs that modify the file.

### 3.2 Required Documents

| Document | Purpose | Status |
|:---------|:--------|:-------|
| `docs/requirements.md` | Scope decisions, functional/non-functional/compatibility requirements, gap list, versioned contracts | living |
| `docs/architecture.md` | System design, module boundaries, pipeline description, compatibility constraints | living |
| `docs/roadmap.md` | Phased delivery plan with explicit blockers, parity gaps, and projected timelines | living |
| `docs/backlog.md` | Open follow-ups, implementation checklists, completed items | living |
| `docs/changelog.md` | All notable changes per release cycle | living |
| `docs/releasenotes.md` | User-facing release summaries (shorter, external-audience changelog) | living |

Optional but recommended:

| Document | Purpose |
|:---------|:--------|
| `docs/steering.md` | Project ownership, decision accountability |
| `docs/<dialect>-support.md` | Explicit feature boundary for a secondary spec, dialect, or standard |
| `docs/corpus-guide.md` | How to add, validate, and interpret golden corpus fixtures |

### 3.3 Documentation Governance Rules

1. All documentation changes flow through PRs. No direct pushes to main.
2. PRs that introduce or modify a `docs/` file trigger frontmatter validation.
3. PRs that bump the project version must update `docs/changelog.md` and `docs/releasenotes.md`. This is CI-enforced.
4. Frontmatter `last_updated` is stamped automatically; do not set it manually in PRs.
5. Documents may not reference items (requirements IDs, gap IDs) that do not exist in `docs/requirements.md`.

### 3.4 CI Validation of Documentation

- **Frontmatter validation**: confirm all required fields are present, status is a valid enum value, date is parseable.
- **Version-bump check**: if `version` in project manifest changed, verify changelog and releasenotes were also modified.
- **Last-updated stamping**: auto-stamp on PR for changed docs (does not block merge; informational).

---

## 4. Requirements Engineering

### 4.1 Structure of `docs/requirements.md`

Organize requirements into four named groups. Each item carries a unique identifier that never changes (items are deprecated, not renumbered).

**Scope Decisions (DEC-NNN)** — binding choices about what the project is and is not. Format:

```
**DEC-001**: <Decision statement.>
Rationale: <Why this choice was made.>
```

**Functional Requirements (FR-NNN)** — what the system does. Format:

```
**FR-001**: <The system shall …>
Acceptance: <Measurable criterion.>
Phase: <Which roadmap phase delivers this.>
```

**Non-Functional Requirements (NFR-NNN)** — quality attributes (performance, reliability, usability). Format:

```
**NFR-001**: <The system shall …>
Metric: <How it is measured.>
Threshold: <Pass/fail boundary.>
```

**Compatibility Requirements (COMP-NNN)** — behavioral parity targets against named external baselines. Format:

```
**COMP-001**: <Compatibility statement against <Baseline>.>
Tolerance: <Numeric or behavioral bound.>
Reference: <Authoritative source.>
```

### 4.2 Gap List

Gaps are items that are in scope but not yet specified, tested, or implemented. Every open gap has an owner and a target phase.

```
**GAP-NNN**: <Description of what is unresolved.>
Status: open | resolved | deferred
Target phase: <Phase number or "N/A">
Owner: <Name or "unassigned">
Resolution: <If resolved, what was decided and when.>
```

Gaps are resolved when:
- The requirement is written and tested (close with resolution note).
- The item is explicitly deferred (update status to `deferred`, note the target phase).
- The item is explicitly descoped (update status to `resolved`, note the decision).

### 4.3 Versioned Contracts

Some requirements define machine-verifiable output contracts (e.g., a text report format, an API schema, a wire protocol). These are numbered separately and frozen on release.

```
**PAR-NNN vN**: <Contract name>
Version: N
Status: locked | draft
Fields: <Structured description of the contract surface.>
Breaking change policy: <What changes are breaking vs additive.>
```

Once a contract version is locked, changing it requires:
1. A new version number (PAR-NNN v2).
2. An explicit changelog entry labeled "Breaking".
3. A CI test that covers the new version's surface.

### 4.4 Traceability Matrix

Maintain a cross-reference between requirements and their tests. The canonical form is a table in `docs/requirements.md` or a separate `docs/traceability.md`:

| Requirement | Test file(s) | Status |
|:-----------|:------------|:-------|
| FR-001 | tests/core_flags_contract.rs | covered |
| NFR-001 | tests/corpus_validation.rs | covered |
| COMP-001 | tests/corpus_validation.rs | partial — see GAP-005 |
| FR-007 | — | gap (GAP-009) |

Every covered row must have a test that would fail if the requirement were violated. Every uncovered row must have an open gap entry.

---

## 5. Roadmap Planning

### 5.1 Phase Structure

Organize work into numbered phases. Each phase has:
- A theme (one-line description of what this phase delivers).
- A projected quarter (QN YYYY).
- A set of concrete deliverables (bulleted, each linked to a FR/NFR/COMP ID where applicable).
- A blocker gate that must pass before the phase is considered complete.
- An implementation checklist (PH-N-CHK-NNN items) that maps to validation artifacts.

```markdown
## Phase N — <Theme> (QN YYYY)

**Goal**: <One-sentence outcome statement.>

**Blocker gate**: BLK-00N (see §5.3).

### Deliverables
- [ ] PH-N-CHK-001: <item> → <validation artifact>
- [ ] PH-N-CHK-002: <item> → <validation artifact>

### Parity targets reached
- PRT-NNN: <baseline comparison> (see §8.2)
```

### 5.2 Phase Sequencing Rules

- Phases are strictly ordered. Phase N does not begin until Phase N-1 is complete (its blocker gate passes).
- Deliverables may move forward (to a later phase) but not backward. Moving forward requires a documented reason.
- No deliverable is "done" without a validation artifact (a test, a CI gate, or a documented parity measurement).

### 5.3 Blocker Gates

Define explicit gates between phases. A gate is a condition, not a date.

```
**BLK-001**: <Condition statement.>
Required by: Phase N → N+1
Status: open | passed
Passed: YYYY-MM-DD
```

Examples of strong blocker gates:
- All Phase N CI gates green on main for 72 hours.
- Golden corpus tolerance pass at 100% with no exceptions.
- Versioned contract (PAR-NNN) locked and CI-gated.
- Architecture document updated to reflect Phase N final state.

### 5.4 Parity Gap Tracking

For projects with external baselines (other implementations, published standards, competitor products), maintain a parity gap table:

| ID | Baseline | Gap description | Impact | Target phase | Status |
|:---|:---------|:---------------|:-------|:------------|:-------|
| PRT-001 | <Baseline A> | <What differs> | <H/M/L> | Phase N | open |
| PRT-002 | <Baseline B> | <What differs> | <H/M/L> | Phase N+1 | open |

Parity gaps are separate from requirements gaps. A parity gap says "we know we diverge from X on Y"; a requirements gap says "we don't yet know how to specify Z."

### 5.5 Projected Timelines

Express timelines as quarters (Q1 2027), not as specific dates, unless a hard external deadline exists. Rationale: quarterly granularity is honest about estimation uncertainty; specific dates invite false precision and then become outdated footnotes.

Review and update projected phases at each phase completion milestone, not continuously.

---

## 6. Backlog Management

### 6.1 Structure of `docs/backlog.md`

The backlog is a flat, running list of work items. It is not a project management tool; it is a quick-reference for what is open and what has been completed.

```markdown
## Open

- [ ] **ITEM-NNN**: <Short description.>
  Details: <Any context needed.>
  Blocked by: <BLK-NNN or dependency, if any.>
  Target phase: <Phase number.>

## Completed

- [x] **ITEM-NNN**: <Short description.> *(completed YYYY-MM-DD)*
```

### 6.2 Backlog Rules

1. Every open backlog item that is in-scope must trace to a requirement or be explicitly labeled "infrastructure" or "tech debt".
2. Items are never deleted from the backlog. Completed items move to the "Completed" section with a date. Cancelled items get a "Cancelled" label and a reason.
3. Backlog items do not carry due dates. Delivery order is determined by the roadmap phase.
4. The backlog is not the source of truth for scope — `docs/requirements.md` is. The backlog operationalizes requirements into discrete work items.

---

## 7. Versioning Scheme

### 7.1 Semantic Versioning

Use `MAJOR.MINOR.PATCH`:

| Segment | Increment when |
|:--------|:--------------|
| `MAJOR` | A breaking change in a public API, a behavioral contract, or a tolerance matrix. |
| `MINOR` | A new capability that does not break existing consumers. |
| `PATCH` | A bug fix, documentation update, or internal refactor with no observable behavior change. |

### 7.2 Pre-release Labels

- `0.N.0` — development series; breaking changes allowed between minors.
- `1.0.0` — first stable release; full semver guarantees apply from this point.
- `1.0.0-rc.1` — release candidate; no new features, only fixes.
- `1.0.0-alpha.1` — early preview; API instability expected.

### 7.3 What Counts as Breaking

The following changes always increment `MAJOR` (or `MINOR` in the 0.x series):
- Removing or renaming a public API surface.
- Changing the output format of a versioned contract (e.g., PAR-NNN).
- Relaxing or tightening a tolerance matrix entry.
- Dropping support for a target platform or minimum language version.
- Changing default behavior in a way that alters observable output.

### 7.4 Version Bump Checklist

When incrementing the version:

1. Update the version field in the project manifest (`Cargo.toml`, `pyproject.toml`, `package.json`, etc.).
2. Regenerate the SBOM: `cargo sbom --output-format spdx-json > SBOM.spdx.json` (or equivalent).
3. Update `docs/changelog.md`: add an entry for the new version with all changes since the last release.
4. Update `docs/releasenotes.md`: add a user-facing summary of what changed.
5. Update any in-doc version references.
6. Open a PR. CI enforces that steps 3–4 were completed.
7. After merge, tag the commit: `git tag vMAJOR.MINOR.PATCH`.

### 7.5 Minimum Supported Language/Runtime Version (MSLV)

Define and document the minimum version of the language runtime, compiler, or interpreter required to build and use the project. Dropping this version is a breaking change.

---

## 8. Target Definitions & Compatibility Contracts

### 8.1 Target Tiers

Define support tiers explicitly. At minimum:

| Tier | Definition | CI gated? |
|:-----|:----------|:----------|
| Primary | Fully supported; regressions are blockers. | Yes |
| Secondary | Best-effort support; regressions are tracked but not release blockers. | Desirable |
| Experimental | Known to sometimes work; no guarantees. | No |

Assign every platform, hardware target, or dialect to a tier.

### 8.2 Numeric Tolerance Contracts (for scientific / engineering projects)

If the project produces numerical results, the tolerance matrix is a first-class document section in `docs/requirements.md`. Every metric has a dual bound (relative and absolute); use whichever is wider:

| Metric | Relative tolerance | Absolute tolerance |
|:-------|:------------------|:------------------|
| <Metric A> | ≤N% | ≤N <unit> |
| <Metric B> | ≤N% | ≤N <unit> |

Rules:
- Tolerance changes are breaking changes (see §7.3).
- Tolerance bounds are enforced by CI, not checked manually.
- External reference sources are named in the matrix (not "expected" or "approximate").
- Deferred tolerance entries are labeled "not yet gated" and tracked as gap items.

### 8.3 Behavioral Compatibility Baselines

For each external baseline against which parity is claimed:
- Name it explicitly (tool name, version, fork URL).
- Define what "parity" means (identical output, within tolerance, behavioral equivalence).
- Define the authoritative source for reference outputs (which version of which tool generated the golden values).
- Track divergences as parity gaps (§5.4).

---

## 9. Testing Concept

### 9.1 Test Pyramid

```
         ┌─────────────────────┐
         │  Contract / Golden  │  ← highest confidence, slowest
         │   corpus tests      │
         ├─────────────────────┤
         │  Integration tests  │  ← per-subsystem behavior
         ├─────────────────────┤
         │   Unit tests        │  ← per-function correctness
         └─────────────────────┘
```

All three layers are mandatory. A project that only has unit tests is not tested.

### 9.2 Golden Corpus Approach

A **golden corpus** is a version-controlled collection of input fixtures paired with reference outputs. The corpus validates that the system produces results within tolerance of established external references.

**Corpus directory structure:**

```
corpus/
├── <case-name>.input            # Input fixture (e.g., a NEC deck, a config file)
├── <case-name>.expected         # Expected output (golden reference)
├── <case-name>.notes            # Human-readable notes on the case and reference source
└── reference-results.json       # Machine-readable reference values with tolerance bounds
```

**reference-results.json structure (per case):**

```json
{
  "case-name": {
    "source": "External tool v1.2.3 (commit abc123)",
    "metrics": {
      "metric_a": { "expected": 1.234, "rel_tol": 0.001, "abs_tol": 0.05 },
      "metric_b": { "expected": -0.567, "rel_tol": 0.001, "abs_tol": 0.001 }
    },
    "status": "active | deferred | experimental"
  }
}
```

**Corpus rules:**
1. Every corpus case has a named external reference source with version information.
2. Cases with `status: deferred` are excluded from CI gates but remain in the corpus.
3. Cases with `status: experimental` run in CI but failures are warnings, not blockers.
4. Cases with `status: active` are hard CI failures on tolerance breach.
5. Adding a new corpus case requires a reference source, a tolerance entry, and a status assignment. It may not be committed without all three.
6. The corpus is never edited to match implementation. The implementation is fixed to match the corpus.

### 9.3 Contract-Based Testing

Beyond numeric parity, define behavioral contracts for the interfaces the system exposes. Each contract has a corresponding test file:

| Contract | Tests enforce |
|:---------|:------------|
| CLI flags | Every flag's type, default, and error behavior. Exit codes for all error paths. |
| Output format | Exact structure of machine-readable output (headers, field order, precision). |
| Scriptability | stdout/stderr separation. Machine-parseable headers. Stable field names across versions. |
| Error messages | Human-readable error text contains required fields (location, cause, suggestion). |
| Geometry/input validation | Invalid input produces a diagnostic, not a panic or silent wrong answer. |
| Execution mode routing | Each named mode dispatches to the correct code path and emits mode identification. |

A contract test is distinct from a unit test: it tests the interface boundary as a consumer would observe it, not the internal implementation.

### 9.4 Property-Based and Invariant Testing

For mathematical or algorithmic code, add invariant tests alongside example-based tests:
- **Commutativity / symmetry**: if the algorithm is symmetric, verify `f(a, b) == f(b, a)`.
- **Reciprocity**: if the physics implies reciprocity, verify `output(a→b) == output(b→a)`.
- **Monotonicity**: if a result should monotonically increase with a parameter, verify it does.
- **Idempotency**: applying the same operation twice produces the same result as once.

These catch entire classes of implementation bugs that example tests miss.

### 9.5 Fuzz Testing

For any component that accepts external input (parsers, decoders, format readers):
- Maintain a fuzz harness (using the language's native fuzz tooling).
- Maintain a seed corpus of representative inputs, including edge cases and malformed inputs.
- The fuzz harness must not panic on any input. Errors are acceptable; panics are not.
- Fuzz results that find crashes are added to the seed corpus after the crash is fixed.

### 9.6 CI Enforcement Rules

- Golden corpus failures are hard build failures, not warnings.
- Contract test failures are hard build failures.
- Unit test failures are hard build failures.
- Fuzz results are not CI-gated by default but are run in a nightly or scheduled pipeline.
- No test is silenced with `#[ignore]`, `skip`, or equivalent without an open gap entry in `docs/backlog.md`.

---

## 10. Git Hooks

### 10.1 Hook Location and Installation

Store hooks in a tracked directory (`.githooks/`), not in `.git/hooks/` (which is gitignored). Provide an installation target:

```makefile
install-hooks:
	chmod +x .githooks/pre-commit .githooks/pre-push scripts/*.sh
	git config core.hooksPath .githooks
```

Every contributor runs `make install-hooks` after cloning. Document this in `README.md`.

### 10.2 Pre-Commit Hook

The pre-commit hook is the last line of defense before a commit is recorded. Keep it fast (under 30 seconds).

```bash
#!/usr/bin/env bash
set -euo pipefail

# Format check
<language formatter> --check

# Fast test pass (unit tests only, or workspace tests if fast)
<test runner> --workspace
```

For large test suites: run only the fast subset (unit + contract tests) in pre-commit, and leave the corpus/integration pass for CI.

### 10.3 Pre-Push Hook

The pre-push hook runs before pushing to a remote. Use it for slower checks that would fail CI anyway.

```bash
#!/usr/bin/env bash
set -euo pipefail

# Security audit (check for known vulnerable dependencies)
<audit tool>
```

### 10.4 Hook Rules

- Hooks must not have side effects (they must not push, create files, or send messages).
- A hook failure must produce a human-readable message explaining what failed and how to fix it.
- Never bypass hooks with `--no-verify` unless a documented emergency procedure explicitly permits it.
- If a hook is unreliable (flaky network, environment-specific), fix the hook; do not bypass it.

---

## 11. CI/CD Workflows

### 11.1 Mandatory Gates

These CI workflows gate every PR to main:

| Workflow | What it checks | Fail behavior |
|:---------|:--------------|:-------------|
| `corpus-validation` | Golden corpus tolerance pass | Hard block |
| `format-and-lint` | Code style, lint warnings | Hard block |
| `unit-tests` | All unit and integration tests | Hard block |
| `docs-validate` | Doc frontmatter, version-bump docs update | Hard block |
| `report-format` | Versioned output contract tests | Hard block |

### 11.2 Optional / Advisory Gates

| Workflow | When to use | Fail behavior |
|:---------|:-----------|:-------------|
| `benchmark-compare` | When benchmark CSVs are attached to PR | Advisory (configurable threshold) |
| `fuzz-regression` | On a nightly schedule | Advisory |
| `security-audit` | On push to main or on dependency updates | Hard block on push to main |

### 11.3 Workflow Principles

- Main branch is protected. No direct pushes.
- All gate workflows run on PR and on push to main.
- Advisory workflows produce a summary comment on the PR; they do not block.
- Workflow files are reviewed in PRs like code; do not modify CI configuration without review.
- Every workflow step that can fail produces an artifact (log file, report CSV, diff) that is uploaded for postmortem.

---

## 12. Release & Packaging

### 12.1 Release Checklist

Before tagging a release:

- [ ] All Phase N blocker gates (BLK-00N) are passed.
- [ ] All CI gates are green on main.
- [ ] `docs/changelog.md` has an entry for this version.
- [ ] `docs/releasenotes.md` has a user-facing summary for this version.
- [ ] SBOM is regenerated (`<sbom-tool> > SBOM.spdx.json`).
- [ ] Version is updated in project manifest(s).
- [ ] All version references in docs are updated.
- [ ] No open gap items are labeled as release blockers.

### 12.2 SBOM (Software Bill of Materials)

Generate an SBOM in SPDX or CycloneDX format on every release. The SBOM:
- Is committed to the repository root.
- Is regenerated (not hand-edited) on every version bump.
- Is attached as a release artifact alongside binaries.
- Is not reviewed in PRs (it is a generated output); the generation command is reviewed instead.

### 12.3 Tagging and Distribution

```bash
# Tag after merge to main
git tag -a vMAJOR.MINOR.PATCH -m "Release vMAJOR.MINOR.PATCH"
git push origin vMAJOR.MINOR.PATCH
```

Tagging a release triggers:
- Binary builds for all Tier 1 target platforms.
- SBOM attachment to the release.
- Release notes published from `docs/releasenotes.md`.

### 12.4 Changelog Format

`docs/changelog.md` uses the [Keep a Changelog](https://keepachangelog.com/) structure:

```markdown
## [Unreleased]

### Added
- <item>

### Changed
- <item>

### Removed
- <item>

## [1.2.0] — 2026-04-30

### Added
- ...
```

- Every notable change gets an entry. "Notable" means user-observable.
- Internal refactors that do not change observable behavior are not changelog entries.
- Breaking changes are prefixed `**Breaking**:`.
- Entries reference requirement IDs where applicable: `(FR-003)`.

---

## 13. Development Workflow

### 13.1 Branch Model

- `main` — always releasable. CI green. Protected.
- `feat/<short-description>` — feature branches. Branch from main, merge to main via PR.
- `fix/<short-description>` — bug fix branches.
- `docs/<short-description>` — documentation-only branches.
- `release/vN.M.P` — release preparation branches (optional; for projects with complex release coordination).

No long-lived feature branches. If a feature takes more than one sprint, break it into incremental PRs that keep main releasable.

### 13.2 PR Process

1. Open a PR with a description that includes:
   - What changed and why (the motivation, not just the diff).
   - The requirement IDs addressed (FR-NNN, NFR-NNN, COMP-NNN).
   - Test coverage added or updated.
   - Any gap items closed or opened.
2. All CI gates must pass before merge.
3. PRs that change `docs/requirements.md`, `docs/roadmap.md`, or `docs/architecture.md` require explicit review of those changes, not just the associated code.
4. PRs are squash-merged to main to keep history readable. Each merge commit message summarizes the PR.

### 13.3 Commit Message Convention

```
<type>(<scope>): <imperative subject, 50 chars max>

<body: optional, wrapped at 72 chars>
<explain why, not what>

Refs: FR-NNN, GAP-NNN
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`.

The subject line must complete the sentence "If applied, this commit will …".

### 13.4 Documentation Review in PRs

- Every PR that adds a new feature must include documentation updates (at minimum, a changelog entry and, if the feature is user-facing, a `docs/` update).
- PRs that only touch `docs/` are valid and encouraged for keeping documentation current.
- Reviewers check documentation consistency, not just code correctness.

---

## 14. Contract Rules for AI Agents

This chapter defines the contract that governs how an AI coding agent (Claude Code, Copilot, Cursor, or any LLM-backed assistant) may operate within this project. These rules protect the project's quality invariants and the contributor's intent.

### 14.1 Scope Rules

**An AI agent operates within the scope of the task given.** It does not:
- Refactor code outside the requested change.
- Add features that were not requested.
- Remove error handling, validation, or tests unless explicitly asked.
- Introduce abstractions "for future extensibility" beyond what the task requires.
- Change the tolerance matrix, versioned contracts, or documented scope decisions without explicit instruction.

If the agent observes code that could be improved but is outside the task scope, it reports the observation in text but does not act on it.

### 14.2 Test Obligations

Every code change an AI agent makes must include tests at the appropriate level (see §9):
- New behavior → new test.
- Modified behavior → updated test.
- Bug fix → regression test that would have caught the bug.
- Refactor with no behavior change → existing tests must still pass; no new tests required, but the agent must verify they do.

An AI agent must never silence a failing test. It must fix the underlying issue.

### 14.3 Documentation Obligations

- If a change addresses a requirement, the agent includes the requirement ID in the commit message.
- If a change resolves a gap item, the agent updates `docs/backlog.md` to mark the item complete.
- If a change closes or partially closes a requirement, the agent updates the traceability matrix.
- An AI agent does not modify `docs/requirements.md`, `docs/roadmap.md`, or `docs/architecture.md` without explicit instruction. These are design-authority documents.

### 14.4 Quality Invariants

An AI agent must not:
- Introduce security vulnerabilities (injection, XSS, path traversal, hardcoded secrets).
- Use `unsafe` (or equivalent language-specific bypass mechanisms) without explicit instruction and justification.
- Add dependencies without noting them in the PR description.
- Bypass git hooks with `--no-verify` or equivalent.
- Edit `SBOM.spdx.json` directly; it is always generated.
- Use `TODO`, `FIXME`, or `HACK` comments as a substitute for fixing the issue or opening a gap entry.

### 14.5 Communication Obligations

The agent must communicate clearly when it:
- Cannot complete a task within the stated scope without changing something outside the scope.
- Finds an ambiguity in the requirement that would affect the implementation.
- Observes that the requested change would relax a tolerance or break a versioned contract.
- Cannot find a test to cover a behavior it is implementing (must say so explicitly, not skip silently).

The agent must not claim a task complete when it has made assumptions that could be wrong. It states the assumption and asks for confirmation.

### 14.6 Destructive Action Protocol

Before executing any of the following, the agent pauses and explicitly states what it is about to do and why, then waits for confirmation:
- Deleting files or directories.
- Force-pushing or resetting git history.
- Modifying CI workflow files.
- Changing the tolerance matrix.
- Relaxing a versioned contract.
- Removing a test.
- Changing a dependency version (especially downgrading).

These actions are not blocked by default; they require explicit acknowledgment.

### 14.7 Commit Discipline

An AI agent creates commits that:
- Are atomic (one logical change per commit).
- Have a conventional commit message (see §13.3).
- Do not bundle unrelated changes ("while I was here" changes go in a separate commit or are deferred).
- Do not stage secrets, environment files, or generated SBOM files unless explicitly asked.

An AI agent does not push to remote, create PRs, or open issues unless explicitly instructed.

---

## 15. Efficient Prompting Guide for Users

This chapter describes how to collaborate effectively with an AI coding agent on this project.

### 15.1 The Single Most Important Rule

**Give the agent the answer to "why" before it asks.** The agent can find "what" by reading the code. It cannot find "why" without context. A prompt that explains the motivation produces far better output than a prompt that only describes the desired change.

Bad: `Extract the solve session into its own module.`

Good: `Extract the solve session logic from main.rs into solve_session.rs so that main.rs is reduced to frontend wiring only. This is part of the CLI orchestration extraction work (backlog item ITEM-023). The goal is testability — solve_session.rs should be unit-testable without a full CLI invocation.`

### 15.2 Frame the Scope Explicitly

The agent defaults to a conservative interpretation of scope. If you want it to do more, say so. If you want it to do less, say so.

- "Only change X. Do not touch Y even if you see improvements."
- "This change is allowed to touch both A and B modules."
- "Fix only the bug. Do not refactor surrounding code."
- "Refactor freely within this file, but do not change the public API."

Without scope framing, the agent will do the minimal safe thing, which is usually correct but sometimes too narrow.

### 15.3 Reference the Project's Own Documents

This project has requirements, a roadmap, a backlog, and a traceability matrix. Use them. Referencing them in prompts:
- Constrains the agent's scope to what was actually agreed upon.
- Eliminates the need to re-explain context that is already written down.
- Produces output that correctly references requirement IDs and gap items.

Good pattern: `Implement FR-005 (geometry diagnostics). The acceptance criterion is in docs/requirements.md. Add tests to tests/geometry_diagnostics.rs. Close GAP-006 in the backlog when done.`

### 15.4 Task Sizing

**One task, one prompt.** If you have three things to do, write three prompts and send them sequentially. This:
- Gives you a review point between each change.
- Keeps the agent's context focused.
- Produces atomic, reviewable commits.

If you must bundle work, use a numbered list and say so explicitly: "Do these three things in order. Stop after each and summarize before continuing."

### 15.5 Provide Examples for Output Contracts

If the task produces machine-readable output, provide an example of the expected format. The agent cannot guess a format contract from description alone.

Include:
- A sample input.
- The expected output (not "something like" — the exact output).
- Any fields whose order or precision is significant.

### 15.6 Specify the Tolerance for Imprecision

When asking the agent to approximate, estimate, or explore (rather than implement precisely), say so. Otherwise it will treat everything as a hard requirement.

- "Give me a rough structure, I'll refine it."
- "This is exploratory — don't worry about edge cases yet."
- "This must be production-ready, including all error paths."

The agent does not distinguish between draft and production intent unless told. Default behavior is closer to draft (minimal, fast); "production-ready" is not assumed.

### 15.7 Closing the Loop: Review and Verification

The agent produces output; you verify it. The agent's summary describes what it intended; the diff shows what it actually did. Always:
1. Read the diff, not just the summary.
2. Run the tests yourself after a significant change.
3. Check that the requirement is actually met, not just plausibly addressed.
4. Confirm that no tolerance or contract was silently changed.

If the agent's output is wrong, say what is wrong and why. "This doesn't work" gives the agent no signal. "The output precision is wrong — it should match the PAR-001 contract (6 decimal places) but your implementation uses 3" gives the agent a precise correction target.

### 15.8 Prompts for Common Task Types

**Adding a new corpus case:**
> Add corpus case `<name>` for `<input description>`. The reference result is `<metric: value>` from `<source>`. Tolerance: `<rel_tol>` relative, `<abs_tol>` absolute. Status: active. Add it to `corpus/reference-results.json` and add a test assertion in `tests/corpus_validation.rs`.

**Closing a gap item:**
> Implement `GAP-NNN` (`<gap description>`). The acceptance criterion is in `docs/requirements.md`. Write the tests first (TDD). When done, mark the gap resolved in `docs/requirements.md` and mark the backlog item complete in `docs/backlog.md`.

**Extracting a module:**
> Extract `<logic>` from `<source file>` into `<target file>`. The extraction must preserve existing test coverage — run the tests and confirm they still pass. Do not change any behavior. Goal: `<motivation>`.

**Bumping a version:**
> Bump the project version from `<old>` to `<new>`. Follow the version bump checklist in the blueprint (§7.4): update the manifest, regenerate the SBOM, update changelog, update releasenotes. Do not tag yet.

**Adding a CI gate:**
> Add a CI gate for `<new contract>`. The gate should run `<test command>` on PR and on push to main. Model it on the existing `<similar workflow>`. The gate name (for branch protection) should be `<gate-name>`.

### 15.9 What the Agent Will Not Do Without Being Told

The agent follows §14 conservatively by default. It will not:
- Change the tolerance matrix.
- Modify requirements documents.
- Relax a contract.
- Push to remote.
- Open or close issues.
- Add a dependency without noting it.

If you want any of these, say so explicitly. The agent will confirm before acting.

---

*This document is a living blueprint. Update it when you discover patterns that improve project quality or when a section proves insufficient for actual project needs. The goal is not process for its own sake — every rule here exists because its absence creates measurable quality problems.*
