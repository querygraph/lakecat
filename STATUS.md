# LakeCat Status

Updated: 2026-06-19

## Current State

- LakeCat is on `master`.
- Latest completed implementation slice:
  `Lift storage profile proof into handoff summary`.
  `scripts/qglake-handoff-local.sh` now writes
  `lakecatReplayVerification.storageProfileUpsertProof` as a compact
  `profileId`/`provider`/`secretRefPresent`/hash proof object in
  `handoff-summary.json`, while still failing closed if the source LakeCat
  replay JSON lacks the full redacted storage-profile upsert evidence.
- Local verification for the compact handoff storage-profile proof slice is
  green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed. The live
  handoff generated one table and one view, drained 26 outbox events, verified
  LakeCat replay through `qglake-verify-replay`, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, and wrote
  `lakecatReplayVerification.storageProfileUpsertProof` to
  `target/qglake-handoff/handoff-summary.json`;
  direct Node summary check for
  `lakecatReplayVerification.storageProfileUpsertProof`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Require storage profile proof in handoff`. `scripts/qglake-handoff-local.sh`
  now fails closed before writing `handoff-summary.json` unless LakeCat replay
  JSON includes `replay-evidence.management.storageProfileUpsert` with profile
  id, provider, explicit `secretRefPresent`, replay event hashes, and
  OpenLineage hashes. `GOAL.md` also now carries a dedicated book-workflow
  section requiring substantial workflow examples as LakeCat behavior lands.
- Local verification for the handoff storage-profile proof and goal guidance
  slice is green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  minimal Node replay-shape check for
  `replay-evidence.management.storageProfileUpsert`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-cli`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Print storage profile upsert replay proof`. `lakecat-cli
  qglake-verify-replay` now surfaces the redacted storage-profile upsert proof
  it already verifies: the management replay line reports
  `storage_profile_upserts` and `credential_roots`, and structured replay JSON
  includes a `storageProfileUpsert` object with profile id, provider,
  `secretRefPresent`, optional provider label, and replay/OpenLineage hashes.
- Local verification for the storage-profile replay output slice is green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_management_replay_line_summarizes_verified_evidence`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Verify storage profile upsert replay evidence`. Lineage-drain event
  summaries now lift redacted storage-profile upsert evidence into compact
  fields: profile id, provider, `secret-ref-present`, and
  `secret-ref-provider`. QGLake replay verification now requires storage-profile
  upsert replay to expose that credential-root proof, letting QueryGraph verify
  the catalog credential boundary without seeing secret-store URIs.
- Local verification for the storage-profile upsert replay evidence slice is
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `cargo test -p lakecat-cli qglake_management_replay_line_summarizes_verified_evidence`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Redact storage profile secret refs from replay`. Storage-profile management
  responses still return the full `secret-ref` to authorized operators, but
  `storage-profile.upserted` audit/outbox replay now carries only
  `secret-ref-present` and `secret-ref-provider` into lineage/OpenLineage
  evidence. The drain path also redacts legacy outbox payloads that still
  contain a full secret-store URI.
- Local verification for the storage-profile replay redaction slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service storage_profile_event_payload_redacts_secret_ref`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage`;
  `cargo test -p lakecat-service remote_storage_profile_accepts_secret_ref_without_vending_raw_secrets`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Reject secret-looking storage profile public config`. Storage profiles now
  reject `public-config` values that appear to embed raw tokens, passwords,
  access keys, or credential query parameters at the durable model boundary and
  through the management API. Public config remains available for non-secret
  routing hints such as region, endpoint labels, and operational purpose; raw
  credential material belongs behind `secret-ref` and the TypeSec-authorized
  resolver path.
- Local verification for the storage-profile public-config validation slice is
  green:
  `cargo fmt -p lakecat-store -p lakecat-service`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_public_config_secret_values`;
  `cargo test -p lakecat-service management_storage_profile_rejects_public_secret_values`;
  `cargo test -p lakecat-service remote_storage_profile_accepts_secret_ref_without_vending_raw_secrets`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Validate storage profile issuance modes`. Storage profiles now reject unsafe
  issuance/provider combinations at the durable model boundary and through the
  management API: `local-file-no-secret` is limited to file storage, and
  `short-lived-secret-ref` is limited to configured remote providers.
- Local verification for the storage-profile issuance-mode validation slice was
  green:
  `cargo fmt -p lakecat-store -p lakecat-service`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_provider_issuance_mismatch`;
  `cargo test -p lakecat-service management_storage_profile_rejects_remote_local_no_secret_mode`;
  `cargo test -p lakecat-service remote_storage_profile_accepts_secret_ref_without_vending_raw_secrets`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Validate storage profile providers`. Storage profile creation now rejects
  provider/location-prefix mismatches at the durable model boundary and through
  the management API, preventing contradictory credential roots such as a
  `file` provider over an `s3://` prefix. The book's storage-profile examples
  now use the current `provider`, `issuance-mode`, and `public-config`
  vocabulary.
- Local verification for the storage-profile provider validation slice was
  green:
  `cargo fmt -p lakecat-store -p lakecat-service`;
  `cargo test -p lakecat-service management_storage_profile_rejects_provider_prefix_mismatch`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_provider_location_mismatch`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_storage_profiles_and_matches_longest_prefix`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind metadata writes to storage profiles`. REST metadata-object commits now
  reject new metadata locations outside the table's matched storage profile
  prefix before touching object storage, keeping the metadata writer within the
  catalog's storage-profile boundary while preserving normal in-profile Iceberg
  commits.
- Local verification for the metadata storage-profile boundary slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service commit_rejects_metadata_object_outside_storage_profile_prefix`;
  `cargo test -p lakecat-service metadata_write_plan_requires_metadata_location`;
  `cargo test -p lakecat-service commit_can_advance_metadata_location_extension`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Preserve commit errors during cleanup`. Failed table commits still attempt
  to clean up newly written metadata objects, but cleanup failures now preserve
  the original store/CAS error class and append cleanup context instead of
  masking a commit conflict as a cleanup/internal failure.
- Local verification for the cleanup error-preservation slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service metadata_cleanup_failure_preserves_commit_conflict`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --all-features stale_commit_cleans_up_uncommitted_metadata_file`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require metadata write locations`. Metadata-write commit plans now fail
  closed when Sail or a future engine seam reports that a metadata object write
  is required but does not provide a concrete new metadata location, preventing
  catalog-pointer commits from succeeding without a corresponding metadata
  object.
- Local verification for the metadata write-location guard slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service metadata_write_plan_requires_metadata_location`;
  `cargo test -p lakecat-service commit_rejects_metadata_object_overwrite_of_current_pointer`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `git diff --check`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Guard current metadata pointer writes`. REST table commits now reject
  metadata-object writes whose target equals the table's current metadata
  pointer before writing through object storage, preventing the current metadata
  object from being overwritten before CAS/store validation.
- Local verification for the current metadata pointer guard slice was green:
  `cargo test -p lakecat-service commit_rejects_metadata_object_overwrite_of_current_pointer`;
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service commit_can_advance_metadata_location_extension`;
  `cargo test -p lakecat-service idempotent_commit_replay_does_not_rewrite_metadata_object`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`.
- Latest completed implementation slice:
  `Test QGLake replay JSON contract`. `lakecat-cli qglake-verify-replay
  --json` now builds its schema-versioned output through a testable helper, and
  the existing matching replay fixture asserts the replay JSON schema version
  plus structured scan, management, credential, and table-commit replay
  evidence fields.
- Local verification for the QGLake replay JSON contract test slice was green:
  `cargo fmt -p lakecat-cli`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`.
- Latest completed implementation slice:
  `Version QGLake handoff contracts`. `lakecat-cli qglake-verify-replay
  --json` now emits `schema-version:
  lakecat.qglake.replay-verification.v1`, and
  `scripts/qglake-handoff-local.sh` requires that replay schema before writing
  `handoff-summary.json` with `schemaVersion:
  lakecat.qglake.handoff-summary.v1`. The summary also records the replay
  schema under `lakecatReplayVerification.schemaVersion`.
- Local verification for the QGLake handoff contract-version slice was green:
  `cargo fmt -p lakecat-cli`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `git diff --check`;
  `cargo test -p lakecat-cli`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed;
  `node -e` JSON parse/assertion check for
  `target/qglake-handoff/handoff-summary.json`. The live handoff generated one
  table and one view, drained 26 outbox events, verified LakeCat replay through
  schema-versioned JSON output, ran QueryGraph `lakecat-verify` and
  `lakecat-import`, and wrote both the handoff summary schema and replay
  verification schema into the summary;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`.
- Latest completed implementation slice:
  `Structure QGLake replay evidence`. `lakecat-cli qglake-verify-replay --json`
  now emits structured scan, management, credential, and table-commit replay
  evidence in addition to the human-readable replay lines. The local handoff
  summary embeds that replay evidence under
  `lakecatReplayVerification.replayEvidence`, so automation can read compact
  proof counts and replay/OpenLineage hashes without parsing terminal text.
- Local verification for the structured replay evidence slice was green:
  `cargo fmt -p lakecat-cli`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `git diff --check`;
  `cargo test -p lakecat-cli`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed;
  `node -e` JSON parse/assertion check for
  `target/qglake-handoff/handoff-summary.json`. The live handoff generated one
  table and one view, drained 26 outbox events, verified LakeCat replay through
  JSON output, ran QueryGraph `lakecat-verify` and `lakecat-import`, and wrote
  structured scan task counts, management counts, credential replay proof, and
  table commit history proof into the summary;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`.
- Latest completed implementation slice:
  `Cross-check handoff standards and semantic hashes`. `lakecat-cli
  qglake-verify-replay --json` now exposes graph hash, OpenLineage hash, and
  standards from the verified QueryGraph bootstrap bundle. The local handoff
  harness requires LakeCat replay, QueryGraph verify, and QueryGraph import to
  agree on graph/OpenLineage hashes and the standards list before accepting the
  handoff summary, and the summary now embeds the accepted standards list.
- Local verification for the handoff standards/hash cross-check was green:
  `cargo fmt -p lakecat-cli`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `git diff --check`;
  `cargo test -p lakecat-cli`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed;
  `node -e` JSON parse/assertion check for
  `target/qglake-handoff/handoff-summary.json`. The live handoff generated one
  table and one view, drained 26 outbox events, verified LakeCat replay through
  JSON output, ran QueryGraph `lakecat-verify` and `lakecat-import`, and wrote
  `querygraphVerification.standards` with Iceberg REST, Croissant, CDIF, OSI
  handoff, ODRL, Grust catalog graph, and OpenLineage after all three phases
  agreed on the semantic hashes;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind handoff summary to LakeCat replay JSON`. `lakecat-cli
  qglake-verify-replay --json` now emits machine-readable replay verification,
  and `scripts/qglake-handoff-local.sh` requires LakeCat replay status, table
  count, view count, bundle hash, and QueryGraph import hash to match the
  QueryGraph verify/import outputs before writing the accepted handoff summary.
- Local verification for the LakeCat replay JSON handoff slice was green:
  `cargo fmt -p lakecat-cli`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed;
  `node -e` JSON parse/assertion check for
  `target/qglake-handoff/handoff-summary.json`. The live handoff generated one
  table and one view, drained 26 outbox events, verified LakeCat replay through
  JSON output, ran QueryGraph `lakecat-verify` and `lakecat-import`, and wrote
  `lakecatReplayVerification.matchesQueryGraph=true` plus
  `querygraphImportVerification.matchesVerify=true`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`.
- Latest completed implementation slice:
  `Cross-check QueryGraph handoff phases`. `scripts/qglake-handoff-local.sh`
  now fails closed unless QueryGraph `lakecat-verify` and `lakecat-import`
  agree on table/view counts and semantic bundle, graph, OpenLineage, and
  QueryGraph import hashes before writing the verified handoff summary.
- Local verification for the QueryGraph handoff phase cross-check was green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed;
  `node -e` JSON parse/assertion check for
  `target/qglake-handoff/handoff-summary.json`. The live handoff generated one
  table and one view, drained 26 outbox events, verified LakeCat replay, ran
  QueryGraph `lakecat-verify` and `lakecat-import`, and wrote
  `querygraphImportVerification.matchesVerify=true` in the summary after the
  semantic counts and hashes matched;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `git diff --check`;
  `cargo test -p lakecat-cli`.
- Latest completed implementation slice:
  `Embed QueryGraph handoff verification`. `scripts/qglake-handoff-local.sh`
  now parses QueryGraph's verifier JSON and embeds verified table/view counts
  plus semantic bundle, graph, OpenLineage, and QueryGraph import hashes in
  `target/qglake-handoff/handoff-summary.json`, alongside raw file hashes for
  generated artifacts.
- Local verification for the embedded handoff verification slice was green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed;
  `node -e` JSON parse/assertion check for
  `target/qglake-handoff/handoff-summary.json`. The live handoff generated one
  table and one view, drained 26 outbox events, verified LakeCat replay, ran
  QueryGraph `lakecat-verify` and `lakecat-import`, and embedded
  `querygraphVerification` with table/view counts and semantic hashes in the
  summary;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `git diff --check`;
  `cargo test -p lakecat-cli`.
- Latest completed implementation slice:
  `Summarize local QGLake handoff outputs`. `scripts/qglake-handoff-local.sh`
  now captures LakeCat replay output, QueryGraph verify output, and QueryGraph
  import output, then writes `target/qglake-handoff/handoff-summary.json` with
  the accepted artifact paths, file hashes, catalog URL, principal, table
  scope, and service log path for operators and automation.
- Local verification for the handoff summary slice was green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed;
  `node -e` JSON parse check for `target/qglake-handoff/handoff-summary.json`.
  The live handoff generated one table and one view, drained 26 outbox events,
  verified LakeCat replay, ran QueryGraph `lakecat-verify` and
  `lakecat-import`, and wrote captured outputs plus
  `target/qglake-handoff/handoff-summary.json`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `git diff --check`;
  `cargo test -p lakecat-cli`.
- Latest completed implementation slice:
  `Prove local QGLake handoff`. Added `scripts/qglake-handoff-local.sh`, which
  starts a local LakeCat service, generates paired QGLake bootstrap and
  lineage-drain artifacts, verifies saved replay with LakeCat, and runs
  QueryGraph's `lakecat-verify` and `lakecat-import` over the same bundle while
  keeping generated artifacts under LakeCat's `target/qglake-handoff/`.
- Local verification for the local handoff slice was green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed. The
  live handoff generated one table and one view, drained 26 outbox events,
  printed compact scan/management/credential/commit replay evidence, verified
  bundle `sha256:1b6e2f869effaf660944eeea6fdc129a27f03a0a9f8a97357f3e4a1f8e7103b7`,
  and wrote QueryGraph import plan
  `target/qglake-handoff/querygraph-import-plan.json`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `git diff --check`;
  `cargo test -p lakecat-cli`.
- Latest completed implementation slice:
  `Verify QGLake scan replay counts`. `LineageDrainEventSummary` now carries
  compact scan-plan, file-scan, delete-file, and child-plan task counts from
  scan/fetch outbox payloads. `lakecat-cli qglake-verify-replay` prints a
  compact scan replay line and QGLake saved replay now rejects drains that do
  not prove both `table.scan-planned` and `table.scan-tasks-fetched` evidence,
  including delete-file counts for governed Sail-planned reads.
- Local verification for the QGLake scan replay slice was green:
  `cargo test -p lakecat-cli qglake_scan_replay_line_summarizes_verified_evidence`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `git diff --check`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Verify QGLake credential replay reasons`. `lakecat-cli qglake-verify-replay`
  now requires the trusted-human raw credential exception reason to survive
  lineage replay, and prints compact restricted-agent and trusted-human
  credential replay evidence after accepting a saved bootstrap bundle and drain.
