#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
QUERYGRAPH_RUST_DIR="${QUERYGRAPH_RUST_DIR:-/Users/alexy/src/querygraph/qg-rust}"
RUN_DIR="${LAKECAT_QGLAKE_HANDOFF_DIR:-$ROOT_DIR/target/qglake-handoff}"
CATALOG_BIND="${LAKECAT_QGLAKE_BIND_ADDR:-127.0.0.1:18181}"
CATALOG_URL="${LAKECAT_QGLAKE_CATALOG_URL:-http://$CATALOG_BIND}"
PRINCIPAL="${LAKECAT_QGLAKE_PRINCIPAL:-did:example:agent}"
WAREHOUSE="${LAKECAT_QGLAKE_WAREHOUSE:-local}"
NAMESPACE="${LAKECAT_QGLAKE_NAMESPACE:-default}"
TABLE="${LAKECAT_QGLAKE_TABLE:-events}"
CARGO_TARGET_DIR="${LAKECAT_QGLAKE_CARGO_TARGET_DIR:-$ROOT_DIR/target/qglake-handoff-cargo}"
READY_TIMEOUT_SECONDS="${LAKECAT_QGLAKE_READY_TIMEOUT_SECONDS:-300}"

export CARGO_TARGET_DIR

BUNDLE="$RUN_DIR/lakecat-bootstrap.json"
DRAIN="$RUN_DIR/lineage-drain.json"
IMPORT_PLAN="$RUN_DIR/querygraph-import-plan.json"
SUMMARY="$RUN_DIR/handoff-summary.json"
LAKECAT_REPLAY_OUTPUT="$RUN_DIR/lakecat-replay.txt"
QUERYGRAPH_VERIFY_OUTPUT="$RUN_DIR/querygraph-verify.json"
QUERYGRAPH_IMPORT_OUTPUT="$RUN_DIR/querygraph-import.json"
TURSO_PATH="$RUN_DIR/lakecat.turso"
SERVICE_LOG="$RUN_DIR/lakecat-service.log"
LOCATION="file://$RUN_DIR/events"
METADATA_LOCATION="$LOCATION/metadata/00000.json"

SERVICE_PID=""

cleanup() {
  if [[ -n "$SERVICE_PID" ]] && kill -0 "$SERVICE_PID" >/dev/null 2>&1; then
    kill "$SERVICE_PID" >/dev/null 2>&1 || true
    wait "$SERVICE_PID" >/dev/null 2>&1 || true
  fi
}

trap cleanup EXIT

wait_for_lakecat() {
  local attempts="$READY_TIMEOUT_SECONDS"
  for _ in $(seq 1 "$attempts"); do
    if curl -fsS "$CATALOG_URL/catalog/v1/config" >/dev/null 2>&1; then
      return 0
    fi
    if [[ -n "$SERVICE_PID" ]] && ! kill -0 "$SERVICE_PID" >/dev/null 2>&1; then
      echo "LakeCat service exited before becoming ready. Log:" >&2
      sed -n '1,160p' "$SERVICE_LOG" >&2 || true
      return 1
    fi
    sleep 1
  done
  echo "Timed out waiting for LakeCat at $CATALOG_URL. Log:" >&2
  sed -n '1,160p' "$SERVICE_LOG" >&2 || true
  return 1
}

sha256_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

json_string() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

json_field() {
  local file="$1"
  local field="$2"
  node -e '
const fs = require("fs");
const [file, field] = process.argv.slice(1);
const value = JSON.parse(fs.readFileSync(file, "utf8"))[field];
if (value === undefined || value === null || typeof value === "object") {
  process.exit(2);
}
process.stdout.write(String(value));
' "$file" "$field"
}

json_value_field() {
  local file="$1"
  local field="$2"
  node -e '
const fs = require("fs");
const [file, field] = process.argv.slice(1);
const value = JSON.parse(fs.readFileSync(file, "utf8"))[field];
if (value === undefined || value === null) {
  process.exit(2);
}
process.stdout.write(JSON.stringify(value));
' "$file" "$field"
}

