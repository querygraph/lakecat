# Iceberg REST Commit Correctness

This document is LakeCat's operator and contributor contract for deterministic
Apache Iceberg REST table commits. Canonical LakeCat source
`ef94b5508e94554f51f4764af932cbb819ae3e41` passes all required checks in the
catalog-neutral C1-06 scenario under the production `turso-local,sail-local`
feature set.

The neutral scenario, runner, five-catalog matrix, transcript hashes, and direct
object-store audit live in
[`catalog-bench`](https://github.com/querygraph/catalog-bench/blob/fdb2a9af1d8570ef36491beb408aabb71570cce6/docs/COMMIT-CONFORMANCE.md).
LakeCat owns the behavior described here; it does not own the comparison.

## Protocol boundary

An Iceberg REST table commit is an optimistic state transition. The client sends
requirements describing the table state it planned against and updates that
produce the next metadata document. Requirements are validated before updates.
If a requirement is stale, the standard response is HTTP/code 409 with Iceberg
error type `CommitFailedException`, and no part of the attempted transition may
become current.

LakeCat keeps this standard boundary thin:

```text
request requirements + updates
              |
              v
validate current UUID/schema/field state
              |
              v
Sail applies Iceberg metadata updates and writes a create-only metadata object
              |
              v
CatalogStore atomically advances the current pointer and durable evidence
              |
              v
standard Iceberg REST response
```

The metadata file remains pristine Iceberg state. LakeCat's pointer log,
idempotency record, audit row, transactional outbox event, graph projection, and
OpenLineage evidence are control-plane effects around the accepted transition;
they do not redefine what an ordinary Iceberg client sends or receives.

## Deterministic C1-06 proof

The C1-06 runner does not infer correctness from a scheduler-dependent conflict
rate. Every catalog receives a fresh, run-owned namespace and committed table.
The runner then performs one transition at a time:

1. prove the fixture namespace absent before any mutation;
2. create schema 0 and its initial metadata object;
3. admit matching table-UUID and current-schema requirements, update one
   scenario-owned property, and require exactly one pointer advance;
4. admit matching UUID, schema-ID, and last-field-ID requirements, add field 2
   as schema 1, and require exactly one pointer advance;
5. submit a request still asserting schema 0;
6. require HTTP/code 409 `CommitFailedException`;
7. reload independently and require the complete UUID, metadata pointer, schema,
   last-field ID, and property map to remain unchanged;
8. run config-gated exact-retry and same-key/content-drift checks only when the
   catalog advertises `idempotency-key-lifetime`;
9. drop the exact table and namespace without purging metadata objects, then
   prove both catalog entries absent; and
10. reject any transcript that persisted a credential, token, raw idempotency
    key, secret-shaped value, or raw response body.

Successful transitions compare the exact scenario-owned `catalog-bench.*` and
`c1-06.*` property projection. Catalog-managed properties may evolve without
being mistaken for requested state. Rejected stale requests and optional retry
checks compare the complete normalized state because those operations must not
have a second effect.

## LakeCat result

LakeCat passes all 10 required assertions. Its stale request returns HTTP 409,
body code 409, and type `CommitFailedException`; the independent reload remains
on schema 1 and the same final metadata object. The stale property is absent.
Fixture isolation, cleanup, and transcript sanitization also pass.

LakeCat's resolved config does not currently advertise the standard
`idempotency-key-lifetime` property. The neutral runner therefore sends no
`Idempotency-Key` header and makes no optional exact-retry or content-binding
claim for LakeCat. This does not erase LakeCat's internal idempotency records and
tests; it keeps the public interoperability claim honest. A standard client may
rely on the optional profile only after LakeCat advertises it and the same
cross-catalog scenario proves exact replay and same-key/content drift end to end.

## Five-catalog outcome

The accepted production runner applies the same scenario to every catalog:

| Catalog | Required | Stale requirement | Advertised retry profile | Exact retry | Content drift |
| --- | ---: | --- | --- | --- | --- |
| LakeCat | **pass, 10/10** | 409 `CommitFailedException`; state unchanged | no | not evaluated | not evaluated |
| Apache Gravitino | **pass, 10/10** | 409 `CommitFailedException`; state unchanged | no | not evaluated | not evaluated |
| Apache Polaris | **pass, 10/10** | 409 `CommitFailedException`; state unchanged | no | not evaluated | not evaluated |
| Lakekeeper | **fail, 9/10** | 409 `CatalogCommitConflicts`; state unchanged | yes, `PT30M` | pass | fail: cached 200; state unchanged |
| Apache Nessie | **fail, 9/10** | 409 with an empty type; state unchanged | no | not evaluated | not evaluated |

Lakekeeper and Nessie enforce the stale requirement and preserve state. Their
required failure is the Iceberg error envelope, not atomicity. Lakekeeper's
advertised exact replay advances once and replays successfully. Reusing the key
with a different body returns the original cached HTTP 200 instead of 409,
although the drifted state does not apply. That is a response content-binding
defect, not silent state mutation.

## Exact production identity

All catalog requests originated on `catalog-bench-net`; catalog traffic never
crossed a host-published port. Every catalog wrote Iceberg metadata to the same
local MinIO `warehouse` bucket while retaining its own private state backend.

| Item | Exact identity |
| --- | --- |
| catalog-bench acceptance | `fdb2a9af1d8570ef36491beb408aabb71570cce6` |
| runner implementation | `f07242219b5ef889507e288ed8f0d23ff4701ef9` |
| candidate profile | SHA-256 `2a428c2bb6ce31eae626d0abcb82db101e9165c5497185111b84288012fbe96d` |
| commit scenario | SHA-256 `7df567363927001aa25e55c607f60feb63b2fe5442d82d800d298d87e8bc886d` |
| runner executable | SHA-256 `243f16e0f2f375113df2516eb593b36d6a736cf3f25a76055409bd8b5e96391f`; 3,805,952 bytes |
| LakeCat source | `ef94b5508e94554f51f4764af932cbb819ae3e41`; `0.3.0-32-gef94b550` |
| LakeCat executable | SHA-256 `0d74e70378f73a9f59eb402cc342e037b29995a3587fc20d2c27f857c671dbaa`; 19,560,096 bytes |
| LakeCat runtime image | local Linux ARM64 image `sha256:7d1eab5295e46e7df06ee14ef807f71fe8e678cc7fa167ead4c4b85a177761e1`; 60,016,569 bytes |
| Rust | stable `rustc 1.97.1`; Cargo 1.97.1; LLVM 22.1.6 |
| production flags | `opt-level=3`, fat LTO, one codegen unit, stripped symbols, `panic=abort`, no debug/incremental, `-Dwarnings`, `-Ctarget-cpu=native`, locked, `-j1` |
| LakeCat dependencies | features `turso-local,sail-local`; Sail `bddb1706ba2308e5029d47f04f03121236edbfa6`; Turso `0.7.0-pre.10` |
| shared MinIO | `RELEASE.2025-10-15T17-29-55Z`; source `9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a` |

The public LakeCat branch underwent a privacy-only history rewrite before this
final C1-06 run. An isolated pre/post-rewrite comparison proved `Cargo.toml`,
`Cargo.lock`, and every crate source-identical at the canonical endpoint,
namespace, no-snapshot, and table milestones. The accepted profile, LakeCat
executable, image, transcripts, and MinIO audit above were rebuilt or rerun from
the reachable canonical source identity.

## Shared MinIO and cleanup proof

The five transcripts reference 16 distinct metadata objects: three each for
LakeCat, Gravitino, Polaris, and Nessie, and four for Lakekeeper because its
advertised optional first request admits one additional transition. A pinned
`mc` client on the same Docker network successfully statted all 16 objects
directly in MinIO.

| Catalog | Objects | Final observed object bytes | Final observed ETag |
| --- | ---: | ---: | --- |
| LakeCat | 3 | 1,278 | `3fd02c39afcea465d2a50da65d839015` |
| Apache Gravitino | 3 | 1,313 | `3a2918fb9e779d3b2bca052546b3e89c` |
| Apache Polaris | 3 | 1,365 | `3756b60eba2480e603dcd55e4d626817` |
| Apache Nessie | 3 | 985 | `29efbc33d2259219cb569a8ad780d745` |
| Lakekeeper | 4 | 504 | `8027815c9b34014ac6e97c851db34f16` |

Every invocation contains 21 operation slots. Cleanup is identical after pass
and fail outcomes: table drop 204, table-absence proof 404, namespace drop 204,
and namespace-absence proof 404. Cleanup requests `purgeRequested=false`, so the
catalog entries are gone while the exact metadata objects remain available for
the independent storage audit.

All five transcripts report `raw_secrets_persisted: false` and
`raw_response_body_persisted: false`. Every persisted authorization or
idempotency header equals `<redacted>`; a separate literal credential/token scan
also passes.

## Relationship to contention conflicts

This deterministic scenario and the concurrent ranking answer different
questions. C1-06 proves one accepted or rejected transition at a time. The
same-table contention benchmark deliberately races eight writers and measures
accepted throughput, correct HTTP 409 conflicts, and non-conflict errors.

LakeCat's high contention conflict rate is therefore not a Turso failure rate.
It is the expected result of several writers planning against one pointer and
then losing the optimistic race. See
[`catalog-bench/docs/CAS-CONFLICTS.md`](https://github.com/querygraph/catalog-bench/blob/c0637076dd4dc2ac871cdde393900dbe87f05583/docs/CAS-CONFLICTS.md)
for the full boundary.

## Reproduction and verification

Use the exact profile, scenario, Docker topology, and production recipe in the
catalog-bench report. Choose a new fixture and output path for every invocation;
the runner refuses to overwrite evidence or mutate a colliding fixture.

```sh
docker compose --profile conformance run --rm conformance commit \
  --profile /contracts/profiles/v1/current-2026-08-26.json \
  --scenario /contracts/scenarios/v1/iceberg-rest.commit.correctness.json \
  --catalog lakecat \
  --fixture-id review_lakecat_commit_01 \
  --output /evidence/review-lakecat-commit-01.json
```

Exit 0 means every required assertion passed. Exit 2 means a sanitized
conformance transcript was written with `fail` or `unsupported`; it is evidence,
not an invocation failure. Exit 1 means the contract, input, or I/O path failed.

LakeCat's focused implementation gates remain the memory/Turso commit,
requirement, pointer-log, idempotency, audit, outbox, and replay suites. Public
acceptance changes additionally require the unified book build and artifact
contract.

## Deliberate non-claims

- C1-06 is behavioral evidence, not a throughput, latency, variance, RSS, or
  conflict-rate ranking.
- It proves deterministic stale-schema rejection and pointer atomicity; it does
  not model ambiguous network failure or restart recovery.
- It makes no LakeCat standard-idempotency claim while the capability is not
  advertised.
- Its transcripts remain ignored smoke evidence. C1-09 owns immutable runnable
  profile/result materialization, manual redaction review, secret scanning,
  generated reports, and adversari.al publication.
