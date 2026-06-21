# LakeCat Status

Updated: 2026-06-21

## Current State

- LakeCat is on `master`.
- Latest completed implementation slice:
  `Bind captured QGLake replay lines to proof fields`.
  Saved handoff verification now recomputes the operator-facing
  `scan-replay` and `credential-replay` text lines from compact
  `governedScanProof` and `credentialVendingProof` fields. A captured replay
  artifact is rejected if those lines drift from the verified purpose, TTL cap,
  task counts, credential decision, or redacted credential storage-scope
  evidence.
- Local verification for this captured replay-line binding slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli qglake -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, ran
  LakeCat replay, QueryGraph verify/import, compared captured replay-line text
  with compact scan and credential proof fields, and ended with
  `QGLake handoff verified`).
- Latest completed implementation slice:
  `Prove outbox batch retry on later projection failure`.
  The service outbox drain now has a focused multi-event regression proving
  that if an earlier event projects successfully but a later lineage projection
  in the same batch fails, LakeCat does not acknowledge any event in that
  batch. The durable outbox remains the recovery source even after graph
  projections have already been emitted for the earlier and failing events.
- Local verification for this outbox batch retry proof is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_does_not_acknowledge_earlier_events_when_later_projection_fails -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --test-threads=1`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --test-threads=1`;
  `cargo test -p lakecat-service --all-features outbox_drain -- --test-threads=1`
  (green with an existing all-features warning for unused test helper
  `CapturingSailEngine`);
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind compact view receipt hashes to structural digest`.
  QGLake handoff verification now recomputes each compact structural
  `receiptHash` from the same content-derived view-version receipt digest that
  LakeCat service emits. Compact receipt bodies now include view hash,
  principal subject, principal kind, and recorded timestamp, so saved compact
  view proofs cannot keep a valid receipt-chain hash while changing the
  catalog-facing receipt body underneath an opaque hash.
- Local verification for this compact structural receipt digest slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `CARGO_INCREMENTAL=0 cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_view_receipt_hash_digest_drift -- --nocapture`;
  `CARGO_INCREMENTAL=0 cargo test -p lakecat-cli qglake`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, ran
  LakeCat replay, QueryGraph verify/import, recomputed compact structural view
  receipt digests from API-provided receipt bodies with UTC timestamp
  normalization, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Bind compact view chain hashes to structural digest`.
  QGLake handoff verification now recomputes each compact structural
  `chainHash` from the same content-derived receipt-chain digest that LakeCat
  service emits: stable view identity, latest view version, latest operation,
  tombstone state, and the ordered structural receipt hashes. Saved compact
  view proofs are rejected when a declared/covered chain hash does not match
  the ordered receipt-chain body.
- Local verification for this compact structural chain digest slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_view_chain_hash_digest_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `cargo test -p lakecat-cli qglake`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, ran
  LakeCat replay, QueryGraph verify/import, recomputed compact structural view
  receipt-chain digests, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Bind compact view receipt hash arrays to structure`.
  QGLake handoff verification now requires each namespace view receipt-chain
  summary's `chainHashes` and `receiptHashes` arrays to be duplicate-free,
  rejects duplicate structural chain hashes, and requires `receiptHashes` to
  match the structural
  `receiptChains[].chains[].receipts[].receiptHash` set exactly. Saved compact
  view proofs are rejected when namespace hash arrays include extra hashes,
  omit structural receipt hashes, or duplicate view-history chain evidence.
- Local verification for this compact receipt hash array binding slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_duplicate_view_receipt_chain_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_duplicate_structural_view_chain_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_duplicate_view_receipt_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_view_receipt_hash_structural_mismatch -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `cargo test -p lakecat-cli qglake`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, ran
  LakeCat replay, QueryGraph verify/import, verified duplicate-free exact
  compact view receipt hash array coverage, and ended with
  `QGLake handoff verified`).
- Latest completed implementation slice:
  `Bind compact view hash coverage per stable view`.
  QGLake handoff verification now maps structural receipt-chain hashes and
  per-receipt hashes by stable view ID. Accepted view
  `acceptedReceiptChainHash` values and tombstone receipt hashes must be
  covered by structural receipt-chain evidence for the same stable view, not
  merely by some other chain in the namespace.
- Local verification for this compact per-view receipt-chain coverage slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_cross_view_receipt_chain_hash_splice -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_cross_view_tombstone_receipt_hash_splice -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `cargo test -p lakecat-cli qglake`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, ran
  LakeCat replay, QueryGraph verify/import, verified compact per-view
  receipt-chain hash coverage, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Bind compact tombstone receipt identity components`.
  QGLake handoff verification now checks tombstone receipt `stableId` values
  against their warehouse, namespace, and view-name components before accepting
  expected-version guard evidence. Saved compact view proofs are rejected when
  deletion-proof component fields drift from the stable view identity.
- Local verification for this compact tombstone receipt identity slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_view_tombstone_stable_id_component_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `cargo test -p lakecat-cli qglake`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, ran
  LakeCat replay, QueryGraph verify/import, verified compact tombstone receipt
  identity binding, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Bind compact view stable IDs to components`.
  QGLake handoff verification now derives the expected
  `lakecat:view:<warehouse>:<namespace>:<name>` stable ID for accepted views
  and structural receipt-chain summaries. Saved compact view proofs are
  rejected when visible warehouse, namespace, or view-name components drift
  from the stable ID even if the verified view set and hash evidence still
  look valid.
- Local verification for this compact view stable-ID component slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_view_stable_id_component_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_view_receipt_chain_stable_id_component_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `cargo test -p lakecat-cli qglake`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, ran
  LakeCat replay, QueryGraph verify/import, verified compact view stable-ID
  component binding, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Bind compact view receipt-chain identities`.
  QGLake handoff verification now binds namespace receipt-chain groups to
  their structural chain summaries and binds each structural chain to every
  per-receipt identity. A saved compact view proof is rejected if the chain
  warehouse or namespace drifts from its enclosing receipt-chain group, or if a
  receipt's stable ID, warehouse, namespace, or view name drifts from the chain
  identity.
- Local verification for this compact view receipt-chain identity slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_view_receipt_chain_group_identity_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_view_receipt_chain_receipt_identity_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `cargo test -p lakecat-cli qglake`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, ran
  LakeCat replay, QueryGraph verify/import, verified compact view
  receipt-chain identity binding, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Verify compact view receipt-chain structure`.
  QGLake replay and handoff proofs now carry compact structural view
  receipt-chain evidence in `receiptChains[].chains[]`, including stable view
  identity, chain hash, verified flag, latest version/operation, tombstone
  state, receipt count, and per-receipt version, operation, receipt hash, and
  previous-link fields. The handoff verifier rejects invalid chain heads,
  forged previous links, skipped upsert versions, unsupported operations, drops
  that advance the durable version, and chain heads that do not match the latest
  receipt. Tombstoned accepted views derive a compact historical accepted chain
  from the verified receipt prefix, so both the drop chain and accepted
  pre-drop chain hash have structural proof.
- Local verification for this compact view receipt-chain structure slice is
  green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service outbox_drain -- --test-threads=1`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --test-threads=1`;
  `cargo test -p lakecat-service --all-features outbox_drain -- --test-threads=1`;
  `cargo test -p lakecat-api`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import,
  verified compact structural view receipt-chain evidence in the handoff
  summary and saved verifier output, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Verify saved handoff replay sections`.
  Saved `lakecat-handoff-verify.json` artifacts now have to preserve every
  compact `capturedOutputSemantics.lakecatReplay` proof section, including
  request identity, QueryGraph bootstrap, governed scans, table commit history,
  view receipt chains, management ID arrays, storage-profile upsert evidence,
  and credential-vending proof. The verifier normalizes management proof
  semantics to the declared QGLake proof fields while checking storage-profile
  upsert evidence as its own section, so raw replay duplication cannot create a
  false mismatch and real proof drift is still rejected.
- Local verification for this saved handoff replay sections slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import,
  re-verified the handoff summary with the saved verifier-output artifact hash
  and replay proof sections, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject captured management ID drift`.
  Captured LakeCat replay output must now match compact QGLake
  `managementProof` ID arrays for servers, projects, warehouses, policies, and
  storage profiles. Saved handoff summaries therefore cannot preserve valid
  artifact hashes while drifting catalog management identities between the
  captured replay artifact and compact proof.
- Local verification for this captured management ID drift slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics_rejects_management_id_drift`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `cargo test -p lakecat-cli qglake`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import,
  verified the handoff summary with captured management ID agreement, and ended
  with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Carry management-list IDs through QGLake proof`.
  Lineage-drain summaries and compact QGLake `managementProof` now preserve the
  redacted stable ID arrays emitted by management-list reads. LakeCat replay
  verification and saved handoff-summary verification reject missing, empty, or
  count-drifted management ID arrays before accepting the compact proof.
- Local verification for this management-list QGLake proof slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics_accept_matching_files`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service outbox_drain -- --test-threads=1`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --test-threads=1`;
  `cargo test -p lakecat-service --all-features outbox_drain -- --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import,
  verified the handoff summary with management ID arrays, and ended with
  `QGLake handoff verified`).
- Latest completed implementation slice:
  `Harden management-list outbox ID evidence`.
  Management-list audit/outbox reads now carry redacted stable ID arrays beside
  their counts for policies, projects, servers, storage profiles, and
  warehouses. Outbox draining rejects malformed or count-drifted optional ID
  arrays before graph projection, OpenLineage emission, or delivery
  acknowledgement.
- Local verification for this management-list ID evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service management_list -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `cargo test -p lakecat-service --all-features outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import,
  verified the handoff summary, and ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox QueryGraph bootstrap evidence`.
  Outbox draining now rejects malformed `querygraph.bootstrap` pending events
  when their warehouse, table/view counts, verified ids, manifest hashes,
  artifact hashes, view receipt hashes, standards, or optional TypeDID/agent
  proof hashes are missing or malformed. Unsafe QueryGraph bootstrap replay
  evidence stays pending and reaches neither graph projection nor lineage
  acknowledgement.
- Local verification for this outbox QueryGraph bootstrap evidence slice is
  green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_querygraph_bootstrap_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `cargo test -p lakecat-service --all-features outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox scan evidence`.
  Outbox draining now rejects malformed `table.scan-planned` and
  `table.scan-tasks-fetched` pending events when their table identity,
  projection/stat arrays, task counts, required filters, or governed
  read-restriction projection constraints are missing, widened, or
  contradictory. Unsafe scan replay evidence stays pending and reaches neither
  graph projection nor lineage acknowledgement.
- Local verification for this outbox scan evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_scan_planned_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_scan_fetch_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `cargo test -p lakecat-service --all-features outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox table commit evidence`.
  Outbox draining now rejects `table.commit` pending events when the commit
  object, unsigned sequence number, root table identity, or optional
  commit-table identity evidence is missing or contradictory. Unsafe commit
  replay evidence stays pending and reaches neither graph projection nor
  lineage acknowledgement.
- Local verification for this outbox table commit evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_missing_table_commit_evidence_before_projection -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_mismatched_table_commit_identity_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_short_table_commit_hash_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox table lifecycle evidence`.
  Outbox draining now rejects `table.created`, `table.loaded`,
  `table.deleted`, and `table.restored` pending events when the root table
  identity is missing, payload scope hints contradict that identity, or
  soft-delete evidence points at a different table. Unsafe table lifecycle
  replay evidence stays pending and reaches neither graph projection nor
  lineage acknowledgement.
- Local verification for this outbox table lifecycle evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_missing_table_lifecycle_identity -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_mismatched_table_soft_delete_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_restores_to_graph_and_lineage -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox view evidence`.
  Outbox draining now rejects `view.listed`, `view.upserted`, `view.loaded`,
  and `view.dropped` pending events when view evidence has malformed warehouse,
  namespace, view name, or view-count fields. Unsafe view replay evidence stays
  pending and reaches neither graph projection nor lineage acknowledgement.
- Local verification for this outbox view evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_view_list_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_view_lifecycle_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox management-list evidence`.
  Outbox draining now rejects `policy-binding.listed`, `project.listed`,
  `server.listed`, `storage-profile.listed`, and `warehouse.listed` pending
  events when list evidence has malformed counts or optional scope fields.
  Unsafe management-list replay evidence stays pending and reaches neither
  graph projection nor lineage acknowledgement.
- Local verification for this outbox management-list evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_management_list_count_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_management_list_scope_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_management_list_reads_to_lineage -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox catalog read evidence`.
  Outbox draining now rejects `catalog.config-read` and `namespace.listed`
  pending events when read evidence has a malformed warehouse or namespace
  list count. Unsafe catalog-read replay evidence stays pending and reaches
  neither graph projection nor lineage acknowledgement.
- Local verification for this outbox catalog-read evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_catalog_read_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_namespace_list_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_catalog_config_reads_to_graph_and_lineage -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_namespace_reads_to_graph_and_lineage -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox namespace evidence`.
  Outbox draining now rejects `namespace.created`, `namespace.loaded`, and
  `namespace.dropped` pending events when lifecycle evidence has a malformed
  warehouse or namespace. Unsafe namespace replay evidence stays pending and
  reaches neither graph projection nor lineage acknowledgement.
- Local verification for this outbox namespace evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_namespace_lifecycle_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_namespace_reads_to_graph_and_lineage -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture --test-threads=1`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox project evidence`.
  Outbox draining now rejects `project.upserted` pending events when project
  evidence has mismatched project ids, malformed optional server scope, invalid
  public properties, or malformed identifiers. Unsafe project replay evidence
  stays pending and reaches neither graph projection nor lineage
  acknowledgement.
- Local verification for this outbox project evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_project_upsert_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_project_upserts_to_graph -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox tenant roots`.
  Outbox draining now rejects `server.upserted` and `warehouse.upserted`
  pending events when tenant-root evidence has malformed endpoint URLs,
  storage roots, identifiers, properties, or redacted hash anchors. Unsafe
  server/warehouse replay evidence stays pending and reaches neither graph
  projection nor lineage acknowledgement.
- Local verification for this outbox tenant-root evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_server_upsert_endpoint_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_warehouse_upsert_storage_root_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_server_upserts_to_lineage -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_warehouse_upserts_to_graph -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox policy-binding evidence`.
  Outbox draining now rejects `policy-binding.upserted` pending events when
  policy-binding evidence has malformed identifiers, warehouse scope,
  namespace/table scope, or missing enforcement/ODRL fields. Unsafe
  policy-binding replay evidence stays pending and reaches neither graph
  projection nor lineage acknowledgement.
- Local verification for this outbox policy-binding evidence slice is green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_policy_binding_upsert_evidence -- --nocapture`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject raw outbox storage-profile secrets`.
  Outbox draining now rejects `storage-profile.upserted` pending events when
  the storage-profile payload carries a raw `secret-ref`, has malformed
  secret-reference provider/hash state, or lacks both a raw location prefix and
  a full redacted `location-prefix-hash`. Unsafe storage-profile replay
  evidence stays pending and reaches neither graph projection nor lineage
  acknowledgement.
- Local verification for this outbox storage-profile evidence slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_raw_storage_profile_secret_ref_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox credential evidence`.
  Outbox draining now rejects `credentials.vend-attempted` pending events when
  `credential-count` does not match `credential-response-evidence`,
  credential prefix or issuer-config evidence is not full `sha256:`-prefixed
  64-hex digest evidence, storage-profile `location-prefix-hash` is malformed,
  or secret-reference provider/hash state contradicts `secret-ref-present`.
  Invalid credential replay evidence stays pending and reaches neither graph
  projection nor lineage acknowledgement.
- Local verification for this outbox credential-evidence slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_credential_vend_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox commit-history evidence`.
  Outbox draining now rejects `table.commits-listed` pending events when
  `commit-count` does not match `commit-hashes` or `sequence-numbers`, commit
  hashes are not full `sha256:`-prefixed 64-hex digests, or sequence numbers
  are not unsigned integers. Invalid pointer-log replay evidence stays pending
  and reaches neither graph projection nor lineage acknowledgement.
- Local verification for this outbox commit-history slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_table_commit_history_evidence -- --nocapture`;
  `cargo test -p lakecat-service management_table_commits_lists_pointer_log_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox view receipt lists`.
  Outbox draining now rejects `view.version-receipts-listed` pending events
  when `receipt-count` does not match `receipt-hashes`, receipt arrays are not
  full `sha256:`-prefixed 64-hex digests, or `drop-receipt-hashes` are not a
  subset of the listed receipts. Invalid view receipt-list evidence stays
  pending and reaches neither graph projection nor lineage acknowledgement.
- Local verification for this outbox view receipt-list slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_view_receipt_list_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Reject malformed outbox view receipt chains`.
  Outbox draining now rejects verified `view.version-receipt-chains-listed`
  pending events when chain hashes, receipt hashes, verified-chain counts,
  first receipt shape, previous links, or upsert/drop version transitions are
  malformed. Invalid view receipt-chain evidence stays pending and reaches
  neither graph projection nor lineage acknowledgement.
- Local verification for this outbox view receipt-chain slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_malformed_view_receipt_chain_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran LakeCat replay, QueryGraph verify/import, and
  ended with `QGLake handoff verified`).
- Latest completed implementation slice:
  `Require full QGLake commit record hashes`.
  QGLake table commit-history verification now rejects compact pointer-log
  records whose request, response, idempotency-key, commit, or optional policy
  hash evidence is not a full `sha256:`-prefixed 64-hex digest, closing the
  readable-placeholder path before commit-history evidence feeds QGLake
  handoff acceptance.
- Local verification for this QGLake commit record hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_commit_history_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, and ended with `QGLake handoff verified`);
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject short outbox commit hashes`.
  Outbox draining now rejects `table.commit` pending events whose
  `request_hash`, `response_hash`, `idempotency_key_sha256`, or present
  `policy_hash` evidence is not a full `sha256:`-prefixed 64-hex digest before
  graph/lineage projection or delivery acknowledgement, keeping malformed REST
  commit receipt evidence out of delivered replay.
- Local verification for this outbox commit hash slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_short_table_commit_hash_evidence -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, and ended with `QGLake handoff verified`);
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject short outbox policy hashes`.
  Outbox draining now rejects pending events whose
  `read-restriction.policy-hashes` are not full `sha256:`-prefixed 64-hex
  digests before graph/lineage projection or delivery acknowledgement, keeping
  malformed governed-read source evidence pending instead of letting it reach
  QGLake replay as delivered catalog evidence.
- Local verification for this outbox policy hash slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_short_read_restriction_policy_hashes -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain -- --nocapture`;
  `cargo test -p lakecat-service --features turso-local outbox_drain -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, and ended with `QGLake handoff verified`);
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full governed policy hashes`.
  QGLake source replay and compact handoff verification now reject governed
  scan read-restriction `policy-hashes` unless each value is a full
  `sha256:`-prefixed 64-hex digest, closing the short placeholder policy-anchor
  path before replay evidence feeds QueryGraph handoff acceptance.
- Local verification for this governed policy hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_scan_policy_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_rejects_short_scan_policy_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, and ended with `QGLake handoff verified`);
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full source secret-ref hashes and covered view chains`.
  QGLake lineage-drain source replay now rejects storage-profile upsert and
  credential-vending `secretRefHash` evidence unless each present hash is a
  full `sha256:`-prefixed 64-hex digest before compact handoff proof
  generation. Generated QGLake view replay evidence now also carries accepted
  view receipt-chain hashes in namespace `receiptChains[].chainHashes`, so the
  live handoff summary covers every `acceptedReceiptChainHash` it later
  verifies.
- Local verification for this source secret-ref and receipt-chain coverage
  slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_storage_profile_upsert_replay_rejects_short_location_prefix_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_credential_replay_line_summarizes_verified_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, and ended with `QGLake handoff verified`);
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full handoff artifact hashes`.
  QGLake handoff artifact verification now rejects bundle, lineage-drain,
  QueryGraph import-plan, captured-output, service-log, and optional
  self-verifier output hash declarations unless each present artifact integrity
  anchor is a full `sha256:`-prefixed 64-hex digest before file content
  comparison.
- Local verification for this handoff artifact hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_short_artifact_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_short_service_log_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_short_handoff_verify_output_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact secret-ref hashes`.
  Compact QGLake handoff verification now rejects storage-profile upsert and
  credential-vending `secretRefHash` proof anchors unless every present hash is
  a full `sha256:`-prefixed 64-hex digest, closing short placeholder
  credential-root evidence while preserving the redacted provider/hash-only
  secret-reference boundary.