storage_profile_upsert_evidence_json() {
  local file="$1"
  node -e '
const fs = require("fs");
const [file] = process.argv.slice(1);
const replay = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = replay["replay-evidence"]?.management?.storageProfileUpsert;
if (!evidence || typeof evidence !== "object") {
  console.error("LakeCat replay evidence is missing management.storageProfileUpsert");
  process.exit(1);
}
if (!evidence.profileId || !evidence.provider) {
  console.error("LakeCat storage-profile upsert evidence is missing profileId/provider");
  process.exit(1);
}
if (typeof evidence.secretRefPresent !== "boolean") {
  console.error("LakeCat storage-profile upsert evidence is missing explicit secretRefPresent");
  process.exit(1);
}
if (!Array.isArray(evidence.replayEventHashes) || evidence.replayEventHashes.length === 0) {
  console.error("LakeCat storage-profile upsert evidence is missing replayEventHashes");
  process.exit(1);
}
if (!Array.isArray(evidence.openLineageHashes) || evidence.openLineageHashes.length === 0) {
  console.error("LakeCat storage-profile upsert evidence is missing openLineageHashes");
  process.exit(1);
}
process.stdout.write(JSON.stringify({
  profileId: evidence.profileId,
  provider: evidence.provider,
  secretRefPresent: evidence.secretRefPresent,
  secretRefProvider: evidence.secretRefProvider ?? null,
  replayEventHashes: evidence.replayEventHashes,
  openLineageHashes: evidence.openLineageHashes,
}));
' "$file"
}

required_summary_field() {
  local label="$1"
  local source_file="$2"
  local value="$3"
  if [[ -z "$value" ]]; then
    echo "Missing $label in QueryGraph output $source_file" >&2
    return 1
  fi
}

require_field_match() {
  local label="$1"
  local value="$2"
  local expected="$3"
  if [[ "$value" != "$expected" ]]; then
    echo "Handoff summary mismatch for $label: expected=$expected actual=$value" >&2
    return 1
  fi
}

