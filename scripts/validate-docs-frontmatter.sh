#!/usr/bin/env bash
set -euo pipefail

shopt -s nullglob

status=0
files=(docs/*.md)

if [[ ${#files[@]} -eq 0 ]]; then
  echo "No docs markdown files found under docs/."
  exit 1
fi

validate_file() {
  local file="$1"
  local line_no=0
  local opening_found=0
  local closing_found=0
  local -a frontmatter_lines=()
  local line

  while IFS= read -r line; do
    ((line_no += 1))

    if [[ ${line_no} -eq 1 ]]; then
      if [[ "${line}" != '---' ]]; then
        echo "${file}: missing opening frontmatter delimiter on line 1"
        return 1
      fi
      opening_found=1
      continue
    fi

    if [[ "${line}" == '---' ]]; then
      closing_found=1
      break
    fi

    frontmatter_lines+=("${line}")
  done < "${file}"

  if [[ ${opening_found} -ne 1 || ${closing_found} -ne 1 ]]; then
    echo "${file}: incomplete frontmatter block"
    return 1
  fi

  if [[ ${#frontmatter_lines[@]} -eq 0 ]]; then
    echo "${file}: frontmatter cannot be empty"
    return 1
  fi

  local expected_keys=(project doc status last_updated)
  local expected_values=("fnec-rust" "${file}" "living" "")
  local -A seen=()
  local key
  local value
  local idx=0

  for line in "${frontmatter_lines[@]}"; do
    if [[ ! "${line}" =~ ^([a-z_]+):[[:space:]]+(.+)$ ]]; then
      echo "${file}: invalid frontmatter entry '${line}'"
      return 1
    fi

    key="${BASH_REMATCH[1]}"
    value="${BASH_REMATCH[2]}"

    if [[ -n "${seen[${key}]:-}" ]]; then
      echo "${file}: duplicate frontmatter key '${key}'"
      return 1
    fi
    seen["${key}"]=1

    if [[ ${idx} -ge ${#expected_keys[@]} ]]; then
      echo "${file}: unexpected frontmatter key '${key}'"
      return 1
    fi

    if [[ "${key}" != "${expected_keys[${idx}]}" ]]; then
      echo "${file}: frontmatter key order must be project, doc, status, last_updated"
      return 1
    fi

    if [[ "${key}" != "last_updated" ]]; then
      if [[ "${value}" != "${expected_values[${idx}]}" ]]; then
        echo "${file}: ${key} must be '${expected_values[${idx}]}'"
        return 1
      fi
    else
      if [[ ! "${value}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
        echo "${file}: last_updated must use YYYY-MM-DD"
        return 1
      fi
      if ! date -u -d "${value}" +%F >/dev/null 2>&1; then
        echo "${file}: last_updated must be a valid UTC calendar date"
        return 1
      fi
    fi

    ((idx += 1))
  done

  if [[ ${idx} -ne ${#expected_keys[@]} ]]; then
    echo "${file}: frontmatter must contain exactly 4 keys"
    return 1
  fi

  return 0
}

for file in "${files[@]}"; do
  if ! validate_file "${file}"; then
    status=1
  fi
done

exit ${status}