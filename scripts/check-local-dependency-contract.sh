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

require_pattern 'grust-graph = \{ package = "grust-graph", path = "../grust/crates/grust", version = "0\.9\.0",' Cargo.toml \
  "grust-graph must keep a versioned local path dependency on ../grust/crates/grust"
require_pattern 'typesec = \{ path = "../typesec/crates/typesec", version = "0\.8\.0",' Cargo.toml \
  "typesec must keep a versioned local path dependency on ../typesec/crates/typesec"
require_pattern 'sail-catalog = \{ path = "../sail/crates/sail-catalog" \}' Cargo.toml \
  "sail-catalog must stay an explicit local Sail path until the needed Sail API is published"
require_pattern 'sail-common-datafusion = \{ path = "../sail/crates/sail-common-datafusion" \}' Cargo.toml \
  "sail-common-datafusion must stay an explicit local Sail path until the needed Sail API is published"

for sibling in ../grust/crates/grust ../typesec/crates/typesec ../sail/crates/sail-catalog ../sail/crates/sail-common-datafusion; do
  require_dir "$sibling"
done

for patch in \
  ci/sail-patches/0001-Expose-Iceberg-table-status-conversion.patch \
  ci/sail-patches/0002-Expose-Iceberg-planning-result-helpers.patch \
  ci/sail-patches/0003-Expose-Iceberg-generated-model-module.patch
do
  require_file "$patch"
done

require_pattern 'git -C sail' .github/workflows/ci.yml \
  "manual CI must apply the LakeCat Sail helper patches"
require_pattern 'ci/sail-patches' .github/workflows/ci.yml \
  "manual CI must reference ci/sail-patches"
require_pattern 'repository: querygraph/grust' .github/workflows/ci.yml \
  "manual CI must check out the Grust sibling repository"
require_pattern 'repository: querygraph/typesec' .github/workflows/ci.yml \
  "manual CI must check out the TypeSec sibling repository"
require_pattern 'repository: lakehq/sail' .github/workflows/ci.yml \
  "manual CI must check out the Sail sibling repository"

cargo metadata --format-version 1 --no-deps > /tmp/lakecat-dependency-contract-metadata.json
require_pattern '"name":"grust-graph".*"req":"\^0\.9\.0".*"path":"/.*/grust/crates/grust"' /tmp/lakecat-dependency-contract-metadata.json \
  "cargo metadata must resolve grust-graph to the local path with version requirement ^0.9.0"
require_pattern '"name":"typesec".*"req":"\^0\.8\.0".*"path":"/.*/typesec/crates/typesec"' /tmp/lakecat-dependency-contract-metadata.json \
  "cargo metadata must resolve typesec to the local path with version requirement ^0.8.0"

echo "LakeCat local dependency contract is intact."