- Local verification for the QGLake credential replay slice was green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_credential_replay_line_summarizes_verified_evidence`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-cli`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Print QGLake management replay summary`. `lakecat-cli qglake-verify-replay`
  now prints compact management replay counts for servers, projects,
  warehouses, policy bindings, and storage profiles after accepting a saved
  bootstrap bundle and lineage drain, making the durable tenant spine and
  control-plane reads visible to QueryGraph handoff scripts.
- Local verification for the QGLake management replay-output slice was green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_management_replay_line_summarizes_verified_evidence`;
  `cargo test -p lakecat-cli qglake_commit_history_replay_line_summarizes_verified_evidence`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Print QGLake commit replay summary`. `lakecat-cli qglake-verify-replay` now
  prints the verified `table.commits-listed` replay summary after accepting a
  saved bootstrap bundle and lineage drain, including compact commit count,
  sequence numbers, and commit hashes for QueryGraph/operator handoff.
- Local verification for the QGLake replay-output slice was green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_commit_history_replay_line_summarizes_verified_evidence`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed documentation slice:
  `Pin AGENTS guidance in GOAL`. `GOAL.md` now carries the current
  `/Users/alexy/src/lakecat/AGENTS.md` contract as durable goal guidance with
  explicit repo-boundary, compatibility, implementation-priority, verification,
  Turso, graph-placement, and commit-discipline sections.
- Local verification for the GOAL guidance slice was documentation-only:
  `git diff --check`.
- Latest completed implementation slice:
  `Summarize commit history in lineage drain`. `LineageDrainEventSummary` now
  carries compact `table-commit-count`, `table-commit-sequence-numbers`, and
  `table-commit-hashes` fields for `table.commits-listed` replay. The service
  fills them from the existing commit-history outbox payload, and QGLake now
  rejects lineage drains that replay table commit history without this typed
  summary evidence.
- Local verification for the commit-history lineage-summary slice was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service management_table_commits_lists_pointer_log_evidence`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Require QGLake commit summary evidence`. QGLake commit-history acceptance now
  factors compact pointer-log record checks through a shared verifier and
  requires the record to preserve the fixture table's Iceberg format-version and
  current snapshot summary, not just generic hashes and principal/idempotency
  evidence. The new CLI regression rejects commit-history evidence that omits
  the format/snapshot summary before QueryGraph handoff is accepted.
- Local verification for the QGLake commit-summary acceptance slice was green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_commit_history_verifier_requires_iceberg_summary`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Bind QGLake replay to commit history`. The QGLake fixture now performs an
  idempotent no-op table commit-history probe, reads the governed compact
  pointer-log endpoint, verifies sequence/request/response/idempotency/principal
  evidence, and rejects lineage drains that do not replay
  `table.commits-listed` receipt hashes. Saved replay artifact verification now
  uses the same acceptance contract, so QueryGraph handoff evidence includes
  commit-history inspection without adding graph mechanics to LakeCat.
- Local verification for the QGLake commit-history replay slice was green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Expose table commit history evidence`. LakeCat now serves a governed
  management read at
  `GET /management/v1/warehouses/{warehouse}/namespaces/{namespace}/tables/{table}/commits`
  that returns compact pointer-log records with request/response hashes,
  idempotency-key hash, format version, snapshot id, policy hash, principal, and
  commit hash. The read records a `table.commits-listed` audit/outbox event and
  drains as LakeCat OpenLineage evidence without adding graph semantics in
  LakeCat.
- Local verification for the table commit-history slice was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-lineage`;
  `cargo test -p lakecat-service management_table_commits_lists_pointer_log_evidence`;
  `cargo test -p lakecat-lineage`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Prove idempotent commits skip metadata rewrites`. The service now has a
  regression that commits a metadata object with an idempotency key, mutates the
  object on disk, retries the exact REST commit, and proves LakeCat returns the
  stored response without invoking Sail or rewriting the metadata object. This
  keeps the P3 commit spine honest: idempotent replay is a pre-storage replay,
  not merely a matching final response.
- Local verification for the idempotent metadata-rewrite slice was green:
  `cargo test -p lakecat-service idempotent_commit_replay_does_not_rewrite_metadata_object`;
  `cargo fmt -p lakecat-service`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Summarize commit metadata in pointer logs`. `TableCommitRecord` now carries
  compact `format_version`, `snapshot_id`, and `policy_hash` evidence alongside
  request/response hashes. Memory/Turso commit paths populate the fields from
  committed metadata and authorization receipts, Turso coverage checks the
  durable commit record/outbox payload, and graph/lineage replay fixtures prove
  those fields survive projection.
- Local verification for the commit summary slice was green:
  `cargo fmt -p lakecat-store -p lakecat-service`;
  `cargo test -p lakecat-store turso_store_round_trips_namespaces_tables_and_idempotent_commits --features turso-local`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Hash commit responses in pointer logs`. `TableCommitRecord` now carries a
  durable `response_hash` over the committed table response alongside the
  request hash. Memory/Turso commit paths populate it, Turso replay coverage
  checks it against the idempotent response, and graph/lineage replay fixtures
  prove the hash survives outbox projection for QueryGraph/audit consumers.
- Local verification for the commit response-hash slice was green:
  `cargo fmt -p lakecat-store -p lakecat-service`;
  `cargo test -p lakecat-store turso_store_round_trips_namespaces_tables_and_idempotent_commits --features turso-local`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Replay commit idempotency before Sail`. `CatalogStore` now exposes a
  side-effect-free table-commit replay probe, implemented for memory and Turso
  stores, so exact REST commit retries return the stored response before Sail
  commit validation, current-pointer loading, or metadata-object writes. The
  service regression proves a retry with an originally valid but now-stale
  commit requirement still replays safely after the table has advanced.
- Local verification for the commit idempotency replay slice was green:
  `cargo fmt -p lakecat-store -p lakecat-service`;
  `cargo test -p lakecat-store turso_store_round_trips_namespaces_tables_and_idempotent_commits --features turso-local`;
  `cargo test -p lakecat-service --features sail-local idempotent_commit_replay_skips_stale_sail_revalidation`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-store -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Verify v4 extension plan tokens`. `lakecat-sail` now proves that a signed
  format-version 4 JSON-bridge manifest-list plan token can be revalidated
  during `fetchScanTasks` with required projection/filter context, while
  drifted manifest-list metadata is rejected without claiming typed v4 Sail
  support.
- Local verification for the v4 extension plan-token slice was green:
  `cargo fmt -p lakecat-sail`;
  `cargo test -p lakecat-sail --all-features v4`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-sail --all-features`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Add local dependency contract audit`. The new
  `scripts/check-local-dependency-contract.sh` script checks the versioned local
  Grust/TypeSec pins, the Sail local path bridge, the CI Sail patch bridge, and
  the manual-only CI trigger. Manual CI now runs the same audit after checking
  out Sail, Grust, and TypeSec.
- Local verification for the dependency contract audit slice was green:
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -p lakecat-cli -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Add v4 extension bridge fixtures`. `lakecat-sail` now has focused tests that
  prove JSON-summary inspection, manifest-list scan planning, and stable
  commit-requirement validation for format-version 4 metadata without claiming
  typed v4 support.
- Local verification for the v4 extension bridge fixture slice was green:
  `cargo fmt -p lakecat-sail`;
  `cargo test -p lakecat-sail --all-features v4 -- --nocapture`;
  `cargo fmt -p lakecat-sail -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-sail --all-features v4`;
  `cargo test -p lakecat-sail --all-features`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Verify view receipt chains`. Governed namespace view receipt-chain reads now
  expose `chain-verified`, lineage summaries carry a verified-chain count, and
  QGLake dropped-view acceptance requires the namespace chain to be both hashed
  and verified.
- Local verification for the verified view receipt-chain slice was green:
  `cargo check -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -p lakecat-sail -- --check`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Add view receipt chain hashes`. Governed namespace view receipt-chain reads
  now expose deterministic `chain-hash` values for each chain, lineage-drain
  summaries carry `view-version-receipt-chain-hashes`, and QGLake dropped-view
  acceptance requires that compact chain proof in addition to per-receipt
  tombstone hashes.
- Local verification for the view receipt chain-hash slice was green:
  `cargo check -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -p lakecat-sail -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`.
- Latest doc-only goal guidance slice:
  `Pin agent contract in goal`. `GOAL.md` now explicitly pins the pasted
  `/Users/alexy/src/lakecat/AGENTS.md` instructions as the durable operating
  contract for repo boundaries, compatibility rules, implementation priorities,
  verification, and commit discipline.
- Latest committed LakeCat implementation slice:
  `fa392d5 Chain view version receipts`.
- Paused after adding compact hash-chain links to durable view-version
  receipts. Memory and Turso stores now attach `previous-receipt-hash` to each
  view upsert/drop receipt after the first receipt for a view. Governed
  `version-receipts` and namespace receipt-chain responses expose the link, so
  QueryGraph/operators can validate ordered view history without reading
  backend storage or adding custom Iceberg metadata. This moves LakeCat toward
  Iceberg view commit/history semantics while keeping full Sail-aligned view
  history work pending.
- Local verification for the view receipt-chain link slice was green:
  `cargo fmt -p lakecat-store -p lakecat-api -p lakecat-service -- --check`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store turso_store_persists_view_records --features turso-local`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Previous committed LakeCat implementation slice:
  `3b4d8ed Prove QGLake handoff through QueryGraph`.
- Paused after proving a regenerated Sail-backed QGLake handoff through both
  LakeCat and QueryGraph. QGLake policy fixtures now use canonical Iceberg REST
  `not-eq` filter spelling; lineage-drain summaries preserve TypeSec
  request-identity evidence from the current
  `authorization-receipt.context.request-identity` receipt shape; QueryGraph
  bootstrap graph projection deduplicates shared namespace nodes when a table
  and view live in the same namespace; and the LakeCat book/design docs now
  show the QueryGraph verify/import workflow.
- Live proof artifacts were regenerated with:
  `LAKECAT_TURSO_PATH=target/qglake-proof-sail4/catalog.db
  LAKECAT_BIND_ADDR=127.0.0.1:18286 cargo run -p lakecat-service --features
  turso-local,sail-local`; `cargo run -p lakecat-cli -- qglake-fixture
  --catalog http://127.0.0.1:18286 --output
  target/qglake-proof-sail4/lakecat-bootstrap.json --drain-output
  target/qglake-proof-sail4/lineage-drain.json --principal
  did:example:agent`. The fixture wrote one table, drained 23 lineage/outbox
  events, and produced bundle hash
  `sha256:d779f54266cefc8b729b9e9a56b9dfcb695448c12b0cb44655fa7fd113056107`.
- LakeCat offline replay verification passed:
  `cargo run -p lakecat-cli -- qglake-verify-replay --bundle
  target/qglake-proof-sail4/lakecat-bootstrap.json --drain
  target/qglake-proof-sail4/lineage-drain.json --principal
  did:example:agent`, proving QueryGraph import hash
  `sha256:dbe7f5178d29bf59b47e746dd26ebff9c3358cfadac2c96eb5901d19dee535eb`,
  one table, and one view.
- QueryGraph Rust verification passed against the same bundle:
  `cargo run -- lakecat-verify --bundle
  /Users/alexy/src/lakecat/target/qglake-proof-sail4/lakecat-bootstrap.json`
  and `cargo run -- lakecat-import --bundle
  /Users/alexy/src/lakecat/target/qglake-proof-sail4/lakecat-bootstrap.json
  --output .querygraph/lakecat/live-proof-import-plan.json`. QueryGraph
  verified one table, one view, graph hash
  `sha256:2eaab8b578455290226bc7fa314c79ea28c16c0e850ddbf32926a2d93ca16471`,
  OpenLineage hash
  `sha256:593a01b31d84c468c8eb60db9c864bc65ca625f4e0556c0b71efcac5f873d3cb`,
  and the same QueryGraph import hash above. The generated import plan has 5
  graph nodes, 4 graph edges, one table, and one view. The generated
  `.querygraph/` artifact is ignored in the QueryGraph repo and was not staged.
- Local verification for the QGLake handoff-through-QueryGraph slice was green:
  `cargo fmt -p lakecat-querygraph -p lakecat-cli -p lakecat-service -- --check`;
  `cargo test -p lakecat-querygraph`;
  `cargo test -p lakecat-cli qglake_fixture_policy_installs_read_restriction`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox -- --nocapture`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`.
- QueryGraph focused verification for the companion importer update was green:
  `cargo fmt -- --check`; `cargo test lakecat -- --nocapture`;
  `cargo run -- lakecat-verify --bundle
  /Users/alexy/src/lakecat/target/qglake-proof-sail4/lakecat-bootstrap.json`;
  `cargo run -- lakecat-import --bundle
  /Users/alexy/src/lakecat/target/qglake-proof-sail4/lakecat-bootstrap.json
  --output .querygraph/lakecat/live-proof-import-plan.json`;
  `git diff --check -- Cargo.toml Cargo.lock src/lakecat.rs`.
- Latest committed status slice:
  `Record QGLake QueryGraph proof status`.
- Latest committed goal-guidance/docs slice:
  `c285958 Reconcile goal with agent guidance`.
- Previous committed LakeCat implementation slice:
  `2db1d32 Verify QGLake replay artifacts offline`.
- Paused after adding offline QGLake handoff verification. `lakecat-cli
  qglake-fixture` now accepts `--drain-output` so a local fixture run can save
  the QueryGraph bootstrap bundle and the lineage-drain response as paired JSON
  artifacts. `lakecat-cli qglake-verify-replay` reads those saved artifacts,
  verifies the bundle manifest and QueryGraph import-compatibility contract,
  then applies the existing QGLake lineage-drain acceptance checks against the
  bundle-derived hashes, policy-binding count, credential replay evidence,
  management-list replay evidence, and view receipt evidence when views are
  present.
- Local verification for the offline QGLake replay verifier slice was green:
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `cargo fmt -p lakecat-cli`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli`;
  `cargo check -p lakecat-cli`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo fmt -p lakecat-cli -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `cargo test --workspace --all-features`.
