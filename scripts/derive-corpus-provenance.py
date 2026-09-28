#!/usr/bin/env python3
"""Derive per-case provenance for `corpus/reference-results.json` from git history.

The file carries one `reference_engine_version` for all 48 cases, which cannot be
true: cases were added and regenerated across many releases. Rather than invent a
per-case version — which would be fabricated provenance, worse than none — this
replays every commit that touched the file, hashes each case's own subtree, and
records the commit where that subtree *last changed*.

That answers the question provenance is for: "when were these stored numbers last
produced, and by which build of fnec". It is derived from the repository, so it can
be re-derived and checked rather than trusted.

Known limit, stated rather than hidden: the workspace version is the version *at
that commit*, which is the release under development, not necessarily a released
build. Cases whose values never changed since they were added report the commit
that added them.

The `--check` compares the VERSION, not the date (FND-160). A squash-merge
replaces the branch commit that last changed a case with a new commit on `main`,
dated the day of the merge: the data is identical, only the date moves. Checking
the date turned `main` red for every re-pin merged on a later day than its
branch commit, and a PR branch can never stamp the date its own merge will get.
The version survives the squash (the merge commit carries the branch's
`Cargo.toml`), so it is what the check can hold. The date is still written, and
a case with no date at all still fails the check.

The version alone cannot see a re-pin made within the release under development
(FND-177): 39 of 51 cases were stamped 0.19.0 while `main` declared 0.19.0, and a
case whose stored numbers changed kept its old stamp and passed. So each case
also carries `last_produced_fingerprint`, the hash of its own data, and the check
fails when the data no longer matches it. It is derived from the data alone, so
a squash cannot move it.

Usage:
  scripts/derive-corpus-provenance.py           # rewrite the file in place
  scripts/derive-corpus-provenance.py --check   # exit 1 if it is stale
"""

import json
import subprocess
import sys
from hashlib import sha256
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cargo_version import version_at  # noqa: E402  (one parser, FND-067)

FILE = "corpus/reference-results.json"
# Bookkeeping keys are provenance *about* the cases, not part of a case's data;
# including them would make every case look changed whenever they are rewritten.
PROVENANCE_KEYS = ("last_produced_on", "last_produced_in", "last_produced_fingerprint")


def run(*args: str) -> str:
    return subprocess.run(args, capture_output=True, text=True, check=False).stdout


def case_fingerprint(case: dict) -> str:
    payload = {k: v for k, v in case.items() if k not in PROVENANCE_KEYS}
    return sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()


def workspace_version_at(sha: str) -> str:
    return version_at(sha) or "unknown"


def derive() -> dict[str, tuple[str, str]]:
    """case -> (iso date, workspace version) of the commit that last changed it."""
    log = run("git", "log", "--reverse", "--format=%H %ad", "--date=short", "--", FILE)
    commits = [ln.split(" ", 1) for ln in log.splitlines() if ln.strip()]

    seen: dict[str, str] = {}          # case -> fingerprint as of the last commit
    provenance: dict[str, tuple[str, str]] = {}
    version_cache: dict[str, str] = {}

    for sha, date in commits:
        blob = run("git", "show", f"{sha}:{FILE}")
        try:
            cases = json.loads(blob).get("cases", {})
        except json.JSONDecodeError:
            continue  # a commit where the file was mid-rewrite; skip it
        for name, case in cases.items():
            if not isinstance(case, dict):
                continue
            fp = case_fingerprint(case)
            if seen.get(name) != fp:
                if sha not in version_cache:
                    version_cache[sha] = workspace_version_at(sha)
                provenance[name] = (date, version_cache[sha])
                seen[name] = fp
    return provenance


def stale_cases(cases: dict, provenance: dict[str, tuple[str, str]]) -> list[str]:
    """The cases whose stamp no longer describes them.

    Stale when the date is missing, the version is not the one that last changed
    the case, or the case's data no longer matches the fingerprint stamped with
    it (FND-177). The date's value is informational: a squash-merge moves it
    without changing the data, so only its presence is checked.
    """
    stale = []
    for name, case in cases.items():
        _date, version = provenance[name]
        if (
            not case.get("last_produced_on")
            or case.get("last_produced_in") != version
            or case.get("last_produced_fingerprint") != case_fingerprint(case)
        ):
            stale.append(name)
    return stale


def main() -> int:
    check = "--check" in sys.argv
    with open(FILE, encoding="utf-8") as fh:
        doc = json.load(fh)

    provenance = derive()
    missing = [c for c in doc["cases"] if c not in provenance]
    if missing:
        print(f"no history found for: {', '.join(sorted(missing))}", file=sys.stderr)
        return 1

    stale = stale_cases(doc["cases"], provenance)
    for name, case in doc["cases"].items():
        date, version = provenance[name]
        case["last_produced_on"] = date
        case["last_produced_in"] = version
        case["last_produced_fingerprint"] = case_fingerprint(case)

    if check:
        if stale:
            print(
                f"{FILE} per-case provenance is stale for {len(stale)} case(s): "
                f"{', '.join(sorted(stale)[:5])}"
                f"{' …' if len(stale) > 5 else ''}\n"
                f"run: python3 {sys.argv[0]}",
                file=sys.stderr,
            )
            return 1
        print("corpus per-case provenance is up to date")
        return 0

    with open(FILE, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, indent=2, ensure_ascii=False)
        fh.write("\n")
    print(f"stamped provenance on {len(doc['cases'])} case(s); {len(stale)} updated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
