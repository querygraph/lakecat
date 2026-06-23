#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "release proof contract check failed: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

for file in README.md DESIGN.md STATUS.md CHANGELOG.md RELEASE.md docs/book/lakecat.md; do
  require_file "$file"
done

proof_refs="$(
  {
    rg --no-filename -o 'release-candidate proof (?:was )?refreshed from (?:clean )?head `([0-9a-f]{8,40})`' \
      README.md CHANGELOG.md docs/book/lakecat.md --replace '$1' || true
    rg --no-filename -o 'from clean head `([0-9a-f]{8,40})`\. `scripts/check-release-readiness\.sh --release-candidate` passed' \
      DESIGN.md --replace '$1' || true
    rg --no-filename -o 'from clean head `([0-9a-f]{8,40})`\. The gate covered' \
      STATUS.md --replace '$1' || true
  } | sort -u
)"

[[ -n "$proof_refs" ]] || fail "could not find active release-candidate proof refs"

proof_count="$(printf '%s\n' "$proof_refs" | sed '/^$/d' | wc -l | tr -d ' ')"
if [[ "$proof_count" != "1" ]]; then
  fail "active release-candidate proof refs disagree: $proof_refs"
fi

proof_ref="$(printf '%s\n' "$proof_refs" | sed -n '1p')"
git rev-parse -q --verify "$proof_ref^{commit}" >/dev/null || \
  fail "release-candidate proof ref is not a local commit: $proof_ref"
git merge-base --is-ancestor "$proof_ref" HEAD || \
  fail "release-candidate proof ref $proof_ref must be an ancestor of HEAD"

if [[ "$(git rev-parse "$proof_ref")" != "$(git rev-parse HEAD)" ]]; then
  while IFS= read -r changed_file; do
    [[ -n "$changed_file" ]] || continue
    case "$changed_file" in
      CHANGELOG.md|DESIGN.md|README.md|RELEASE.md|STATUS.md|docs/book/lakecat.md|docs/book/dist/*|scripts/check-release-proof-contract.sh)
        ;;
      *)
        fail "non-documentation file changed after release-candidate proof $proof_ref: $changed_file"
        ;;
    esac
  done < <(git diff --name-only "$proof_ref"..HEAD)
fi

if ! rg -q 'scripts/check-release-readiness\.sh --release-candidate' RELEASE.md; then
  fail "RELEASE.md must name the release-candidate proof command"
fi
if ! rg -q 'documentation/book artifact refresh is allowed' RELEASE.md; then
  fail "RELEASE.md must document the post-proof documentation/book artifact refresh rule"
fi

echo "LakeCat release proof contract is intact: $proof_ref"
