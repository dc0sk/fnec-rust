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

## Autonomous PR Merge Authority

- Delegated authority: the agent may merge eligible PRs without additional confirmation
- Eligibility requirements:
  - all required branch-protection checks are successful
  - no required check is pending, canceled, or failing
  - no merge conflicts
  - no blocking labels such as wip, do-not-merge, or hold
  - PR scope matches the active workstream target
- Branch protection compatibility:
  - autonomous merge is allowed only when repository review policy permits it
  - if review policy blocks merge, stop and report the policy blocker

## Automation Identity Requirements

- pull requests: write
- repository contents: write
- checks and status: read
- branch deletion after merge: allowed

## Merge Policy

- Default merge method: normal merge commit
- Auto-merge: allowed when checks pass and eligibility requirements are met
- Squash: allowed only when there are many small low-risk commits
- Delete source branch after merge
- Immediately create next topic branch and continue with the next smallest safe increment

## Check Polling And Timeout

- Poll required checks until completion
- Treat pending checks as non-fatal while waiting
- Timeout window: 60 minutes per PR
- On timeout, stop merge attempts and report:
  - PR number
  - still-pending checks
  - last known check snapshot

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

## Hard Stop Conditions

- Any required check fails
- Merge conflict appears
- Permission or policy denial prevents merge
- Unexpected repository state makes continuation unsafe

When a hard stop is hit, do not force merge; publish blocker report and wait.

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

## Merge Audit Trail Fields

For each autonomous merge cycle, include:

- branch name
- commit sha
- PR link or number
- checks summary at merge time
- next branch name after merge

## Safety Constraints

- Never revert unrelated local changes
- Never use destructive history rewrite commands unless explicitly requested
- Keep each increment small and scoped
- If scope is ambiguous, pause and ask for clarification

## Definition Of Done Per Task Type

Every completed task must satisfy all of:

- code done
- test done
- docs done
- release-note done
