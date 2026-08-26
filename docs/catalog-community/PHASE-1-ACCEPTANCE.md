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

## C1-02 — typed catalog adapters and capability coverage

Accepted catalog-bench revision:

- [`7269f8853bf1afe0902c663f0ef9bd9a0902f920`](https://github.com/querygraph/catalog-bench/commit/7269f8853bf1afe0902c663f0ef9bd9a0902f920)
  adds the adapter contract, current bindings, schema, tests, and documentation.

The profile contract now models adapters as closed Rust algebraic data types
rather than launcher conditionals. Each binding identifies its catalog component,
Iceberg REST protocol, Docker-network base URL, `/v1/config` request, static,
unprefixed, or negotiated route prefix, authentication mode, optional standard
`createTable.location`, request-handling mode, and capability disposition.

The current candidate contains exact bindings for LakeCat, Apache Polaris,
Apache Gravitino, Lakekeeper, and Apache Nessie. Every catalog component has
exactly one matching `iceberg-rest-catalog` service endpoint and every adapter is
`protocol-native`. Lakekeeper selects `warehouse=bench` and resolves the returned
`/defaults/prefix`; Polaris uses the static `bench` prefix and its OAuth2
client-credentials route; Nessie uses `main`; LakeCat and Gravitino use the
unprefixed route. Only LakeCat supplies the standard optional create-table
location needed to target its shared MinIO prefix.

### Capability and shim semantics

The profile defines 27 Phase 1 capabilities once. `exercise-all` is the compact
exhaustive disposition used by all five adapters. It means the harness must send
the standard request and let scenario assertions decide pass or fail; it is not a
claim that the catalog supports or will pass the operation. When evidence proves
an optional capability absent before execution, a new profile can use the
`explicit` variant to partition exercised and unsupported operations with
catalog-or-adapter attribution, explanation, and an optional upstream reference.
An attempted failure cannot be relabeled as unsupported afterward.

No current adapter rewrites a request, response, status, error, metadata document,
or advertised endpoint. The contract can represent a behavior-changing shim only
by naming a separately pinned connector component and explaining the mutation;
that path cannot masquerade as no-shim compatibility evidence.

### Validation evidence

Semantic and integration tests reject missing or duplicate adapters, non-catalog
targets, missing or duplicate service bindings, service/adapter endpoint drift,
credential-bearing or malformed URLs, secret-shaped config keys, invalid static
or negotiated prefixes, missing/undefined/overlapping capability declarations,
and undisclosed or incorrectly typed shim components. The accepted revision
passed:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked

cargo run -p catalog-bench-contract --locked -- schemas check
cargo run -p catalog-bench-contract --locked -- validate \
  profiles/v1 scenarios/v1 results/v1
cargo run -p catalog-bench-contract --locked -- historical-import check --root .
cargo run -p catalog-bench-contract --locked -- bundle validate \
  --manifest results/v1/2026-08-08/manifest.json
cargo run -p catalog-bench-contract --locked -- matrix check \
  --manifest results/v1/2026-08-08/manifest.json \
  --output results/v1/2026-08-08/MATRIX.md

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
```

The historical reproduction profile and its result bundle remained byte-for-byte
unchanged. C1-02 is static adapter acceptance, not live behavioral evidence. C1-03
owns config negotiation, endpoint-advertisement assertions, and sanitized HTTP
transcripts; the candidate profile remains `draft` and contains no new timings.

## C1-03 — config negotiation and endpoint advertisement

Accepted revisions:

- [`catalog-bench@feb803f83135fc2814a56a1859fafd20af6561d0`](https://github.com/querygraph/catalog-bench/commit/feb803f83135fc2814a56a1859fafd20af6561d0)
  implements the scenario contract, production runner, same-Docker topology,
  sanitization boundary, readiness helpers, and tests;
- [`catalog-bench@ec3e40d6b530c6dae32d0fe8bddce55b02e874d3`](https://github.com/querygraph/catalog-bench/commit/ec3e40d6b530c6dae32d0fe8bddce55b02e874d3)
  pins the implementation and independently accepted LakeCat revision in the
  current candidate profile; and
- [`lakecat@09dd7ee358e79d4acd04b5732e61e0330b7c1c97`](https://github.com/querygraph/lakecat/commit/09dd7ee358e79d4acd04b5732e61e0330b7c1c97)
  corrects LakeCat's config advertisement and resolves the Rust 1.97
  warning-denying production-gate debt exposed while preparing the run.

The exact contract inputs are:

| Input | Identity |
| --- | --- |
| Candidate profile | `catalog-community-current-2026-08-26-linux-arm64` |
| Profile SHA-256 | `c162a1a3ccd71cc0e5b75b33fa10f8f80a183cf1c3be0a4460e453cf6577f3c7` |
| Scenario | `iceberg-rest.config.negotiation` version 1, classification `strict-v1` |
| Scenario SHA-256 | `c1352f1f10c5e8b3853fa45a12c2edc3dd42c1be1be4fae113c7789a2678574f` |
| Apache Iceberg contract | 1.11.0 REST OpenAPI SHA-256 `80d2ec83a70eeff6e7194853f8791c17cceb14610fae6a0e6afdd2921806ee4a` |

The profile timestamp was corrected from a future local time to the actual
source-resolution time, `2026-08-26T06:29:17-04:00`. Its readiness remains
`draft`. The commit driver and conformance runner are modeled as distinct
source-build components and services so a future bundle cannot omit either
executable's provenance.

### Strict scenario and evidence semantics

The runner projects the selected catalog adapter directly from the validated
profile. It performs anonymous or OAuth2 client-credentials authentication,
sends the declared `GET /v1/config` request, resolves unprefixed, static, or
negotiated warehouse routing, and evaluates seven required assertions:

1. authentication is ready;
2. config returns HTTP 200;
3. the response media type is `application/json`, with parameters allowed;
4. required `defaults` and `overrides` members are JSON string maps;
5. the route prefix resolves exactly as the adapter declares;
6. every explicitly advertised method/path is an Apache Iceberg 1.11 REST
   operation, or omission is recorded without inventing a universal client
   default; and
7. the persisted transcript is sanitized.

The endpoint check is exact and catalog-neutral. It neither accepts proprietary
routes as Iceberg routes nor penalizes a server for omitting the optional
`endpoints` member. The runner performs no request or response rewrite, and an
attempted failure cannot be relabeled as unsupported after execution.

HTTP evidence is bounded to 1 MiB before bytes are admitted to the response
buffer. Request and response headers use allowlists; authorization, cookies,
URL credentials/fragments/secret query values, OAuth client values and bearer
tokens are never persisted. JSON response bodies are recursively sanitized for
secret-shaped keys, including client identifiers. A raw-body hash is retained
only when the body is valid JSON and no redaction changed it. Output uses
create-new semantics so a rerun cannot overwrite prior evidence. Transport and
setup errors are body-safe and credential-safe as well.

The transcript format, `catalog-bench/config-transcript/v1`, is deliberately an
intermediate evidence format rather than a `catalog-bench/v1` result. A smoke
transcript cannot enter a public ranking by itself.

### LakeCat correction

The pre-probe audit found that LakeCat's config response mixed Iceberg routes
with mount aliases, management endpoints, compatibility aliases, and QueryGraph
control-plane routes. That over-advertisement was a LakeCat defect, not something
the harness could normalize away.

`lakecat@09dd7ee3` replaces those variants with one shared, duplicate-free list
of the 13 Apache Iceberg 1.11 operations actually implemented by the production
service. Config-read replay, the QGLake handoff, CLI proof verification, tests,
design documentation, and the book now all require that exact list. LakeCat's
management and QueryGraph APIs remain available through their owning responses
and events; they no longer masquerade as Iceberg endpoint advertisements.

The same revision replaces long CLI proof-helper parameter lists with typed
fixture, catalog-context, artifact-bundle, and receipt-chain abstractions; names
the TypeSec environment-reader boundary; removes stale feature-gated Sail
imports; and combines equivalent evidence branches. These changes restore
strict Rust 1.97 gates without weakening readability, modularity, or behavior.

### Optimized same-Docker build proof

All three local images were rebuilt from source immediately before the accepted
probe. No host-staged executable was copied into an image. Both Rust builds used
Rust 1.97.1, `aarch64-unknown-linux-gnu`, locked dependencies, release opt-level
3, fat LTO, one codegen unit, disabled incremental/debug data, symbol stripping,
`panic=abort`, one build job, `-Dwarnings`, and `-Ctarget-cpu=native`. LakeCat
enabled the production `turso-local,sail-local` feature set. MinIO and its typed
setup helpers used Go 1.24.8, `CGO_ENABLED=0`, `linux/arm64`, `-trimpath`, and the
verified upstream release linkage.

These identities describe the accepted local smoke build. BuildKit's OCI index
includes its provenance attestation; the Linux ARM64 manifest and executable
hashes identify the runnable payload independently.

| Image | OCI index / Linux ARM64 manifest | Runtime executable SHA-256 |
| --- | --- | --- |
| `catalog-bench-commit:latest` | `sha256:55ae114674d91c631a0384b527abe9e2fc4dba8d3a893aef37a80909a9afd887` / `sha256:d0f120244857f86162c80d8990402534030fc51ee8a8aeaaddf58bd34e32340c` | commit driver `ffddd28c0b61ad1ff02c26ad594f07db88fa3e2f202ccbe7866a2f9fc6a9c766`; conformance runner `53d55ccb4cf1661caaee61387079f60de10d764c9af595a71a590490068de904` |
| `lakecat-service:bench` | `sha256:fe67f45528825793219e41ec11be010bbc6aa966c882d3c2097a2dc9abe22ab0` / `sha256:9670c1923a6c75d40977fd5d59d62d10dd303d8301fc81262fe47c026485cb82` | `617eb5068512f52382ad3236680ce3b75003c48e97f3ba076be34e3fa0f7dcdb` |
| `catalog-bench/minio:RELEASE.2025-10-15T17-29-55Z` | `sha256:28c9405d4591b7803c8cf79afcef6a32f8fe9964982e5075babcb6a1c7ddecdb` / `sha256:3cce4c4b2a32fd55e96c0399daf4d00fabab1d76610e5b041ec4ecfe559a53e5` | MinIO `16020fd2829fb8f23b29b2d108b35bfecfd73aa9ada05d499939bfb59abbe582` |

The MinIO binary reports release `RELEASE.2025-10-15T17-29-55Z`, commit
`9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a`, Go 1.24.8, Linux, and ARM64.
The same source-built image contains the typed helper payloads below:

| Helper | SHA-256 |
| --- | --- |
| `ensure-bucket` | `e0a4e76352655d936f37a6e0fe8c908e211dc00a50c0d83f1e98e17841a33363` |
| `healthcheck` | `70c8d0120ce907460280c69f2116ed52cd283fa060efbdb996e5eee0556b0d9a` |
| `lakekeeper-setup` | `cc9141864a5e062c69bf689c7434720cad7745dddfccdc5917f6285e00642b13` |
| `polaris-setup` | `e3f62aec72ec5d8f6c10b4605a480a07cc7c4648f5bacf95d7369a1a171c2bcd` |
| `wait-http` | `0e0a1db1c8e4cf33e972ceb51895680a98638c9dcf3bce04f01fa85a3665852d` |

C1-09 will materialize the final publishable artifact identities; these smoke
image identities do not prematurely make the draft profile runnable.

The rebuilt MinIO and LakeCat containers were force-recreated while preserving
their named volumes. The MinIO bucket initializer then exited 0. LakeCat became
healthy, and the rebuilt helper image replayed all idempotent gates:

- Lakekeeper bootstrap, warehouse reconciliation, and Iceberg config readiness;
- Polaris catalog reconciliation, OAuth2 negotiation, and Iceberg config
  readiness; and
- Nessie and Gravitino Iceberg config readiness.

Every gate exited 0. Catalog processes, object storage, setup tools, and every
probe shared the Compose-owned `catalog-bench-net`; no host route participated in
the measured request path.

### Live config outcomes

Each catalog was probed by a fresh conformance container from the optimized
image. All transcripts carry the exact profile and scenario hashes above, the
selected adapter identity, seven assertion outcomes, `raw_secrets_persisted:
false`, and `raw_response_body_persisted: false`.

| Catalog | Config result | Routing / advertisement | Transcript SHA-256 |
| --- | --- | --- | --- |
| Apache Gravitino 1.3.0 | `pass`, HTTP 200, 7/7 required assertions | unprefixed / explicit standard endpoints | `d5a86a62b65d21f775bc2b60963c58403260c687246afc517eccc2c306b7fb2a` |
| LakeCat 0.3.0-20-g09dd7ee3 | `pass`, HTTP 200, 7/7 required assertions | unprefixed / exact 13-route advertisement | `bf66e4ec6dffbfce84c27c6476dbb43e16b601dc2e5eb30cb4058035fa835076` |
| Lakekeeper 0.13.3 | `pass`, HTTP 200, 7/7 required assertions | config-negotiated warehouse prefix / explicit standard endpoints | `a6868cfb601589f4ceb5827bb6a1fc1993af39f425d1cd41b20bf318edd585aa` |
| Apache Nessie 0.108.4 | `pass`, HTTP 200, 7/7 required assertions | static `main` prefix / explicit standard endpoints | `5226dd522a88720d910f42b05132d58aa0195ba31e35435dbf2e0d4d932dd0a1` |
| Apache Polaris 1.7.0 | `fail`, HTTP 200, 6/7 required assertions | static `bench` prefix / invalid proprietary additions | `18cc1e0df6b896ee8ddc9197637fbd95308375ee168730fdfddab4d72f7358e8` |

The four passing probes exited 0. Polaris exited the runner's reserved
conformance-failure status 2. Independent validation proved that its only failed
assertion is `endpoint-advertisement-valid`; authentication, HTTP status, media
type, config map shape, prefix resolution, and transcript sanitization all pass.
A scan of every transcript found none of the fixed fixture credentials, bearer
authorization, client-secret keys, or access-key material.

#### Why Polaris fails this scenario

Polaris successfully issues an OAuth token and returns a valid HTTP 200 config
response for warehouse `bench`. Its explicit `endpoints` array then appends
Polaris-only generic-table and policy routes under `polaris/v1/...` and
`/polaris/v1/...`. The first rejected entry is
`GET polaris/v1/{prefix}/namespaces/{namespace}/generic-tables`, which is not a
method/path operation in the pinned Apache Iceberg 1.11 OpenAPI document.

The result is therefore a narrow, deterministic standards-conformance failure,
not an availability, authentication, config-shape, or general Polaris failure.
The harness preserves the response and reports the failed assertion instead of
silently deleting proprietary routes or treating them as standard Iceberg.

#### Why Nessie passes here but remains unranked historically

Nessie's C1-03 result tests config negotiation, not concurrent table commits. It
returns HTTP 200, resolves the declared static `main` prefix, advertises only
recognized Iceberg operations, and passes all seven config assertions. This
proves the current Nessie service is reachable and correctly negotiates config.

It does not supersede the preserved 2026-08-08
`iceberg-rest.commit.same-table-contention` result. In that different scenario,
Nessie returned 97 non-conflict HTTP 500 responses across five measured rounds,
failed `zero-request-errors`, and completed 0/5 valid rounds. The earlier public
row had discarded non-409 request errors; the strict driver made the existing
load-sensitive failure observable. Config conformance and concurrent-commit
correctness are independent claims, so the ledger retains both outcomes without
generalizing either one.

### Repository and documentation gates

catalog-bench passed the full Rust workspace tests with all features and targets,
strict Clippy, warning-denied Rustdoc, formatting, and diff checks. Focused suites
included 21 common contract tests, 14 conformance integration tests, and four
historical bundle tests. Its generated-schema comparison, profile/scenario/result
validation, historical import, bundle validation, generated matrix check, and
all-profile Compose rendering passed. The source-built Go helpers passed
`gofmt`, `go mod tidy -diff`, `go vet`, and all package tests. Historical profile
and result bytes remained unchanged.

LakeCat passed strict all-workspace/all-target/all-feature Clippy, warning-denied
Rustdoc, the dependency contract, and default/all-feature tests. The acceptance
run included the focused four config-fixture tests plus the API (10), CLI (501),
service library (471), service binary (5), and store (189) suites. The full book
build rendered eight Mermaid diagrams and a 59-page PDF, then passed PDF, EPUB,
library, and version contracts; tracked PDF, EPUB, MOBI, HTML, and chapter
artifacts identify source `09dd7ee3`.

This closes C1-03 only. No latency or throughput was measured, the transcripts
are not checked in as result records, and the candidate profile remains `draft`.
C1-04 next owns namespace behavior. C1-09 still owns the optimized final
materialization, immutable sanitized bundles, generated site/report output, and
public secret scan.
