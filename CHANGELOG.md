# Changelog

## Unreleased

- Required QGLake bootstrap OpenLineage acceptance to verify the LakeCat
  producer, OpenLineage schema URL, and semantic-bundle table/view counts
  before accepting the QueryGraph handoff bundle.
- Added the QueryGraph handoff standards to the OpenLineage semantic-bundle
  facet and required QGLake bootstrap acceptance to verify those standards in
  OpenLineage, not only in the bundle manifest.
- Required QGLake bootstrap acceptance to prove the QueryGraph manifest
  advertises the expected Iceberg REST, Croissant, CDIF, OSI handoff, ODRL,
  Grust catalog graph, and OpenLineage standards before writing the bundle.
- Extended QGLake governed `fetchScanTasks` acceptance to follow every child
  manifest plan-task token returned by manifest-list expansion, proving each
  terminal manifest fetch remains governed.
- Extended QGLake governed `fetchScanTasks` acceptance to fetch the child
  manifest plan-task token and verify terminal manifest expansion still returns
  governed data-file scan work under the table location.
- Required the QGLake governed `fetchScanTasks` verifier to prove manifest-list
  expansion returns at least one child Iceberg REST plan-task token and a
  LakeCat manifest child task, keeping acceptance on the standard multi-step
  planning path.
- Required the QGLake governed scan-plan verifier to prove the plan exposes at
  least one Iceberg REST plan-task token and a LakeCat manifest-list plan task,
  ensuring acceptance starts from manifest-backed planning before task fetch.
- Required the QGLake governed scan and `fetchScanTasks` verifiers to prove the
  response was planned by Sail's REST-model engine (`sail-rest-models`), so the
  acceptance path cannot pass with a non-Sail planner identity.
- Required the QGLake governed `fetchScanTasks` verifier to prove the fetched
  residual read restriction still carries the narrowed allowed-column set,
  preventing `raw_payload` from reappearing during task materialization.
- Required the QGLake governed `fetchScanTasks` verifier to prove fetched
  Iceberg data-file paths remain under the fixture table location, rejecting
  escaped or wrong-table scan work.
- Required the QGLake governed `fetchScanTasks` verifier to prove at least one
  fetched file scan task carries an Iceberg REST `data-file.file-path`, so
  placeholder task JSON cannot satisfy the acceptance proof.
- Required the QGLake governed `fetchScanTasks` verifier to prove Sail expanded
  the plan-task token into at least one fetched file scan task, not only a
  residual policy proof.
- Required QGLake fixture reruns to preflight local snapshot manifest-list
  files referenced by existing fixture metadata before accepting a table for
  governed plan/fetch verification.
- Required QGLake fixture reruns to validate that an existing table's advertised
  local `metadata_location` JSON file exists and matches the Iceberg metadata
  returned by the catalog before accepting the table.
- Made the QGLake local fixture write the Iceberg table metadata JSON at its
  advertised `metadata_location`, keeping the bootstrap pointer usable by
  standard metadata consumers as well as LakeCat's inline REST response.
- Made `lakecat-cli qglake-fixture` create fetchable local Iceberg manifest
  metadata for its bootstrap table, so the QGLake acceptance verifier exercises
  a real plan-task token and governed `fetchScanTasks` proof instead of a
  schema-only table.
- Stamped governed scan and credential-vend authorization receipts with a
  deterministic top-level `policy_hash` derived from enforced
  `ReadRestriction` policy hashes, preserving any underlying governance-engine
  hash as an input.
- Surfaced the re-applied governed `ReadRestriction` in Iceberg REST
  `fetchScanTasks` responses and extended the QGLake verifier to require the
  governed scan to produce a plan-task token whose fetch response carries the
  same policy hash proof.
- Wired the in-process Sail `CatalogProvider` namespace drop path to LakeCat's
  governed durable namespace deletion, including typed `namespace.drop`
  capability validation, `if_exists` handling, and explicit rejection of
  unsupported cascading drops.
- Required the QGLake governed scan verifier to prove the enforced
  `ReadRestriction` carries the expected ODRL policy hash, so acceptance now
  binds projection and row-filter enforcement to the bootstrapped policy
  document.