- Previous committed LakeCat implementation slice:
  `0651ccb Bind QueryGraph import to view receipts`.
- Paused after adding compact view receipt evidence to the QueryGraph import
  compatibility contract. View-bearing bootstrap bundles now carry one
  manifest-covered `view-receipt-evidence` record per exported view version,
  plus a receipt-evidence hash; `QueryGraphBootstrap::verify_manifest` rejects
  view bundles without matching receipt evidence, the service attaches the
  store-derived receipt hashes before recording `querygraph.bootstrap`, and
  QGLake validates the import contract for view-bearing bundles.
- Previous committed LakeCat implementation slice:
  `43d4991 Require QGLake view receipt chains`.
- Paused after making QGLake consume the namespace-level receipt-chain read as
  acceptance evidence. The fixture now drops its accepted transient view, checks
  the governed per-view receipt list, checks the governed namespace-level
  `view-version-receipt-chains` read for a tombstoned chain with hashed drop
  receipts, and rejects lineage drains that do not replay
  `view.version-receipt-chains-listed` as compact lineage evidence. The catalog
  config response now advertises the receipt-chain endpoint.
- Local verification for the QGLake receipt-chain acceptance slice was green:
  `cargo fmt -p lakecat-cli -p lakecat-api`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-cli -p lakecat-api -- --check`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service config_endpoint_reports_lakecat_capabilities`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo fmt -p lakecat-api -p lakecat-cli -p lakecat-service -p lakecat-sail -- --check`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Previous committed LakeCat implementation slice:
  `4b5a6ed Expose view receipt chains`.
- Paused after adding a namespace-level governed management read for active and
  tombstoned view receipt chains:
  `GET /management/v1/warehouses/{warehouse}/namespaces/{namespace}/view-version-receipt-chains`.
  Memory and Turso stores can now list view-version receipts by namespace; the
  service groups them by stable view id, exposes latest operation/version,
  tombstone state, and receipt counts, and records
  `view.version-receipt-chains-listed` audit/outbox evidence. The read projects
  as compact lineage evidence only, leaving richer graph topology and query
  behavior to Grust and QueryGraph.
- Local verification for the view receipt-chain slice was green:
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -p lakecat-lineage`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store turso_store_persists_view_records --features turso-local`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-lineage`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -p lakecat-lineage -p lakecat-cli -p lakecat-sail -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `6148f26 Require QGLake view tombstone receipts`.
- Paused after binding QGLake lineage-drain acceptance to view tombstone
  receipt evidence. The QGLake fixture now creates a transient catalog view,
  accepts a QueryGraph bootstrap containing that view, drops the view, reads the
  governed view-version receipt chain, and then requires lineage-drain replay
  to include `view.dropped` plus `view.version-receipts-listed` evidence with a
  non-empty tombstone receipt hash. LakeCat projects the receipt-chain read as
  lineage evidence only, leaving reusable graph topology to Grust.
- Local verification for the QGLake view tombstone acceptance slice was green:
  `cargo fmt -p lakecat-cli -p lakecat-service -p lakecat-lineage -p lakecat-api`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `cargo fmt -p lakecat-cli -p lakecat-service -p lakecat-lineage -p lakecat-api -- --check`;
  `cargo test -p lakecat-lineage`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `aeb266e Record view drop tombstones`.
- Previous committed goal-guidance/docs slice:
  `e9bd014 Mirror agent guidance into goal`.
- Paused after adding compact view drop/tombstone receipts to the durable
  view-version receipt chain. Memory and Turso stores now append a `drop`
  receipt when a view is deleted, preserving the last durable `view-version`,
  stable view id, previous version, content hash, principal, and timestamp
  after the current view row is removed. The governed receipt endpoint remains
  readable after a drop so QueryGraph/operators can verify tombstones without
  using custom Iceberg metadata or backend-specific storage access.
- Local verification for the view drop/tombstone receipt slice was green:
  `cargo fmt -p lakecat-store -p lakecat-service -p lakecat-sail -p lakecat-api`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store turso_store_persists_view_records --features turso-local`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo test -p lakecat-sail provider_manages_durable_views_with_typed_columns`
  (compiled only; test is behind `catalog-provider`);
  `cargo test -p lakecat-sail --features catalog-provider provider_manages_durable_views_with_typed_columns`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-store -p lakecat-api -p lakecat-service -p lakecat-cli -p lakecat-sail -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `ed1be2f Expose view version receipt reads`.
- Paused after adding a governed read-side management endpoint for compact
  view-version receipts:
  `GET /management/v1/warehouses/{warehouse}/namespaces/{namespace}/views/{view}/version-receipts`.
  The endpoint uses the view-load authorization path, returns compact receipt
  records plus receipt hashes, and records a `view.version-receipts-listed`
  audit/outbox event so QueryGraph/operators can inspect the durable receipt
  chain without using backend-specific storage access or non-standard Iceberg
  metadata.
- Local verification for the view-version receipt read slice was green:
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-api -p lakecat-service -- --check`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-store -p lakecat-cli -p lakecat-sail -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `9433b18 Persist view version receipts`.
- Paused after adding compact durable view-version receipts to memory and Turso
  stores. Each view upsert now records the store-assigned version, previous
  version, stable view id, content hash, principal, operation, and timestamp;
  QueryGraph bootstrap audit/outbox payloads include matching compact receipt
  hashes; lineage-drain summaries expose those hashes; and QGLake rejects
  view-bearing replay that omits them.
- Local verification for the view-version receipt slice was green:
  `cargo fmt -p lakecat-store -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store turso_store_persists_view_records --features turso-local`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog_views`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-store -p lakecat-api -p lakecat-service -p lakecat-cli -p lakecat-sail -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `67ebd4f Bind QGLake view replay to view version`.
- Paused after adding compact `view-version` evidence to lineage-drain event
  summaries and binding QGLake view replay acceptance to the accepted
  QueryGraph bootstrap view artifact version. QueryGraph verification now
  exports a stable-id-to-version map for views, service replay summaries expose
  replayed view versions, and QGLake rejects replay that reports a stale or
  missing version for a currently verified view.
- Local verification for the QGLake view-version replay slice was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -p lakecat-querygraph`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-querygraph projects_catalog_views_into_querygraph_bundle`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -p lakecat-querygraph -p lakecat-sail -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `3f45f69 Add durable view version evidence`.
- Paused after adding store-assigned durable `view-version` counters to
  `ViewRecord` values in memory and Turso stores. View responses now expose the
  current version, QueryGraph bootstrap carries it through view graph nodes,
  OSI handoff, and OpenLineage facets, and the docs/book describe it as the
  first bridge toward full Iceberg view history and commit semantics.
- Local verification for the durable view-version evidence slice was green:
  `cargo fmt -p lakecat-store -p lakecat-api -p lakecat-service -p lakecat-querygraph`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store turso_store_persists_view_records --features turso-local`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo test -p lakecat-querygraph projects_catalog_views_into_querygraph_bundle`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-querygraph`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-store -p lakecat-api -p lakecat-service -p lakecat-querygraph -p lakecat-sail -- --check`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `28d9939 Require QGLake tenant spine list replay`.
- Paused after making QGLake acceptance establish and list its durable
  server/project/warehouse tenant spine. The fixture now upserts the
  `lakecat-local` server, `default` project, and selected warehouse before
  table setup, lists each management level, and lineage-drain verification
  rejects runs that do not replay matching `server.listed`, `project.listed`,
  and `warehouse.listed` count evidence with sink receipt hashes alongside the
  existing policy and storage-profile list evidence.
- Local verification for the QGLake tenant-spine list acceptance slice was
  green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo check -p lakecat-cli`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-cli -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `b92e8d9 Require QGLake storage profile list replay`.
- Paused after making QGLake acceptance exercise the governed
  storage-profile-list management read. The QGLake fixture now lists warehouse
  storage profiles after installing its local storage profile, and
  lineage-drain verification rejects runs that do not replay matching compact
  `storage-profile.listed` count/scope evidence with sink receipt hashes
  alongside the existing policy-list replay evidence.
- Local verification for the QGLake storage-profile-list acceptance slice was
  green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo check -p lakecat-cli`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-cli -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `b7fd69f Require QGLake policy list replay`.
- Paused after making QGLake acceptance exercise the governed policy-list
  management read. The QGLake fixture now lists warehouse policy bindings after
  installing the restricted read policy, and lineage-drain verification rejects
  runs that do not replay matching compact `policy-binding.listed` count/scope
  evidence with sink receipt hashes.
- Local verification for the QGLake policy-list acceptance slice was green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo check -p lakecat-cli`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `0a1a596 Summarize management list replay counts`.
- Paused after adding compact management-list replay evidence to lineage-drain
  event summaries. Replayed policy-binding, project, server, storage-profile,
  and warehouse list events now expose typed count/scope fields in the drain
  response so QueryGraph can verify durable control-plane read evidence without
  parsing raw lineage payloads or depending on list-specific graph nodes in
  LakeCat.
- Local verification for the management-list summary slice was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_management_list_reads_to_lineage`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `0a0e414 Replay management list reads into lineage`.
- Paused after adding lineage replay for management list outbox events.
  `policy-binding.listed`, `project.listed`, `server.listed`,
  `storage-profile.listed`, and `warehouse.listed` now emit LakeCat
  OpenLineage receipts from durable outbox replay, while LakeCat avoids
  inventing list-specific graph nodes and leaves reusable hierarchy/traversal
  semantics to Grust.
- Local verification for the management-list replay slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_projects_management_list_reads_to_lineage`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `c028ac3 Replay table restores into graph`.
- Paused after adding catalog-facing graph replay for `table.restored` outbox
  events and refreshing the persistent goal guidance. Table restore replay now
  emits a Table graph event using the existing `Loaded` graph action plus the
  existing LakeCat OpenLineage restore receipt, leaving restore-specific graph
  taxonomy to Grust.
- Local verification for the table-restore replay slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_table_restores_to_graph_and_lineage`;
  `cargo test -p lakecat-lineage projects_table_restore_to_openlineage_output`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `69ce88c Replay catalog config reads into lineage`.
- Paused after adding `catalog.config-read` outbox replay. The standard Iceberg
  REST config entrypoint now emits warehouse-scoped catalog graph evidence and
  a LakeCat OpenLineage receipt from durable outbox replay, so config,
  namespace, and view reads all participate in replayable graph/lineage
  evidence without requiring non-standard client endpoints.
- Local verification for the catalog-config replay slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_catalog_config_reads_to_graph_and_lineage`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli -p lakecat-api -p lakecat-sail -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `ae3b796 Replay namespace reads into lineage`.
- Paused after adding `namespace.listed` and `namespace.loaded` outbox replay.
  Standard namespace reads now emit warehouse/namespace-scoped catalog graph
  events and LakeCat OpenLineage receipts, complementing namespace create/drop
  replay and the existing view read replay. The book now documents that
  namespace list/load reads participate in durable graph and lineage replay
  without leaving the Iceberg-compatible catalog surface.
- Local verification for the namespace-read replay slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_namespace_reads_to_graph_and_lineage`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli -p lakecat-api -p lakecat-sail -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `da2a590 Replay view listings into lineage`.
- Paused after adding `view.listed` outbox replay. Standard view listing reads
  now emit namespace-scoped catalog graph events and LakeCat OpenLineage
  receipts, while `view.upserted`, `view.loaded`, and `view.dropped` continue
  to project single-view graph and lineage evidence. The book now documents why
  list replay carries warehouse/namespace/view-count evidence without
  fabricating a single `view-stable-id`.
- Local verification for the view-list replay slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli -p lakecat-api -p lakecat-sail -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub "$expected_title"`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `27af9ac Replay view loads into lineage`.
- Paused after adding `view.loaded` outbox replay. Standard catalog view reads
  now emit catalog-facing View graph events and LakeCat OpenLineage receipts,
  alongside `view.upserted` and `view.dropped`, so view access through the
  Iceberg-compatible catalog surface has replayable graph/lineage evidence. The
  book now documents that view reads and management changes share the same
  durable replay proof shape.
- Local verification for the view-load replay slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli -p lakecat-api -p lakecat-sail -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub "$expected_title"`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `f6ca1e0 Require QGLake view replay evidence`.
- Paused after adding compact view replay identity to lineage-drain event
  summaries and tightening QGLake lineage-drain acceptance so every accepted
  QueryGraph bootstrap view artifact must have matching `view.upserted` or
  `view.dropped` replay evidence with graph and OpenLineage receipt hashes.
  The book now documents the view replay proof fields and how they connect
  QueryGraph bootstrap artifacts to durable outbox replay.
- Local verification for the view replay evidence slice was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo fmt -p lakecat-api -p lakecat-cli -p lakecat-service -p lakecat-sail -- --check`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub "$expected_title"`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `f052cd6 Replay view events and expand book workflows`.
- Paused after implementing the view outbox replay and book-workflow slice.
  `view.upserted` and `view.dropped` durable events now replay into
  catalog-facing View graph events and LakeCat OpenLineage receipts. The book
  is now explicitly part of the development workflow in `GOAL.md`, and
  `docs/book/lakecat.md` now includes substantial workflow examples from
  service startup and PySpark through credential vending, QueryGraph bootstrap,
  outbox draining, and agentic QGLake flows.
- Local verification for the view/book slice was green:
  `cargo fmt -p lakecat-graph -p lakecat-lineage -p lakecat-service`;
  `cargo test -p lakecat-graph view_event --features grust-local`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo fmt -p lakecat-graph -p lakecat-lineage -p lakecat-service -p lakecat-sail -p lakecat-api -- --check`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub "$expected_title"`;
  `pdftotext -f 1 -l 1 docs/book/dist/lakecat.pdf -`;
  `pdftotext -f 2 -l 2 docs/book/dist/lakecat.pdf -`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `f13f729 Project storage profiles into lineage`.
