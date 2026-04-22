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

base_version="$(git show "${base_ref}:Cargo.toml" | sed -n 's/^version = "\([^"]*\)"$/\1/p' | head -n1)"
head_version="$(git show "${head_ref}:Cargo.toml" | sed -n 's/^version = "\([^"]*\)"$/\1/p' | head -n1)"

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

exit "$missing"
