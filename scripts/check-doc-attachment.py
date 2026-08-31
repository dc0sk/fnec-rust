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
    pending_test = False
    depth = 0
    for i, line in enumerate(lines):
        stripped = line.strip()
        # Test modules are excluded: their helpers are deliberately terse, and
        # including them would make the doc-regression half noisy enough to ignore.
        #
        # EXCLUDED, not "everything after the first one". `in_test` used to be set
        # here and never cleared, so the scan went blind at the first test module
        # and stayed blind: production items below it were skipped, and in files
        # that OPEN with a test module -- nec_worker's protocol.rs, capability.rs
        # and solve.rs -- every production item was. 49 items, and the "1406 items,
        # zero detached" that FND-061 reported was inflated by exactly that
        # (FND-089).
        #
        # Now the module's braces are tracked so the scan resumes after it. Brace
        # counting is textual and a brace inside a string literal would confuse
        # it; that is the same assumption the rest of this script already makes,
        # and it fails toward scanning MORE rather than less.
        if stripped.startswith("#[cfg(test)]"):
            pending_test = True
        if pending_test and "{" in line:
            in_test = True
            pending_test = False
            depth = line.count("{") - line.count("}")
            continue
        if in_test:
            depth += line.count("{") - line.count("}")
            if depth <= 0:
                in_test = False
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
        # A LIST per name, not one entry. Keying by name alone meant a repeated
        # name -- 13 of them across 7 files -- kept only its last occurrence, so
        # the others were neither checked for detachment nor counted, and the
        # base diff silently skipped them (FND-089).
        out.setdefault(name, []).append(
            {"line": i + 1, "documented": attached, "detached": detached}
        )
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
    ambiguous = 0
    for path in rust_files("."):
        text = path.read_text()
        items = documented_items(text)
        checked += sum(len(v) for v in items.values())
        for name, occurrences in items.items():
            for info in occurrences:
                if info["detached"]:
                    failures.append(
                        f"{path}:{info['line']}: doc comment is separated from `{name}` "
                        f"by a blank line, so it documents nothing"
                    )
        if base:
            old = file_at(base, str(path))
            if old is None:
                continue
            for name, old_occ in documented_items(old).items():
                now_occ = items.get(name)
                # Only a name that is unique on BOTH sides can be diffed by name.
                # An ambiguous one is counted and reported rather than quietly
                # skipped, so the gate's coverage claim stays honest.
                if not now_occ:
                    continue
                if len(old_occ) != 1 or len(now_occ) != 1:
                    ambiguous += 1
                    continue
                was, now = old_occ[0], now_occ[0]
                if was["documented"] and not now["documented"]:
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
    note = (
        f"; {ambiguous} repeated name(s) not diffable by name" if ambiguous else ""
    )
    print(f"doc attachment OK — {checked} item(s) checked{scope}{note}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
