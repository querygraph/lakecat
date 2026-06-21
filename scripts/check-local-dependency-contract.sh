#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "dependency contract check failed: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_dir() {
  [[ -d "$1" ]] || fail "missing required directory: $1"
}

require_pattern() {
  local pattern="$1"
  local file="$2"
  local description="$3"
  rg -q "$pattern" "$file" || fail "$description"
}

require_manual_only_workflows() {
  local workflow
  local workflow_files=()
  local workflow_dir="${LAKECAT_WORKFLOW_DIR:-.github/workflows}"
  local automatic_events="push|pull_request|pull_request_target|merge_group|repository_dispatch|schedule|workflow_run|workflow_call"
  local quoted_event_key="[\"']?(${automatic_events})[\"']?"

  while IFS= read -r -d '' workflow; do
    workflow_files+=("$workflow")
  done < <(find "$workflow_dir" -maxdepth 1 -type f \( -name '*.yml' -o -name '*.yaml' \) -print0 | sort -z)

  [[ "${#workflow_files[@]}" -gt 0 ]] || fail "missing GitHub workflow files"

  for workflow in "${workflow_files[@]}"; do
    if rg -q "^[[:space:]]*[\"']?on[\"']?[[:space:]]*:[[:space:]]*${quoted_event_key}([[:space:]#]|$)" "$workflow"; then
      fail "$workflow must not use compact automatic GitHub event syntax until the local gates are proven stable"
    fi
    if rg -q "^[[:space:]]*[\"']?on[\"']?[[:space:]]*:[[:space:]]*\\[[^]]*${quoted_event_key}" "$workflow"; then
      fail "$workflow must not use inline automatic GitHub event lists until the local gates are proven stable"
    fi
    if rg -q "^[[:space:]]*[\"']?on[\"']?[[:space:]]*:[[:space:]]*\\{[^}]*${quoted_event_key}[[:space:]]*:" "$workflow"; then
      fail "$workflow must not use inline automatic GitHub event maps until the local gates are proven stable"
    fi
    if awk -v events="$automatic_events" '
      BEGIN {
        split(events, event_names, "|")
        for (event_index in event_names) {
          automatic[event_names[event_index]] = 1
        }
      }
      /^[^[:space:]#][^:]*:/ {
        key = $0
        sub(/:.*/, "", key)
        gsub(/^[[:space:]"'\''"]+|[[:space:]"'\''"]+$/, "", key)
        in_on_block = key == "on"
        next
      }
      in_on_block {
        line = $0
        sub(/[[:space:]]*#.*/, "", line)
        if (line ~ /^[[:space:]]*-[[:space:]]*/) {
          item = line
          sub(/^[[:space:]]*-[[:space:]]*/, "", item)
          sub(/[[:space:]:].*/, "", item)
          gsub(/^[[:space:]"'\''"]+|[[:space:]"'\''"]+$/, "", item)
          if (item in automatic) {
            exit 42
          }
        } else if (line ~ /^[[:space:]]+["'\''"]?[^[:space:]"'\''":]+["'\''"]?[[:space:]]*:/) {
          item = line
          sub(/^[[:space:]]+/, "", item)
          sub(/:.*/, "", item)
          gsub(/^[[:space:]"'\''"]+|[[:space:]"'\''"]+$/, "", item)
          if (item in automatic) {
            exit 42
          }
        }
      }
    ' "$workflow"; then
      :
    else
      fail "$workflow must not use block-list automatic GitHub event syntax until the local gates are proven stable"
    fi
  done
}

if [[ "${LAKECAT_CONTRACT_CHECK_ONLY:-}" == "workflows" ]]; then
  require_manual_only_workflows
  exit 0
fi

require_file Cargo.toml
require_file .github/workflows/ci.yml
require_file scripts/check-release-readiness.sh

require_pattern 'workflow_dispatch:' .github/workflows/ci.yml \
  "CI must remain manual-only through workflow_dispatch"
require_manual_only_workflows
require_pattern 'scripts/check-local-dependency-contract.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must run the dependency contract"
require_pattern 'scripts/check-workflow-trigger-contract.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must run the workflow trigger self-test"
require_pattern 'cargo test --workspace --all-features' scripts/check-release-readiness.sh \
  "release-readiness gate must run the all-features workspace test"
require_pattern 'cargo test -p lakecat-api --lib' scripts/check-release-readiness.sh \
  "release-readiness gate must run explicit lakecat-api unit tests"
require_pattern 'encodes_null_and_nested_partition_literals_for_iceberg_rest' scripts/check-release-readiness.sh \
  "release-readiness gate must prove v4 bridge partition literal encoding"
require_pattern 'scripts/qglake-handoff-local.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must include the QGLake handoff proof"

require_pattern 'grust-graph = \{ package = "grust-graph", version = "0\.9\.0",' Cargo.toml \
  "grust-graph must use the published 0.9.0 crate"
require_pattern 'typesec = \{ version = "0\.8\.0",' Cargo.toml \
  "typesec must use the published 0.8.0 crate"
require_pattern 'grust-cypher' Cargo.lock \
  "Cargo.lock must include the published Grust Cypher facade used by grust-local graph tests"
require_pattern 'name = "grust-cypher"' Cargo.lock \
  "Cargo.lock must include grust-cypher"
require_pattern 'version = "0\.9\.0"' Cargo.lock \
  "Cargo.lock must keep grust-cypher on the published 0.9.0 crate"
require_pattern 'sail-catalog = \{ path = "../sail/crates/sail-catalog" \}' Cargo.toml \
  "sail-catalog must stay an explicit local Sail path until the needed Sail API is published"
require_pattern 'sail-common-datafusion = \{ path = "../sail/crates/sail-common-datafusion" \}' Cargo.toml \
  "sail-common-datafusion must stay an explicit local Sail path until the needed Sail API is published"
require_pattern 'qglake-fixture = \["dep:sail-iceberg"\]' crates/lakecat-cli/Cargo.toml \
  "lakecat-cli qglake-fixture must keep its Sail fixture writer behind an explicit feature"
require_pattern 'sail-iceberg = \{ path = "../../../sail/crates/sail-iceberg", optional = true \}' crates/lakecat-cli/Cargo.toml \
  "lakecat-cli must keep sail-iceberg optional outside the qglake-fixture feature"
require_pattern 'cargo run -p lakecat-cli --features qglake-fixture -- qglake-fixture' scripts/qglake-handoff-local.sh \
  "local QGLake handoff must opt into the fixture feature only for the generator step"

for sibling in ../sail/crates/sail-catalog ../sail/crates/sail-common-datafusion; do
  require_dir "$sibling"
done
require_dir ../querygraph/qg-rust

for patch in \
  ci/sail-patches/0001-Expose-Iceberg-table-status-conversion.patch \
  ci/sail-patches/0002-Expose-Iceberg-planning-result-helpers.patch \
  ci/sail-patches/0003-Expose-Iceberg-generated-model-module.patch
do
  require_file "$patch"
done

require_file ../sail/crates/sail-catalog-iceberg/src/lib.rs
require_file ../sail/crates/sail-catalog-iceberg/src/planning.rs
require_file ../sail/crates/sail-catalog-iceberg/src/provider.rs
require_pattern 'pub mod models;' ../sail/crates/sail-catalog-iceberg/src/lib.rs \
  "local Sail bridge must expose the generated Iceberg REST model module"
require_pattern 'pub use crate::models::\{LoadTableResult, TableMetadata\};' ../sail/crates/sail-catalog-iceberg/src/lib.rs \
  "local Sail bridge must expose LakeCat's typed table metadata inputs"
require_pattern 'completed_planning_result_from_values' ../sail/crates/sail-catalog-iceberg/src/lib.rs \
  "local Sail bridge must expose completed planning result helpers"
require_pattern 'completed_planning_with_id_result_from_values' ../sail/crates/sail-catalog-iceberg/src/lib.rs \
  "local Sail bridge must expose plan-id planning result helpers"
require_pattern 'fetch_scan_tasks_result_from_values' ../sail/crates/sail-catalog-iceberg/src/lib.rs \
  "local Sail bridge must expose fetchScanTasks result helpers"
require_pattern 'load_table_result_to_status' ../sail/crates/sail-catalog-iceberg/src/lib.rs \
  "local Sail bridge must expose table-status conversion"
require_pattern 'pub fn completed_planning_result_from_values' ../sail/crates/sail-catalog-iceberg/src/planning.rs \
  "local Sail planning helper module must define completed planning conversion"
require_pattern 'pub fn completed_planning_with_id_result_from_values' ../sail/crates/sail-catalog-iceberg/src/planning.rs \
  "local Sail planning helper module must define completed planning-with-id conversion"
require_pattern 'pub fn fetch_scan_tasks_result_from_values' ../sail/crates/sail-catalog-iceberg/src/planning.rs \
  "local Sail planning helper module must define fetchScanTasks conversion"
require_pattern 'pub fn load_table_result_to_status' ../sail/crates/sail-catalog-iceberg/src/provider.rs \
  "local Sail provider module must define table-status conversion"

require_pattern 'git -C sail' .github/workflows/ci.yml \
  "manual CI must apply the LakeCat Sail helper patches"
require_pattern 'ci/sail-patches' .github/workflows/ci.yml \
  "manual CI must reference ci/sail-patches"
require_pattern 'repository: lakehq/sail' .github/workflows/ci.yml \
  "manual CI must check out the Sail sibling repository"
require_pattern 'cargo test -p lakecat-cli --features qglake-fixture qglake_fixture' .github/workflows/ci.yml \
  "manual CI matrix must keep explicit QGLake fixture feature coverage without automatic triggers"

require_file ../querygraph/qg-rust/src/lakecat.rs
require_pattern 'pub receipt_chain_hash: String' ../querygraph/qg-rust/src/lakecat.rs \
  "local QueryGraph importer must preserve LakeCat view receipt-chain evidence"
require_pattern 'record\.receipt_chain_hash\.is_empty' ../querygraph/qg-rust/src/lakecat.rs \
  "local QueryGraph importer must reject missing LakeCat view receipt-chain evidence"
require_pattern 'receipt-chain hash' ../querygraph/qg-rust/src/lakecat.rs \
  "local QueryGraph importer must expose a clear receipt-chain validation error"

cargo metadata --format-version 1 --no-deps > /tmp/lakecat-dependency-contract-metadata.json
require_pattern '"name":"grust-graph".*"source":"registry\+https://github.com/rust-lang/crates.io-index".*"req":"\^0\.9\.0"' /tmp/lakecat-dependency-contract-metadata.json \
  "cargo metadata must resolve grust-graph to crates.io with version requirement ^0.9.0"
require_pattern '"name":"typesec".*"source":"registry\+https://github.com/rust-lang/crates.io-index".*"req":"\^0\.8\.0"' /tmp/lakecat-dependency-contract-metadata.json \
  "cargo metadata must resolve typesec to crates.io with version requirement ^0.8.0"
cargo metadata --format-version 1 --all-features > /tmp/lakecat-dependency-contract-full-metadata.json
tr '{' '\n' < /tmp/lakecat-dependency-contract-full-metadata.json > /tmp/lakecat-dependency-contract-full-metadata-lines.json
require_pattern '"name":"grust-cypher","version":"0\.9\.0".*"source":"registry\+https://github.com/rust-lang/crates.io-index"' /tmp/lakecat-dependency-contract-full-metadata-lines.json \
  "full cargo metadata must resolve grust-cypher 0.9.0 from crates.io through grust-graph"

echo "LakeCat local dependency contract is intact."