- Paused after implementing the storage-profile outbox lineage slice.
  `storage-profile.upserted` outbox replay now emits LakeCat
  lineage/OpenLineage receipts for credential-root management changes while
  leaving graph taxonomy and traversal work out of LakeCat and in Grust.
- Local verification for the storage-profile outbox lineage slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -- --check`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage`;
  `cargo test -p lakecat-lineage`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Push remains intentionally paused in this thread because `master` also
  contains the separate docs/book commit `5b7b8cf Version LakeCat book outputs`
  from the other task, and the user asked this task not to interfere with that
  work.
- Previous committed LakeCat implementation slice:
  `a5a130b Project server upserts into lineage`.
- Paused after implementing the server outbox lineage slice. `server.upserted`
  outbox replay now emits LakeCat lineage/OpenLineage receipts for durable
  server management writes while keeping reusable Server graph hierarchy work
  out of LakeCat and in Grust.
- Local verification for the server outbox lineage slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -- --check`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `cargo test -p lakecat-service outbox_drain_projects_server_upserts_to_lineage`;
  `cargo test -p lakecat-lineage`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `d4fc1f3 Project control-plane upserts into lineage`.
- Paused after implementing the control-plane outbox lineage slice.
  `policy-binding.upserted`, `project.upserted`, and `warehouse.upserted`
  outbox replay now emit LakeCat lineage/OpenLineage receipts alongside their
  Grust-facing graph anchors, so management/tenancy control-plane mutations
  carry replayable lineage evidence from the durable outbox.
- Local verification for the control-plane outbox lineage slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -- --check`;
  `cargo test -p lakecat-lineage projects_control_plane_upserts_to_openlineage_outputs`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-service outbox_drain_projects_warehouse_upserts_to_graph`;
  `cargo test -p lakecat-service outbox_drain_projects_project_upserts_to_graph`;
  `cargo test -p lakecat-lineage`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Previous committed LakeCat implementation slice:
  `cad81d2 Expose lineage drain authorization proof`.
- Paused after committing the lineage-drain authorization proof slice.
  `/management/v1/lineage/drain` now returns compact request-level
  lineage-read authorization evidence, the CLI prints that proof, and QGLake
  lineage-drain acceptance requires the drain request itself to carry principal,
  principal-kind, authorization-receipt hash, and request-identity state.
- Local verification for the lineage-drain authorization proof slice was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Previous pushed implementation slice:
  `be713f4 Require QGLake delete manifest evidence`.
- The QGLake fixture writes a position-delete manifest beside the data manifest,
  and governed `fetchScanTasks` acceptance requires Sail to attach delete-file
  refs to data tasks while treating delete-manifest child tasks as terminal
  governed delete-file work.
- Local verification for the pushed QGLake delete-manifest acceptance slice was
  green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `cargo test -p lakecat-cli qglake_delete_manifest_fetch_scan_tasks_verifier`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-cli`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test --workspace --all-features`;
  `cargo fmt -p lakecat-cli -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-service --all-features`.
- This earlier status commit records the pushed QGLake delete-manifest
  acceptance slice.
- Previous implementation slice:
  `ff08e77 Project credential replay into OpenLineage`.
- Paused after pushing the QGLake credential OpenLineage replay slice.
  `credentials.vend-attempted` outbox replay now emits LakeCat
  lineage/OpenLineage receipts, and QGLake lineage-drain acceptance requires
  both the restricted and trusted-human credential probes to carry lineage
  projection counts plus sink receipt hashes.
- Local verification for the pushed QGLake credential OpenLineage replay slice
  was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli -p lakecat-api`;
  `cargo test -p lakecat-lineage credential`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -p lakecat-cli -p lakecat-api -- --check`;
  `git diff --check`.
- This status commit records the pushed QGLake credential OpenLineage replay
  slice.
- Previous implementation slice:
  `69a43b9 Verify QGLake credential replay evidence`.
- Paused after pushing the QGLake credential replay-evidence slice.
  Lineage-drain event summaries now expose compact credential-vend evidence:
  credential count, block reason, and raw-credential-exception decision/reason.
  QGLake lineage-drain acceptance now rejects replay that omits either the
  restricted agent/anonymous credential block or the trusted-human audited raw
  credential exception.
- Local verification for the pushed QGLake credential replay-evidence slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- This status commit records the pushed QGLake credential replay-evidence slice.
- Previous implementation slice:
  `9166806 Prove QGLake human credential contrast`.
- Paused after pushing the QGLake human/agent credential contrast slice.
  `loadCredentials` now honors the audited raw-credential exception in the
  authorization receipt: explicit agent DID requests on fine-grained restricted
  tables receive no raw credentials and stay on governed Sail-planned reads,
  while trusted human principals can receive the same standard non-secret local
  credential response with audit evidence recording the exception.
- Local verification for the pushed QGLake human/agent credential contrast
  slice was green:
  `cargo fmt -p lakecat-cli -p lakecat-service`;
  `cargo test -p lakecat-service credential_vend`;
  `cargo test -p lakecat-cli qglake_credentials`;
  `cargo test -p lakecat-cli qglake_trusted_human_credentials_verifier_requires_standard_local_credentials`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `cargo fmt -p lakecat-cli -p lakecat-service -- --check`;
  `git diff --check`.
- This status commit records the pushed QGLake human/agent credential contrast
  slice.
- Previous implementation slice:
  `f1b415a Mirror QueryGraph import hash in OpenLineage`.
- Paused after pushing the QueryGraph import-hash OpenLineage projection slice.
  `querygraph.bootstrap` now projects bundle, graph, OpenLineage, and
  QueryGraph import-compatibility hashes as explicit OpenLineage bootstrap facet
  fields, while service replay tests pin the durable lineage payload so the
  import hash cannot be dropped before it reaches lineage sinks.
- Local verification for the pushed QueryGraph import-hash OpenLineage
  projection slice was green:
  `cargo fmt -p lakecat-lineage -p lakecat-service`;
  `cargo test -p lakecat-lineage projects_querygraph_bootstrap_to_openlineage_output`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `cargo fmt -p lakecat-lineage -p lakecat-service -- --check`;
  `git diff --check`.
- This status commit records the pushed QueryGraph import-hash OpenLineage
  projection slice.
- Previous implementation slice:
  `f699f68 Replay QueryGraph import hash evidence`.
- Paused after pushing the QueryGraph import-hash replay slice.
  `querygraph.bootstrap` audit/outbox payloads now persist the accepted
  QueryGraph import hash, lineage-drain summaries expose it, and QGLake
  lineage-drain acceptance rejects replay evidence that drops or changes the
  import hash relative to the accepted bootstrap contract.
- Local verification for the pushed QueryGraph import-hash replay slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed QueryGraph import-hash replay slice.
- Earlier implementation slice:
  `47d1666 Expose QueryGraph import compatibility hash`.
- Paused after pushing the QueryGraph import-compatibility slice. Bootstrap
  manifests now carry a `querygraph-import` contract with a table-only bundle
  hash matching the current QueryGraph Rust importer hash domain, the manifest
  verifier recomputes that evidence, `/querygraph/v1/bootstrap` exposes it, and
  QGLake rejects bootstrap bundles that drop the import contract.
- Local verification for the pushed QueryGraph import-compatibility slice was
  green:
  `cargo fmt -p lakecat-querygraph -p lakecat-cli -p lakecat-service`;
  `cargo test -p lakecat-querygraph`;
  `cargo test -p lakecat-cli qglake_bootstrap`;
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog_tables`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service`;
  `cargo fmt -p lakecat-querygraph -p lakecat-cli -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed QueryGraph import-compatibility slice.
- Previous implementation slice:
  `88955ec Bind QGLake replay agent summary proofs`.
- Paused after pushing the QGLake replay agent-summary proof slice. The QGLake
  agent-DID fixture mode now sends deterministic local delegation and
  agent-summary proof headers, lineage-drain replay summaries expose their
  sanitized hashes, and QGLake rejects explicit agent replay evidence that
  drops either hash.
- Local verification for the pushed QGLake replay agent-summary proof slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-cli parses_qglake_fixture_command_defaults`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed QGLake replay agent-summary proof
  slice.
- Previous implementation slice:
  `9f7ec88 Bind QGLake replay principal kind`.
- Paused after pushing the QGLake replay principal-kind slice. The CLI now uses
  agent-DID request headers for explicit `qglake-fixture` principals while
  leaving normal admin commands on `x-lakecat-principal`; lineage-drain replay
  summaries now expose `principal-kind`, and QGLake rejects bootstrap replay
  evidence whose principal kind does not match the accepted agent/anonymous
  actor.
- Local verification for the pushed QGLake replay principal-kind slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-cli parses_qglake_fixture_command_defaults`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake replay principal-kind slice.
- Previous implementation slice:
  `d7b6da6 Bind QGLake replay identity state`.
- Paused after pushing the QGLake replay request-identity-state slice. The
  lineage-drain event summary now exposes `request-identity-state` for replayed
  bootstrap events, service tests pin the state in direct and HTTP drain
  responses, and QGLake rejects `querygraph.bootstrap` replay evidence that
  drops the request identity attestation state.
- Local verification for the pushed QGLake replay request-identity-state slice
  was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli qglake`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake replay request-identity-state
  slice.
- Previous implementation slice:
  `79d7f26 Bind QGLake replay authorization proof`.
- Paused after pushing the QGLake replay authorization-proof slice. The
  lineage-drain event summary now exposes a compact
  `authorization-receipt-hash` for replayed bootstrap events, service tests pin
  the hash in direct and HTTP drain responses, and QGLake rejects
  `querygraph.bootstrap` replay evidence that lacks an authorization receipt
  proof.
- Local verification for the pushed QGLake replay authorization-proof slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli qglake`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake replay authorization-proof
  slice.
- Previous implementation slice:
  `77d72cc Bind QGLake replay policy count`.
- Paused after pushing the QGLake replay policy-binding count slice. The
  lineage-drain event summary now exposes the replayed bootstrap
  policy-binding count, service tests pin it in direct and HTTP drain
  responses, and QGLake rejects replay evidence whose policy-binding count does
  not match the accepted bootstrap bundle.
- Local verification for the pushed QGLake replay policy-binding count slice
  was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli qglake`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake replay policy-binding count
  slice.
- Previous implementation slice:
  `0dbefd7 Bind QGLake replay principal`.
- Paused after pushing the QGLake replay-principal slice. The lineage-drain
  event summary now exposes the replayed bootstrap authorization principal, the
  service tests pin that principal in direct and HTTP drain responses, and the
  QGLake lineage-drain verifier rejects replay evidence whose principal does not
  match the principal used for the accepted handoff.
- Local verification for the pushed QGLake replay-principal slice was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli qglake`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake replay-principal slice.
- Previous implementation slice:
  `14326c3 Bind QGLake replay standards`.
- Paused after pushing the QGLake replay-standards slice. The lineage-drain
  event summary now exposes QueryGraph bootstrap standards, the service tests
  pin those replayed standards in direct and HTTP drain responses, and the
  QGLake lineage-drain verifier rejects replay evidence whose standards do not
  match the accepted bootstrap bundle.
- Local verification for the pushed QGLake replay-standards slice was green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli qglake`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake replay-standards slice.
- Previous implementation slice:
  `605696e Expose QGLake projection replay counts`.
- Paused after pushing the QGLake projection replay-count slice. The
  lineage-drain event summary now exposes per-event graph and lineage
  projection counts, the service tests pin those counts for
  `querygraph.bootstrap`, and the QGLake lineage-drain verifier now rejects
  drains that replay no graph projections or whose bootstrap replay emits no
  lineage projection.
- Local verification for the pushed QGLake projection replay-count slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli qglake`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake projection replay-count slice.
- Previous implementation slice:
  `de7c393 Bind QGLake lineage replay to bundle`.
- Paused after pushing the QGLake lineage replay/bundle binding slice. The
  `lakecat-cli qglake-fixture` lineage-drain verifier now compares the replayed
  `querygraph.bootstrap` evidence against the exact QueryGraph bundle QGLake
  accepted and wrote, rejecting drifted bundle, graph, OpenLineage, table-count,
  or view-count evidence.
- Local verification for the pushed QGLake lineage replay/bundle binding slice
  was green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake lineage replay/bundle binding
  slice.
- Previous implementation slice:
  `94847d8 Expose QGLake lineage replay evidence`.
- Paused after pushing the QGLake lineage replay evidence slice. The management
  lineage-drain response now exposes compact per-event replay evidence for
  QueryGraph bootstrap events: bundle, graph, OpenLineage, table/view artifact
  counts, and sink receipt hashes. The QGLake lineage-drain verifier now
  rejects bootstrap replay that lacks QueryGraph hashes, table artifact
  evidence, or OpenLineage-facing sink receipt hashes.
- Local verification for the pushed QGLake lineage replay evidence slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli qglake`;
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake lineage replay evidence slice.
- Previous implementation slice:
  `d5ec6d4 Persist QueryGraph artifact hashes in outbox`.
- Paused after pushing the QueryGraph bootstrap outbox artifact-hash proof
  slice. The `querygraph.bootstrap` audit/outbox payload now persists the
  QueryGraph manifest table/view artifact hashes, and lineage-drain replay tests
  prove those hashes survive into the replayed OpenLineage-facing event payload.
- Local verification for the pushed outbox artifact-hash proof slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-service`;
  `cargo fmt -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QueryGraph bootstrap outbox
  artifact-hash proof slice.
- Previous implementation slice:
  `bd14b08 Verify bootstrap OpenLineage hashes at service boundary`.
- Paused after pushing the QueryGraph bootstrap service-boundary hash proof
  slice. The `/querygraph/v1/bootstrap` service tests now verify that the
  OpenLineage `queryGraph_semanticBundle` graph, table, and view artifact hashes
  exposed by the API match the QueryGraph manifest hashes returned in the same
  bundle.
- Local verification for the pushed service-boundary hash proof slice was
  green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog`;
  `cargo test -p lakecat-service`;
  `cargo fmt -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QueryGraph bootstrap service-boundary
  hash proof slice.
- Previous implementation slice:
  `a4c4975 Verify all QGLake OpenLineage artifacts`.
- Paused after pushing the all-artifact OpenLineage proof slice. The
  `lakecat-cli qglake-fixture` bootstrap verifier now checks every table and
  view artifact listed in the QueryGraph manifest against the OpenLineage
  semantic-bundle hash evidence, so a bundle cannot pass by matching only the
  selected fixture table while carrying drifted evidence for another artifact.
- Local verification for the pushed all-artifact OpenLineage proof slice was
  green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_bootstrap`;
  `cargo test -p lakecat-cli qglake`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed all-artifact OpenLineage proof slice.