- Added durable typed view columns and wired the in-process Sail
  `CatalogProvider` view bridge to create, load, list, and drop LakeCat
  `ViewRecord` values with `TableKind::View` status conversion for QueryGraph
  bootstrap.
- Added governed Iceberg REST namespace load/drop routes on unprefixed and
  warehouse-prefixed catalog paths, with memory/Turso persistence, typed
  `namespace.load` / `namespace.drop` capabilities, non-empty namespace guards,
  and audited `namespace.dropped` graph/lineage projection.
- Added governed durable view deletion on management and warehouse-prefixed
  catalog REST paths, with memory/Turso persistence, a typed `view.drop`
  capability, and audited `view.dropped` events.
- Exercised TypeSec-gated production secret-ref handling for `vault://`,
  `aws-sm://`, `gcp-sm://`, and `azure-kv://`, proving each accepted provider
  authorizes the exact secret URI before failing closed when no resolver backend
  is configured.
- Added catalog-path view REST aliases for listing, loading, and upserting
  durable views under
  `/catalog/v1/{warehouse}/namespaces/{namespace}/views`, with governed
  `view.load` authorization and audited Iceberg REST `view.*` events.
- Added project-scoped warehouse management routes for listing and upserting
  warehouses under `/management/v1/projects/{project}/warehouses`, using the
  existing durable Warehouse records without changing standard Iceberg REST
  table access routes.
- Enforced warehouse-to-project attachment in memory and Turso stores, with
  governed warehouse management rejecting warehouses that point at missing
  projects while preserving standard Iceberg table access routes.
- Added optional `server-id` attachment for durable project records, with
  memory/Turso validation that rejects projects pointing at missing servers and
  management responses that expose the Server > Project link.
- Added governed durable server records with management list/upsert endpoints,
  memory/Turso persistence, and audited `server.*` events, starting the
  architecture's Server > Project > Warehouse hierarchy.
- Added stored view projections to QueryGraph bootstrap bundles, including
  manifest view artifact hashes, view-aware graph edges, OpenLineage view counts,
  service-level export, and verification coverage.
- Added governed durable view records with management list/upsert endpoints,
  memory/Turso persistence, and audited outbox-backed `view.*` events as the
  next Lakekeeper-style tenancy entity after Project and Warehouse.
- Routed commit metadata object writes and orphan cleanup through
  `object_store::parse_url_opts`, keeping local `file://` behavior while moving
  the commit writer toward configured object-store backends.
- Made `lakecat-cli qglake-fixture` probe the restricted table's
  `loadCredentials` response and fail unless LakeCat withholds raw credentials,
  proving QGLake acceptance uses governed Sail-planned reads for restricted data.
- Blocked raw credential vending when an authorization receipt carries
  fine-grained row or column read restrictions, forcing those principals through
  governed Sail-planned reads and auditing the blocked credential attempt.
- Required warehouse-prefixed Iceberg REST catalog routes to resolve a durable
  `WarehouseRecord`, preventing catalog operations under unregistered warehouse
  prefixes while preserving unprefixed default-warehouse compatibility.
- Added warehouse-prefixed Iceberg REST catalog routes for config, namespace,
  table, commit, scan-plan, fetch-scan-tasks, and credential access while
  preserving the existing unprefixed default-warehouse routes.
- Allowed management APIs to route by the requested warehouse instead of the
  configured default warehouse, so operators can manage multiple durable
  warehouses from one LakeCat service.
- Added durable project records with governed management list/upsert endpoints,
  Turso persistence, and outbox-drained `Project` graph anchors for QueryGraph
  tenancy bootstrap.
- Added durable warehouse records with management list/upsert endpoints,
  TypeSec-governed warehouse management authorization, Turso persistence, and
  outbox-drained `Warehouse` graph anchors for QueryGraph tenancy bootstrap.
- Projected table metadata graph summaries from durable outbox replay into
  stable catalog-facing `Column` and `Snapshot` events, giving QueryGraph schema
  and snapshot anchors while leaving graph traversal semantics in Grust.
- Projected resolved non-anonymous outbox principals into LakeCat's
  catalog-facing graph sink as stable `Principal` events, giving QueryGraph
  actor anchors without moving traversal semantics into LakeCat.
