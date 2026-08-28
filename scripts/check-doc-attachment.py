#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
"""Catch doc comments that came adrift from the item they describe.

Two shapes, both of which compile, pass clippy, and pass every test — so nothing
mechanical caught them and each was found by a human or a reviewer reading the
file:

**Detached** — a blank line between a `///` block and its item. The doc silently
stops belonging to it.

**Spliced** — new code inserted between an item's doc comment and the item. The
previous item's rationale becomes the *new* item's rustdoc, and the original is
left undocumented. This one is invisible statically: the doc is still contiguous
with *a* function, just the wrong one. Its signature is a diff property — an item
that had a doc no longer has one — so that half needs a base revision to compare
against.

This happened four times in one working session, three of them caught only by
review. A rule that is forgotten four times is not working as a rule.

Usage:
    check-doc-attachment.py            # detached only (works on any checkout)
    check-doc-attachment.py --base REF # ...plus doc regressions against REF
"""

import re
import subprocess
import sys
from pathlib import Path

ITEM = re.compile(
    r"^(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?"
    r"(?:fn\s+(?P<fn>[A-Za-z_][A-Za-z0-9_]*)"
    r"|(?:struct|enum|trait)\s+(?P<ty>[A-Za-z_][A-Za-z0-9_]*))"
)
SKIP_DIRS = ("/target/", "/.git/")


def documented_items(text):
    """Map each top-level item name to whether a doc comment is attached to it.

    "Attached" means the `///` block sits directly above the item, allowing only
    `#[attributes]` in between — which is where a `#[derive]` legitimately lives.
    A blank line breaks attachment, which is the whole point of the check.
    """
    lines = text.split("\n")
    out = {}
    in_test = False
    for i, line in enumerate(lines):
        stripped = line.strip()
        # Test modules are excluded: their helpers are deliberately terse, and
        # including them would make the doc-regression half noisy enough to ignore.
        if stripped.startswith("#[cfg(test)]"):
            in_test = True
        if in_test:
            continue
        m = ITEM.match(stripped)
        if not m:
            continue
        name = m.group("fn") or m.group("ty")
        j = i - 1
        while j >= 0 and lines[j].strip().startswith("#["):
            j -= 1
        attached = j >= 0 and lines[j].strip().startswith("///")
        detached = False
        if not attached and j >= 0 and lines[j].strip() == "":
            k = j
            while k >= 0 and lines[k].strip() == "":
                k -= 1
            detached = k >= 0 and lines[k].strip().startswith("///")
        out[name] = {"line": i + 1, "documented": attached, "detached": detached}
    return out


def rust_files(root):
    for p in sorted(Path(root).rglob("*.rs")):
        sp = str(p)
        if any(d in f"/{sp}" for d in SKIP_DIRS):
            continue
        yield p


def file_at(ref, path):
    r = subprocess.run(
        ["git", "show", f"{ref}:{path}"], capture_output=True, text=True
    )
    return r.stdout if r.returncode == 0 else None


def main():
    base = None
    if "--base" in sys.argv:
        base = sys.argv[sys.argv.index("--base") + 1]

    failures = []
    checked = 0
    for path in rust_files("."):
        text = path.read_text()
        items = documented_items(text)
        checked += len(items)
        for name, info in items.items():
            if info["detached"]:
                failures.append(
                    f"{path}:{info['line']}: doc comment is separated from `{name}` "
                    f"by a blank line, so it documents nothing"
                )
        if base:
            old = file_at(base, str(path))
            if old is None:
                continue
            for name, was in documented_items(old).items():
                now = items.get(name)
                if was["documented"] and now and not now["documented"]:
                    failures.append(
                        f"{path}:{now['line']}: `{name}` had a doc comment at {base} "
                        f"and does not now — did an insert land between them?"
                    )

    if failures:
        for f in failures:
            print(f)
        print(f"\ndoc attachment: {len(failures)} problem(s) across {checked} items")
        return 1
    scope = f" (and doc regressions vs {base})" if base else ""
    print(f"doc attachment OK — {checked} item(s) checked{scope}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
