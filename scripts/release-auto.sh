#!/usr/bin/env bash
set -euo pipefail

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command '$1' was not found" >&2
    exit 1
  fi
}

current_branch() {
  git branch --show-current
}

ensure_clean_tree() {
  if [ -n "$(git status --short)" ]; then
    echo "error: working tree must be clean before release" >&2
    git status --short >&2
    exit 1
  fi
}

next_version() {
  local output

  if ! output="$(cog bump --dry-run --auto 2>&1)"; then
    if printf '%s\n' "$output" | grep -qiE 'no commits|no conventional|no bump|nothing to bump'; then
      echo "No conventional commits require a release."
      return 1
    fi

    printf '%s\n' "$output" >&2
    exit 1
  fi

  printf '%s\n' "$output" | tail -n 1 | sed 's/^v//'
}

ensure_tag_is_available() {
  local tag="$1"
  local status

  if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    echo "error: local tag ${tag} already exists" >&2
    exit 1
  fi

  set +e
  git ls-remote --exit-code --tags origin "refs/tags/${tag}" >/dev/null 2>&1
  status=$?
  set -e

  if [ "$status" -eq 0 ]; then
    echo "error: remote tag ${tag} already exists" >&2
    exit 1
  fi

  if [ "$status" -ne 2 ]; then
    echo "error: could not check remote tag ${tag}" >&2
    exit 1
  fi
}

changelog_header() {
  cat <<'EOF'
# Changelog

All notable changes to this project will be documented in this file.

See [Conventional Commits](https://www.conventionalcommits.org/) for commit guidelines.

EOF
}

previous_changelog_sections() {
  if [ ! -f CHANGELOG.md ]; then
    return
  fi

  awk 'found || /^## / { found = 1; print }' CHANGELOG.md
}

write_changelog_entry() {
  local version="$1"
  local tag="$2"
  local latest_tag="$3"
  local today
  local body
  local existing
  local next

  today="$(date -u +%F)"
  body="$(mktemp)"
  existing="$(mktemp)"
  next="$(mktemp)"

  if [ -n "$latest_tag" ]; then
    cog changelog "${latest_tag}.." >"$body"
  else
    cog changelog >"$body"
  fi

  # Range changelogs are expected to be body-only, but strip a leading heading
  # if a future Cocogitto version emits one for HEAD ranges.
  awk 'NR == 1 && /^## / { next } { print }' "$body" >"${body}.clean"
  mv "${body}.clean" "$body"

  previous_changelog_sections >"$existing"

  changelog_header >"$next"
  if [ -n "$latest_tag" ]; then
    printf '## [%s](https://github.com/TudorAndrei/barrs/compare/%s...%s) - %s\n\n' \
      "$version" "$latest_tag" "$tag" "$today" >>"$next"
  else
    printf '## [%s](https://github.com/TudorAndrei/barrs/releases/tag/%s) - %s\n\n' \
      "$version" "$tag" "$today" >>"$next"
  fi
  cat "$body" >>"$next"

  if [ -s "$existing" ]; then
    printf '\n' >>"$next"
    cat "$existing" >>"$next"
  fi

  mv "$next" CHANGELOG.md
}

trigger_release_workflow() {
  local tag="$1"

  if [ "${GITHUB_ACTIONS:-}" != "true" ]; then
    return
  fi

  require_command gh
  gh workflow run release.yml --ref "$tag"
}

require_command cargo
require_command cog
require_command git

branch="$(current_branch)"
if [ "$branch" != "main" ]; then
  echo "error: releases must be cut from main, got '${branch:-detached HEAD}'" >&2
  exit 1
fi

ensure_clean_tree

version="$(next_version)" || exit 0
if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$'; then
  echo "error: Cocogitto returned an invalid version: $version" >&2
  exit 1
fi

tag="v${version}"
latest_tag="$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)"

ensure_tag_is_available "$tag"

cargo release version "$version" --execute --no-confirm
write_changelog_entry "$version" "$tag" "$latest_tag"

git add CHANGELOG.md Cargo.toml Cargo.lock

cargo test
cargo run -- --version | grep -Fx "barrs ${version}"

cargo release commit --execute --no-confirm
cargo release tag --execute --no-confirm
cargo release push --execute --no-confirm
trigger_release_workflow "$tag"
