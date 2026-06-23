# LakeCat

LakeCat is a Rust-native Iceberg REST catalog and QueryGraph foundation.

The implementation keeps Iceberg compatibility at the service boundary while
pushing engine-heavy metadata planning, pruning, and commit validation toward
Sail. See [ARCHITECTURE.md](ARCHITECTURE.md) for the system design and
[docs/book/lakecat.md](docs/book/lakecat.md) for the longer workflow guide.

The current implementation exposes an Iceberg REST-compatible catalog surface
under `/catalog/v1` and a QueryGraph bootstrap bundle at
`/querygraph/v1/bootstrap`. The bootstrap bundle projects live catalog tables
into Croissant, CDIF, OSI, ODRL, OpenLineage, and a Grust-ready graph envelope.
The full local release-readiness gate is green as of June 23, 2026; keep that
local proof green before making release or cloud-automation claims. Use
[RELEASE.md](RELEASE.md) for the first-release checklist.

Scan planning already routes through the Sail-facing engine. Point-in-time scans
produce opaque Iceberg REST plan-task tokens from stable Sail metadata, and
append-only incremental scans over a parent snapshot chain use Sail's manifest
list reader to plan only manifests added in `(start-snapshot-id, end-snapshot-id]`
when the table metadata and manifests are locally readable. Added delete
manifests are expanded through Sail's delete-file index so file scan tasks carry
Iceberg delete-file references. Non-append snapshot operations intentionally fail
until overwrite/delete incremental semantics are planned end to end.

REST scan filters are validated against Sail's generated Iceberg expression
models and stable table schema before planning. The accepted expression bundle is
preserved in structured opaque plan-task tokens, which are bound to the planned
table for stateless fetchScanTasks calls. During local manifest expansion, simple
predicates are applied conservatively to Iceberg file bounds when metrics are
present; missing metrics keep the file.

HTTP handlers resolve principals from `x-lakecat-principal`,
`x-lakecat-agent-did`, or bearer authorization headers before calling the
governance engine; absent credentials remain anonymous for local compatibility.
The service binary exposes `sail-local`, `typesec-local`, `grust-local`,
`grust-turso-local`, and `turso-local` feature gates so local real integrations
can be activated without code edits. `LAKECAT_WAREHOUSE` selects the served
warehouse, and `LAKECAT_BIND_ADDR` selects the listen address; defaults are
`local` and `127.0.0.1:8181`. With the `turso-local` feature, `LAKECAT_TURSO_PATH` selects a
Turso-backed `TursoCatalogStore` for namespaces, table records, metadata pointer
history, audit/outbox rows, and idempotent commit replay; without it the binary
keeps the in-memory store.

The Grust feature gate follows the local Grust 0.10 path checkout so LakeCat
can use the dedicated `grust-turso` crate for durable catalog graph projection.
Plain `grust-local` keeps the fast memory-backed Grust sink;
`grust-turso-local` constructs a bootstrapped `grust_turso::TursoGraphStore`,
using `LAKECAT_GRUST_TURSO_PATH` when set and an in-memory Turso graph database
otherwise. Startup connect/bootstrap failures for that graph sink are reported
with `graph-store-path-hash` and `backend-error-hash` evidence, not raw graph
database paths or backend text. TypeSec remains on the published `typesec`
0.8.0 crate, and Sail integration still uses local Sail paths plus the
checked-in helper patch bridge until the required Sail APIs are published.

The local QueryGraph handoff path has a separate compatibility contract:
`/Users/alexy/src/querygraph/qg-rust` follows the local Grust 0.10.0 path
checkout for `lakecat-verify` and `lakecat-import`. The handoff harness starts
LakeCat with `grust-turso-local` plus `LAKECAT_GRUST_TURSO_PATH`, so the
end-to-end QueryGraph acceptance path exercises Grust's Turso-backed catalog
graph sink. The handoff summary carries hash-only `graphProjectionProof`
evidence for that backend, including the configured `lakecat_graph` table
prefix, and the Rust verifier rejects missing or drifted graph-backend proof
before accepting saved artifacts. The dependency contract keeps that harness
aligned with the active local Grust graph implementation while graph
persistence, traversal, and Cypher-over-Turso work remain Grust-owned.
LakeCat's `grust-turso-local` graph tests cover writing catalog events,
traversing the projection, and querying/mutating it through Grust Cypher over
the same Turso-backed store. They also prove Grust's matched-node mutation plan
can patch a projected LakeCat table node in Turso, keeping QueryGraph readiness
updates in Grust rather than turning LakeCat into a graph database.

Useful local checks:

