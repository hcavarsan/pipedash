#!/usr/bin/env bash
set -euo pipefail

output="${1:?usage: release_notes.sh <output file>}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}" "${GITHUB_REPO:?GITHUB_REPO is required}"
export GH_TOKEN="$GITHUB_TOKEN"

tag="$(git describe --tags --abbrev=0 --match 'v[0-9]*')"
previous="$(git describe --tags --abbrev=0 --match 'v[0-9]*' "$tag^" 2>/dev/null || true)"
range="${previous:+$previous..}$tag"

skip=()
while IFS= read -r sha; do
  [ -n "$sha" ] && skip+=(--skip-commit "$sha")
done < <(comm -13 <(git rev-list --first-parent "$range" | sort) <(git rev-list "$range" | sort))

git-cliff --config cliff.toml ${skip[@]+"${skip[@]}"} --output "$output" "$range"

while IFS= read -r number; do
  author="$(gh api "repos/$GITHUB_REPO/pulls/$number" --jq '.user.login' 2>/dev/null || true)"
  if [ -n "$author" ]; then
    sed -E "s|\((@[^ ]+ )?(\[#$number\])|(@$author \2|" "$output" > "$output.tmp"
    mv "$output.tmp" "$output"
  fi
done < <(grep -oE '\[#[0-9]+\]' "$output" | tr -d '[]#' | sort -u)
