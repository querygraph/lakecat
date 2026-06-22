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
LAKECAT_HANDOFF_VERIFY_OUTPUT="$RUN_DIR/lakecat-handoff-verify.json"
LAKECAT_HANDOFF_SELF_VERIFY_OUTPUT="$RUN_DIR/lakecat-handoff-self-verify.json"
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

request_identity_evidence_json() {
  local file="$1"
  local expected_principal="$2"
  node -e '
const fs = require("fs");
const [file, expectedPrincipal] = process.argv.slice(1);
const replay = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = replay["replay-evidence"]?.requestIdentity;
if (!evidence || typeof evidence !== "object") {
  console.error("LakeCat replay evidence is missing requestIdentity");
  process.exit(1);
}
if (evidence.principalSubject !== expectedPrincipal) {
  console.error(`LakeCat request identity principal mismatch: expected=${expectedPrincipal} actual=${evidence.principalSubject}`);
  process.exit(1);
}
if (evidence.principalKind !== "agent") {
  console.error("LakeCat request identity evidence does not prove an agent principal");
  process.exit(1);
}
if (typeof evidence.requestIdentitySource !== "string" || evidence.requestIdentitySource.length === 0) {
  console.error("LakeCat request identity evidence is missing requestIdentitySource");
  process.exit(1);
}
if (typeof evidence.requestIdentityState !== "string" || evidence.requestIdentityState.length === 0) {
  console.error("LakeCat request identity evidence is missing requestIdentityState");
  process.exit(1);
}
if (typeof evidence.authorizationReceiptHash !== "string" || evidence.authorizationReceiptHash.length === 0) {
  console.error("LakeCat request identity evidence is missing authorizationReceiptHash");
  process.exit(1);
}
if (typeof evidence.authorizationReceiptAction !== "string" || evidence.authorizationReceiptAction.length === 0) {
  console.error("LakeCat request identity evidence is missing authorizationReceiptAction");
  process.exit(1);
}
function requireOptionalHash(value, label) {
  if (value == null) {
    return false;
  }
  if (typeof value === "string" && value.startsWith("sha256:")) {
    return true;
  }
  console.error(`LakeCat request identity evidence ${label} must be null or a sha256 hash`);
  process.exit(1);
}
const hasTypedIdEnvelope = requireOptionalHash(evidence.typedidEnvelopeHash, "typedidEnvelopeHash");
const hasTypedIdProof = requireOptionalHash(evidence.typedidProofHash, "typedidProofHash");
if (hasTypedIdProof && !hasTypedIdEnvelope) {
  console.error("LakeCat request identity evidence has typedidProofHash without typedidEnvelopeHash");
  process.exit(1);
}
process.stdout.write(JSON.stringify(evidence));
process.exit(0);
process.stdout.write(JSON.stringify({
  principalSubject: evidence.principalSubject,
  principalKind: evidence.principalKind,
  requestIdentitySource: evidence.requestIdentitySource,
  requestIdentityState: evidence.requestIdentityState,
  authorizationReceiptHash: evidence.authorizationReceiptHash,
  authorizationReceiptAction: evidence.authorizationReceiptAction,
  typedidEnvelopeHash: evidence.typedidEnvelopeHash ?? null,
  typedidProofHash: evidence.typedidProofHash ?? null,
}));
' "$file" "$expected_principal"
}

