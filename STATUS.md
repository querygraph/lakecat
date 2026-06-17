# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `0bae214 Reject unsupported Sail unique constraints`.
- Current working slice adds an environment-backed `typesec://env/VARIABLE`
  secret-ref resolver for the `typesec-local` credential issuer, so local
  TypeSec-authorized credential vending can resolve external config without
  storing raw secrets in LakeCat catalog rows.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- `EnvironmentSecretRefCredentialResolver` resolves `typesec://env/VARIABLE`
  references after TypeSec authorizes `credentials.issue`.
- The service binary uses the environment resolver when built with
  `typesec-local`, replacing the previous no-op demo resolver.
- Environment secrets can be JSON objects of string config values or
  `ConfigEntry` arrays.
- Tests cover TypeSec-gated env resolution and parser failure modes without
  mutating process environment.
- GitHub Actions now checks out Grust `codex/cypher-write`, matching LakeCat's
  `grust-graph` 0.9.0 path dependency instead of the older default branch.

## Verification Completed

- `cargo fmt`
- `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_secret_ref_resolution -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local environment_secret_resolver_parses_supported_secret_shapes -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- Local CI-layout dependency resolution with sibling `lakecat`, `grust`, `sail`,
  and `typesec` checkouts under `/tmp/lakecat-ci-repro.0hBHY7`:
  `cargo metadata --format-version 1 --no-deps`
- `git diff --check`

## Next Recommended Slice

Add production external secret-store resolver backends such as Vault, AWS
Secrets Manager, GCP Secret Manager, or Azure Key Vault behind the same
TypeSec-gated resolver boundary.