- Projected `table.commit` outbox events into LakeCat's catalog-facing graph
  sink as stable `Commit` events keyed by table and committed sequence number,
  preserving metadata pointer movement and idempotency hashes for replay.
- Projected scan-planning outbox events into LakeCat's catalog-facing graph sink
  as stable `ScanPlan` events derived from durable outbox IDs, preserving the
  governed read restriction payload for QueryGraph replay.
- Projected `policy-binding.upserted` outbox events into LakeCat's
  catalog-facing graph sink as stable `Policy` events carrying ODRL and
  authorization payloads for QueryGraph replay.
- Projected `namespace.created` outbox events into LakeCat's catalog-facing graph
  sink with stable namespace subjects and authorization payloads, extending the
  durable graph replay path beyond table events.
- Added verified QueryGraph bootstrap bundle, graph, OpenLineage, standards, and
  table hash evidence to the `querygraph.bootstrap` audit/outbox payload so
  lineage replay carries the same integrity facts as the manifest.
- Added a QueryGraph bootstrap `graph-hash` manifest entry, verified graph hash
  validation, and made `lakecat-cli qglake-fixture` require the fixture table's
  graph node and namespace edge before writing the bundle.
- Extended the governed lineage-drain response with delivered event types plus
  graph and lineage projection counts, and made `lakecat-cli qglake-fixture`
  require `querygraph.bootstrap` lineage replay in the drain summary.
- Added embedded memory-store audit/outbox delivery parity for catalog audit
  events and made `lakecat-cli qglake-fixture` fail if the lineage drain
  delivers zero events, so local QGLake acceptance proves replay actually
  happened.
- Added a governed `/management/v1/lineage/drain` endpoint plus
  `lakecat-cli lineage-drain`, and made `lakecat-cli qglake-fixture` drain the
  lineage/outbox stream after writing the verified QueryGraph bootstrap bundle.
- Projected `querygraph.bootstrap` outbox events into LakeCat OpenLineage
  output events, preserving the bootstrap authorization/request-identity
  payload so QueryGraph acceptance runs can replay catalog-level bootstrap
  lineage alongside table scan lineage.
- Added QGLake-specific QueryGraph bootstrap verification to
  `lakecat-cli qglake-fixture`, proving the exported bundle carries the
  enforced fixture policy binding, restricted ODRL material, and OpenLineage
  output before writing the bootstrap file.
- Made `lakecat-cli qglake-fixture` repeatable: namespace and table creation
  now tolerate existing resources only after loading and validating that they
  match the expected QGLake fixture shape, while storage profile and policy
  setup remain idempotent upserts.
- Added a live governed scan-plan verification to `lakecat-cli qglake-fixture`,
  proving the fixture policy narrows `raw_payload` out of the effective
  projection and carries the policy row predicate before exporting the bootstrap.
- Exported stored table-scoped `PolicyBinding` documents through the
  QueryGraph bootstrap table projection and manifest hashes, so `/querygraph/v1/bootstrap`
  carries the actual LakeCat ODRL policy used for governed planning.
- Made `lakecat-cli qglake-fixture` install an enforceable
  `lakecat:read-restriction` with allowed columns, row predicate, and credential
  TTL, plus a restricted raw payload column so the fixture proves governed
  projection narrowing.
- Surfaced governed scan-task fetch `read-restriction`, storage location, and
  metadata location at the top level of `table.scan-tasks-fetched` audit/outbox
  payloads, and routed fetched scan-task events through the existing graph and
  OpenLineage scan projection sink path.
- Surfaced governed scan-planning `read-restriction`, storage location, and
  metadata location at the top level of `table.scan-planned` audit/outbox
  payloads, and proved OpenLineage carries the restriction through the LakeCat
  catalog dataset facet.
- Surfaced governed credential-vending `read-restriction` and
  `lakecat:raw-credential-exception` markers at the top level of the
  `credentials.vend-attempted` audit/outbox payload, matching the nested
  authorization receipt context for QueryGraph and lineage consumers.
- Attached policy-derived `ReadRestriction` context to credential-vending
  authorization receipts and marked governed raw credential requests as explicit
  LakeCat raw-credential exceptions for audit and issuer decisions.
