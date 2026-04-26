---
project: fnec-rust
doc: docs/copilot-efficiency-guide.md
status: living
last_updated: 2026-04-26
---

# Copilot Efficiency Guide (Rate-Limit Friendly)

This guide is based on how we worked in this branch: many short "continue" steps, frequent validate/commit cycles, and broad scope over multiple slices.

## Main idea

You get the best throughput when each request contains one complete work packet:

- scope
- acceptance criteria
- validation depth
- git action expectation

That reduces back-and-forth turns, which is the biggest driver of hitting chat limits.

## What to do differently (based on our recent workflow)

1. Bundle multiple "continue" steps into one explicit packet.
- Instead of: continue
- Use: implement X and Y, update docs A and B, run tests T1 and T2, commit and push.

2. Decide validation level per slice up front.
- Say one of: targeted tests only, full cargo test, or no tests for docs-only.
- This avoids repeated clarification and reruns.

3. Set commit strategy in the same prompt.
- Example choices: one commit now, split into two commits, or stage changes but do not commit.

4. Include stop conditions.
- Example: do at most one fix loop if tests fail, then report blockers.

5. Give priority and defer list in one shot.
- Example: do A now, record B in backlog, do not implement B this week.

6. Reuse templates for recurring requests.
- Same structure means fewer mistakes and fewer correction turns.

## Prompt templates that save turns

## Template A: Implementation slice

Goal:
Implement <feature> in <files/components>.

Requirements:
- Keep output/report contract unchanged.
- Update docs: <list>.
- Add tests: <list>.

Validation:
- Run: <commands/tests>.
- If failing: one repair pass, then report remaining blockers.

Git:
- Commit message: <message>.
- Push to current branch.

## Template B: Docs-only slice

Goal:
Update docs for <topic>.

Requirements:
- Update <files>.
- Keep wording consistent with current behavior.
- Add rationale paragraph for defer/priority changes.

Validation:
- No build/tests required unless docs checks are needed.

Git:
- Commit and push.

## Template C: Assessment request

Goal:
Assess implementation effort for <idea>.

Evidence:
- Use these sources: <paths/urls>.

Output:
- Effort level (low/medium/high)
- Top risks
- Recommended phase target
- Exact docs updates to backlog and roadmap

Then apply the docs updates and push.

## High-impact habits for rate-limit avoidance

1. Ask for bigger chunks, fewer turns.
- One strong prompt can replace many small "continue" prompts.

2. Separate exploration and implementation intentionally.
- If you want analysis only, say analysis only.
- Otherwise I will implement immediately.

3. Declare strict boundaries early.
- Mention no-go areas (for example no large refactor, no API changes, no new deps).

4. Ask for concise output format.
- Example: "return only changed files, test result summary, commit SHA".

5. Request batching behavior explicitly.
- Example: "do all steps end-to-end before replying".

## Recommended default command you can reuse

Use this single-line style for most work packets:

Implement <task>, update <docs>, run <validation scope>, fix failures once, then commit with "<message>" and push.

## Suggested operating cadence

- Use small number of larger slices (2-4 per session) instead of many tiny slices.
- Reserve separate turns for major direction changes only.
- Ask for one final summary after each slice.

## Collaboration expectations

I can be fastest when you provide:

- concrete desired outcome
- exact constraints
- validation level
- git intention

If any of those are missing, I can still proceed, but it usually costs extra turns.