querygraph_bootstrap_evidence_json() {
  local file="$1"
  local expected_principal="$2"
  local expected_table_count="$3"
  local expected_view_count="$4"
  local expected_bundle_hash="$5"
  local expected_graph_hash="$6"
  local expected_open_lineage_hash="$7"
  local expected_querygraph_import_hash="$8"
  local expected_standards_json="$9"
  node -e '
const fs = require("fs");
const [
  file,
  expectedPrincipal,
  expectedTableCount,
  expectedViewCount,
  expectedBundleHash,
  expectedGraphHash,
  expectedOpenLineageHash,
  expectedQueryGraphImportHash,
  expectedStandardsJson,
] = process.argv.slice(1);
const replay = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = replay["replay-evidence"]?.queryGraphBootstrap;
if (!evidence || typeof evidence !== "object") {
  console.error("LakeCat replay evidence is missing queryGraphBootstrap");
  process.exit(1);
}
function requireHash(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    console.error(`LakeCat QueryGraph bootstrap evidence is missing ${label}`);
    process.exit(1);
  }
}
function requireOptionalHash(value, label) {
  if (value == null) {
    return false;
  }
  if (typeof value === "string" && value.startsWith("sha256:")) {
    return true;
  }
  console.error(`LakeCat QueryGraph bootstrap evidence ${label} must be null or a sha256 hash`);
  process.exit(1);
}
function requireHashArray(value, label) {
  if (!Array.isArray(value) || value.length === 0) {
    console.error(`LakeCat QueryGraph bootstrap evidence is missing ${label}`);
    process.exit(1);
  }
  const seen = new Set();
  for (const item of value) {
    if (typeof item !== "string" || !/^sha256:[0-9a-fA-F]{64}$/.test(item)) {
      console.error(`LakeCat QueryGraph bootstrap evidence ${label} must contain full SHA-256 hashes`);
      process.exit(1);
    }
    if (seen.has(item)) {
      console.error(`LakeCat QueryGraph bootstrap evidence ${label} must be duplicate-free`);
      process.exit(1);
    }
    seen.add(item);
  }
}
function requireIntegerMatch(value, expected, label) {
  const expectedNumber = Number(expected);
  if (!Number.isInteger(value) || value !== expectedNumber) {
    console.error(`LakeCat QueryGraph bootstrap ${label} mismatch: expected=${expectedNumber} actual=${value}`);
    process.exit(1);
  }
}
function requireMatch(value, expected, label) {
  if (value !== expected) {
    console.error(`LakeCat QueryGraph bootstrap ${label} mismatch: expected=${expected} actual=${value}`);
    process.exit(1);
  }
}
requireMatch(evidence.bundleHash, expectedBundleHash, "bundleHash");
requireMatch(evidence.graphHash, expectedGraphHash, "graphHash");
requireMatch(evidence.openLineageHash, expectedOpenLineageHash, "openLineageHash");
requireMatch(evidence.queryGraphImportHash, expectedQueryGraphImportHash, "queryGraphImportHash");
requireIntegerMatch(evidence.tableArtifactCount, expectedTableCount, "tableArtifactCount");
requireIntegerMatch(evidence.viewArtifactCount, expectedViewCount, "viewArtifactCount");
if (!Number.isInteger(evidence.policyBindingCount) || evidence.policyBindingCount <= 0) {
  console.error("LakeCat QueryGraph bootstrap evidence is missing positive policyBindingCount");
  process.exit(1);
}
const expectedStandards = JSON.parse(expectedStandardsJson);
if (JSON.stringify(evidence.standards) !== JSON.stringify(expectedStandards)) {
  console.error("LakeCat QueryGraph bootstrap standards mismatch");
  process.exit(1);
}
requireMatch(evidence.principalSubject, expectedPrincipal, "principalSubject");
requireMatch(evidence.principalKind, "agent", "principalKind");
if (typeof evidence.requestIdentitySource !== "string" || evidence.requestIdentitySource.length === 0) {
  console.error("LakeCat QueryGraph bootstrap evidence is missing requestIdentitySource");
  process.exit(1);
}
if (typeof evidence.requestIdentityState !== "string" || evidence.requestIdentityState.length === 0) {
  console.error("LakeCat QueryGraph bootstrap evidence is missing requestIdentityState");
  process.exit(1);
}
requireHash(evidence.authorizationReceiptHash, "authorizationReceiptHash");
if (typeof evidence.authorizationReceiptAction !== "string" || evidence.authorizationReceiptAction.length === 0) {
  console.error("LakeCat QueryGraph bootstrap evidence is missing authorizationReceiptAction");
  process.exit(1);
}
requireHash(evidence.agentDelegationHash, "agentDelegationHash");
requireHash(evidence.agentSummarySignatureHash, "agentSummarySignatureHash");
const hasTypedIdEnvelope = requireOptionalHash(evidence.typedidEnvelopeHash, "typedidEnvelopeHash");
const hasTypedIdProof = requireOptionalHash(evidence.typedidProofHash, "typedidProofHash");
if (hasTypedIdProof && !hasTypedIdEnvelope) {
  console.error("LakeCat QueryGraph bootstrap evidence has typedidProofHash without typedidEnvelopeHash");
  process.exit(1);
}
if (Number(expectedViewCount) > 0) {
  requireHashArray(evidence.viewVersionReceiptHashes, "viewVersionReceiptHashes");
} else if (!Array.isArray(evidence.viewVersionReceiptHashes)) {
  console.error("LakeCat QueryGraph bootstrap evidence is missing viewVersionReceiptHashes");
  process.exit(1);
}
requireHashArray(evidence.replayEventHashes, "replayEventHashes");
requireHashArray(evidence.openLineageHashes, "openLineageHashes");
process.stdout.write(JSON.stringify(evidence));
process.exit(0);
process.stdout.write(JSON.stringify({
  bundleHash: evidence.bundleHash,
  graphHash: evidence.graphHash,
  openLineageHash: evidence.openLineageHash,
  queryGraphImportHash: evidence.queryGraphImportHash,
  tableArtifactCount: evidence.tableArtifactCount,
  viewArtifactCount: evidence.viewArtifactCount,
  policyBindingCount: evidence.policyBindingCount,
  standards: evidence.standards,
  principalSubject: evidence.principalSubject,
  principalKind: evidence.principalKind,
  requestIdentitySource: evidence.requestIdentitySource,
  requestIdentityState: evidence.requestIdentityState,
  authorizationReceiptHash: evidence.authorizationReceiptHash,
  authorizationReceiptAction: evidence.authorizationReceiptAction,
  agentDelegationHash: evidence.agentDelegationHash,
  agentSummarySignatureHash: evidence.agentSummarySignatureHash,
  typedidEnvelopeHash: evidence.typedidEnvelopeHash ?? null,
  typedidProofHash: evidence.typedidProofHash ?? null,
  viewVersionReceiptHashes: evidence.viewVersionReceiptHashes,
  replayEventHashes: evidence.replayEventHashes,
  openLineageHashes: evidence.openLineageHashes,
}));
' "$file" "$expected_principal" "$expected_table_count" "$expected_view_count" "$expected_bundle_hash" "$expected_graph_hash" "$expected_open_lineage_hash" "$expected_querygraph_import_hash" "$expected_standards_json"
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
if (!evidence.issuanceMode) {
  console.error("LakeCat storage-profile upsert evidence is missing issuanceMode");
  process.exit(1);
}
if (!evidence.locationPrefixHash) {
  console.error("LakeCat storage-profile upsert evidence is missing locationPrefixHash");
  process.exit(1);
}
if (typeof evidence.secretRefPresent !== "boolean") {
  console.error("LakeCat storage-profile upsert evidence is missing explicit secretRefPresent");
  process.exit(1);
}
if (evidence.secretRefPresent && !evidence.secretRefProvider) {
  console.error("LakeCat storage-profile upsert evidence is missing secretRefProvider");
  process.exit(1);
}
if (evidence.secretRefPresent && !evidence.secretRefHash) {
  console.error("LakeCat storage-profile upsert evidence is missing secretRefHash");
  process.exit(1);
}
if (!evidence.secretRefPresent && evidence.secretRefProvider != null) {
  console.error("LakeCat storage-profile upsert evidence has secretRefProvider without secretRefPresent");
  process.exit(1);
}
if (!evidence.secretRefPresent && evidence.secretRefHash != null) {
  console.error("LakeCat storage-profile upsert evidence has secretRefHash without secretRefPresent");
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
if (!Number.isInteger(evidence.graphEvents) || evidence.graphEvents <= 0) {
  console.error("LakeCat storage-profile upsert evidence is missing graphEvents");
  process.exit(1);
}
process.stdout.write(JSON.stringify(evidence));
process.exit(0);
process.stdout.write(JSON.stringify({
  profileId: evidence.profileId,
  provider: evidence.provider,
  issuanceMode: evidence.issuanceMode,
  locationPrefixHash: evidence.locationPrefixHash,
  secretRefPresent: evidence.secretRefPresent,
  secretRefProvider: evidence.secretRefProvider ?? null,
  secretRefHash: evidence.secretRefHash ?? null,
  graphEvents: evidence.graphEvents,
  replayEventHashes: evidence.replayEventHashes,
  openLineageHashes: evidence.openLineageHashes,
}));
' "$file"
}