- Added `typesec-local` RBAC policy loading for the service binary via
  `LAKECAT_TYPESEC_RBAC_POLICY`, using TypeSec's `RbacEngine` through
  `TypeSecGovernanceEngine` instead of embedding RBAC semantics in LakeCat.
- Extended `ReadRestriction` ODRL parsing to accept max credential TTL from
  nested read-restriction objects and ODRL constraints, compose multiple TTLs to
  the shortest governed lifetime, and reject malformed non-numeric TTL values.
- Routed REST `sail-local` `fetch-scan-tasks` through
  `LakeCatCatalogProvider`, so plan-task expansion now uses the same
  provider-owned scan authorization and shared `ReadRestriction` mandatory
  projection/filter requirements before delegating to Sail.
- Routed REST `sail-local` scan planning through the in-process
  `LakeCatCatalogProvider` seam, so the REST endpoint now exercises the same
  provider-owned authorization and shared `ReadRestriction` projection/filter
  application before delegating to Sail.
- Added and tracked the LakeSail book under `docs/book/`, with a TypeSec-style
  publishing pipeline, EPUB metadata validation, and generated PDF/EPUB/MOBI
  artifacts explaining the LakeCat/Sail catalog foundation for QueryGraph.
- Added the first server-owned governed read restriction: enforced policy
  bindings can now provide allowed scan columns, table-scan capabilities carry
  the resulting `ReadRestriction`, and scan planning intersects client
  projection with the policy before calling Sail.
- Added governed row-predicate extraction from enforced ODRL policy bindings:
  LakeCat now carries policy predicates in `ReadRestriction`, composes multiple
  predicates with `and`, and appends them as mandatory Sail scan filters.
- Bound Sail plan-task tokens to the governed read surface by embedding the
  effective projection alongside filters and revalidating `fetch-scan-tasks`
  against the current server-derived restriction before expanding plan tasks.
- Added a TypeSec-backed governance composition hook so LakeCat can use
  TypeSec's priority fallback semantics, letting delegated ODRL-style policy
  decisions fall through to an RBAC-style policy engine instead of becoming an
  implicit catalog denial.
- Wired REST table commits to the store's idempotency replay path via the
  `x-lakecat-idempotency-key` header, with conservative header validation and
  a service test proving duplicate keyed commits produce a single pointer-log
  record.
- Added bounded cleanup for local metadata objects written during commit
  planning when the subsequent catalog pointer commit fails, preventing stale
  CAS/rejected commits from leaving orphaned `file://` metadata JSON behind.
- Added audit-safe idempotency evidence to table commit records, audit payloads,
  and outbox payloads by persisting only the idempotency key SHA-256 hash when
  a keyed commit is accepted.
- Hardened idempotency-key replay so REST commits compare a normalized hash of
  the original Iceberg commit request and the memory/Turso stores reject reused
  keys with different commit bodies as conflicts.
- Moved ODRL read-restriction parsing/composition into `lakecat-security` so
  the REST service and future in-process provider scan path share one
  governance primitive for allowed columns, row predicates, purpose, TTL, and
  policy hashes.
- Moved governed projection narrowing, stats-field narrowing, and mandatory
  row-filter extraction onto `ReadRestriction`, keeping the scan restriction
  application logic reusable outside the REST service.
- Added a `LakeCatCatalogProvider::authorize_table_scan` seam that mints
  provider-side scan capabilities with policy-binding context and shared
  `ReadRestriction` enforcement, preparing provider-routed reads without
  duplicating REST policy logic.
- Added provider-side governed scan planning through `LakeCatCatalogProvider`,
  applying the shared `ReadRestriction` projection and mandatory filters before
  delegating to the configured Sail engine.
- Changed the QueryGraph bootstrap OSI artifact from a LakeCat-authored semantic
  model into a stable OSI handoff: LakeCat now publishes dataset/field anchors
  and governed Sail/LakeCat source metadata while leaving metrics, dimensions,
  joins, ontology claims, and authoritative semantic names to QueryGraph.
- Fixed the temporary Sail patch bridge to pass absolute patch paths to
  `git am` after `git -C sail` changes directories.
- Fixed the temporary Sail patch bridge path for the GitHub Actions workspace
  layout.
