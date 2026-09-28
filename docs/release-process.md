---
project: fnec-rust
doc: docs/release-process.md
status: living
last_updated: 2026-09-28
---

# Cutting a release

The steps from a merged release PR to a published GitHub release. The version
bump, changelog, release notes and SBOM are the release PR's job, and
`check-version-bump-docs.sh` holds it to them. This page covers what comes after.

## 1. Everything the tag will carry is in the notes

The mint workflow tags **main's tip**, not the release PR's merge commit. Anything
merged after the release PR ships in the tag, so its changelog entry belongs under
the release's heading, not `[Unreleased]`. Move it with a docs PR before minting.
(0.19.0: #482 folded #481 in after the release PR #480.) Any number the notes
quote as "measured at the release commit" is re-measured at the tip.

## 2. Mint the tag

Actions → **Mint release tag** → *Run workflow*, version left empty. Or:
`gh workflow run release-tag.yml`. The workflow refuses to re-tag. It checks the
changelog and release-notes sections, the binding version and the SBOM, and it
requires green CI at the commit it tags. Never tag by hand.

## 3. Build the assets in the container

```sh
git checkout main && git pull --ff-only       # HEAD on the new tag, clean tree
scripts/build-release-assets.sh
```

This builds the CLI, the GUI and the Python wheel in one digest-pinned manylinux2014
container, with the toolchain from `rust-toolchain.toml`, and copies the SBOM. It
then:

- smoke-solves the corpus reference dipole with the built CLI against its pin;
- checks that the SBOM names the version;
- runs `scripts/check-asset-platform.py`, which refuses any asset requiring a
  newer glibc than the floor in `docs/project/release-asset-platform.toml`.

It takes the host-wide build lock itself, so don't wrap it in `flock`. It refuses
to write inside the worktree or on a tmpfs. Assets land in
`~/.cache/fnec-release-v<version>`, and the cargo cache in `~/.cache/fnec-release-build`.
`--dry-run` builds and checks without the tag, for trying the pipeline first.

**Why a container** (FND-168): a native artifact inherits the build host's glibc.
On a rolling-release host, 0.19.0's new `sinh`/`cosh` calls bound GLIBC_2.44. The
host-built wheel would have stopped loading on every older distro, and maturin
only warned. Never build a release asset on the host. Raising the glibc floor
is a decision: edit the baseline file in a commit and say so in the release
notes.

The GUI links only core system libraries and loads Wayland, X11 and Vulkan at
runtime. Opening its window is a manual check on a machine with a display.

## 4. Publish

```sh
V=0.19.0; A=~/.cache/fnec-release-v$V
python3 - <<'PY' > $A.notes.md
import re, pathlib
t = pathlib.Path("docs/releasenotes.md").read_text()
m = re.search(r"^## 0\.19\.0[^\n]*\n(.*?)(?=^## \d|\Z)", t, re.M | re.S)
print(m.group(1).strip())
PY
gh release create v$V --verify-tag --title "v$V — <release title>" --notes-file $A.notes.md $A/*
```

Then check the release page: it should be marked Latest, with four assets.

## 5. Clean up

Delete `~/.cache/fnec-release-v<version>`, its notes file, and
`~/.cache/fnec-release-build`. The cargo target is about 1.3 GB. Keep the
container image for the next release; `podman rmi` it when done with releases
for a while.