- Previous implementation slice:
  `570b973 Mirror QueryGraph artifact hashes in OpenLineage`.
- Paused after pushing the QueryGraph OpenLineage artifact-hash proof slice. The
  QueryGraph bootstrap OpenLineage `queryGraph_semanticBundle` facet now carries
  the manifest's graph hash plus table/view artifact hashes, and the
  `lakecat-cli qglake-fixture` bootstrap verifier rejects bundles whose
  OpenLineage hash evidence diverges from the manifest before accepting the
  handoff.
- Local verification for the pushed QueryGraph OpenLineage artifact-hash proof
  slice was green:
  `cargo fmt -p lakecat-querygraph -p lakecat-cli`;
  `cargo test -p lakecat-querygraph`;
  `cargo test -p lakecat-cli qglake_bootstrap`;
  `cargo test -p lakecat-cli qglake`;
  `cargo fmt -p lakecat-cli -p lakecat-querygraph -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QueryGraph OpenLineage artifact-hash
  proof slice.
- Previous implementation slice:
  `107ea6c Verify QGLake bootstrap manifest hashes`.
- Paused after pushing the QGLake bootstrap manifest-hash proof slice. The
  `lakecat-cli qglake-fixture` bootstrap verifier now runs the QueryGraph bundle
  manifest verifier before accepting the handoff, rejecting tampered Croissant,
  CDIF, OSI handoff, ODRL, graph, OpenLineage, policy-binding, or bundle-hash
  content after the QGLake-specific semantic checks pass.
- Local verification for the pushed QGLake bootstrap manifest-hash proof slice
  was green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_bootstrap`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-querygraph`;
  `cargo fmt -p lakecat-cli -p lakecat-querygraph -- --check`;
  `git diff --check`;
  `cargo test --workspace`.
- This status commit records the pushed QGLake bootstrap manifest-hash proof
  slice.
- Previous implementation slice:
  `5147cfb Verify QGLake OpenLineage job output`.
- Paused after pushing the QGLake OpenLineage job/output proof slice. The
  `lakecat-cli qglake-fixture` bootstrap verifier now rejects QueryGraph
  bundles whose OpenLineage event is not COMPLETE, whose job identity is not
  the LakeCat QueryGraph bootstrap job, or whose output data-source URI does
  not match the exported table location.
- Local verification for the pushed QGLake OpenLineage job/output proof slice
  was green:
  `cargo fmt -p lakecat-cli -p lakecat-querygraph -- --check`;
  `cargo test -p lakecat-cli qglake_bootstrap`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-querygraph`;
  `git diff --check`.
- This status commit records the pushed QGLake OpenLineage job/output proof
  slice.
- Previous implementation slice:
  `5dc9884 Verify QGLake OpenLineage envelope`.
- Paused after pushing the QGLake OpenLineage envelope proof slice. The
  `lakecat-cli qglake-fixture` bootstrap verifier now rejects QueryGraph
  bundles whose OpenLineage output is not produced by LakeCat, does not use the
  expected OpenLineage schema URL, or whose semantic-bundle table/view counts do
  not match the exported bundle.
- Local verification for the pushed QGLake OpenLineage envelope proof slice was
  green:
  `cargo fmt -p lakecat-cli -p lakecat-querygraph -- --check`;
  `cargo test -p lakecat-cli qglake_bootstrap`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-querygraph`;
  `git diff --check`.
- This status commit records the pushed QGLake OpenLineage envelope proof slice.
- Previous implementation slice:
  `b08a2e7 Mirror QueryGraph standards in OpenLineage`.
- Paused after pushing the QueryGraph OpenLineage standards facet slice. The
  QueryGraph bootstrap OpenLineage `queryGraph_semanticBundle` facet now carries
  the same Iceberg REST, Croissant, CDIF, OSI handoff, ODRL, Grust catalog
  graph, and OpenLineage standards as the manifest, and the QGLake bootstrap
  verifier rejects bundles whose OpenLineage facet omits any required standard.
- Local verification for the pushed QueryGraph OpenLineage standards facet
  slice was green:
  `cargo fmt -p lakecat-cli -p lakecat-querygraph -- --check`;
  `cargo test -p lakecat-cli qglake_bootstrap`;
  `cargo test -p lakecat-querygraph projects_iceberg_table_into_querygraph_bundle`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-querygraph`;
  `git diff --check`.
- This status commit records the pushed QueryGraph OpenLineage standards facet
  slice.
- Previous implementation slice:
  `12a6d18 Require QGLake bootstrap standards`.
- Paused after pushing the QGLake bootstrap-standards proof slice. The
  `lakecat-cli qglake-fixture` bootstrap verifier now rejects QueryGraph
  bundles whose manifest does not advertise the expected Iceberg REST,
  Croissant, CDIF, OSI handoff, ODRL, Grust catalog graph, and OpenLineage
  standards, so QGLake acceptance proves the exported bundle carries the full
  QueryGraph handoff surface.
- Local verification for the pushed QGLake bootstrap-standards proof slice was
  green:
  `cargo fmt -p lakecat-cli -p lakecat-querygraph -- --check`;
  `cargo test -p lakecat-cli qglake_bootstrap`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-querygraph projects_iceberg_table_into_querygraph_bundle`;
  `git diff --check`.
- This status commit records the pushed QGLake bootstrap-standards proof slice.
- Previous implementation slice:
  `dadf1ad Verify all QGLake manifest children`.
- Paused after pushing the QGLake all-manifest-children proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now follows
  every child manifest plan-task token returned by manifest-list expansion and
  requires each terminal manifest fetch to remain governed and table-local,
  instead of treating the first child token as representative.
- Local verification for the pushed QGLake all-manifest-children proof slice
  was green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`;
  `git diff --check`.
- This status commit records the pushed QGLake all-manifest-children proof
  slice.
- Previous implementation slice:
  `004a27f Verify QGLake leaf manifest fetch`.
- Paused after pushing the QGLake leaf-manifest fetch proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now follows
  the child manifest plan-task token returned by manifest-list expansion and
  requires the terminal manifest fetch to produce governed data-file scan work
  under the table location without emitting further child plan tasks.
- Local verification for the pushed QGLake leaf-manifest fetch proof slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `cargo test -p lakecat-cli qglake_leaf_fetch_scan_tasks_verifier`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`;
  `git diff --check`.
- This status commit records the pushed QGLake leaf-manifest fetch proof slice.
- Previous implementation slice:
  `09b6b06 Require QGLake child plan tasks`.
- Paused after pushing the QGLake child-plan-task proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now rejects
  manifest-list expansions that do not return both a child Iceberg REST
  plan-task token and a LakeCat manifest child task, keeping acceptance on the
  standard multi-step planning path rather than accepting a terminal file list
  without follow-on planning proof.
- Local verification for the pushed QGLake child-plan-task proof slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`;
  `git diff --check`.
- This status commit records the pushed QGLake child-plan-task proof slice.
- Previous implementation slice:
  `feec688 Require QGLake manifest-backed plan`.
- Paused after pushing the QGLake manifest-backed plan proof slice. The
  `lakecat-cli qglake-fixture` governed scan-plan verifier now rejects plans
  that lack both an Iceberg REST plan-task token and a LakeCat manifest-list
  plan task, proving QGLake acceptance starts from manifest-backed Sail planning
  before `fetchScanTasks` expansion.
- Local verification for the pushed QGLake manifest-backed plan proof slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_scan_plan_verifier`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`;
  `git diff --check`.
- This status commit records the pushed QGLake manifest-backed plan proof
  slice.
- Previous implementation slice:
  `db252d3 Require QGLake Sail planner identity`.
- Paused after pushing the QGLake Sail planner-identity proof slice. The
  `lakecat-cli qglake-fixture` governed scan-plan and `fetchScanTasks`
  verifiers now reject responses whose `planned_by` value is not
  `sail-rest-models`, proving the acceptance path is the Sail REST-model
  planner/fetch expansion rather than a non-Sail compatible response.
- Local verification for the pushed QGLake Sail planner-identity proof slice
  was green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_scan_plan_verifier`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`.
- This status commit records the pushed QGLake Sail planner-identity proof
  slice.
- Previous implementation slice:
  `7df6e99 Require QGLake fetched column proof`.
- Paused after pushing the QGLake fetched-column proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now rejects
  fetched residual read restrictions that widen the allowed-column set, proving
  `raw_payload` stays excluded during task materialization as well as initial
  scan planning.
- Local verification for the pushed QGLake fetched-column proof slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`.
- This status commit records the pushed QGLake fetched-column proof slice.
- Previous implementation slice:
  `44d0265 Constrain QGLake fetched files to table location`.
- Paused after pushing the QGLake fetched-file table-location proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now carries
  the fixture table location through the live plan/fetch check and rejects any
  fetched Iceberg data-file path outside that table location, catching
  escaped-path or wrong-table scan work.
- Local verification for the pushed QGLake fetched-file table-location proof
  slice was green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`.
- This status commit records the pushed QGLake fetched-file table-location
  proof slice.
- Previous implementation slice:
  `d9c8ac7 Require QGLake fetched data files`.
- Paused after pushing the QGLake fetched-data-file proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now rejects
  placeholder scan-task JSON and requires at least one fetched file scan task to
  carry an Iceberg REST `data-file.file-path`, proving acceptance sees actual
  data-file work from Sail manifest expansion.
- Local verification for the pushed QGLake fetched-data-file proof slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`.
- This status commit records the pushed QGLake fetched-data-file proof slice.
- Previous implementation slice:
  `e02045b Require QGLake fetched scan work`.
- Paused after pushing the QGLake fetched-scan-work proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now rejects
  responses that carry only the residual policy proof but no fetched file scan
  tasks, proving the plan-task token was expanded into real Sail-backed scan
  work.
- Local verification for the pushed QGLake fetched-scan-work proof slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`.
- This status commit records the pushed QGLake fetched-scan-work proof slice.
- Previous implementation slice:
  `d5b496a Preflight QGLake manifest lists`.
- Paused after pushing the QGLake manifest-list preflight slice. QGLake fixture
  reruns now reject existing fixture tables when snapshot manifest-list files
  referenced by the table metadata are missing locally, failing before live
  governed plan/fetch verification reaches Sail's manifest expansion.
- Local verification for the pushed QGLake manifest-list preflight slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_existing_table_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`.
- This status commit records the pushed QGLake manifest-list preflight slice.
- Previous implementation slice:
  `d38d1f2 Validate QGLake metadata pointer reruns`.
- Paused after pushing the QGLake metadata-pointer rerun validation slice.
  `lakecat-cli qglake-fixture` now rejects existing fixture tables when the
  advertised local `metadata_location` file is missing, invalid, or drifted
  from the Iceberg metadata returned by the catalog, so reruns cannot silently
  accept a non-openable metadata pointer.
- Local verification for the pushed QGLake metadata-pointer rerun validation
  slice was green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_existing_table_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`.
- This status commit records the pushed QGLake metadata-pointer rerun
  validation slice.
- Previous implementation slice:
  `e201570 Write QGLake metadata pointer file`.
- Paused after pushing the QGLake metadata-pointer file slice. The local
  QGLake fixture now writes the Iceberg table metadata JSON at the advertised
  `metadata_location` in addition to the manifest list and data manifest, so
  standard metadata-pointer consumers can open the bootstrap table metadata
  file directly.
- Local verification for the pushed QGLake metadata-pointer file slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fixture_metadata_contains_restricted_raw_payload_column`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`.
- This status commit records the pushed QGLake metadata-pointer file slice.
- Previous implementation slice:
  `97b6e60 Make QGLake fixture fetchable`.
- Paused after pushing the QGLake fetchable fixture slice. `lakecat-cli
  qglake-fixture` now creates local Iceberg manifest-list and data-manifest
  files with Sail's Iceberg writer types, records a current snapshot in the
  bootstrap table metadata, rejects existing QGLake tables that cannot support
  governed scan planning and `fetchScanTasks`, and keeps OPUS2-DESIGN aligned
  with the now-built ODRL restriction/plan/fetch proof.
- Local verification for the pushed QGLake fetchable fixture slice was green:
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`;
  `cargo test -p lakecat-cli`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fixture_metadata_contains_restricted_raw_payload_column`;
  `git diff --check`.
- This status commit records the pushed QGLake fetchable fixture slice.
- Previous implementation slice:
  `d0dc194 Stamp governed receipts with policy hashes`.
- Paused after pushing governed receipt policy-hash stamping. Governed scan and
  credential-vend authorization receipts now get a deterministic top-level
  `policy_hash` derived from enforced `ReadRestriction` policy hashes while
  preserving any governance-engine hash as an input. The service authorization
  boundary and in-process Sail provider scan path both use the shared
  `lakecat-security` receipt helper.
- Local verification for the pushed governed receipt policy-hash slice was
  green:
  `cargo fmt -p lakecat-security -p lakecat-service -p lakecat-sail -- --check`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-sail --features catalog-provider`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`.
- This status commit records the pushed governed receipt policy-hash slice.
- Previous implementation slice:
  `5a20750 Expose governed fetch restriction proof`.
- Paused after pushing the governed fetch restriction proof. Iceberg REST
  `fetchScanTasks` responses now surface a `lakecat:fetch-scan-tasks`
  extension carrying the re-applied `ReadRestriction`, and the QGLake verifier
  now requires a governed plan-task token whose fetch response carries the same
  policy hash proof as the scan plan.
- Local verification for the pushed governed fetch restriction proof slice was
  green:
  `cargo fmt -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`;
  `cargo test -p lakecat-service --features sail-local,turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`.
- This status commit records the pushed governed fetch restriction proof slice.
- Previous implementation slice:
  `b4b5116 Add Sail provider namespace drop`.
- Paused after pushing the Sail provider namespace-drop bridge. The in-process
  Sail `CatalogProvider` now routes `drop_database` through LakeCat's governed
  durable namespace deletion with typed `namespace.drop` capability validation,
  `if_exists` handling, and explicit rejection of unsupported cascading drops.
  LakeCat's store remains the enforcement point for non-empty namespace guards.