- Local verification for this compact secret-ref hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_secret_ref_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_credential_secret_ref_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact TypeDID hashes`.
  Compact QGLake handoff verification now rejects request-identity and
  QueryGraph bootstrap TypeDID envelope/proof hash slots unless every present
  hash is a full `sha256:`-prefixed 64-hex digest, closing short placeholder
  TypeDID proof anchors in saved handoff summaries while keeping TypeSec
  responsible for TypeDID trust semantics.
- Local verification for this compact TypeDID hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_typedid_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact authorization hashes`.
  Compact QGLake handoff verification now rejects request-identity
  authorization, bootstrap authorization, agent delegation, and agent summary
  signature hashes unless every required proof anchor is a full
  `sha256:`-prefixed 64-hex digest, closing short placeholder identity proof
  anchors in saved handoff summaries.
- Local verification for this compact authorization hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_authorization_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact view hashes`.
  Compact QGLake handoff verification now rejects view receipt-chain proof
  accepted-view receipt hashes, accepted chain hashes, tombstone receipts,
  namespace receipt/chain hashes, and replay/OpenLineage arrays unless every
  hash is a full `sha256:`-prefixed 64-hex digest, closing short placeholder
  evidence paths for saved view acceptance summaries.
- Local verification for this compact view hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_view_receipt_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact credential hashes`.
  Compact QGLake handoff verification now rejects
  `credentialVendingProof` restricted-agent and trusted-human replay and
  OpenLineage arrays unless every entry is a full `sha256:`-prefixed 64-hex
  digest, closing the short placeholder hash path for credential-vending
  receipt anchors in saved handoff summaries.
- Local verification for this compact credential hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_credential_replay_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact storage-profile hashes`.
  Compact QGLake handoff verification now rejects
  `storageProfileUpsertProof` replay and OpenLineage arrays unless every entry
  is a full `sha256:`-prefixed 64-hex digest, closing the short placeholder
  hash path for credential-root replay anchors in saved handoff summaries.
- Local verification for this compact storage-profile hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_storage_profile_replay_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact bootstrap hashes`.
  Compact QGLake handoff verification now rejects QueryGraph bundle, graph,
  OpenLineage, import, bootstrap replay, and bootstrap OpenLineage anchors
  unless they are full `sha256:`-prefixed 64-hex digests, closing short
  placeholder hash paths in saved QueryGraph bootstrap/import handoff
  summaries.
- Local verification for this compact bootstrap hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_bootstrap_replay_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact commit-history hashes`.
  Compact QGLake handoff verification now rejects table commit-history
  `commitHashes`, `replayEventHashes`, and `openLineageHashes` unless every
  entry is a full `sha256:`-prefixed 64-hex digest, closing the short
  placeholder hash path for pointer-history receipts in saved handoff
  summaries.
- Local verification for this compact commit-history hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_commit_history_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact management hashes`.
  Compact QGLake handoff verification now rejects management proof
  server/project/warehouse/policy/storage-profile replay and OpenLineage arrays
  unless every entry is a full `sha256:`-prefixed 64-hex digest, closing the
  short placeholder hash path for control-plane read receipts in saved handoff
  summaries.
- Local verification for this compact management hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_management_receipt_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full compact governed scan hashes`.
  Compact QGLake handoff verification now rejects `governedScanProof`
  planned/fetched replay and OpenLineage arrays unless every entry is a full
  `sha256:`-prefixed 64-hex digest, closing the remaining short placeholder
  scan receipt path in saved handoff summaries.
- Local verification for this compact governed scan hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_short_scan_replay_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`.
- Latest completed implementation slice:
  `Require full governed scan replay hashes`.
  QGLake lineage-drain verification now rejects governed scan replay and
  OpenLineage receipt arrays unless they contain full `sha256:`-prefixed
  64-hex digests, closing the short placeholder hash path for scan planning and
  scan-task fetch evidence.
- Local verification for this governed scan replay hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_rejects_short_scan_receipt_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain -- --nocapture`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_replay_line_summarizes_verified_evidence -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Preserve full view receipt replay coverage`.
  Lineage-drain summaries for view receipt-list and namespace receipt-chain
  reads now preserve full receipt hash coverage, including nested receipts from
  `view-version-receipt-chains`, so QGLake replay can prove namespace chains
  cover both upsert and tombstone receipts.
- Local verification for this view receipt replay coverage slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain -- --nocapture`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind tombstoned view chains in compact QGLake handoff`.
  Compact QGLake handoff verification now rejects tombstoned accepted views
  whose `acceptedReceiptChainHash` is not covered by namespace
  `receiptChains[].chainHashes`, so deletion evidence cannot stand apart from
  the accepted view receipt-chain proof.
- Local verification for this tombstoned view-chain binding slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_tombstoned_uncovered_view_receipt_chain_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin service commit hash producer evidence`.
  Service table commit-history coverage now explicitly proves produced
  request, response, idempotency-key, and commit hashes are full SHA-256
  digests across the management route, pointer-log outbox payload, lineage-drain
  summary, and graph projection that QGLake consumes.
- Local verification for this service commit-hash producer slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service management_table_commits_lists_pointer_log_evidence -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin service view receipt hash producer evidence`.
  Service view workflow coverage now explicitly proves produced
  `receipt-hash`, `view-hash`, and namespace `chain-hash` values are full
  SHA-256 digests while preserving the positive receipt-chain structure that
  QGLake and QueryGraph consume.
- Local verification for this service view receipt producer slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin service storage-scope hash producer evidence`.
  Service-side storage-profile upsert replay and credential-vend audit payload
  coverage now explicitly proves produced `location-prefix-hash` values are
  full SHA-256 digests before the QGLake verifier consumes the corresponding
  `locationPrefixHash` proof.
- Local verification for this service storage-scope producer slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage -- --nocapture`;
  `cargo test -p lakecat-service credentials_vend_audit_payload_surfaces_policy_context -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full QGLake storage-scope hashes`.
  QGLake compact handoff summaries, management replay lines, credential replay
  lines, and storage-profile upsert lineage replay now require
  `locationPrefixHash` storage-scope evidence to be full `sha256:`-prefixed
  64-hex digests instead of placeholder hash labels.
- Local verification for this QGLake storage-scope hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_storage_profile_location_hash_shape -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_credential_location_prefix_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_storage_profile_upsert_replay_rejects_short_location_prefix_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin QueryGraph tenant hash producer evidence`.
  QueryGraph tenant projection and the service bootstrap route now have focused
  coverage proving durable tenant roots are emitted as full SHA-256 hash
  evidence, with raw server endpoint URLs and warehouse storage roots absent
  from the produced graph.
- Local verification for this QueryGraph tenant hash producer slice is green:
  `cargo fmt -p lakecat-querygraph -p lakecat-service -- --check`;
  `cargo test -p lakecat-querygraph tenant_records_project_full_hash_evidence_without_raw_roots -- --nocapture`;
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog_tables -- --nocapture`;
  `cargo test -p lakecat-cli qglake_bootstrap_verifier_rejects_short -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require full tenant-root hashes in QGLake bootstrap verification`.
  QGLake bootstrap verification now rejects tenant `Server.endpointUrlHash` and
  `Warehouse.storageRootHash` values unless they are full `sha256:`-prefixed
  64-hex digests, so QueryGraph acceptance cannot be satisfied by placeholder
  hash labels after a bundle hash resync.
- Local verification for this tenant-root hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_bootstrap_verifier_rejects_short -- --nocapture`;
  `cargo test -p lakecat-cli qglake_bootstrap_verifier_rejects_raw -- --nocapture`;
  `cargo test -p lakecat-cli qglake_bootstrap_verifier_requires_graph_tenant_spine -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require HTTP server endpoint URLs`.
  Server endpoint URLs now must parse as absolute `http` or `https` URLs before
  memory or Turso persistence. Invalid strings and non-HTTP schemes fail with
  `server-endpoint-url-hash` evidence only, matching the existing decorated
  endpoint rejection and keeping tenant management roots clean before
  QueryGraph handoff.
- Local verification for this server endpoint scheme slice is green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-store --features turso-local endpoint_urls -- --nocapture`;
  `cargo test -p lakecat-service management_server_rejects_invalid_endpoint_urls -- --nocapture`;
  `cargo test -p lakecat-service management_server_rejects_decorated_endpoint_urls -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject raw tenant roots in QGLake bootstrap verification`.
  QGLake bootstrap verification now rejects otherwise self-consistent bundles
  whose tenant `Server` or `Warehouse` graph nodes expose raw `endpointUrl` or
  `storageRoot` values, and checks any present `endpointUrlHash` or
  `storageRootHash` fields are shaped as SHA-256 evidence.
- Local verification for this QGLake bootstrap verifier slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_bootstrap_verifier_rejects_raw -- --nocapture`;
  `cargo test -p lakecat-cli qglake_bootstrap_verifier_requires_graph_tenant_spine -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact QueryGraph tenant graph roots`.
  QueryGraph bootstrap tenant graph nodes now emit `endpointUrlHash` and
  `storageRootHash` instead of raw server endpoint URLs or warehouse storage
  roots, preserving tenant IDs, display names, and spine edges while keeping
  handoff graph artifacts hash-only for operator-managed roots.
- Local verification for this QueryGraph tenant graph slice is green:
  `cargo fmt -p lakecat-querygraph -p lakecat-service -- --check`;
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog_tables -- --nocapture`;
  `cargo test -p lakecat-cli qglake_bootstrap_verifier_requires_graph_tenant_spine -- --nocapture`;
  `scripts/check-local-dependency-contract.sh`;
  `scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject decorated server endpoint URLs`.
  Server endpoint URLs now reject query strings, fragments, and URI userinfo
  before memory or Turso persistence. `server.upserted` replay also redacts
  legacy/imported endpoint URLs before graph or lineage projection, replacing
  them with hash-only endpoint evidence.
- Local verification for this server endpoint hardening slice is green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-store --features turso-local endpoint_urls -- --nocapture`;
  `cargo test -p lakecat-service server_upserts_to_lineage -- --nocapture`;
  `cargo test -p lakecat-service management_server_rejects_decorated_endpoint_urls -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Refresh live QGLake handoff after warehouse-root hardening`.
  The local QGLake handoff harness is green after warehouse-root replay
  redaction and validation hardening: it generated one table and one view,
  drained 26 replay events, verified saved LakeCat replay, ran QueryGraph
  verify/import, and self-verified the compact handoff summary while preserving
  hash-only management and storage-profile evidence.
- Local verification for this live handoff refresh is green:
  `scripts/qglake-handoff-local.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject unsafe warehouse storage roots`.
  Warehouse storage roots now reject query strings, fragments, URI userinfo, and
  literal or percent-encoded dot path segments before memory or Turso
  persistence, returning only `warehouse-storage-root-hash` evidence for the
  submitted root.
- Local verification for this warehouse-root validation slice is green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_roots -- --nocapture`;
  `cargo test -p lakecat-service storage_roots -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact config-read warehouse roots`.
  Catalog config-read replay now applies the same warehouse-record redaction as
  warehouse upserts, so any attached `storage-root` is replaced with
  `storage-root-hash` before graph or lineage projection.
- Local verification for this config-read redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_catalog_config_reads_to_graph_and_lineage -- --nocapture`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact warehouse replay storage roots`.
  Warehouse upsert replay now strips raw `storage-root` values before graph and
  lineage projection and replaces them with `storage-root-hash` evidence,
  keeping tenant roots replayable without exposing local paths or bucket roots
  downstream.
- Local verification for this warehouse replay redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_warehouse_upserts_to_graph -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject block-list workflow triggers`.
  The local dependency-contract audit now also rejects YAML block-list trigger
  syntax under top-level `on:`, such as `on:\n  - push`, so manual-only cloud
  CI cannot be bypassed through GitHub Actions list syntax.
- Local verification for this block-list workflow-contract slice is green:
  `bash -n scripts/check-local-dependency-contract.sh`;
  `scripts/check-local-dependency-contract.sh`;
  block-list trigger smoke checks against temporary workflow files;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject compact automatic workflow triggers`.
  The local dependency-contract audit now rejects compact GitHub Actions
  trigger forms such as `on: push`, inline event lists, and inline event maps,
  preserving the manual-only cloud CI policy even if a future workflow avoids
  mapping-style trigger blocks.
- Local verification for this workflow-contract slice is green:
  `bash -n scripts/check-local-dependency-contract.sh`;
  `scripts/check-local-dependency-contract.sh`;
  compact-trigger smoke checks against temporary workflow files;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject decorated resolver secret refs`.
  Credential resolver provider detection, Vault path construction, and
  TypeSec environment secret resolution now fail closed on secret refs with
  query strings, fragments, or URI userinfo, returning only `secret-ref-hash`
  evidence even if a legacy/imported profile bypasses storage-profile
  constructor validation.
- Local verification for this resolver secret-ref slice is green:
  `cargo test -p lakecat-service --features typesec-local environment_secret_resolver_parses_supported_secret_shapes -- --nocapture`.
- Latest completed implementation slice:
  `Refresh live QGLake handoff verification`.
  The local QGLake handoff harness is green after the tombstone receipt-chain
  binding change: it generated one table and one view, drained 26 replay events,
  verified saved LakeCat replay, ran QueryGraph verify/import, and self-verified
  the compact handoff summary with tombstone receipt hashes covered by namespace
  receipt-chain evidence.
- Local verification for this live handoff refresh is green:
  `scripts/qglake-handoff-local.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind QGLake tombstones to receipt chains`.
  QGLake live replay and compact handoff verification now reject dropped-view
  proofs unless the tombstone receipt hashes are covered by the namespace
  receipt-chain read, tightening QueryGraph view tombstone evidence without
  adding non-standard Iceberg access paths.
- Local verification for this QGLake tombstone receipt-chain slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_uncovered_view_tombstone_receipts -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin stale view guard replay boundary`.
  Route-level coverage now proves stale guarded view upserts and drops fail
  without emitting new replay outbox events or extending view-version receipt
  evidence, preserving QueryGraph receipt-chain semantics at the catalog
  boundary.
- Local verification for this stale view guard replay slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service stale_view_mutation_guards_do_not_emit_replay_events -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin planned projection drain summary`.
  Service replay-summary coverage now asserts `table.scan-planned` outbox drain
  summaries preserve requested/effective projection and statistics-field
  evidence, keeping planned scan replay aligned with the QGLake handoff proof
  contract.
- Local verification for this planned projection drain-summary slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service scan_planned_drain_summary_preserves_projection_evidence -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin fetch projection drain summary`.
  Service outbox-drain coverage now asserts `table.scan-tasks-fetched` replay
  summaries preserve fetched `effective_projection`, keeping the source
  lineage summary aligned with the stricter QGLake replay and handoff checks.
- Local verification for this fetched projection drain-summary slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require live handoff projection proof`.
  The local QGLake handoff harness now requires the same governed-scan
  planned/fetched projection evidence as the CLI verifier before it writes the
  compact `handoff-summary.json`, including fetched `effectiveProjection`
  evidence matched to the fetched read restriction.
- Local verification for this live handoff projection-proof slice is green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_fetch_effective_projection_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 lineage/outbox events, ran QueryGraph verify/import, and self-verified
  `handoff-summary.json` with fetched effective projection evidence);
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require QGLake fetch effective projection proof`.
  QGLake CLI replay and compact handoff verification now reject missing or
  drifted fetched `effective-projection` evidence, tying fetch replay to the
  same server-derived read restriction emitted by `fetchScanTasks`.
- Local verification for this QGLake fetch proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Expose fetch effective projection evidence`.
  `fetchScanTasks` responses and `table.scan-tasks-fetched` audit/outbox
  payloads now carry `effective-projection` alongside the required projection
  and filters, so replay can use the same server-derived projection vocabulary
  as scan planning without inventing a fetch-time client projection.
- Local verification for this fetch projection-evidence slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service scan_tasks_fetched_audit_payload_surfaces_policy_context -- --nocapture`;
  `cargo test -p lakecat-service fetch_scan_tasks_route_sends_required_policy_scope_to_sail -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact custom TypeDID verifier errors`.
  The live request-identity path now wraps every configured TypeDID verifier
  failure before HTTP response or governance dispatch, preserving the original
  error class while exposing only `typedid-envelope-hash` and
  `error-detail-hash` evidence.
- Local verification for this TypeDID verifier boundary slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service config_endpoint_redacts_custom_typedid_verifier_errors_before_governance -- --nocapture`;
  `cargo test -p lakecat-service config_endpoint_redacts_typedid_subject_mismatch_before_governance -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact TypeDID verifier failures`.
  Live TypeDID envelope verification now reports malformed/rejected envelopes
  with `typedid-envelope-hash` plus `error-detail-hash`, and subject mismatch
  failures expose only verified/supplied principal hashes before governance
  dispatch.
- Local verification for this TypeDID verifier redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service config_endpoint_redacts_typedid_subject_mismatch_before_governance -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local typesec_typedid_envelope_verification_updates_authorization_context -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact unsupported credential provider schemes`.
  TypeSec credential resolver provider detection now rejects unsupported
  secret-reference schemes with only `secret-ref-hash` evidence, keeping both
  the raw secret ref and scheme/path fragments out of operator-facing errors.
- Local verification for this credential provider redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features typesec-local environment_secret_resolver_parses_supported_secret_shapes -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact credential resolver failure details`.
  Configured TypeSec environment and Vault credential resolvers now report
  lookup and secret payload parse failures with `secret-ref-hash` plus
  `error-detail-hash` evidence instead of echoing environment variable names,
  Vault paths, tokens, namespaces, or backend error text.
- Local verification for this credential resolver redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_redacts_vault_backend_failures -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_redacts_environment_backend_failures -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Hash metadata cleanup conflict details`.
  Metadata cleanup failures appended to preserved commit conflicts now expose
  only `error-detail-hash` evidence, so a cleanup path cannot leak raw backend
  text while preserving the original commit error class.
- Local verification for this metadata cleanup conflict slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_cleanup_failure_preserves_commit_conflict -- --nocapture`;
  `cargo test -p lakecat-service metadata_cleanup_error_redacts_metadata_location -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Hash malformed outbox decode diagnostics`.
  Malformed outbox table/principal JSON decode failures now include outbox
  event-hash evidence without echoing raw event IDs, and focused drain
  regressions prove both corrupt table identity and corrupt principal identity
  records fail before acknowledgement.
- Local verification for this outbox decode diagnostic slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_hashes_malformed_table_decode_errors -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_hashes_malformed_principal_decode_errors -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_redacts_corrupt_pending_event_ids -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact corrupt outbox event ids`.
  Outbox projection helper diagnostics now report malformed/corrupt pending
  records with `outbox event hash sha256:...` instead of echoing raw event IDs.
  Regression coverage proves a corrupt namespace event fails before graph,
  lineage, or acknowledgement side effects.
- Local verification for this outbox diagnostic slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_redacts_corrupt_pending_event_ids -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_duplicate_pending_event_ids_before_projection -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject non-agent proof headers`.
  Live request-identity parsing now rejects agent delegation and agent summary
  proof headers unless the request uses an agent-shaped identity path. The
  rejection returns only `agent-delegation-hash` or
  `agent-summary-signature-hash` evidence, and config-route coverage proves the
  request fails before governance dispatch.
- Local verification for this agent request-boundary slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service request_identity_rejects_agent_proof_headers_without_agent_identity -- --nocapture`;
  `cargo test -p lakecat-service config_endpoint_rejects_agent_summary_without_agent_before_governance -- --nocapture`;
  `cargo test -p lakecat-service request_identity_hashes_typedid_envelope_material -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject unpaired TypeDID proof headers`.
  Live request-identity parsing now rejects `x-lakecat-typedid-proof` unless
  `x-lakecat-typedid-envelope` is present, returns only
  `typedid-proof-hash` evidence, and config-route coverage proves the request
  fails before governance dispatch.
