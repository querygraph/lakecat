# Catalog Community Phase 4 Acceptance Ledger

Phase 4 establishes Apache Ossie as a pinned interchange boundary while
retaining repository ownership. It does not yet claim an end-to-end TPC-DS
answer; that live supply chain belongs to Phase 5.

## Accepted revisions

- QueryGraph `5177c2e7ec827ea6551d55ccc0d26f4944552791`
- Sail `9f6f8065dd810be8146995876acf872ea803adf0`
- LakeCat `d7b9e3bef36134aeeac11eccf2761488d3ace463`
- TypeSec `3c5e0b185d6deb523c7f1568cc6b9cd46cf9f00f`
- Grust `cec8ce10f2fb2289cf77d561ac95cc459e733c17`

## Upstream and document boundary

QueryGraph pins Apache Ossie commit
`1d9ebcea2932d3381c0840cc8304f0850d366509`. Fetching verifies these upstream
SHA-256 values before writing disposable build inputs:

- schema: `8ce9f82aa92080265f9ae119e31cda5bef062f489674d3c467245c2d4c5ff264`
- validator: `dc3ef8914a283d0568f65843343ed7592377aa813230e1990c6adbb2241a2be3`
- TPC-DS model: `438372de9b8ca0f074aed72806f92ac9b84047851a0385423f004748efe5a316`

The upstream validator passes the fetched TPC-DS example. QueryGraph's artifact
envelope round-trips that example through JSON and YAML without losing unknown
keys, multiple models, custom extensions, or dialect expressions. It reports
schema errors and conversion loss explicitly; it does not redefine Ossie's
schema.

## Owned implementation boundaries

- Sail validates physical dataset fields, exact Iceberg types, nullability, and
  inputs of already-parsed expressions.
- LakeCat stores only immutable artifact pointers/hashes, physical and policy
  binding references, publisher identity, and monotonic CAS versions. Turso
  admits publication, audit, and outbox state in one transaction.
- TypeSec signs positive decisions for model publication/consumption, field
  access, metric execution, semantic queries, and AI-context access. Claims are
  bound to model version plus artifact, policy, and optional binding hashes.
- Grust projects the reusable model/dataset/field/relationship/metric taxonomy.
  Replaying identical input yields identical graph identity; version changes
  identity and missing relationship endpoints fail.
- QueryGraph owns document composition and orders admission as structural/model
  checks, authorization, physical validation, LakeCat publication, graph, then
  lineage.

## Fail-closed evidence

Tests independently inject malformed content, denial, missing physical state,
schema drift, artifact/model drift, unknown Ossie version, and catalog version
drift. Each failure records no graph or lineage promotion; pre-admission cases
also record no catalog publication. LakeCat separately proves a stale CAS or
missing policy reference admits no publication or outbox event.

Accepted verification included:

```sh
# QueryGraph: upstream fetch/validator and all Python composition tests
python scripts/fetch-ossie.py fetch <temporary-directory>
python scripts/fetch-ossie.py verify <temporary-directory>
uv run --with jsonschema --with pyyaml --with sqlglot \
  <temporary-directory>/validation/validate.py \
  <temporary-directory>/examples/tpcds_semantic_model.yaml
(cd python && uv run pytest) # 74 passed

# Sail
cargo test -p sail-iceberg semantic_binding
cargo clippy -p sail-iceberg -- -D warnings

# LakeCat
cargo test -p lakecat-store --features turso-local -- --test-threads=1 # 220 passed
cargo test -p lakecat-service --all-features -- --test-threads=1 # 495 + 5 passed

# TypeSec
cargo test -p typesec-integrations --lib # 105 passed
cargo clippy -p typesec-integrations --lib -- -D warnings

# Grust
cargo test -p grust-graph semantic -- --test-threads=1
cargo clippy -p grust-graph -- -D warnings
```

Phase 4 is closed. Phase 5 must replace the callback-level composition proof
with a clean, one-command physical TPC-DS run and exact cross-system hashes.
