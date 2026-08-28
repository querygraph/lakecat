# Catalog Community Phase 3 Acceptance Ledger

This ledger accumulates evidence for Phase 3, **failure, recovery, and
migration**. A completed row proves only its named backlog unit. The phase does
not close until C3-01 through C3-06 and the Phase 3 exit gates are complete.

## C3-01 — deterministic network and object-store faults

Accepted catalog-bench revision:

- `catalog-bench@1633d30` publishes the deterministic proxy, isolated Compose
  overlay, neutral scenario, runnable Linux ARM64 profile, one-command fresh
  runner, reviewed source evidence, and reproduction guide.

The proxy has distinct `before-upstream` and `after-upstream` disconnect
phases, occurrence matching, and a bounded injection count that keeps automatic
client retries observable. Its control and evidence schemas reject unknown or
invalid rules. Evidence retains the method, rule identity, phase, match number,
upstream status when one exists, and a path hash; it does not retain headers,
queries, bodies, signed URLs, credentials, or raw metadata paths.

Fresh run `objfault_0828a` used a signed S3 metadata PUT and direct MinIO state
observation:

| Case | Client | Proxy observation | Direct object state |
| --- | --- | --- | --- |
| Before upstream | disconnected | no upstream status | absent |
| After upstream | disconnected | HTTP 200 | present |

Both cases used content hash
`sha256:9c6a95372144d03bb2f58ebf9dc3049576560f9b6811787e3eb3baec95f02f61`.
The reviewed summary hash is
`sha256:350c5a2e9d2c61b6e7b19c51bc0d306d5a24fecd8f0fc1259265caee14cb3faf`.
The runner removed both objects, every run container, and every run volume.

The exact accepted command was:

```sh
docker/run-object-faults.sh objfault_0828a
```

Static and contract verification included:

```sh
(cd docker/minio/tools && \
  test -z "$(gofmt -l .)" && \
  go mod tidy -diff && \
  go vet ./... && \
  go test ./...)

cargo fmt --all -- --check
cargo test -p catalog-bench-common --locked
cargo run -p catalog-bench-contract --locked -- validate \
  profiles/v1/object-faults-2026-08-28.json \
  scenarios/v1/object-store.metadata-persistence-faults.json
docker compose -f docker-compose.yml -f docker-compose.fault.yml \
  --profile lakekeeper --profile polaris --profile gravitino config --quiet
git diff --check
```

The overlay provides one object-store proxy and one REST proxy for LakeCat,
Polaris, Gravitino, and Lakekeeper without changing the already-published
correctness topology. This closes the injection substrate, not catalog recovery
or performance. C3-02 and C3-04 own the catalog-specific behavior.