- Local verification for this TypeDID request-boundary slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service request_identity_rejects_unpaired_typedid_proof -- --nocapture`;
  `cargo test -p lakecat-service config_endpoint_rejects_unpaired_typedid_proof_before_governance -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin scan replay projection proof`.
  Scan-planned audit/outbox payloads and drain summaries now preserve
  requested/effective projection evidence, and QGLake source replay plus compact
  handoff verification reject missing, widened, or unrequested effective
  projection proof.
- Local verification for this scan replay projection-proof slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service scan_planned_audit_payload_surfaces_policy_context -- --nocapture`;
  `cargo test -p lakecat-service scan_planning_route_sends_effective_policy_scope_to_sail -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_replay_rejects_missing_projection_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_replay_rejects_widened_effective_projection -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_replay_rejects_unrequested_effective_projection -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_scan_projection_widening -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_unrequested_effective_scan_projection -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_replay_line_summarizes_verified_evidence -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Order outbox drain batches before projection`.
  Outbox drains now defensively order pending batches by `created_at,event_id`
  before graph/lineage projection, response summarization, and delivery
  acknowledgement, so QueryGraph/OpenLineage replay stays deterministic even if
  a custom store returns an unsorted batch.
- Local verification for this outbox determinism slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_orders_pending_batch_before_projection -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_duplicate_pending_event_ids_before_projection -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_partial_acknowledgement -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin storage-profile issuance mismatch redaction`.
  Storage-profile issuance/provider mismatch errors now carry
  `storage-profile-prefix-hash` evidence, and management-route coverage proves
  remote roots cannot use local no-secret mode and local roots cannot use
  short-lived secret-ref mode without echoing raw prefixes or secret refs.
- Local verification for this storage-profile credential-mode slice is green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_provider_issuance_mismatch -- --nocapture`;
  `cargo test -p lakecat-service management_storage_profile_rejects_remote_local_no_secret_mode -- --nocapture`;
  `cargo test -p lakecat-service management_storage_profile_rejects_local_secret_ref_mode -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Route-prove decorated storage prefixes are redacted`.
  The management storage-profile route now has focused regression coverage
  proving decorated `location-prefix` values fail with
  `storage-profile-prefix-hash` evidence and do not echo the raw prefix, query
  token, or embedded userinfo.
- Local verification for this management-route storage-profile slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service management_storage_profile_rejects_decorated_location_prefixes -- --nocapture`;
  `cargo test -p lakecat-service management_storage_profile_rejects_provider_prefix_mismatch -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject decorated storage profile prefixes`.
  Storage-profile validation now rejects location prefixes with query strings,
  fragments, or URI userinfo before memory or Turso persistence and returns only
  `storage-profile-prefix-hash` evidence, so storage roots remain plain
  catalog-controlled boundaries.
- Local verification for this storage-profile slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_decorated_location_prefixes -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profile_upsert_rejects_deserialized_decorated_location_prefixes -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin Turso idempotency mismatch redaction`.
  Turso store coverage now proves reused-key commit conflicts and explicit
  replay probes return the generic idempotency mismatch conflict without
  echoing the raw idempotency key, mismatched request hash, or mismatched
  metadata object location.
- Local verification for this Turso idempotency slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local turso_store_round_trips_namespaces_tables_and_idempotent_commits -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Route-prove malformed JSON-LD ODRL blocks credentials`.
  The REST credential-vending route now has focused regression coverage proving
  a malformed JSON-LD ODRL allowed-column `@list` fails before credential issuer
  dispatch and before `credentials.vend-attempted` replay evidence is emitted.
- Local verification for this route-level ODRL credential slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service credential_vend_rejects_malformed_jsonld_odrl_before_issuer -- --nocapture`;
  `cargo test -p lakecat-service credential_vend_rejects_malformed_odrl_before_issuer -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Route-prove malformed JSON-LD ODRL blocks fetch`.
  The REST `fetchScanTasks` route now has focused regression coverage proving a
  malformed JSON-LD ODRL allowed-column `@list` fails before Sail fetch
  execution and before `table.scan-tasks-fetched` replay evidence is emitted.
- Local verification for this route-level ODRL fetch slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service fetch_scan_tasks_rejects_malformed_jsonld_odrl_before_sail -- --nocapture`;
  `cargo test -p lakecat-service fetch_scan_tasks_rejects_malformed_odrl_before_sail -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Route-prove malformed JSON-LD ODRL blocks scan`.
  The REST scan-planning route now has focused regression coverage proving a
  malformed JSON-LD ODRL allowed-column `@list` fails before Sail planning and
  before `table.scan-planned` replay evidence is emitted.
- Local verification for this route-level ODRL slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service scan_planning_rejects_malformed_jsonld_odrl_before_sail -- --nocapture`;
  `cargo test -p lakecat-service scan_planning_rejects_malformed_odrl_before_sail -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Accept JSON-LD ODRL right operand values`.
  `lakecat-security` now accepts compact JSON-LD `@value` and `@list` right
  operands for the bounded allowed-column, purpose, and credential-TTL ODRL
  constraint subset, including `lakecat:purpose` and `lakecat:credential-ttl`
  operand aliases. Malformed JSON-LD allowed-column lists still fail closed.
- Local verification for this ODRL compatibility slice is green:
  `cargo fmt -p lakecat-security -- --check`;
  `cargo test -p lakecat-security read_restriction_accepts_jsonld_value_objects_for_right_operands -- --nocapture`;
  `cargo test -p lakecat-security read_restriction_rejects_malformed_jsonld_allowed_column_lists -- --nocapture`;
  `cargo test -p lakecat-security read_restriction -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject duplicate pending outbox ids`.
  Outbox drains now validate the pending batch before projection and fail on
  duplicate event IDs with only a duplicate event-id hash. The regression proves
  graph projection, lineage projection, and delivery acknowledgement are all
  untouched when a corrupted or custom store returns duplicate pending IDs.
- Local verification for this outbox hardening slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_duplicate_pending_event_ids_before_projection -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_rejects_partial_acknowledgement -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Scan all workflows for automatic triggers`.
  The local dependency contract now scans every GitHub workflow file, including
  future `.yml` and `.yaml` additions, for forbidden automatic cloud triggers
  while LakeCat keeps CI manual-only and relies on local proof before pushes.
- Local verification for this reproducibility slice is green:
  `bash -n scripts/check-local-dependency-contract.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin view receipt-chain head invariants`.
  View receipt-chain verifier coverage now directly proves a chain must begin
  with a version-1 upsert that has no previous version or previous receipt hash.
  Zero-version chains, first-receipt tombstones, and first receipts with forged
  previous-link fields fail the compact QueryGraph/QGLake chain check.
- Local verification for this view receipt-chain head-invariant slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service view_receipt_chain_verifier_requires_version_transitions -- --nocapture`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin forged view receipt-chain rejection`.
  View receipt-chain verifier coverage now directly proves forged
  `previous-receipt-hash` links and unsupported operations fail the compact
  QueryGraph/QGLake chain check, alongside the existing version-transition and
  tombstone checks.
- Local verification for this view receipt-chain verifier slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service view_receipt_chain_verifier_requires_version_transitions -- --nocapture`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject zero expected view versions`.
  View mutation routes now have focused regression coverage proving
  `expected-view-version=0` is rejected before LakeCat updates the active view
  or appends any view-version receipt. The active view remains at version 1 and
  the receipt chain remains a single version-1 upsert after invalid guarded
  update and drop attempts.
- Local verification for this view guard slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service view_mutations_reject_zero_expected_version_without_receipts -- --nocapture`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Route-prove malformed ODRL blocks fetchScanTasks`.
  The default REST `fetchScanTasks` route now has focused regression coverage
  proving a malformed active ODRL read restriction fails before Sail fetch
  execution and before `table.scan-tasks-fetched` replay evidence is emitted.
  This closes the same route-level fail-closed loop already pinned for scan
  planning and credential vending.
- Local verification for this fetch-route ODRL fail-closed slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service fetch_scan_tasks_rejects_malformed_odrl_before_sail -- --nocapture`;
  `cargo test -p lakecat-service scan_planning_rejects_malformed_odrl_before_sail -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin metadata object-store setup redaction`.
  Metadata-object store setup now has direct regression coverage proving invalid
  metadata URI parsing and unsupported backend setup failures return only
  `metadata-location-hash` and `error-detail-hash` evidence. The error surface
  does not echo raw local paths, object names, schemes, or backend error text.
- Local verification for this metadata object-store setup redaction slice is
  green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_object_store_redacts_invalid_location_parse_failures -- --nocapture`;
  `cargo test -p lakecat-service metadata_object_store_redacts_unsupported_backend_setup_failures -- --nocapture`;
  `cargo test -p lakecat-service metadata_object_store_redacts -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin metadata root child-object guard`.
  Metadata-object commit validation now has direct regression coverage proving a
  planned metadata write cannot target the selected storage profile root itself.
  The rejection reports only `metadata-location-hash` and
  `storage-profile-prefix-hash` evidence, preserving the create-only child
  object invariant without echoing the raw local path or storage root.
- Local verification for this metadata-root guard slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_object_location_must_be_child_of_storage_profile_root -- --nocapture`;
  `cargo test -p lakecat-service metadata_cleanup -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Accept JSON-LD ODRL term objects`.
  `lakecat-security` now accepts compact JSON-LD `@id` term objects for
  bounded ODRL constraint `leftOperand` and `operator` values, including
  prefixed operand-key forms. This keeps common JSON-LD ODRL encodings on the
  same governed read-restriction path without moving broader ODRL reasoning into
  LakeCat.
- Local verification for this JSON-LD term-object slice is green:
  `cargo fmt -p lakecat-security -- --check`;
  `cargo test -p lakecat-security read_restriction_accepts_jsonld_term_objects_for_constraint_terms -- --nocapture`;
  `cargo test -p lakecat-security read_restriction_accepts_prefixed_odrl_constraint_operands -- --nocapture`;
  `cargo test -p lakecat-security read_restriction -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Accept prefixed ODRL constraint operands`.
  `lakecat-security` now treats prefixed JSON-LD ODRL constraint operand keys
  (`odrl:leftOperand`, `odrl:rightOperand`) as equivalent to the already
  supported camel/kebab forms for LakeCat's enforceable read-restriction subset.
  Prefixed operands still inherit the same fail-closed right-operand rule, so
  compatibility does not silently weaken governed reads.
- Local verification for this prefixed ODRL operand slice is green:
  `cargo fmt -p lakecat-security -- --check`;
  `cargo test -p lakecat-security read_restriction_accepts_prefixed_odrl_constraint_operands -- --nocapture`;
  `cargo test -p lakecat-security read_restriction_rejects_missing_odrl_constraint_right_operands -- --nocapture`;
  `cargo test -p lakecat-security read_restriction -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Route-prove missing ODRL right operands fail closed`.
  `lakecat-security` now rejects recognized ODRL read-restriction constraints
  for allowed columns, row predicates, purpose, and credential TTL when the
  constraint omits `rightOperand`/`right-operand`; the scan-planning route now
  proves malformed active policy fails before Sail planning, and the credential
  route proves it fails before issuer dispatch or credential-vend outbox
  emission.
- Local verification for this ODRL right-operand slice is green:
  `cargo fmt -p lakecat-security -p lakecat-service -- --check`;
  `cargo test -p lakecat-security read_restriction_rejects_missing_odrl_constraint_right_operands -- --nocapture`;
  `cargo test -p lakecat-security read_restriction -- --nocapture`;
  `cargo test -p lakecat-service scan_planning_rejects_malformed_odrl_before_sail -- --nocapture`;
  `cargo test -p lakecat-service credential_vend_rejects_malformed_odrl_before_issuer -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin metadata cleanup current-pointer skip`.
  Rejected-commit metadata cleanup now has direct regression coverage proving
  that a staged write equal to the previous committed metadata pointer is skipped
  and the current metadata object remains intact.
- Local verification for this metadata cleanup safety slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_cleanup_skips_previous_metadata_pointer -- --nocapture`;
  `cargo test -p lakecat-service metadata_cleanup_treats_missing_uncommitted_object_as_clean -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact storage-profile public-config validation`.
  Storage-profile public-config validation now returns
  `public-config-key-hash` evidence for secret-looking keys, rejected values,
  and reserved LakeCat credential-evidence keys without echoing submitted public
  config keys or values.
- Local verification for this public-config redaction slice is green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_public_config_secret_values -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profile_validate_rejects_public_config_secret_values -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_redact_secret_like_public_config_keys -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_reserved_public_config_keys -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profile_upsert_rejects_deserialized_public_config_secrets -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profile_upsert_rejects_reserved_public_config_keys -- --nocapture`;
  `cargo test -p lakecat-service management_storage_profile_rejects_public_secret_values -- --nocapture`;
  `cargo test -p lakecat-service management_storage_profile_rejects_reserved_public_config_keys -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact resolver secret-ref parse failures`.
  TypeSec-gated credential provider detection plus Vault and TypeSec
  environment resolver parsing now return `secret-ref-hash` evidence for
  malformed credential-root strings without echoing the submitted secret ref.
- Local verification for this resolver parse redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features typesec-local environment_secret_resolver_parses_supported_secret_shapes -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_production_secret_refs_before_dispatch -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact all storage-profile secret-ref validation roots`.
  Storage-profile secret-reference validation now returns `secret-ref-hash`
  evidence for invalid URI, decorated URI, and embedded-secret failures without
  echoing the submitted credential-root URI or token-like material.
- Local verification for this secret-ref redaction slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_decorated_secret_ref_uris -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_redact_invalid_secret_ref_uris -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_redact_embedded_secret_ref_material -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact storage-profile provider mismatch roots`.
  Storage-profile provider/location-prefix validation now reports provider
  labels and `storage-profile-prefix-hash` evidence without echoing the raw
  storage root when a configured provider conflicts with or cannot support the
  submitted location prefix.
- Local verification for this storage-profile redaction slice is green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_provider_location_mismatch -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_redact_unsupported_provider_location_prefixes -- --nocapture`;
  `cargo test -p lakecat-service management_storage_profile_rejects_provider_prefix_mismatch -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject partial outbox drain acknowledgements`.
  `drain_outbox_once` now treats graph/lineage projection followed by a short
  store acknowledgement as a conflict instead of returning a successful drain
  with a smaller delivered count. This keeps the lineage/graph outbox recovery
  contract visibly all-or-retry under concurrent or anomalous acknowledgement.
- Local verification for this outbox acknowledgement slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_rejects_partial_acknowledgement -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_does_not_acknowledge -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Compose ODRL TTL caps within policies`.
  `lakecat-security` now folds every supported ODRL credential-TTL source in a
  policy document to the tightest `max-credential-ttl-seconds` cap before
  composing active bindings, so direct fields cannot mask stricter constraint
  caps in the same policy.
- Local verification for this ODRL TTL slice is green:
  `cargo fmt -p lakecat-security -- --check`;
  `cargo test -p lakecat-security read_restriction -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject conflicting ODRL purposes`.
  `lakecat-security` now composes ODRL read-restriction purposes by agreement:
  top-level purpose fields, purpose constraints, and multiple active policy
  bindings must all name the same purpose or authorization fails closed before
  the restriction can reach Sail planning or credential decisions.
- Local verification for this ODRL purpose slice is green:
  `cargo fmt -p lakecat-security -- --check`;
  `cargo test -p lakecat-security read_restriction -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject ambiguous storage profile roots`.
  Storage-profile selection now fails closed if multiple profiles
  in one warehouse match a table with the same longest location prefix, so
  credential issuance and metadata-object validation cannot silently depend on
  profile iteration order. The error reports profile ids plus a
  `location-prefix-hash` without echoing the raw storage root.
- Local verification for this storage-profile selection slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profile_matching -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind saved handoff drain identity semantics`.
  Saved `lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics`
  sections are now checked against the compact `requestIdentityProof`, so a
  rehashed verifier artifact cannot drift the accepted drain principal,
  authorization receipt, request-identity source/state, or TypeDID hash slots.
- Local verification for this saved handoff semantics slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_lineage_identity_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_accepts_handoff_verify_output_hash -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Expose handoff drain identity semantics`.
  `lakecat-cli qglake-verify-handoff --json` now carries the
  lineage-drain request identity source/state and TypeDID envelope/proof hash
  slots in `lineageDrainArtifactSemantics`, so QueryGraph consumers can inspect
  the accepted drain identity boundary without reopening the raw drain artifact.
- Local verification for this handoff semantics slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_lineage_drain_artifact_semantics_accept_matching_drain -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_accepts_handoff_verify_output_hash -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reconcile live QGLake handoff replay proofs`.
  The full local QGLake handoff harness is green again. LakeCat now preserves
  failed `qglake-fixture --drain-output` artifacts before replay verification,
  suppresses restricted-agent `rawCredentialExceptionReason` in lineage-drain
  summaries while keeping the explicit block reason, treats request-identity
  and QueryGraph bootstrap authorization/TypeDID hashes as distinct replay
  receipts that are independently shaped and artifact-bound, and emits explicit
  `secretRefHash: null` proof in compact handoff summaries for no-secret
  storage profiles.
- Local verification for this live handoff reconciliation slice is green:
  `cargo fmt -p lakecat-cli -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_allows_distinct_bootstrap_authorization_receipt -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_allows_distinct_bootstrap_typedid -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind QGLake bootstrap TypeDID hashes`.
  Superseded by `Reconcile live QGLake handoff replay proofs`: live handoff
  replay proved request and bootstrap TypeDID hashes are independent
  request/event evidence slots, not equality-bound fields.
- Local verification for this handoff TypeDID-binding slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_bootstrap_typedid_envelope_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_bootstrap_typedid_proof_drift -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind QGLake bootstrap authorization receipt`.
  Superseded by `Reconcile live QGLake handoff replay proofs`: live handoff
  replay proved the request-identity authorization receipt is the lineage-drain
  read receipt while the bootstrap authorization receipt is the original
  bootstrap event receipt.
- Local verification for this handoff authorization-receipt slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_bootstrap_authorization_receipt_drift -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Block chained CI trigger classes`.
  The local dependency-contract audit now rejects additional automatic or
  chained GitHub Actions triggers: `pull_request_target`, `merge_group`,
  `repository_dispatch`, and `workflow_call`. Manual CI remains limited to
  direct `workflow_dispatch` while local gates are the proof source.
- Local verification for this reproducibility slice is green:
  `bash -n scripts/check-local-dependency-contract.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Block scheduled CI triggers in dependency contract`.
  The local dependency-contract audit now rejects `schedule` and `workflow_run`
  triggers in addition to push and pull-request triggers, keeping cloud CI
  genuinely manual-only until local gates are known green.
- Local verification for this reproducibility slice is green:
  `bash -n scripts/check-local-dependency-contract.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin REST idempotency outbox singularity`.
  REST commit idempotency coverage now proves exact replay and reused-key
  mismatch conflicts leave only the original `table.commit` outbox event,
  preventing retry paths from creating duplicate graph/lineage work.
- Local verification for this idempotency side-effect slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact REST idempotency mismatch conflicts`.
  REST commit idempotency coverage now proves a reused-key mismatch returns a
  conflict without echoing the raw `x-lakecat-idempotency-key` value or the
  mismatched metadata object location.
- Local verification for this idempotency redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin graph-failure outbox retryability`.
  Service outbox coverage now proves a graph projection failure makes
  `drain_outbox_once` fail before lineage emission and before delivery
  acknowledgement, leaving the durable outbox event pending for retry.
- Local verification for this outbox retryability slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_does_not_acknowledge_graph_projection_failures -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_does_not_acknowledge_projection_failures -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin compact QGLake credential secret refs`.
  Compact QGLake handoff tests now directly prove that each
  `credentialVendingProof` storage-profile branch rejects malformed
  secret-reference evidence: present secret refs require provider and SHA-256
  hash proof, and absent secret refs cannot carry hash evidence.
