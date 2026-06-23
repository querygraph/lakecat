# LakeCat Release Checklist

Use this checklist for the first LakeCat release while cloud CI remains
manual-only. A release is ready only when the evidence below is collected from
a clean local tree and the resulting documentation, book artifacts, and release
notes are committed and pushed.

## Scope

The first release covers the locally verifiable LakeCat catalog substrate:

- Standard Iceberg REST catalog behavior for config, namespaces, tables,
  metadata-pointer commits, table loads, and warehouse-prefixed routing.
- The Rust service spine, `CatalogStore` seam, memory store, Turso-backed local
  store, pointer CAS, idempotency, pointer logs, audit rows, and outbox rows.
- Replay admission before graph or OpenLineage projection for malformed table,
  namespace, view, management, credential, scan, commit, catalog-config, and
  QueryGraph bootstrap evidence.
- Governed scan/fetch and credential proof around TypeSec-style restrictions
  and Sail-planned work.
- QGLake handoff proof for bootstrap, management, scan/fetch, credentials,
  structural view receipt chains, table commit history, OpenLineage, Grust
  projection, and QueryGraph import evidence.
- Reader-facing docs that keep the same standard-vs-extension posture as the
  code: `README.md`, `DESIGN.md`, `STATUS.md`, `CHANGELOG.md`, and
  `docs/book/lakecat.md`.

The first release does not require typed Iceberg v4 semantics, cloud SDK-backed
secret managers, reusable graph algorithms beyond the Grust boundary already
exercised by LakeCat, or full QueryGraph product semantics.

## Preflight

Start from a clean repo:

```sh
git status --short --branch
```

Confirm dependency posture before the heavy gate:

```sh
scripts/check-local-dependency-contract.sh
scripts/check-release-version-contract.sh
```

This contract is part of the release. It proves that LakeCat's Grust feature
surface follows the active local Grust 0.10 path checkout, including the
`grust-turso-local` durable graph sink, TypeSec stays on the published TypeSec
crate, local Sail paths and patch bridge remain explicit, manual workflow
triggers remain intentional, and the local QueryGraph handoff verifier stays
aligned with the same active Grust path checkout.

## Required Local Gate

Run the broad local gate from the clean release candidate commit:

```sh
scripts/check-release-readiness.sh
```

The full gate must pass without `--skip-book` or `--skip-handoff` for a release
candidate. It covers shell syntax, dependency contracts, manual workflow trigger
contracts, release version consistency across all LakeCat crates and book
artifacts, formatting, default workspace tests, explicit Turso/Sail/TypeSec/
Grust feature tests, all-features CLI and workspace tests, book rebuild, EPUB
metadata and PDF layout validation, QGLake handoff replay verification, and
`git diff --check`.
The QGLake handoff proof must run QueryGraph `lakecat-verify` and
`lakecat-import` through `cargo run --locked` against the local `qg-rust`
manifest, then persist both outputs in the saved handoff summary.

Use the quick gate only while preparing a narrow slice:

```sh
scripts/check-release-readiness.sh --quick
```

The quick gate is not release evidence by itself.
Full runs that use `--skip-book` or `--skip-handoff` are also partial evidence;
the script labels them that way and they must not be used for a release
candidate.

## Book Artifacts

The full gate rebuilds the book, but release preparation should still inspect
the artifact contract before tagging:

```sh
docs/book/build.sh
scripts/check-release-version-contract.sh
expected_title=$(awk -F': ' '/^kindle_name:/ { print $2 }' docs/book/dist/VERSION.md)
docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub "$expected_title"
docs/book/check_pdf_layout.sh docs/book/dist/lakecat.pdf
readlink "docs/book/dist/$(awk -F': ' '/^kindle_link:/ { print $2 }' docs/book/dist/VERSION.md)"
```

Expected `readlink` output:

```text
lakecat.epub
```

If the source changed after the full gate, rebuild the book again and rerun the
quick gate before committing.

## Release Notes

Before tagging:

- Move the relevant `CHANGELOG.md` entries from `Unreleased` into a versioned
  release heading.
- Update `STATUS.md` with the final full-gate command and date.
- Update `README.md` if the release evidence date, dependency posture, feature
  gates, or first-release scope changed.
- Update `DESIGN.md` only if release scope or deferred scope changed.
- Rebuild and stage book artifacts when `docs/book/lakecat.md` or book metadata
  changed.

Do not claim cloud CI success unless a manual cloud run was intentionally
triggered after the same local gate passed. Cloud CI is not the release source
of truth yet.

## Tagging

After the full gate passes from the final clean candidate and release notes are
committed:

```sh
git status --short --branch
git tag -a v0.1.0 -m "LakeCat 0.1.0"
git push origin master
git push origin v0.1.0
```

Use a different tag only if `Cargo.toml` version has changed.

## Deferred Work Ledger

Keep these out of the first-release blocker list unless the user explicitly
expands scope:

- Typed Iceberg v4 support belongs in Sail and should replace LakeCat JSON
  passthrough when ready.
- Cloud SDK-backed secret managers belong behind the existing TypeSec-gated
  provider seam.
- Reusable graph taxonomy, traversal, stores, algorithms, and Cypher behavior
  belong in Grust.
- Full Croissant/CDIF/OSI/ODRL application composition and agentic workflow
  semantics belong in QueryGraph and TypeSec above LakeCat.
