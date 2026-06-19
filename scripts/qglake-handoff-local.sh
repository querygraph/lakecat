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

write_summary() {
  local bundle_sha drain_sha import_plan_sha
  bundle_sha="$(sha256_file "$BUNDLE")"
  drain_sha="$(sha256_file "$DRAIN")"
  import_plan_sha="$(sha256_file "$IMPORT_PLAN")"
  cat >"$SUMMARY" <<JSON
{
  "status": "verified",
  "catalogUrl": "$(json_string "$CATALOG_URL")",
  "principal": "$(json_string "$PRINCIPAL")",
  "warehouse": "$(json_string "$WAREHOUSE")",
  "namespace": "$(json_string "$NAMESPACE")",
  "table": "$(json_string "$TABLE")",
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
