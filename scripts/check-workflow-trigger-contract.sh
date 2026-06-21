#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
contract_script="$repo_root/scripts/check-local-dependency-contract.sh"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

write_workflow() {
  local name="$1"
  local body="$2"
  rm -rf "$tmp_dir/workflows"
  mkdir -p "$tmp_dir/workflows"
  printf '%s\n' "$body" > "$tmp_dir/workflows/$name.yml"
}

expect_accept() {
  local name="$1"
  local body="$2"
  write_workflow "$name" "$body"
  LAKECAT_CONTRACT_CHECK_ONLY=workflows \
    LAKECAT_WORKFLOW_DIR="$tmp_dir/workflows" \
    "$contract_script" >/dev/null
}

expect_reject() {
  local name="$1"
  local body="$2"
  write_workflow "$name" "$body"
  if LAKECAT_CONTRACT_CHECK_ONLY=workflows \
    LAKECAT_WORKFLOW_DIR="$tmp_dir/workflows" \
    "$contract_script" >/dev/null 2>"$tmp_dir/error.log"; then
    echo "workflow trigger contract accepted an automatic trigger in $name" >&2
    exit 1
  fi
  rg -q "must not" "$tmp_dir/error.log"
}

expect_accept "manual" 'name: CI
on:
  workflow_dispatch:'

expect_accept "job-named-push" 'name: CI
on:
  workflow_dispatch:
jobs:
  push:
    runs-on: ubuntu-latest
    steps:
      - run: echo local gate'

expect_accept "nested-job-step-event-names" 'name: CI
on:
  workflow_dispatch:
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - name: Mention pull_request safely
        run: echo "push pull_request schedule are inert outside on"'

expect_reject "compact-unquoted-event" 'name: CI
on: push'

expect_reject "compact-quoted-event" 'name: CI
on: "pull_request"'

expect_reject "compact-single-quoted-event" "name: CI
on: 'merge_group'"

expect_reject "compact-quoted-on" 'name: CI
"on": ["push"]'

expect_reject "inline-list-unquoted-event" 'name: CI
on: [workflow_call]'

expect_reject "inline-list-quoted-event" 'name: CI
on: ["schedule"]'

expect_reject "block-map-event" 'name: CI
on:
  push:
    branches: [main]'

expect_reject "block-map-quoted-event" 'name: CI
on:
  "pull_request_target":
    branches: [main]'

expect_reject "block-map-single-quoted-event" "name: CI
on:
  'repository_dispatch':
    types: [deploy]"

expect_reject "block-quoted-event" 'name: CI
"on":
  - "pull_request"'

expect_reject "block-single-quoted-event" "name: CI
'on':
  - 'workflow_call'"

expect_reject "inline-map-quoted-event" 'name: CI
on: {"workflow_run": {}}'

expect_reject "inline-map-unquoted-event" 'name: CI
on: {merge_group: {}}'

echo "LakeCat workflow trigger contract self-test passed."