management_evidence_json() {
  local file="$1"
  node -e '
const fs = require("fs");
const [file] = process.argv.slice(1);
const replay = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = replay["replay-evidence"]?.management;
if (!evidence || typeof evidence !== "object") {
  console.error("LakeCat replay evidence is missing management");
  process.exit(1);
}
function requirePositiveInteger(value, label) {
  if (!Number.isInteger(value) || value <= 0) {
    console.error(`LakeCat management replay evidence is missing positive ${label}`);
    process.exit(1);
  }
}
function requireHashArray(value, label) {
  if (!Array.isArray(value) || value.length === 0) {
    console.error(`LakeCat management replay evidence is missing ${label}`);
    process.exit(1);
  }
  const seen = new Set();
  for (const item of value) {
    if (typeof item !== "string" || !/^sha256:[0-9a-fA-F]{64}$/.test(item)) {
      console.error(`LakeCat management replay evidence ${label} must contain full SHA-256 hashes`);
      process.exit(1);
    }
    if (seen.has(item)) {
      console.error(`LakeCat management replay evidence ${label} must be duplicate-free`);
      process.exit(1);
    }
    seen.add(item);
  }
}
function requireStringArrayCount(value, expected, label) {
  if (!Array.isArray(value) || value.length !== expected || value.some((item) => typeof item !== "string" || item.length === 0)) {
    console.error(`LakeCat management replay evidence ${label} must have ${expected} non-empty string value(s)`);
    process.exit(1);
  }
}
for (const field of [
  "serverCount",
  "serverGraphEvents",
  "projectCount",
  "projectGraphEvents",
  "warehouseCount",
  "warehouseGraphEvents",
  "policyBindingCount",
  "policyGraphEvents",
  "storageProfileCount",
  "storageProfileGraphEvents",
]) {
  requirePositiveInteger(evidence[field], field);
}
for (const field of [
  "serverReplayEventHashes",
  "serverOpenLineageHashes",
  "projectReplayEventHashes",
  "projectOpenLineageHashes",
  "warehouseReplayEventHashes",
  "warehouseOpenLineageHashes",
  "policyReplayEventHashes",
  "policyOpenLineageHashes",
  "storageProfileReplayEventHashes",
  "storageProfileOpenLineageHashes",
]) {
  requireHashArray(evidence[field], field);
}
requireStringArrayCount(evidence.serverIds, evidence.serverCount, "serverIds");
requireStringArrayCount(evidence.projectIds, evidence.projectCount, "projectIds");
requireStringArrayCount(evidence.warehouseNames, evidence.warehouseCount, "warehouseNames");
requireStringArrayCount(evidence.policyIds, evidence.policyBindingCount, "policyIds");
requireStringArrayCount(evidence.storageProfileIds, evidence.storageProfileCount, "storageProfileIds");
process.stdout.write(JSON.stringify(evidence));
process.exit(0);
process.stdout.write(JSON.stringify({
  serverCount: evidence.serverCount,
  serverIds: evidence.serverIds,
  serverGraphEvents: evidence.serverGraphEvents,
  projectCount: evidence.projectCount,
  projectIds: evidence.projectIds,
  projectGraphEvents: evidence.projectGraphEvents,
  warehouseCount: evidence.warehouseCount,
  warehouseNames: evidence.warehouseNames,
  warehouseGraphEvents: evidence.warehouseGraphEvents,
  policyBindingCount: evidence.policyBindingCount,
  policyIds: evidence.policyIds,
  policyGraphEvents: evidence.policyGraphEvents,
  storageProfileCount: evidence.storageProfileCount,
  storageProfileIds: evidence.storageProfileIds,
  storageProfileGraphEvents: evidence.storageProfileGraphEvents,
  serverReplayEventHashes: evidence.serverReplayEventHashes,
  serverOpenLineageHashes: evidence.serverOpenLineageHashes,
  projectReplayEventHashes: evidence.projectReplayEventHashes,
  projectOpenLineageHashes: evidence.projectOpenLineageHashes,
  warehouseReplayEventHashes: evidence.warehouseReplayEventHashes,
  warehouseOpenLineageHashes: evidence.warehouseOpenLineageHashes,
  policyReplayEventHashes: evidence.policyReplayEventHashes,
  policyOpenLineageHashes: evidence.policyOpenLineageHashes,
  storageProfileReplayEventHashes: evidence.storageProfileReplayEventHashes,
  storageProfileOpenLineageHashes: evidence.storageProfileOpenLineageHashes,
}));
' "$file"
}

credential_vending_evidence_json() {
  local file="$1"
  node -e '
const fs = require("fs");
const [file] = process.argv.slice(1);
const replay = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = replay["replay-evidence"]?.credentials;
if (!evidence || typeof evidence !== "object") {
  console.error("LakeCat replay evidence is missing credentials");
  process.exit(1);
}
const restricted = evidence.restricted;
const trustedHuman = evidence.trustedHuman;
function requireHashArray(value, label) {
  if (!Array.isArray(value) || value.length === 0) {
    console.error(`LakeCat credential replay evidence is missing ${label}`);
    process.exit(1);
  }
  const seen = new Set();
  for (const item of value) {
    if (typeof item !== "string" || !/^sha256:[0-9a-fA-F]{64}$/.test(item)) {
      console.error(`LakeCat credential replay evidence ${label} must contain full SHA-256 hashes`);
      process.exit(1);
    }
    if (seen.has(item)) {
      console.error(`LakeCat credential replay evidence ${label} must be duplicate-free`);
      process.exit(1);
    }
    seen.add(item);
  }
}
function requireCredentialPrefixHashes(value, credentialCount, label) {
  if (!Array.isArray(value) || value.length !== credentialCount) {
    console.error(`LakeCat credential replay evidence ${label}.credentialPrefixHashes count mismatch`);
    process.exit(1);
  }
  const seen = new Set();
  for (const hash of value) {
    if (typeof hash !== "string" || !/^sha256:[0-9a-fA-F]{64}$/.test(hash)) {
      console.error(`LakeCat credential replay evidence ${label}.credentialPrefixHashes must contain full SHA-256 hashes`);
      process.exit(1);
    }
    if (seen.has(hash)) {
      console.error(`LakeCat credential replay evidence ${label}.credentialPrefixHashes must be duplicate-free`);
      process.exit(1);
    }
    seen.add(hash);
  }
  return value;
}
function requireStorageProfile(value, label) {
  if (!value || typeof value !== "object") {
    console.error(`LakeCat credential replay evidence is missing ${label}.storageProfile`);
    process.exit(1);
  }
  if (!value.profileId || !value.provider || !value.issuanceMode || !value.locationPrefixHash) {
    console.error(`LakeCat credential replay evidence is missing ${label} storage-profile identity`);
    process.exit(1);
  }
  if (typeof value.secretRefPresent !== "boolean") {
    console.error(`LakeCat credential replay evidence is missing ${label} storage-profile secret-ref presence`);
    process.exit(1);
  }
  if (!Number.isInteger(value.graphEvents) || value.graphEvents <= 0) {
    console.error(`LakeCat credential replay evidence is missing ${label} credential-root graph projection`);
    process.exit(1);
  }
  if (value.secretRefPresent === false && value.secretRefProvider !== null && value.secretRefProvider !== undefined) {
    console.error(`LakeCat credential replay evidence carried ${label} secret-ref provider without secret-ref presence`);
    process.exit(1);
  }
  if (value.secretRefPresent === true && !value.secretRefHash) {
    console.error(`LakeCat credential replay evidence is missing ${label} secret-ref hash`);
    process.exit(1);
  }
  if (value.secretRefPresent === false && value.secretRefHash !== null && value.secretRefHash !== undefined) {
    console.error(`LakeCat credential replay evidence carried ${label} secret-ref hash without secret-ref presence`);
    process.exit(1);
  }
  return {
    profileId: value.profileId,
    provider: value.provider,
    issuanceMode: value.issuanceMode,
    locationPrefixHash: value.locationPrefixHash,
    secretRefPresent: value.secretRefPresent,
    secretRefProvider: value.secretRefProvider ?? null,
    secretRefHash: value.secretRefHash ?? null,
    graphEvents: value.graphEvents,
  };
}
function requireMaxCredentialTtl(value, label) {
  const ttl = value.maxCredentialTtlSeconds ?? value.max_credential_ttl_seconds ?? value.readRestriction?.["max-credential-ttl-seconds"] ?? value.read_restriction?.["max-credential-ttl-seconds"];
  if (!Number.isInteger(ttl) || ttl <= 0) {
    console.error(`LakeCat credential replay evidence is missing ${label} max credential TTL evidence`);
    process.exit(1);
  }
  return ttl;
}
if (!restricted || typeof restricted !== "object") {
  console.error("LakeCat replay evidence is missing credentials.restricted");
  process.exit(1);
}
if (!restricted.principalSubject || !restricted.principalKind) {
  console.error("LakeCat restricted credential evidence is missing principal identity");
  process.exit(1);
}
if (restricted.credentialCount !== 0 || restricted.blockReason !== "fine-grained read restriction requires Sail-planned reads") {
  console.error("LakeCat restricted credential evidence does not prove Sail-planned reads were required");
  process.exit(1);
}
const restrictedCredentialPrefixHashes = requireCredentialPrefixHashes(
  restricted.credentialPrefixHashes,
  restricted.credentialCount,
  "restricted"
);
requireHashArray(restricted.replayEventHashes, "restricted replayEventHashes");
requireHashArray(restricted.openLineageHashes, "restricted openLineageHashes");
const restrictedStorageProfile = requireStorageProfile(restricted.storageProfile, "restricted");
const restrictedMaxCredentialTtlSeconds = requireMaxCredentialTtl(restricted, "restricted");
if (!trustedHuman || typeof trustedHuman !== "object") {
  console.error("LakeCat replay evidence is missing credentials.trustedHuman");
  process.exit(1);
}
if (!trustedHuman.principalSubject || trustedHuman.principalKind !== "human") {
  console.error("LakeCat trusted-human credential evidence is missing human principal identity");
  process.exit(1);
}
if (!(trustedHuman.credentialCount > 0) || trustedHuman.rawCredentialExceptionAllowed !== true) {
  console.error("LakeCat trusted-human credential evidence does not prove audited credential vending");
  process.exit(1);
}
const trustedHumanCredentialPrefixHashes = requireCredentialPrefixHashes(
  trustedHuman.credentialPrefixHashes,
  trustedHuman.credentialCount,
  "trusted-human"
);
if (trustedHuman.rawCredentialExceptionReason !== "trusted human principal may use audited raw credential vending") {
  console.error("LakeCat trusted-human credential evidence is missing the audited exception reason");
  process.exit(1);
}
requireHashArray(trustedHuman.replayEventHashes, "trusted-human replayEventHashes");
requireHashArray(trustedHuman.openLineageHashes, "trusted-human openLineageHashes");
const trustedHumanStorageProfile = requireStorageProfile(trustedHuman.storageProfile, "trusted-human");
const trustedHumanMaxCredentialTtlSeconds = requireMaxCredentialTtl(trustedHuman, "trusted-human");
process.stdout.write(JSON.stringify(evidence));
process.exit(0);
process.stdout.write(JSON.stringify({
  restricted: {
    principalSubject: restricted.principalSubject,
    principalKind: restricted.principalKind,
    credentialCount: restricted.credentialCount,
    credentialPrefixHashes: restrictedCredentialPrefixHashes,
    rawCredentialExceptionAllowed: restricted.rawCredentialExceptionAllowed,
    blockReason: restricted.blockReason,
    maxCredentialTtlSeconds: restrictedMaxCredentialTtlSeconds,
    storageProfile: restrictedStorageProfile,
    replayEventHashes: restricted.replayEventHashes,
    openLineageHashes: restricted.openLineageHashes,
  },
  trustedHuman: {
    principalSubject: trustedHuman.principalSubject,
    principalKind: trustedHuman.principalKind,
    credentialCount: trustedHuman.credentialCount,
    credentialPrefixHashes: trustedHumanCredentialPrefixHashes,
    rawCredentialExceptionAllowed: trustedHuman.rawCredentialExceptionAllowed,
    rawCredentialExceptionReason: trustedHuman.rawCredentialExceptionReason,
    blockReason: trustedHuman.blockReason ?? null,
    maxCredentialTtlSeconds: trustedHumanMaxCredentialTtlSeconds,
    storageProfile: trustedHumanStorageProfile,
    replayEventHashes: trustedHuman.replayEventHashes,
    openLineageHashes: trustedHuman.openLineageHashes,
  },
}));
' "$file"
}