- Fixed the temporary Sail patch bridge to supply an explicit `git am`
  committer identity in GitHub Actions.
- Added a temporary manual-CI bridge that applies the LakeCat-required Sail
  helper/model API patches to the `lakehq/sail` checkout before building,
  keeping the helper implementation in Sail-shaped patches until those commits
  are available from an upstream branch.
- Recorded the manual CI run after the `protoc` fix: the remaining cloud
  failures are due to LakeCat depending on local Sail helper commits that are
  not present in the workflow's `lakehq/sail@main` checkout.
- Added `protobuf-compiler` installation to the manual GitHub Actions workflow
  so Sail's `prost-build` code generation can find `protoc` in cloud test
  jobs.
- Scoped the manual GitHub Actions formatting check to LakeCat workspace
  packages so sibling Grust/Sail/TypeSec rustfmt drift cannot fail the LakeCat
  cloud gate before tests run.
- Expanded the manual-only GitHub Actions matrix to cover the current
  TypeSec 0.8 service path and the Grust Cypher catalog-graph boundary without
  re-enabling automatic push/PR triggers.
- Added `GOAL.md` as the durable working goal for continuing LakeCat from the
  current design documents and repository state.
- Updated `STATUS.md` after pushing the Grust Cypher and TypeSec 0.8
  reconciliation slice.
- Recorded the Grust Cypher and TypeSec 0.8 reconciliation commit in
  `STATUS.md`.
- Updated LakeCat's local TypeSec baseline to `typesec` 0.8.0 and enabled
  Grust's `cypher` facade feature for `grust-local`, with a boundary test that
  runs a Grust Cypher mutation over LakeCat's catalog graph projection without
  adding graph query logic to LakeCat.
- Clarified `STATUS.md` to track the latest pushed implementation slice instead
  of making status-only commits self-referential.
- Updated `STATUS.md` after pushing the TypeDID verifier slice to LakeCat and
  the supporting TypeSec attestation commit to TypeSec.
- Recorded the TypeSec-backed TypeDID verifier slice and supporting TypeSec
  attestation commit in `STATUS.md`.
- Added a TypeSec-backed TypeDID envelope verifier seam for `typesec-local`:
  LakeCat can now verify a protected TypeDID envelope through TypeSec, authorize
  as the verified DID subject, and persist only audit-safe attestation context
  plus envelope hashes without raw payloads or signatures.
- Recorded the pushed scan-planning helper integration commit in `STATUS.md`.
- Validated LakeCat's Sail-backed scan-planning and fetch-scan-tasks output
  through Sail's exported Iceberg REST planning-result helpers, keeping the
  standard response shape Sail-owned while LakeCat retains its extension fields.
- Recorded the pushed LakeCat helper-reuse commit and the blocked Sail upstream
  push status in `STATUS.md`.
- Reused Sail's exported Iceberg `LoadTableResult` to `TableStatus` helper in
  the in-process LakeCat `CatalogProvider`, leaving only LakeCat-specific
  stable-id/version properties and v4 extension fallback logic local.
- Switched the `grust-local` catalog graph sink to Grust's LakeCat
  catalog-event projection helper, preserving outbox event ids as graph event
  vertices and keeping graph taxonomy out of LakeCat.
- Promoted reusable LakeCat catalog graph envelope ingestion into Grust and
  updated QueryGraph to validate LakeCat imports through the Grust adapter
  instead of growing graph mechanics in LakeCat.
- Verified the LakeCat-generated QGLake bundle through QueryGraph's
  `lakecat-import` path, which now checks the outer bundle hash and writes a
  QueryGraph import plan without moving graph ingest mechanics into LakeCat.
- Added `lakecat-cli qglake-fixture`, a repeatable live-service setup command
  that creates a demo namespace/table, storage profile, ODRL policy binding, and
  verified QueryGraph bootstrap bundle through LakeCat APIs.
- Added `lakecat-cli` admin commands for local governed demo setup:
  storage-profile list/upsert and ODRL policy-binding list/upsert now call the
  management API using the same typed payloads as the service.
- Added predictable local runtime controls for the service binary:
  `LAKECAT_WAREHOUSE` and `LAKECAT_BIND_ADDR`, plus a `lakecat-cli config`
  command that validates and prints the Iceberg REST config response.
