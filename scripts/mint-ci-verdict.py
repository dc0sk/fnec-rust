#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Decide whether CI is green at the commit the Mint release tag workflow tags.

Reads two environment variables:

  SHA                the commit about to be tagged (full 40 hex)
  GITHUB_REPOSITORY  owner/name, as GitHub Actions sets it

and calls `gh api --paginate` for the workflow runs at SHA, then for the jobs of
each run that judges. Prints one `verdict=WORD` line on stdout; a refusal is an
`::error::` line on stderr naming the jobs, the run and the remedy. Writes the
verdict, and any run at SHA that does not judge and is not green, to
`$GITHUB_STEP_SUMMARY` when that is set.

What judges (FND-324): every run of the CI workflow (`.github/workflows/ci.yml`)
at SHA whose event is `push`, latest attempt, every job. A commit reaches main by
a push, and that push runs every ci.yml job (none has an `if:`), so "the push CI
run is green" is "every required status check is green" with no copy of the
list. Scheduled and dispatch runs do not judge: they test the calendar and the
advisory database, not the tree — after day 7 the weekly run at an unminted tip
fails `docs contract` on the release's age, and if that blocked the mint, the
alarm that says "mint this" would make the tip unmintable (and `sha=` the tip is
the same commit). They are listed when not green, so they are seen, not obeyed.
The mint's own runs are excluded by event, not by job name.

Verdicts, in this precedence, from the closed table CONCLUSIONS below:

  none       no push-event CI run at SHA (exit 1)
  pending    a job has not completed (exit 75, EX_TEMPFAIL: dispatch again later)
  cancelled  a job was cancelled — a later push cancels a run (exit 1)
  failed     failure, timed_out, action_required, startup_failure, stale (exit 1)
  not-run    skipped, neutral (exit 1)
  unknown    a conclusion this table does not list (exit 1)
  green      every job of every push CI run is completed/success (exit 0)

Never the word "failed" for a state that is not a failure: the check-runs gate
this replaces called an in-progress job "failed" (run 41404269403, 21 s after
the merge) and sent the operator looking for a failure.

`--paginate` is never combined with `--jq` here or in any workflow: gh applies
the filter to each page and prints one result per page, so a reduction over it
is a per-page verdict (measured, FND-324). The page stream — concatenated JSON
documents — is parsed here, all of it.

A broken gh call is a refusal with gh's own stderr, never a verdict.

Run the self-test: python3 scripts/test-mint-ci-verdict.py
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys

CI_WORKFLOW = ".github/workflows/ci.yml"
JUDGING_EVENT = "push"

# The closed vocabulary of a completed job's `conclusion` (GitHub's full set),
# mapped to the word the mint says. A token not listed here is `unknown`, which
# refuses: a new GitHub state must be read before it can pass or be named.
CONCLUSIONS: dict[str, str] = {
    "success": "green",
    "cancelled": "cancelled",
    "failure": "failed",
    "timed_out": "failed",
    "action_required": "failed",
    "startup_failure": "failed",
    "stale": "failed",
    "skipped": "not-run",
    "neutral": "not-run",
}

# Highest first. `pending` is decided by `status`, before any conclusion.
PRECEDENCE = ["pending", "cancelled", "failed", "not-run", "unknown"]

EXIT = {"green": 0, "pending": 75}  # everything else: 1

FULL_SHA = re.compile(r"[0-9a-f]{40}")


class GhError(Exception):
    pass


def parse_pages(text: str) -> list[dict]:
    """Every JSON document in gh's `--paginate` output, in order."""
    decoder = json.JSONDecoder()
    docs: list[dict] = []
    i = 0
    while True:
        while i < len(text) and text[i].isspace():
            i += 1
        if i >= len(text):
            break
        doc, i = decoder.raw_decode(text, i)
        if not isinstance(doc, dict):
            raise ValueError(f"a page is a {type(doc).__name__}, not an object")
        docs.append(doc)
    if not docs:
        raise ValueError("no JSON document in the output")
    return docs


def gh_paginate(url: str, key: str) -> list[dict]:
    """All items under `key` across every page of `gh api --paginate URL`."""
    p = subprocess.run(["gh", "api", "--paginate", url],
                       capture_output=True, text=True, check=False)
    if p.returncode != 0:
        raise GhError(f"`gh api --paginate {url}` exited {p.returncode}: "
                      f"{p.stderr.strip() or '(no stderr)'}")
    try:
        pages = parse_pages(p.stdout)
    except ValueError as e:
        raise GhError(f"`gh api --paginate {url}` did not print JSON pages ({e}): "
                      f"{p.stdout[:200]!r} {p.stderr.strip()}") from None
    items: list[dict] = []
    for page in pages:
        if not isinstance(page.get(key), list):
            raise GhError(f"`gh api --paginate {url}`: a page has no `{key}` list")
        items.extend(page[key])
    return items