- Local verification for the pushed Sail provider namespace-drop slice was
  green:
  `cargo fmt -p lakecat-sail -- --check`;
  `cargo test -p lakecat-sail --features catalog-provider provider_drops_durable_namespaces`;
  `cargo test -p lakecat-sail --features catalog-provider`;
  `git diff --check`.
- This status commit records the pushed Sail provider namespace-drop slice.
- Previous implementation slice:
  `28c044e Require QGLake scan policy hash proof`.
- Paused after pushing the QGLake scan policy-hash proof. The QGLake governed
  scan verifier now requires the enforced `ReadRestriction` to carry the
  expected hash of the bootstrapped ODRL policy document, proving the accepted
  scan is bound to the policy that defined the allowed columns and row
  predicate rather than merely carrying a lookalike restriction.
- Local verification for the pushed QGLake scan policy-hash proof was green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake`;
  `git diff --check`;
  `cargo test -p lakecat-service scan_planning_applies_policy_column_restriction_before_sail --features sail-local,turso-local`.
- This status commit records the pushed QGLake scan policy-hash proof slice.
- Previous implementation slice:
  `cbda084 Add Sail provider view column bridge`.
- Paused after pushing the Sail provider view-column bridge. `ViewRecord` now
  persists typed view columns in memory and Turso, the management/catalog view
  APIs accept and return those columns, QueryGraph bootstrap exports them in
  view projections and OSI handoff, and the in-process Sail `CatalogProvider`
  can create, load, list, and drop durable LakeCat views as `TableKind::View`
  statuses. Full Iceberg view version/commit semantics remain pending.
- Local verification for the pushed Sail provider view-column bridge slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -p lakecat-querygraph -p lakecat-sail -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-sail --features catalog-provider provider_manages_durable_views_with_typed_columns -- --nocapture`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-querygraph projects_catalog_views_into_querygraph_bundle`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_view_records`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-querygraph`;
  `cargo test -p lakecat-sail --features catalog-provider`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed Sail provider view-column bridge slice.
- Previous implementation slice:
  `b73f503 Add governed namespace load and drop`.
- Paused after pushing governed namespace load/drop. Unprefixed and
  warehouse-prefixed Iceberg REST catalog paths can now load and delete durable
  namespace state through typed `namespace.load` and `namespace.drop`
  capabilities. Namespace drops are blocked while tables, views, or scoped
  policy bindings remain, and audited `namespace.dropped` events replay into
  graph/lineage projection as deleted namespace events.
- Local verification for the pushed governed namespace load/drop slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -p lakecat-lineage -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-security table_capabilities_require_matching_allowed_receipts`;
  `cargo test -p lakecat-store memory_store_loads_and_drops_namespaces`;
  `cargo test -p lakecat-store --features turso-local turso_store_loads_and_drops_namespaces`;
  `cargo test -p lakecat-service namespaces_load_and_drop_through_catalog_routes -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `cargo test -p lakecat-lineage`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed governed namespace load/drop slice.
- Previous implementation slice:
  `5d53aea Add governed durable view drop`.
- Paused after pushing governed durable view deletion. Management and
  warehouse-prefixed catalog REST paths can now delete durable `ViewRecord`
  values from memory and Turso stores through a typed `view.drop` capability,
  emitting audited `view.dropped` events while preserving Iceberg table access
  semantics.
- Local verification for the pushed governed view drop slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service`;
  `cargo test -p lakecat-security table_capabilities_require_matching_allowed_receipts`;
  `cargo test -p lakecat-store memory_tests::memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store::tests::turso_store_persists_view_records`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed governed view drop slice.
- Previous implementation slice:
  `d31ace5 Exercise production secret-ref gating`.
- Paused after pushing production secret-ref gate coverage. The TypeSec-backed
  credential issuer now has service-level coverage proving `vault://`,
  `aws-sm://`, `gcp-sm://`, and `azure-kv://` refs authorize the exact secret
  URI before failing closed with provider-specific not-configured errors when no
  resolver backend is wired.
- Local verification for the pushed production secret-ref gating slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_production_secret_refs_before_dispatch -- --nocapture`;
  `cargo fmt -p lakecat-service -p lakecat-api -p lakecat-security -p lakecat-store -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service --features typesec-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed production secret-ref gating slice.
- Previous implementation slice:
  `efd548a Add catalog view REST aliases`.
- Paused after pushing catalog-path view REST aliases. Warehouse-prefixed catalog
  callers can now list, load, and upsert durable `ViewRecord` values through
  `/catalog/v1/{warehouse}/namespaces/{namespace}/views`, with governed
  `view.load` authorization for reads and audited Iceberg REST `view.*` events.
- Local verification for the pushed catalog view REST alias slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-security table_capabilities_require_matching_allowed_receipts`;
  `cargo test -p lakecat-store memory_tests::memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store::tests::turso_store_persists_view_records`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed catalog view REST alias slice.
- Previous implementation slice:
  `2680f95 Add project-scoped warehouse management`.
- Paused after pushing project-scoped warehouse management routes. Management
  callers can now list and upsert warehouses through
  `/management/v1/projects/{project}/warehouses`, using the same durable
  `WarehouseRecord` state and validation while leaving standard Iceberg table
  routes unchanged.
- Local verification for the pushed project-scoped warehouse management slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_warehouse_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_warehouse_records`;
  `cargo test -p lakecat-service management_warehouses_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed project-scoped warehouse management
  slice.
- Previous implementation slice:
  `e200664 Validate warehouse project attachments`.
- Paused after pushing warehouse-to-project validation. Durable warehouse upserts
  in memory and Turso now reject references to missing projects, and the service
  management/prefixed-route tests create the durable project before registering
  warehouses.
- Local verification for the pushed warehouse-project validation slice was green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_warehouse_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_warehouse_records`;
  `cargo test -p lakecat-service management_warehouses_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed warehouse-project validation slice.
- Previous implementation slice:
  `9a27369 Attach projects to durable servers`.
- Paused after pushing optional `server-id` attachment for durable project
  records. Project management responses now expose the Server > Project link,
  and memory/Turso stores reject project attachments to missing servers while
  preserving existing projects that do not declare a server.
- Local verification for the pushed project-server attachment slice was green:
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_project_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_project_records`;
  `cargo test -p lakecat-service management_projects_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed project-server attachment slice.
- Previous implementation slice:
  `7bc33ab Add governed durable server records`.
- Paused after pushing governed durable `ServerRecord` support with management
  list/upsert endpoints, memory/Turso persistence, `server.manage`
  authorization, audited `server.*` events, and docs updated to reflect the
  Server > Project > Warehouse control-plane hierarchy now starting in code.
- Local verification for the pushed governed-server slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_server_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_server_records`;
  `cargo test -p lakecat-service management_servers_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed governed-server slice.
- Previous implementation slice:
  `a6669e5 Export views in QueryGraph bootstrap`.
- Local verification for the previous QueryGraph view-bootstrap slice was green:
  `cargo fmt -p lakecat-cli -p lakecat-querygraph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-querygraph projects_catalog_views_into_querygraph_bundle -- --nocapture`;
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog_views -- --nocapture`;
  `cargo test -p lakecat-cli qglake_bootstrap -- --nocapture`;
  `cargo test -p lakecat-querygraph`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Previous implementation slice:
  `1c3cfb8 Add governed durable view records`.
- Local verification for the previous governed-view slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_view_records`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Prior implementation slice:
  `d905f27 Route commit metadata through object_store`.
- Local verification for the previous object-store metadata writer slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo fmt -p lakecat-service -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service commit_can_advance_metadata_location_extension -- --nocapture`;
  `cargo test -p lakecat-service --all-features stale_commit_cleans_up_uncommitted_metadata_file -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Prior implementation slice:
  `ce4b82b Verify QGLake credential blocking`.
- Local verification for the previous QGLake credential-blocking slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service credential_vend_blocks_raw_credentials_for_fine_grained_restriction -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Prior implementation slice:
  `109e0dd Block credential bypass for restricted reads`.
- Local verification for the previous credential-bypass hardening slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service credential_vend_blocks_raw_credentials_for_fine_grained_restriction -- --nocapture`;
  `cargo test -p lakecat-service credentials_vend_audit_payload_surfaces_policy_context`;
  `cargo test -p lakecat-security read_restriction`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Local verification for the previous registered-prefix slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_warehouse_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_warehouse_records`;
  `cargo test -p lakecat-service prefixed_catalog_routes_target_requested_warehouse -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- The all-feature gates again required local syntax repairs in the dirty sibling
  `/Users/alexy/src/grust/crates/grust-cypher/src/lib.rs` checkout around
  return-projection helper edits. LakeCat did not stage the sibling Grust repo.
- Local verification for the previous management-routing slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service management_warehouses_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Local verification for the previous project-management slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service`;
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_project_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_project_records`;
  `cargo test -p lakecat-service management_projects_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_project_upserts_to_graph -- --nocapture`;
  `cargo test -p lakecat-graph --features grust-local converts_project_event_to_valid_grust_graph_event`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-graph`;
  `cargo test -p lakecat-graph --features grust-local`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- The `grust-local` gates required local syntax repairs in the dirty sibling
  `/Users/alexy/src/grust/crates/grust-cypher/src/lib.rs` checkout around the
  current return-projection helper edits. LakeCat did not stage the sibling Grust
  repo.
- The all-feature service/workspace gates required another one-line local Grust
  fix in `/Users/alexy/src/grust/crates/grust-cypher/src/lib.rs`: a stale
  recursive `evaluate_scalar_return_projection` call still passed
  `&split.target` after the current dirty Grust checkout changed the callee
  signature. LakeCat did not stage the dirty sibling Grust repo.
- The all-feature service/workspace gates required a one-token local Grust fix
  in `/Users/alexy/src/grust/crates/grust-cypher/src/lib.rs`: an accidental
  duplicated `target` argument inside a `matches!` macro in the current dirty
  Grust checkout. The Grust repo had broad pre-existing uncommitted edits, so
  LakeCat did not stage that sibling repo.
- Manual cloud gate status: run `27722995692` was started only after local
  workflow reproduction. It completed with all focused rows green, including
  default workspace, `sail-local service`, `typesec-local service`,
  `typesec-local security`, `grust-local graph`, `grust cypher boundary`, and
  `turso-local store`. The only failing row was `all features workspace`, where
  `lakecat-sail::sail_integration::tests::preserves_filter_context_and_prunes_loaded_file_bounds`
  panicked on an unwrap in the cloud checkout at `22827ee`; the same focused
  command passes locally on the current worktree. Automatic push/PR CI remains
  disabled.
- LakeCat carries a temporary CI bridge under
  `ci/sail-patches/` that applies local Sail helper commits `68631016` and
  `fdb3b657`, plus the generated-model module export LakeCat service tests use,
  to the `lakehq/sail@main` checkout before building. The bridge now supplies
  a local `git am` committer identity and passes absolute patch paths so
  `git -C sail` can apply them; it should be removed once those APIs are
  available from an upstream Sail branch.
- Status commit recording the pushed Grust Cypher reconciliation:
  `e6ca9e0 Record Grust Cypher reconciliation status`.
- Status commits recording the pushed verifier work:
  `f68cc05 Record TypeDID verifier status` and
  `d720dc4 Record pushed TypeDID verifier status`.
- Supporting TypeSec commits are pushed through
  `e05460f Prepare Typesec 0.8.0 release`.
- Current local dependency reconciliation is complete: LakeCat now targets
  `typesec` 0.8.0 and Grust 0.9.0 with the `cypher` facade feature enabled for
  `grust-local`.
- Supporting Sail helper commit exists locally in `/Users/alexy/src/sail` as
  `68631016 Expose Iceberg table status conversion`. Pushing to
  `lakehq/sail` is blocked for this machine/account: HTTPS has no configured
  credential prompt, and SSH reports permission denied for `alexy`.
- Additional supporting Sail helper commit exists locally as
  `fdb3b657 Expose Iceberg planning result helpers`; it has the same upstream
  push blocker until Sail repository credentials/permissions are resolved.
- Cloud CI remains manual-only. The local dependency chain is green against
  Grust 0.9.0 (`grust-cypher` included) and TypeSec 0.8.0; automatic GitHub
  Actions should stay disabled until the same graph is green in the cloud.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In Latest Implementation Slice

- Added project-scoped warehouse management routes for listing and upserting
  warehouses under durable projects without changing standard Iceberg table
  routes.
- Enforced warehouse-to-project attachment in memory and Turso stores, with
  governed warehouse management rejecting warehouses that point at missing
  projects.
- Added optional `server-id` attachment to durable project records, surfaced it
  through management API responses, and made memory/Turso project upserts reject
  attachments to missing servers.
- Added governed durable server records with management list/upsert endpoints,
  memory/Turso persistence, and audited `server.*` events as the root of the
  Server > Project > Warehouse control-plane hierarchy.
- Added stored view projections to QueryGraph bootstrap bundles, including
  manifest view artifact hashes, view-aware graph edges, OpenLineage view counts,
  service-level export, and verification coverage.
- Added governed durable view records with management list/upsert endpoints,
  memory/Turso persistence, and audited `view.*` events as the next
  Lakekeeper-style catalog entity after Project and Warehouse.
- Routed commit metadata object writes and stale-write cleanup through
  `object_store::parse_url_opts`, preserving local `file://` behavior while
  opening the writer boundary for configured object-store backends.
- Extended `lakecat-cli qglake-fixture` to prove the restricted QGLake table does
  not receive raw credentials, keeping the acceptance path on governed
  Sail-planned reads.
- Blocked raw credential vending for authorization receipts that carry
  fine-grained row or column read restrictions, and audited the blocked attempt
  with the same policy context.
- Required warehouse-prefixed Iceberg REST catalog routes to resolve a stored
  warehouse record before catalog state changes or loads.
- Added warehouse-prefixed Iceberg REST catalog routes while preserving
  unprefixed default-warehouse routes for existing clients.
- Allowed governed warehouse management endpoints to manage a second durable
  warehouse from the same service instead of rejecting non-default warehouse
  path parameters.
- Added governed project list/upsert management endpoints and a
  `ProjectManage` capability action.
- Added durable `ProjectRecord` persistence to memory and Turso stores.
- Added catalog-facing Project graph events and durable `project.upserted`
  outbox replay to the graph sink.
- Added governed warehouse list/upsert management endpoints and a
  `WarehouseManage` capability action.