- Added a `lakecat-cli bootstrap-export` command that fetches
  `/querygraph/v1/bootstrap`, verifies the manifest hashes with the reusable
  `lakecat-querygraph` verifier, and writes the bundle for QueryGraph import.
- Added a QueryGraph bootstrap manifest with stable per-table hashes for the
  Croissant, CDIF, OSI, ODRL, and OpenLineage artifacts LakeCat exports, giving
  QueryGraph an import-verification contract without moving graph logic into
  LakeCat.
- Added the first production external secret-store backend: TypeSec-authorized
  `vault://` credential refs can now resolve through Vault HTTP using
  `LAKECAT_VAULT_ADDR` / `LAKECAT_VAULT_TOKEN` (or `VAULT_ADDR` /
  `VAULT_TOKEN`) without storing raw secrets in catalog rows.
- Disabled automatic GitHub Actions CI triggers while LakeCat waits for the
  Grust and TypeSec published-crate chain; CI is now manual-only until the cloud
  dependency graph is known to work.
- Added TypeSec-gated production secret-ref dispatch for credential vending:
  `vault://`, `aws-sm://`, `gcp-sm://`, and `azure-kv://` references are now
  authorized against TypeSec by exact URI before failing with explicit
  "provider backend not configured" errors, while `typesec://env/VARIABLE`
  remains the local resolver path.
- Documented the cloud CI publish gate: LakeCat should rebuild in GitHub Actions
  against published Grust and TypeSec crates after their release chain lands,
  instead of pinning CI to unpublished sibling checkout states.
- Fixed GitHub Actions to check out the Grust branch that matches LakeCat's
  `grust-graph` 0.9.0 path dependency, preventing CI from testing against the
  older default-branch Grust 0.8.1 checkout.
- Added an environment-backed `typesec://env/VARIABLE` secret-ref resolver for
  the `typesec-local` credential issuer, letting TypeSec-authorized local runs
  vend scoped short-lived credential config without storing raw secrets in
  catalog rows.
- Rejected unsupported Sail `UNIQUE` table constraints in the in-process Iceberg
  provider instead of silently dropping them from generated LakeCat metadata.
- Added nested Iceberg type projection to the in-process Sail provider,
  including struct/list/map parsing into Arrow/DataFusion types and nested field
  id allocation when Sail creates Iceberg metadata.
- Added Iceberg identifier-field projection to the in-process Sail provider:
  Sail primary-key constraints are now written as Iceberg schema
  `identifier-field-ids`, and loaded Iceberg identifier fields are exposed as
  Sail `CatalogTableConstraint::PrimaryKey`.
- Fixed `validate_secret_ref` not re-running on the `upsert_storage_profile`
  path: all three store implementations (no-op default, in-memory, Turso) now
  call `StorageProfile::validate()` before persisting, closing the bypass where
  a profile reconstructed via serde deserialization could be re-stored without
  any validation.
- Fixed `validate_secret_ref` keyword blocklist missing common embedded-secret
  patterns: `api_key=`, `apikey=`, `access_key=`, `private_key=`, `pass=`, and
  `auth=` are now rejected alongside the existing `password=`, `secret=`,
  `token=`, and `credential=` patterns.
- Fixed Iceberg sort-order direction parsing accepting only the 4-char
  abbreviation `"desc"`: `table_sort_fields` now matches both `"desc"` and
  `"descending"` (case-insensitive) so externally-written Iceberg metadata using
  the verbose form is read correctly; fields with an absent or unrecognised
  direction are skipped rather than silently defaulted to ascending.
- Fixed `create_table` in the in-process Sail provider always writing
  `"default-sort-order-id": 1` even for unsorted tables: Iceberg spec §4.1.2
  reserves id 0 for the unsorted order; a non-zero id implies intentional
  sorting and caused clients that issue `assert-default-sort-order-id: 0` on
  subsequent commits to receive a 409 Conflict. Unsorted tables now write id 0;
  the id-0 sentinel entry is included in `sort-orders` in all cases.
