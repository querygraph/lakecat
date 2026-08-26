# Catalog Community Phase 1 Acceptance Ledger

This ledger accumulates evidence for Phase 1, **Lakekeeper and behavioral
conformance**. It is deliberately incremental. A completed row proves only its
named backlog unit; Phase 1 remains open until C1-01 through C1-10 and the phase
exit criteria in `CATALOG-COMMUNITY-PLAN.md` are satisfied.

No timing in this ledger is public benchmark evidence. The current candidate
profile remains `draft`, and C1-09 still owns optimized production builds,
same-Docker execution, immutable raw bundles, generated reports, and secret
scanning.

## C1-01 — owned Lakekeeper infrastructure

Accepted catalog-bench revisions:

- [`dea3c575ec8953cfb4b84de9272207b3a58944ff`](https://github.com/querygraph/catalog-bench/commit/dea3c575ec8953cfb4b84de9272207b3a58944ff)
  owns the Docker network/object store/state topology and Lakekeeper readiness
  chain;
- [`bd8171340a43c3ff8e1c6c29ae945d9bb2549f3f`](https://github.com/querygraph/catalog-bench/commit/bd8171340a43c3ff8e1c6c29ae945d9bb2549f3f)
  makes the clean-worktree contract gates independent of stale Cargo target
  paths.

The implementation establishes this dependency chain on one Compose-owned
`catalog-bench-net` bridge:

```text
PostgreSQL healthy -> Lakekeeper migration exit 0 -> Lakekeeper healthy
                                                               |
MinIO healthy -> bucket initializer exit 0 --------------------+
                                                               v
                                            bootstrap reconciler exit 0
                                                               v
                                            warehouse reconciler exit 0
                                                               v
                                      Iceberg config readiness exit 0
```

The pinned inputs are:

| Component | Exact identity |
| --- | --- |
| MinIO | `RELEASE.2025-10-15T17-29-55Z`, source `9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a` |
| Go build image | `golang:1.24.8-bookworm@sha256:4ed690d6649d63c312b99a6120025ec79ce3b542968a37da53d6236c7c61a848` |
| Lakekeeper | `quay.io/lakekeeper/catalog:v0.13.3@sha256:db2ba6168eb107f22242fb7f2edc4016fa35e57bdcc606894e809c418e32e8dc` |
| PostgreSQL | `postgres:17.11-bookworm@sha256:051f7b7b3abdd564d5d1bd1e8c4b9c1b6e77087d1dd22020ede611c096a272e0` |

MinIO is built from the exact fetched tag only after verifying its commit. Its
binary reported the expected release, commit, Go 1.24.8 toolchain, Linux OS, and
ARM64 architecture. MinIO publishes no host port. Lakekeeper's state uses a
dedicated PostgreSQL process, role, database, and named volume; MinIO has a
separate named volume. The fixture warehouse is `bench` at
`s3://warehouse/lakekeeper` in the shared `warehouse` bucket.

### State reconciliation

The setup path does not equate an arbitrary HTTP conflict with success.
Typed Go helpers:

- validate environment endpoints without permitting embedded credentials;
- idempotently create the MinIO bucket and re-probe a concurrent create;
- read Lakekeeper's management information, verify version `0.13.3`, and
  bootstrap only an uninitialized server;
- compare an existing named warehouse's project, status, S3 endpoint, bucket,
  prefix, region, path-style mode, STS mode, flavor, and credential type with the
  checked-in request;
- fail closed on warehouse configuration drift; and
- negotiate `GET /catalog/v1/config?warehouse=bench`, requiring a nonempty
  prefix and advertised standard config endpoint.

The helpers and their tests compile inside the pinned Go build container. The
final scratch image contains MinIO, CA roots, the MinIO license, and only the
three typed setup/readiness binaries.

### Live Docker evidence

The first live run used an empty, run-specific Compose project named
`catalog-bench-c1-01-smoke`. Its `minio-data` and
`lakekeeper-postgres-data` volumes carried that exact project label. All eight
states matched their contract:

| Service | Observed state |
| --- | --- |
| `minio` | running, healthy |
| `minio-init` | exited 0 |
| `postgresql` | running, healthy |
| `lakekeeper-migrate` | exited 0 |
| `lakekeeper` | running, healthy |
| `lakekeeper-bootstrap` | exited 0 |
| `lakekeeper-warehouse` | exited 0 |
| `lakekeeper-ready` | exited 0 |

The host inspection endpoint also returned HTTP 200 for
`/catalog/v1/config?warehouse=bench`. The temporary smoke project and only its
two test volumes were removed after verification; the normal project's persisted
volumes were preserved and restored. A second execution against that persisted
state produced the same eight successful states, proving repeatability without
recreating accepted catalog configuration.

### Static and repository gates

The accepted working tree passed:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked

(cd docker/minio/tools && \
  test -z "$(gofmt -l .)" && \
  go mod tidy -diff && \
  go vet ./... && \
  go test ./...)

docker compose \
  --profile lakekeeper \
  --profile nessie \
  --profile polaris \
  --profile gravitino \
  --profile bench \
  config --quiet

cargo run -p catalog-bench-contract --locked -- schemas check
cargo run -p catalog-bench-contract --locked -- validate \
  profiles/v1 scenarios/v1 results/v1
cargo run -p catalog-bench-contract --locked -- historical-import check --root .
cargo run -p catalog-bench-contract --locked -- bundle validate \
  --manifest results/v1/2026-08-08/manifest.json
cargo run -p catalog-bench-contract --locked -- matrix check \
  --manifest results/v1/2026-08-08/manifest.json \
  --output results/v1/2026-08-08/MATRIX.md
```

This closes C1-01 only. Adapter capability declarations and endpoint validation
begin at C1-02; no optional catalog is considered conformant merely because its
container starts.
