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
expected_release_date="$(date -u +%F)"
if git rev-parse -q --verify "refs/tags/$local_tag" >/dev/null; then
  git merge-base --is-ancestor "$local_tag^{}" HEAD || \
    fail "published release tag $local_tag must be an ancestor of HEAD"
  if rg -q "git tag -a $local_tag\\b" RELEASE.md; then
    fail "RELEASE.md must not instruct retagging already-published $local_tag"
  fi
  rg -q "For the already-published \`$local_tag\` baseline" RELEASE.md || \
    fail "RELEASE.md must identify already-published $local_tag as the fixed baseline"
  rg -q 'For a future version-bump release, before tagging:' RELEASE.md || \
    fail "RELEASE.md must scope tagging chores to future version-bump releases"
  if rg -q '^Before tagging:$' RELEASE.md; then
    fail "RELEASE.md must not present a standalone pre-tagging section for already-published $local_tag"
  fi
  rg -q "For the already-published \`$local_tag\`, do not retag" DESIGN.md || \
    fail "DESIGN.md must say not to retag already-published $local_tag"
  rg -q "The already-published \`$local_tag\` tag is a baseline, not something to move" docs/book/lakecat.md || \
    fail "LakeCat book must say already-published $local_tag is a fixed baseline"
  expected_release_date="$(
    git for-each-ref "refs/tags/$local_tag" --format='%(creatordate:short)'
  )"
  [[ -n "$expected_release_date" ]] || \
    fail "could not derive release date from published tag $local_tag"
  tag_commit="$(git rev-parse "$local_tag^{}")"
  head_commit="$(git rev-parse HEAD)"
  if [[ "$tag_commit" != "$head_commit" ]]; then
    rg -q '^## Unreleased$' CHANGELOG.md || \
      fail "post-$local_tag hardening must remain under CHANGELOG.md Unreleased until the workspace version moves forward"
  fi
else
  rg -q 'git tag -a "v\$version"' RELEASE.md || \
    fail "RELEASE.md must derive future unpublished tags from the workspace version"
fi
rg -q "^## $workspace_version - $expected_release_date$" CHANGELOG.md || \
  fail "CHANGELOG.md must contain release heading '$workspace_version - $expected_release_date'"

kindle_name="$(awk -F': ' '/^kindle_name:/ { print $2; exit }' docs/book/dist/VERSION.md)"
epub_file="$(awk -F': ' '/^epub_file:/ { print $2; exit }' docs/book/dist/VERSION.md)"
kindle_link="$(awk -F': ' '/^kindle_link:/ { print $2; exit }' docs/book/dist/VERSION.md)"

# The linked, versioned artifact name carries a short git-hash suffix:
#   lakecat (<workspace_version>-<short_hash>)
kindle_name_re="^lakecat \($workspace_version-[0-9a-f]{7,}\)$"
kindle_link_re="^lakecat \($workspace_version-[0-9a-f]{7,}\)\.epub$"

[[ "$kindle_name" =~ $kindle_name_re ]] || \
  fail "book kindle_name '$kindle_name' does not match expected 'lakecat ($workspace_version-<hash>)'"
[[ "$epub_file" == "lakecat.epub" ]] || \
  fail "book epub_file '$epub_file' does not match lakecat.epub"
[[ "$kindle_link" =~ $kindle_link_re ]] || \
  fail "book kindle_link '$kindle_link' does not match expected 'lakecat ($workspace_version-<hash>).epub'"

kindle_path="docs/book/dist/$kindle_link"
[[ -L "$kindle_path" ]] || fail "versioned Kindle EPUB is not a symlink: $kindle_path"
[[ "$(readlink "$kindle_path")" == "$epub_file" ]] || \
  fail "versioned Kindle EPUB must link to $epub_file"

echo "LakeCat release version contract is intact: $workspace_version"
