#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
dist_dir="${LAKECAT_BOOK_DIST_DIR:-docs/book/dist}"
publishing_root="${FIRSTPAIR_PUBLISHING_ROOT:-$HOME/src/firstpair/publishing}"
builder="$publishing_root/scripts/build-library-book.sh"

if [[ ! -x "$builder" ]]; then
  echo "missing FirstPair book builder: $builder" >&2
  exit 1
fi

# Preserve LakeCat's release-gate guarantees at the repository boundary even
# though the shared builder also isolates its own per-edition conversion state.
wrapper_tmpdir="$(mktemp -d)"
trap 'rm -rf "$wrapper_tmpdir"' EXIT
export CALIBRE_CONFIG_DIRECTORY="${CALIBRE_CONFIG_DIRECTORY:-$wrapper_tmpdir/calibre-config}"
mkdir -p "$CALIBRE_CONFIG_DIRECTORY"

"$builder" \
  --repo-root "$repo_root" \
  --dist "$dist_dir" \
  "$@"