- Local verification for this compact credential-root slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_credential_secret_ref_provider_when_present -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_credential_secret_ref_hash_when_present -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_credential_secret_ref_hash_when_absent -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Validate QGLake credential source secret refs`.
  QGLake source replay verification now validates credential-branch
  secret-reference shape directly: secret refs marked present must carry a
  non-empty provider and SHA-256 hash, and branches marked absent cannot carry
  provider or hash evidence.
- Local verification for this source credential-root slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject restricted QGLake source exception reasons`.
  QGLake source replay verification now rejects restricted-agent credential
  replay events that carry a `rawCredentialExceptionReason`, keeping the
  captured LakeCat replay contract aligned with the compact handoff verifier.
- Local verification for this source credential-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_restricted_exception_reason -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject restricted QGLake exception reasons`.
  The compact QGLake handoff verifier now rejects restricted-agent
  `credentialVendingProof` branches that carry a
  `rawCredentialExceptionReason`; the exception reason is reserved for the
  audited trusted-human raw-credential path.
- Local verification for this compact credential-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_restricted_exception_reason -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_restricted_raw_exception -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject unrequested QGLake source stats proof`.
  QGLake source replay verification now rejects effective stats-field evidence
  that was not present in the requested stats fields, keeping captured replay
  proof aligned with the compact handoff verifier.
- Local verification for this source stats narrowing slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_scan_replay_rejects_unrequested_effective_stats_fields -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_replay_rejects_widened_effective_stats_fields -- --nocapture`.
- Latest completed implementation slice:
  `Reject unrequested QGLake handoff stats proof`.
  The compact QGLake handoff verifier now rejects
  `plannedEffectiveStatsFields` entries that were not present in
  `plannedRequestedStatsFields`, proving effective stats evidence is a true
  narrowing rather than an unrelated replacement list.
- Local verification for this handoff stats narrowing slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_unrequested_effective_scan_stats_field -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_scan_stats_field_widening -- --nocapture`.
- Latest completed implementation slice:
  `Require QGLake handoff effective stats proof`.
  The compact QGLake handoff verifier now rejects governed scan proofs that
  omit `plannedEffectiveStatsFields`, complementing the existing missing
  requested-stats and widened-effective-stats checks.
- Local verification for this handoff stats-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_effective_scan_stats_field_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_scan_stats_field_widening -- --nocapture`.
- Latest completed implementation slice:
  `Require QGLake handoff fetch filter proof`.
  The compact QGLake handoff verifier now rejects governed scan proofs that
  omit fetched `required-filters` evidence, matching the existing response-side
  verifier and extra-filter drift checks.
- Local verification for this handoff fetch filter-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_fetch_filter_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_extra_fetch_filter_evidence -- --nocapture`.
- Latest completed implementation slice:
  `Require QGLake fetch filter proof`.
  The QGLake `fetchScanTasks` verifier now rejects fetched scan-task responses
  that omit the `required-filters` proof for the server-derived row predicate,
  complementing the existing required-projection and drift checks.
- Local verification for this QGLake fetch filter-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier_rejects_missing_required_filters -- --nocapture`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier_rejects_missing_required_projection -- --nocapture`.
- Latest completed implementation slice:
  `Pin fetch scan-task policy scope`.
  Default-feature REST `fetchScanTasks` coverage now proves the service
  re-sends the required projection and mandatory policy filter to Sail while
  preserving required-projection/filter evidence in both the response extension
  and durable audit/outbox payload.
- Local verification for this fetch scan-task policy-scope slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service fetch_scan_tasks_route_sends_required_policy_scope_to_sail -- --nocapture`;
  `cargo test -p lakecat-service scan_tasks_fetched_audit_payload_surfaces_policy_context -- --nocapture`.
- Latest completed implementation slice:
  `Pin default scan planning policy scope`.
  Default-feature REST scan-planning coverage now proves the service sends
  only the server-derived effective projection and mandatory policy filter to
  Sail while preserving requested/effective stats and restriction evidence in
  both the response extension and durable audit/outbox payload.
- Local verification for this scan-planning policy-scope slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service scan_planning_route_sends_effective_policy_scope_to_sail -- --nocapture`;
  `cargo test -p lakecat-service scan_planned_audit_payload_surfaces_policy_context -- --nocapture`.
- Latest completed implementation slice:
  `Enforce credential issuer scope at service boundary`.
  The public `loadCredentials` path now revalidates every credential returned
  by the configured issuer against the selected storage profile before
  canonical evidence is attached. A default-feature route test proves a custom
  issuer cannot return a broader prefix than the table profile, and the error
  remains hash-only.
- Local verification for this credential issuer scope slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service credential_vend_rejects_issuer_credentials_outside_profile_scope -- --nocapture`;
  `cargo test -p lakecat-service credential_vend_allows_trusted_human_raw_exception_for_restricted_table -- --nocapture`.
- Latest completed implementation slice:
  `Reject userinfo metadata object locations`.
  Metadata-object commit validation now rejects planned new metadata locations
  containing URI query strings, fragments, or URI userinfo before object-store
  writes. The error reports `metadata-location-hash=sha256:...` without
  echoing the raw decorated object location or embedded userinfo.
- Local verification for this metadata location decoration slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_write_plan_rejects_userinfo_locations -- --nocapture`;
  `cargo test -p lakecat-service metadata_write_plan_rejects_query_or_fragment_locations -- --nocapture`;
  `git diff --check`.
- Latest completed implementation slice:
  `Pin blocked credential response evidence`.
  The blocked-agent credential-vend route now proves governed
  Sail-planned-read decisions commit an explicit empty
  `credential-response-evidence` array in the outbox payload, while the paired
  trusted-human route still proves one redacted credential response proof for
  the audited raw-credential exception path.
- Local verification for this blocked credential response evidence slice is
  green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service credential_vend_blocks_raw_credentials_for_fine_grained_restriction -- --nocapture`;
  `cargo test -p lakecat-service credential_vend_allows_trusted_human_raw_exception_for_restricted_table -- --nocapture`.
- Latest completed implementation slice:
  `Pin credential response outbox evidence`.
  The trusted-human raw credential exception route now has regression coverage
  proving the committed credential-vend outbox payload contains one redacted
  `credential-response-evidence` entry with canonical LakeCat profile/provider/
  mode/principal/governed-read/TTL values, SHA-256-shaped prefix and issuer
  config hashes, and no raw credential prefix.
- Local verification for this credential outbox evidence slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service credential_vend_allows_trusted_human_raw_exception_for_restricted_table -- --nocapture`;
  `cargo test -p lakecat-service credentials_vend_audit_payload_surfaces_policy_context -- --nocapture`.
- Latest completed implementation slice:
  `Audit canonical credential response evidence`.
  Credential-vend audit/outbox payloads now include redacted
  `credential-response-evidence` for each returned credential. The proof carries
  LakeCat-owned canonical evidence values, hashes the credential prefix, hashes
  issuer-owned config, and avoids raw session credential material while keeping
  replay able to prove what `loadCredentials` exposed.
- Local verification for this credential audit evidence slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service credentials_vend_audit_payload_surfaces_policy_context -- --nocapture`;
  `cargo test -p lakecat-service credential_vend_response_replaces_shadowed_lakecat_evidence -- --nocapture`.
- Latest completed implementation slice:
  `Canonicalize credential response evidence`.
  The public credential-vending path now strips issuer-supplied values for
  LakeCat-owned evidence keys and appends canonical catalog values for storage
  profile id, provider, issuance mode, authorization principal,
  governed-read-required, and effective TTL. Issuer-owned credential details
  such as credential kind and provider session tokens are preserved.
- Local verification for this credential response evidence slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service credential_vend_response_replaces_shadowed_lakecat_evidence -- --nocapture`;
  `cargo test -p lakecat-service credential_vend_response_normalizes_duplicate_ttl_entries -- --nocapture`.
- Latest completed implementation slice:
  `Reject reserved storage-profile public config keys`.
  Storage profiles now reject user-supplied `public-config` keys reserved for
  LakeCat credential evidence, including `lakecat.storage-profile-id`, before
  memory or Turso persistence. The management API returns a bad request for the
  same shadowing attempt while preserving allowed non-secret routing hints such
  as `lakecat.endpoint`.
- Local verification for this reserved public-config slice is green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `cargo test -p lakecat-store --features turso-local reserved_public_config -- --nocapture`;
  `cargo test -p lakecat-service management_storage_profile_rejects_reserved_public_config_keys -- --nocapture`;
  `cargo test -p lakecat-service management_storage_profile_overrides_inferred_credentials_by_prefix -- --nocapture`.
- Latest completed implementation slice:
  `Pin REST credential TTL normalization`.
  The public credential-vending path now has a service-level regression proving
  a backend that returns duplicate, wider, or malformed
  `lakecat.max-credential-ttl-seconds` entries is normalized before the
  `loadCredentials` response leaves LakeCat, while non-TTL credential config is
  preserved.
- Local verification for this REST credential TTL normalization slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service credential_vend_response_normalizes_duplicate_ttl_entries -- --nocapture`;
  `cargo test -p lakecat-service credential_ttl_cap -- --nocapture`.
- Latest completed implementation slice:
  `Normalize credential TTL evidence`.
  Credential-vending responses now collapse duplicate
  `lakecat.max-credential-ttl-seconds` config entries into one effective value,
  preserving stricter valid issuer TTLs while preventing wider or malformed
  duplicate backend entries from leaving ambiguous policy-cap evidence.
- Local verification for this credential TTL normalization slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service credential_ttl_cap -- --nocapture`.
- Latest completed implementation slice:
  `Pin REST stale-commit cleanup evidence`.
  The service-level stale metadata commit regression now proves the HTTP
  conflict response carries hashed expected/actual metadata-pointer evidence,
  does not expose raw committed or rejected metadata object paths, and still
  removes the uncommitted metadata object after compare-and-swap rejection.
- Local verification for this stale-commit cleanup evidence slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features sail-local cleans_up_uncommitted_metadata_file -- --nocapture`.
- Latest completed implementation slice:
  `Verify Grust Cypher dependency contract`.
  The local dependency-contract audit now checks that `grust-cypher` 0.9.0 is
  locked and resolves from crates.io through `cargo metadata --all-features`,
  so the `grust-local` graph/Cypher boundary is covered by the same published
  Grust crate proof as `grust-graph`.
- Local verification for this dependency-contract slice is green:
  `bash -n scripts/check-local-dependency-contract.sh`;
  `scripts/check-local-dependency-contract.sh`.
- Latest completed implementation slice:
  `Redact metadata backend error details`.
  Metadata-object object-store setup, create-only write, and cleanup failures
  now expose `error-detail-hash=sha256:...` evidence instead of raw backend
  error text that may contain local paths, bucket/object names, or configuration
  details. Metadata-location evidence remains hash-only.
- Local verification for this metadata backend redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_write_error_redacts_backend_detail -- --nocapture`;
  `cargo test -p lakecat-service metadata_cleanup_error_redacts_metadata_location -- --nocapture`.
- Latest completed implementation slice:
  `Pin outbox drain all-or-retry acknowledgement`.
  Lineage/graph draining now has an explicit regression for the recovery
  contract: if a projection fails, `drain_outbox_once` returns the projection
  error before acknowledging delivery, leaving the pending outbox event retryable
  even if an earlier sink already emitted a side effect.
- Local verification for this outbox acknowledgement slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_does_not_acknowledge_projection_failures -- --nocapture`.
- Latest completed implementation slice:
  `Deterministic outbox replay ordering`.
  Embedded memory and Turso stores now share the same pending-outbox contract:
  undelivered events are selected by `created_at,event_id`, and duplicate event
  IDs in a delivery acknowledgement count at most once. This keeps local tests,
  replay tooling, and durable Turso-backed deployments aligned on catalog
  side-effect ordering.
- Local verification for this outbox replay slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store memory_store_orders_pending_outbox_events_deterministically -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local turso_store_orders_pending_outbox_events_deterministically -- --nocapture`.
- Latest completed implementation slice:
  `Bind storage-profile secret-ref hash proof`.
  Storage-profile upsert replay now carries a redacted secret-reference hash
  when `secret-ref-present` is true. Lineage-drain summaries, QGLake replay
  JSON, credential storage-profile evidence, and compact handoff verification
  now require the hash to be SHA-256-shaped and reject hash evidence when the
  proof says no secret reference exists.
- Local verification for this storage-profile proof slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo check -p lakecat-api -p lakecat-service -p lakecat-cli`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_secret_ref_hash_when_present -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_secret_ref_hash_when_absent -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact storage-profile management secret refs`.
  Storage-profile upsert/list responses no longer echo raw `secret-ref`
  locators. They return `secret-ref-present`, `secret-ref-provider`, and
  `secret-ref-hash` evidence instead, matching the existing graph/lineage and
  resolver-error redaction discipline while leaving the management request
  shape unchanged.
- Local verification for this management-response redaction slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -- --check`;
  `cargo test -p lakecat-service remote_storage_profile_accepts_secret_ref_without_vending_raw_secrets -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject storage-profile dot-segment prefixes`.
  Storage-profile validation now rejects `location-prefix` values containing
  literal or percent-encoded `.`/`..` path segments before memory/Turso
  persistence. The validation error reports
  `storage-profile-prefix-hash=sha256:...` instead of echoing traversal-shaped
  storage roots.
- Local verification for this location-prefix hardening slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_dot_segment_location_prefixes -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local location_prefix_dot_segment_detection_allows_ordinary_dotted_names -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profile_upsert_rejects_deserialized_dot_segment_location_prefixes -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject sibling storage-profile prefix matches`.
  Storage-profile selection now treats the stored `location-prefix` as an exact
  root or path-boundary parent, not a raw string prefix. A credential root such
  as `s3://lakecat-demo/events` can select `s3://lakecat-demo/events` and child
  locations, but not sibling paths such as
  `s3://lakecat-demo/events-shadow/table`; unmatched tables fall back to the
  inferred governed-read profile.
- Local verification for this storage-profile boundary slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profile_matching_respects_location_boundaries -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local turso_storage_profile_matching_respects_trailing_slash_boundaries -- --nocapture`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Re-audit OPUS consolidation`.
  The OPUS review/design corpus is consolidated into `DESIGN.md` and adjacent
  canonical docs. The active tree has no root-level `OPUS*.md` files, and
  `docs/completed/` contains exactly `OPUS1.md`, `OPUS1-DESIGN.md`,
  `OPUS2.md`, and `OPUS2-DESIGN.md`, each with an archive banner pointing back
  to the living design.
- Local verification for this OPUS consolidation audit is documentation-only:
  `git ls-files 'OPUS*.md'`;
  `git ls-files 'docs/completed/OPUS*.md'`;
  `rg --files -g 'OPUS*.md' -g '!docs/completed/**'`;
  `rg --files docs/completed -g 'OPUS*.md'`.
- Latest completed implementation slice:
  `Reject secret-ref dot-segment roots`.
  Storage-profile validation now rejects `secret-ref` URIs containing literal
  or percent-encoded `.`/`..` path segments before memory/Turso persistence or
  resolver dispatch. The validation error reports only
  `secret-ref-hash=sha256:...`, keeping credential-root paths redacted.
- Local verification for this secret-ref hardening slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_dot_segment_secret_refs -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local secret_ref_dot_segment_detection_allows_ordinary_dotted_names -- --nocapture`.
- Latest completed implementation slice:
  `Reject metadata dot-segment commit locations`.
  Metadata-object commit validation now rejects planned new metadata locations
  containing literal or percent-encoded `.`/`..` path segments before object
  store parsing or create-only writes. The error stays audit-safe by reporting
  a `metadata-location-hash=sha256:...` instead of the raw path.
- Local verification for this metadata-location hardening slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_write_plan_rejects_dot_segment_locations -- --nocapture`;
  `cargo test -p lakecat-service location_dot_segment_detection_decodes_percent_encoded_segments -- --nocapture`.
- Latest completed implementation slice:
  `Bind QGLake stats-field replay proof`.
  Scan-planned audit/outbox summaries now carry both
  `requested-stats-fields` and `effective-stats-fields`; QGLake replay JSON
  exposes them as `plannedRequestedStatsFields` and
  `plannedEffectiveStatsFields`; the replay verifier, local handoff bridge,
  compact handoff verifier, and captured-output semantic checks now reject loss,
  widening, or drift of that proof.
- Local verification for this QGLake stats-field replay slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service scan_planned_audit_payload_surfaces_policy_context -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_replay_rejects_missing_stats_field_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_replay_rejects_widened_effective_stats_fields -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`.
- Latest completed implementation slice:
  `Record requested scan stats-field evidence`.
  Governed scan planning now records both `requested-stats-fields` and
  `effective-stats-fields` in the LakeCat scan-request extension while
  preserving the existing effective `stats-fields` alias. This keeps replay
  evidence from hiding attempted metadata/stat requests for columns outside the
  server-derived restriction.
- Local verification for this scan-proof slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`.
- Latest completed implementation slice:
  `Reject root metadata-object commit locations`.
  Metadata-object commit validation now requires the planned new metadata
  location to be a strict child of the selected storage-profile prefix, not the
  storage root itself. Root-targeted metadata-write plans fail before
  object-store writes with redacted metadata-location and storage-profile prefix
  hashes.
- Local verification for this commit-location slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_write_plan_rejects_storage_profile_root_location -- --nocapture`.
- Latest completed implementation slice:
  `Reject overbroad secret-manager credential prefixes`.
  TypeSec-authorized production secret-manager backends may now issue
  credentials only when every returned credential prefix is within the selected
  LakeCat storage-profile `location-prefix`. Overbroad backend responses fail
  closed with redacted prefix hashes before credentials are returned to Iceberg
  clients, preserving LakeCat's catalog-owned storage scope even when a
  configured external secret backend misbehaves.
- Local verification for this credential-scope slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_rejects_backend_credentials_outside_profile_scope -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_dispatches_configured_production_secret_backends_after_authorization -- --nocapture`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo test -p lakecat-service --features typesec-local`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Complete OPUS consolidation routing`.
  `DESIGN.md` now records not only which OPUS review/design sections were
  absorbed into the design, but also where adjacent OPUS-derived guidance lives:
  `ARCHITECTURE.md`, `GOAL.md`, `AGENTS.md`, `STATUS.md`, and the book. The
  completed-review archive README points future readers at that routing ledger,
  and the four archived OPUS files remain provenance-only.
- Local verification for this OPUS routing slice is documentation-only:
  `git ls-files 'OPUS*.md'`;
  `git ls-files 'docs/completed/OPUS*.md'`;
  `rg --files -g 'OPUS*.md' -g '!docs/completed/**'`;
  `rg --files docs/completed -g 'OPUS*.md'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject non-ASCII idempotency keys early`.
  REST table commits now validate `x-lakecat-idempotency-key` from raw header
  bytes against the documented ASCII key contract, so non-ASCII and invalid
  header bytes fail as `400 Bad Request` before authorization, Sail commit
  preparation, table loading, or metadata-object writes can run.
- Local verification for this idempotency-key validation slice is green:
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service commit_rejects_invalid_rest_idempotency_keys -- --nocapture`;
  `cargo test -p lakecat-service`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind handoff self-verifier semantic sections`.
  When a QGLake handoff summary carries `lakecatHandoffVerifyOutputHash`, the
  saved `lakecat-handoff-verify.json` artifact must now preserve the compact
  summary's captured replay semantics, bootstrap-bundle semantics, QueryGraph
  import-plan semantics, lineage-drain semantics, and bundle/import graph
  counts, so a self-verifier capture cannot carry correct top-level proof while
  embedding drifted semantic reconstructions.
- Local verification for this self-verifier semantic-section slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_accepts_handoff_verify_output_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_captured_semantic_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_count_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_semantic_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_artifact_hash_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_capture_hash_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier -- --nocapture`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind handoff self-verifier artifact hashes`.
  When a QGLake handoff summary carries `lakecatHandoffVerifyOutputHash`, the
  saved `lakecat-handoff-verify.json` artifact must now match the compact
  summary's bundle, lineage-drain, QueryGraph import-plan, captured-output, and
  service-log hashes, so a self-verifier capture cannot describe a different
  artifact manifest than the accepted handoff summary.
