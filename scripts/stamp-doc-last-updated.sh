#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage:
  stamp-doc-last-updated.sh <base-ref> <head-ref>
  stamp-doc-last-updated.sh --from-git-diff <base-ref> <head-ref>

The positional form is retained for compatibility.
EOF
}

mode=""
base_ref=""
head_ref=""

if [[ $# -eq 2 ]]; then
  mode="from-git-diff"
  base_ref="$1"
  head_ref="$2"
elif [[ $# -eq 3 && "$1" == "--from-git-diff" ]]; then
  mode="from-git-diff"
  base_ref="$2"
  head_ref="$3"
elif [[ $# -eq 1 && ( "$1" == "-h" || "$1" == "--help" ) ]]; then
  usage
  exit 0
else
  usage
  exit 1
fi

if [[ "$mode" != "from-git-diff" ]]; then
  echo "Unsupported mode: ${mode}" >&2
  exit 1
fi

today="$(date -u +%F)"

mapfile -t changed_docs < <(git diff --name-only "$base_ref" "$head_ref" -- 'docs/*.md')

if [[ ${#changed_docs[@]} -eq 0 ]]; then
  echo "No changed docs to stamp."
  exit 0
fi

for file in "${changed_docs[@]}"; do
  tmp_file="$(mktemp)"
  awk -v today="$today" '
    /^last_updated:[[:space:]][0-9]{4}-[0-9]{2}-[0-9]{2}$/ {
      print "last_updated: " today
      next
    }
    { print }
  ' "$file" > "$tmp_file"
  mv "$tmp_file" "$file"
  echo "Stamped $file"
done