governed_scan_evidence_json() {
  local file="$1"
  node -e '
const fs = require("fs");
const [file] = process.argv.slice(1);
const replay = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = replay["replay-evidence"]?.scan;
if (!evidence || typeof evidence !== "object") {
  console.error("LakeCat replay evidence is missing scan");
  process.exit(1);
}
function requirePositiveInteger(value, label) {
  if (!Number.isInteger(value) || value <= 0) {
    console.error(`LakeCat scan replay evidence is missing positive ${label}`);
    process.exit(1);
  }
}
function requireNonNegativeInteger(value, label) {
  if (!Number.isInteger(value) || value < 0) {
    console.error(`LakeCat scan replay evidence is missing non-negative ${label}`);
    process.exit(1);
  }
}
function requireHashArray(value, label) {
  if (!Array.isArray(value) || value.length === 0) {
    console.error(`LakeCat scan replay evidence is missing ${label}`);
    process.exit(1);
  }
  const seen = new Set();
  for (const item of value) {
    if (typeof item !== "string" || !/^sha256:[0-9a-fA-F]{64}$/.test(item)) {
      console.error(`LakeCat scan replay evidence ${label} must contain full SHA-256 hashes`);
      process.exit(1);
    }
    if (seen.has(item)) {
      console.error(`LakeCat scan replay evidence ${label} must be duplicate-free`);
      process.exit(1);
    }
    seen.add(item);
  }
}
requirePositiveInteger(evidence.planTaskCount, "planTaskCount");
requirePositiveInteger(evidence.planGraphEvents, "planGraphEvents");
requirePositiveInteger(evidence.fileTaskCount, "fileTaskCount");
requirePositiveInteger(evidence.childPlanTaskCount, "childPlanTaskCount");
requireNonNegativeInteger(evidence.deleteFileCount, "deleteFileCount");
requireHashArray(evidence.plannedReplayEventHashes, "plannedReplayEventHashes");
requireHashArray(evidence.fetchedReplayEventHashes, "fetchedReplayEventHashes");
requireHashArray(evidence.plannedOpenLineageHashes, "plannedOpenLineageHashes");
requireHashArray(evidence.fetchedOpenLineageHashes, "fetchedOpenLineageHashes");
if (!Array.isArray(evidence.fetchedRequiredProjection) || evidence.fetchedRequiredProjection.length === 0) {
  console.error("LakeCat scan replay evidence is missing fetchedRequiredProjection");
  process.exit(1);
}
if (!Array.isArray(evidence.fetchedEffectiveProjection) || evidence.fetchedEffectiveProjection.length === 0) {
  console.error("LakeCat scan replay evidence is missing fetchedEffectiveProjection");
  process.exit(1);
}
if (!Array.isArray(evidence.plannedRequestedProjection) || evidence.plannedRequestedProjection.length === 0) {
  console.error("LakeCat scan replay evidence is missing plannedRequestedProjection");
  process.exit(1);
}
if (!Array.isArray(evidence.plannedEffectiveProjection) || evidence.plannedEffectiveProjection.length === 0) {
  console.error("LakeCat scan replay evidence is missing plannedEffectiveProjection");
  process.exit(1);
}
if (evidence.plannedRequestedProjection.some((item) => typeof item !== "string" || item.length === 0)) {
  console.error("LakeCat scan replay evidence has invalid plannedRequestedProjection");
  process.exit(1);
}
if (evidence.plannedEffectiveProjection.some((item) => typeof item !== "string" || item.length === 0)) {
  console.error("LakeCat scan replay evidence has invalid plannedEffectiveProjection");
  process.exit(1);
}
if (evidence.plannedRequestedProjection.length <= evidence.plannedEffectiveProjection.length) {
  console.error("LakeCat scan replay evidence does not prove projection narrowing");
  process.exit(1);
}
if (!Array.isArray(evidence.plannedRequestedStatsFields) || evidence.plannedRequestedStatsFields.length === 0) {
  console.error("LakeCat scan replay evidence is missing plannedRequestedStatsFields");
  process.exit(1);
}
if (!Array.isArray(evidence.plannedEffectiveStatsFields) || evidence.plannedEffectiveStatsFields.length === 0) {
  console.error("LakeCat scan replay evidence is missing plannedEffectiveStatsFields");
  process.exit(1);
}
if (evidence.plannedRequestedStatsFields.some((item) => typeof item !== "string" || item.length === 0)) {
  console.error("LakeCat scan replay evidence has invalid plannedRequestedStatsFields");
  process.exit(1);
}
if (evidence.plannedEffectiveStatsFields.some((item) => typeof item !== "string" || item.length === 0)) {
  console.error("LakeCat scan replay evidence has invalid plannedEffectiveStatsFields");
  process.exit(1);
}
if (evidence.plannedRequestedStatsFields.length <= evidence.plannedEffectiveStatsFields.length) {
  console.error("LakeCat scan replay evidence does not prove stats-field narrowing");
  process.exit(1);
}
if (!Array.isArray(evidence.fetchedRequiredFilters) || evidence.fetchedRequiredFilters.length === 0) {
  console.error("LakeCat scan replay evidence is missing fetchedRequiredFilters");
  process.exit(1);
}
function requireRestriction(value, label) {
  if (!value || typeof value !== "object") {
    console.error(`LakeCat scan replay evidence is missing ${label}`);
    process.exit(1);
  }
  if (!Array.isArray(value["allowed-columns"]) || value["allowed-columns"].length === 0) {
    console.error(`LakeCat scan replay evidence ${label} is missing allowed-columns`);
    process.exit(1);
  }
  if (!value["row-predicate"] || typeof value["row-predicate"] !== "object") {
    console.error(`LakeCat scan replay evidence ${label} is missing row-predicate`);
    process.exit(1);
  }
  requireHashArray(value["policy-hashes"], `${label} policy-hashes`);
}
requireRestriction(evidence.plannedReadRestriction, "plannedReadRestriction");
requireRestriction(evidence.fetchedReadRestriction, "fetchedReadRestriction");
if (JSON.stringify(evidence.plannedReadRestriction) !== JSON.stringify(evidence.fetchedReadRestriction)) {
  console.error("LakeCat scan replay evidence planned/fetched read restrictions do not match");
  process.exit(1);
}
if (JSON.stringify(evidence.plannedEffectiveStatsFields) !== JSON.stringify(evidence.plannedReadRestriction["allowed-columns"])) {
  console.error("LakeCat scan replay evidence plannedEffectiveStatsFields do not match allowed columns");
  process.exit(1);
}
if (JSON.stringify(evidence.plannedEffectiveProjection) !== JSON.stringify(evidence.plannedReadRestriction["allowed-columns"])) {
  console.error("LakeCat scan replay evidence plannedEffectiveProjection does not match allowed columns");
  process.exit(1);
}
if (JSON.stringify(evidence.fetchedEffectiveProjection) !== JSON.stringify(evidence.fetchedReadRestriction["allowed-columns"])) {
  console.error("LakeCat scan replay evidence fetchedEffectiveProjection does not match allowed columns");
  process.exit(1);
}
if (JSON.stringify(evidence.fetchedRequiredProjection) !== JSON.stringify(evidence.fetchedReadRestriction["allowed-columns"])) {
  console.error("LakeCat scan replay evidence fetchedRequiredProjection does not match allowed columns");
  process.exit(1);
}
const requestedProjection = new Set(evidence.plannedRequestedProjection);
for (const field of evidence.plannedEffectiveProjection) {
  if (!requestedProjection.has(field)) {
    console.error("LakeCat scan replay evidence has an effective projection field that was not requested");
    process.exit(1);
  }
}
const requestedStats = new Set(evidence.plannedRequestedStatsFields);
for (const field of evidence.plannedEffectiveStatsFields) {
  if (!requestedStats.has(field)) {
    console.error("LakeCat scan replay evidence has an effective stats field that was not requested");
    process.exit(1);
  }
}
process.stdout.write(JSON.stringify(evidence));
process.exit(0);
process.stdout.write(JSON.stringify({
  planTaskCount: evidence.planTaskCount,
  planGraphEvents: evidence.planGraphEvents,
  fileTaskCount: evidence.fileTaskCount,
  deleteFileCount: evidence.deleteFileCount,
  childPlanTaskCount: evidence.childPlanTaskCount,
  plannedReadRestriction: evidence.plannedReadRestriction,
  fetchedReadRestriction: evidence.fetchedReadRestriction,
  plannedRequestedProjection: evidence.plannedRequestedProjection,
  plannedEffectiveProjection: evidence.plannedEffectiveProjection,
  plannedRequestedStatsFields: evidence.plannedRequestedStatsFields,
  plannedEffectiveStatsFields: evidence.plannedEffectiveStatsFields,
  fetchedRequiredProjection: evidence.fetchedRequiredProjection,
  fetchedEffectiveProjection: evidence.fetchedEffectiveProjection,
  fetchedRequiredFilters: evidence.fetchedRequiredFilters,
  plannedReplayEventHashes: evidence.plannedReplayEventHashes,
  fetchedReplayEventHashes: evidence.fetchedReplayEventHashes,
  plannedOpenLineageHashes: evidence.plannedOpenLineageHashes,
  fetchedOpenLineageHashes: evidence.fetchedOpenLineageHashes,
}));
' "$file"
}