write_summary() {
  local bundle_sha drain_sha import_plan_sha
  local verified_tables verified_views bundle_hash graph_hash open_lineage_hash querygraph_import_hash
  local verified_standards
  local lakecat_schema lakecat_status lakecat_tables lakecat_views lakecat_bundle_hash lakecat_graph_hash lakecat_open_lineage_hash lakecat_querygraph_import_hash lakecat_standards lakecat_replay_evidence
  local lakecat_storage_profile_upsert_evidence
  local imported_tables imported_views imported_bundle_hash imported_graph_hash imported_open_lineage_hash imported_querygraph_import_hash
  local imported_standards
  bundle_sha="$(sha256_file "$BUNDLE")"
  drain_sha="$(sha256_file "$DRAIN")"
  import_plan_sha="$(sha256_file "$IMPORT_PLAN")"
  lakecat_schema="$(json_field "$LAKECAT_REPLAY_OUTPUT" "schema-version")"
  lakecat_status="$(json_field "$LAKECAT_REPLAY_OUTPUT" "status")"
  lakecat_tables="$(json_field "$LAKECAT_REPLAY_OUTPUT" "table-count")"
  lakecat_views="$(json_field "$LAKECAT_REPLAY_OUTPUT" "view-count")"
  lakecat_bundle_hash="$(json_field "$LAKECAT_REPLAY_OUTPUT" "bundle-hash")"
  lakecat_graph_hash="$(json_field "$LAKECAT_REPLAY_OUTPUT" "graph-hash")"
  lakecat_open_lineage_hash="$(json_field "$LAKECAT_REPLAY_OUTPUT" "open-lineage-hash")"
  lakecat_querygraph_import_hash="$(json_field "$LAKECAT_REPLAY_OUTPUT" "querygraph-import-hash")"
  lakecat_standards="$(json_value_field "$LAKECAT_REPLAY_OUTPUT" "standards")"
  lakecat_replay_evidence="$(json_value_field "$LAKECAT_REPLAY_OUTPUT" "replay-evidence")"
  lakecat_storage_profile_upsert_evidence="$(storage_profile_upsert_evidence_json "$LAKECAT_REPLAY_OUTPUT")"
  verified_tables="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "table-count")"
  verified_views="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "view-count")"
  bundle_hash="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "bundle-hash")"
  graph_hash="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "graph-hash")"
  open_lineage_hash="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "open-lineage-hash")"
  querygraph_import_hash="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "querygraph-import-hash")"
  verified_standards="$(json_value_field "$QUERYGRAPH_VERIFY_OUTPUT" "standards")"
  imported_tables="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "table-count")"
  imported_views="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "view-count")"
  imported_bundle_hash="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "bundle-hash")"
  imported_graph_hash="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "graph-hash")"
  imported_open_lineage_hash="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "open-lineage-hash")"
  imported_querygraph_import_hash="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "querygraph-import-hash")"
  imported_standards="$(json_value_field "$QUERYGRAPH_IMPORT_OUTPUT" "standards")"
  required_summary_field "table-count" "$QUERYGRAPH_VERIFY_OUTPUT" "$verified_tables"
  required_summary_field "view-count" "$QUERYGRAPH_VERIFY_OUTPUT" "$verified_views"
  required_summary_field "bundle-hash" "$QUERYGRAPH_VERIFY_OUTPUT" "$bundle_hash"
  required_summary_field "graph-hash" "$QUERYGRAPH_VERIFY_OUTPUT" "$graph_hash"
  required_summary_field "open-lineage-hash" "$QUERYGRAPH_VERIFY_OUTPUT" "$open_lineage_hash"
  required_summary_field "querygraph-import-hash" "$QUERYGRAPH_VERIFY_OUTPUT" "$querygraph_import_hash"
  required_summary_field "standards" "$QUERYGRAPH_VERIFY_OUTPUT" "$verified_standards"
  required_summary_field "schema-version" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_schema"
  required_summary_field "status" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_status"
  required_summary_field "table-count" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_tables"
  required_summary_field "view-count" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_views"
  required_summary_field "bundle-hash" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_bundle_hash"
  required_summary_field "graph-hash" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_graph_hash"
  required_summary_field "open-lineage-hash" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_open_lineage_hash"
  required_summary_field "querygraph-import-hash" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_querygraph_import_hash"
  required_summary_field "standards" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_standards"
  required_summary_field "replay-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_replay_evidence"
  required_summary_field "storage-profile-upsert-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_storage_profile_upsert_evidence"
  required_summary_field "table-count" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_tables"
  required_summary_field "view-count" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_views"
  required_summary_field "bundle-hash" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_bundle_hash"
  required_summary_field "graph-hash" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_graph_hash"
  required_summary_field "open-lineage-hash" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_open_lineage_hash"
  required_summary_field "querygraph-import-hash" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_querygraph_import_hash"
  required_summary_field "standards" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_standards"
  require_field_match "table-count" "$imported_tables" "$verified_tables"
  require_field_match "view-count" "$imported_views" "$verified_views"
  require_field_match "bundle-hash" "$imported_bundle_hash" "$bundle_hash"
  require_field_match "graph-hash" "$imported_graph_hash" "$graph_hash"
  require_field_match "open-lineage-hash" "$imported_open_lineage_hash" "$open_lineage_hash"
  require_field_match "querygraph-import-hash" "$imported_querygraph_import_hash" "$querygraph_import_hash"
  require_field_match "standards" "$imported_standards" "$verified_standards"
  require_field_match "LakeCat replay schema-version" "$lakecat_schema" "lakecat.qglake.replay-verification.v1"
  require_field_match "LakeCat replay status" "$lakecat_status" "verified"
  require_field_match "LakeCat table-count" "$lakecat_tables" "$verified_tables"
  require_field_match "LakeCat view-count" "$lakecat_views" "$verified_views"
  require_field_match "LakeCat bundle-hash" "$lakecat_bundle_hash" "$bundle_hash"
  require_field_match "LakeCat graph-hash" "$lakecat_graph_hash" "$graph_hash"
  require_field_match "LakeCat open-lineage-hash" "$lakecat_open_lineage_hash" "$open_lineage_hash"
  require_field_match "LakeCat querygraph-import-hash" "$lakecat_querygraph_import_hash" "$querygraph_import_hash"
  require_field_match "LakeCat standards" "$lakecat_standards" "$verified_standards"
  cat >"$SUMMARY" <<JSON
{
  "schemaVersion": "lakecat.qglake.handoff-summary.v1",
  "status": "verified",
  "catalogUrl": "$(json_string "$CATALOG_URL")",
  "principal": "$(json_string "$PRINCIPAL")",
  "warehouse": "$(json_string "$WAREHOUSE")",
  "namespace": "$(json_string "$NAMESPACE")",
  "table": "$(json_string "$TABLE")",
  "querygraphVerification": {
    "tableCount": $verified_tables,
    "viewCount": $verified_views,
    "bundleHash": "$(json_string "$bundle_hash")",
    "graphHash": "$(json_string "$graph_hash")",
    "openLineageHash": "$(json_string "$open_lineage_hash")",
    "querygraphImportHash": "$(json_string "$querygraph_import_hash")",
    "standards": $verified_standards
  },
  "querygraphImportVerification": {
    "matchesVerify": true
  },
  "lakecatReplayVerification": {
    "schemaVersion": "$(json_string "$lakecat_schema")",
    "status": "$(json_string "$lakecat_status")",
    "matchesQueryGraph": true,
    "storageProfileUpsertProof": $lakecat_storage_profile_upsert_evidence,
    "replayEvidence": $lakecat_replay_evidence
  },
  "artifacts": {
    "bundle": {
      "path": "$(json_string "$BUNDLE")",
      "sha256": "sha256:$bundle_sha"
    },
    "lineageDrain": {
      "path": "$(json_string "$DRAIN")",
      "sha256": "sha256:$drain_sha"
    },
    "querygraphImportPlan": {
      "path": "$(json_string "$IMPORT_PLAN")",
      "sha256": "sha256:$import_plan_sha"
    },
    "lakecatReplayOutput": "$(json_string "$LAKECAT_REPLAY_OUTPUT")",
    "querygraphVerifyOutput": "$(json_string "$QUERYGRAPH_VERIFY_OUTPUT")",
    "querygraphImportOutput": "$(json_string "$QUERYGRAPH_IMPORT_OUTPUT")",
    "serviceLog": "$(json_string "$SERVICE_LOG")"
  }
}
JSON
}

