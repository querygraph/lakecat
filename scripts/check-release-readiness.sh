#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mode="full"
skip_handoff=0
skip_book=0

usage() {
  cat <<'USAGE'
Usage: scripts/check-release-readiness.sh [--quick] [--skip-handoff] [--skip-book]

Runs the local-first LakeCat release gate. Full mode is intentionally heavier
than a per-slice check and is meant to replace cloud CI as the release proof
while CI remains manual-only.

Options:
  --quick         Run script syntax, dependency contract, formatting, and diff checks.
  --skip-handoff Skip scripts/qglake-handoff-local.sh in full mode.
  --skip-book    Skip docs/book/build.sh in full mode.
USAGE
}

for arg in "$@"; do
  case "$arg" in
    --quick)
      mode="quick"
      skip_handoff=1
      skip_book=1
      ;;
    --skip-handoff)
      skip_handoff=1
      ;;
    --skip-book)
      skip_book=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

run bash -n scripts/check-local-dependency-contract.sh
run bash -n scripts/check-workflow-trigger-contract.sh
run bash -n scripts/qglake-handoff-local.sh
run bash -n scripts/check-release-readiness.sh
run scripts/check-local-dependency-contract.sh

run cargo fmt \
  -p lakecat-api \
  -p lakecat-cli \
  -p lakecat-core \
  -p lakecat-graph \
  -p lakecat-lineage \
  -p lakecat-querygraph \
  -p lakecat-sail \
  -p lakecat-security \
  -p lakecat-service \
  -p lakecat-store \
  -- --check

if [[ "$mode" == "full" ]]; then
  run cargo test --workspace
  run cargo test -p lakecat-cli --features qglake-fixture qglake_fixture -- --test-threads=1
  run cargo test -p lakecat-store --features turso-local --lib -- --test-threads=1
  run cargo test -p lakecat-service --features turso-local --lib -- --test-threads=1
  run cargo test -p lakecat-service --features sail-local --lib -- --test-threads=1
  run cargo test -p lakecat-service --features typesec-local --lib -- --test-threads=1
  run cargo test -p lakecat-security --features typesec-local --lib -- --test-threads=1
  run cargo test -p lakecat-graph --features grust-local --lib -- --test-threads=1
  run cargo test -p lakecat-graph --features grust-local --lib \
    grust_cypher_can_query_lakecat_catalog_projection_boundary -- --test-threads=1
  run cargo test -p lakecat-cli --all-features -- --test-threads=1
  run cargo test --workspace --all-features --lib -- --test-threads=1

  if [[ "$skip_book" -eq 0 ]]; then
    run docs/book/build.sh
  fi
  if [[ "$skip_handoff" -eq 0 ]]; then
    run scripts/qglake-handoff-local.sh
  fi
fi

run git diff --check

echo
if [[ "$mode" == "quick" ]]; then
  echo "LakeCat quick release-readiness checks passed."
else
  echo "LakeCat full release-readiness checks passed."
fi