table_commit_history_evidence_json() {
  local file="$1"
  local expected_principal="$2"
  node -e '
const fs = require("fs");
const [file, expectedPrincipal] = process.argv.slice(1);
const replay = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = replay["replay-evidence"]?.tableCommitHistory;
if (!evidence || typeof evidence !== "object") {
  console.error("LakeCat replay evidence is missing tableCommitHistory");
  process.exit(1);
}
function requirePositiveInteger(value, label) {
  if (!Number.isInteger(value) || value <= 0) {
    console.error(`LakeCat table commit-history replay evidence is missing positive ${label}`);
    process.exit(1);
  }
}
function requireArray(value, label) {
  if (!Array.isArray(value) || value.length === 0) {
    console.error(`LakeCat table commit-history replay evidence is missing ${label}`);
    process.exit(1);
  }
}
function requireHashArray(value, label) {
  requireArray(value, label);
  const seen = new Set();
  for (const item of value) {
    if (typeof item !== "string" || !/^sha256:[0-9a-fA-F]{64}$/.test(item)) {
      console.error(`LakeCat table commit-history replay evidence ${label} must contain full SHA-256 hashes`);
      process.exit(1);
    }
    if (seen.has(item)) {
      console.error(`LakeCat table commit-history replay evidence ${label} must be duplicate-free`);
      process.exit(1);
    }
    seen.add(item);
  }
}
requirePositiveInteger(evidence.commitCount, "commitCount");
if (evidence.principalSubject !== expectedPrincipal) {
  console.error(`LakeCat table commit-history replay principal mismatch: expected=${expectedPrincipal} actual=${evidence.principalSubject}`);
  process.exit(1);
}
if (evidence.principalKind !== "agent") {
  console.error("LakeCat table commit-history replay evidence does not prove an agent principal");
  process.exit(1);
}
requireArray(evidence.sequenceNumbers, "sequenceNumbers");
if (evidence.sequenceNumbers.some((item) => !Number.isInteger(item) || item <= 0)) {
  console.error("LakeCat table commit-history replay evidence has invalid sequenceNumbers");
  process.exit(1);
}
requireHashArray(evidence.commitHashes, "commitHashes");
requirePositiveInteger(evidence.graphEvents, "graphEvents");
requireHashArray(evidence.replayEventHashes, "replayEventHashes");
requireHashArray(evidence.openLineageHashes, "openLineageHashes");
process.stdout.write(JSON.stringify(evidence));
process.exit(0);
process.stdout.write(JSON.stringify({
  commitCount: evidence.commitCount,
  sequenceNumbers: evidence.sequenceNumbers,
  commitHashes: evidence.commitHashes,
  principalSubject: evidence.principalSubject,
  principalKind: evidence.principalKind,
  graphEvents: evidence.graphEvents,
  replayEventHashes: evidence.replayEventHashes,
  openLineageHashes: evidence.openLineageHashes,
}));
' "$file" "$expected_principal"
}