- Local verification for this self-verifier artifact-hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_accepts_handoff_verify_output_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_artifact_hash_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_capture_hash_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_semantic_drift -- --nocapture`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind handoff self-verifier semantics`.
  When a QGLake handoff summary carries `lakecatHandoffVerifyOutputHash`, the
  saved `lakecat-handoff-verify.json` artifact must now match the compact
  summary's table/view counts, stable ids, standards, request-identity proof,
  and QueryGraph bootstrap proof, so an archived self-verifier capture cannot
  be spliced from a different semantic handoff.
- Local verification for this self-verifier artifact slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_accepts_handoff_verify_output_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_semantic_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_proof_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier_rejects_handoff_verify_output_scope_drift -- --nocapture`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind QueryGraph import-plan graph counts`.
  QGLake handoff verification now compares
  `querygraphImportPlanSemantics.graphNodes` and `graphEdges` with
  `bundleArtifactSemantics.graphNodes` and `graphEdges`, rejecting a compact
  handoff whose QueryGraph import plan drops graph material from the verified
  bootstrap bundle while preserving table/view ids and semantic hashes.
- Local verification for this import-plan graph-count slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_semantics_reject_saved_import_plan_graph_count_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_rejects_import_plan_graph_count_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_bundle_artifact_semantics -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_querygraph_import_plan_semantics -- --nocapture`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Consolidate OPUS archive notes`.
  `DESIGN.md` now has one canonical OPUS consolidation section with the active
  document map, archive policy, source ledger, archive lock, and operating
  digest. The four OPUS files remain archived under `docs/completed/`, and
  `docs/completed/README.md` points future review work back through the living
  design instead of treating archived reviews as an active backlog.
- Local verification for this OPUS consolidation cleanup is documentation-only:
  `git ls-files 'OPUS*.md'`;
  `git ls-files 'docs/completed/OPUS*.md'`;
  `rg --files -g 'OPUS*.md' -g '!docs/completed/**'`;
  `rg --files docs/completed -g 'OPUS*.md'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind bootstrap request identity state`.
  Compact QGLake handoff verification now requires
  `queryGraphBootstrapProof.requestIdentitySource` and
  `queryGraphBootstrapProof.requestIdentityState` to match
  `requestIdentityProof`, preventing a summary from combining bootstrap
  evidence with a different identity path or verification state.
- Local verification for this bootstrap identity consistency slice is green:
  `cargo fmt -p lakecat-cli`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`.
- Latest completed implementation slice:
  `Bind QGLake handoff verifier output`.
  Compact handoff artifact verification now accepts
  `lakecatHandoffVerifyOutputHash` and, when present, reads the saved
  `lakecat-handoff-verify.json` artifact, verifies the SHA-256 hash, and checks
  that it is a `lakecat.qglake.handoff-verification.v1` success for the same
  principal, catalog URL, warehouse, namespace, and table. The local handoff
  harness now writes the verifier output, binds its hash into the summary, and
  performs a second sidecar self-check without overwriting the declared
  artifact.
- Local verification for this handoff verifier-output artifact slice is green:
  `cargo fmt -p lakecat-cli`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Preserve live QGLake replay evidence in handoff summaries`.
  The local handoff harness now carries governed scan graph events, fetched
  projection/filter requirements, management graph proof, storage-profile graph
  proof, credential exception/blocking proof, and table commit-history graph
  events from `qglake-verify-replay` into `handoff-summary.json` before running
  the compact Rust verifier.
- Local verification for this live handoff reconciliation slice is green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`.
- Latest completed implementation slice:
  `Reject invalid REST idempotency keys`.
  REST table commits now have service-level coverage proving illegal or
  overlong `x-lakecat-idempotency-key` values fail with `400 Bad Request`
  before authorization, Sail validation, table loading, or metadata-object
  writes.
- Local verification for this idempotency-key validation slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service commit_rejects_invalid_rest_idempotency_keys -- --nocapture`;
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require HTTP QGLake handoff catalog URLs`.
  Compact QGLake handoff verification now rejects malformed or non-HTTP(S)
  `catalogUrl` values, so saved handoff summaries bind replay/import evidence
  to an operator-reachable catalog endpoint rather than an arbitrary string.
- Local verification for this catalog URL slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_malformed_catalog_url -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_non_http_catalog_url -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Hash QGLake handoff service logs`.
  QGLake handoff summaries now carry `serviceLogHash`, and
  `qglake-verify-handoff` recomputes the service log bytes so an archived
  operational log cannot drift behind a path-only artifact alias.
- Local verification for this service-log artifact slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Finalize OPUS archive consolidation`.
  `DESIGN.md` now explicitly records that the OPUS corpus is no longer a
  parallel design system, and `docs/completed/README.md` keeps the archived
  OPUS files provenance-only with future review guidance routed back through
  canonical docs first.
- Local verification for this OPUS archive consolidation slice is
  documentation-only:
  `git ls-files 'OPUS*.md'` returned no active root OPUS files;
  `git ls-files 'docs/completed/OPUS*.md'` returned exactly the four archived
  OPUS files;
  `rg --files -g 'OPUS*.md' -g '!docs/completed/**'` returned no active OPUS
  files;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject malformed core QueryGraph hashes`.
  Compact QGLake handoff verification now requires SHA-256-shaped bundle,
  graph, OpenLineage, and QueryGraph import hashes before accepting the matched
  `querygraphVerification`, `querygraphImportVerification`, and
  `queryGraphBootstrapProof` sections.
- Local verification for this compact QueryGraph hash-shape slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_core_ -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind bootstrap view receipt hashes`.
  Compact QGLake handoff verification now requires
  `queryGraphBootstrapProof.viewVersionReceiptHashes` to match
  `viewReceiptChainProof.views[].acceptedReceiptHash` exactly, so a saved
  summary cannot splice bootstrap view receipt evidence from another run.
- Local verification for this compact view receipt-hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_bootstrap_view_receipt_hash_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Re-audit OPUS consolidation`.
  `DESIGN.md` remains the active synthesis for all OPUS review/design guidance,
  `docs/completed/README.md` records the archive shape, and each archived
  `OPUS*.md` file is explicitly marked as historical provenance rather than a
  live backlog.
- Local verification for this OPUS consolidation slice is documentation-only:
  archive inventory commands confirmed no active root `OPUS*.md` files and the
  four archived OPUS files under `docs/completed/`.
- Latest completed implementation slice:
  `Prove scan-plan graph replay`.
  QGLake governed scan source replay now requires scan-planned graph projection
  evidence and carries `planGraphEvents` through compact `governedScanProof`,
  captured LakeCat replay agreement, and the operator-readable scan replay line.
- Local verification for this scan-plan graph replay slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove management graph replay counts`.
  Compact QGLake `managementProof` and captured LakeCat replay agreement now
  carry positive graph event counts for server, project, warehouse,
  policy-binding, and storage-profile list replay, preserving the graph
  projection evidence that source replay already requires.
- Local verification for this management graph-count handoff slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject management graph replay gaps`.
  QGLake lineage-drain management-list source replay now requires catalog graph
  projection evidence for server, project, warehouse, policy-binding,
  storage-profile, and storage-profile-upsert replay before management proof can
  feed compact handoff verification.
- Local verification for this management graph source-replay slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject view receipt hash drift`.
  QGLake lineage-drain QueryGraph bootstrap source replay now compares replayed
  view-version receipt hashes with the accepted QueryGraph verification hash
  set and rejects drift before view proof can feed compact handoff
  verification.
- Local verification for this view receipt-hash source-replay slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject view receipt-chain namespace drift`.
  QGLake lineage-drain dropped-view source replay now requires namespace
  receipt-chain evidence to come from the accepted view's warehouse/namespace
  and rejects verified-chain count or receipt-hash coverage drift before view
  receipt proof can feed compact handoff verification.
- Local verification for this view receipt-chain source-replay slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject credential restriction drift`.
  QGLake lineage-drain credential source replay now requires both restricted
  agent and trusted-human branches to carry complete read-restriction evidence
  and rejects policy-derived restriction drift between the blocked agent path
  and audited raw-credential exception before credential proof can feed compact
  handoff verification.
- Local verification for this credential restriction source-replay slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject malformed bootstrap proof hashes`.
  QGLake lineage-drain request identity and QueryGraph bootstrap source replay
  now require SHA-256-shaped authorization, QueryGraph, agent delegation,
  summary-signature, and TypeDID proof hashes before request/bootstrap evidence
  can feed compact handoff proof.
- Local verification for this request/bootstrap proof-hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject scan restriction replay drift`.
  QGLake lineage-drain scan source replay now requires planned and fetched read
  restrictions to match and requires fetched projection/filter requirements to
  exactly preserve the fetched restriction before governed scan evidence can
  feed compact handoff proof.
- Local verification for this scan restriction source-replay slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject commit history count drift`.
  QGLake lineage-drain table commit-history source replay now requires the
  compact commit count to match both sequence-number and commit-hash evidence
  and rejects non-positive or non-increasing commit sequences before
  pointer-history evidence can feed compact handoff proof.
- Local verification for this commit-history count/sequence slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Consolidate archived OPUS docs`.
  The active root contains no live `OPUS*.md` files; the four historical OPUS
  files remain archived under `docs/completed/`, and their durable findings,
  design decisions, and working-plan guidance are routed through `DESIGN.md`.
- Local verification for this OPUS consolidation slice is green:
  `git ls-files 'OPUS*.md'`;
  `git ls-files 'docs/completed/OPUS*.md'`;
  `rg --files docs/completed -g 'OPUS*.md'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject malformed commit history hashes`.
  QGLake lineage-drain table commit-history replay now rejects malformed table
  commit hashes before pointer-history evidence can feed compact handoff proof.
- Local verification for this commit-history hash-shape slice is green:
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject malformed view receipt hashes`.
  QGLake lineage-drain replay now rejects malformed bootstrap
  view-version receipt hashes plus tombstone and namespace receipt-chain hashes
  before accepted-view replay can feed compact handoff proof.
- Local verification for this view receipt-shape slice is green:
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_view -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject malformed replay receipt hashes`.
  QGLake lineage-drain replay now rejects malformed receipt hashes for
  bootstrap, scan planning/fetch, credential replay, accepted views,
  receipt-chain reads, and table commit-history before compact handoff proof
  can consume those arrays.
- Local verification for this replay receipt-shape slice is green:
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_scan_receipt_hash_shape -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_management_receipt_hash_shape -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject malformed management receipt hashes`.
  QGLake lineage-drain replay now rejects malformed management-list and
  storage-profile-upsert receipt hashes before compact handoff proof is built,
  keeping source replay acceptance aligned with the stricter compact
  `managementProof` hash arrays.
- Local verification for this management receipt-shape slice is green:
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_management_receipt_hash_shape -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove management receipt hashes`.
  QGLake compact `managementProof` and captured LakeCat replay agreement now
  require replay and OpenLineage hash arrays for server, project, warehouse,
  policy-binding, and storage-profile list evidence, so management counts
  cannot stand alone without receipt-backed replay proof.
- Local verification for this management receipt-hash slice is green:
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_management_receipt_hashes -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics_accept_matching_files -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Lock OPUS consolidation`.
  The active OPUS material is consolidated into `DESIGN.md`, the root tree has
  no active `OPUS*.md` files, and the only tracked OPUS artifacts are the four
  historical reviews under `docs/completed/`.
- Local verification for this OPUS consolidation lock is green:
  `git ls-files 'OPUS*.md'`;
  `git ls-files 'docs/completed/OPUS*.md'`;
  `rg --files -g 'OPUS*.md' -g '!docs/completed/**'`;
  `rg --files docs/completed -g 'OPUS*.md'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove management replay counts`.
  QGLake compact `managementProof` now carries server, project, warehouse,
  policy-binding, and storage-profile replay counts, requires the management
  policy count to match bootstrap policy evidence, and rejects captured LakeCat
  replay drift for those counts before accepting a handoff summary.
- Local verification for this management replay-count slice is green:
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_management_policy_count_match -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics_accept_matching_files -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove accepted-view graph replay`.
  QGLake compact `viewReceiptChainProof.views[]` and captured LakeCat replay
  semantics now require positive `graphEvents` evidence for accepted view
  replay, aligning the handoff summary with the existing lineage-drain verifier
  requirement that view replay emits catalog graph and lineage projections.
- Local verification for this accepted-view graph replay slice is green:
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_view_graph_events -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics_accept_matching_files -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove table commit-history graph replay`.
  QGLake compact `tableCommitHistoryProof`, captured LakeCat replay semantics,
  and lineage-drain replay verification now require positive `graphEvents`
  evidence for `table.commits-listed`, so commit-history acceptance cannot prove
  only pointer-log and OpenLineage receipts while omitting the catalog graph
  projection. The operator-readable table commit-history replay line also
  prints the same graph event count.
- Local verification for this table commit-history graph replay slice is green:
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_commit_history_graph_events -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics_accept_matching_files -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_commit_history_replay_line_summarizes_verified_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Finalize OPUS consolidation digest`.
  `DESIGN.md` now carries the active OPUS synthesis as a concise operating
  digest plus archive-health commands, and `docs/completed/README.md` records
  the same mechanical audit expectations for the frozen OPUS provenance files.
- Local verification for this OPUS consolidation digest slice is green:
  `git ls-files 'OPUS*.md'`;
  `git ls-files 'docs/completed/OPUS*.md'`;
  `rg --files -g 'OPUS*.md' -g '!docs/completed/**'`;
  `rg --files docs/completed -g 'OPUS*.md'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove storage-profile graph replay`.
  QGLake compact storage-profile upsert proof and captured LakeCat replay
  semantics now require a positive `graphEvents` count for the replayed
  credential-root upsert, so credential graph anchors cannot appear without
  proof that the underlying storage-profile management event projected graph
  evidence too.
- Local verification for this storage-profile graph replay slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove trusted credential allowance`.
  QGLake compact credential proof and captured LakeCat replay semantics now
  require the trusted-human branch to carry `blockReason: null` alongside the
  audited raw-credential exception reason, so the human exception path cannot
  look both allowed and blocked in different replay views.
- Local verification for this trusted credential allowance slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove restricted credential exception denial`.
  QGLake compact credential proof and captured LakeCat replay semantics now
  require the restricted-agent branch to carry
  `rawCredentialExceptionAllowed: false`, so an agent credential block cannot
  be summarized as Sail-planned-read-only while replay evidence records a raw
  credential exception.
- Local verification for this restricted credential exception slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject credential TTL drift`.
  QGLake saved lineage-drain replay and compact handoff verification now reject
  drift between the restricted-agent credential TTL cap and the trusted-human
  audited raw-credential exception TTL cap, keeping both branches bound to the
  same policy-derived `max-credential-ttl-seconds` evidence.
- Local verification for this credential TTL drift slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Cross-check captured scan fetch requirements`.
  Captured QGLake LakeCat replay output now must match compact
  `governedScanProof` evidence for `fetchedRequiredProjection` and
  `fetchedRequiredFilters`, preventing a handoff summary from proving governed
  fetch narrowing while the terminal replay artifact records different
  projection/filter requirements.
- Local verification for this captured scan fetch-requirements slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require QGLake scan work counts`.
  Compact QGLake governed scan proof now rejects missing or zero
  `deleteFileCount` and `childPlanTaskCount` values, keeping delete-file and
  child-plan-task evidence as load-bearing acceptance proof beside plan-task
  and file-task counts.
- Local verification for this scan work-count slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require exact fetched filter proof`.
  Compact QGLake governed scan proof now rejects `fetchedRequiredFilters`
  arrays that include extra filters beyond the mandatory row predicate evidence
  derived from the fetched read restriction.
- Local verification for this exact fetched-filter slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Reject compact scan TTL drift`.
  Compact QGLake governed scan proof now rejects drift between planned and
  fetched `max-credential-ttl-seconds` values, matching the existing agreement
  checks for columns, predicates, policy hashes, and purpose.
- Local verification for this compact scan TTL drift slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require compact handoff QGLake standards`.
  Compact QGLake handoff summary verification now rejects summaries that omit a
  required QGLake standard such as `ODRL`, even when QueryGraph verify/import
  and LakeCat bootstrap proof sections agree with each other.
- Local verification for this compact standards slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Cross-check bootstrap embedded ODRL policy evidence`.
  QGLake bootstrap projection verification now rejects embedded ODRL policy
  bindings that drift from the structured policy-binding projection, preventing
  QueryGraph import evidence from carrying a stale read-restriction copy.
- Local verification for this bootstrap embedded-policy slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_bootstrap_projection_verifier -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require bootstrap TTL cap before replay proof`.
  QGLake bootstrap policy projection verification now rejects missing or drifted
  `max-credential-ttl-seconds` values before exported policy evidence can feed
  replay proof.
- Local verification for this bootstrap TTL-cap slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_bootstrap_projection_verifier -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require scan/fetch TTL cap before replay proof`.
  QGLake scan-plan and `fetchScanTasks` verification now reject missing or
  drifted `max-credential-ttl-seconds` values in live read-restriction evidence,
  keeping the lower-level plan/fetch verifier aligned with compact handoff
  proof requirements.
- Local verification for this plan/fetch TTL-cap slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_scan_plan_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier -- --nocapture`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Complete OPUS consolidation audit`.
  The full OPUS1/OPUS2 review and design corpus is mapped into `DESIGN.md` and
  the completed-review archive now records the expected audit shape for
  `docs/completed/OPUS*.md`.
- Local verification for this OPUS consolidation audit is green:
  `git ls-files 'OPUS*.md'`;
  `git ls-files 'docs/completed/OPUS*.md'`;
  `rg --files docs/completed -g 'OPUS*.md'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require scan purpose before replay proof`.
  QGLake bootstrap policy projection, scan-plan verification, and
  `fetchScanTasks` verification now reject missing or drifted
  read-restriction purpose before compact replay/handoff evidence is accepted.
- Local verification for this plan/fetch purpose slice is green:
  `cargo test -p lakecat-cli qglake_bootstrap_projection_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_scan_plan_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli --quiet`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Surface scan replay purpose`.
  The operator-readable QGLake scan replay line now prints planned and fetched
  read-restriction purpose values, so captured replay text preserves the same
  purpose evidence required by compact handoff proof.
- Local verification for this scan replay purpose slice is green:
  `cargo test -p lakecat-cli qglake_scan_replay_line_summarizes_verified_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics_rejects_governed_scan_drift -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require QGLake scan restriction purpose`.
  QGLake governed scan replay and compact handoff verification now require the
  read-restriction `purpose` alongside allowed columns, row predicate,
  policy-hash evidence, and `max-credential-ttl-seconds`, and reject drift
  between planned and fetched purpose values.
- Local verification for this scan restriction purpose slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_scan_restriction_purpose -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_scan_restriction_purpose_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_fixture_policy_installs_read_restriction -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind credential replay to storage-profile upsert replay`.
  QGLake lineage-drain replay verification now rejects restricted or trusted
  credential events whose redacted storage-profile evidence differs from the
  `storage-profile.upserted` replay event, including profile identity,
  provider, issuance mode, location-prefix hash, and secret-reference state.
- Local verification for this credential replay binding slice is green:
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind credential proof to storage-profile upsert`.
  QGLake compact handoff verification now rejects credential-vending proof
  whose credential storage-profile evidence drifts from the management
  `storageProfileUpsertProof`, including profile identity, provider,
  issuance-mode, location-prefix hash, and secret-reference state.
- Local verification for this credential/storage-profile binding slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_credential_storage_profile_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_rejects_credential_secret_ref_drift -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli`;
  `git diff --check`.
- Latest completed implementation slice:
  `Centralize QGLake hash-shape checks`.
  QGLake compact handoff and replay verification now validates required hash
  fields, optional hash fields, and hash arrays through the same shared
  `is_sha256_hash` predicate, keeping future proof fields aligned with the
  existing verifier rule.
