# Catalog Community Phase 1 Acceptance Ledger

This ledger accumulates evidence for Phase 1, **Lakekeeper and behavioral
conformance**. It is deliberately incremental. A completed row proves only its
named backlog unit; Phase 1 remains open until C1-01 through C1-10 and the phase
exit criteria and global acceptance gates are satisfied.

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
C1-09 still owns the optimized final materialization, immutable sanitized
bundles, generated site/report output, and public secret scan.

## C1-04 — namespace behavior

Accepted revisions:

- [`catalog-bench@1f4e640566906ded6aa0589d52351eb1c32788f0`](https://github.com/querygraph/catalog-bench/commit/1f4e640566906ded6aa0589d52351eb1c32788f0)
  implements the typed namespace scenario, fixture isolation, bounded
  pagination, cleanup, sanitization, production CLI, and tests;
- [`catalog-bench@2c3ef824a8c49af516eecec5f62a594c2dced274`](https://github.com/querygraph/catalog-bench/commit/2c3ef824a8c49af516eecec5f62a594c2dced274)
  pins the independently accepted runner and LakeCat revisions in the draft
  candidate profile;
- [`catalog-bench@b149ee74d574d13c51a894d9f440606be2b5a0c1`](https://github.com/querygraph/catalog-bench/commit/b149ee74d574d13c51a894d9f440606be2b5a0c1)
  records the exact five-catalog acceptance matrix, artifact identities,
  findings, and reproduction boundary; and
- [`lakecat@42b2f34b85d7cbcce1b36d4008211075b6c51593`](https://github.com/querygraph/lakecat/commit/42b2f34b85d7cbcce1b36d4008211075b6c51593)
  corrects LakeCat's namespace protocol, properties, storage, governance, and
  replay behavior.

The exact contract and optimized executable inputs are:

| Input | Identity |
| --- | --- |
| Candidate profile | `catalog-community-current-2026-08-26-linux-arm64`, SHA-256 `db90aba01066ab2bcfc4843915c70020c53ffbe29f86ae25cb5fb553f531f286` |
| Scenario | `iceberg-rest.namespace.behavior` version 1, SHA-256 `0cd6262c9bda87ac217e8fc618cf3138ddabe6ca89aac94ee05628a67729b7ac` |
| Runner executable | SHA-256 `6a81806f955924dd2961bc6bfe68fab97cd24d302a50532d6410bccbf9c0f78e` |
| LakeCat executable | SHA-256 `5a6a867c0e3923505f107d418f2a3cc327fd7fa73566b9ac89af77dc588ab839` |
| LakeCat local runtime image | tag `lakecat-service:c104-42b2f34b`; image ID `sha256:33dfed34779cd601cf8b98b30dde49d0f363020b0daac8f27baa35756e118691` |
| Rust toolchain | stable Rust 1.97.1; Cargo 1.97.1 |
| Production build | opt-level 3, fat LTO, one codegen unit, stripped symbols, `panic=abort`, disabled incremental compilation, `-Dwarnings`, `-Ctarget-cpu=native`, `-j1` |
| LakeCat features | `turso-local,sail-local` |

The local image ID identifies the accepted runtime payload; it is not presented
as a registry-pullable digest. The candidate profile remains `draft` because
C1-09 still owns final artifact materialization.

### Strict scenario semantics

The runner derives two top-level namespaces and one multipart child from a
portable, run-owned fixture ID. It first loads all three and requires
spec-shaped 404 responses. Any pre-existing fixture aborts mutation, preventing
the runner from deleting another user's state.

The attempted workflow then covers:

- create, top-level list, load, and exact namespace/property round trips;
- U+001F multipart encoding and immediate-child `parent` listing;
- optional property update with preservation, removal, and missing-key proof;
- duplicate create as HTTP 409 with an Iceberg error envelope;
- absent-parent list as HTTP 404 with an Iceberg error envelope;
- one-item pagination from an explicit empty token, including bounded pages,
  unique tokens, no duplicate/lost namespaces, and the spec-permitted
  unpaginated fallback; and
- child-first cleanup plus post-drop 404 verification even after an earlier
  assertion fails.

The scenario has 13 assertions: 12 required and one optional
`namespace-properties-updated` assertion. An optional failure remains in the
transcript but does not invalidate an otherwise conformant required workflow.
Raw response bodies, OAuth credentials, bearer tokens, and opaque page tokens
are not persisted. Every accepted transcript reports
`raw_secrets_persisted: false` and `raw_response_body_persisted: false`.

### LakeCat correction

The exploratory LakeCat transcript
`b520f308debbdfc80b3ecb17053de47113c7923b9a5a3eff38f144e2e5db9506`
exposed two required defects. LakeCat treated the decoded U+001F separator as an
unsupported character inside one component, and its list handler ignored
`parent`. Multipart load, hierarchy, and absent-parent semantics therefore
could not pass through a stock REST client.

The accepted revision adds a dedicated REST namespace codec and list policy:

- U+001F separates ordered namespace components while a literal dot remains a
  wire component character;
- unscoped lists return only top-level namespaces and scoped lists return only
  immediate children after proving the parent exists;
- sorted pagination uses bounded `lakecat-v1:<offset>` tokens, validates empty,
  malformed, zero-size, and out-of-range requests, and caps page size;
- create/load/update round-trip durable string properties;
- duplicate create returns 409, missing namespaces/parents return 404, and a
  removal/update overlap returns an exact 422
  `UnprocessableEntityException` without changing state; and
- parent drop is rejected while any descendant namespace, table, view, or
  policy binding remains.

`MemoryCatalogStore` now keeps namespace identity and properties in one map.
Turso writes the namespace identity row and its new
`namespace_properties` side row transactionally. A legacy identity row without
a side row loads as an empty map and is lazily materialized on first update.
Turso updates and drops validate durable scope, and each resulting mutation is
transactional.

Property update has its own `namespace.update` authorization action and typed
capability. The accepted audit payload records receipt, warehouse, and namespace
but omits property keys and values. Its paired outbox event is admitted as
`namespace.properties-updated`, then projects a Grust namespace upsert and an
OpenLineage namespace-properties-updated event. A service regression proves the
event contains neither fixture property value and that the normal drain empties
the pending outbox.

The complete operator and contributor contract is
[`docs/ICEBERG-NAMESPACES.md`](../ICEBERG-NAMESPACES.md).

### Optimized same-Docker proof

The runner and LakeCat were compiled from read-only committed checkouts inside
Docker with the production settings above. LakeCat linked in 25 minutes 42
seconds. The Docker daemon's registry-metadata resolver was unresponsive, so the
exact already-local Rust 1.97.1 layer and Cargo caches were reused in an isolated
builder rather than restarting or pruning Docker. Package resolution,
compilation, linking, packaging, and probes still ran inside Docker.

The runtime replaced the prior LakeCat container on the same
`catalog-bench-net` service alias and persisted test volume. The comparison
catalogs used these profile-pinned local images:

| Catalog | Version | Image digest |
| --- | --- | --- |
| Apache Gravitino | 1.3.0 | `sha256:80136ae753ee77735153fc1482a389018f8c2638a54f453cb96967c7194584c7` |
| Lakekeeper | 0.13.3 | `sha256:db2ba6168eb107f22242fb7f2edc4016fa35e57bdcc606894e809c418e32e8dc` |
| Apache Nessie | 0.108.4 | `sha256:c0f42874c810f28ac30fc991e979c1b8cf5a2cbfa94212086cdddeae49629517` |
| Apache Polaris | 1.7.0 | `sha256:3495f67f38cca33892a045f7dd3f46eb52387f0fd52d4145538a772fd8aedad7` |

Every catalog request originated from the same optimized conformance container
on that Docker network. No host route participated.

### Live namespace outcomes

| Catalog | Required result | Optional property update | Pagination | Sanitized transcript SHA-256 |
| --- | --- | --- | --- | --- |
| LakeCat | **pass** | pass | 13 pages, 13 unique top-level namespaces | `f344244b1b0a586728e37126725b0fa0be9729a01a3af832018bcb8403a4b854` |
| Apache Gravitino | **pass** | pass | 2 pages, 2 unique namespaces | `80ac2d1ffd244fa7546516c19324c34ddf2a6e01b113b5ba1d014dcb00f2956c` |
| Lakekeeper | **pass** | pass | 3 pages, 2 unique namespaces including terminal traversal | `2eb9bdd1d704a981b0cde73fdaf7154f2cc5f3d414e3bde8eee795f312c75ada` |
| Apache Polaris | **pass** | fail: update returned HTTP 409 | unpaginated fallback, 2 unique namespaces | `4d77732a6dbc801ea70c7c486ced8e03aed24e27226ba929e6317c171c93e88a` |
| Apache Nessie | **fail** | pass | 2 pages, 2 unique namespaces | `f5813af7285ead3ea5947a62d8d99dea79e83fb97974ccd9985114cd35c45eab` |

All five runs passed fixture isolation, cleanup, and transcript sanitization.
LakeCat's 13 pages reflect pre-existing unrelated top-level namespaces in its
durable test database, not leaked fixtures or a timing result. The runner proved
complete no-loss/no-duplication traversal and removed only its own three names.

Nessie's one required failure is narrow: listing under the run-owned absent
parent returns HTTP 200 with an empty page, while Apache Iceberg 1.11 requires
HTTP 404. Its other required behavior, optional update, pagination, and cleanup
pass. This does not restate or explain Nessie's separate historical concurrent
commit HTTP 500 result.

Polaris returns HTTP 409 for namespace property update. Iceberg permits servers
not to support namespace properties, so the scenario preserves that optional
failure while retaining Polaris's required pass. Its unpaginated list response
is also permitted by the OpenAPI contract and was checked for completeness.

### Repository gates and remaining debt

catalog-bench passed strict full-workspace tests and Clippy under
`RUSTFLAGS=-Dwarnings`, including 14 config and 13 namespace runner tests.
Generated schemas, all profiles/scenarios/results, the capability matrix, and
the 21 common contract integration tests passed after the documentation update.

LakeCat passed warning-denied focused API, lineage, security, service, and store
tests; 193 Turso-store tests; 457 production-feature service tests with
`turso-local,sail-local`; and production-feature Clippy. The focused coverage
includes legacy Turso migration, unchanged state after 422, descendant drop
protection, value-redacted evidence, and successful outbox drain.

One broader pre-existing identity ambiguity remains. `Namespace::path()` joins
components with `.`, and Turso plus multiple table/view/policy-derived keys use
that string. A literal component `["a.b"]` can therefore alias multipart
`["a", "b"]` internally even though the corrected REST codec distinguishes
them. Fixing one key family would leave cross-object corruption risk, so C1-04
does not claim arbitrary-punctuation coverage. C1-10 now tracks a versioned,
component-unambiguous key migration across every affected object class.

This closes C1-04 only. It is a behavioral conformance result, not a latency or
throughput ranking. The transcripts remain ignored smoke evidence rather than
checked-in `catalog-bench/v1` result records. C1-05 is recorded below; C1-09
retains immutable bundle/site/report publication, and C1-10 retains remaining
LakeCat conformance corrections.

## C1-05 — table behavior

Accepted revisions:

- [`catalog-bench@621cc4bbc80169547c497b6829a4982e20f24e58`](https://github.com/querygraph/catalog-bench/commit/621cc4bbc80169547c497b6829a4982e20f24e58)
  makes the table runner consume and verify a profile-declared standard create
  location while preserving catalog-managed defaults for adapters that omit it;
- [`catalog-bench@b3b5e6799aaf0b68c97f983c363b3160ed915227`](https://github.com/querygraph/catalog-bench/commit/b3b5e6799aaf0b68c97f983c363b3160ed915227)
  pins that runner in the candidate profile;
- [`catalog-bench@75c95cf`](https://github.com/querygraph/catalog-bench/commit/75c95cf)
  corrects Gravitino 1.3.0's exact `GRAVITINO_ICEBERG_REST_*` environment
  contract and adds a deployment regression;
- [`catalog-bench@99971e8a84f116646bd05eb48728b4982b5a4444`](https://github.com/querygraph/catalog-bench/commit/99971e8a84f116646bd05eb48728b4982b5a4444)
  adds the least-privilege one-shot that prepares Gravitino's private SQLite
  volume for its unprivileged UID 1000 process;
- [`catalog-bench@6bc668b`](https://github.com/querygraph/catalog-bench/commit/6bc668b)
  records the final matrix, exact artifacts, direct shared-MinIO audit, rejected
  evidence, reproduction workflow, and publication boundary;
- [`lakecat@c1abd976`](https://github.com/querygraph/lakecat/commit/c1abd976)
  completes standard table lifecycle behavior, including governed registration
  and atomic same-warehouse rename; and
- [`lakecat@335f94ef`](https://github.com/querygraph/lakecat/commit/335f94ef)
  normalizes valid Iceberg no-snapshot state at the durable commit-evidence
  boundary, with [`lakecat@762527c7`](https://github.com/querygraph/lakecat/commit/762527c7)
  carrying the exact regenerated reader artifacts used by the accepted runtime.

The neutral source of detail is catalog-bench's
[`TABLE-CONFORMANCE.md`](https://github.com/querygraph/catalog-bench/blob/6bc668b/docs/TABLE-CONFORMANCE.md).
LakeCat's durable protocol and governance contract is
[`docs/ICEBERG-TABLES.md`](../ICEBERG-TABLES.md).

### Exact optimized inputs

| Input | Identity |
| --- | --- |
| Acceptance execution checkout | `catalog-bench@99971e8a84f116646bd05eb48728b4982b5a4444` |
| Candidate profile | `catalog-community-current-2026-08-26-linux-arm64`, SHA-256 `a8d86ab535ac84780ad3694775deec7ae74556ccdf4ed9bf65f97335a18edf52` |
| Scenario | `iceberg-rest.table.behavior` version 1, SHA-256 `50237ef4dfefb2e3f58f0cca3d6a0550c6b7d08a3cceccf4ecc68d5a606fe6e9` |
| Runner executable | SHA-256 `e2f1d622640a3dc987322c185a2ff369f6612780ed62ae57651f2c57bbcfb3a7`; 3,609,344 bytes |
| LakeCat source | `762527c7d27730dd789cf41b1cdee021ab712aef`, version `0.3.0-31-g762527c7` |
| LakeCat executable | SHA-256 `70bc7d84b5c08a9addf52848edec4c0746f65a2680074d1c606dd2889ae60abd`; 19,560,096 bytes |
| LakeCat local image | Linux ARM64 image ID `sha256:3936e3576bfee378e2fde0227a4a1f9f2eb6b75322291feb3b67b4fd87ae23f6`; 60,017,816 bytes |
| Rust builder | `rust:1.97.1-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97`, ARM64 `sha256:6e957ef098dcc77d33e310261e4ed5843bb108d5c3b5dc2b476cbc8b6caf53fa` |
| Rust toolchain | `rustc 1.97.1 (8bab26f4f 2026-07-14)`; Cargo 1.97.1; LLVM 22.1.6 |
| Production build | opt-level 3, fat LTO, one codegen unit, stripped symbols, `panic=abort`, no debug or incremental compilation, `-Dwarnings`, `-Ctarget-cpu=native`, `--locked`, `-j1` |
| LakeCat features | `turso-local,sail-local`; Sail `bddb1706ba2308e5029d47f04f03121236edbfa6`; Turso `0.7.0-pre.10` |
| Shared MinIO | `RELEASE.2025-10-15T17-29-55Z`, source `9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a`, local image `sha256:28c9405d4591b7803c8cf79afcef6a32f8fe9964982e5075babcb6a1c7ddecdb` |

The final runner build was repeated from the clean execution checkout after the
Gravitino deployment corrections and remained byte-identical. LakeCat used its
full production feature shape rather than a reduced benchmark-only binary.

The comparison catalogs remained the exact candidate-profile images:

| Catalog | Version | Index digest | ARM64 digest |
| --- | --- | --- | --- |
| Apache Polaris | 1.7.0 | `sha256:3495f67f38cca33892a045f7dd3f46eb52387f0fd52d4145538a772fd8aedad7` | `sha256:53022013a54121d6f81a130b80df85e2c3c1961c592c39e7e3e2353db2ab7acf` |
| Apache Gravitino | 1.3.0 | `sha256:80136ae753ee77735153fc1482a389018f8c2638a54f453cb96967c7194584c7` | `sha256:01cf367b77f91652da6c545ad5253d94c11f4e3dd71c5442863eaa330d8a1088` |
| Lakekeeper | 0.13.3 | `sha256:db2ba6168eb107f22242fb7f2edc4016fa35e57bdcc606894e809c418e32e8dc` | `sha256:ba9424131ff088e8eb5263dbdf66e63c2aec0e71687971673ca37a97389394f2` |
| Apache Nessie | 0.108.4 | `sha256:c0f42874c810f28ac30fc991e979c1b8cf5a2cbfa94212086cdddeae49629517` | `sha256:10d751690c54c837d687437e1cb269f61b8d2ca541277639d623f495b408fe9c` |

### Scenario semantics

One preflighted run-owned namespace contains primary and sibling committed
tables plus distinct rename, registration, and missing candidates. Any
pre-existing namespace aborts before mutation, so cleanup cannot delete someone
else's state.

The workflow proves 15 required assertions:

- authentication and config/prefix/separator negotiation without retaining a
  credential;
- absent fixture preflight and exact namespace creation;
- two committed creates with distinct UUIDs, requested schema/properties,
  immutable metadata locations, and requested table location when declared;
- exact list/load round trips and bounded, complete, duplicate-free pagination
  or the OpenAPI-permitted complete unpaginated fallback;
- one property set/removal commit that preserves identity and schema while
  advancing the metadata location;
- spec-shaped duplicate-table 409, missing-table 404, and missing-namespace 404;
- non-purging table drop followed by spec-shaped absence;
- reconciliation and post-drop absence for all source/destination candidates
  plus the fixture namespace; and
- recursively sanitized evidence with no raw response body, OAuth credential,
  bearer token, storage credential, cookie, or opaque page token.

Two optional operations are attempted honestly. Same-namespace rename must
return 204, remove the source, and preserve destination UUID and metadata
location. False-overwrite registration must return 200 and load the dropped
sibling metadata under a new name with its exact UUID and metadata location.
Optional failures remain visible but do not change required classification.

Cleanup runs after an earlier assertion failure and uses
`purgeRequested=false`. Retained metadata therefore remains available for
direct object-store inspection after all catalog names and the fixture namespace
are proved absent.

### LakeCat defects and correction

The first optimized LakeCat transcript,
`6bdf4237bede510da22b718d880048fe9bb36b5b7df83a5dc15821a336429b90`,
passed all required assertions and registration but returned HTTP 500 from
optional rename:

```text
internal error: table commit record snapshot id must be non-negative
```

Iceberg uses `current-snapshot-id: -1` for a valid table with no current
snapshot. A property-only update copied that wire sentinel into LakeCat's
durable commit record. Rename later revalidated history and found the invalid
internal negative value. The accepted implementation normalizes exactly `-1`
to LakeCat's established zero-valued no-snapshot evidence, decodes legacy
serialized `-1` compatibly, rejects every other negative value, validates before
memory/Turso persistence, and stages memory state before mutation. Shared tests
prove create, property commit, history, failure atomicity, and rename semantics
on both stores.

A repaired behavior transcript,
`ae757976fdce33564c233cd7139b944e1cdd8e405df5d750a9158074ce4ef28b`,
then passed all 17 assertions but was rejected because the old runner had
silently omitted LakeCat's declared create location and accepted
`file:///tmp/lakecat/...`. The corrected runner derives unique
`s3://warehouse/lakecat/<namespace>/<table>` children and requires that location
to remain stable across create, load, update, rename, and registration.

### Shared object-store and deployment proof

All requests originated from one production conformance container on
`catalog-bench-net`; no host route participated. LakeCat started from a fresh
Turso volume and received one scoped governed S3 profile rooted at
`s3://warehouse/lakecat`. Its accepted primary object was:

```text
s3://warehouse/lakecat/cb_c105_lakecat_c105s_lakecat_826/primary/metadata/00000-7c643e01-d092-4605-bfd4-17bcd14c7aa2.metadata.json
```

An earlier shared matrix was rejected as a whole because Gravitino still
returned `/tmp/...`. The pinned image's own rewrite script recognizes only the
`GRAVITINO_ICEBERG_REST_*` environment namespace; the prior shorter names had
silently retained memory and `/tmp` defaults. After correcting those bindings,
a fresh named volume exposed its root ownership while the image runs as UID
1000. The accepted one-shot adjusts only that private directory and exits as
root; the long-running catalog remains UID 1000. Its effective file-backed
SQLite/S3 config negotiated HTTP 200 before the final probe.

The final transcripts returned only `s3://warehouse/...` metadata locations.
A pinned local `mc` container on the same network statted every distinct
metadata object referenced by every transcript: original primary, updated
primary, and sibling metadata for each catalog, 15 of 15 total. Representative
primary objects were 955 bytes for LakeCat, 782 for Gravitino, 341 compressed
bytes for Lakekeeper, 830 for Polaris, and 829 for Nessie.

### Live table outcomes

| Catalog | Required | Rename | Register | Pagination | Sanitized transcript SHA-256 |
| --- | ---: | --- | --- | --- | --- |
| LakeCat | **pass, 15/15** | pass | pass | complete unpaginated fallback, 2 tables | `202b6fcffcb1cb832f0eb818b34454c956d777b6eee7d44445c8126ca365a0b9` |
| Apache Gravitino | **pass, 15/15** | pass | pass | 2 pages, 2 tables | `941deab4facf307b50c5e6bf3edcf2311dd4b644762441d4a546edc35117f379` |
| Lakekeeper | **pass, 15/15** | pass | pass | 3 pages, 2 tables | `c336b88e0aa6382f0d6c13818567554684ae868f98a1d297d4c3d9a6548aa004` |
| Apache Polaris | **pass, 15/15** | pass | pass | complete unpaginated fallback, 2 tables | `79e0fbead68feb142de7c3ce3d145560c831bcba937a9a15e43f352f32f63ac0` |
| Apache Nessie | **fail, 14/15** | pass | pass | 2 pages, 2 tables | `8019de1556f3bcedd7de2471c74acf8b518dd10c0ecd5888b31d1a69c163fda1` |

Every catalog passed fixture isolation, candidate reconciliation, cleanup, and
transcript sanitization. All final transcripts report
`raw_secrets_persisted: false` and `raw_response_body_persisted: false`; a
separate literal secret scan also passed.

Nessie's sole required mismatch is deterministic: listing tables under the
run-owned absent namespace returns HTTP 200 with an empty page, while the pinned
Iceberg OpenAPI requires 404. It still passes rename, registration, shared-MinIO
storage, and cleanup. This is not Nessie's historical concurrent result. That
separate workload remains unranked because 97 HTTP 500 request errors occurred
across five measured rounds; no table-conformance result is interpreted as a
throughput or concurrency claim.

### Repository gates and next boundary

Catalog-bench passed stable formatting, full workspace/all-target tests,
checked-in schema equality, semantic validation of every profile/scenario/result,
strict workspace/all-target Clippy, the full multi-profile Compose render, and
diff checks. Named suites included 21 contract, two deployment, 14 config, 13
namespace, 17 table, and four bundle/matrix tests.

LakeCat passed 209 store tests, the full all-feature workspace tests, strict
all-feature Clippy, and book contracts while implementing the lifecycle and
no-snapshot corrections. Memory/Turso tests exercise transition staging,
atomicity, commit history, registration, rename, policy retargeting,
idempotency retirement, outbox evidence, and cleanup.

This closes C1-05 only. It is behavioral evidence, not a performance ranking.
The exact transcripts remain ignored smoke files rather than immutable
`catalog-bench/v1` result records. C1-06 next owns commit requirements, stale
metadata pointers, exact retry, and idempotency-content drift. C1-09 still owns
final optimized artifact materialization, immutable bundle generation, manual
redaction review, public matrix/report generation, adversari.al publication, and
site verification.
