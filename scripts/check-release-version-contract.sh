#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "release version contract check failed: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_file Cargo.toml
require_file CHANGELOG.md
require_file RELEASE.md
require_file docs/book/dist/VERSION.md
require_file docs/book/dist/lakecat.epub

workspace_version="$(
  awk '
    /^\[workspace\.package\]/ { in_workspace_package = 1; next }
    /^\[/ { in_workspace_package = 0 }
    in_workspace_package && /^version[[:space:]]*=/ {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' Cargo.toml
)"

[[ -n "$workspace_version" ]] || fail "could not read [workspace.package] version from Cargo.toml"

while IFS= read -r -d '' manifest; do
  if ! rg -q '^version\.workspace[[:space:]]*=[[:space:]]*true$' "$manifest"; then
    fail "$manifest must inherit version.workspace = true"
  fi
done < <(find crates -mindepth 2 -maxdepth 2 -name Cargo.toml -path 'crates/lakecat-*/*' -print0 | sort -z)

metadata_file="$(mktemp)"
trap 'rm -f "$metadata_file"' EXIT
cargo metadata --format-version 1 --no-deps > "$metadata_file"
metadata_mismatches="$(
  jq -r --arg version "$workspace_version" '
    .packages[]
    | select(.name | startswith("lakecat-"))
    | select(.version != $version)
    | "\(.name)\t\(.version)"
  ' "$metadata_file"
)"

if [[ -n "$metadata_mismatches" ]]; then
  fail "LakeCat package versions do not match $workspace_version: $metadata_mismatches"
fi

local_tag="v$workspace_version"
if git rev-parse -q --verify "refs/tags/$local_tag" >/dev/null; then
  git merge-base --is-ancestor "$local_tag^{}" HEAD || \
    fail "published release tag $local_tag must be an ancestor of HEAD"
  if rg -q "git tag -a $local_tag\\b" RELEASE.md; then
    fail "RELEASE.md must not instruct retagging already-published $local_tag"
  fi
else
  rg -q 'git tag -a "v\$version"' RELEASE.md || \
    fail "RELEASE.md must derive future unpublished tags from the workspace version"
fi
release_date="$(date -u +%F)"
rg -q "^## $workspace_version - $release_date$" CHANGELOG.md || \
  fail "CHANGELOG.md must contain release heading '$workspace_version - $release_date'"

kindle_name="$(awk -F': ' '/^kindle_name:/ { print $2; exit }' docs/book/dist/VERSION.md)"
epub_file="$(awk -F': ' '/^epub_file:/ { print $2; exit }' docs/book/dist/VERSION.md)"
kindle_link="$(awk -F': ' '/^kindle_link:/ { print $2; exit }' docs/book/dist/VERSION.md)"

expected_kindle_name="lakecat ($workspace_version)"
expected_kindle_link="$expected_kindle_name.epub"

[[ "$kindle_name" == "$expected_kindle_name" ]] || \
  fail "book kindle_name '$kindle_name' does not match expected '$expected_kindle_name'"
[[ "$epub_file" == "lakecat.epub" ]] || \
  fail "book epub_file '$epub_file' does not match lakecat.epub"
[[ "$kindle_link" == "$expected_kindle_link" ]] || \
  fail "book kindle_link '$kindle_link' does not match expected '$expected_kindle_link'"

kindle_path="docs/book/dist/$kindle_link"
[[ -L "$kindle_path" ]] || fail "versioned Kindle EPUB is not a symlink: $kindle_path"
[[ "$(readlink "$kindle_path")" == "$epub_file" ]] || \
  fail "versioned Kindle EPUB must link to $epub_file"

echo "LakeCat release version contract is intact: $workspace_version"
