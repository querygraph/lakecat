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

## C3-02 — commit ambiguity, exact retry, and restart recovery

Accepted catalog-bench revision:

- `catalog-bench@c7eb664` publishes the v2 standard Iceberg REST recovery
  scenario, deterministic in-flight request gate, fresh four-catalog runner,
  raw sanitized `restart_0828d` evidence, review record, and reproduction guide.

Every catalog passed both accepted-state ambiguity cases. A disconnect before
upstream admission left the property absent and exact retry returned 200. A
disconnect after upstream HTTP 200 left the property accepted; exact retry was
safe and the accepted property remained unchanged.

For restart, the proxy transmitted one request-body byte, recorded the
sanitized `during-upstream` event, paused the remainder, restarted only the
target catalog service, and released the old connection. Each interrupted
request returned HTTP 502 and no partial property mutation was observed.

| Catalog | Fixture after restart | Exact retry | Result |
| --- | --- | --- | --- |
| LakeCat | present | HTTP 200 | pass |
| Polaris | absent | HTTP 500 | fail in the benchmark's ephemeral persistence configuration |
| Gravitino | present | HTTP 200 | pass |
| Lakekeeper | present | HTTP 200 | pass |

Lakekeeper alone advertised idempotency. Its exact replay returned 200. A
same-key/different-content request also returned cached 200 without mutating
state; this is retained as a content-binding defect and is not described as
exactly-once behavior. Polaris's result is scoped to the benchmark's ephemeral
server configuration and does not claim behavior for an external database.

The reviewed summary hash is
`sha256:7083e202a2a4a1ad6d44faf01e51684b3a0c028d4b1ce538c643736b8cad76dc`.
Per-catalog artifact hashes are:

- LakeCat: `sha256:632b7352472dfaa3d126ecd17b7ba6c2635ad83fff8d47cfb9f123d7f01d5b23`
- Polaris: `sha256:61bfab91797cdb7d1ced131259766502f8dd5d491e63aa0c8082e1fea2442930`
- Gravitino: `sha256:5556685c8572468359258c86db83acf950e02fed62d18be5c6c1540dec6387f7`
- Lakekeeper: `sha256:3ce4f86e612d97dfdd89c3594047bb7d2358bfc357efd72b261e4f403b716739`

The runner removed every run container and volume. This closes C3-02 only; it
does not prove state-store/outbox failure, rolling restart, backup/restore,
migration, federation, or performance.