- Fixed `TypeSecCredentialIssuer` silently returning `Ok(vec![])` for
  `secret_ref` URIs that use a scheme other than `typesec://` (e.g.
  `vault://`, `aws-sm://`): these schemes pass `validate_secret_ref` at profile
  creation time but were not handled by the TypeSec issuer, returning an empty
  credential list with HTTP 200 rather than surfacing the misconfiguration.  The
  issuer now returns `InvalidArgument` for unsupported schemes.
- Fixed `create_table` handler deriving the stored principal from the
  pre-authorization identity instead of `capability.receipt().principal`:
  the principal embedded in `TableRecord` and the `table.created` audit event
  now consistently comes from the governance receipt, matching all other
  request handlers.
- Fixed `request_identity` computing `content_hash_bytes` twice on the same
  Bearer token bytes: the SHA-256 is now computed once and reused for both the
  principal subject string and the `bearer-token-sha256` envelope field.
- Fixed `x-lakecat-typedid` not selecting `PrincipalKind::Agent`: a caller
  sending only `x-lakecat-typedid` (without `x-lakecat-agent-did`) fell through
  to `Principal::anonymous()` because the principal-selection chain only checked
  `agent_did`. The TypeDID value was captured in the audit envelope but never
  used for authorization, so TypeSec policy ran against the wrong subject.
  `x-lakecat-typedid` is now an independent Agent-principal selector with
  `x-lakecat-agent-did` taking precedence when both headers are present.
- Added Iceberg default sort-order projection to the in-process Sail provider
  so LakeCat `TableStatus.sort_by` reflects Iceberg sort metadata.
- Added a `typesec-local` credential issuer that gates `typesec://` secret-ref
  credential resolution through TypeSec `credentials.issue` policy checks before
  returning scoped short-lived credential config.
- Added a pluggable credential issuer on `LakeCatState`; the default issuer keeps
  remote profiles empty, while integrations can mint scoped short-lived
  credentials from governed storage-profile secret references.
- Added external secret-store references to governed storage profiles, including
  `short-lived-secret-ref` issuance metadata while keeping remote credential
  responses empty until a real issuer is wired in.
- Added sanitized TypeDID/agent request envelopes to authorization receipts so
  governed audit/outbox records carry durable identity context without storing
  raw proof, delegation, bearer-token, or signature material.
- Added basic Iceberg partition-spec projection to the in-process Sail provider
  so Sail `TableStatus` includes partition fields and partition column flags.
- Added Iceberg current-schema column projection to the in-process Sail provider
  so LakeCat tables expose useful Sail `TableStatus` columns.
- Added Sail `CatalogProvider::get_table_commits` support backed by LakeCat's
  memory/Turso metadata pointer log.
- Added a feature-gated in-process Sail `CatalogProvider` bridge that lets Sail
  resolve governed LakeCat namespaces and tables without a REST hop.
- Added governed table restore, including a management restore endpoint,
  table-scoped restore capability, memory/Turso soft-delete removal, and
  durable `table.restored` audit/outbox records with OpenLineage projection.
- Reconciled the architecture and OPUS1 working-plan docs with the governed
  table soft-delete implementation.
- Added governed table soft deletion, including catalog `DELETE` handling,
  memory/Turso soft-delete records, hidden deleted tables in normal reads, and
  durable audit/outbox projection for `table.deleted`.
- Reconciled the architecture and OPUS1 working-plan docs with the governed
  ODRL policy-binding management implementation.
- Added governed policy-binding management for ODRL documents, with memory/Turso
  persistence and active table bindings attached to authorization context.
- Reconciled the architecture and OPUS1 working-plan docs with the governed
  storage-profile management implementation.
- Added governed management endpoints and durable store support for warehouse
  storage profiles, including longest-prefix profile selection for credential
  responses.
- Updated the architecture and OPUS1 working-plan docs to mark storage-profile
  modeling as started while keeping remote credential issuance and management
  APIs as pending work.
- Added a typed storage-profile model for credential vending, returning scoped
  no-secret `file://` profile hints while keeping remote object-store credentials
  empty until short-lived issuance is implemented.
- Reconciled the OPUS1 working-plan and architecture docs with the current
  implementation status for Turso CAS commits, local object metadata writes,
  durable outbox draining, OpenLineage projection, and remaining storage-profile,
  Sail Tier-1, TypeDID, and Grust-taxonomy work.
