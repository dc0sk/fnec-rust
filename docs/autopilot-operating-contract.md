---
project: fnec-rust
doc: docs/autopilot-operating-contract.md
status: living
last_updated: 2026-04-28
---

# Autopilot Operating Contract

## Purpose

Define near-continuous execution rules for autonomous development with minimal interruption, while preserving branch safety and quality gates.

## Priority Source Of Truth

- Single source of truth: docs/roadmap.md
- Selection rule: always pick the top not-done item unless blocked
- Blocking condition: if blocked, publish blocker report using the required format and pause for input

## Branch And Main Protection

- Main stays protected
- Never work directly on main
- Every work cycle uses a topic branch
- Branch naming:
  - feat/topic
  - fix/topic
  - docs/topic
- Lifecycle rule: create a new topic branch immediately after each merge

## Allowed Autonomous Actions

Without additional approval, the agent may:

- Edit files
- Run tests and validation commands
- Commit changes
- Push branch updates
- Open PRs
- Merge PRs when checks pass and policy allows

## Merge Policy

- Default merge method: normal merge commit
- Auto-merge: allowed when checks pass
- Squash: allowed only when there are many small low-risk commits

## Mandatory Quality Gates

### Code Tasks

- fmt
- check
- unit and integration tests

### Docs-Only Tasks

- Docs frontmatter validation script used previously in this repository workflow

### Release Tasks

- Full validation matrix
- Version checks

### Flaky Failures

- Stop and ask for user decision

## Mandatory Interrupt Boundaries

The agent must interrupt and ask before proceeding when a change would cause any of the following:

- Numerical result changes beyond corpus tolerance
- Warning or error message contract changes
- CLI output format changes
- Flag semantics changes

## Blocker Report Template

- blocker: what exactly failed
- tried: what was already attempted
- need: smallest decision or input required from user
- impact: what is blocked until resolved

## End-Of-Run Handoff Fields

- current branch
- PR link and status
- checks status
- done in this cycle
- next exact action

## Definition Of Done Per Task Type

Every completed task must satisfy all of:

- code done
- test done
- docs done
- release-note done
