#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Known-pass / known-fail cases for `mint-resolve.py`.

The resolver decides which commit the Mint release tag workflow tags — the one
workflow whose token can write release history — and it cannot be run except by
minting. So its refusals are watched failing here, in CI, in check-all and as
the mint job's first step, before a mint depends on them (FND-322).

Every case builds its own scratch repository: the same history each time, so
that no case can pass on state another one left behind.

Run: python3 scripts/test-mint-resolve.py
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).parent.resolve() / "mint-resolve.py"
REAL_GIT = shutil.which("git")
assert REAL_GIT, "git is not on PATH"

failures: list[str] = []
ran = 0

ENV = {
    "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
    "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e",
}

# The history every case gets. `release` is the release PR's merge commit; two
# commits land on main after it, the second bumping to the next version — the
# v0.25.0 situation one step further on, where main's version no longer names
# the release at all. `side` is a commit that never reached main.
LATER_ON_MAIN = 2


def build(root: Path) -> dict[str, str]:
    env = {**os.environ, **ENV}

    def git(*a: str) -> str:
        return subprocess.run([REAL_GIT, "-C", str(root), *a], check=True, env=env,
                              capture_output=True, text=True).stdout.strip()

    def commit(version: str, subject: str) -> str:
        (root / "Cargo.toml").write_text(f'[workspace.package]\nversion = "{version}"\n')
        (root / "notes.txt").write_text(subject + "\n")
        git("add", "Cargo.toml", "notes.txt")
        git("commit", "-qm", subject)
        return git("rev-parse", "HEAD")

    subprocess.run([REAL_GIT, "init", "-q", "-b", "main", str(root)], check=True,
                   capture_output=True)
    shas = {"old": commit("1.0.0", "the previous release")}
    git("tag", "-a", "v1.0.0", "-m", "v1.0.0")
    shas["tag_object"] = git("rev-parse", "v1.0.0")
    shas["release"] = commit("1.1.0", "release 1.1.0 (#10)")
    git("checkout", "-qb", "side")
    shas["side"] = commit("1.1.0", "a branch that never merged")
    git("checkout", "-q", "main")
    commit("1.1.0", "a later fix (#11)")
    shas["tip"] = commit("1.2.0", "bump to 1.2.0 (#12)")
    return shas


def resolve(sha_input: str | None, requested: str | None = None, spy: bool = False
            ) -> tuple[int, dict[str, str], str, str, bool, dict[str, str]]:
    """Run the resolver in a fresh scratch repo.

    `sha_input` may name a key of the built history (`@release`) or be literal.
    Returns (exit, outputs, stderr, step summary, whether git was ever called,
    the built history's commit ids).
    With `spy`, git on PATH is a wrapper that records each call and then runs the
    real git — so a refusal "before any git call" is observed, not inferred.
    """
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "repo"
        shas = build(root)
        env = {k: v for k, v in os.environ.items()
               if k not in ("REQUESTED", "SHA_INPUT", "GITHUB_STEP_SUMMARY")}
        env.update(ENV)
        if sha_input is not None:
            env["SHA_INPUT"] = shas[sha_input[1:]] if sha_input.startswith("@") else sha_input
        if requested is not None:
            env["REQUESTED"] = requested
        summary = Path(tmp) / "summary.md"
        env["GITHUB_STEP_SUMMARY"] = str(summary)
        calls = Path(tmp) / "git-calls"
        if spy:
            bindir = Path(tmp) / "bin"
            bindir.mkdir()
            wrapper = bindir / "git"
            wrapper.write_text(f'#!/bin/sh\necho "$@" >> "{calls}"\nexec "{REAL_GIT}" "$@"\n')
            wrapper.chmod(0o755)
            env["PATH"] = f"{bindir}{os.pathsep}{env.get('PATH', '')}"
        p = subprocess.run([sys.executable, str(SCRIPT)], cwd=root, env=env,
                           capture_output=True, text=True)
        outputs = dict(line.split("=", 1) for line in p.stdout.splitlines() if "=" in line)
        text = summary.read_text() if summary.exists() else ""
        return p.returncode, outputs, p.stderr, text, calls.exists(), shas


def case(name: str, ok: bool, detail: str) -> None:
    global ran
    ran += 1
    if not ok:
        failures.append(f"{name}: {detail}")


# --- tip mode: today's behaviour --------------------------------------------
code, out, err, _, _, shas = resolve(None)
case("no input tags the tip",
     code == 0 and out.get("sha") == shas["tip"] and out.get("mode") == "tip"
     and out.get("version") == "1.2.0" and out.get("behind") == "0",
     f"exit {code}, {out}, {err.strip()!r}")

code, out, err, _, _, shas = resolve(None, requested="1.1.0")
case("tip mode: a requested version main does not declare is refused",
     code == 1 and "sha" not in out, f"exit {code}, {out}")

# --- sha mode: the release commit, behind a bumped main ---------------------
code, out, err, summary, _, shas = resolve("@release", requested="1.1.0")
case("an ancestor declaring the version is tagged",
     code == 0 and out.get("sha") == shas["release"] and out.get("mode") == "sha"
     and out.get("version") == "1.1.0" and out.get("tip") == shas["tip"],
     f"exit {code}, {out}, {err.strip()!r}")