- Local verification for this hash-shape verifier slice is green:
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli`;
  `git diff --check`.
- Latest completed implementation slice:
  `Verify management replay secret-ref state`.
  QGLake management replay verification now rejects storage-profile upsert
  replay whose secret-reference presence/provider fields contradict each other,
  and the operator-readable management replay line prints the redacted
  `secret_ref` state beside the credential-root storage-scope hash.
- Local verification for this management replay secret-ref slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_management_replay_line_summarizes_verified_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Refresh QGLake replay transcript example`.
  The LakeCat book's saved QGLake replay transcript example now matches the
  verified CLI output for management and credential replay lines, including the
  `location_prefix_hash` storage-scope evidence required by the current
  verifiers.
- Local verification for this book transcript sync slice is green:
  `docs/book/build.sh`;
  `rg -n "credential_roots=|credential_root=|location_prefix_hash=sha256:storage-location-prefix" docs/book/lakecat.md`;
  `git diff --check`.
- Latest completed implementation slice:
  `Expose management replay storage-scope hash`.
  QGLake management replay verification now requires storage-profile upsert
  replay to carry a SHA-256 `location-prefix-hash`, and the operator-readable
  management replay line prints the same credential-root storage-scope anchor
  as the structured replay and handoff proof.
- Local verification for this management replay storage-scope slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_management_replay_line_summarizes_verified_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events -- --nocapture`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Require credential replay storage-scope hash`.
  QGLake lineage-drain credential replay verification now rejects restricted or
  trusted-human credential events whose redacted storage-profile evidence lacks
  a `location-prefix-hash`, and the operator-readable credential replay line now
  prints that hash beside profile/provider/issuance-mode evidence.
- Local verification for this credential replay storage-scope verifier slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_credential_replay_line_summarizes_verified_evidence -- --nocapture`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier -- --nocapture`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Consolidate OPUS review log`.
  `DESIGN.md` now carries the durable OPUS1/OPUS2 review history and
  dev-manager working plan, including the current restriction, QGLake handoff,
  commit-hardening, graph-boundary, tenancy/credential, reproducibility, and
  done-state expectations. `docs/completed/README.md` now points each archived
  OPUS file at that consolidated active design section.
- Local verification for this OPUS consolidation slice is green:
  `cargo fmt --all -- --check`;
  `docs/book/build.sh`;
  `rg --files -g 'OPUS*.md' -g '!docs/completed/**'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Bind credential-vend replay to storage scope`.
  Credential-vend audit/outbox payloads now include
  `storage-profile.location-prefix-hash`, and QGLake compact handoff
  verification rejects credential proofs whose storage-profile evidence omits
  that hash.
- Local verification for this credential-vend storage-scope proof slice is
  green:
  `cargo fmt -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service credentials_vend_audit_payload_surfaces_policy_context -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_credential_location_prefix_hash -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact storage-profile replay location prefixes`.
  Storage-profile outbox projection now removes raw `location-prefix` values
  before graph and lineage emission and replaces them with
  `location-prefix-hash`, while summary extraction remains compatible with
  older raw-prefix outbox rows.
- Local verification for this storage-profile replay redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Validate storage-profile public config on upsert`.
  `StorageProfile::validate` now enforces the same public-config
  secret-material checks as `StorageProfile::new`, so deserialized or manually
  constructed profiles cannot bypass validation before memory or Turso
  persistence.
- Local verification for this storage-profile public-config validation slice is
  green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profile_validate_rejects_public_config_secret_values -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profile_upsert_rejects_deserialized_public_config_secrets -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Finalize OPUS archive consolidation`.
  `DESIGN.md` now carries the canonical document map and archive policy for the
  completed OPUS reviews. `docs/completed/README.md` maps each archived OPUS file
  to its current canonical home, and each OPUS file has an archive banner
  pointing readers back to `DESIGN.md`.
- Local verification for this OPUS archive consolidation slice is green:
  `cargo fmt --all -- --check`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact metadata object write errors`.
  Metadata-object commit validation and create-only write failures now report
  `metadata-location-hash=sha256:...` evidence, plus a
  `storage-profile-prefix-hash=sha256:...` for prefix mismatches, instead of
  echoing raw metadata object locations or storage roots in operator-facing
  errors.
- Local verification for this metadata object write error redaction slice is
  green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service commit_rejects_metadata_object_overwrite_of_current_pointer -- --nocapture`;
  `cargo test -p lakecat-service commit_rejects_metadata_object_overwrite_of_existing_target -- --nocapture`;
  `cargo test -p lakecat-service commit_rejects_metadata_object_outside_storage_profile_prefix -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact storage-profile secret-ref validation`.
  Storage-profile secret-reference validation now rejects unsupported
  credential-root schemes with `secret-ref-hash=sha256:...` evidence instead of
  echoing the submitted URI from the durable catalog validation path.
- Local verification for this storage-profile secret-ref validation redaction
  slice is green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_redact_invalid_secret_ref_uris -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_decorated_secret_ref_uris -- --nocapture`;
  `cargo test -p lakecat-store --features turso-local`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact resolver validation secret refs`.
  Vault and TypeSec environment secret-ref resolver validation errors now use
  `secret-ref-hash=sha256:...` evidence for wrong-scheme, missing-mount,
  missing-path, and invalid environment-variable cases instead of echoing the
  raw credential-root URI.
- Local verification for this resolver validation redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features typesec-local environment_secret_resolver_parses_supported_secret_shapes -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_production_secret_refs_before_dispatch -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact production secret-ref resolver errors`.
  Production secret-ref resolver not-configured errors now name the missing
  provider backend and include `secret-ref-hash=sha256:...` evidence instead of
  echoing the raw Vault, AWS Secrets Manager, GCP Secret Manager, or Azure Key
  Vault URI. TypeSec still authorizes the exact secret-ref resource before this
  resolver boundary is reached.
- Local verification for this production secret-ref error redaction slice is
  green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_production_secret_refs_before_dispatch -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_dispatches_configured_production_secret_backends_after_authorization -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove production credential TTL caps`.
  Configured `aws-sm://`, `gcp-sm://`, and `azure-kv://` production secret-ref
  backends are now exercised with a policy-derived
  `max-credential-ttl-seconds` cap. The test backend records that it received
  the cap, returned credentials must preserve the cap in
  `lakecat.max-credential-ttl-seconds`, and denied TypeSec decisions still avoid
  backend dispatch entirely.
- Local verification for this production credential TTL cap slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_dispatches_configured_production_secret_backends_after_authorization -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_production_secret_refs_before_dispatch -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed documentation slice:
  `Consolidate OPUS design reviews`.
  The active OPUS review decisions are now represented directly in `DESIGN.md`
  through a closure map and permanent decision list. The original OPUS files are
  archived under `docs/completed/` with a local archive index and should be used
  only as historical audit inputs.
- Latest completed implementation slice:
  `Verify fetch restriction requirements live`.
  The live QGLake `fetchScanTasks` verifier now requires the
  `lakecat:fetch-scan-tasks` extension to carry `required-projection` and
  `required-filters` evidence matching the governed allowed columns and
  mandatory row predicate.
- Local verification for this live fetch verifier slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier -- --nocapture`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Expose fetch restriction requirements`.
  `fetchScanTasks` responses now include LakeCat extension evidence for the
  exact `required-projection` and `required-filters` derived from the authorized
  table scan capability. This keeps stateless fetch replay tied to the
  revalidated restriction, not only to the raw read-restriction policy object.
  Lineage-drain summaries and compact QGLake replay proofs now carry and verify
  the same fetch-side requirement evidence.
- Local verification for this fetch restriction evidence slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-api`;
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `git diff --check`.
- Latest completed implementation slice:
  `Redact metadata cleanup failure locations`.
  Rejected commit cleanup still appends cleanup context to the original store or
  compare-and-swap error, but true cleanup failures now identify the
  uncommitted metadata object by SHA-256 metadata-location hash instead of
  echoing its raw object path.
- Local verification for this metadata cleanup redaction slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_cleanup_error_redacts_metadata_location -- --nocapture`;
  `cargo test -p lakecat-service metadata_cleanup -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `git diff --check`.
- OPUS review/design notes are consolidated into `DESIGN.md` and archived under
  `docs/completed/`; archive-relative links now resolve from their completed
  location so the files remain useful as historical audit inputs.
- Latest completed implementation slice:
  `Hash stale pointer conflict evidence`.
  Memory and Turso table commits now include SHA-256 hashes of the expected and
  actual metadata locations when a compare-and-swap check finds a stale table
  pointer. The conflict remains an Iceberg-visible conflict, but the message
  now gives operators reproducible evidence without echoing raw metadata object
  locations.
- Latest completed implementation slice:
  `Treat missing metadata cleanup targets as clean`.
  Rejected commits still attempt to delete uncommitted metadata objects after a
  store conflict, but cleanup now treats `NotFound` from the object store as
  success because the orphan is already absent. Real cleanup failures still
  preserve the original commit error class and append cleanup context.
- Latest completed implementation slice:
  `Emit embedded commit outbox events`.
  `MemoryCatalogStore::commit_table` now records the same catalog-facing
  `table.commit` audit/outbox evidence as the Turso-backed commit path,
  including the commit record, authorization receipt, response hash, and
  redacted idempotency-key hash. Idempotent replay still returns before writing
  new side effects, so embedded tests and in-memory deployments exercise the
  same transactional outbox contract as the durable local spine.
- Latest completed implementation slice:
  `Show governed scan TTL caps in replay text`.
  `lakecat-cli qglake-verify-replay` now includes the policy-derived
  `max-credential-ttl-seconds` cap in the compact scan replay line for both
  scan planning and scan-task fetch evidence. This keeps operator-readable
  terminal captures aligned with the structured QGLake proof that already
  rejects missing or drifted read restrictions.
- Latest completed implementation slice:
  `Bind QGLake credential proof to TTL caps`.
  QGLake replay evidence, compact handoff summaries, and saved lineage-drain
  verification now require the restricted-agent and trusted-human credential
  branches to carry the policy-derived `maxCredentialTtlSeconds` cap. The local
  handoff script derives that value from replay JSON/read restrictions before
  writing a summary, so credential exceptions cannot be replayed into
  QueryGraph without the same duration bound that LakeCat used at issuance.
- Latest completed implementation slice:
  `Carry policy TTL caps into credential issuance`.
  `CredentialIssuanceRequest` now includes the effective
  `max-credential-ttl-seconds` value derived from the read restriction, and the
  service annotates every returned storage credential with
  `lakecat.max-credential-ttl-seconds` when a policy cap exists. This makes the
  trusted-human raw credential exception and secret-ref issuer boundary carry a
  concrete TTL cap, not only a receipt-side note.
- Latest completed implementation slice:
  `Fail closed on unsupported ODRL restriction operators`.
  `lakecat-security` now rejects enforceable ODRL constraint forms for allowed
  columns, row predicates, purpose, and credential TTL when the constraint uses
  a missing or unsupported operator. This keeps F2 moving in the right
  direction: direct LakeCat read-restriction objects still work, while ODRL
  constraints must use allow/narrowing operators before LakeCat turns them into
  governed read restrictions.
- Latest completed documentation slice:
  `Consolidate OPUS design docs`.
  The OPUS review/design files are archived under `docs/completed/`, and the
  active design thesis, division of labor, finding status, and priority plan now
  live in root `DESIGN.md`. `AGENTS.md`, `GOAL.md`, and `ARCHITECTURE.md` point
  future work at the consolidated design instead of treating archived OPUS files
  as current instructions.
- Latest completed implementation slice:
  `Guard QueryGraph receipt-chain import compatibility`.
  `scripts/check-local-dependency-contract.sh` now verifies the sibling
  `/Users/alexy/src/querygraph/qg-rust` importer preserves
  `receipt-chain-hash` on LakeCat view receipt evidence and rejects missing
  receipt-chain evidence. This makes the QGLake handoff compatibility field an
  executable local contract rather than relying only on the live handoff
  harness to catch a stale QueryGraph consumer.
- Latest completed implementation slice:
  `Bind QGLake accepted view chain hashes to chain evidence`.
  `lakecat-cli qglake-verify-handoff` now rejects compact
  `viewReceiptChainProof` summaries whose active per-view
  `acceptedReceiptChainHash` is not covered by the namespace-level
  `receiptChains[].chainHashes` evidence. Tombstoned accepted views may carry
  the accepted prefix-chain hash only when the tombstone proof preserves the
  accepted view version. The local `scripts/qglake-handoff-local.sh` harness now
  performs the same check before writing `handoff-summary.json`, so live
  handoffs cannot carry unrelated valid-looking active view and namespace
  receipt-chain hashes. The real QueryGraph consumer in
  `/Users/alexy/src/querygraph/qg-rust` was also updated in scoped commit
  `46bc615 Preserve LakeCat view receipt chain evidence` to preserve and
  validate `receipt-chain-hash` in LakeCat import evidence, fixing the live
  import-contract hash check.
- Local verification for this QGLake view chain-hash coverage slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-cli`;
  QueryGraph `/Users/alexy/src/querygraph/qg-rust`: `cargo fmt -- --check`;
  QueryGraph `/Users/alexy/src/querygraph/qg-rust`: `cargo test --locked lakecat`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, ran LakeCat replay, QueryGraph verify/import, and verified
  the saved handoff summary with `graphEvents: 53`);
  `git diff --check`.
- QueryGraph book/diagram follow-through is checked in on
  `/Users/alexy/src/querygraph/qg-rust` as
  `e7b9fc6 Update QueryGraph book for LakeCat handoff`. That commit adds a
  LakeCat catalog-boundary chapter, a LakeCat handoff diagram, refreshed
  book/blog diagram materializations, and rebuilt QueryGraph EPUB/PDF/MOBI
  artifacts using the new `querygraph (0.1.0-46bc615)` book marker.
- Latest completed implementation slice:
  `Bind QueryGraph view imports to receipt-chain hashes`.
  QueryGraph bootstrap view receipt evidence now carries a per-view
  `receipt-chain-hash` beside the accepted version receipt hash. The service
  computes it from the same ordered durable receipt chain used by the
  management receipt-chain endpoint, `lakecat-querygraph` verifies it in the
  import compatibility contract and exposes it in
  `QueryGraphBootstrapVerification`, and QGLake replay/handoff verification now
  requires the accepted view proof to carry that chain anchor.
- Local verification for this QueryGraph view receipt-chain import slice is
  green:
  `cargo fmt -p lakecat-querygraph -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-querygraph`;
  `cargo test -p lakecat-service --features typesec-local querygraph_bootstrap_projects_catalog_views -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Dispatch configured production secret-ref backends`.
  `ExternalSecretRefCredentialResolver` now accepts explicit provider backends
  for production `aws-sm://`, `gcp-sm://`, and `azure-kv://` secret refs and
  dispatches to them only after TypeSec authorizes the exact
  `credentials.issue` resource. If no backend is configured, those providers
  still fail closed with the existing not-configured error, and tests prove
  denied TypeSec decisions do not call the backend. Built-in SDK resolvers
  beyond Vault remain pending.
- Local verification for this production secret backend dispatch slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_dispatches_configured_production_secret_backends_after_authorization -- --nocapture`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_production_secret_refs_before_dispatch -- --nocapture`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-service --features typesec-local`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Expose credential-root graph anchors in compact replay text`.
  `lakecat-cli qglake-verify-replay` now prints the same redacted
  storage-profile graph-anchor evidence for restricted-agent and trusted-human
  credential replay that the structured QGLake handoff verifier requires:
  profile id, provider, issuance mode, secret-reference presence/provider, and
  credential-root graph event count. This keeps operator-readable replay
  captures byte-compatible with the stronger structured proof without moving
  graph taxonomy or query behavior into LakeCat.
- Local verification for this replay-text slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_credential_replay_line_summarizes_verified_evidence -- --nocapture`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics -- --nocapture`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier -- --nocapture`;
  `cargo test -p lakecat-cli`;
  `git diff --check`.
- Latest completed implementation slice:
  `Prove credential-root graph anchors in QGLake handoff`.
  Compact `credentialVendingProof` branches now carry a redacted
  `storageProfile` object with profile id, provider, issuance mode,
  secret-ref presence/provider, and credential replay graph-event count. The
  standalone handoff verifier rejects summaries that omit that proof, and
  saved lineage-drain artifact verification now rejects credential replay that
  lacks the corresponding credential-root graph projection.
- Latest completed implementation slice:
  `Project credential-root access to graph`.
  Current `credentials.vend-attempted` audit/outbox payloads now include a
  redacted `storage-profile` anchor, and durable replay emits a
  catalog-facing `StorageProfile` graph event for each credential-vend attempt
  when that anchor is present. The graph projection carries profile id,
  warehouse, provider, issuance mode, and secret-ref presence only, so
  QueryGraph can see credential-root access without LakeCat exposing secret
  references, raw credentials, or graph query behavior.
- Latest completed implementation slice:
  `Project commit-history reads to graph`.
  Durable `table.commits-listed` replay now emits catalog-facing `Commit`
  graph events for each listed commit sequence, keyed by the table stable id
  and committed sequence number. The projection carries the matching
  commit-hash evidence from the pointer-log read payload, so QueryGraph can see
  governed commit-history inspection through Grust while LakeCat remains at the
  thin event-boundary layer.
- Latest completed implementation slice:
  `Verify QGLake handoff artifact path aliases`.
  `lakecat-cli qglake-verify-handoff` now compares the legacy
  `lakecatReplayOutput`, `querygraphVerifyOutput`, and
  `querygraphImportOutput` string aliases with the hashed
  `artifacts.capturedOutputs` entries they duplicate. The verifier also
  requires the operational `serviceLog` path to exist while treating
  `lakecatHandoffVerifyOutput` as a declared output path, since the local
  harness writes the verifier capture after the verifier has accepted the
  summary. This closes the remaining summary-shape gap where automation could
  preserve correct captured-output hashes while a stale alias pointed a reader
  at a different file.
- Latest completed implementation slice:
  `Verify saved QGLake lineage-drain artifact semantics`.
  `lakecat-cli qglake-verify-handoff` now parses the saved
  `lineage-drain.json` artifact, reruns the typed QGLake lineage-drain
  verifier against the compact accepted QueryGraph proof, regenerates LakeCat
  replay evidence from the archived drain response, and compares that evidence
  with `lakecatReplayVerification`. This closes the saved-drain gap where a
  handoff could keep a stale replay capture and compact summary while the
  archived drain artifact itself lost or changed outbox/lineage receipt
  evidence.
- Latest completed implementation slice:
  `Verify saved QueryGraph import plan artifact semantics`.
  `lakecat-cli qglake-verify-handoff` now parses the saved
  `querygraph-import-plan.json` artifact and compares its embedded QueryGraph
  import verification, accepted table/view ids, hashes, standards, and graph
  node/edge evidence with the compact `querygraphImportVerification` proof.
  This closes the archived-import gap where the compact summary and captured
  stdout could agree while the saved import plan file dropped or rebound an
  accepted table/view id.
- Latest completed implementation slice:
  `Verify saved QGLake bundle artifact semantics`.
  `lakecat-cli qglake-verify-handoff` now parses the saved
  `lakecat-bootstrap.json` artifact and reruns the QGLake bootstrap verifier
  against it, then compares the artifact's hashes, counts, standards, and
  verified table/view ids with the compact QueryGraph and LakeCat replay
  summary. This closes the archived-handoff gap where a summary and captured
  output could be self-consistent while the saved bundle file no longer proved
  the tenant graph path.
- Latest completed implementation slice:
  `Require QGLake bootstrap tenant graph proof`.
  `lakecat-cli qglake-fixture` / bootstrap verification now rejects accepted
  bundles whose catalog graph lacks the full Catalog > Server > Project >
  Warehouse > Namespace > Table path. This turns the manifest-covered tenant
  spine from exported context into acceptance evidence while keeping graph
  taxonomy, traversal, and query behavior in Grust.
- Latest completed implementation slice:
  `Bind QueryGraph bootstrap tenant spine to management records`.
  QueryGraph bootstrap graphs now prefer durable `ServerRecord`,
  `ProjectRecord`, and `WarehouseRecord` values for the manifest-covered
  Server > Project > Warehouse path, including display/endpoint/storage-root
  evidence. The older deterministic default tenant spine remains the fallback
  when management rows are absent, so existing bootstrap and import flows keep
  working while QueryGraph can bind namespace/table/view imports to real
  tenant records when LakeCat has them.
- Latest completed implementation slice:
  `Add QueryGraph bootstrap tenant spine`.
  QueryGraph bootstrap graphs now include deterministic Server, Project, and
  Warehouse anchors plus Warehouse-to-Namespace edges inside the manifest-hashed
  graph payload. The existing Catalog-to-Namespace edge remains for importer
  compatibility, while richer tenant hierarchy semantics stay Grust-owned and
  can later be replaced with actual management-record projection.
- Latest completed implementation slice:
  `Project server upserts to graph`.
  `lakecat-graph` now has a stable `Server` catalog subject, and durable
  `server.upserted` replay emits that graph event beside the existing
  OpenLineage receipt. This completes the thin catalog-facing tenant spine
  anchors for Server, Project, and Warehouse while leaving hierarchy semantics,
  traversal, and graph query behavior in Grust.
- Latest completed implementation slice:
  `Project storage-profile upserts to graph`.
  `lakecat-graph` now has a warehouse-scoped `StorageProfile` catalog subject,
  and durable `storage-profile.upserted` replay emits that graph event beside
  the existing OpenLineage receipt. The graph payload uses the redacted
  storage-profile evidence (`secret-ref-present` and provider only), so
  QueryGraph can see credential-root anchors without LakeCat leaking
  secret-store URIs or taking over Grust graph semantics.
- Latest completed implementation slice:
  `Reconcile sibling Sail commit state`.
  The local Sail checkout at `/Users/alexy/src/sail` is on `codex/graph` with
  tracked source changes committed in scoped local commits:
  `a6964906 Expose Iceberg REST models for LakeCat`,
  `e5393c9f Preserve Iceberg manifest bounds in Avro`, and
  `e4fb1d1b Add Sail Cypher graph query extension`. The graph-language work in
  that last commit is Sail's SQL/Cypher extension surface; reusable catalog
  graph taxonomy, projection, traversal, and stores remain Grust-owned. Sail has
  no tracked or staged diffs after those commits. Its remaining dirty status is
  untracked `.codex-artifacts/` and `book/`, which were left out of the scoped
  commits.
- Sail upstream push is still blocked by repository authentication, not by a
  local build failure: `git push origin codex/graph` failed with
  `could not read Username for 'https://github.com': Device not configured`.
  LakeCat remains clean and pushed on `master`; continue to rely on the local
  dependency-contract audit until Sail credentials or an upstream branch/publish
  path is resolved.
- Latest completed implementation slice:
  `Verify local Sail helper API surface`.
  `scripts/check-local-dependency-contract.sh` now checks the local Sail
  checkout for the helper exports LakeCat depends on: generated Iceberg REST
  models, typed metadata inputs, planning result helpers, fetchScanTasks
  helpers, and table-status conversion. This makes F10 drift visible even when
  the local Sail checkout already contains the patch bridge and raw patch
  application would be ambiguous.
- Local verification for this dependency-contract slice is green:
  `bash -n scripts/check-local-dependency-contract.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `cargo test --workspace`;
  `git diff --check`.
