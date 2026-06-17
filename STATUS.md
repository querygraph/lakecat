# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `c49b359 Gate production secret refs through TypeSec`.
- Cloud CI remains gated on the publish chain: wait for Grust to publish the
  needed crates, then for TypeSec to publish its matching crates, then rebuild
  LakeCat in GitHub Actions against published crates rather than pinning CI to
  unpublished sibling checkout states.
- Automatic GitHub Actions CI is disabled while that publish gate is open. The
  workflow is manual-only via `workflow_dispatch` until the cloud dependency
  graph is locally reproduced and known to work.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- `.github/workflows/ci.yml` now has only `workflow_dispatch`; `push` and
  `pull_request` triggers are disabled until the Grust and TypeSec crate publish
  chain lands and the cloud dependency graph is reproduced locally.

## Verification Completed

- `git diff --check`

## Next Recommended Slice

After Grust and TypeSec publish the needed crates, reproduce the GitHub Actions
dependency graph locally, re-enable automatic CI, and then run the manual
workflow once before treating cloud CI as a gate again.
