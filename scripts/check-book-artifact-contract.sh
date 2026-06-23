#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dist_dir="${1:-docs/book/dist}"

fail() {
  echo "book artifact contract check failed: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
  [[ -s "$1" ]] || fail "required file is empty: $1"
}

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
[[ -d "$dist_dir" ]] || fail "missing book dist directory: $dist_dir"

version_file="$dist_dir/VERSION.md"
stable_epub="$dist_dir/lakecat.epub"
stable_pdf="$dist_dir/lakecat.pdf"
stable_mobi="$dist_dir/lakecat.mobi"

require_file "$version_file"
require_file "$stable_epub"
require_file "$stable_pdf"
require_file "$stable_mobi"

kindle_name="$(awk -F': ' '/^kindle_name:/ { print $2; exit }' "$version_file")"
built_at="$(awk -F': ' '/^built_at:/ { print $2; exit }' "$version_file")"
epub_file="$(awk -F': ' '/^epub_file:/ { print $2; exit }' "$version_file")"
kindle_link="$(awk -F': ' '/^kindle_link:/ { print $2; exit }' "$version_file")"

expected_kindle_name="lakecat ($workspace_version)"
expected_kindle_link="$expected_kindle_name.epub"

[[ "$kindle_name" == "$expected_kindle_name" ]] || \
  fail "kindle_name '$kindle_name' does not match expected '$expected_kindle_name'"
[[ "$epub_file" == "lakecat.epub" ]] || \
  fail "epub_file '$epub_file' does not match lakecat.epub"
[[ "$kindle_link" == "$expected_kindle_link" ]] || \
  fail "kindle_link '$kindle_link' does not match expected '$expected_kindle_link'"
[[ "$built_at" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || \
  fail "built_at '$built_at' must be an ISO date"

kindle_path="$dist_dir/$kindle_link"
[[ -L "$kindle_path" ]] || fail "versioned Kindle EPUB is not a symlink: $kindle_path"
[[ "$(readlink "$kindle_path")" == "$epub_file" ]] || \
  fail "versioned Kindle EPUB must link to $epub_file"

docs/book/check_epub_metadata.sh "$stable_epub" "$kindle_name"
docs/book/check_pdf_layout.sh "$stable_pdf"

echo "LakeCat book artifact contract is intact: $dist_dir"
