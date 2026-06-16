# LakeCat Status

Updated: 2026-06-16

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `a700fc0 Add pluggable credential issuer`.
- Current working slice adds a `typesec-local` credential issuer that gates
  `typesec://` secret-ref resolution through TypeSec `credentials.issue` policy
  checks before returning scoped short-lived credential config.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## In Progress In This Commit

- `lakecat-service/typesec-local` now depends on the TypeSec facade directly.
- `TypeSecCredentialIssuer` checks `credentials.issue` against the requesting
  principal and `typesec://` secret-ref resource.
- `SecretRefCredentialResolver` is an injected boundary for resolving a
  policy-approved secret reference into scoped short-lived credential config.
- The service binary installs a no-op TypeSec issuer when `typesec-local` is
  enabled, preserving safe defaults until a real resolver backend is configured.
- Feature-gated tests cover allowed issuance and denied issuance.

## Verification To Run Before Commit

- `cargo fmt`
- `cargo test -p lakecat-service -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Implement real external secret-store resolver backends for `typesec://` profiles
and OIDC/cloud federation, keeping raw long-lived secret material out of LakeCat
catalog state.
