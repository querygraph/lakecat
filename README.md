# LakeCat

LakeCat is a Rust-native Iceberg REST catalog and QueryGraph foundation.

The implementation keeps Iceberg compatibility at the service boundary while
pushing engine-heavy metadata planning, pruning, and commit validation toward
Sail. See [ARCHITECTURE.md](ARCHITECTURE.md) for the system design.

The current scaffold exposes an Iceberg REST-compatible catalog surface under
`/catalog/v1` and a QueryGraph bootstrap bundle at `/querygraph/v1/bootstrap`.
The bootstrap bundle projects live catalog tables into Croissant, CDIF, OSI,
ODRL, OpenLineage, and a Grust-ready graph envelope.

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
The service binary exposes `sail-local`, `typesec-local`, `grust-local`, and
`turso-local` feature gates so local real integrations can be activated without
code edits. `LAKECAT_WAREHOUSE` selects the served warehouse, and
`LAKECAT_BIND_ADDR` selects the listen address; defaults are `local` and
`127.0.0.1:8181`. With the `turso-local` feature, `LAKECAT_TURSO_PATH` selects a
Turso-backed `TursoCatalogStore` for namespaces, table records, metadata pointer
history, audit/outbox rows, and idempotent commit replay; without it the binary
keeps the in-memory store.

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
cargo run -p lakecat-cli -- qglake-fixture \
  --output target/qglake/lakecat-bootstrap.json \
  --drain-output target/qglake/lineage-drain.json \
  --principal did:example:agent
cargo run -p lakecat-cli -- qglake-verify-replay \
  --bundle target/qglake/lakecat-bootstrap.json \
  --drain target/qglake/lineage-drain.json \
  --principal did:example:agent
scripts/qglake-handoff-local.sh
cargo run -p lakecat-cli -- bootstrap-export --output lakecat-bootstrap.json
```

`scripts/qglake-handoff-local.sh` is the local-first end-to-end handoff proof:
it starts LakeCat on `127.0.0.1:18181`, generates paired QGLake bootstrap and
lineage-drain artifacts, verifies saved replay with LakeCat, then runs
QueryGraph's `lakecat-verify` and `lakecat-import` over the same bundle while
writing all generated artifacts under `target/qglake-handoff/`. It also writes
`target/qglake-handoff/handoff-summary.json`, which records the verified
LakeCat replay status, QueryGraph table/view counts, semantic hashes, and
standards after LakeCat replay, `lakecat-verify`, and `lakecat-import` agree,
artifact paths, raw file hashes, captured LakeCat replay output, QueryGraph
verify output, QueryGraph import output, and service log path for automation.
