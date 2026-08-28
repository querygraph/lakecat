# Phase 5 acceptance — semantic supply chain

Phase 5 is closed with a fresh, source-pinned, one-command TPC-DS run. The
reviewed result is `querygraph/catalog-bench@61183ba`, under
`results/source/semantic/tpcds_0828g/`; later community packaging does not
rewrite that evidence.

## Accepted revisions

- Apache Ossie: `1d9ebcea2932d3381c0840cc8304f0850d366509`
- LakeCat: `8917e5c639d0ec6bfb39a3923550ed73ddb163aa`
- QueryGraph: `b857204d427239cc997fc423574e17d8b814a3ff` for the live run and
  `f0e4afd` for the converter report/proposal/demo material
- Sail: `9f6f8065` on `querygraph/sail#lakecat`
- Grust: `e5edc99` for the pinned TPC-DS projection-count replay test
- catalog-bench: `61183ba` for reviewed run evidence

## C5-01 through C5-03

Stock Spark 4.1.3 with Iceberg 1.11.0 creates five physical tables through the
standard REST catalog and S3 FileIO boundary. The run records 15 rows, 30
fixture columns, five realized schema hashes, and five nonzero snapshot IDs.
The input is the checksum-fetched upstream TPC-DS model; the source artifact
hash is `sha256:438372de...e5a316` and its canonical model-input hash is
`sha256:3400c9e7...dd74b`.

LakeCat installs the governed `tpcds-semantic` policy and CAS-publishes version
1 of `tpcds_retail_model` with the exact artifact URI/hash and five physical
bindings. Publication read-after-write matches exactly. LakeCat `8917e5c6`
drains `model.published` into one stable QueryGraph-model graph event and one
OpenLineage event. The replay event and OpenLineage hashes are recorded in the
reviewed proof. This is at-least-once outbox replay evidence, not distributed
exactly-once delivery.

Grust `e5edc99` independently projects the pinned model identity, five datasets,
31 upstream fields, five metrics, and four relationships to 42 stable nodes and
45 stable edges. Exact replay is equal and broken relationship references fail.
LakeCat emits the catalog-facing graph boundary; Grust owns the taxonomy and
projection behavior.

## C5-04 and C5-05

The live run evaluates total sales (`105.00`), total profit (`28.00`), customer
lifetime value (`35.000000000000`), sales by the three fixture brands, and store
productivity (`1.909090909091`). Exact decimal strings avoid host/runtime float
drift. The answer proof binds seven bases: physical snapshots, canonical model,
source artifact, governed policy, SQL plans, graph replay, and lineage replay.
The proof hash is `sha256:c8571a63...cadcab63`.

Six independent adversarial mutations—physical, model, policy, graph, lineage,
and artifact—are all rejected. Answer, basis, and proof-hash tampering also fail
the QueryGraph unit contract. These are integrity checks, not cryptographic
attestation of the Spark runtime.

## C5-06 and C5-07

The upstream Apache Ossie Polaris converter was run with Java 21 against an
isolated live Polaris 1.7.0 catalog. All 45 converter tests pass. TPC-DS
export/import preserves one model, five datasets, and 31 fields, while omitting
four relationships, five metrics, AI context, and two input model extensions.
It generates five COMMON dataset and 31 POLARIS physical-type extensions and
warns about four decimal precision defaults. QueryGraph records this as
`verified-with-loss`; it is not described as lossless interchange.

The clean operator entry point is:

```bash
docker/run-querygraph-tpcds-fixture.sh tpcds_<unique-id>
```

It refuses reused output, fetches pinned source into disposable storage, uses
run-owned catalog/object state, and leaves zero labeled containers and volumes.
Run `tpcds_0828g` completed with summary content hash
`sha256:3057bc81...94e3376`.

## Exit decision

C5-01 through C5-07 are complete. Phase 5 proves a small deterministic semantic
supply chain and explicit drift invalidation. It does not claim TPC-DS scale or
performance, arbitrary Ossie-model coverage, engine-native OpenLineage, or
lossless Polaris conversion.
