#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <base-ref> <head-ref>" >&2
  exit 1
fi

base_ref="$1"
head_ref="$2"

if ! git cat-file -e "${base_ref}:Cargo.toml" 2>/dev/null; then
  echo "Base ref does not contain Cargo.toml; skipping version bump check."
  exit 0
fi

if ! git cat-file -e "${head_ref}:Cargo.toml" 2>/dev/null; then
  echo "Head ref does not contain Cargo.toml; skipping version bump check."
  exit 0
fi

# Parsed, not grepped: the first-match `^version =` extraction worked only
# because the manifest has one such line (FND-068).
here="$(cd "$(dirname "$0")" && pwd)"
base_version="$(python3 "$here/cargo_version.py" "$base_ref" || true)"
head_version="$(python3 "$here/cargo_version.py" "$head_ref" || true)"

if [[ -z "$base_version" || -z "$head_version" ]]; then
  echo "Unable to determine workspace version from Cargo.toml." >&2
  exit 1
fi

if [[ "$base_version" == "$head_version" ]]; then
  echo "No version bump detected."
  exit 0
fi

mapfile -t changed_files < <(git diff --name-only "$base_ref" "$head_ref")
required_files=(
  "docs/changelog.md"
  "docs/releasenotes.md"
  "SBOM.spdx.json"
)

missing=0
for file in "${required_files[@]}"; do
  if ! printf '%s
' "${changed_files[@]}" | grep -Fxq "$file"; then
    echo "Version bump from $base_version to $head_version requires updating $file"
    missing=1
  fi
done

# The changelog's compare links: a release adds its own and moves [Unreleased] on.
# Nothing checked them, and they stopped at v0.19.0 for eleven releases.
changelog="$(git show "${head_ref}:docs/changelog.md" 2>/dev/null || true)"
repo="https://github.com/dc0sk/fnec-rust"
for line in \
  "[Unreleased]: ${repo}/compare/v${head_version}...HEAD" \
  "[${head_version}]: ${repo}/compare/v${base_version}...v${head_version}"; do
  if ! printf '%s\n' "$changelog" | grep -Fxq -- "$line"; then
    echo "Version bump to $head_version requires this compare link in docs/changelog.md: $line"
    missing=1
  fi
done

exit "$missing"
