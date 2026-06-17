# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `56a4372 Disable automatic CI while crates publish`.
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

- `ExternalSecretRefCredentialResolver` can now resolve `vault://` refs through
  a Vault HTTP backend after TypeSec authorizes the exact secret URI.
- The service binary wires the Vault backend automatically when
  `LAKECAT_VAULT_ADDR` / `LAKECAT_VAULT_TOKEN` or `VAULT_ADDR` / `VAULT_TOKEN`
  are present; `LAKECAT_VAULT_NAMESPACE` / `VAULT_NAMESPACE` is also supported.
- Vault KV v1-style `{"data": {...}}` and KV v2-style
  `{"data": {"data": {...}}}` response shapes are converted into Iceberg REST
  credential config entries, with non-string values rejected.
- AWS Secrets Manager, GCP Secret Manager, and Azure Key Vault refs still fail
  closed with explicit not-configured errors.

## Verification Completed

- `cargo check -p lakecat-service --features typesec-local`
- `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_resolves_vault_secret_refs_after_authorization -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local environment_secret_resolver_parses_supported_secret_shapes -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `cargo fmt --all -- --check` (passes with existing stable-rustfmt warnings for
  nightly-only `imports_granularity` / `group_imports` config keys)
- `git diff --check`

## Next Recommended Slice

Add the next production secret-store resolver backend, or wait for Grust and
TypeSec to publish the needed crates, reproduce the GitHub Actions dependency
graph locally, re-enable automatic CI, and run the manual workflow once before
treating cloud CI as a gate again.
