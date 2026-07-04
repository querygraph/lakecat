# LakeCat: a thin Iceberg REST catalog with governance, lineage, and proof built in

LakeCat is a Rust-native, Iceberg-compatible **REST catalog foundation** for
QueryGraph. It speaks the standard Iceberg REST protocol — pyiceberg, Spark, and
Trino talk to it unchanged — but the interesting part is what it adds *underneath*
the standard surface, and what it deliberately refuses to reinvent.

## A deliberately thin catalog with a durable spine

LakeCat keeps the catalog boundary thin: identity and tenancy, Iceberg REST
compatibility, metadata-pointer state, policy gates, and integration events live
here; everything reusable is pushed to siblings — **Sail** (Iceberg
format/scan/pruning/engine), **Grust** (graph), and **TypeSec**
(governance/policy/receipts). The catalog stays small and honest; the engines stay
shared.

Underneath is a durable spine on **Turso**, a Rust embedded database. Every commit
is **one transaction** that does more than move a pointer:

- a metadata-pointer **compare-and-swap** — optimistic concurrency, fail-closed;
- an **audit event** — who changed what, when;
- a **transactional-outbox** row — lineage and graph events staged *atomically*
  with the commit, so a catalog change can never be lost or emitted without it;
- an **idempotency** record — a retried commit replays its prior result instead of
  double-applying.

That is a durable audit trail, atomic lineage, and idempotency in the *same*
transaction as the table change. Governance runs through **TypeSec**: governed
reads narrow projection and apply mandatory filters, raw credential vending is a
deliberate and audited exception, and verdicts fail closed. Lineage drains from the
outbox as **OpenLineage** events only after the catalog transaction commits — so
lineage reflects committed state, never a handler's best-effort side effect.

## Turso MVCC: concurrent commits without a global lock

The spine recently moved to **Turso MVCC** (`journal_mode = mvcc` + `BEGIN
CONCURRENT`): commits to different tables run truly concurrently, while a same-table
race converges to exactly one winner through the pointer CAS and bounded retry. No
global write lock, no `database is locked`.

## The Commit Benchmark

How fast is a catalog, *really*? TPC-DS/TPC-H measure query engines and touch the
catalog only incidentally, so we built
[`catalog-commit-bench`](https://github.com/querygraph/catalog-commit-bench) to
measure the part they ignore — the commit transaction itself. It is an **impartial**
harness: LakeCat, Apache Nessie, Apache Gravitino, and Apache Polaris all do the
*identical* unit of work — validate, write a fresh `metadata.json`, move the pointer
under CAS — to the **same MinIO/S3 bucket**, so you compare catalogs, not object
stores. (The repo's README has the one-command Docker/MinIO setup.)

The result: after two connection-reuse fixes — cache the S3 client, pool the write
connection — LakeCat's median commit dropped from ~12.6 ms to ~4.5 ms, landing
**second of four** on both per-commit latency and concurrent throughput, ahead of
Gravitino and Polaris. The small remaining gap to Nessie is exactly the audit +
outbox + idempotency work LakeCat does per commit that leaner stores don't —
**features, not language**. The full story (and why a Rust catalog has to *earn* its
speed against warm JVM servers) is the book's *The Commit Benchmark* chapter.

## Check it out

LakeCat is open in the [QueryGraph org](https://github.com/querygraph). **TypeSec
and Grust ship as published crates** on crates.io; **Sail** is the only remaining
git dependency — a coordination fork branch, until its LakeCat-needed Iceberg
changes land upstream. Everything fetches automatically, so it builds out of the box
— no sibling checkouts required:

```sh
git clone https://github.com/querygraph/lakecat
cd lakecat && cargo build
```

The design surface is in `DESIGN.md` and `AGENTS.md`; the full narrative — from the
boundary model and the commit path to the benchmark — is in the book under
`docs/book/`. It is an early foundation, built in the open. Kick the tires, run the
benchmark, and tell us what's missing.
