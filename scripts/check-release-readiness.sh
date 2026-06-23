#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mode="full"
skip_handoff=0
skip_book=0
require_clean=0
cleanup_dirs=()

cleanup() {
  local dir
  if [[ "${#cleanup_dirs[@]}" -eq 0 ]]; then
    return
  fi
  for dir in "${cleanup_dirs[@]}"; do
    rm -rf "$dir"
  done
}
trap cleanup EXIT

usage() {
  cat <<'USAGE'
Usage: scripts/check-release-readiness.sh [--quick] [--release-candidate] [--skip-handoff] [--skip-book]

Runs the local-first LakeCat release gate. Full mode is intentionally heavier
than a per-slice check and is meant to replace cloud CI as the release proof
while CI remains manual-only.

Options:
  --quick         Run syntax, dependency, workflow, release-version, tracked
                  book artifact, formatting, and diff checks. This is not
                  release evidence by itself.
  --release-candidate
                  Require a clean tree before and after the complete full gate.
                  This rejects skipped book or handoff proof.
  --skip-handoff Skip scripts/qglake-handoff-local.sh in full mode and report
                  partial evidence instead of release-candidate success.
  --skip-book    Skip docs/book/build.sh in full mode and report partial
                  evidence instead of release-candidate success.
USAGE
}

for arg in "$@"; do
  case "$arg" in
    --quick)
      mode="quick"
      skip_handoff=1
      skip_book=1
      ;;
    --release-candidate)
      require_clean=1
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

require_clean_tree() {
  local phase="$1"
  if [[ -n "$(git status --short)" ]]; then
    echo "release candidate gate requires a clean tree $phase" >&2
    git status --short >&2
    exit 1
  fi
}

if [[ "$require_clean" -ne 0 ]]; then
  if [[ "$mode" != "full" || "$skip_book" -ne 0 || "$skip_handoff" -ne 0 ]]; then
    echo "--release-candidate requires the complete full gate without skipped evidence" >&2
    exit 2
  fi
  require_clean_tree "before running checks"
fi

run bash -n scripts/check-local-dependency-contract.sh
run bash -n scripts/check-workflow-trigger-contract.sh
run bash -n scripts/qglake-handoff-local.sh
run bash -n scripts/check-book-artifact-contract.sh
run bash -n scripts/check-release-version-contract.sh
run bash -n scripts/check-release-readiness.sh
run scripts/check-local-dependency-contract.sh
run scripts/check-workflow-trigger-contract.sh
run scripts/check-release-version-contract.sh
run scripts/check-book-artifact-contract.sh docs/book/dist

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
  run cargo test -p lakecat-api --lib -- --test-threads=1
  run cargo test -p lakecat-sail --features sail-local --lib \
    encodes_null_and_nested_partition_literals_for_iceberg_rest -- --test-threads=1
  run cargo test -p lakecat-cli --features qglake-fixture qglake_fixture -- --test-threads=1
  run cargo test -p lakecat-cli qglake_handoff -- --test-threads=1
  run cargo test -p lakecat-store --lib --no-default-features -- --test-threads=1
  run cargo test -p lakecat-store --features turso-local --lib -- --test-threads=1
  run cargo test -p lakecat-service --features turso-local --lib -- --test-threads=1
  run cargo test -p lakecat-service --features sail-local --lib -- --test-threads=1
  run cargo test -p lakecat-service --features typesec-local --lib -- --test-threads=1
  run cargo test -p lakecat-service --features grust-local --lib \
    outbox_drain_projects_table_events_to_sinks -- --test-threads=1
  run cargo test -p lakecat-service --features grust-turso-local --bin lakecat-service \
    configured_grust_turso_graph_sink_projects_catalog_events_to_turso_store -- --test-threads=1
  run cargo test -p lakecat-security --features typesec-local --lib -- --test-threads=1
  run cargo test -p lakecat-graph --features grust-local --lib -- --test-threads=1
  run cargo test -p lakecat-graph --features grust-local --lib \
    grust_cypher_can_query_lakecat_catalog_projection_boundary -- --test-threads=1
  run cargo test -p lakecat-graph --features grust-turso-local --lib \
    grust_turso_store -- --test-threads=1
  run cargo test -p lakecat-graph --features grust-turso-local --lib \
    grust_turso_store_runs_cypher_over_lakecat_catalog_projection_boundary -- --test-threads=1
  run cargo test -p lakecat-graph --features grust-turso-local --lib \
    grust_turso_store_patches_lakecat_catalog_projection_nodes -- --test-threads=1
  run cargo test -p lakecat-cli --all-features -- --test-threads=1
  run cargo test --workspace --all-features

  if [[ "$skip_book" -eq 0 ]]; then
    if [[ "$require_clean" -ne 0 ]]; then
      book_tmpdir="$(mktemp -d)"
      cleanup_dirs+=("$book_tmpdir")
      run env LAKECAT_BOOK_DIST_DIR="$book_tmpdir/book-dist" docs/book/build.sh
      run scripts/check-book-artifact-contract.sh "$book_tmpdir/book-dist"
    else
      run docs/book/build.sh
      run scripts/check-book-artifact-contract.sh docs/book/dist
    fi
  fi
  if [[ "$skip_handoff" -eq 0 ]]; then
    run scripts/qglake-handoff-local.sh
  fi
fi

run git diff --check
if [[ "$require_clean" -ne 0 ]]; then
  require_clean_tree "after running checks"
fi

echo
if [[ "$mode" == "quick" ]]; then
  echo "LakeCat quick release-readiness checks passed."
elif [[ "$skip_book" -ne 0 || "$skip_handoff" -ne 0 ]]; then
  echo "LakeCat partial release-readiness checks passed with skipped release-candidate evidence."
  if [[ "$skip_book" -ne 0 ]]; then
    echo "Skipped book artifact validation."
  fi
  if [[ "$skip_handoff" -ne 0 ]]; then
    echo "Skipped QGLake handoff proof."
  fi
else
  if [[ "$require_clean" -ne 0 ]]; then
    echo "LakeCat release-candidate checks passed from a clean tree."
  else
    echo "LakeCat full release-readiness checks passed."
  fi
fi
