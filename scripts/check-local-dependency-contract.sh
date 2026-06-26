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

forbid_cargo_dependency() {
  local dependency="$1"
  local manifest="$2"
  local description="$3"
  if awk -v dependency="$dependency" '
    /^\[dependencies\]/ {
      in_dependencies = 1
      next
    }
    /^\[/ {
      in_dependencies = 0
    }
    in_dependencies {
      line = $0
      sub(/[[:space:]]*#.*/, "", line)
      if (line ~ "^[[:space:]]*" dependency "[[:space:]]*=") {
        exit 42
      }
    }
  ' "$manifest"; then
    :
  else
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
require_file scripts/check-release-proof-contract.sh
require_file DESIGN.md
require_file README.md
require_file RELEASE.md
require_file docs/book/lakecat.md
require_file docs/book/check_pdf_layout.sh

require_pattern 'workflow_dispatch:' .github/workflows/ci.yml \
  "CI must remain manual-only through workflow_dispatch"
require_manual_only_workflows
require_pattern 'scripts/check-workflow-trigger-contract\.sh' .github/workflows/ci.yml \
  "manual CI must explicitly run the workflow trigger contract self-test"
require_pattern 'scripts/check-release-version-contract\.sh' .github/workflows/ci.yml \
  "manual CI must explicitly run the release version contract"
require_pattern 'scripts/check-local-dependency-contract.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must run the dependency contract"
require_pattern 'tmpdir="\$\(mktemp -d\)"' scripts/check-local-dependency-contract.sh \
  "dependency contract must use per-run temp files for cargo metadata"
require_pattern 'scripts/check-workflow-trigger-contract.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must run the workflow trigger self-test"
require_pattern 'scripts/check-release-version-contract.sh' scripts/check-release-readiness.sh \
  "release-readiness gate must run the release version contract"
require_pattern 'scripts/check-release-proof-contract.sh' scripts/check-release-readiness.sh \
  "release-candidate gate must run the release proof contract"
require_pattern 'LAKECAT_RELEASE_PROOF_CANDIDATE=1' scripts/check-release-readiness.sh \
  "release-candidate gate must run the release proof contract in candidate mode"
require_pattern 'LAKECAT_RELEASE_PROOF_CANDIDATE' scripts/check-release-proof-contract.sh \
  "release proof contract must support clean candidate mode"
require_pattern 'scripts/check-book-artifact-contract.sh docs/book/dist' scripts/check-release-readiness.sh \
  "release-readiness gate must run the tracked book artifact contract"
require_pattern 'require_file CHANGELOG\.md' scripts/check-release-version-contract.sh \
  "release version contract must verify the changelog release heading"
require_pattern 'published release tag \$local_tag must be an ancestor of HEAD' scripts/check-release-version-contract.sh \
  "release version contract must guard the published release tag ancestry"
require_pattern 'creatordate:short' scripts/check-release-version-contract.sh \
  "release version contract must derive existing release-heading dates from published tags"
require_pattern 'post-\$local_tag hardening must remain under CHANGELOG\.md Unreleased' scripts/check-release-version-contract.sh \
  "release version contract must keep post-tag hardening under Unreleased until the workspace version moves forward"
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
require_pattern 'qglake_handoff_querygraph_import_plan_semantics_rejects_extra_verification_fields' crates/lakecat-cli/src/tests/handoff_misc.rs \
  "QGLake handoff verifier must reject extra QueryGraph import-plan verification fields"
require_pattern 'cargo test -p lakecat-service --features grust-local --lib' scripts/check-release-readiness.sh \
  "release-readiness gate must prove service outbox projection through the Grust feature"
require_pattern 'short-response-hash' crates/lakecat-service/src/tests/outbox.rs \
  "service raw lineage summary tests must reject malformed table commit response hashes"
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
require_pattern 'grust_turso_store_runs_cypher_over_lakecat_catalog_projection_boundary' crates/lakecat-graph/src/grust_integration/tests.rs \
  "LakeCat graph tests must prove Grust Cypher over the Turso-backed catalog projection"
require_pattern 'grust_turso_store_patches_lakecat_catalog_projection_nodes' scripts/check-release-readiness.sh \
  "release-readiness gate must prove Grust Turso matched-node patches over LakeCat catalog projection nodes"
require_pattern 'grust_turso_store_patches_lakecat_catalog_projection_nodes' crates/lakecat-graph/src/grust_integration/tests.rs \
  "LakeCat graph tests must prove Grust Turso matched-node patches stay in Grust"
forbid_pattern '(^|[^[:alnum:]_])turso::' crates/lakecat-graph/src/lib.rs \
  "lakecat-graph must not use the Turso crate directly; durable graph persistence belongs in Grust grust-turso"
forbid_pattern '(^|[^[:alnum:]_])turso::' crates/lakecat-service/src/main.rs \
  "lakecat-service must not use the Turso crate directly for graph sink wiring; durable graph persistence belongs in Grust grust-turso"
forbid_cargo_dependency 'turso' crates/lakecat-graph/Cargo.toml \
  "lakecat-graph must not depend on the Turso crate directly; use grust-turso for durable graph persistence"
forbid_cargo_dependency 'turso' crates/lakecat-service/Cargo.toml \
  "lakecat-service must not depend on the Turso crate directly for graph sink wiring; use grust-turso"
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
require_pattern 'workflow, release-version, tracked' scripts/check-release-readiness.sh \
  "release-readiness help must describe the current quick-gate contract surface"
require_pattern 'release-candidate' scripts/check-release-readiness.sh \
  "release-readiness help must document clean release-candidate mode"
require_pattern 'release candidate gate requires a clean tree' scripts/check-release-readiness.sh \
  "release-readiness gate must enforce clean-tree release-candidate evidence"
require_pattern 'LAKECAT_BOOK_DIST_DIR' scripts/check-release-readiness.sh \
  "release-candidate gate must build book artifacts out of tree"
require_pattern 'LAKECAT_BOOK_DIST_DIR' docs/book/build.sh \
  "book build must support an explicit artifact dist directory"
require_pattern 'CALIBRE_CONFIG_DIRECTORY' docs/book/build.sh \
  "book build must isolate Calibre conversion state from the operator profile"
require_pattern 'partial[[:space:]]+evidence instead of release-candidate success' scripts/check-release-readiness.sh \
  "release-readiness help must describe skipped full runs as partial evidence"
require_pattern 'docs/book/check_pdf_layout\.sh' RELEASE.md \
  "RELEASE.md must include the PDF layout artifact check"
require_pattern 'For the already-published `v0\.1\.0` baseline, do not run another tag command' RELEASE.md \
  "RELEASE.md must preserve the post-v0.1.0 no-retag rule"
require_pattern 'For the already-published `v0\.1\.0` baseline, do not move current post-tag' RELEASE.md \
  "RELEASE.md must keep post-v0.1.0 hardening under Unreleased"
require_pattern 'For a future version-bump release, before tagging:' RELEASE.md \
  "RELEASE.md must scope tagging chores to future version-bump releases"
forbid_pattern '^Before tagging:$' RELEASE.md \
  "RELEASE.md must not present a standalone pre-tagging section for the already-published baseline"
require_pattern 'git tag -a "v\$version"' RELEASE.md \
  "RELEASE.md must derive future unpublished release tags from the workspace version"
forbid_pattern 'git tag -a v0\.1\.0' RELEASE.md \
  "RELEASE.md must not instruct retagging the already-published v0.1.0 baseline"
require_pattern 'must not instruct retagging already-published \$local_tag' scripts/check-release-version-contract.sh \
  "release version contract must reject retag instructions for already-published workspace tags"
require_pattern 'must scope tagging chores to future version-bump releases' scripts/check-release-version-contract.sh \
  "release version contract must enforce future-version release-note scope"
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
require_pattern 'For the already-published `v0\.1\.0`, do not retag' DESIGN.md \
  "DESIGN.md must preserve the post-v0.1.0 no-retag rule"
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
require_pattern '## The Proper-Noun Test' docs/book/lakecat.md \
  "LakeCat book must keep the standard/extension/proposal taxonomy (the Proper-Noun Test in the Standards And Engine Boundary Decision Record chapter)"
require_pattern 'The handoff between LakeCat and Sail should therefore be compact and typed' docs/book/lakecat.md \
  "LakeCat book must keep the LakeCat/Sail responsibility ledger"
require_pattern 'The already-published `v0\.1\.0` tag is a baseline, not something to move' docs/book/lakecat.md \
  "LakeCat book must preserve the post-v0.1.0 no-retag explanation"
require_pattern '# First Release Readiness' docs/book/lakecat.md \
  "LakeCat book must keep the first-release readiness section"
require_pattern 'typed-sail=unavailable' docs/book/lakecat.md \
  "LakeCat book must preserve the honest typed Sail v4 posture"
require_pattern 'docs/book/check_pdf_layout\.sh docs/book/dist/lakecat\.pdf' docs/book/PUBLISH.md \
  "LakeCat book publishing runbook must include the PDF layout validator"
require_pattern 'docs/book/check_pdf_layout\.sh "\$dist_dir/lakecat\.pdf"' docs/book/build.sh \
  "LakeCat book build must run the PDF layout validator"

require_pattern 'grust-graph = \{ package = "grust-graph", git = "https://github.com/querygraph/grust.git", branch = "turso-mvcc"' Cargo.toml \
  "grust-graph must build from the querygraph/grust turso-mvcc branch as a Cargo git dependency"
require_pattern 'grust-turso = \{ package = "grust-turso", git = "https://github.com/querygraph/grust.git", branch = "turso-mvcc"' Cargo.toml \
  "grust-turso must build from the querygraph/grust turso-mvcc branch as a Cargo git dependency"
require_pattern 'grust-turso-local = \["grust-local", "dep:grust-turso"' crates/lakecat-service/Cargo.toml \
  "lakecat-service must expose the Grust Turso graph sink behind an explicit feature"
require_pattern 'grust-turso-local = \["grust-local", "dep:grust-turso"\]' crates/lakecat-graph/Cargo.toml \
  "lakecat-graph must expose Turso-backed Grust projection behind an explicit feature"
require_pattern 'grust_turso::TursoGraphStore' crates/lakecat-service/src/main.rs \
  "lakecat-service must configure Turso graph projection through the dedicated grust-turso crate"
require_pattern 'grust_turso::TursoGraphStore' crates/lakecat-graph/src/grust_integration/tests.rs \
  "LakeCat graph tests must exercise the dedicated grust-turso backend crate"
lakecat_graph_turso_tree="$tmpdir/lakecat-graph-turso-tree.txt"
cargo tree -p lakecat-graph --features grust-turso-local -i turso > "$lakecat_graph_turso_tree"
require_pattern '^turso v0\.7\.0-pre\.10$' "$lakecat_graph_turso_tree" \
  "lakecat-graph Turso inverse tree must resolve the Turso crate used by grust-turso"
require_pattern 'grust-turso v0\.10\.0 \(https://github.com/querygraph/grust.git\?branch=turso-mvcc' "$lakecat_graph_turso_tree" \
  "lakecat-graph must reach Turso only through the dedicated local grust-turso crate"
require_pattern 'lakecat-graph v0\.2\.0 \(/Users/alexy/src/lakecat/crates/lakecat-graph\)' "$lakecat_graph_turso_tree" \
  "lakecat-graph Turso inverse tree must include LakeCat graph as a grust-turso consumer"
lakecat_service_turso_tree="$tmpdir/lakecat-service-turso-tree.txt"
cargo tree -p lakecat-service --features grust-turso-local -i turso > "$lakecat_service_turso_tree"
require_pattern '^turso v0\.7\.0-pre\.10$' "$lakecat_service_turso_tree" \
  "lakecat-service Turso inverse tree must resolve the Turso crate used by grust-turso"
require_pattern 'grust-turso v0\.10\.0 \(https://github.com/querygraph/grust.git\?branch=turso-mvcc' "$lakecat_service_turso_tree" \
  "lakecat-service must reach Turso graph storage only through the dedicated local grust-turso crate"
require_pattern 'lakecat-service v0\.2\.0 \(/Users/alexy/src/lakecat/crates/lakecat-service\)' "$lakecat_service_turso_tree" \
  "lakecat-service Turso inverse tree must include the service as a grust-turso consumer"
require_pattern 'typesec = \{ version = "0\.8\.0",' Cargo.toml \
  "typesec must use the published 0.8.0 crate"
require_pattern 'name = "grust-turso"' Cargo.lock \
  "Cargo.lock must include the Grust Turso backend used by grust-turso-local graph tests"
require_pattern 'version = "0\.10\.0"' Cargo.lock \
  "Cargo.lock must keep local Grust crates on the active 0.10.0 line"
require_pattern 'version = "0\.7\.0-pre\.10"' Cargo.lock \
  "Cargo.lock must use the Turso crate line required by grust-turso"
for sail_crate in sail-catalog sail-catalog-iceberg sail-common-datafusion sail-iceberg; do
  require_pattern "$sail_crate = \\{ git = \"https://github.com/querygraph/sail.git\", branch = \"lakecat\" \\}" Cargo.toml \
    "$sail_crate must build from the querygraph/sail lakecat branch as a Cargo git dependency"
done
require_pattern 'git\+https://github.com/querygraph/sail.git\?branch=lakecat' Cargo.lock \
  "Cargo.lock must pin Sail to the querygraph/sail lakecat branch git source"
require_pattern 'qglake-fixture = \["dep:sail-iceberg"\]' crates/lakecat-cli/Cargo.toml \
  "lakecat-cli qglake-fixture must keep its Sail fixture writer behind an explicit feature"
require_pattern 'sail-iceberg = \{ workspace = true, optional = true \}' crates/lakecat-cli/Cargo.toml \
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
require_pattern '"tablePrefix",' crates/lakecat-cli/src/verify_proof.rs \
  "Rust QGLake handoff verifier must preserve graph projection table-prefix proof"
require_pattern 'const QGLAKE_GRUST_TURSO_TABLE_PREFIX: &str = "lakecat_graph";' crates/lakecat-cli/src/verify_proof.rs \
  "Rust QGLake handoff verifier must reject Grust Turso table-prefix drift"
require_pattern 'qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_projection_table_prefix_drift' crates/lakecat-cli/src/tests/handoff_artifact.rs \
  "Rust QGLake handoff artifact verifier must reject saved verifier graph projection table-prefix drift"
require_pattern 'const graphNodes = graphCount\("graph-nodes"\)' scripts/qglake-handoff-local.sh \
  "local QGLake handoff verification artifact must derive graph node count from the QueryGraph import plan"
require_pattern 'const graphEdges = graphCount\("graph-edges"\)' scripts/qglake-handoff-local.sh \
  "local QGLake handoff verification artifact must derive graph edge count from the QueryGraph import plan"
require_pattern 'require_graph_projection_proof' crates/lakecat-cli/src/verify_proof.rs \
  "LakeCat handoff verifier must require graph projection proof"

require_dir ../querygraph/qg-rust

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
require_pattern '"name":"grust-graph".*"source":"git\+https://github.com/querygraph/grust.git\?branch=turso-mvcc"' "$metadata_json" \
  "cargo metadata must resolve LakeCat's grust-graph dependency to the querygraph/grust git dependency"
require_pattern '"name":"typesec".*"source":"registry\+https://github.com/rust-lang/crates.io-index".*"req":"\^0\.8\.0"' "$metadata_json" \
  "cargo metadata must resolve typesec to crates.io with version requirement ^0.8.0"
cargo metadata --format-version 1 --all-features > "$full_metadata_json"
tr '{' '\n' < "$full_metadata_json" > "$full_metadata_lines_json"
require_pattern '"name":"grust-cypher","version":"0\.10\.0".*"source":"git\+https://github.com/querygraph/grust.git' "$full_metadata_lines_json" \
  "full cargo metadata must resolve grust-cypher 0.10.0 from the querygraph/grust git dependency"
require_pattern '"name":"grust-turso","version":"0\.10\.0".*"source":"git\+https://github.com/querygraph/grust.git' "$full_metadata_lines_json" \
  "full cargo metadata must resolve grust-turso 0.10.0 from the querygraph/grust git dependency"

echo "LakeCat local dependency contract is intact."
