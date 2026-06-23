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

release_tag="$(sed -n 's/.*git tag -a v\([0-9][0-9A-Za-z.-]*\) .*/\1/p' RELEASE.md | head -n 1)"
[[ -n "$release_tag" ]] || fail "could not read release tag command from RELEASE.md"
[[ "$release_tag" == "$workspace_version" ]] || \
  fail "RELEASE.md tag v$release_tag does not match workspace version $workspace_version"

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