- Latest completed implementation slice:
  `Make QGLake import proof self-contained`.
  `lakecat-cli qglake-verify-handoff` now requires compact
  `querygraphImportVerification` to carry the same QueryGraph table/view ids,
  counts, hashes, and standards as `querygraphVerification`. Captured
  QueryGraph import output is checked against that import proof rather than a
  bare boolean, so the summary, verify capture, and import capture all bind the
  same accepted table/view scope and semantic hashes.
- Local verification for this import-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, ran LakeCat replay, QueryGraph verify/import, and verified
  self-contained compact `querygraphImportVerification` table/view ids and
  hashes against captured import output);
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `cargo test --workspace`;
  `git diff --check`.
- Latest completed implementation slice:
  `Cross-check captured QueryGraph verified ids`.
  `lakecat-cli qglake-verify-handoff` now compares captured QueryGraph
  verify/import `verified-tables` and `verified-views` arrays exactly against
  compact `querygraphVerification.verifiedTables` and `verifiedViews`, in
  addition to requiring the declared table/view scope. This prevents a compact
  summary from embedding one accepted id set while the saved QueryGraph captures
  name another.
- Local verification for this captured verified-id cross-check slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, ran LakeCat replay, QueryGraph verify/import, and verified
  matching compact/captured `verifiedTables`/`verifiedViews`);
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `cargo test --workspace`.
- Latest completed implementation slice:
  `Make QGLake handoff verified ids self-contained`.
  Compact `handoff-summary.json` files now embed
  `querygraphVerification.verifiedTables` and `verifiedViews`, and
  `lakecat-cli qglake-verify-handoff` validates those arrays directly against
  `tableCount`, `viewCount`, the declared warehouse/namespace/table scope, and
  the accepted stable view ids in `viewReceiptChainProof.views`. The local
  handoff harness also cross-checks the QueryGraph import arrays against the
  QueryGraph verify arrays before writing the summary.
- Local verification for this self-contained verified-id slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, ran LakeCat replay, QueryGraph verify/import, and emitted
  compact verification JSON with `verifiedTables` and `verifiedViews`);
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `cargo test --workspace`.
- Latest completed implementation slice:
  `Bind QGLake handoff view scope`.
  `lakecat-cli qglake-verify-handoff` and the local QGLake handoff harness now
  require QueryGraph verify/import captures to list every accepted LakeCat view
  stable id from `viewReceiptChainProof.views` in `verified-views`. This keeps
  a compact handoff from preserving the declared table scope while swapping or
  dropping the governed view evidence that LakeCat replay accepted.
- Local verification for this QGLake handoff view-scope slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, verified LakeCat replay, ran QueryGraph `lakecat-verify`
  and `lakecat-import`, then verified `handoff-summary.json` with QueryGraph
  verify/import `verified-views` containing
  `lakecat:view:local:default:active_customers_view`);
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `cargo test --workspace`.
- Latest completed implementation slice:
  `Bind QGLake handoff table scope`.
  `lakecat-cli qglake-verify-handoff` and the local QGLake handoff harness now
  require QueryGraph verify/import captures to list the stable table id derived
  from the handoff summary's `warehouse`, `namespace`, and `table` fields in
  `verified-tables`. This keeps a compact handoff from rebinding a verified
  bundle to a different table inside the same catalog tenant.
- Local verification for this QGLake handoff table-scope slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, verified LakeCat replay, ran QueryGraph `lakecat-verify`
  and `lakecat-import`, then verified `handoff-summary.json` with
  QueryGraph verify/import `verified-tables` containing
  `lakecat:table:local:default:events`);
  `cargo test --workspace --all-features`;
  `cargo test --workspace`.
- Latest completed implementation slice:
  `Verify QGLake handoff catalog scope`.
  `lakecat-cli qglake-verify-handoff` now rejects compact handoff summaries
  that omit non-empty `catalogUrl`, `warehouse`, `namespace`, or `table`
  scope fields. The verifier also rejects captured QueryGraph verify/import
  outputs whose `warehouse` no longer matches the summary, and the local
  QGLake handoff harness now checks the same warehouse agreement before
  writing `handoff-summary.json`.
- Local verification for this QGLake handoff catalog-scope slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, verified LakeCat replay, ran QueryGraph `lakecat-verify`
  and `lakecat-import`, then verified `handoff-summary.json` with
  `catalogUrl: http://127.0.0.1:18181`, `warehouse: local`,
  `namespace: default`, `table: events`, and captured QueryGraph
  verify/import warehouse semantics matching `local`);
  `cargo test --workspace --all-features`;
  `cargo test --workspace`.
- Latest completed implementation slice:
  `Align TypeDID handoff hash slots`.
  `lakecat-cli qglake-verify-handoff` and the local QGLake handoff harness now
  reject compact `requestIdentityProof` and `queryGraphBootstrapProof`
  TypeDID hash slots unless each optional envelope/proof hash is null or a
  SHA-256 value. A TypeDID proof hash is accepted only when the paired envelope
  hash is present, keeping compact QGLake handoffs self-describing while
  TypeSec remains responsible for TypeDID trust semantics.
- Local verification for this TypeDID handoff hash-slot slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, verified LakeCat replay, ran QueryGraph `lakecat-verify`
  and `lakecat-import`, then verified `handoff-summary.json` with
  `requestIdentityProof.typedidEnvelopeHash: null`,
  `requestIdentityProof.typedidProofHash: null`,
  `queryGraphBootstrapProof.typedidEnvelopeHash: null`, and
  `queryGraphBootstrapProof.typedidProofHash: null`);
  `cargo test --workspace --all-features`;
  `cargo test --workspace`.
- Latest completed implementation slice:
  `Align storage-profile secret-ref provider proof`.
  `lakecat-cli qglake-verify-handoff` and the local QGLake handoff harness now
  reject compact `storageProfileUpsertProof` summaries that carry a
  `secretRefProvider` while `secretRefPresent` is false, while still requiring
  a non-empty provider whenever `secretRefPresent` is true. This keeps the
  redacted credential-root handoff proof from implying an external secret-store
  dependency when the replay says no secret reference was configured.
- Local verification for this storage-profile proof-alignment slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, verified LakeCat replay, ran QueryGraph `lakecat-verify`
  and `lakecat-import`, then verified `handoff-summary.json` with
  `secretRefPresent: false` and `secretRefProvider: null`);
  `cargo test --workspace --all-features`;
  `cargo test --workspace`.
- Latest completed implementation slice:
  `Align view receipt-chain handoff counts`.
  `lakecat-cli qglake-verify-handoff` and the local QGLake handoff harness now
  reject compact `viewReceiptChainProof.receiptChains` evidence whose
  `verifiedChainCount` does not match the number of namespace chain hashes, or
  whose receipt hashes do not cover those verified chain hashes. This keeps
  QueryGraph/operator handoffs from accepting a summary that claims more
  verified chains than the replay evidence names.
- Local verification for this view receipt-chain count-alignment slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/qglake-handoff-local.sh` (generated one table and one view, drained
  26 outbox events, verified LakeCat replay, ran QueryGraph `lakecat-verify`
  and `lakecat-import`, then verified `handoff-summary.json` with
  `verifiedChainCount` matching its namespace chain hashes);
  `cargo test --workspace --all-features`;
  `cargo test --workspace`.
- Latest completed implementation slice:
  `Verify view receipt-chain version transitions`.
  Governed namespace view receipt-chain verification now fails closed unless a
  chain proves both ordered `previous-receipt-hash` links and catalog
  view-version semantics. The first receipt must be a version-1 upsert, each
  later upsert must advance exactly one version from the previous receipt, and
  drop tombstones must preserve the accepted durable view version while linking
  to the previous receipt. This keeps QueryGraph/QGLake from accepting a
  cryptographically linked receipt list that lies about view progression.
- Local verification for this view receipt-chain transition slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service view_receipt_chain_verifier_requires_version_transitions`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `cargo test --workspace`;
  `git diff --check`.
- Latest completed implementation slice:
  `Use published Grust and TypeSec crates`.
  A temporary registry-only Cargo probe under `/private/tmp` proved that
  `grust-graph` 0.9.0, `grust-cypher` 0.9.0, and `typesec` 0.8.0 resolve from
  crates.io and pass `cargo check` together. LakeCat now depends on published
  Grust/TypeSec crates instead of sibling path pins, manual CI no longer checks
  out those sibling repos, and `scripts/check-local-dependency-contract.sh`
  now proves registry resolution for Grust/TypeSec while keeping the Sail
  helper bridge local until those APIs are published.
- Local verification for this published Grust/TypeSec dependency slice is
  green:
  temp registry probe `cargo metadata --format-version 1 --no-deps`;
  temp registry probe `cargo check`;
  `cargo test -p lakecat-graph --features grust-local`;
  `cargo test -p lakecat-security --features typesec-local`;
  `cargo test -p lakecat-service --features typesec-local`;
  `cargo fmt -p lakecat-api -p lakecat-cli -p lakecat-core -p lakecat-graph -p lakecat-lineage -p lakecat-querygraph -p lakecat-sail -p lakecat-security -p lakecat-service -p lakecat-store -- --check`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`;
  `cargo test --workspace`;
  `git diff --check`.
- Latest completed implementation slice:
  `Harden storage-profile secret-ref URIs`.
  Storage-profile validation now parses external secret-store references and
  rejects query strings, fragments, or URI userinfo before the profile can be
  persisted in memory or Turso. This keeps `secret-ref` as a clean external
  locator for TypeSec-gated resolution rather than another place to smuggle
  token-like material into catalog state.
- Local verification for this storage-profile secret-ref hardening slice is
  green:
  `cargo fmt -p lakecat-store -- --check`;
  `cargo test -p lakecat-store --features turso-local storage_profiles_reject_decorated_secret_ref_uris`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_secret_ref_profiles_without_secret_material`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Verify governed scan OpenLineage handoff proof`.
  `lakecat-cli qglake-verify-handoff` now requires compact
  `governedScanProof` summaries to carry planned and fetched OpenLineage hashes
  in addition to planned/fetched replay hashes and matching read restrictions.
  This moves the live handoff harness's scan OpenLineage evidence contract into
  the Rust verifier consumed by QueryGraph and operator automation.
- Local verification for this governed scan OpenLineage handoff-proof slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Verify table commit-history handoff proof`.
  `lakecat-cli qglake-verify-handoff` now independently validates compact
  `tableCommitHistoryProof` pointer-log evidence. The proof must carry a
  positive commit count, sequence numbers whose length matches the count,
  commit hashes whose length matches the count, positive strictly increasing
  sequence numbers, and replay/OpenLineage hashes. This moves the shell
  harness's commit-history guard into the Rust verifier used by QueryGraph and
  operator automation.
- Local verification for this table commit-history handoff-proof slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`.
- Latest completed documentation adjustment:
  `Pin current AGENTS contract in GOAL`.
  `GOAL.md` now explicitly treats the current user-supplied
  `LakeCat Agent Guidance` from `AGENTS.md` as durable goal state across
  resumes and context compaction, covering repo boundaries, compatibility,
  implementation priorities, Turso usage, local verification, book updates,
  changelog/commit discipline, and sibling-repo placement.
- Latest completed implementation slice:
  `Verify storage-profile handoff proof`.
  `lakecat-cli qglake-verify-handoff` now independently validates compact
  `storageProfileUpsertProof` credential-root evidence. The proof must carry
  profile id, provider, issuance mode, a SHA-256 location-prefix hash, explicit
  `secretRefPresent`, a non-empty redacted secret-reference provider whenever a
  secret reference is present, and replay/OpenLineage hashes.
- Local verification for this storage-profile handoff-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Verify credential-vending handoff proof`.
  `lakecat-cli qglake-verify-handoff` now independently validates compact
  `credentialVendingProof` identity and receipt evidence instead of relying on
  captured-output comparison alone. The restricted branch must name the
  accepted agent principal, prove zero credentials, carry the governed
  Sail-planned-read block reason, and include replay/OpenLineage hashes. The
  trusted-human branch must name a human principal, prove a positive credential
  count, carry the audited raw-credential exception decision and exact reason,
  and include replay/OpenLineage hashes.
- Local verification for this credential-vending handoff-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Verify view receipt-chain handoff identity proof`.
  `lakecat-cli qglake-verify-handoff` now independently validates the compact
  `viewReceiptChainProof` receipt-chain evidence instead of only checking that
  the fields exist. For handoffs with views, the verifier requires the compact
  `views` array to match `viewCount`, preserves stable view warehouse,
  namespace, and name identity, proves `viewVersion == acceptedViewVersion`,
  and requires accepted receipt hashes, tombstone receipt hashes, positive
  verified-chain counts, receipt-chain warehouse/namespace identity, namespace
  chain hashes, and replay/OpenLineage hashes on the accepted, tombstone, and
  namespace-chain branches. This keeps the hash-chain proof self-contained in
  the Rust verifier and aligned with the local shell harness.
- Local verification for this view receipt-chain handoff-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed documentation slice:
  `Pin latest AGENTS guidance in GOAL`.
  `GOAL.md` now explicitly records that the latest pasted `AGENTS.md` block is
  the current standing operating contract across thread resumes and context
  compaction, including thin LakeCat boundaries, Sail/Grust/TypeSec placement,
  QueryGraph integration, Turso preference, local verification, changelog, and
  commit/push discipline.
- Latest completed implementation slice:
  `Prove governed scan restrictions in handoff`.
  Lineage-drain event summaries now preserve the governed scan
  `read-restriction` from scan-planned and scan-task-fetched outbox payloads.
  QGLake replay JSON lifts the planned and fetched restriction into
  `replay-evidence.scan`, the local handoff harness writes both into compact
  `governedScanProof`, and `lakecat-cli qglake-verify-handoff` rejects
  summaries where either restriction is missing or where the fetched branch
  drifts from the planned branch. The compact QueryGraph handoff now proves the
  narrowed allowed columns, row predicate, and policy hashes, not just Sail task
  counts and replay hashes.
- Local verification for this governed scan restriction-proof slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `docs/book/build.sh`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then verified
  `handoff-summary.json` with matching
  `governedScanProof.plannedReadRestriction` and
  `governedScanProof.fetchedReadRestriction` containing the restricted columns,
  `severity != debug` row predicate, max credential TTL, and policy hash;
  `cargo test --workspace`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Verify guarded tombstone handoff proof`.
  `lakecat-cli qglake-verify-handoff` now independently rejects compact
  `viewReceiptChainProof.tombstoneReceipts` entries whose
  `expectedViewVersion` is missing or does not match the accepted durable view
  version for the same stable view id. The standalone Rust verifier now
  enforces the same governed deletion proof that the live local handoff harness
  enforces before writing `handoff-summary.json`, so QueryGraph automation does
  not have to rely on shell-only JSON checks.
- Local verification for this guarded tombstone handoff-proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `docs/book/build.sh`;
  `cargo test --workspace`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Require guarded QGLake view tombstones`.
  The live QGLake fixture now remembers the durable version assigned to its
  transient accepted view and passes that value as `expected-view-version` when
  dropping the view. QGLake lineage-drain acceptance rejects dropped accepted
  views unless the `view.dropped` replay preserves the expected guard, and
  replay JSON now lifts the guarded tombstone value into
  `viewReceiptChainProof.tombstoneReceipts[*].expectedViewVersion`. The local
  handoff harness validates that each tombstone receipt's expected version
  matches the accepted view version before writing the compact summary, so the
  saved LakeCat replay and handoff summary prove the accepted view was deleted
  through LakeCat's optimistic catalog guard.
- Local verification for this guarded QGLake tombstone slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json`; the resulting
  `capturedOutputSemantics.lakecatReplay.viewReceiptChainProof.tombstoneReceipts[0].expectedViewVersion`
  was `1`;
  `docs/book/build.sh`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Replay guarded view version evidence`.
  View upsert/drop audit payloads now preserve accepted
  `expected-view-version` guards, lineage-drain event summaries expose
  `expected-view-version` alongside the accepted durable `view-version`, and
  QGLake view replay JSON lifts the value as `expectedViewVersion`. The service
  drain test now distinguishes guarded view mutations from ordinary view loads,
  and the QGLake replay fixture models an optimistic replacement guarded by
  version 1 that produces accepted view version 2. QueryGraph handoffs can now
  prove not only which durable view version was replayed, but which optimistic
  catalog version LakeCat checked before accepting the replacement or
  tombstone.
- Local verification for this guarded view replay-evidence slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_view_events_to_graph_and_lineage`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `cargo test -p lakecat-cli qglake_lineage_drain_verifier_requires_delivered_events`;
  `docs/book/build.sh`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`.
- Latest completed implementation slice:
  `Guard view mutations with expected versions`.
  Management and catalog REST view upserts and drops now accept optional
  `expected-view-version`. When present, LakeCat checks the current durable
  view version atomically in the `CatalogStore` mutation path before replacing
  the view, deleting it, or appending a receipt. Stale replacements and stale
  tombstones return conflict and leave the current view plus receipt chain
  unchanged. The check is implemented for both the embedded memory store and
  the Turso-backed local store, preserving compatibility for callers that omit
  the field while giving QueryGraph agents and operators a catalog-owned guard
  for view commit semantics.
- Local verification for this guarded view-mutation slice is green:
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_view_records`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities`;
  `cargo test -p lakecat-cli ensure_qglake_transient_view`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test --workspace --all-features`;
  `git diff --check`.
