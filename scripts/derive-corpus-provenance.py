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

Usage:
  scripts/derive-corpus-provenance.py           # rewrite the file in place
  scripts/derive-corpus-provenance.py --check   # exit 1 if it is stale
"""

import json
import subprocess
import sys
from hashlib import sha256

FILE = "corpus/reference-results.json"
# Bookkeeping keys are provenance *about* the cases, not part of a case's data;
# including them would make every case look changed whenever they are rewritten.
PROVENANCE_KEYS = ("last_produced_on", "last_produced_in")


def run(*args: str) -> str:
    return subprocess.run(args, capture_output=True, text=True, check=False).stdout


def case_fingerprint(case: dict) -> str:
    payload = {k: v for k, v in case.items() if k not in PROVENANCE_KEYS}
    return sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()


def workspace_version_at(sha: str) -> str:
    for line in run("git", "show", f"{sha}:Cargo.toml").splitlines():
        if line.startswith("version"):
            return line.split('"')[1]
    return "unknown"


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


def main() -> int:
    check = "--check" in sys.argv
    with open(FILE, encoding="utf-8") as fh:
        doc = json.load(fh)

    provenance = derive()
    missing = [c for c in doc["cases"] if c not in provenance]
    if missing:
        print(f"no history found for: {', '.join(sorted(missing))}", file=sys.stderr)
        return 1

    stale = []
    for name, case in doc["cases"].items():
        date, version = provenance[name]
        if case.get("last_produced_on") != date or case.get("last_produced_in") != version:
            stale.append(name)
        case["last_produced_on"] = date
        case["last_produced_in"] = version

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