if [[ ! -f "$QUERYGRAPH_RUST_DIR/Cargo.toml" ]]; then
  echo "QueryGraph Rust crate not found at $QUERYGRAPH_RUST_DIR" >&2
  exit 1
fi

mkdir -p "$RUN_DIR"
rm -f "$BUNDLE" "$DRAIN" "$IMPORT_PLAN" "$SUMMARY" \
  "$LAKECAT_REPLAY_OUTPUT" "$QUERYGRAPH_VERIFY_OUTPUT" "$QUERYGRAPH_IMPORT_OUTPUT" \
  "$TURSO_PATH" "$SERVICE_LOG"

echo "Starting LakeCat at $CATALOG_URL"
(
  cd "$ROOT_DIR"
  LAKECAT_BIND_ADDR="$CATALOG_BIND" \
    LAKECAT_WAREHOUSE="$WAREHOUSE" \
    LAKECAT_TURSO_PATH="$TURSO_PATH" \
    cargo run -p lakecat-service --features sail-local,grust-local,typesec-local,turso-local \
    >"$SERVICE_LOG" 2>&1
) &
SERVICE_PID="$!"

wait_for_lakecat

echo "Generating live QGLake bootstrap and lineage-drain artifacts"
cargo run -p lakecat-cli -- qglake-fixture \
  --catalog "$CATALOG_URL" \
  --warehouse "$WAREHOUSE" \
  --namespace "$NAMESPACE" \
  --table "$TABLE" \
  --location "$LOCATION" \
  --metadata-location "$METADATA_LOCATION" \
  --output "$BUNDLE" \
  --drain-output "$DRAIN" \
  --principal "$PRINCIPAL"

echo "Verifying saved LakeCat replay artifacts"
cargo run -p lakecat-cli -- qglake-verify-replay \
  --bundle "$BUNDLE" \
  --drain "$DRAIN" \
  --principal "$PRINCIPAL" \
  --json \
  | tee "$LAKECAT_REPLAY_OUTPUT"

echo "Verifying bundle with QueryGraph"
cargo run --locked --manifest-path "$QUERYGRAPH_RUST_DIR/Cargo.toml" -- lakecat-verify \
  --bundle "$BUNDLE" \
  | tee "$QUERYGRAPH_VERIFY_OUTPUT"

echo "Writing QueryGraph import plan"
cargo run --locked --manifest-path "$QUERYGRAPH_RUST_DIR/Cargo.toml" -- lakecat-import \
  --bundle "$BUNDLE" \
  --output "$IMPORT_PLAN" \
  | tee "$QUERYGRAPH_IMPORT_OUTPUT"

write_summary

echo "QGLake handoff verified"
echo "  bundle:      $BUNDLE"
echo "  drain:       $DRAIN"
echo "  import plan: $IMPORT_PLAN"
echo "  summary:     $SUMMARY"
