#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

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

forbid_pattern() {
  local pattern="$1"
  local file="$2"
  local description="$3"
  if rg -q "$pattern" "$file"; then
    fail "$description"
  fi
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
require_file scripts/check-release-version-contract.sh
require_file DESIGN.md
require_file README.md
require_file RELEASE.md
require_file docs/book/lakecat.md
require_file docs/book/check_pdf_layout.sh

require_pattern 'workflow_dispatch:' .github/workflows/ci.yml \
  "CI must remain manual-only through workflow_dispatch"
require_manual_only_workflows
require_pattern 'scripts/check-local-dependency-contract.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must run the dependency contract"
require_pattern 'tmpdir="\$\(mktemp -d\)"' scripts/check-local-dependency-contract.sh \
  "dependency contract must use per-run temp files for cargo metadata"
require_pattern 'scripts/check-workflow-trigger-contract.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must run the workflow trigger self-test"
require_pattern 'scripts/check-release-version-contract.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must run the release version contract"
require_pattern 'require_file CHANGELOG\.md' scripts/check-release-version-contract.sh \
  "release version contract must verify the changelog release heading"
require_pattern 'published release tag \$local_tag must be an ancestor of HEAD' scripts/check-release-version-contract.sh \
  "release version contract must guard the published release tag ancestry"
require_pattern 'run cargo test --workspace --all-features$' scripts/check-release-readiness.sh \
  "release-readiness gate must run the complete all-features workspace test"
require_pattern 'cargo test -p lakecat-api --lib' scripts/check-release-readiness.sh \
  "release-readiness gate must run explicit lakecat-api unit tests"
require_pattern 'cargo test -p lakecat-store --lib --no-default-features' scripts/check-release-readiness.sh \
  "release-readiness gate must prove lakecat-store no-default-feature builds"
require_pattern 'encodes_null_and_nested_partition_literals_for_iceberg_rest' scripts/check-release-readiness.sh \
  "release-readiness gate must prove v4 bridge partition literal encoding"
require_pattern 'cargo test -p lakecat-cli qglake_handoff' scripts/check-release-readiness.sh \
  "release-readiness gate must explicitly exercise the Rust QGLake handoff verifier"
require_pattern 'qglake_handoff_querygraph_import_plan_semantics_rejects_extra_verification_fields' crates/lakecat-cli/src/main.rs \
  "QGLake handoff verifier must reject extra QueryGraph import-plan verification fields"
require_pattern 'cargo test -p lakecat-service --features grust-local --lib' scripts/check-release-readiness.sh \
  "release-readiness gate must prove service outbox projection through the Grust feature"
require_pattern 'cargo test -p lakecat-service --features grust-turso-local --bin lakecat-service' scripts/check-release-readiness.sh \
  "release-readiness gate must prove service startup projection through the Grust Turso feature"
require_pattern 'configured_grust_turso_graph_sink_projects_catalog_events_to_turso_store' scripts/check-release-readiness.sh \
  "release-readiness gate must exercise the configured Grust Turso graph sink"
require_pattern 'cargo test -p lakecat-graph --features grust-turso-local --lib' scripts/check-release-readiness.sh \
  "release-readiness gate must prove LakeCat graph projection persists through Grust Turso"
require_pattern 'grust_turso_store' scripts/check-release-readiness.sh \
  "release-readiness gate must run the Grust Turso graph persistence, traversal, and Cypher tests"
require_pattern 'grust_turso_store_runs_cypher_over_lakecat_catalog_projection_boundary' scripts/check-release-readiness.sh \
  "release-readiness gate must keep the explicit Grust Turso Cypher projection row"
require_pattern 'grust_turso_store_runs_cypher_over_lakecat_catalog_projection_boundary' crates/lakecat-graph/src/lib.rs \
  "LakeCat graph tests must prove Grust Cypher over the Turso-backed catalog projection"
require_pattern 'grust_turso_store_patches_lakecat_catalog_projection_nodes' scripts/check-release-readiness.sh \
  "release-readiness gate must prove Grust Turso matched-node patches over LakeCat catalog projection nodes"
require_pattern 'grust_turso_store_patches_lakecat_catalog_projection_nodes' crates/lakecat-graph/src/lib.rs \
  "LakeCat graph tests must prove Grust Turso matched-node patches stay in Grust"
forbid_pattern '(^|[^[:alnum:]_])turso::' crates/lakecat-graph/src/lib.rs \
  "lakecat-graph must not use the Turso crate directly; durable graph persistence belongs in Grust grust-turso"
forbid_pattern '(^|[^[:alnum:]_])turso::' crates/lakecat-service/src/main.rs \
  "lakecat-service must not use the Turso crate directly for graph sink wiring; durable graph persistence belongs in Grust grust-turso"
require_pattern 'scripts/qglake-handoff-local.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must include the QGLake handoff proof"

require_pattern '\[RELEASE\.md\]\(RELEASE\.md\)' README.md \
  "README.md must link to the first-release checklist"
require_pattern 'full local release-readiness gate is green as of June 23, 2026' README.md \
  "README.md must carry the latest local full-gate evidence date"
require_pattern 'First-release checklist, gate commands, release notes, and tagging steps' DESIGN.md \
  "DESIGN.md canonical map must route release checklist work to RELEASE.md"
require_pattern 'use `RELEASE\.md` as the executable release' DESIGN.md \
  "DESIGN.md first-release ledger must route release execution to RELEASE.md"
require_pattern 'manual-only' RELEASE.md \
  "RELEASE.md must preserve the manual-only cloud CI posture"
require_pattern 'scripts/check-release-readiness\.sh' RELEASE.md \
  "RELEASE.md must name the full local release gate"
require_pattern 'scripts/check-local-dependency-contract\.sh' RELEASE.md \
  "RELEASE.md must name the dependency contract preflight"
require_pattern 'without `--skip-book` or `--skip-handoff`' RELEASE.md \
  "RELEASE.md must forbid skipped book/handoff checks for release candidates"
require_pattern 'partial release-readiness checks passed with skipped release-candidate evidence' scripts/check-release-readiness.sh \
  "release-readiness gate must label skipped full runs as partial evidence"
require_pattern 'workflow, release-version, formatting' scripts/check-release-readiness.sh \
  "release-readiness help must describe the current quick-gate contract surface"
require_pattern 'release-candidate' scripts/check-release-readiness.sh \
  "release-readiness help must document clean release-candidate mode"
require_pattern 'release candidate gate requires a clean tree' scripts/check-release-readiness.sh \
  "release-readiness gate must enforce clean-tree release-candidate evidence"
require_pattern 'LAKECAT_BOOK_DIST_DIR' scripts/check-release-readiness.sh \
  "release-candidate gate must build book artifacts out of tree"
require_pattern 'LAKECAT_BOOK_DIST_DIR' docs/book/build.sh \
  "book build must support an explicit artifact dist directory"
require_pattern 'partial[[:space:]]+evidence instead of release-candidate success' scripts/check-release-readiness.sh \
  "release-readiness help must describe skipped full runs as partial evidence"
require_pattern 'docs/book/check_pdf_layout\.sh' RELEASE.md \
  "RELEASE.md must include the PDF layout artifact check"
require_pattern 'For the already-published `v0\.1\.0` baseline, do not run another tag command' RELEASE.md \
  "RELEASE.md must preserve the post-v0.1.0 no-retag rule"
require_pattern 'git tag -a "v\$version"' RELEASE.md \
  "RELEASE.md must derive future unpublished release tags from the workspace version"
forbid_pattern 'git tag -a v0\.1\.0' RELEASE.md \
  "RELEASE.md must not instruct retagging the already-published v0.1.0 baseline"
require_pattern 'must not instruct retagging already-published \$local_tag' scripts/check-release-version-contract.sh \
  "release version contract must reject retag instructions for already-published workspace tags"
require_pattern 'Typed Iceberg v4 support belongs in Sail' RELEASE.md \
  "RELEASE.md must keep typed Iceberg v4 in the deferred Sail-owned ledger"

require_pattern '### First-Release Ledger' DESIGN.md \
  "DESIGN.md must keep the first-release ledger as the living release scope"
require_pattern 'Release-blocking scope:' DESIGN.md \
  "DESIGN.md must name the release-blocking scope"
require_pattern 'Release-deferred scope:' DESIGN.md \
  "DESIGN.md must name the release-deferred scope"
require_pattern 'standard Iceberg REST surface' DESIGN.md \
  "DESIGN.md first-release ledger must preserve the standard Iceberg compatibility claim"
require_pattern 'typed-sail=unavailable' DESIGN.md \
  "DESIGN.md must preserve the honest typed Sail v4 posture"
require_pattern 'scripts/check-release-readiness.sh' DESIGN.md \
  "DESIGN.md must name the local release-readiness proof"
require_pattern 'scripts/qglake-handoff-local.sh' DESIGN.md \
  "DESIGN.md must name the QGLake handoff proof"
require_pattern 'First-release scope is intentionally narrower' README.md \
  "README.md must preserve the first-release scope warning"
require_pattern 'release version consistency across all LakeCat crates and book' README.md \
  "README.md must include the release version contract in the full-gate summary"
require_pattern 'explicit Rust `lakecat-cli qglake_handoff` verifier row' README.md \
  "README.md must include the focused Rust QGLake handoff verifier row in the full-gate summary"
require_pattern 'EPUB metadata and PDF layout validation' README.md \
  "README.md must include book artifact validators in the full-gate summary"
require_pattern '### Standard, Extension, Or Proposal\?' docs/book/lakecat.md \
  "LakeCat book must keep the standard/extension/proposal taxonomy"
require_pattern 'The handoff between LakeCat and Sail should therefore be compact and typed' docs/book/lakecat.md \
  "LakeCat book must keep the LakeCat/Sail responsibility ledger"
require_pattern '## First Release Readiness' docs/book/lakecat.md \
  "LakeCat book must keep the first-release readiness section"
require_pattern 'typed-sail=unavailable' docs/book/lakecat.md \
  "LakeCat book must preserve the honest typed Sail v4 posture"
require_pattern 'docs/book/check_pdf_layout\.sh docs/book/dist/lakecat\.pdf' docs/book/PUBLISH.md \
  "LakeCat book publishing runbook must include the PDF layout validator"
require_pattern 'docs/book/check_pdf_layout\.sh "\$dist_dir/lakecat\.pdf"' docs/book/build.sh \
  "LakeCat book build must run the PDF layout validator"

require_pattern 'grust-graph = \{ package = "grust-graph", version = "0\.10\.0", path = "../grust/crates/grust"' Cargo.toml \
  "grust-graph must use the active local Grust 0.10 path for graph traits, memory projection, and Cypher helpers"
require_pattern 'grust-turso = \{ package = "grust-turso", version = "0\.10\.0", path = "../grust/crates/grust-turso"' Cargo.toml \
  "grust-turso must use the active local Grust 0.10 path for Turso-backed graph projection"
require_pattern 'grust-turso-local = \["grust-local", "dep:grust-turso"' crates/lakecat-service/Cargo.toml \
  "lakecat-service must expose the Grust Turso graph sink behind an explicit feature"
require_pattern 'grust-turso-local = \["grust-local", "dep:grust-turso"\]' crates/lakecat-graph/Cargo.toml \
  "lakecat-graph must expose Turso-backed Grust projection behind an explicit feature"
require_pattern 'grust_turso::TursoGraphStore' crates/lakecat-service/src/main.rs \
  "lakecat-service must configure Turso graph projection through the dedicated grust-turso crate"
require_pattern 'grust_turso::TursoGraphStore' crates/lakecat-graph/src/lib.rs \
  "LakeCat graph tests must exercise the dedicated grust-turso backend crate"
require_pattern 'typesec = \{ version = "0\.8\.0",' Cargo.toml \
  "typesec must use the published 0.8.0 crate"
require_pattern 'name = "grust-turso"' Cargo.lock \
  "Cargo.lock must include the Grust Turso backend used by grust-turso-local graph tests"
require_pattern 'version = "0\.10\.0"' Cargo.lock \
  "Cargo.lock must keep local Grust crates on the active 0.10.0 line"
require_pattern 'version = "0\.7\.0-pre\.10"' Cargo.lock \
  "Cargo.lock must use the Turso crate line required by grust-turso"
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
require_pattern 'LAKECAT_GRUST_TURSO_PATH="\$GRUST_TURSO_PATH"' scripts/qglake-handoff-local.sh \
  "local QGLake handoff must configure a dedicated Grust Turso graph database"
require_pattern 'cargo run -p lakecat-service --features sail-local,grust-turso-local,typesec-local,turso-local' scripts/qglake-handoff-local.sh \
  "local QGLake handoff must exercise Grust Turso graph projection end to end"
require_pattern 'cargo run --locked --manifest-path "\$QUERYGRAPH_RUST_DIR/Cargo.toml" -- lakecat-verify' scripts/qglake-handoff-local.sh \
  "local QGLake handoff must run QueryGraph lakecat-verify with the checked lockfile"
require_pattern 'cargo run --locked --manifest-path "\$QUERYGRAPH_RUST_DIR/Cargo.toml" -- lakecat-import' scripts/qglake-handoff-local.sh \
  "local QGLake handoff must run QueryGraph lakecat-import with the checked lockfile"
require_pattern 'querygraphVerification' scripts/qglake-handoff-local.sh \
  "local QGLake handoff summary must persist QueryGraph verification evidence"
require_pattern 'querygraphImportVerification' scripts/qglake-handoff-local.sh \
  "local QGLake handoff summary must persist QueryGraph import evidence"
require_pattern '"graphProjectionProof"' scripts/qglake-handoff-local.sh \
  "local QGLake handoff must write machine-readable graph projection proof"
require_pattern '"tablePrefix": "lakecat_graph"' scripts/qglake-handoff-local.sh \
  "local QGLake handoff graph projection proof must bind the Grust Turso table prefix"
require_pattern '"tablePrefix",' crates/lakecat-cli/src/main.rs \
  "Rust QGLake handoff verifier must preserve graph projection table-prefix proof"
require_pattern 'const QGLAKE_GRUST_TURSO_TABLE_PREFIX: &str = "lakecat_graph";' crates/lakecat-cli/src/main.rs \
  "Rust QGLake handoff verifier must reject Grust Turso table-prefix drift"
require_pattern 'qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_projection_table_prefix_drift' crates/lakecat-cli/src/main.rs \
  "Rust QGLake handoff artifact verifier must reject saved verifier graph projection table-prefix drift"
require_pattern 'const graphNodes = graphCount\("graph-nodes"\)' scripts/qglake-handoff-local.sh \
  "local QGLake handoff verification artifact must derive graph node count from the QueryGraph import plan"
require_pattern 'const graphEdges = graphCount\("graph-edges"\)' scripts/qglake-handoff-local.sh \
  "local QGLake handoff verification artifact must derive graph edge count from the QueryGraph import plan"
require_pattern 'require_graph_projection_proof' crates/lakecat-cli/src/main.rs \
  "LakeCat handoff verifier must require graph projection proof"

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
require_pattern 'grust = \{ package = "grust-graph", version = "0\.10\.0", path = "../../grust/crates/grust", features = \["sail"\] \}' ../querygraph/qg-rust/Cargo.toml \
  "local QueryGraph handoff verifier must match the current Grust 0.10.0 path dependency"
require_pattern 'name = "grust-graph"' ../querygraph/qg-rust/Cargo.lock \
  "local QueryGraph lockfile must include grust-graph for LakeCat handoff verification"
require_pattern 'version = "0\.10\.0"' ../querygraph/qg-rust/Cargo.lock \
  "local QueryGraph lockfile must resolve the Grust 0.10.0 path crate used by LakeCat handoff verification"

metadata_json="$tmpdir/metadata.json"
full_metadata_json="$tmpdir/full-metadata.json"
full_metadata_lines_json="$tmpdir/full-metadata-lines.json"

cargo metadata --format-version 1 --no-deps > "$metadata_json"
require_pattern '"name":"grust-graph".*"source":null.*"req":"\^0\.10\.0".*"path":"/Users/alexy/src/grust/crates/grust"' "$metadata_json" \
  "cargo metadata must resolve LakeCat's grust-graph dependency to the local Grust 0.10 path"
require_pattern '"name":"typesec".*"source":"registry\+https://github.com/rust-lang/crates.io-index".*"req":"\^0\.8\.0"' "$metadata_json" \
  "cargo metadata must resolve typesec to crates.io with version requirement ^0.8.0"
cargo metadata --format-version 1 --all-features > "$full_metadata_json"
tr '{' '\n' < "$full_metadata_json" > "$full_metadata_lines_json"
require_pattern '"name":"grust-cypher","version":"0\.10\.0".*"source":null' "$full_metadata_lines_json" \
  "full cargo metadata must resolve grust-cypher 0.10.0 from the local Grust path"
require_pattern '"name":"grust-turso","version":"0\.10\.0".*"source":null' "$full_metadata_lines_json" \
  "full cargo metadata must resolve grust-turso 0.10.0 from the local Grust path"

echo "LakeCat local dependency contract is intact."
