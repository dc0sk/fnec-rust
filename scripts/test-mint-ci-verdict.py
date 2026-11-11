#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Known-pass / known-fail cases for `mint-ci-verdict.py`.

The verdict gates the one workflow whose token can write release history, and
it cannot be run except by minting — which is how its old inline form called an
in-progress job "failed" and reduced per page without anyone seeing it
(FND-324). So its verdicts are watched here, in CI, in check-all and as the mint
job's first step.

A fake `gh` goes first on PATH. It answers `gh api --paginate URL` from a fixture
file, printing every page as a concatenated JSON document the way gh does
(measured, gh 2.46.0); without `--paginate` it prints only the first page, as gh
does, and it refuses `--jq`. Every case gets a fresh fixture set, and the
real gh is never reached: an unexpected URL is an error, not an empty answer.

Last, the seam: the mint's "Require CI green" step runs the script, its job can
read actions, and nothing under .github/workflows/, scripts/ or .githooks/ pairs
`--paginate` with `--jq`.

Run: python3 scripts/test-mint-ci-verdict.py
"""

from __future__ import annotations

import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).parent.resolve()
ROOT = HERE.parent
SCRIPT = HERE / "mint-ci-verdict.py"

_spec = importlib.util.spec_from_file_location("mint_ci_verdict", SCRIPT)
assert _spec and _spec.loader
verdict_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(verdict_mod)

REPO = "owner/repo"
SHA = "59fa4b4afa7184115684571c691b5dbdd9bf14c3"
CI = ".github/workflows/ci.yml"
BENCH = ".github/workflows/benchmark-dashboard.yml"
MINT = ".github/workflows/release-tag.yml"
CI_JOBS = ["rustfmt", "clippy -D warnings", "test", "cargo-audit", "cargo-deny",
           "python bindings", "docs contract", "coverage"]

# GitHub's documented set of completed-job conclusions. The script's table must
# cover every one; a token GitHub adds is the "unknown" case below until read.
GITHUB_CONCLUSIONS = {"success", "failure", "neutral", "cancelled", "skipped",
                      "timed_out", "action_required", "stale", "startup_failure"}

FAKE_GH = r'''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["FAKE_GH_CALLS"], "a") as f:
    f.write(json.dumps(args) + "\n")
if "--jq" in args or "-q" in args:
    sys.stderr.write("fake gh: --jq refused\n"); sys.exit(2)
if not args or args[0] != "api":
    sys.stderr.write("fake gh: only `api` is faked\n"); sys.exit(2)
urls = [a for a in args[1:] if not a.startswith("-")]
if len(urls) != 1:
    sys.stderr.write(f"fake gh: expected one URL, got {urls}\n"); sys.exit(2)
fixtures = json.load(open(os.environ["FAKE_GH_FIXTURES"]))
entry = fixtures.get(urls[0])
if entry is None:
    sys.stderr.write(f"fake gh: no fixture for {urls[0]}\n"); sys.exit(1)
if "exit" in entry:
    sys.stderr.write(entry.get("stderr", "")); sys.exit(entry["exit"])
if "raw" in entry:
    sys.stdout.write(entry["raw"]); sys.exit(0)
pages = entry["pages"] if "--paginate" in args else entry["pages"][:1]
sys.stdout.write("".join(json.dumps(p) for p in pages))
'''

failures: list[str] = []
ran = 0


def case(name: str, ok: bool, detail: str) -> None:
    global ran
    ran += 1
    if not ok:
        failures.append(f"{name}: {detail}")


def runs_url() -> str:
    return f"repos/{REPO}/actions/runs?head_sha={SHA}&per_page=100"


def jobs_url(run_id: int) -> str:
    return f"repos/{REPO}/actions/runs/{run_id}/jobs?per_page=100"


def split(items: list[dict], per_page: int | None) -> list[list[dict]]:
    if not per_page:
        return [items]
    return [items[i:i + per_page] for i in range(0, len(items), per_page)] or [[]]


def run(run_id: int, event: str, path: str, jobs: list[tuple[str, str, str | None]],
        jobs_per_page: int | None = None, status: str = "completed",
        conclusion: str | None = None) -> dict:
    if conclusion is None and jobs:
        conclusion = "success" if all(c == "success" for _, _, c in jobs) else "failure"
    return {
        "run": {"id": run_id, "name": Path(path).stem, "event": event, "path": path,
                "status": status, "conclusion": conclusion, "run_attempt": 1,
                "html_url": f"https://github.com/{REPO}/actions/runs/{run_id}"},
        "jobs": [{"id": run_id * 100 + i, "name": n, "status": s, "conclusion": c,
                  "run_attempt": 1} for i, (n, s, c) in enumerate(jobs)],
        "jobs_per_page": jobs_per_page,
    }


def green(names: list[str] = CI_JOBS) -> list[tuple[str, str, str | None]]:
    return [(n, "completed", "success") for n in names]


def with_job(name: str, status: str, conclusion: str | None) -> list[tuple[str, str, str | None]]:
    return [(n, status, conclusion) if n == name else (n, "completed", "success")
            for n in CI_JOBS]


def fixtures_for(runs: list[dict], runs_per_page: int | None = None) -> dict:
    fx: dict = {}
    pages = split([r["run"] for r in runs], runs_per_page)
    fx[runs_url()] = {"pages": [{"total_count": len(runs), "workflow_runs": p}
                                for p in pages]}
    for r in runs:
        jp = split(r["jobs"], r["jobs_per_page"])
        fx[jobs_url(r["run"]["id"])] = {"pages": [{"total_count": len(r["jobs"]), "jobs": p}
                                                  for p in jp]}
    return fx


def verdict(fixtures: dict, extra_env: dict | None = None
            ) -> tuple[int, str, str, str, list[list[str]]]:
    """(exit, verdict word, stderr, step summary, gh calls)."""
    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)
        bindir = t / "bin"
        bindir.mkdir()
        (bindir / "gh").write_text(f"#!{sys.executable}\n" + FAKE_GH.split("\n", 1)[1])
        (bindir / "gh").chmod(0o755)
        (t / "fixtures.json").write_text(json.dumps(fixtures))
        calls = t / "calls"
        summary = t / "summary.md"
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(("GH_", "GITHUB_"))}
        env.update(PATH=f"{bindir}{os.pathsep}{os.environ.get('PATH', '')}",
                   FAKE_GH_FIXTURES=str(t / "fixtures.json"), FAKE_GH_CALLS=str(calls),
                   GITHUB_STEP_SUMMARY=str(summary), GITHUB_REPOSITORY=REPO, SHA=SHA)
        env.update(extra_env or {})
        p = subprocess.run([sys.executable, str(SCRIPT)], env=env, capture_output=True,
                           text=True, cwd=tmp)
        word = ""
        for line in p.stdout.splitlines():
            if line.startswith("verdict="):
                word = line.split("=", 1)[1]
        text = summary.read_text() if summary.exists() else ""
        made = ([json.loads(x) for x in calls.read_text().splitlines()]
                if calls.exists() else [])
        return p.returncode, word, p.stderr, text, made


def push_ci(run_id: int = 41404231024, jobs=None, **kw) -> dict:
    return run(run_id, "push", CI, jobs if jobs is not None else green(), **kw)


BENCH_GREEN = run(41404231000, "push", BENCH, green(["benchmark", "gate-injection-test"]))

# --- control: the fake really is what the script calls ----------------------
code, word, err, summ, calls = verdict(fixtures_for([push_ci(), BENCH_GREEN]))
case("pass: every push CI job completed/success is green, exit 0",
     code == 0 and word == "green" and "green" in summ, f"exit {code}, {word!r}, {err!r}")
case("control: the script called the fake gh, with --paginate and without --jq",
     len(calls) == 2 and all(c[:2] == ["api", "--paginate"] and "--jq" not in c
                             for c in calls), f"calls {calls}")
case("only the CI workflow judges: Benchmark Dashboard's jobs are not read",
     all(jobs_url(41404231000) not in c for c in calls), f"calls {calls}")

# --- the incident: 21 s after the merge, test and coverage still running ------
jobs = [(n, "in_progress", None) if n in ("test", "coverage") else (n, "completed", "success")
        for n in CI_JOBS]
code, word, err, summ, _ = verdict(fixtures_for([push_ci(jobs=jobs, status="in_progress")]))
case("pending: an in-progress job is pending, exit 75",
     code == 75 and word == "pending", f"exit {code}, {word!r}, {err!r}")
case("pending: the message names the jobs and the run, and never says failed",
     "pending" in err and "test (in_progress/None)" in err and "coverage" in err
     and "41404231024" in err and "failed" not in err and "failed" not in summ,
     f"{err!r}")
case("pending: the remedy is to wait and dispatch again",
     "wait for run 41404231024" in err, f"{err!r}")

# A queued job that has not even started is pending too, whatever it carries.
code, word, err, _, _ = verdict(fixtures_for([push_ci(jobs=with_job("test", "queued", None))]))
case("pending: a queued job is pending", code == 75 and word == "pending",
     f"exit {code}, {word!r}, {err!r}")

# --- cancelled: the 8a42df39 shape, four cancelled and four green ------------
jobs = [(n, "completed", "cancelled") if i < 4 else (n, "completed", "success")
        for i, n in enumerate(CI_JOBS)]
code, word, err, _, _ = verdict(fixtures_for([push_ci(39487155576, jobs=jobs,
                                                      conclusion="cancelled")]))
case("cancelled: four cancelled jobs are cancelled, exit 1",
     code == 1 and word == "cancelled", f"exit {code}, {word!r}, {err!r}")
case("cancelled: the remedy is gh run rerun, and the message never says failed",
     "`gh run rerun 39487155576`" in err and "failed" not in err, f"{err!r}")

# --- failed --------------------------------------------------------------------
code, word, err, _, _ = verdict(fixtures_for([push_ci(jobs=with_job("clippy -D warnings",
                                                                    "completed", "failure"))]))
case("failed: one failure is failed, exit 1, naming the job",
     code == 1 and word == "failed" and "clippy -D warnings" in err,
     f"exit {code}, {word!r}, {err!r}")

# --- precedence: pending beats cancelled beats failed beats not-run ----------
for higher, lower in zip(verdict_mod.PRECEDENCE, verdict_mod.PRECEDENCE[1:]):
    def job(word: str) -> tuple[str, str | None]:
        # Literal tokens, not looked up in the script's table: a row deleted there
        # must turn a case red, not crash the test.
        return {"pending": ("in_progress", None), "cancelled": ("completed", "cancelled"),
                "failed": ("completed", "failure"), "not-run": ("completed", "skipped"),
                "unknown": ("completed", "made_up_token")}[word]
    (hs, hc), (ls, lc) = job(higher), job(lower)
    jobs = [("rustfmt", ls, lc), ("test", hs, hc)] + green(CI_JOBS[3:])
    code, word, err, _, _ = verdict(fixtures_for([push_ci(jobs=jobs)]))
    case(f"precedence: {higher} over {lower}",
         word == higher and "Also not green" in err, f"{word!r}, {err!r}")

# --- the closed table: one case per token, generated from the script's own table
case("the table covers GitHub's whole conclusion vocabulary",
     GITHUB_CONCLUSIONS <= set(verdict_mod.CONCLUSIONS),
     f"missing {sorted(GITHUB_CONCLUSIONS - set(verdict_mod.CONCLUSIONS))}")
EXPECTED = {"success": "green", "cancelled": "cancelled", "failure": "failed",
            "timed_out": "failed", "action_required": "failed", "startup_failure": "failed",
            "stale": "failed", "skipped": "not-run", "neutral": "not-run"}
for token, expected in EXPECTED.items():
    case(f"table: {token} means {expected}", verdict_mod.CONCLUSIONS.get(token) == expected,
         f"the table says {verdict_mod.CONCLUSIONS.get(token)!r}")
for token, word_expected in verdict_mod.CONCLUSIONS.items():
    code, word, err, _, _ = verdict(fixtures_for([push_ci(jobs=with_job("test", "completed",
                                                                        token))]))
    want_exit = 0 if word_expected == "green" else 1
    case(f"token {token} → {word_expected}, exit {want_exit}",
         word == word_expected and code == want_exit, f"exit {code}, {word!r}, {err!r}")
    if word_expected not in ("green", "failed"):
        case(f"token {token}: the message does not say failed", "failed" not in err, f"{err!r}")

code, word, err, _, _ = verdict(fixtures_for([push_ci(jobs=with_job("test", "completed",
                                                                    "made_up_token"))]))
case("a made-up conclusion refuses as unknown conclusion",
     code == 1 and word == "unknown" and "unknown conclusion made_up_token" in err,
     f"exit {code}, {word!r}, {err!r}")

# --- none: no push run at the SHA -----------------------------------------------
code, word, err, _, _ = verdict(fixtures_for([
    run(42574132665, "schedule", CI, green()),
    run(41437031590, "workflow_dispatch", MINT, green(["mint"])),
]))
case("none: only schedule and dispatch runs is none, exit 1",
     code == 1 and word == "none", f"exit {code}, {word!r}, {err!r}")
case("none: the message names the anomaly, a commit on main without a push run",
     "without a push run" in err and "anomaly" in err and "failed" not in err, f"{err!r}")

# A push run of another workflow is not the CI workflow's.
code, word, err, _, _ = verdict(fixtures_for([BENCH_GREEN]))
case("none: a Benchmark Dashboard push run alone is none",
     code == 1 and word == "none", f"exit {code}, {word!r}, {err!r}")

# --- event filter --------------------------------------------------------------
code, word, err, summ, _ = verdict(fixtures_for([
    push_ci(),
    run(41404269403, "workflow_dispatch", MINT, [("mint", "completed", "failure")]),
    run(42574132665, "schedule", CI, with_job("docs contract", "completed", "failure")),
]))
case("event filter: a red mint dispatch and a red schedule run do not decide",
     code == 0 and word == "green", f"exit {code}, {word!r}, {err!r}")
case("event filter: the red schedule run is listed in the summary and the log",
     "42574132665" in summ and "schedule" in summ and "42574132665" in err, f"{summ!r}")

# The mint's own in-progress run is not listed against itself.
code, word, err, summ, _ = verdict(fixtures_for([
    push_ci(), run(55, "workflow_dispatch", MINT, [("mint", "in_progress", None)],
                   status="in_progress")]), {"GITHUB_RUN_ID": "55"})
case("the mint's own run is not listed", code == 0 and "/55" not in summ, f"{summ!r}")

# A Benchmark Dashboard push run that is red is listed but does not decide.
code, word, err, summ, _ = verdict(fixtures_for([
    push_ci(), run(41404231000, "push", BENCH, [("benchmark", "completed", "failure")])]))
case("a red Benchmark Dashboard push run does not decide, and is listed",
     code == 0 and word == "green" and "41404231000" in summ, f"exit {code}, {summ!r}")

# --- pagination ---------------------------------------------------------------
others = [run(1000 + i, "workflow_dispatch", MINT, green(["mint"])) for i in range(3)]
code, word, err, _, _ = verdict(fixtures_for(
    others + [push_ci(jobs=with_job("test", "completed", "failure"))], runs_per_page=2))
case("pagination: a failing push run on page 2 of the runs is failed",
     word == "failed" and code == 1, f"exit {code}, {word!r}, {err!r}")
code, word, err, _, _ = verdict(fixtures_for(
    [push_ci(jobs=with_job("coverage", "completed", "failure"), jobs_per_page=3)]))
case("pagination: a failing job on page 3 of the jobs is failed",
     word == "failed" and code == 1 and "coverage" in err, f"exit {code}, {word!r}, {err!r}")
code, word, err, _, _ = verdict(fixtures_for(
    [push_ci(jobs=with_job("coverage", "in_progress", None), jobs_per_page=3)]))
case("pagination: a pending job on the last page is pending",
     word == "pending" and code == 75, f"exit {code}, {word!r}, {err!r}")

# --- a broken gh call is not a verdict -------------------------------------------
for name, entry in (("exits 1", {"exit": 1, "stderr": "HTTP 403: Resource not accessible\n"}),
                    ("prints non-JSON", {"raw": "<html>rate limited</html>"}),
                    ("prints nothing", {"raw": ""})):
    code, word, err, _, _ = verdict({runs_url(): entry})
    case(f"gh failure ({name}) on the runs list refuses with gh's words, never none",
         code == 1 and word == "" and "could not read" in err and "none" not in err
         and (name != "exits 1" or "HTTP 403" in err), f"exit {code}, {word!r}, {err!r}")
fx = fixtures_for([push_ci()])
fx[jobs_url(41404231024)] = {"exit": 1, "stderr": "HTTP 502\n"}
code, word, err, _, _ = verdict(fx)
case("gh failure on a jobs list refuses with gh's words",
     code == 1 and word == "" and "HTTP 502" in err, f"exit {code}, {word!r}, {err!r}")

for bad_sha in ("59fa4b4", "$(touch pwned)" + "0" * 30):
    code, word, err, _, calls = verdict(fixtures_for([push_ci()]), {"SHA": bad_sha})
    case(f"a SHA that is not 40 hex is refused before gh is called ({bad_sha[:10]!r})",
         code == 1 and word == "" and not calls, f"exit {code}, {calls}, {err!r}")

# --- the seam ------------------------------------------------------------------
def logical_lines(text: str) -> list[str]:
    """Shell-ish lines with `\\`-continuations joined, comment lines dropped."""
    return [line for line in re.sub(r"\\\n", " ", text).splitlines()
            if not line.lstrip().startswith("#")]


wf = (ROOT / ".github/workflows/release-tag.yml").read_text(encoding="utf-8")
m = re.search(r"- name: Require CI green[^\n]*\n(.*?)(?=\n      - (?:name|uses):|\Z)", wf, re.S)
step = m.group(1) if m else ""
case("seam: release-tag.yml's Require CI green step runs mint-ci-verdict.py",
     "python3 scripts/mint-ci-verdict.py" in step
     and not any("--jq" in line for line in logical_lines(step)),
     f"step {step!r}")
case("seam: the mint job's token can read actions", re.search(
    r"\n  mint:\n(?:.*\n)*?    permissions:\n(?:      .*\n)*?      actions: read\n", wf)
    is not None, "no `actions: read` in the mint job's permissions")
case("seam: the mint job runs this self-test",
     "python3 scripts/test-mint-ci-verdict.py" in wf, "not in release-tag.yml")


offenders = []
for d in (".github/workflows", "scripts", ".githooks"):
    for f in sorted((ROOT / d).glob("*")):
        if not f.is_file() or f.resolve() in (SCRIPT, Path(__file__).resolve()):
            continue
        try:
            text = f.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for line in logical_lines(text):
            if "--paginate" in line and re.search(r"(--jq|\s-q\s)", line):
                offenders.append(f"{f.relative_to(ROOT)}: {line.strip()}")
case("seam: nothing pairs `gh api --paginate` with `--jq` (a per-page reduction)",
     not offenders, f"{offenders}")

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    sys.exit(1)
print(f"mint-ci-verdict self-test OK — {ran} case(s)")