- Latest completed implementation slice:
  `Cross-check remaining captured replay proofs`.
  `lakecat-cli qglake-verify-handoff` now compares compact
  `lakecatReplayVerification.tableCommitHistoryProof` and
  `lakecatReplayVerification.viewReceiptChainProof` values with the captured
  LakeCat replay JSON at `replay-evidence.tableCommitHistory` and
  `replay-evidence.views`. A handoff is rejected if the saved replay artifact
  and compact summary disagree on pointer-log commit count, sequence numbers,
  commit hashes, table-commit replay/OpenLineage hashes, accepted view receipt
  evidence, tombstone receipt evidence, namespace receipt-chain hashes, or
  view replay/OpenLineage hashes. The verifier output now echoes both accepted
  branches under `capturedOutputSemantics.lakecatReplay`, so the compact
  table-commit and view-history proofs are checked against the same captured
  replay artifact as scan, identity, bootstrap, storage-profile, and credential
  evidence.
- Local verification for this remaining captured replay proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json` and emitted
  `capturedOutputSemantics.lakecatReplay.tableCommitHistoryProof` plus
  `viewReceiptChainProof`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Cross-check captured governed scan proof`.
  `lakecat-cli qglake-verify-handoff` now compares compact
  `lakecatReplayVerification.governedScanProof` values with the captured
  LakeCat replay JSON at `replay-evidence.scan`. A handoff is rejected if the
  replay artifact and compact summary disagree on Sail plan task count, file
  task count, delete-file count, child plan task count, planned/fetched replay
  event hashes, or planned/fetched OpenLineage hashes. The verifier output now
  echoes the accepted captured scan proof under
  `capturedOutputSemantics.lakecatReplay.governedScanProof`, making the
  governed Sail-planned read path a replay-checked acceptance proof rather than
  only a summary claim.
- Local verification for this captured governed scan proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json` and emitted
  `capturedOutputSemantics.lakecatReplay.governedScanProof`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Cross-check captured request/bootstrap replay proofs`.
  `lakecat-cli qglake-verify-handoff` now compares compact
  `requestIdentityProof` and `queryGraphBootstrapProof` values with the
  captured LakeCat replay JSON at `replay-evidence.requestIdentity` and
  `replay-evidence.queryGraphBootstrap`. A handoff is rejected if the replay
  artifact and compact summary disagree on principal identity, request-identity
  source/state, authorization receipt hash, TypeDID envelope/proof hash slots,
  QueryGraph bootstrap/import hashes, graph/OpenLineage hashes, artifact
  counts, policy count, standards, agent delegation hash, agent summary
  signature hash, view receipt hashes, replay event hashes, or OpenLineage
  replay hashes. The verifier output now echoes those accepted captured proofs
  under `capturedOutputSemantics.lakecatReplay.requestIdentityProof` and
  `capturedOutputSemantics.lakecatReplay.queryGraphBootstrapProof`.
- Local verification for this captured request/bootstrap replay proof slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json` and emitted
  `capturedOutputSemantics.lakecatReplay.requestIdentityProof` plus
  `queryGraphBootstrapProof`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Cross-check captured credential replay proof`.
  `lakecat-cli qglake-verify-handoff` now compares the compact
  `lakecatReplayVerification.credentialVendingProof` with the captured LakeCat
  replay JSON at `replay-evidence.credentials`. A handoff is rejected if the
  replay artifact and compact summary disagree on restricted-agent identity,
  credential count, Sail-planned-read block reason, replay/OpenLineage hashes,
  trusted-human identity, audited raw-credential exception allowance/reason, or
  trusted-human replay/OpenLineage hashes. The verifier output also echoes the
  accepted captured credential proof under
  `capturedOutputSemantics.lakecatReplay.credentialVendingProof`. The local
  handoff harness also now includes storage-profile `issuanceMode` and
  `locationPrefixHash` when generating `handoff-summary.json`, keeping the live
  script compatible with the stricter storage-profile verifier.
- Local verification for this captured credential replay proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json` and emitted
  `capturedOutputSemantics.lakecatReplay.credentialVendingProof` plus
  `storageProfileUpsertProof` with `issuanceMode` and `locationPrefixHash`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Cross-check captured storage-profile replay proof`.
  `lakecat-cli qglake-verify-handoff` now compares the compact
  `lakecatReplayVerification.storageProfileUpsertProof` with the captured
  LakeCat replay JSON at
  `replay-evidence.management.storageProfileUpsert`. A handoff is rejected if
  the replay artifact and compact summary disagree on profile id, provider,
  issuance mode, location-prefix hash, secret-reference presence/provider,
  replay event hashes, or OpenLineage hashes. The verifier output also echoes
  the accepted captured storage-profile proof under
  `capturedOutputSemantics.lakecatReplay.storageProfileUpsertProof`, giving
  QueryGraph and operators a compact local proof that the credential-root
  evidence was not rewritten between replay and handoff summary.
- Local verification for this captured replay proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output_semantics`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Hash storage-profile credential roots in QGLake replay`.
  Lineage-drain event summaries now carry
  `storage-profile-location-prefix-hash` for storage-profile upserts, computed
  over the configured `location-prefix` without placing the raw prefix in the
  compact proof. QGLake replay JSON lifts that value into
  `replay-evidence.management.storageProfileUpsert.locationPrefixHash`, and
  `lakecat-cli qglake-verify-handoff` now rejects handoff summaries whose
  `storageProfileUpsertProof` omits `locationPrefixHash`. This binds the
  credential-root proof to its storage scope while preserving the redacted
  operator/QueryGraph handoff shape.
- Local verification for this credential-root hash slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_storage_profile_location_prefix_hash`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Prove storage-profile issuance mode in QGLake replay`.
  Lineage-drain event summaries now carry redacted
  `storage-profile-issuance-mode` evidence for storage-profile upserts. QGLake
  replay JSON lifts that value into
  `replay-evidence.management.storageProfileUpsert.issuanceMode`, and
  `lakecat-cli qglake-verify-handoff` now rejects handoff summaries whose
  `storageProfileUpsertProof` omits `issuanceMode`. This keeps credential-root
  proofs useful for QueryGraph and operators without exposing raw secret-store
  URIs or credentials.
- Local verification for this credential-root proof slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-service outbox_drain_projects_storage_profile_upserts_to_lineage`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_requires_storage_profile_issuance_mode`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier_accepts_compact_proofs`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Verify Grust catalog-event taxonomy labels`.
  `lakecat-graph` now has a `grust-local` boundary test that writes LakeCat
  `Column`, `Snapshot`, `Commit`, `Principal`, and `ScanPlan` events through
  the Grust-owned catalog event adapter, then uses Grust Cypher to match the
  `Column` and `Snapshot` catalog-event labels and mutate/query them from
  `MemoryGraphStore`. This strengthens the QueryGraph graph boundary while
  keeping graph mechanics, Cypher behavior, and richer typed taxonomy work in
  Grust.
- Local verification for this graph-boundary slice is green:
  `cargo fmt -p lakecat-graph -- --check`;
  `cargo test -p lakecat-graph --features grust-local grust_cypher_can_query_catalog_event_taxonomy_labels`;
  `cargo test -p lakecat-graph --features grust-local`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed documentation slice:
  `Pin current AGENTS guidance in GOAL`.
  `GOAL.md` now explicitly says the 2026-06-19 `AGENTS.md` instruction block
  supplied in the active thread is mirrored there as durable project guidance,
  covering repo boundaries, compatibility rules, implementation priorities,
  verification, and commit discipline.
- Local verification for this documentation slice is limited to
  `git diff --check`.
- Latest completed implementation slice:
  `Reject metadata object overwrite targets`.
  REST metadata-object commits now write through `object_store` with
  create-only semantics (`PutMode::Create`). A commit whose requested new
  metadata location already exists now fails with conflict instead of
  overwriting a non-current, orphaned, or concurrently created metadata file.
  This extends the earlier current-pointer overwrite guard to every existing
  metadata object target while preserving idempotent replay before object
  writes.
- Local verification for the metadata overwrite guard slice is green:
  `cargo fmt -p lakecat-service -- --check`;
  `cargo test -p lakecat-service metadata_object_overwrite`;
  `cargo test -p lakecat-service commit_can_advance_metadata_location_extension`;
  `cargo test -p lakecat-service --features turso-local management_table_commits_lists_pointer_log_evidence`;
  `cargo test -p lakecat-service --features turso-local metadata_object_overwrite`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Verify QGLake captured-output semantics`.
  `lakecat-cli qglake-verify-handoff --summary ... [--json]` now parses the
  captured LakeCat replay JSON and QueryGraph verify/import JSON files named
  in `handoff-summary.json` after recomputing their hashes. It rejects a
  handoff when those saved captures disagree with the compact summary on the
  replay schema/status, table/view counts, bundle hash, graph hash,
  OpenLineage hash, QueryGraph import hash, or standards, and emits a
  `capturedOutputSemantics` object in the verifier output for operator and
  automation evidence.
- Local verification for the captured-output semantic slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_handoff_captured_output`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json` and emitted
  `capturedOutputSemantics` for the LakeCat replay, QueryGraph verify, and
  QueryGraph import captures;
  direct CLI check:
  `cargo run -p lakecat-cli -- qglake-verify-handoff --summary target/qglake-handoff/handoff-summary.json --json`;
  `cargo test -p lakecat-cli`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Hash captured QGLake verifier outputs`.
  `scripts/qglake-handoff-local.sh` now records `capturedOutputs` hashes for
  the LakeCat replay JSON, QueryGraph verify JSON, and QueryGraph import JSON
  captures in `handoff-summary.json`. `lakecat-cli qglake-verify-handoff`
  recomputes those captured-output hashes along with the raw bundle,
  lineage-drain, and QueryGraph import-plan artifact hashes, so automation can
  prove the compact summary, raw artifact files, and captured verifier outputs
  still belong to the same accepted handoff run.
- Local verification for the captured-output hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json` and emitted
  `artifactFiles.capturedOutputs` hashes for the LakeCat replay,
  QueryGraph verify, and QueryGraph import captures;
  direct CLI check:
  `cargo run -p lakecat-cli -- qglake-verify-handoff --summary target/qglake-handoff/handoff-summary.json --json`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-cli`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Verify QGLake handoff artifact file hashes`.
  `lakecat-cli qglake-verify-handoff --summary ... [--json]` now validates the
  raw `artifacts.bundle`, `artifacts.lineageDrain`, and
  `artifacts.querygraphImportPlan` file hashes recorded in
  `handoff-summary.json`, in addition to validating the compact proof objects.
  The verifier output now includes an `artifactFiles` object with the accepted
  paths and computed SHA-256 hashes, so stale or tampered handoff artifacts
  fail locally before QueryGraph automation consumes the summary.
- Local verification for the handoff artifact-hash slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_handoff_artifact_verifier`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json` and emitted matching
  `artifactFiles` hashes for the bundle, lineage-drain response, and
  QueryGraph import plan;
  direct CLI check:
  `cargo run -p lakecat-cli -- qglake-verify-handoff --summary target/qglake-handoff/handoff-summary.json --json`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-cli`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Verify QGLake handoff summaries with LakeCat CLI`.
  `lakecat-cli qglake-verify-handoff --summary ... [--json]` validates the
  `lakecat.qglake.handoff-summary.v1` schema and compact proof boundary,
  including QueryGraph verify/import agreement, LakeCat replay agreement,
  request identity, QueryGraph bootstrap, governed scan, table commit-history,
  view receipt-chain, storage-profile, and credential-vending proof objects.
  `scripts/qglake-handoff-local.sh` now runs that verifier after writing
  `handoff-summary.json` and captures
  `target/qglake-handoff/lakecat-handoff-verify.json` as an accepted artifact.
- Local verification for the handoff-summary verifier slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli parses_qglake_verify_handoff_command`;
  `cargo test -p lakecat-cli qglake_handoff_summary_verifier`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, then ran
  `lakecat-cli qglake-verify-handoff --json` over the written summary and
  emitted `lakecat.qglake.handoff-verification.v1` with matching table/view
  counts, standards, request identity proof, and QueryGraph bootstrap proof;
  direct CLI check:
  `cargo run -p lakecat-cli -- qglake-verify-handoff --summary target/qglake-handoff/handoff-summary.json --json`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-cli`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Lift QueryGraph bootstrap proof into handoff summary`.
  `lakecat-cli qglake-verify-replay --json` now emits structured
  `replay-evidence.queryGraphBootstrap`, and
  `scripts/qglake-handoff-local.sh` now writes
  `lakecatReplayVerification.queryGraphBootstrapProof` in
  `handoff-summary.json`, proving QueryGraph bootstrap/import hashes,
  table/view artifact counts, policy count, standards, agent delegation and
  summary signature hashes, view-version receipt hashes, and replay/OpenLineage
  sink hashes without requiring QueryGraph/operators to parse the full replay
  tree.
- Local verification for the QueryGraph bootstrap proof slice is green:
  `cargo fmt -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay through
  `qglake-verify-replay`, ran QueryGraph `lakecat-verify` and
  `lakecat-import`, and wrote
  `lakecatReplayVerification.queryGraphBootstrapProof` to
  `target/qglake-handoff/handoff-summary.json` with matching
  bundle/graph/OpenLineage/QueryGraph import hashes, one policy binding, one
  view-version receipt hash, and agent delegation/summary signature hashes;
  direct Node summary check for
  `lakecatReplayVerification.queryGraphBootstrapProof`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-cli`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Expose request identity source in handoff proof`.
  Lineage drain responses and event summaries now carry sanitized
  request-identity source plus optional TypeDID envelope/proof hashes. The
  QGLake replay JSON and handoff summary lift those fields into
  `lakecatReplayVerification.requestIdentityProof`, so QueryGraph/operators can
  distinguish the current agent-header fixture path from future
  TypeDID-envelope runs without seeing raw proof material.
- Local verification for the request-identity source proof slice is green:
  `cargo fmt -p lakecat-api -p lakecat-service -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-service lineage_drain_endpoint_replays_querygraph_bootstrap_outbox`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay through
  `qglake-verify-replay`, ran QueryGraph `lakecat-verify` and
  `lakecat-import`, and wrote
  `lakecatReplayVerification.requestIdentityProof` with
  `requestIdentitySource: x-lakecat-agent-did`, `requestIdentityState:
  unverified`, and null TypeDID hash slots for the local fixture;
  direct Node summary check for the request identity proof source/hash fields;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-cli`;
  `git diff --check`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Lift request identity proof into handoff summary`.
  `lakecat-cli qglake-verify-replay --json` now emits structured
  `replay-evidence.requestIdentity`, and `scripts/qglake-handoff-local.sh` now
  writes `lakecatReplayVerification.requestIdentityProof` in
  `handoff-summary.json`, proving the accepted replay principal, principal
  kind, explicit request-identity state, and authorization receipt hash before
  QueryGraph import accepts the artifact set.
- Local verification for the compact handoff request-identity proof slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay through
  `qglake-verify-replay`, ran QueryGraph `lakecat-verify` and
  `lakecat-import`, and wrote
  `lakecatReplayVerification.requestIdentityProof` to
  `target/qglake-handoff/handoff-summary.json`. The local fixture records
  `requestIdentityState: unverified` for the agent-header path, so the proof
  intentionally requires an explicit state and receipt hash rather than
  overclaiming TypeDID verification;
  direct Node summary check for
  `lakecatReplayVerification.requestIdentityProof`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Lift view receipt-chain proof into handoff summary`.
  `lakecat-cli qglake-verify-replay --json` now emits structured
  `replay-evidence.views`, and `scripts/qglake-handoff-local.sh` now writes
  `lakecatReplayVerification.viewReceiptChainProof` in
  `handoff-summary.json`, proving QueryGraph-accepted view versions, accepted
  receipt hashes, tombstone receipt hashes, namespace receipt-chain hashes,
  verified-chain counts, and replay/OpenLineage hashes.
- Local verification for the compact handoff view receipt-chain proof slice is
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `bash -n scripts/qglake-handoff-local.sh`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `git diff --check`;
  `scripts/qglake-handoff-local.sh`. The live handoff generated one table and
  one view, drained 26 outbox events, verified LakeCat replay through
  `qglake-verify-replay`, ran QueryGraph `lakecat-verify` and
  `lakecat-import`, and wrote
  `lakecatReplayVerification.viewReceiptChainProof` to
  `target/qglake-handoff/handoff-summary.json`;
  direct Node summary check for
  `lakecatReplayVerification.viewReceiptChainProof`;
  `docs/book/build.sh`;
  `scripts/check-local-dependency-contract.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Lift table commit-history proof into handoff summary`.
  `scripts/qglake-handoff-local.sh` now writes
  `lakecatReplayVerification.tableCommitHistoryProof` in
  `handoff-summary.json`, proving the pointer-log read replayed with commit
  count, sequence numbers, commit hashes, and replay/OpenLineage hashes.
- Local verification for the compact handoff table commit-history proof slice
  is green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed. The live
  handoff generated one table and one view, drained 26 outbox events, verified
  LakeCat replay through `qglake-verify-replay`, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, and wrote
  `lakecatReplayVerification.tableCommitHistoryProof` to
  `target/qglake-handoff/handoff-summary.json`;
  direct Node summary check for
  `lakecatReplayVerification.tableCommitHistoryProof`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-cli qglake_commit_history_replay_line_summarizes_verified_evidence`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Lift governed scan proof into handoff summary`.
  `scripts/qglake-handoff-local.sh` now writes
  `lakecatReplayVerification.governedScanProof` in `handoff-summary.json`,
  proving governed scan planning and scan-task fetch replay with plan, file,
  delete-file, and child plan-task counts plus replay/OpenLineage hashes.
- Local verification for the compact handoff governed-scan proof slice is
  green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed. The live
  handoff generated one table and one view, drained 26 outbox events, verified
  LakeCat replay through `qglake-verify-replay`, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, and wrote
  `lakecatReplayVerification.governedScanProof` to
  `target/qglake-handoff/handoff-summary.json`;
  direct Node summary check for
  `lakecatReplayVerification.governedScanProof`;
  `docs/book/build.sh`;
  `cargo test -p lakecat-cli qglake_scan_replay_line_summarizes_verified_evidence`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`.
- Latest completed implementation slice:
  `Lift credential vending proof into handoff summary`.
  `scripts/qglake-handoff-local.sh` now writes
  `lakecatReplayVerification.credentialVendingProof` in
  `handoff-summary.json`, proving the restricted agent credential probe
  returned zero raw credentials with the Sail-planned-read block reason while
  the trusted human probe used the audited raw-credential exception and both
  paths carried replay/OpenLineage hashes.
- Local verification for the compact handoff credential-vending proof slice is
  green:
  `bash -n scripts/qglake-handoff-local.sh`;
  `scripts/qglake-handoff-local.sh` with local socket binding allowed. The live
  handoff generated one table and one view, drained 26 outbox events, verified
  LakeCat replay through `qglake-verify-replay`, ran QueryGraph
  `lakecat-verify` and `lakecat-import`, and wrote
  `lakecatReplayVerification.credentialVendingProof` to
  `target/qglake-handoff/handoff-summary.json`;
  direct Node summary check for
  `lakecatReplayVerification.credentialVendingProof`;
  `docs/book/build.sh`;
  `docs/book/check_epub_metadata.sh docs/book/dist/lakecat.epub 'lakecat (0.1.0)'`;
  `scripts/check-local-dependency-contract.sh`;
  `cargo test -p lakecat-cli qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain`;
  `cargo test -p lakecat-cli`;
  `cargo test --workspace --all-features`.
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
