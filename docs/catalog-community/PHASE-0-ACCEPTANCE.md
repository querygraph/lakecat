# Phase 0 Acceptance Record

Phase 0 closed on 2026-08-26 with the baseline either reproduced or explained,
the neutral publication contract implemented in its owning repository, the
historical evidence migrated without changing its meaning, and LakeCat's public
book corrected to consume that evidence.

This record closes only **Phase 0: baseline and design**. It does not claim that
Lakekeeper, behavioral conformance, multi-engine workflows, failure injection,
migration, or Apache Ossie integration are implemented.

## Accepted revisions

| Repository | Accepted revision | Role |
| --- | --- | --- |
| `querygraph/catalog-bench` | `c0637076dd4dc2ac871cdde393900dbe87f05583` | Neutral v1 contracts, profiles, scenario, historical bundle, generated matrix, and validators. |
| `querygraph/lakecat` manuscript | `0b7f1fe5598d1cbac360f553a8d5a641600ffd73` | Corrected benchmark chapter and Phase 0 program documentation. |
| `querygraph/lakecat` artifact package | `1b0b9501b606f2eaa7d27600fca74095cc29f485` | PDF, EPUB, MOBI, HTML, chapter reader, and versioned artifact metadata generated from the manuscript revision above. |

The corresponding draft pull requests are
[`catalog-bench#2`](https://github.com/querygraph/catalog-bench/pull/2) and
[`lakecat#2`](https://github.com/querygraph/lakecat/pull/2).

## Historical reproduction verdict

The three preserved 2026-08-08 TSV artifacts retain these exact SHA-256 hashes:

| Artifact | SHA-256 |
| --- | --- |
| summary | `ce0730e6212c087d72fde2983830736e4989b29d3c361f1a00f32ea586b3bdd9` |
| all runs | `6aa5cd519aaa2e4c776be360394ea10d5be33ee130d8c7f3cd8b34eec2772819` |
| object audit | `9cdfb8bbbfef079cd0c934c81308aef1e7bf71bf10dd1e488fba1fd7e494a8c3` |

The deterministic importer verifies those bytes, exact four-catalog/six-round
coverage, request-rate arithmetic, object-count arithmetic, accepted-commit
growth, five-round medians and ranges, validity totals, and legacy rank fields.
It emits four aggregate records and a manifest, then the generated matrix ranks
only the three `pass` outcomes. Nessie's 190.0/s raw median remains diagnostic;
97 request errors and 0/5 valid measured rounds produce an unranked `fail`.

No new live timing is claimed. The historical LakeCat source check passes, but
Docker Desktop's VM reported `no space left on device` during the reproduction
audit. A closure-time `docker info` attempt again returned no server data from
the selected `desktop-linux` context. Deleting Docker images, volumes, or its
data store was not authorized, so the limitation remains explicit in the result
manifest. This satisfies the phase rule that a discrepancy must be explained;
it does not satisfy any later live-run gate.

## Clean-worktree catalog-bench gates

A detached worktree at `c0637076` began clean and remained clean after all
non-writing checks. The following commands passed on stable Rust:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo run -p catalog-bench-contract --locked -- schemas check
cargo run -p catalog-bench-contract --locked -- \
  validate profiles/v1 scenarios/v1 results/v1
cargo run -p catalog-bench-contract --locked -- \
  historical-import check --root .
cargo run -p catalog-bench-contract --locked -- bundle validate \
  --manifest results/v1/2026-08-08/manifest.json
cargo run -p catalog-bench-contract --locked -- matrix check \
  --manifest results/v1/2026-08-08/manifest.json \
  --output results/v1/2026-08-08/MATRIX.md
git diff --check
test -z "$(git status --porcelain)"
```

The workspace ran 21 tests: 3 process-report tests, 14 contract/profile/schema
tests, and 4 historical bundle/matrix/tamper tests. Eight checked-in contract
documents validated: two profiles, one scenario, four results, and one manifest.
The importer regenerated one scenario-linked four-result bundle, and the matrix
matched it exactly.

## Clean-worktree LakeCat documentation gates

A detached worktree at `1b0b9501` began clean. The unified build then regenerated
all artifacts from that checkout and passed:

```sh
docs/book/build.sh
git diff --check
scripts/check-book-artifact-contract.sh docs/book/dist
~/src/firstpair/publishing/scripts/check-version-marker.sh docs/book/dist
```

The build verified the pinned publishing toolchain, rendered eight Mermaid
diagrams, produced a 59-page PDF plus EPUB, MOBI, single-page HTML, and chapter
reader, and passed PDF layout, EPUB metadata/layout, artifact, library-book, and
version-marker checks. Poppler emitted its existing recursive-dictionary syntax
warnings while joining the PDF; every subsequent PDF parser/layout/contract check
passed.

## Exit-criteria mapping

- **Current results reproduce or discrepancies are explained:** raw hashes and
  arithmetic reproduce; the missing live Docker rerun is recorded in the
  baseline, manifest, matrix, book, and this acceptance record.
- **All planned versions are pinned:** the historical profile is runnable; the
  2026-08-26 candidate profile pins every selected component and explicitly
  rejects execution while five artifact identities remain unresolved.
- **Scenario/result schemas are reviewed:** closed v1 Rust ADTs, generated Draft
  2020-12 schemas, semantic validators, cross-bundle validation, docs, tests, and
  draft PR review surfaces exist in the neutral repository.
- **Ownership boundaries remain intact:** `DESIGN.md` assigns the harness to
  catalog-bench, format behavior to Sail, policy to TypeSec, graph behavior to
  Grust, semantic composition to QueryGraph, and only the thin physical catalog
  boundary to LakeCat. No LakeCat-local Ossie or benchmark implementation was
  added.

## Inputs carried into Phase 1

The current candidate profile remains deliberately `draft`. Before measured
Phase 1 runs, one same-Docker Linux ARM64 build environment must materialize and
hash the optimized catalog-bench, LakeCat, DuckDB, MinIO, and Iceberg Java
artifacts and emit a new runnable profile. Lakekeeper and PostgreSQL must then be
added to the shared catalog-bench MinIO/network topology. Smoke output is not
publishable performance evidence.