- Added durable `WarehouseRecord` persistence to memory and Turso stores.
- Added catalog-facing Warehouse graph events and durable `warehouse.upserted`
  outbox replay to the graph sink.
- Added stable catalog-facing `Column` and `Snapshot` graph event constructors
  plus default and `grust-local` graph-crate coverage.
- Added compact table metadata graph summaries to table create/load audit
  payloads and replayed those summaries from the durable outbox into column and
  snapshot graph anchors.
- Added principal graph projection to durable outbox drain for non-anonymous
  resolved principals, while preserving existing domain graph and OpenLineage
  replay.
- Added a stable principal subject helper plus default and `grust-local`
  graph-crate coverage for the catalog-facing principal event shape.
- Added commit graph projection to durable outbox drain for `table.commit`, while
  preserving the existing table graph and OpenLineage replay.
- Added a stable commit subject helper plus default and `grust-local`
  graph-crate coverage for the catalog-facing commit event shape.
- Added scan-plan graph projection to durable outbox drain for
  `table.scan-planned` and `table.scan-tasks-fetched`, while preserving the
  existing table graph and OpenLineage replay.
- Added a stable scan-plan subject helper plus default and `grust-local`
  graph-crate coverage for the catalog-facing scan-plan event shape.
- Added policy-binding graph projection to the durable outbox drain, so
  `policy-binding.upserted` events now replay to the graph sink with ODRL and
  authorization payloads intact.
- Added a stable policy subject helper plus default and `grust-local` graph-crate
  coverage for the catalog-facing policy event shape.
- Added namespace graph projection to the durable outbox drain, so
  `namespace.created` events now replay to both graph and lineage sinks.
- Added a stable namespace subject helper and graph-crate unit coverage for the
  catalog-facing namespace event shape.
- Added verified QueryGraph bootstrap bundle, graph, OpenLineage, standards, and
  table hash evidence to the `querygraph.bootstrap` audit/outbox payload.
- Extended lineage replay tests to prove bootstrap hash evidence survives the
  outbox drain into OpenLineage-shaped events.
- Added a QueryGraph bootstrap manifest `graph-hash` and verification failure
  for graph projection drift.
- Made `lakecat-cli qglake-fixture` verify the QGLake table graph node and
  namespace edge before accepting the bootstrap bundle.
- Extended the lineage drain API response with delivered event types, graph
  event count, and lineage event count.
- Made `lakecat-cli qglake-fixture` require the drain summary to include
  `querygraph.bootstrap` and at least one lineage emission.
- Added embedded memory-store audit/outbox delivery parity for explicit catalog
  audit events, matching the Turso lineage-and-graph outbox envelope.
- Made `lakecat-cli qglake-fixture` reject a lineage drain that delivers zero
  events, turning the QGLake acceptance run into a real replay check.
- Added a governed lineage-drain endpoint and CLI command, and wired
  `lakecat-cli qglake-fixture` to drain lineage/outbox events after writing the
  verified QueryGraph bootstrap bundle.
- Projected `querygraph.bootstrap` outbox events into LakeCat OpenLineage output
  events while preserving bootstrap authorization/request-identity payloads.
- Added QGLake-specific QueryGraph bootstrap verification to
  `lakecat-cli qglake-fixture`, covering policy bindings, ODRL restriction
  export, and OpenLineage output presence before the bootstrap file is written.
- Made `lakecat-cli qglake-fixture` repeatable by accepting existing namespace
  and table resources only after loading and validating that they match the
  expected QGLake fixture shape.
- Added a live governed scan-plan verification step to
  `lakecat-cli qglake-fixture` after policy installation and before bootstrap
  export.
- Added a fixture verifier test proving `raw_payload` is removed from the
  effective projection and the row predicate survives in the scan extension.
- Added `QueryGraphBootstrap::from_tables_with_policy_bindings` and a
  per-table `policy-bindings` projection with its own manifest hash.
- Changed `/querygraph/v1/bootstrap` to load stored table policy bindings and
  include the actual ODRL documents in the QueryGraph table projection and ODRL
  artifact.
- Added a `raw_payload` column to the QGLake fixture table metadata and kept it
  outside the fixture policy's allowed agent columns.
- Extended the QGLake fixture policy with `lakecat:read-restriction` allowed
  columns, row predicate, and max credential TTL, verified through
  `ReadRestriction::from_odrl_policies`.
- Surfaced governed scan-task fetch `read-restriction`, storage location, and
  metadata location in the top-level `table.scan-tasks-fetched` audit/outbox
  payload.
- Routed `table.scan-tasks-fetched` outbox records through the existing scan
  graph/OpenLineage projection path so fetched concrete file work carries the
  governed restriction context to QueryGraph consumers.
- Surfaced governed scan-planning `read-restriction`, storage location, and
  metadata location in the top-level `table.scan-planned` audit/outbox payload,
  matching the nested authorization receipt context.
- Extended the OpenLineage table-scan projection test to prove the governed
  restriction is preserved in the LakeCat catalog dataset facet for QueryGraph
  consumers.
- Extended authorization restriction derivation from scan planning to credential
  vending so raw credential issuance sees the same policy-derived allowed
  columns, row predicate, TTL, purpose, and policy hashes.
- Surfaced governed credential-vending `read-restriction` and
  `lakecat:raw-credential-exception` markers in the top-level
  `credentials.vend-attempted` audit/outbox payload, matching the nested
  authorization receipt context.
- Marked governed credential-vending authorization context with
  `lakecat:raw-credential-exception = true` so audit/outbox receipts distinguish
  the deliberate exception path from the preferred governed Sail-planned reads.
- Added a service test proving the credential issuer receives the governed
  credential authorization receipt with the composed read restriction and raw
  credential exception marker.
- Enabled the TypeSec RBAC feature for LakeCat's `typesec-local` integration.
- Added `TypeSecGovernanceEngine::rbac_from_yaml`, a narrow constructor that
  loads RBAC policy text through TypeSec's `RbacEngine` and returns LakeCat
  errors on invalid policy documents.
- Added `LAKECAT_TYPESEC_RBAC_POLICY` support to the service binary so local
  deployments can boot with a real RBAC fallback policy instead of the
  allow-all placeholder.
- Added focused tests proving RBAC policy loading authorizes matching table
  scan requests and missing/invalid policy files fail closed.
- Extended `ReadRestriction::from_odrl_policies` to parse max credential TTLs
  from top-level, nested `lakecat:read-restriction` / `readRestriction`, and
  ODRL constraint forms.
- Changed TTL composition to keep the shortest governed credential lifetime
  across multiple active policy bindings.
- Added security tests proving constraint-based TTL extraction, shortest-TTL
  composition, and fail-closed rejection for non-numeric TTL constraints.
- Added `LakeCatCatalogProvider::fetch_table_scan_tasks` and
  `fetch_table_scan_tasks_for_ident`, applying the provider-owned scan
  authorization and shared `ReadRestriction` mandatory projection/filter
  requirements before delegating plan-task expansion to Sail.
- Routed REST `sail-local` `fetch-scan-tasks` through the provider fetch seam
  while preserving the direct `FetchScanTasksRequest` path for default builds.
- Added a provider recording-engine test proving policy-derived projection and
  row predicates are passed into Sail during provider-routed fetch expansion.
- Routed REST `sail-local` scan planning through
  `LakeCatCatalogProvider::plan_table_scan_for_ident`, so REST planning now
  uses the provider-owned scan authorization and governed plan seam.
- Enabled `lakecat-sail/catalog-provider` from the service `sail-local` feature
  and preserved the direct `ScanPlanningRequest` path for builds without local
  Sail provider integration.
- Preserved HTTP error semantics across the provider route by mapping provider
  invalid-argument, not-found, conflict, and not-supported failures back to
  LakeCat HTTP errors instead of flattening them to 500s.
- Added `ReadRestriction` to `lakecat-security` and exposed it through
  `TableScanCapability` so the scan capability carries the server-owned
  restriction from the authorization receipt.
- Added governed row-predicate extraction from enforced ODRL policy bindings,
  including nested `lakecat:read-restriction` fields and ODRL row-predicate
  constraints.
- Composed multiple enforced row predicates with `and` so additional bindings
  can only narrow the governed read surface.
- Proved Sail receives the policy-derived row predicate as an accepted scan
  filter while column projection is also narrowed by policy.
- Bound structured Sail plan-task tokens to the effective projection, preserving
  the column narrowing context through manifest-list expansion.
- Reapplied governed projection/filter requirements on `fetch-scan-tasks` by
  recomputing the current `ReadRestriction` in LakeCat and passing mandatory
  requirements into the Sail expansion path.
- Added a negative guard that legacy/no-projection plan-task tokens cannot
  satisfy a governed projection, preventing empty projection from widening to
  all columns during fetch.
- Added `TypeSecGovernanceEngine::with_fallback`, backed by TypeSec's
  `ComposedEngine` with `PriorityOrder`, so LakeCat can compose ODRL-style
  delegation onto an RBAC-style fallback without implementing local policy
  semantics.
- Added a `typesec-local` test proving a delegated primary policy allows
  authorization through a fallback engine while preserving the authorization
  context and policy hash.
- Wired REST table commits to the store's existing idempotency replay by
  parsing `x-lakecat-idempotency-key` and passing it into `TableCommit`.
- Added conservative idempotency-key validation: non-empty ASCII keys up to 128
  characters using alphanumeric characters plus `-`, `_`, `.`, and `:`.
- Added a service-level test proving two REST commits with the same
  idempotency key replay to one table version and one commit-log row.
- Added a `PlannedMetadataWrite` handle for local `file://` metadata writes so
  the service can clean up a newly written metadata JSON object if the store
  commit/CAS step rejects the commit.
- Added a `sail-local` service test proving a stale commit requirement that
  writes a new metadata location returns `409 Conflict` and removes the
  uncommitted metadata file.
- Added `TableCommitRecord::idempotency_key_sha256`, populated from accepted
  keyed commits and serialized through the metadata pointer log, audit payload,
  and outbox payload.
- Extended store and REST commit idempotency tests to prove the durable hash is
  present and the raw idempotency key is not written to the outbox payload.
- Hardened memory and Turso idempotency replay to persist/check a normalized
  idempotency request hash and reject a reused idempotency key when the request
  body differs from the accepted commit.
- Extended REST and Turso idempotency tests to prove mismatched keyed retries
  return conflict and do not create a second table version or pointer-log row.
- Moved ODRL read-restriction parsing and composition into
  `ReadRestriction::from_odrl_policies` in `lakecat-security`.
- Kept the REST authorization path behavior-equivalent by deriving
  table-scan restrictions from stored `PolicyBinding` ODRL documents through
  the shared security primitive.
- Added security-crate tests proving ODRL policy documents compose allowed
  columns by intersection, row predicates by `and`, and purpose/credential TTL
  into one `ReadRestriction`, plus a negative guard for non-object row
  predicates.
- Moved effective projection narrowing, stats-field narrowing, and mandatory
  row-filter extraction from `lakecat-service` into reusable `ReadRestriction`
  methods.
- Updated REST scan planning and fetch-scan-tasks to call the shared
  `ReadRestriction` methods while preserving the existing governed Sail request
  behavior.
- Added the LakeSail book source, publishing runbook, build/validation scripts,
  and stable generated PDF/EPUB/MOBI deliverables under `docs/book/`.
- Kept the generated versioned Kindle EPUB symlink ignored by `.gitignore` while
  validating that it points to the stable `lakesail.epub` deliverable.
- Added `LakeCatCatalogProvider::authorize_table_scan`, which reads table policy
  bindings from the catalog store, composes a shared `ReadRestriction`, and
  returns a `TableScanCapability` carrying the restriction in the authorization
  receipt context.
- Added a provider test proving stored ODRL policy bindings are visible through
  the provider scan capability and preserve the Sail-provider context.
- Added `LakeCatCatalogProvider::plan_table_scan` and a small provider scan
  request type so provider-routed scan planning can apply governed projection
  and row filters before invoking the configured Sail engine.
- Added a recording-engine provider test proving policy-derived projection and
  row predicates are passed into Sail from the provider scan-planning seam.
- Parsed a minimal enforceable ODRL subset from active policy bindings:
  `allowed-columns` / `allowedColumns` at the policy root, in
  `lakecat:read-restriction`, or in ODRL constraints, plus purpose and policy
  hashes.
- Applied allowed-column restrictions before Sail scan planning. Empty client
  projection under a restriction now means the allowed columns, and client
  projection can only narrow within those columns.
- Added tests proving scan authorization carries the restriction and that a
  `sail-local` scan requesting `event_id,payload` is narrowed to `event_id`
  before Sail receives it.
- Added the OPUS2 review/design docs to the tracked design record and updated
  the OPUS2 plan with this first restriction slice.
- Changed the QueryGraph bootstrap OSI artifact from a LakeCat-authored semantic
  model into a QueryGraph-owned OSI handoff. The handoff keeps the manifest hash
  contract and stable field anchors, but no longer publishes LakeCat-owned OSI
  metrics, dimensions, joins, ontology claims, business semantic names, or SQL
  field expressions.
- Updated architecture and OPUS working-plan docs so LakeCat is the catalog
  discovery substrate for OSI import while QueryGraph owns rich semantic model
  authorship.
- Fixed the Sail patch bridge to pass `git am` committer identity in GitHub
  Actions after run `27722483267` failed before tests with "Committer identity
  unknown".
- Fixed the Sail patch bridge path after run `27722686028` failed before tests
  because `../lakecat/ci/sail-patches/*.patch` did not exist from the Actions
  workspace root.
- Fixed the Sail patch bridge again after run `27722752741` showed that
  `git -C sail` resolves expanded relative patch paths from inside the Sail
  checkout; the workflow now computes an absolute `PATCH_DIR`.
- Added `ci/sail-patches/` with the Sail helper/model API patches LakeCat
  already depends on locally.
- Updated manual GitHub Actions to apply those patches to the Sail checkout
  before building LakeCat.
- Recorded that manual run `27720653125` moved past formatting, TypeSec 0.8
  resolution, and `protoc`, and is now blocked on unpublished Sail helper
  commits.
- Added `protobuf-compiler` installation to the manual GitHub Actions workflow
  so Sail's `prost-build` custom build can find `protoc` on Ubuntu runners.
- Scoped the manual GitHub Actions formatting check to the LakeCat workspace
  packages instead of `cargo fmt --all`, so sibling checkout formatting does not
  fail LakeCat's cloud gate before the matrix tests start.
- Expanded manual GitHub Actions coverage to include
  `cargo test -p lakecat-service --features typesec-local`.