- Added catalog-level OpenLineage projection in `lakecat-lineage`, including
  OpenLineage event hashing in the default lineage sink for outbox-drained
  table and namespace operations.
- Added HMAC-signed Sail plan-task envelopes for new scan-planning tokens while
  keeping legacy unsigned structured tokens decodable for compatibility.
- Added a capability-gated Iceberg REST table credentials endpoint that audits
  credential-vending attempts and returns no raw storage secrets until storage
  profiles can issue short-lived credentials safely.
- Added a GitHub Actions Rust CI matrix for default workspace tests plus
  `sail-local`, `typesec-local`, `grust-local`, `turso-local`, and all-features
  rows, with sibling Sail/Grust/TypeSec checkouts matching LakeCat path deps.
- Removed inline graph and lineage side effects from catalog request handlers;
  durable outbox events are now the delivery path for table/namespace
  graph-lineage projections.
- Added a service-level outbox drain that projects durable
  `lakecat.lineage-and-graph` events into the graph and lineage sinks and marks
  them delivered after successful sink projection.
- Added typed catalog-config and namespace capabilities plus durable
  `catalog.config-read`, `namespace.created`, and `namespace.listed`
  audit/outbox events for the remaining catalog-scope read/write paths.
- Added a typed graph-read capability and durable `querygraph.bootstrap`
  audit/outbox event so QueryGraph bootstrap reads carry a replayable governance
  proof without moving graph behavior into LakeCat.
- Added a typed table-create capability and durable `table.created` audit/outbox
  events so table creation is governed and replayable through the catalog outbox.
- Added a typed table-commit capability so metadata commits must carry a minted
  governance proof before Sail prepares metadata and Turso advances the pointer.
- Added a typed table-load capability and durable `table.loaded` audit/outbox
  events so metadata reads leave the same governance trail as governed scans.
- Added durable audit/outbox recording for governed scan-task fetches, carrying
  the table-scan capability receipt, plan-task token, and materialized task
  counts.
- Added durable Turso audit/outbox recording for governed scan planning events,
  including the typed table-scan authorization receipt and Sail plan summary.
- Required the typed table-scan capability for fetch-scan-tasks as well as scan
  planning, keeping task materialization on the governed Sail read path.
- Added a typed table-scan capability and routed scan planning through a helper
  that requires the minted governance proof before invoking Sail.
- Persisted commit authorization receipts into Turso audit and outbox payloads,
  keeping TypeSec governance decisions attached to the durable commit record.
- Added a Turso concurrent commit regression that exercises two writers racing on
  the same metadata pointer and verifies compare-and-swap admits only one.
- Added a typed catalog outbox drain API and Turso implementation so commit
  events can be fetched by sink and marked delivered without coupling graph or
  lineage side effects to the request path.
- Added local `file://` object-store metadata writes for commit plans that carry
  new metadata, keeping the Sail-prepared metadata JSON, the written metadata
  object, and the Turso CAS table record in sync.
- Added a LakeCat `metadata-location` commit extension that the Sail-facing
  commit plan validates alongside standard Iceberg REST requirements and threads
  through Turso pointer CAS; aligned the local Grust path dependency with the
  current Grust 0.9 workspace.
- Added metadata pointer compare-and-swap enforcement to Turso commits, including
  expected previous pointer tracking, pointer movement, idempotent replay, and a
  stale-pointer conflict regression test.
- Scaffolded LakeCat as a Rust workspace for an Iceberg-compatible catalog and
  QueryGraph foundation.
- Added REST catalog handlers, typed principal resolution, and integration seams
  for Sail, TypeSec, Grust, OpenLineage, OSI, Croissant, ODRL, and QueryGraph
  bootstrap projection.
- Added Sail-backed scan planning for local Iceberg metadata, including
  structured table-bound plan-task tokens, incremental append planning, delete
  file references, and conservative bounds pruning.
- Added a Rust-native Turso local durable catalog store behind `turso-local`,
  including namespaces, table records, metadata pointer log, idempotency replay,
  audit events, and outbox events.
- Added repo guidance in `AGENTS.md`: push graph behavior into Grust, Iceberg
  and planning work into Sail, governance into TypeSec, and describe each
  logical unit in this changelog before committing.
