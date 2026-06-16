# LakeCat Status

Updated: 2026-06-16

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `6e62202 Model storage profile secret refs`.
- Current working slice adds a pluggable credential issuer to governed credential
  vending. The default issuer remains conservative; integrations can now mint
  scoped short-lived credentials from storage-profile secret references.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## In Progress In This Commit

- `LakeCatState` now carries an object-safe `CredentialIssuer`.
- `ConservativeCredentialIssuer` is the default and preserves the previous safe
  behavior: local `file://` profiles return no-secret hints, remote profiles
  return no credentials.
- Credential vending calls the issuer after the typed `CredentialsVendCapability`
  is minted.
- The issuer receives the table record, matched storage profile, and
  authorization receipt.
- Tests include a recording issuer that vends mock short-lived credentials from
  a `short-lived-secret-ref` profile without exposing the `secret-ref` in the
  response.

## Verification To Run Before Commit

- `cargo fmt`
- `cargo test -p lakecat-service -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Implement a TypeSec-backed credential issuer that resolves `typesec://` secret
references and mints real short-lived cloud credentials, keeping raw long-lived
secret material out of LakeCat catalog state.
