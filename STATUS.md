# LakeCat Status

Updated: 2026-06-16

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this pause: `81c83fe Capture TypeDID request identity envelopes`.
- Current working slice adds external secret-store references to governed storage
  profiles and keeps remote credential responses empty until a short-lived issuer
  is implemented.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## In Progress In This Commit

- `StorageProfile` now has an optional `secret_ref`.
- New credential issuance mode: `short-lived-secret-ref`.
- Storage profile management accepts and returns `secret-ref`.
- Turso persists the profile JSON, including `secret_ref`.
- Secret references are validated as external secret-store URIs such as
  `typesec://`, `vault://`, `aws-sm://`, `gcp-sm://`, or `azure-kv://`.
- Secret references reject obvious embedded raw secret material such as query
  parameters named `token`, `secret`, `password`, or `credential`.
- Remote credentials are still not vended. This is deliberate until a real
  short-lived credential issuer is connected.

## Verification To Run Before Commit

- `cargo fmt`
- `cargo test -p lakecat-service -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Implement a credential issuer trait that can mint short-lived scoped credentials
from a `StorageProfile` secret reference without exposing raw secrets through
catalog state, probably with a no-op/unsupported default and a TypeSec-backed
feature implementation once the TypeSec API shape is confirmed.
