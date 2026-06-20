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

require_file Cargo.toml
require_file .github/workflows/ci.yml

require_pattern 'workflow_dispatch:' .github/workflows/ci.yml \
  "CI must remain manual-only through workflow_dispatch"
if rg -q '(^|[[:space:]])(push|pull_request):' .github/workflows/ci.yml; then
  fail "CI must not run on push or pull_request until the local gates are proven stable"
fi

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
