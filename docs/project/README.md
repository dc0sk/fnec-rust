---
project: fnec-rust
doc: docs/project/README.md
status: living
last_updated: 2026-07-02
---

# Project traceability layer

This directory is the **consolidated traceability layer** for fnec-rust. It ties
the whole delivery chain together in one place:

```
requirements / changes
        │
        ▼
architecture / design decisions
        │
        ▼
implementation (crates, apps, modules)
        │
        ▼
tests (unit, integration, corpus, GPU parity)
        │
        ▼
test results (recorded runs, gates)
        │
        ▲
helper & validation tooling ──────────┘
   (scripts, harnesses, external reference engines)
```

The individual layers already exist across the repo (requirements in
`docs/requirements.md`, per-checklist design docs like
`docs/ph7-chk-003-gpu-resident-solve.md`, tests under `*/tests/`, scripts under
`scripts/`). This layer does **not** replace them — it **links** them so any
requirement can be traced forward to the code and tests that satisfy it, and any
test can be traced back to the requirement it defends.

## Document map

| Layer | Document | What it holds |
|:------|:---------|:--------------|
| ★ Matrix | [traceability-matrix.md](traceability-matrix.md) | The end-to-end matrix: requirement → design → implementation → tests → result. Start here. |
| Requirements / changes | [requirements-register.md](requirements-register.md) | Every requirement, decision, gap, and blocker ID with its source and coverage state. |
| Architecture / design | [architecture-design-index.md](architecture-design-index.md) | Index of foundational architecture docs and per-checklist design/decision records. |
| Implementation | [implementation-map.md](implementation-map.md) | Every crate/app/module with its responsibility and the requirements it serves. |
| Tests | [test-catalog.md](test-catalog.md) | Every test file with its count, what it validates, and the checklist it gates. |
| Test results | [test-results.md](test-results.md) | Recorded test-run results and the standing CI gates. |
| Tooling | [tooling-catalog.md](tooling-catalog.md) | Helper/validation scripts, benchmark harnesses, corpus, and external reference engines. |

## Source of truth vs. this layer

- **`docs/roadmap.md`** remains the authoritative source for *checklist status*
  (which `PHx-CHK-*` items are done and their embedded evidence). The matrix here
  mirrors that status and adds the cross-layer links.
- **`docs/requirements.md`, `docs/nec4-support.md`, `docs/card-support-matrix.md`,
  `docs/corpus-validation-strategy.md`** remain authoritative for their domains.
  The register and matrix here aggregate and point into them.

If this layer and a source-of-truth doc ever disagree, the source-of-truth doc
wins and this layer is the bug — fix it in the same change.

## Maintenance rule (keep it current before each push)

The matrix is only useful if it is honest. **Before every `git push`**, run the
pre-push traceability check:

1. **Requirements** — did this change add/alter a requirement, decision, gap, or
   blocker? Update [requirements-register.md](requirements-register.md).
2. **Design** — did it add or supersede a design/decision doc? Update
   [architecture-design-index.md](architecture-design-index.md).
3. **Implementation** — did it add a module or change a module's responsibility?
   Update [implementation-map.md](implementation-map.md).
4. **Tests** — did it add/rename/remove a test file or change a test count area?
   Update [test-catalog.md](test-catalog.md).
5. **Results** — run `cargo test --workspace` (and, if GPU code changed,
   `cargo test -p nec_accel --features wgpu`) and record the new aggregate in
   [test-results.md](test-results.md) with the commit, date, and toolchain.
6. **Tooling** — did it add a script/harness/reference dependency? Update
   [tooling-catalog.md](tooling-catalog.md).
7. **Matrix** — add/adjust the row(s) in
   [traceability-matrix.md](traceability-matrix.md) so the changed checklist item
   links all five layers, and bump every touched doc's `last_updated`.

A change that closes or advances a `PHx-CHK-*` item is not "done" for this project
until its matrix row links requirement → design → implementation → tests → result.

## Frontmatter contract

Every file here follows the repo frontmatter contract (enforced for `docs/*.md`
by `scripts/validate-docs-frontmatter.sh`; `docs/project/*.md` follow it for
consistency): exactly four keys in order — `project: fnec-rust`, `doc: <path>`,
`status: living`, `last_updated: YYYY-MM-DD`.