```bash
cargo run -p lakecat-cli -- config
cargo run -p lakecat-cli -- storage-profile-list
cargo run -p lakecat-cli -- storage-profile-upsert \
  --profile local-events \
  --location-prefix file:///tmp/events \
  --provider file \
  --issuance-mode local-file-no-secret
cargo run -p lakecat-cli -- policy-list
cargo run -p lakecat-cli -- policy-upsert \
  --policy agent-read \
  --namespace default \
  --table events \
  --odrl-file ./policy.odrl.json
cargo run -p lakecat-cli --features qglake-fixture -- qglake-fixture \
  --output target/qglake/lakecat-bootstrap.json \
  --drain-output target/qglake/lineage-drain.json \
  --principal did:example:agent
cargo run -p lakecat-cli -- qglake-verify-replay \
  --bundle target/qglake/lakecat-bootstrap.json \
  --drain target/qglake/lineage-drain.json \
  --principal did:example:agent
scripts/qglake-handoff-local.sh
scripts/check-release-readiness.sh --quick
cargo run -p lakecat-cli -- bootstrap-export --output lakecat-bootstrap.json
```

`scripts/qglake-handoff-local.sh` is the local-first end-to-end handoff proof:
it starts LakeCat on `127.0.0.1:18181`, generates paired QGLake bootstrap and
lineage-drain artifacts, verifies saved replay with LakeCat, then runs
QueryGraph's `lakecat-verify` and `lakecat-import` over the same bundle while
writing all generated artifacts under `target/qglake-handoff/`. The script
owns that default target directory for each run: it clears stale Turso
WAL/SHM files and generated fixture table storage, fails fast if the handoff
bind address is already occupied, and stops the spawned LakeCat service tree on
exit. It also writes `target/qglake-handoff/handoff-summary.json`, a
`lakecat.qglake.handoff-summary.v1` contract which records the verified
LakeCat replay status from `lakecat.qglake.replay-verification.v1`,
QueryGraph table/view counts, semantic hashes, and standards after LakeCat
replay, `lakecat-verify`, and `lakecat-import` agree, structured
scan/management/credential/commit replay evidence, artifact paths, raw file
hashes, captured LakeCat replay output, QueryGraph verify output, QueryGraph
import output, and service log path for automation. Handoff verification keeps
those artifacts bundle-local and schema-closed: paths must resolve under the
handoff summary directory before LakeCat hashes or parses them, and the primary
`artifacts` manifest, nested `capturedOutputs` manifest, and individual
bundle/lineage/import/captured-output artifact objects reject unexpected fields
beside the checked `path` and `sha256` evidence. A saved handoff summary cannot
attach alternate hashes, mirror artifacts, or unverified captured-output claims
beside otherwise valid files. View receipt-chain proof is structural in this
local gate as well: the script walks verified chains and receipts, checks
version-1 upsert heads, previous receipt links, supported operations, version
transitions, identity binding, tombstone posture, and tombstone receipt coverage
before compact handoff proof is accepted.

For first-release readiness, run the local release gate instead of relying on
cloud CI. The full release checklist lives in [RELEASE.md](RELEASE.md):

```bash
scripts/check-release-readiness.sh --release-candidate
```

The full gate runs shell syntax checks, the local dependency contract, workflow
trigger checks, release version consistency across all LakeCat crates and book
artifacts, formatting, default workspace tests, integration feature tests, the
Turso store row, service feature rows, Grust/TypeSec/Sail feature rows, the
explicit Rust `lakecat-cli qglake_handoff` verifier row, explicit all-features
CLI tests, all-features workspace tests, book rebuild with
EPUB metadata and PDF layout validation, QGLake handoff proof, and
`git diff --check`.
The current full proof also verifies the Grust Turso graph projection evidence,
including `graphProjectionProof.backend = grust-turso` and
`graphProjectionProof.tablePrefix = lakecat_graph`; the latest clean
release-candidate proof was refreshed from head `45468f0d`.
In `--release-candidate` mode, book artifacts are built into a temporary dist
directory through `LAKECAT_BOOK_DIST_DIR`; run `docs/book/build.sh` directly
when intentionally refreshing tracked `docs/book/dist` artifacts.
Use `--quick` for a faster script/contract smoke check while developing a
narrow slice.

First-release scope is intentionally narrower than the long-term architecture:
standard Iceberg REST behavior, the Rust/Turso catalog spine, CAS/idempotency,
audit/outbox replay, governed Sail-planned access, redacted credentials,
OpenLineage/Grust projection boundaries, and QGLake handoff proof are in scope.
Typed Iceberg v4 semantics, richer reusable graph mechanics, cloud SDK secret
managers, and full QueryGraph product semantics remain Sail, Grust, TypeSec, and
QueryGraph follow-on work rather than release blockers for LakeCat's catalog
substrate.