view_receipt_chain_evidence_json() {
  local file="$1"
  node -e '
const fs = require("fs");
const [file] = process.argv.slice(1);
const replay = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = replay["replay-evidence"]?.views;
if (!evidence || typeof evidence !== "object") {
  console.error("LakeCat replay evidence is missing views");
  process.exit(1);
}
function requireHashArray(value, label) {
  if (!Array.isArray(value) || value.length === 0) {
    console.error(`LakeCat view replay evidence is missing ${label}`);
    process.exit(1);
  }
  const seen = new Set();
  for (const item of value) {
    if (typeof item !== "string" || !/^sha256:[0-9a-fA-F]{64}$/.test(item)) {
      console.error(`LakeCat view replay evidence ${label} must contain full SHA-256 hashes`);
      process.exit(1);
    }
    if (seen.has(item)) {
      console.error(`LakeCat view replay evidence ${label} must be duplicate-free`);
      process.exit(1);
    }
    seen.add(item);
  }
}
if (!Number.isInteger(evidence.viewCount) || evidence.viewCount < 0) {
  console.error("LakeCat view replay evidence is missing viewCount");
  process.exit(1);
}
if (evidence.viewCount === 0) {
  process.stdout.write(JSON.stringify({
    viewCount: 0,
    views: [],
    tombstoneReceipts: [],
    receiptChains: [],
  }));
  process.exit(0);
}
if (!Array.isArray(evidence.views) || evidence.views.length !== evidence.viewCount) {
  console.error("LakeCat view replay evidence does not match viewCount");
  process.exit(1);
}
for (const [index, view] of evidence.views.entries()) {
  if (!view || typeof view !== "object" || !view.stableId || !view.warehouse || !view.name) {
    console.error(`LakeCat view replay evidence ${index} is missing compact identity`);
    process.exit(1);
  }
  if (!Array.isArray(view.namespace) || view.namespace.length === 0) {
    console.error(`LakeCat view replay evidence ${index} is missing namespace`);
    process.exit(1);
  }
  if (!Number.isInteger(view.viewVersion) || view.viewVersion <= 0 || view.acceptedViewVersion !== view.viewVersion) {
    console.error(`LakeCat view replay evidence ${index} does not prove the accepted view version`);
    process.exit(1);
  }
  if (!view.acceptedReceiptHash) {
    console.error(`LakeCat view replay evidence ${index} is missing acceptedReceiptHash`);
    process.exit(1);
  }
  if (!view.acceptedReceiptChainHash) {
    console.error(`LakeCat view replay evidence ${index} is missing acceptedReceiptChainHash`);
    process.exit(1);
  }
  requireHashArray(view.replayEventHashes, `view ${index} replayEventHashes`);
  requireHashArray(view.openLineageHashes, `view ${index} openLineageHashes`);
}
if (!Array.isArray(evidence.tombstoneReceipts) || evidence.tombstoneReceipts.length === 0) {
  console.error("LakeCat view replay evidence is missing tombstoneReceipts");
  process.exit(1);
}
const tombstonedViews = new Set();
for (const [index, receipt] of evidence.tombstoneReceipts.entries()) {
  if (!receipt || typeof receipt !== "object" || !receipt.stableId) {
    console.error(`LakeCat view tombstone receipt evidence ${index} is missing stableId`);
    process.exit(1);
  }
  const acceptedView = evidence.views.find((view) => view.stableId === receipt.stableId);
  if (!acceptedView) {
    console.error(`LakeCat view tombstone receipt evidence ${index} does not match an accepted view`);
    process.exit(1);
  }
  if (receipt.expectedViewVersion !== acceptedView.acceptedViewVersion) {
    console.error(`LakeCat view tombstone receipt evidence ${index} does not prove the accepted expectedViewVersion`);
    process.exit(1);
  }
  tombstonedViews.add(`${receipt.stableId}\0${receipt.expectedViewVersion}`);
  requireHashArray(receipt.receiptHashes, `tombstone ${index} receiptHashes`);
  requireHashArray(receipt.replayEventHashes, `tombstone ${index} replayEventHashes`);
  requireHashArray(receipt.openLineageHashes, `tombstone ${index} openLineageHashes`);
}
if (!Array.isArray(evidence.receiptChains) || evidence.receiptChains.length === 0) {
  console.error("LakeCat view replay evidence is missing receiptChains");
  process.exit(1);
}
const verifiedChainHashes = new Set();
for (const [index, chain] of evidence.receiptChains.entries()) {
  if (!chain || typeof chain !== "object" || !chain.warehouse) {
    console.error(`LakeCat view receipt-chain evidence ${index} is missing warehouse`);
    process.exit(1);
  }
  if (!Array.isArray(chain.namespace) || chain.namespace.length === 0) {
    console.error(`LakeCat view receipt-chain evidence ${index} is missing namespace`);
    process.exit(1);
  }
  if (!Number.isInteger(chain.verifiedChainCount) || chain.verifiedChainCount <= 0) {
    console.error(`LakeCat view receipt-chain evidence ${index} is missing verifiedChainCount`);
    process.exit(1);
  }
  requireHashArray(chain.receiptHashes, `chain ${index} receiptHashes`);
  requireHashArray(chain.chainHashes, `chain ${index} chainHashes`);
  if (chain.verifiedChainCount !== chain.chainHashes.length) {
    console.error(`LakeCat view receipt-chain evidence ${index} verifiedChainCount does not match chainHashes`);
    process.exit(1);
  }
  if (chain.receiptHashes.length < chain.chainHashes.length) {
    console.error(`LakeCat view receipt-chain evidence ${index} receiptHashes do not cover chainHashes`);
    process.exit(1);
  }
  for (const chainHash of chain.chainHashes) {
    verifiedChainHashes.add(chainHash);
  }
  requireHashArray(chain.replayEventHashes, `chain ${index} replayEventHashes`);
  requireHashArray(chain.openLineageHashes, `chain ${index} openLineageHashes`);
}
for (const [index, view] of evidence.views.entries()) {
  if (!verifiedChainHashes.has(view.acceptedReceiptChainHash)) {
    if (tombstonedViews.has(`${view.stableId}\0${view.acceptedViewVersion}`)) {
      continue;
    }
    console.error(`LakeCat view replay evidence ${index} acceptedReceiptChainHash is not covered by receiptChains.chainHashes`);
    process.exit(1);
  }
}
process.stdout.write(JSON.stringify({
  viewCount: evidence.viewCount,
  views: evidence.views,
  tombstoneReceipts: evidence.tombstoneReceipts,
  receiptChains: evidence.receiptChains,
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

require_verified_table_scope() {
  local source_file="$1"
  local expected_table="$2"
  node -e '
const fs = require("fs");
const [file, expectedTable] = process.argv.slice(1);
const parsed = JSON.parse(fs.readFileSync(file, "utf8"));
const tables = parsed["verified-tables"];
if (!Array.isArray(tables) || !tables.includes(expectedTable)) {
  console.error(`Handoff summary mismatch for verified-tables: expected ${expectedTable}`);
  process.exit(1);
}
' "$source_file" "$expected_table"
}

require_verified_view_scope() {
  local source_file="$1"
  local view_receipt_chain_evidence="$2"
  node -e '
const fs = require("fs");
const [file, evidenceJson] = process.argv.slice(1);
const parsed = JSON.parse(fs.readFileSync(file, "utf8"));
const evidence = JSON.parse(evidenceJson);
const verifiedViews = parsed["verified-views"];
if (!Array.isArray(verifiedViews)) {
  console.error("Handoff summary mismatch for verified-views: missing verified-views array");
  process.exit(1);
}
for (const [index, view] of (evidence.views || []).entries()) {
  if (!view || typeof view !== "object" || !view.stableId) {
    console.error(`Handoff summary mismatch for verified-views: view evidence ${index} is missing stableId`);
    process.exit(1);
  }
  if (!verifiedViews.includes(view.stableId)) {
    console.error(`Handoff summary mismatch for verified-views: expected ${view.stableId}`);
    process.exit(1);
  }
}
' "$source_file" "$view_receipt_chain_evidence"
}

write_summary() {
  local bundle_sha drain_sha import_plan_sha lakecat_replay_sha querygraph_verify_sha querygraph_import_sha service_log_sha
  local verified_tables verified_views verified_table_ids verified_view_ids verified_warehouse bundle_hash graph_hash open_lineage_hash querygraph_import_hash
  local verified_standards
  local lakecat_schema lakecat_status lakecat_tables lakecat_views lakecat_bundle_hash lakecat_graph_hash lakecat_open_lineage_hash lakecat_querygraph_import_hash lakecat_standards lakecat_replay_evidence
  local lakecat_request_identity_evidence lakecat_querygraph_bootstrap_evidence lakecat_storage_profile_upsert_evidence lakecat_credential_vending_evidence lakecat_governed_scan_evidence lakecat_table_commit_history_evidence lakecat_management_evidence lakecat_view_receipt_chain_evidence
  local imported_tables imported_views imported_warehouse imported_bundle_hash imported_graph_hash imported_open_lineage_hash imported_querygraph_import_hash
  local imported_standards imported_table_ids imported_view_ids
  local expected_verified_table
  bundle_sha="$(sha256_file "$BUNDLE")"
  drain_sha="$(sha256_file "$DRAIN")"
  import_plan_sha="$(sha256_file "$IMPORT_PLAN")"
  lakecat_replay_sha="$(sha256_file "$LAKECAT_REPLAY_OUTPUT")"
  querygraph_verify_sha="$(sha256_file "$QUERYGRAPH_VERIFY_OUTPUT")"
  querygraph_import_sha="$(sha256_file "$QUERYGRAPH_IMPORT_OUTPUT")"
  service_log_sha="$(sha256_file "$SERVICE_LOG")"
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
  lakecat_request_identity_evidence="$(request_identity_evidence_json "$LAKECAT_REPLAY_OUTPUT" "$PRINCIPAL")"
  lakecat_storage_profile_upsert_evidence="$(storage_profile_upsert_evidence_json "$LAKECAT_REPLAY_OUTPUT")"
  lakecat_credential_vending_evidence="$(credential_vending_evidence_json "$LAKECAT_REPLAY_OUTPUT")"
  lakecat_governed_scan_evidence="$(governed_scan_evidence_json "$LAKECAT_REPLAY_OUTPUT")"
  lakecat_table_commit_history_evidence="$(table_commit_history_evidence_json "$LAKECAT_REPLAY_OUTPUT" "$PRINCIPAL")"
  lakecat_management_evidence="$(management_evidence_json "$LAKECAT_REPLAY_OUTPUT")"
  lakecat_view_receipt_chain_evidence="$(view_receipt_chain_evidence_json "$LAKECAT_REPLAY_OUTPUT")"
  verified_tables="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "table-count")"
  verified_views="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "view-count")"
  verified_table_ids="$(json_value_field "$QUERYGRAPH_VERIFY_OUTPUT" "verified-tables")"
  verified_view_ids="$(json_value_field "$QUERYGRAPH_VERIFY_OUTPUT" "verified-views")"
  bundle_hash="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "bundle-hash")"
  graph_hash="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "graph-hash")"
  open_lineage_hash="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "open-lineage-hash")"
  querygraph_import_hash="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "querygraph-import-hash")"
  verified_standards="$(json_value_field "$QUERYGRAPH_VERIFY_OUTPUT" "standards")"
  verified_warehouse="$(json_field "$QUERYGRAPH_VERIFY_OUTPUT" "warehouse")"
  lakecat_querygraph_bootstrap_evidence="$(querygraph_bootstrap_evidence_json "$LAKECAT_REPLAY_OUTPUT" "$PRINCIPAL" "$verified_tables" "$verified_views" "$bundle_hash" "$graph_hash" "$open_lineage_hash" "$querygraph_import_hash" "$verified_standards")"
  imported_tables="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "table-count")"
  imported_views="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "view-count")"
  imported_bundle_hash="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "bundle-hash")"
  imported_graph_hash="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "graph-hash")"
  imported_open_lineage_hash="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "open-lineage-hash")"
  imported_querygraph_import_hash="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "querygraph-import-hash")"
  imported_standards="$(json_value_field "$QUERYGRAPH_IMPORT_OUTPUT" "standards")"
  imported_table_ids="$(json_value_field "$QUERYGRAPH_IMPORT_OUTPUT" "verified-tables")"
  imported_view_ids="$(json_value_field "$QUERYGRAPH_IMPORT_OUTPUT" "verified-views")"
  imported_warehouse="$(json_field "$QUERYGRAPH_IMPORT_OUTPUT" "warehouse")"
  required_summary_field "warehouse" "$QUERYGRAPH_VERIFY_OUTPUT" "$verified_warehouse"
  required_summary_field "table-count" "$QUERYGRAPH_VERIFY_OUTPUT" "$verified_tables"
  required_summary_field "view-count" "$QUERYGRAPH_VERIFY_OUTPUT" "$verified_views"
  required_summary_field "verified-tables" "$QUERYGRAPH_VERIFY_OUTPUT" "$verified_table_ids"
  required_summary_field "verified-views" "$QUERYGRAPH_VERIFY_OUTPUT" "$verified_view_ids"
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
  required_summary_field "request-identity-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_request_identity_evidence"
  required_summary_field "querygraph-bootstrap-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_querygraph_bootstrap_evidence"
  required_summary_field "storage-profile-upsert-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_storage_profile_upsert_evidence"
  required_summary_field "credential-vending-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_credential_vending_evidence"
  required_summary_field "governed-scan-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_governed_scan_evidence"
  required_summary_field "table-commit-history-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_table_commit_history_evidence"
  required_summary_field "view-receipt-chain-evidence" "$LAKECAT_REPLAY_OUTPUT" "$lakecat_view_receipt_chain_evidence"
  required_summary_field "table-count" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_tables"
  required_summary_field "view-count" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_views"
  required_summary_field "warehouse" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_warehouse"
  required_summary_field "bundle-hash" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_bundle_hash"
  required_summary_field "graph-hash" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_graph_hash"
  required_summary_field "open-lineage-hash" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_open_lineage_hash"
  required_summary_field "querygraph-import-hash" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_querygraph_import_hash"
  required_summary_field "standards" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_standards"
  required_summary_field "verified-tables" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_table_ids"
  required_summary_field "verified-views" "$QUERYGRAPH_IMPORT_OUTPUT" "$imported_view_ids"
  require_field_match "warehouse" "$verified_warehouse" "$WAREHOUSE"
  require_field_match "table-count" "$imported_tables" "$verified_tables"
  require_field_match "view-count" "$imported_views" "$verified_views"
  require_field_match "import warehouse" "$imported_warehouse" "$verified_warehouse"
  require_field_match "verified-tables" "$imported_table_ids" "$verified_table_ids"
  require_field_match "verified-views" "$imported_view_ids" "$verified_view_ids"
  expected_verified_table="lakecat:table:$WAREHOUSE:$NAMESPACE:$TABLE"
  require_verified_table_scope "$QUERYGRAPH_VERIFY_OUTPUT" "$expected_verified_table"
  require_verified_table_scope "$QUERYGRAPH_IMPORT_OUTPUT" "$expected_verified_table"
  require_verified_view_scope "$QUERYGRAPH_VERIFY_OUTPUT" "$lakecat_view_receipt_chain_evidence"
  require_verified_view_scope "$QUERYGRAPH_IMPORT_OUTPUT" "$lakecat_view_receipt_chain_evidence"
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
    "verifiedTables": $verified_table_ids,
    "verifiedViews": $verified_view_ids,
    "bundleHash": "$(json_string "$bundle_hash")",
    "graphHash": "$(json_string "$graph_hash")",
    "openLineageHash": "$(json_string "$open_lineage_hash")",
    "querygraphImportHash": "$(json_string "$querygraph_import_hash")",
    "standards": $verified_standards
  },
  "querygraphImportVerification": {
    "matchesVerify": true,
    "tableCount": $imported_tables,
    "viewCount": $imported_views,
    "verifiedTables": $imported_table_ids,
    "verifiedViews": $imported_view_ids,
    "bundleHash": "$(json_string "$imported_bundle_hash")",
    "graphHash": "$(json_string "$imported_graph_hash")",
    "openLineageHash": "$(json_string "$imported_open_lineage_hash")",
    "querygraphImportHash": "$(json_string "$imported_querygraph_import_hash")",
    "standards": $imported_standards
  },
  "lakecatReplayVerification": {
    "schemaVersion": "$(json_string "$lakecat_schema")",
    "status": "$(json_string "$lakecat_status")",
    "matchesQueryGraph": true,
    "requestIdentityProof": $lakecat_request_identity_evidence,
    "queryGraphBootstrapProof": $lakecat_querygraph_bootstrap_evidence,
    "governedScanProof": $lakecat_governed_scan_evidence,
    "tableCommitHistoryProof": $lakecat_table_commit_history_evidence,
    "managementProof": $lakecat_management_evidence,
    "viewReceiptChainProof": $lakecat_view_receipt_chain_evidence,
    "storageProfileUpsertProof": $lakecat_storage_profile_upsert_evidence,
    "credentialVendingProof": $lakecat_credential_vending_evidence,
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
    "lakecatHandoffVerifyOutput": "$(json_string "$LAKECAT_HANDOFF_VERIFY_OUTPUT")",
    "querygraphVerifyOutput": "$(json_string "$QUERYGRAPH_VERIFY_OUTPUT")",
    "querygraphImportOutput": "$(json_string "$QUERYGRAPH_IMPORT_OUTPUT")",
    "capturedOutputs": {
      "lakecatReplay": {
        "path": "$(json_string "$LAKECAT_REPLAY_OUTPUT")",
        "sha256": "sha256:$lakecat_replay_sha"
      },
      "querygraphVerify": {
        "path": "$(json_string "$QUERYGRAPH_VERIFY_OUTPUT")",
        "sha256": "sha256:$querygraph_verify_sha"
      },
      "querygraphImport": {
        "path": "$(json_string "$QUERYGRAPH_IMPORT_OUTPUT")",
        "sha256": "sha256:$querygraph_import_sha"
      }
    },
    "serviceLog": "$(json_string "$SERVICE_LOG")",
    "serviceLogHash": "sha256:$service_log_sha"
  }
}
JSON
}

bind_handoff_verify_output_hash() {
  local handoff_verify_sha
  handoff_verify_sha="$(sha256_file "$LAKECAT_HANDOFF_VERIFY_OUTPUT")"
  node -e '
const fs = require("fs");
const [summaryFile, hash] = process.argv.slice(1);
const summary = JSON.parse(fs.readFileSync(summaryFile, "utf8"));
if (!summary.artifacts || typeof summary.artifacts !== "object") {
  console.error("Handoff summary is missing artifacts before binding verifier output hash");
  process.exit(1);
}
summary.artifacts.lakecatHandoffVerifyOutputHash = hash;
fs.writeFileSync(summaryFile, `${JSON.stringify(summary, null, 2)}\n`);
' "$SUMMARY" "sha256:$handoff_verify_sha"
}

if [[ ! -f "$QUERYGRAPH_RUST_DIR/Cargo.toml" ]]; then
  echo "QueryGraph Rust crate not found at $QUERYGRAPH_RUST_DIR" >&2
  exit 1
fi

mkdir -p "$RUN_DIR"
rm -f "$BUNDLE" "$DRAIN" "$IMPORT_PLAN" "$SUMMARY" \
  "$LAKECAT_REPLAY_OUTPUT" "$LAKECAT_HANDOFF_VERIFY_OUTPUT" "$LAKECAT_HANDOFF_SELF_VERIFY_OUTPUT" "$QUERYGRAPH_VERIFY_OUTPUT" "$QUERYGRAPH_IMPORT_OUTPUT" \
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
cargo run -p lakecat-cli --features qglake-fixture -- qglake-fixture \
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

echo "Verifying LakeCat handoff summary"
cargo run -p lakecat-cli -- qglake-verify-handoff \
  --summary "$SUMMARY" \
  --json \
  | tee "$LAKECAT_HANDOFF_VERIFY_OUTPUT"

bind_handoff_verify_output_hash

echo "Re-verifying LakeCat handoff summary with verifier-output artifact hash"
cargo run -p lakecat-cli -- qglake-verify-handoff \
  --summary "$SUMMARY" \
  --json \
  | tee "$LAKECAT_HANDOFF_SELF_VERIFY_OUTPUT"

echo "QGLake handoff verified"
echo "  bundle:      $BUNDLE"
echo "  drain:       $DRAIN"
echo "  import plan: $IMPORT_PLAN"
echo "  summary:     $SUMMARY"