def judge(status: str | None, conclusion: str | None) -> str:
    if status != "completed":
        return "pending"
    return CONCLUSIONS.get(conclusion or "", "unknown")


def refuse(message: str) -> int:
    print(f"::error::{message}", file=sys.stderr)
    return 1


def write_summary(text: str) -> None:
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(text)


def main() -> int:
    sha = os.environ.get("SHA", "")
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    if not FULL_SHA.fullmatch(sha):
        return refuse(f"SHA must be a full 40-character lowercase hex commit id, got {sha!r}")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
        return refuse(f"GITHUB_REPOSITORY must be owner/name, got {repo!r}")
    own_run = os.environ.get("GITHUB_RUN_ID", "")

    try:
        runs = gh_paginate(f"repos/{repo}/actions/runs?head_sha={sha}&per_page=100",
                           "workflow_runs")
        judging = [r for r in runs
                   if r.get("event") == JUDGING_EVENT and r.get("path") == CI_WORKFLOW]
        others = [r for r in runs if r not in judging and str(r.get("id")) != own_run]

        # (word, job name, run) for every job that is not green.
        bad: list[tuple[str, str, dict]] = []
        for run in judging:
            jobs = gh_paginate(f"repos/{repo}/actions/runs/{run['id']}/jobs?per_page=100",
                               "jobs")
            if not jobs:
                # A run that never created a job (queued, or a startup_failure)
                # is judged by its own status and conclusion.
                jobs = [{"name": "(the run itself)", "status": run.get("status"),
                         "conclusion": run.get("conclusion")}]
            for job in jobs:
                word = judge(job.get("status"), job.get("conclusion"))
                if word != "green":
                    shown = (f"unknown conclusion {job.get('conclusion')}"
                             if word == "unknown"
                             else f"{job.get('status')}/{job.get('conclusion')}")
                    bad.append((word, f"{job.get('name')} ({shown})", run))
    except GhError as e:
        write_summary(f"### CI at {sha}: could not be read\n\n{e}\n")
        return refuse(f"could not read the CI runs at {sha} — no verdict: {e}")

    advisory = [r for r in others
                if judge(r.get("status"), r.get("conclusion")) != "green"]
    advisory_text = "".join(
        f"- {r.get('name')} ({r.get('event')}): {r.get('status')}/{r.get('conclusion')} "
        f"{r.get('html_url')}\n" for r in advisory)
    if advisory:
        sys.stderr.write("Not judged (not a push CI run), and not green — seen, not obeyed:\n"
                         + advisory_text)

    if not judging:
        verdict = "none"
        message = (f"CI at {sha}: none — no push-event CI run at this commit. A commit "
                   f"on main without a push run is an anomaly: every push to main runs "
                   f"{CI_WORKFLOW}, so find how this commit reached main before minting it")
    elif not bad:
        verdict = "green"
        message = (f"CI at {sha}: green — every job of "
                   + ", ".join(f"run {r['id']}" for r in judging) + " is completed/success")
    else:
        verdict = min((w for w, _, _ in bad), key=PRECEDENCE.index)
        hit = [(name, run) for w, name, run in bad if w == verdict]
        run_ids = sorted({run["id"] for _, run in hit})
        urls = ", ".join(sorted({str(run.get("html_url")) for _, run in hit}))
        names = ", ".join(name for name, _ in hit)
        ids = ", ".join(str(i) for i in run_ids)
        remedy = {
            "pending": f"wait for run {ids} to finish, then dispatch the mint again",
            "cancelled": (f"a later push cancelled run {ids}; "
                          + "; ".join(f"`gh run rerun {i}`" for i in run_ids)
                          + ", then dispatch the mint again"),
            "failed": f"the push CI run {ids} is red; fix it on main before minting",
            "not-run": (f"run {ids} skipped jobs, and no ci.yml job has an `if:`; "
                        f"find out why before minting"),
            "unknown": (f"run {ids} reports a conclusion this gate does not know; read "
                        f"it, then add it to CONCLUSIONS in scripts/mint-ci-verdict.py"),
        }[verdict]
        message = f"CI at {sha}: {verdict} — {names} in {urls}. {remedy}"
        others_bad = [f"{w}: {name} (run {run['id']})" for w, name, run in bad if w != verdict]
        if others_bad:
            message += ". Also not green: " + "; ".join(others_bad)

    print(f"verdict={verdict}")
    write_summary(f"### CI at {sha}: {verdict}\n\n{message}\n"
                  + (f"\nNot judged (not a push CI run), and not green:\n\n{advisory_text}"
                     if advisory else ""))
    if verdict == "green":
        print(message, file=sys.stderr)
        return 0
    print(f"::error::{message} — refusing to tag it", file=sys.stderr)
    return EXIT.get(verdict, 1)


if __name__ == "__main__":
    sys.exit(main())