case("behind is the real count of later commits",
     out.get("behind") == str(LATER_ON_MAIN), f"behind={out.get('behind')!r}")
case("what is left out reaches the log and the step summary",
     all(f"{LATER_ON_MAIN} commit(s) on main after the tagged commit" in s
         and "a later fix (#11)" in s and "bump to 1.2.0 (#12)" in s
         for s in (err, summary)),
     f"stderr {err!r}, summary {summary!r}")

code, out, err, summary, _, shas = resolve("@tip")
case("the tip passed as sha is behind 0",
     code == 0 and out.get("mode") == "sha" and out.get("behind") == "0"
     and out.get("version") == "1.2.0" and "main's tip" in err and "main's tip" in summary,
     f"exit {code}, {out}, {err.strip()!r}")

# --- sha mode refusals ------------------------------------------------------
code, out, err, _, _, shas = resolve("@side")
case("a commit that never reached main is refused",
     code == 1 and "sha" not in out and "not an ancestor" in err,
     f"exit {code}, {out}, {err.strip()!r}")

# The tree's version, not main's: main declares 1.2.0, the old commit 1.0.0, and
# asking for 1.2.0 there must not pass on main's say-so.
code, out, err, _, _, shas = resolve("@old", requested="1.2.0")
case("an ancestor declaring a different version than requested is refused",
     code == 1 and "sha" not in out, f"exit {code}, {out}, {err.strip()!r}")

code, out, err, _, _, shas = resolve("@release", requested="1.1.1")
case("a requested version the sha's tree does not declare is refused",
     code == 1 and "sha" not in out and "declares 1.1.0" in err,
     f"exit {code}, {out}, {err.strip()!r}")


# A control for the spy itself: in tip mode the resolver does call git, so a
# wrapper that recorded nothing would make every "git never called" case vacuous.
code, out, err, _, called, shas = resolve(None, spy=True)
case("control: the git spy records calls", called, "the wrapper recorded nothing in tip mode")


def resolve_abbrev() -> tuple[int, dict[str, str], str, bool]:
    """A 7-char prefix of the release commit, in a repo where git resolves it.

    The refusal must be on the pattern, so the prefix must be one git would have
    accepted; otherwise the case could pass on ambiguity instead.
    """
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "repo"
        shas = build(root)
        prefix = shas["release"][:7]
        unique = subprocess.run([REAL_GIT, "-C", str(root), "rev-parse", "--verify", "-q",
                                 f"{prefix}^{{commit}}"], capture_output=True, text=True)
        assert unique.stdout.strip() == shas["release"], \
            "fixture: the prefix does not resolve uniquely, so the case would prove nothing"
        bindir = Path(tmp) / "bin"
        bindir.mkdir()
        calls = Path(tmp) / "git-calls"
        (bindir / "git").write_text(f'#!/bin/sh\necho "$@" >> "{calls}"\nexec "{REAL_GIT}" "$@"\n')
        (bindir / "git").chmod(0o755)
        env = {k: v for k, v in os.environ.items() if k not in ("REQUESTED", "GITHUB_STEP_SUMMARY")}
        env.update(ENV, SHA_INPUT=prefix, PATH=f"{bindir}{os.pathsep}{os.environ.get('PATH', '')}")
        p = subprocess.run([sys.executable, str(SCRIPT)], cwd=root, env=env,
                           capture_output=True, text=True)
        outputs = dict(line.split("=", 1) for line in p.stdout.splitlines() if "=" in line)
        return p.returncode, outputs, p.stderr, calls.exists()


code, out, err, called = resolve_abbrev()
case("a 7-char abbreviation that resolves uniquely is refused before git is called",
     code == 1 and "sha" not in out and not called and "40-character" in err,
     f"exit {code}, {out}, git called: {called}, {err.strip()!r}")

code, out, err, _, _, shas = resolve("@tag_object")
case("40 hex naming a tag object is refused",
     code == 1 and "sha" not in out and "not a commit" in err,
     f"exit {code}, {out}, {err.strip()!r}")

code, out, err, _, _, shas = resolve("0" * 40)
case("40 hex naming no object is refused",
     code == 1 and "sha" not in out and "no object" in err,
     f"exit {code}, {out}, {err.strip()!r}")

for hostile in ("$(touch pwned)", "0123456789abcdef0123456789abcdef0123456;",
                "0123456789abcdef0123456789abcdef01234567\n"):
    code, out, err, _, called, shas = resolve(hostile, spy=True)
    case(f"a shell metacharacter is refused by the pattern ({hostile!r})",
         code == 1 and "sha" not in out and not called and "40-character" in err,
         f"exit {code}, {out}, git called: {called}, {err.strip()!r}")

if failures:
    for f in failures:
        print(f"FAIL {f}", file=sys.stderr)
    sys.exit(1)
print(f"mint-resolve self-test OK — {ran} case(s)")