- Added an explicit manual CI row for the Grust Cypher catalog-graph boundary
  test.
- Bumped the LakeCat TypeSec path dependency to 0.8.0.
- Enabled Grust's `cypher` facade feature for `grust-local` graph integration.
- Added a LakeCat graph boundary test that writes the Grust-owned catalog graph
  projection to `MemoryGraphStore` and verifies Grust Cypher can mutate/query it
  without LakeCat owning Cypher parsing, traversal, or graph execution.
- Added a `typesec-local` TypeDID verifier seam in LakeCat service that asks
  TypeSec to open and verify protected TypeDID envelopes.
- Authorization now upgrades anonymous or matching supplied request identity to
  the verified DID subject and attaches only an audit-safe attestation plus
  envelope hash to the authorization context.
- TypeSec now exposes `TypeDidAttestation` from verified TypeDID messages so
  LakeCat can persist receipts without raw plaintext payloads or signatures.
- Exported Sail's Iceberg REST `LoadTableResult` to `TableStatus` conversion as
  a reusable `sail-catalog-iceberg` helper.
- Updated LakeCat's in-process Sail `CatalogProvider` to use the Sail helper for
  stable Iceberg metadata and keep only LakeCat-specific properties plus the v4
  extension fallback local.
- Added Sail-owned Iceberg REST planning-result helpers and updated LakeCat's
  Sail-backed scan planning/fetch path to validate generated standard response
  payloads through them before returning LakeCat extension fields.
- Added a reusable LakeCat catalog-event graph taxonomy helper to Grust, covering
  catalog events, warehouses, namespaces, and tables with stable containment
  edges.
- Updated LakeCat's `grust-local` graph sink to call the Grust helper and pass
  durable outbox event ids into graph event vertices.
- Added a reusable LakeCat catalog graph adapter to Grust in commit
  `15952a9 Add LakeCat catalog graph adapter`.
- Updated QueryGraph in commit `657fd6a Validate LakeCat imports with Grust`
  so `lakecat-import` validates LakeCat graph envelopes with Grust and reports
  graph size from the validated graph.
- Verified the LakeCat-generated QGLake bundle through QueryGraph's
  `lakecat-import` path.
- QueryGraph now checks the outer LakeCat bundle hash, validates the graph
  envelope through Grust, and writes an import plan for downstream graph
  ingestion.
- No graph taxonomy, traversal, or ingest mechanics moved into LakeCat; reusable
  graph work remains targeted at Grust.

## Verification Completed

- LakeSail book artifact checks passed:
  `bash docs/book/check_epub_metadata.sh docs/book/dist/lakesail.epub 'lakesail (0.1.0)'`,
  `pdftotext -f 1 -l 1 docs/book/dist/lakesail.pdf -`,
  `pdftotext -f 2 -l 2 docs/book/dist/lakesail.pdf -`, and
  `readlink 'docs/book/dist/lakesail (0.1.0).epub'`.
- Provider-side governed scan authorization checks passed:
  `cargo fmt -p lakecat-sail -- --check`,
  `cargo test -p lakecat-sail --features catalog-provider provider_scan_authorization_carries_policy_restriction -- --nocapture`, and
  `cargo test -p lakecat-sail --features catalog-provider provider_resolves_governed_tables_in_process -- --nocapture`,
  `cargo test -p lakecat-sail --features catalog-provider`,
  `cargo test -p lakecat-sail --all-features`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Provider-side governed scan planning checks passed:
  `cargo fmt -p lakecat-sail -- --check`,
  `cargo test -p lakecat-sail --features catalog-provider provider_scan_planning_applies_policy_restriction_before_sail -- --nocapture`, and
  `cargo test -p lakecat-sail --features catalog-provider provider_scan_authorization_carries_policy_restriction -- --nocapture`,
  `cargo test -p lakecat-sail --features catalog-provider`,
  `cargo test -p lakecat-sail --all-features`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Reusable read-restriction parser checks passed:
  `cargo fmt -p lakecat-security -p lakecat-service -- --check`,
  `cargo test -p lakecat-security read_restriction -- --nocapture`,
  `cargo test -p lakecat-security`,
  `cargo test -p lakecat-service table_scan_authorization_carries_policy_read_restriction -- --nocapture`, and
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Reusable read-restriction application checks passed:
  `cargo fmt -p lakecat-security -p lakecat-service -- --check`,
  `cargo test -p lakecat-security read_restriction -- --nocapture`,
  `cargo test -p lakecat-security`,
  `cargo test -p lakecat-service`,
  `cargo test -p lakecat-service effective_projection_cannot_widen_policy_columns -- --nocapture`, and
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Strict idempotency replay checks passed:
  `cargo fmt -p lakecat-store -p lakecat-service -p lakecat-sail -- --check`,
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`,
  `cargo test -p lakecat-service --all-features commit_replays_rest_idempotency_key -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local turso_store_round_trips_namespaces_tables_and_idempotent_commits -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local`,
  `cargo test -p lakecat-service`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Audit-safe idempotency evidence focused checks passed:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`,
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local turso_store_round_trips_namespaces_tables_and_idempotent_commits -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local`,
  `cargo test -p lakecat-service`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Bounded metadata orphan cleanup focused checks passed:
  `cargo fmt -p lakecat-service -- --check`,
  `cargo test -p lakecat-service --features sail-local stale_commit_cleans_up_uncommitted_metadata_file -- --nocapture`,
  `cargo test -p lakecat-service --features sail-local stale_commit_requirement_returns_conflict_with_sail_local_engine -- --nocapture`,
  `cargo test -p lakecat-service --features sail-local`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- REST commit idempotency focused checks passed:
  `cargo fmt -p lakecat-service -- --check`,
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`,
  `cargo test -p lakecat-service`,
  `cargo test -p lakecat-store --features turso-local`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- TypeSec delegate fallback focused checks passed:
  `cargo fmt -p lakecat-security -- --check`,
  `cargo test -p lakecat-security --features typesec-local delegates_to_typesec_fallback_policy_engine -- --nocapture`,
  `cargo test -p lakecat-security --features typesec-local delegates_authorization_to_typesec_policy_engine -- --nocapture`,
  `cargo test -p lakecat-security --features typesec-local`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Governed row-predicate focused checks passed:
  `cargo fmt -p lakecat-service -- --check`,
  `cargo test -p lakecat-service table_scan_authorization_carries_policy_read_restriction`,
  and
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`.
- Governed fetch-token reapplication focused checks passed:
  `cargo fmt -p lakecat-sail -p lakecat-service -- --check`,
  `cargo test -p lakecat-sail --features sail-local preserves_filter_context_and_prunes_loaded_file_bounds -- --nocapture`,
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`,
  `cargo test -p lakecat-service --all-features`,
  `cargo test -p lakecat-sail --all-features`,
  and `cargo test --workspace --all-features`.
- Governed read restriction focused checks passed:
  `cargo fmt -p lakecat-security -p lakecat-service -- --check`,
  `cargo test -p lakecat-security`,
  `cargo test -p lakecat-service table_scan_authorization_carries_policy_read_restriction`,
  `cargo test -p lakecat-service effective_projection_cannot_widen_policy_columns`,
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`,
  `cargo test -p lakecat-sail --all-features preserves_filter_context_and_prunes_loaded_file_bounds -- --nocapture`,
  `cargo test -p lakecat-service`,
  `cargo test -p lakecat-service --all-features`,
  `cargo test --workspace --all-features`,
  and `git diff --check`.
- Applied `ci/sail-patches/*.patch` with `git am` to a clean temporary Sail
  checkout at `7a34be78`.
- Temporary patched Sail checkout passed:
  `cargo test -p sail-catalog-iceberg planning -- --nocapture`.
- Current `lakehq/sail@ceab87693f8e37f50d855ba6cf479c3a169ccc95` accepted the
  patch series with the identity-configured `git am` command and passed:
  `cargo test -p sail-catalog-iceberg planning -- --nocapture`.
- A GitHub Actions-shaped temporary directory with sibling `sail/` and
  `lakecat/` paths successfully applied the patch series using the workflow's
  absolute `PATCH_DIR` shell block.
- Local workflow matrix commands passed before rerunning any cloud gate:
  `cargo test --workspace`,
  `cargo test -p lakecat-service --features sail-local`,
  `cargo test -p lakecat-security --features typesec-local`,
  `cargo test -p lakecat-service --features typesec-local`,
  `cargo test -p lakecat-graph --features grust-local`,
  `cargo test -p lakecat-graph --features grust-local grust_cypher_can_query_lakecat_catalog_projection_boundary -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local`, and
  `cargo test --workspace --all-features`.
- QueryGraph OSI handoff focused checks passed:
  `cargo fmt -p lakecat-querygraph -- --check`,
  `cargo test -p lakecat-querygraph`,
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog_tables`,
  and `git diff --check`.
- LakeCat focused Sail-local service test passed:
  `cargo test -p lakecat-service --features sail-local fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens -- --nocapture`.
- Manual GitHub Actions run `27720360961` after pushing TypeSec 0.8 reached the
  matrix tests. Passing rows: `grust-local graph`, `grust cypher boundary`,
  `typesec-local security`, and `turso-local store`. Failed rows now all report
  missing `protoc` from Sail's `sail-common-datafusion` build script.
- Manual GitHub Actions run `27720653125` after installing `protobuf-compiler`
  proved `protoc` is no longer missing. Passing rows: `grust cypher boundary`,
  `grust-local graph`, `typesec-local security`, and `turso-local store`.
  Failed rows now report missing Sail helper exports such as
  `LoadTableResult`, `load_table_result_to_status`,
  `completed_planning_with_id_result_from_values`, and
  `fetch_scan_tasks_result_from_values` from the cloud `lakehq/sail@main`
  checkout.
- `cargo fmt -p lakecat-api -p lakecat-cli -p lakecat-core -p lakecat-graph -p lakecat-lineage -p lakecat-querygraph -p lakecat-sail -p lakecat-security -p lakecat-service -p lakecat-store -- --check`
- `git diff --check`
- `cargo test -p lakecat-service --features typesec-local -- --nocapture`
- `cargo test -p lakecat-graph --features grust-local grust_cypher_can_query_lakecat_catalog_projection_boundary -- --nocapture`
- `git diff --check`
- `cargo fmt -p lakecat-graph -p lakecat-service -- --check`
- `cargo test -p lakecat-graph --features grust-local grust_cypher_can_query_lakecat_catalog_projection_boundary -- --nocapture`
- `cargo test -p lakecat-graph --features grust-local -- --nocapture`
- `cargo test --workspace`
- `cargo test -p lakecat-service --features typesec-local -- --nocapture`
- `cargo test --workspace --all-features`
- Grust focused check in `/Users/alexy/src/grust`:
  `cargo test -p grust-graph --features cypher,memory lakecat -- --nocapture`
- TypeSec focused check in `/Users/alexy/src/typesec`:
  `cargo test -p typesec-integrations typedid_verified_message_exposes_audit_safe_attestation -- --nocapture`
- `cargo fmt -p lakecat-service -- --check`
- `cargo fmt -p typesec-integrations -p typesec -- --check`
- `cargo test -p typesec-integrations typedid_verified_message_exposes_audit_safe_attestation -- --nocapture`
- `cargo test -p typesec-integrations typedid_signature_covers_conversation_metadata -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local typesec_typedid_envelope_verification_updates_authorization_context -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local -- --nocapture`
- `cargo test -p lakecat-service --all-features`
- `cargo test --workspace --all-features`
- `git diff --check`
- `cargo fmt --all -- --check` (passes with existing stable-rustfmt warnings for
  nightly-only `imports_granularity` / `group_imports` config keys)
- `cargo check -p lakecat-cli`
- `cargo test -p lakecat-cli`
- Live LakeCat service with `LAKECAT_TURSO_PATH=target/qglake-live/catalog.db`
  and `LAKECAT_BIND_ADDR=127.0.0.1:18281`
- `cargo run -p lakecat-cli -- config --catalog http://127.0.0.1:18281`
- `cargo run -p lakecat-cli -- qglake-fixture --catalog http://127.0.0.1:18281 --output target/qglake-live/lakecat-bootstrap.json`
- `jq` inspection of `target/qglake-live/lakecat-bootstrap.json`
- QueryGraph verifier:
  `cargo run -- lakecat-verify --bundle /Users/alexy/src/lakecat/target/qglake-live/lakecat-bootstrap.json`
  in `/Users/alexy/src/querygraph/qg-rust`
- QueryGraph importer:
  `cargo run -- lakecat-import --bundle /Users/alexy/src/lakecat/target/qglake-live/lakecat-bootstrap.json --output .querygraph/lakecat/import-plan.json`
  in `/Users/alexy/src/querygraph/qg-rust`
- Grust facade tests: `cargo test -p grust-graph`
- Sail Iceberg catalog test:
  `cargo test -p sail-catalog-iceberg test_get_table -- --nocapture`
- LakeCat Sail catalog-provider tests:
  `cargo test -p lakecat-sail --features catalog-provider catalog_provider -- --nocapture`
- Sail Iceberg planning helper tests:
  `cargo test -p sail-catalog-iceberg planning -- --nocapture`
- LakeCat Sail scan-planning tests:
  `cargo test -p lakecat-sail --features sail-local validates_scan_with_sail_rest_models -- --nocapture`
  and
  `cargo test -p lakecat-sail --features sail-local expands_local_manifest_list_with_sail_io -- --nocapture`
- LakeCat service Sail scan/fetch tests:
  `cargo test -p lakecat-service --features sail-local fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens -- --nocapture`
  and
  `cargo test -p lakecat-service --features sail-local create_load_commit_and_plan_table_round_trips_through_integrations -- --nocapture`
- LakeCat graph tests: `cargo test -p lakecat-graph --features grust-local`
- LakeCat service outbox test:
  `cargo test -p lakecat-service --features grust-local outbox_drain_projects_table_events_to_sinks -- --nocapture`
- Grust Sail compile check: `cargo check -p grust-sail`
- QueryGraph tests: `cargo test` in `/Users/alexy/src/querygraph/qg-rust`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Keep `scripts/qglake-handoff-local.sh` in the local verification loop whenever
QGLake handoff behavior changes, then continue tightening the handoff boundary
without adding new non-standard Iceberg access paths. If the next step starts
to become reusable typed view-history or Iceberg view-history semantics, push
that model into Sail first and consume it through LakeCat's existing seams.
Keep CI manual-only until local gates are green and the temporary Sail patch
bridge can be replaced by an upstream branch or published Sail helper crate.
