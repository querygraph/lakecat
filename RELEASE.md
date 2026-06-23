# LakeCat Release Checklist

Use this checklist for the first LakeCat release while cloud CI remains
manual-only. A release is ready only when the evidence below is collected from
a clean local tree and the resulting documentation, book artifacts, and release
notes are committed and pushed.

## Published Baseline

`v0.1.0` is already tagged and pushed. Current `master` is post-tag hardening on
top of that first release baseline, not a reason to move or recreate the tag.
The release version contract verifies that the local `v0.1.0` tag is an
ancestor of the current tree when the tag is present, so follow-up release-gate
or Grust/Turso proof work can continue without rewriting published history.

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
scripts/check-release-proof-contract.sh
```

This contract is part of the release. It proves that LakeCat's Grust feature
surface follows the active local Grust 0.10 path checkout, including the
`grust-turso-local` durable graph sink and Grust-owned matched-node mutation
plan over projected LakeCat table nodes. It also proves that QGLake handoff
summaries bind the configured `lakecat_graph` Grust Turso table prefix, TypeSec
stays on the published TypeSec crate, local Sail paths and patch bridge remain
explicit, the checked-in Sail patch files match the local Sail helper commits
by stable `git patch-id`, manual workflow triggers remain intentional, and the
local QueryGraph handoff verifier stays aligned with the same active Grust path
checkout.

## Required Local Gate

Run the broad local gate from the clean release candidate commit:

```sh
scripts/check-release-readiness.sh --release-candidate
```

The full gate must pass without `--skip-book` or `--skip-handoff` for a release
candidate. It covers shell syntax, dependency contracts, manual workflow trigger
contracts, release version consistency across all LakeCat crates and book
artifacts, formatting, default workspace tests, explicit Turso/Sail/TypeSec/
Grust feature tests, all-features CLI and workspace tests, book rebuild, EPUB
metadata and PDF layout validation, QGLake handoff replay verification, and
`git diff --check`.
The gate also runs the Rust `lakecat-cli` `qglake_handoff` verifier tests as
an explicit row, so saved handoff summary, artifact-hash, graph-count,
QueryGraph import-plan, and self-verification drift fail with a focused
release-gate label before the broader all-features CLI row.
`--release-candidate` additionally requires the tree to be clean before and
after the complete full gate. In release-candidate mode the book build writes
to a temporary artifact directory via `LAKECAT_BOOK_DIST_DIR`, so the gate still
validates EPUB/PDF/MOBI generation without letting nondeterministic binary
metadata dirty the candidate commit.
The QGLake handoff proof must run QueryGraph `lakecat-verify` and
`lakecat-import` through `cargo run --locked` against the local `qg-rust`
manifest, then persist both outputs in the saved handoff summary.

After a full release-candidate proof has passed, a post-proof
documentation/book artifact refresh is allowed so the proof is recorded in
`README.md`, `DESIGN.md`, `STATUS.md`, `CHANGELOG.md`, and the book without
rerunning the heavy gate only to update the hash again. That refresh must stay
limited to documentation and checked-in book artifacts. Prove that shape with:

```sh
scripts/check-release-proof-contract.sh
```

If any Rust source, manifest, release script, workflow, dependency bridge, or
other executable behavior changes after the cited proof commit, rerun
`scripts/check-release-readiness.sh --release-candidate` from the new clean
candidate and refresh the proof references again.
The proof contract requires a clean working tree by default. While editing the
contract or release docs, use `LAKECAT_RELEASE_PROOF_ALLOW_DIRTY=1` only as a
local self-test; that mode still checks unstaged, staged, and untracked paths
against the post-proof allowlist.
The full release-candidate gate runs the same contract with
`LAKECAT_RELEASE_PROOF_CANDIDATE=1`. Candidate mode still requires a clean tree
and coherent active proof references, but it allows the current clean `HEAD` to
become the next proof commit so the proof-refresh documentation commit does not
create an infinite hash-update loop.

Use the quick gate only while preparing a narrow slice:

```sh
scripts/check-release-readiness.sh --quick
```

The quick gate is not release evidence by itself. It does validate the tracked
`docs/book/dist` artifact contract so narrow slices catch stale or malformed
book deliverables before the full release-candidate build regenerates them out
of tree.
Full runs that use `--skip-book` or `--skip-handoff` are also partial evidence;
the script labels them that way and they must not be used for a release
candidate.

## Book Artifacts

The release-candidate gate rebuilds the book into a temporary dist directory.
During ordinary development slices, edit `docs/book/lakecat.md` as the source
of truth and defer checked-in `docs/book/dist` regeneration until an explicit
finishing or release-proof step. Deliberate tracked artifact refreshes still
use the default build path, and release preparation should inspect the tracked
artifact contract before tagging:

```sh
docs/book/build.sh
scripts/check-book-artifact-contract.sh docs/book/dist
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

For the already-published `v0.1.0` baseline, do not move current post-tag
hardening out of `Unreleased` while the workspace version remains `0.1.0`.
Keep proof refs, status, and book artifacts current instead.

For a future version-bump release, before tagging:

- Move the relevant `CHANGELOG.md` entries from `Unreleased` into the new
  versioned release heading.
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

For the already-published `v0.1.0` baseline, do not run another tag command.
Post-`v0.1.0` hardening should keep changes under `Unreleased` until the
workspace version moves forward. The release version contract enforces that
post-tag state: if `v$workspace_version` already exists and `HEAD` is past that
tag, `CHANGELOG.md` must still contain an `Unreleased` section. For existing
tags, the same contract derives the expected versioned changelog heading date
from the tag creation date rather than the current day, so post-tag hardening
does not require rewriting published release notes.

For a future release where `Cargo.toml` has moved to a version without an
existing local tag, tag only after the full gate passes from the final clean
candidate and release notes are committed:

```sh
git status --short --branch
scripts/check-release-readiness.sh --release-candidate
version=$(awk '
  /^\[workspace\.package\]/ { in_workspace_package = 1; next }
  /^\[/ { in_workspace_package = 0 }
  in_workspace_package && /^version[[:space:]]*=/ {
    gsub(/"/, "", $3)
    print $3
    exit
  }
' Cargo.toml)
git tag -a "v$version" -m "LakeCat $version"
git push origin master
git push origin "v$version"
```

For post-`v0.1.0` hardening while `Cargo.toml` remains at `0.1.0`, keep the
local release gate green and cut the next tag only after the workspace version
and release notes move forward together.

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
