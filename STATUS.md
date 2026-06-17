# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `231929c Document cloud CI publish gate`.
- Cloud CI remains gated on the publish chain: wait for Grust to publish the
  needed crates, then for TypeSec to publish its matching crates, then rebuild
  LakeCat in GitHub Actions against published crates rather than pinning CI to
  unpublished sibling checkout states.
- Current implementation slice adds TypeSec-gated production secret-ref
  dispatch for Vault, AWS Secrets Manager, GCP Secret Manager, and Azure Key
  Vault URI schemes. The dispatch authorizes the exact external secret URI
  before returning an explicit "provider backend not configured" error until
  real provider SDK resolvers are wired.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- Production secret-ref schemes now share the same TypeSec authorization path as
  the local `typesec://env/VARIABLE` resolver instead of being rejected before
  policy. Actual cloud/Vault provider backends remain pending.
- `ExternalSecretRefCredentialResolver` dispatches supported external secret-ref
  schemes after authorization, resolves `typesec://env/VARIABLE` through the
  local environment backend, and fails closed for unconfigured Vault/AWS/GCP/Azure
  providers with explicit errors.
- Tests cover provider classification, TypeSec authorization before production
  provider dispatch, and the existing environment-backed resolver path.

## Verification Completed

- `cargo fmt`
- `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_secret_ref_resolution -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_production_secret_refs_before_dispatch -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local environment_secret_resolver_parses_supported_secret_shapes -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `cargo fmt --all -- --check` (passes with existing stable-rustfmt warnings for
  nightly-only `imports_granularity` / `group_imports` config keys)
- `git diff --check`

## Next Recommended Slice

Add the first real production external secret-store resolver backend, such as
Vault or AWS Secrets Manager, behind the TypeSec-gated dispatch boundary.
