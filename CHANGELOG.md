# Changelog

## Unreleased

- Extended standard catalog replay coverage for denied or incomplete
  authorization receipt decisions. `namespace.listed`, `namespace.created`,
  `namespace.loaded`, and `namespace.dropped` now have direct coverage in the
  standard catalog allowed-decision regression before acknowledgement, graph
  projection, OpenLineage projection, QGLake proof, or QueryGraph import can
  inherit unauthorized namespace evidence.
- Added QueryGraph bootstrap replay coverage for denied or incomplete
  authorization receipt decisions. `querygraph.bootstrap` outbox admission now
  has a direct regression proving a missing or false `allowed` decision fails
  before acknowledgement, graph projection, OpenLineage projection, QGLake
  proof, or QueryGraph import can inherit unauthorized bootstrap evidence.
- Added view receipt-chain replay coverage for denied or incomplete
  authorization receipt decisions. `view.version-receipt-chains-listed`
  outbox admission now has a direct regression proving a missing or false
  `allowed` decision fails before acknowledgement, graph projection,
  OpenLineage projection, QGLake proof, or QueryGraph import can inherit
  unauthorized view-history chain evidence.
- Added view receipt-list replay coverage for denied or incomplete
  authorization receipt decisions. `view.version-receipts-listed` outbox
  admission now has a direct regression proving a missing or false `allowed`
  decision fails before acknowledgement, graph projection, OpenLineage
  projection, QGLake proof, or QueryGraph import can inherit unauthorized
  view-history evidence.
- Added management-list replay coverage for denied or incomplete authorization
  receipt decisions. `server.listed` outbox admission now has a direct
  regression proving a missing or false `allowed` decision fails before
  acknowledgement, graph projection, OpenLineage projection, QGLake proof, or
  QueryGraph import can inherit unauthorized inventory evidence.
- Bound fetched scan replay `stats-fields` to effective stats evidence. When
  `table.scan-tasks-fetched` replay carries `stats-fields`, service outbox
  admission now rejects empty, duplicate, or drifted arrays before graph,
  OpenLineage, QGLake, or QueryGraph import can inherit unverified stats-field
  claims.
- Hardened fetched scan replay plan-task evidence. `table.scan-tasks-fetched`
  outbox admission now rejects non-string, non-LakeCat, decorated, or
  credential-bearing `plan-task` values before acknowledgement, graph
  projection, OpenLineage projection, QGLake proof, or QueryGraph import can
  inherit token/path claims.
- Expanded the book with a workflow-facing catalog concept guide. The new
  section walks PySpark/Spark reads and commits, operator inspection, governed
  service reads, agent access, and QueryGraph bootstrap through standard
  Iceberg vocabulary versus LakeCat implementation proof, TypeSec governance
  evidence, QueryGraph integration surfaces, and future engine-neutral profile
  candidates, while reinforcing that Sail should own reusable table-format
  interpretation.
- Bound catalog config-read tenant-root replay roots to their hash evidence.
  `catalog.config-read` replay now applies the same hash binding to optional
  `server-record.endpoint-url` and `warehouse-record.storage-root` values that
  management-upsert replay already applies, so config discovery evidence cannot
  project raw tenant roots unless the matching full hash proof is present.
- Refreshed the release-readiness record after the latest book and handoff
  slices. The quick local gate is green for dependency-contract shell syntax,
  workflow trigger syntax, QGLake handoff script syntax, release script syntax,
  the local dependency contract, manual workflow trigger contract, workspace
  formatting, and `git diff --check`.
- Added a front-loaded book chapter on catalog concepts, standards, and engine
  ownership. It now directly classifies the Rust service spine, Turso store,
  Iceberg REST paths, commit CAS, idempotency/pointer-log/audit/outbox replay
  hardening, governed scan and credential receipts, QueryGraph/QGLake proof
  surfaces, and typed Iceberg v4 posture as standard Iceberg vocabulary,
  LakeCat implementation, TypeSec/QueryGraph extension, or possible future
  optional profile material. The chapter also makes the detailed case for
  pushing table-format interpretation, planning, metadata-as-data, commit
  validation, and v4 behavior into Sail.
- Aligned the README with the latest QGLake handoff contract. It now points
  readers to the LakeCat book for expanded workflow guidance and documents that
  saved handoff artifact manifests are both bundle-local and schema-closed
  before archived artifact hashes or captured-output semantics are trusted.
- Closed QGLake handoff summary artifact manifests in the CLI verifier. The
  primary `artifacts` object, its nested `capturedOutputs` manifest, and each
  bundle/lineage/import/captured-output artifact object now reject unexpected
  sibling fields before hashing or parsing archived files, preventing a saved
  handoff summary from carrying unverified artifact or alternate-hash claims
  beside otherwise valid bundle-local `path`/`sha256` evidence.
- Expanded the LakeCat book's catalog-concepts chapter with workflow-focused
  explanations for PySpark clients, platform operators, governed agents, and
  QueryGraph/QGLake import. The new text more explicitly separates standard
  Iceberg compatibility from LakeCat reliability extensions, TypeSec governance
  evidence, QueryGraph semantic handoff surfaces, and future optional
  engine-neutral profile candidates, and it strengthens the argument that Sail
  should own reusable table interpretation so LakeCat proofs are tied to engine
  facts rather than catalog-local approximations.
- Restored the full local release-readiness gate. QGLake CLI fixture tests now
  use full deterministic view receipt hashes instead of stale short
  `sha256:` placeholders, and the handoff script now binds readiness
  `catalog.config-read` proof to the QGLake agent, writes the canonical
  handoff-verifier output artifact before verification, keeps
  `managementProof` and `storageProfileUpsertProof` separated, and preserves
  nested policy-upsert proof in the management summary. The full
  `scripts/check-release-readiness.sh` gate passes locally again.
- Deepened the book's catalog-concepts chapter with sharper terminology
  boundaries for standard Iceberg parlance versus LakeCat catalog-control
  extensions, TypeSec governance evidence, QueryGraph/QGLake semantic proof
  surfaces, and future engine-neutral Iceberg profile candidates. The expanded
  text now explicitly explains why Rust and Turso are implementation choices,
  why CAS is standard but idempotency/audit/outbox/replay hardening is a
  catalog reliability envelope, why governed scan and credential receipts must
  stay additive, and why reusable table-format interpretation should move into
  Sail.
- Tightened raw QGLake lineage-drain view proof verification. View replay
  sink receipt hashes, tombstone view-receipt hashes, namespace receipt-chain
  hashes, and receipt-chain replay/OpenLineage hashes must now be full
  SHA-256-shaped values before archived handoff proof can use them; short
  `sha256:` placeholders are rejected.
- Added QGLake handoff artifact path traversal regression coverage. The CLI
  verifier now has focused tests proving both artifact hash verification and
  captured-output semantic readers reject relative `..` paths that resolve
  outside the handoff summary directory, keeping archived handoff artifacts
  bundle-local before hashing or parsing.
- Expanded the book's first-release catalog concept explanation with an
  owner-first decision rule: Iceberg terms stay standard, LakeCat owns the
  catalog-control envelope, TypeSec owns governance evidence, Grust owns graph
  mechanics, QueryGraph owns QGLake/semantic integration, and Sail owns
  reusable table-format interpretation. The new text also distinguishes local
  product architecture from future Iceberg-adjacent profile candidates such as
  proof-carrying scans, replay-admissible catalog events, pointer history, and
  governed credential proof.
- Closed authorization receipt context policy-binding entries over the
  service-produced policy response fields. Standard catalog replay now rejects
  unexpected policy-binding context claims before acknowledgement, graph
  projection, OpenLineage projection, QGLake proof, or QueryGraph import proof
  can inherit unverified ODRL, scope, enforcement, delegation, or application
  evidence beside checked policy context.
- Closed shared authorization receipt context replay schemas over the context
  keys LakeCat compares. Standard catalog replay now rejects unexpected
  context-level claims before acknowledgement, graph projection, OpenLineage
  projection, QGLake proof, or QueryGraph import proof can inherit unverified
  restriction, raw-credential, request-identity, delegation, or application
  evidence beside checked authorization receipt context.
- Closed shared authorization receipt and principal replay schemas over the
  fields LakeCat compares. Standard catalog replay now rejects unexpected
  receipt-level or principal-level claims before acknowledgement, graph
  projection, OpenLineage projection, QGLake proof, or QueryGraph import proof
  can inherit unverified actor, authorization, TypeDID, delegation, token, or
  policy evidence beside checked receipt fields.
- Closed QueryGraph bootstrap request-identity replay evidence over the known
  request-identity envelope fields. `querygraph.bootstrap` now rejects
  unexpected request-identity claims inside authorization receipt context before
  acknowledgement, graph projection, OpenLineage projection, QGLake proof, or
  QueryGraph import proof can inherit unverified actor, TypeDID, delegation,
  attestation, token, or agent claims beside checked hash evidence.
- Closed service outbox admission over top-level table commit replay payloads.
  `table.commit` now rejects unexpected sibling fields beside the checked table
  identity, optional scope hints, authorization receipt, and nested commit
  evidence before acknowledgement, graph projection, OpenLineage projection, or
  QGLake proof can inherit unverified commit, policy, storage, graph, lineage,
  QueryGraph, or application claims.
- Closed service outbox admission over top-level catalog config and QueryGraph
  bootstrap replay payloads. `catalog.config-read` and `querygraph.bootstrap`
  now reject unexpected top-level payload fields before acknowledgement, graph
  projection, OpenLineage projection, or QGLake proof can inherit unverified
  compatibility, endpoint, authorization, artifact, standards, graph,
  OpenLineage, QueryGraph, or application claims beside checked config,
  tenant-root, bootstrap, and receipt evidence.
- Rebuilt and verified the LakeCat book artifacts after the catalog-concepts
  expansion. The current book already covers the Rust service spine, Turso
  store direction, Iceberg REST paths, commit CAS, idempotency, pointer logs,
  audit/outbox, replay validation, TypeSec-style governed scan and credential
  receipts, QueryGraph/QGLake/OpenLineage proof surfaces, the
  standard-versus-extension-versus-future-profile distinction, and the argument
  for pushing table-format work into Sail.
- Closed service outbox admission over top-level credential-vend replay
  payloads. `credentials.vend-attempted` now rejects unexpected top-level
  payload fields before acknowledgement, graph projection, OpenLineage
  projection, or QGLake proof can inherit unverified credential, storage-scope,
  authorization, issuer, graph, OpenLineage, QueryGraph, or application claims
  beside checked table, read-restriction, raw-credential exception,
  storage-profile, response evidence, and authorization proof.
- Closed service outbox admission over top-level table lifecycle replay
  payloads. `table.created`, `table.loaded`, `table.deleted`, and
  `table.restored` now reject unexpected top-level payload fields before
  acknowledgement, graph projection, OpenLineage projection, or QGLake proof can
  inherit unverified lifecycle, storage, lineage, graph, QueryGraph, or
  application claims beside checked table identity, version, format-version,
  location, soft-delete, metadata-graph summary, and authorization evidence.
- Closed service outbox admission over top-level storage-profile upsert replay
  payloads. `storage-profile.upserted` now rejects unexpected top-level payload
  fields before acknowledgement, graph projection, OpenLineage projection, or
  QGLake proof can inherit unverified storage-profile, credential-root,
  governance, lineage, graph, QueryGraph, or application claims beside checked
  warehouse, redacted storage-profile object, and authorization evidence.
- Expanded the book's catalog-concepts explanation with a stricter
  standard-versus-extension-versus-future-profile decision rule. The book now
  more thoroughly separates standard Iceberg catalog/table vocabulary from
  LakeCat implementation choices, TypeSec governance evidence, QueryGraph/QGLake
  proof surfaces, and reusable profile candidates, and it sharpens the argument
  for pushing table-format, governed-scan, metadata-as-data, and v4 semantics
  into Sail instead of building a shadow engine inside LakeCat.
- Closed service outbox admission over top-level view lifecycle replay
  payloads. `view.upserted`, `view.loaded`, and `view.dropped` now reject
  unexpected top-level payload fields before acknowledgement, graph projection,
  OpenLineage projection, or QGLake proof can inherit unverified view lifecycle,
  lineage, graph, QueryGraph, or application claims beside checked view scope,
  version, expected-version, interface, and authorization evidence.
- Closed service outbox admission over top-level management upsert replay
  payloads. `project.upserted`, `server.upserted`, and `warehouse.upserted` now
  reject unexpected top-level payload fields before acknowledgement, graph
  projection, OpenLineage projection, or QGLake proof can inherit unverified
  tenant-root, endpoint, storage-root, lineage, graph, QueryGraph, or
  application claims beside checked route identity, nested record, optional
  project scope, and authorization evidence.
- Closed service outbox admission over top-level policy-binding upsert replay
  payloads. `policy-binding.upserted` now rejects unexpected top-level payload
  fields before acknowledgement, graph projection, OpenLineage projection, or
  QGLake proof can inherit unverified ODRL, governance, scope, lineage, graph,
  QueryGraph, or application claims beside checked warehouse, policy object,
  ODRL content hash, enforcement state, and authorization evidence.
- Closed service outbox admission over top-level table commit-history replay
  payloads. `table.commits-listed` now rejects unexpected top-level payload
  fields before acknowledgement, graph projection, OpenLineage projection, or
  QGLake proof can inherit unverified commit, pointer, lineage, graph,
  QueryGraph, or application claims beside checked table scope, count,
  sequence, commit hash, principal, and authorization evidence.
- Closed service outbox admission over top-level governed scan replay payloads.
  `table.scan-planned` and `table.scan-tasks-fetched` now reject unexpected
  top-level payload fields before acknowledgement, graph projection,
  OpenLineage projection, or QGLake proof can inherit unverified scan, lineage,
  graph, QueryGraph, or application claims beside checked restriction,
  projection, stats, filter, task-count, and authorization evidence.
- Closed service outbox admission over top-level view receipt read payloads.
  `view.version-receipts-listed` and
  `view.version-receipt-chains-listed` now reject unexpected top-level payload
  fields before acknowledgement, graph projection, OpenLineage projection, or
  QGLake proof can inherit unverified view-history, lineage, graph,
  QueryGraph, or application claims beside checked receipt hashes, chain hashes,
  tombstone counts, warehouse/namespace scope, and authorization evidence.
- Closed service outbox admission over namespace lifecycle replay payloads.
  `namespace.created`, `namespace.loaded`, and `namespace.dropped` now reject
  unexpected top-level payload fields before acknowledgement, graph projection,
  OpenLineage projection, or QGLake proof can inherit unverified namespace,
  scope, replay, or lineage claims beside checked warehouse, namespace, and
  authorization evidence.
- Added front-of-book guidance that points readers from the release vocabulary
  to the detailed standard-versus-extension claim ledger and Sail engine-boundary
  argument, so the Rust spine, Turso store, Iceberg REST paths, CAS,
  idempotency, pointer logs, audit/outbox, replay validation, governed receipt
  evidence, and QueryGraph proof surfaces are easier to read as either standard
  Iceberg parlance, LakeCat/TypeSec/QueryGraph extensions, or future optional
  profile candidates.
- Closed service outbox admission over list-event replay payloads.
  `namespace.listed`, `view.listed`, and management list events now reject
  unexpected top-level payload fields before acknowledgement, graph projection,
  OpenLineage projection, or QGLake proof can inherit unverified inventory,
  scope, replay, or lineage claims beside checked count and ID evidence.
- Replayed storage-profile public config now fails closed on reserved or
  secret-like keys and values. `storage-profile.upserted` and
  `credentials.vend-attempted` outbox admission reject public config that would
  shadow LakeCat credential evidence before acknowledgement, graph projection,
  OpenLineage projection, or QGLake credential-root proof.
- Closed service outbox admission over individual table commit evidence.
  `table.commit` replay now rejects unexpected fields inside the nested
  `commit` object before acknowledgement, graph projection, OpenLineage
  projection, or QGLake commit proof can inherit unverified pointer-transition
  claims.
- Closed service outbox admission over raw credential exception evidence.
  Governed credential-vending replay now rejects unexpected fields inside
  top-level `lakecat:raw-credential-exception` and the matching authorization
  receipt context object before acknowledgement, graph projection,
  OpenLineage projection, or QGLake credential proof can inherit unverified
  raw-credential claims.
- Added a first-release catalog-concepts chapter to the book that explicitly
  delineates standard Iceberg vocabulary from LakeCat implementation terms,
  TypeSec governance terms, and QueryGraph/QGLake integration terms. The new
  chapter classifies the Rust service spine, Turso local store direction,
  Iceberg REST namespace/table paths, commit CAS, idempotency, pointer logs,
  audit/outbox, replay validation, governed scan and credential receipts,
  QueryGraph/OpenLineage/bootstrap/management/view/credential/commit proof
  surfaces, and typed Iceberg v4 posture as standard compatibility, local
  extension, governance/application integration, or narrow future
  Iceberg-adjacent profile candidates. It also gives a concrete argument for
  pushing table-format, scan-planning, metadata-as-data, commit-validation, and
  v4 semantics into Sail, with concrete PySpark, Sail-planned, governed-agent,
  and QueryGraph/QGLake handoff examples.
- Closed service outbox admission over table lifecycle identity and soft-delete
  schemas. Table lifecycle replay now rejects unexpected fields inside full
  table identity objects and soft-delete evidence before acknowledgement, graph
  projection, OpenLineage projection, or QGLake proof can inherit unverified
  table-scope or delete-state claims.
- Closed service outbox admission over view receipt-chain entry schemas.
  `view.version-receipt-chains-listed` replay now rejects unexpected fields
  inside nested receipt-chain and receipt objects before acknowledgement, graph
  projection, OpenLineage projection, or QGLake proof can inherit unverified
  view-history claims.
- Closed service outbox admission over QueryGraph bootstrap entry schemas.
  `querygraph.bootstrap` replay now rejects unexpected fields inside
  `table-artifacts`, `view-artifacts`, and `view-version-receipts` entries
  before acknowledgement, graph projection, OpenLineage projection, or QGLake
  handoff proof can inherit unverified semantic artifact or view receipt
  claims.
- Closed service outbox admission over catalog config key/value entries.
  `catalog.config-read` replay now rejects unexpected fields inside `defaults`
  and `overrides` entries before acknowledgement, graph projection,
  OpenLineage projection, or QGLake config proof can inherit unverified
  compatibility, v4 bridge, or integration-discovery claims.
- Closed service outbox admission over nested view lifecycle evidence.
  `view.upserted`, `view.loaded`, and `view.dropped` replay now reject
  unexpected fields inside their nested `view` object before acknowledgement,
  graph projection, OpenLineage projection, or QGLake proof can inherit
  unverified view lifecycle claims.
- Closed service outbox admission over nested management record evidence.
  `project.upserted`, `server.upserted`, and `warehouse.upserted` replay now
  reject unexpected fields inside their nested record objects before
  acknowledgement, graph projection, OpenLineage projection, or QGLake proof
  can inherit unverified tenant-root, endpoint, or storage-root claims.
- Closed service outbox admission over nested policy-binding upsert evidence.
  `policy-binding.upserted` replay now rejects unexpected fields inside the
  nested `policy` object before acknowledgement, graph projection,
  OpenLineage projection, or QGLake proof can inherit unverified ODRL,
  governance, scope, or enforcement claims.
- Closed service outbox admission over nested storage-profile evidence.
  `storage-profile.upserted` replay and `credentials.vend-attempted` replay
  now reject unexpected fields inside their nested `storage-profile` object
  before acknowledgement, graph projection, OpenLineage projection, or QGLake
  proof can inherit unverified credential-root or storage-scope claims.
- Expanded the book's catalog-boundary chapter with a detailed concept ledger
  that classifies the Rust service spine, Turso local store, Iceberg REST
  routes, commit CAS, idempotency, pointer logs, audit/outbox, replay
  validation, governed scan and credential paths, QueryGraph/QGLake handoff,
  OpenLineage, bootstrap, management, view, credential, and commit proof
  surfaces as standard Iceberg parlance, LakeCat implementation, additive
  TypeSec/QueryGraph governance and integration, or narrow future
  Iceberg-adjacent profile candidates. The chapter now also spells out what
  "push work into Sail" means operationally for REST models, manifests,
  pruning, deletes, metadata-as-data, commit validation, governed planning,
  and typed v4 support.
- Closed service outbox admission over credential-response evidence entries.
  Governed credential-vending replay now rejects unexpected fields inside each
  `credential-response-evidence` entry before acknowledgement, graph
  projection, OpenLineage projection, or QGLake credential proof can inherit
  unverified credential-scope claims.
- Closed service outbox admission over governed read-restriction schemas.
  `table.scan-planned`, `table.scan-tasks-fetched`, and governed
  credential-vending replay now reject unexpected top-level
  `read-restriction` fields and nested `row-predicate` fields before outbox
  acknowledgement, graph projection, OpenLineage projection, or later QGLake
  proof can inherit unverified restriction claims.
- Added a dedicated book chapter that maps catalog concepts by boundary:
  standard Iceberg clients, LakeCat catalog authority, Sail engine
  interpretation, TypeSec governance, Grust graph mechanics, and
  QueryGraph/QGLake workflow composition. The chapter gives a durable ASCII
  concept diagram, explains why Rust and Turso are implementation choices
  rather than Iceberg extensions, distinguishes commit CAS from LakeCat's
  idempotency/pointer-log/audit/outbox/replay envelope, and sharpens the
  argument that reusable table-format, scan-planning, metadata-as-data, commit
  validation, and typed v4 work belongs in Sail.
- Closed nested QGLake governed-scan restriction proof objects over their
  verified schema. Compact and captured planned/fetched read restrictions now
  reject unexpected fields, and their row-predicate children reject unexpected
  predicate claims, before handoff verification can accept unverified purpose,
  policy, predicate, projection, or credential-scope evidence inside a matched
  Sail-planned read proof.
- Closed compact and captured QGLake QueryGraph bootstrap proof objects over
  their compared schema. `queryGraphBootstrapProof` and captured LakeCat
  replay `queryGraphBootstrap` evidence now reject unexpected fields before
  summary or sidecar verification can accept unverified bundle/import,
  artifact-count, standards, identity, TypeDID, authorization, delegation,
  view-receipt, replay, or OpenLineage claims beside checked bootstrap proof.
- Expanded the book's release concept deep dive with a workflow-oriented
  explanation of the same catalog concepts across PySpark, Rust/Sail,
  governed-agent, and QueryGraph/QGLake paths. The new section explicitly
  separates standard Iceberg catalog/table parlance from LakeCat reliability
  proof, TypeSec governance receipts, Sail-owned engine interpretation, Grust
  graph mechanics, and QueryGraph application workflow evidence.
- Closed compact and captured QGLake view receipt-chain proof objects over
  their verified schema. `viewReceiptChainProof`, captured LakeCat replay
  `views`, and nested accepted-view, tombstone, receipt-chain group,
  structural chain, and receipt objects now reject unexpected fields before
  summary or sidecar verification can accept unverified view lifecycle,
  tombstone, receipt-chain, principal, replay, or OpenLineage claims beside
  checked structural view proof.
- Closed compact and captured QGLake management proof objects over their
  compared schema. `managementProof`, captured LakeCat replay `management`,
  and nested `policyUpsertProof` evidence now reject unexpected fields before
  summary or sidecar verification can accept unverified tenant, project,
  warehouse, policy, storage-profile list, policy-upsert, authorization,
  replay, or OpenLineage claims beside checked management proof. Captured
  `warehouseProjectId` is now matched with compact summary scope evidence.
- Expanded the book's catalog concept guide with a reader-facing walkthrough
  of the Rust service spine, Turso local store, Iceberg REST compatibility,
  commit CAS, idempotency, pointer logs, audit/outbox, replay validation,
  TypeSec-governed scan and credential receipts, QueryGraph/QGLake proof
  surfaces, and the exact line between LakeCat extensions and possible future
  Iceberg-adjacent optional profiles. The same section makes the Sail-first
  engine argument explicit for table-format interpretation, governed scan
  planning, commit validation, metadata-as-data, and typed v4 behavior.
- Closed compact and captured QGLake credential-vending proof objects over
  their compared schema. `credentialVendingProof`, its restricted/trusted-human
  branches, and their nested storage-profile anchors now reject unexpected
  fields before summary or sidecar verification can accept unverified raw
  credential, storage-scope, authorization, replay, or OpenLineage claims
  beside checked TypeSec-style credential decisions.
- Closed compact and captured QGLake storage-profile upsert proof objects over
  their compared schema. `storageProfileUpsertProof` and captured LakeCat
  replay `management.storageProfileUpsert` evidence now reject unexpected
  fields before summary or sidecar verification can accept unverified
  credential-root, provider, secret-reference, authorization, graph, replay, or
  OpenLineage claims beside checked storage-profile management proof.
- Closed compact and captured QGLake governed-scan proof objects over their
  compared schema. `governedScanProof` and captured LakeCat replay `scan`
  evidence now reject unexpected fields before summary or sidecar verification
  can accept unverified scan-planning, restriction, projection, stats,
  replay-hash, or OpenLineage claims beside checked Sail-planned read proof.
- Closed compact and captured QGLake request-identity proof objects over their
  compared schema. `requestIdentityProof` and captured LakeCat replay
  `requestIdentity` evidence now reject unexpected fields before summary or
  sidecar verification can accept unverified actor, identity-source, TypeDID,
  authorization, or drain-read action claims beside checked identity proof.
- Closed compact and captured QGLake catalog-config proof objects over their
  compared schema. `catalogConfigProof` and captured LakeCat replay
  `catalogConfig` evidence now reject unexpected fields before summary or
  sidecar verification can accept unverified v4 bridge, endpoint, authorization,
  graph, replay, or OpenLineage compatibility claims beside checked config-read
  proof.
- Closed compact and captured QGLake table commit-history proof objects over
  their compared schema. `tableCommitHistoryProof` and captured LakeCat replay
  `tableCommitHistory` evidence now reject unexpected fields before summary or
  sidecar verification can accept unverified pointer-log claims beside checked
  counts, sequences, hashes, principals, authorization receipts, graph events,
  replay hashes, and OpenLineage hashes.
- Added a detailed release concept deep dive to the book. The new chapter
  directly delineates Rust service spine, Turso local store, Iceberg REST
  paths, commit CAS, idempotency, pointer logs, audit/outbox, replay
  validation, governed scan/credential receipts, and QueryGraph/QGLake proof
  surfaces as standard Iceberg behavior, LakeCat/TypeSec/QueryGraph extensions,
  or narrow future Iceberg-adjacent profile candidates, and strengthens the
  argument for pushing table-format work into Sail.
- Closed saved QGLake self-verifier artifact hash leaf objects over `sha256`
  only. Nested bundle, lineage-drain, QueryGraph import-plan, and captured
  LakeCat/QueryGraph output hash objects now reject unexpected fields before
  comparing with the compact handoff summary, preventing sidecars from attaching
  alternate unverified hash claims to accepted artifact evidence.
- Closed saved QGLake self-verifier semantic sections over their compared
  fields. LakeCat replay semantics, QueryGraph verify/import semantics, bundle
  artifact semantics, import-plan semantics, and lineage-drain semantics now
  reject unexpected fields so saved sidecars cannot carry unverified semantic
  proof beside the values LakeCat actually compares.
- Closed saved QGLake self-verifier proof schemas over their known keys. The
  verifier now rejects unexpected top-level
  `lakecatHandoffVerifyOutput` fields and unexpected
  `capturedOutputSemantics` sections, so a saved sidecar cannot append
  unverified proof claims that no verifier compares to the compact handoff
  summary.
- Closed saved QGLake self-verifier artifact manifests over their known key
  set. `lakecatHandoffVerifyOutput.artifactFiles` now rejects unexpected
  top-level artifact keys and unexpected nested captured-output keys before
  comparing hashes with the compact handoff summary, preventing a saved sidecar
  from carrying unverified artifact claims beside accepted evidence.
- Expanded the book's catalog-concept chapter with a stricter standards
  vocabulary for the current implementation claims. The chapter now separates
  standard Iceberg compatibility, LakeCat control-plane extensions, TypeSec
  governance extensions, QueryGraph integration surfaces, and narrow future
  Iceberg-adjacent profile candidates, and adds an operational Sail contract
  for table-format interpretation, governed scans, fetch-task revalidation,
  metadata-as-data, commit validation, and typed v4 work.
- Reused the QGLake bundle-local artifact resolver for semantic artifact reads.
  Captured output, bootstrap bundle, QueryGraph import-plan, and lineage-drain
  semantic verification now parse only artifacts that resolve under the handoff
  summary directory, matching the hash verifier's containment rule.
- Required QGLake handoff artifact paths to resolve inside the handoff summary
  directory before hashing or semantic verification. The CLI verifier now
  rejects absolute or relative artifact-path splices that point outside the
  archived bundle even when the referenced file bytes match the declared hash.
- Required saved QGLake self-verification artifact hashes to be full SHA-256
  digests. `lakecatHandoffVerifyOutput.artifactFiles` now rejects short nested
  bundle, lineage-drain, QueryGraph import-plan, captured LakeCat/QueryGraph
  output, and service-log hashes before comparing them with the compact
  handoff summary. The book and design now spell out that this sidecar proof is
  part of the catalog-control evidence, not standard Iceberg metadata.
- Required saved QGLake self-verification output to be hash-bound. Handoff
  artifact verification now rejects missing, null, or short
  `lakecatHandoffVerifyOutputHash` values instead of treating the
  self-verifier artifact as optional once a path is present.
- Bound raw lineage-drain catalog config proof into saved QGLake
  self-verification output. `lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics`
  now carries `catalogConfigProof` from the raw drain artifact, and artifact
  verification rejects saved handoff verifier output whose config proof drifts
  from the raw lineage drain.
- Bound catalog config-read proof into compact QGLake handoff verification.
  `lakecatReplayVerification.catalogConfigProof` now carries advertised config
  defaults, overrides, endpoints, principal/action receipt proof, graph counts,
  replay hashes, and OpenLineage hashes. Compact handoff summaries and captured
  LakeCat replay sidecars now reject missing config proof, unsupported
  `lakecat.format.v4*` defaults, v4 overrides, missing standard/governed/
  QueryGraph/OpenLineage endpoints, or captured replay drift.
- Expanded the book's catalog-concepts discussion with a stricter extension/
  optional-profile/proposal distinction and a governed-read example showing why
  Sail must be the engine of record for proof-bearing scan planning and typed
  Iceberg v4 interpretation while LakeCat keeps identity, pointer state,
  policy binding, audit, outbox, and replay evidence.
- Added QGLake lineage-drain summary proof for catalog config reads. The API
  summary now carries config defaults, overrides, and advertised endpoints for
  `catalog.config-read` events; the service emits those fields after replay
  admission; and the CLI verifier rejects saved drains whose config proof drops
  the pinned v4 bridge defaults, introduces unsupported `lakecat.format.v4*`
  claims, carries v4 overrides, or omits required standard, governed-access, or
  QueryGraph/OpenLineage integration endpoints.
- Tightened compact QGLake handoff and raw lineage-drain credential-root
  verification so storage-profile provider and issuance-mode proof must remain
  compatible. Saved evidence now rejects `local-file-no-secret` on remote
  providers and `short-lived-secret-ref` on the file provider before
  QueryGraph handoff or replay import can accept contradictory credential-root
  posture.
- Expanded the LakeCat book with a front-loaded release-claims section that
  explains the Rust service spine, Turso-backed local store, Iceberg REST
  namespace/table compatibility, commit CAS, idempotency, pointer logs,
  audit/outbox, replay validation, governed scan and credential receipts,
  QueryGraph/QGLake handoff, OpenLineage, bootstrap, management, view,
  credential, and commit proof surfaces. The new material explicitly separates
  standard Iceberg parlance from LakeCat implementation, QueryGraph/TypeSec
  additions, and narrow future Iceberg-adjacent profile candidates, while
  making the case for pushing table-format interpretation and proof-bearing
  planning into Sail.
- Tightened raw QGLake bootstrap replay verification so
  `querygraph.bootstrap` replay and OpenLineage receipt hashes must be full
  SHA-256 digests, matching the compact handoff verifier and the book/design
  claim that captured QGLake replay evidence cannot use prefix-only
  placeholders.
- Tightened raw QGLake lineage-drain commit-history verification so
  non-empty `table.commits-listed` commit hashes must be full SHA-256 digests,
  matching the compact handoff verifier and route response proof while keeping
  explicit zero-count histories valid with empty arrays.
- Aligned QGLake compact and raw lineage-drain verification with the service's
  empty commit-history proof. `tableCommitHistoryProof` and
  `table.commits-listed` replay now accept explicit zero-count histories with
  empty sequence/hash arrays while retaining strict hash/sequence checks for
  present commits.
- Added service coverage for the empty table commit-history path. A
  `table.commits-listed` read over a table with no commit records now has a
  regression proving it drains as explicit zero-count proof, emits lineage
  evidence, and does not fabricate loaded commit graph nodes.
- Kept standard Iceberg table commits compatible with clients that do not send
  LakeCat's REST idempotency header. `table.commit` replay admission now
  requires request/response hash evidence and validates
  `idempotency_key_sha256` only when present, while retaining fail-closed
  coverage for malformed idempotency hashes. The service now has a regression
  proving a no-idempotency commit still drains to graph and OpenLineage.
- Added an early book vocabulary guide that cleanly separates standard Iceberg
  terms from LakeCat implementation machinery, TypeSec governance semantics,
  and QueryGraph integration surfaces. The guide explicitly answers which
  ideas are LakeCat extensions, which are future Iceberg-adjacent optional
  profile candidates, and why Iceberg table-format interpretation should move
  into Sail while LakeCat preserves the catalog authority boundary.
- Updated the README release surface to match the current verified state:
  LakeCat is described as the current implementation rather than a scaffold,
  the full local release-readiness gate is called out as green on
  2026-06-22, and the QGLake handoff harness now documents its stale-state and
  occupied-port protections.
- Preserved governed stats-field proof for `table.scan-tasks-fetched` events
  by carrying the restricted fetch projection as requested, effective, and
  compact stats-field evidence. The route and outbox tests now assert that
  stateless task fetch replay remains non-empty and bounded for QGLake lineage
  drain validation.
- Made `scripts/qglake-handoff-local.sh` clean the Turso WAL/SHM files and
  generated fixture table storage before starting a local handoff run, fail
  fast when its bind address is already occupied, and recursively stop the
  spawned service process tree on exit. This keeps the release-readiness gate
  from inheriting stale QGLake metadata pointers or talking to an orphaned
  service from a previous run.
- Refreshed the checked-in LakeCat book artifacts after the final local book
  build so the committed EPUB, MOBI, and PDF match the current generated
  output for the expanded catalog-surface chapter.
- Expanded the LakeCat book's current catalog surface explanation with a
  dedicated standard-vs-extension classification for the Rust service spine,
  Turso store, REST namespace/table paths, commit CAS, idempotency, pointer
  logs, audit/outbox, replay validation, governed scan and credential receipts,
  QueryGraph/QGLake handoff, OpenLineage, and semantic/governance vocabularies.
  The same slice strengthens the argument that LakeCat should push
  table-format interpretation and proof-bearing planning into Sail so catalog
  evidence is based on engine-shaped Iceberg facts rather than catalog-local
  JSON shortcuts.
- Bound `view.version-receipt-chains-listed` replay hash arrays to structural
  receipt-chain evidence, so declared chain, receipt, and drop-receipt hashes
  must exactly cover the nested verified chains before projection.
- Hardened `view.version-receipt-chains-listed` replay admission so verified
  view receipt chains must bind declared `receipt-count`, latest view version,
  latest operation, and tombstone state back to the actual receipt array before
  acknowledgement, graph projection, or OpenLineage projection.
- Expanded the LakeCat book's catalog-concepts chapter with an explicit
  concept ledger for standard Iceberg, LakeCat, Sail, and
  QueryGraph/TypeSec/Grust responsibilities. The chapter now classifies the
  Rust service spine, Turso-backed store, REST paths, commit CAS, idempotency,
  pointer logs, audit/outbox, replay validation, governed scan/credential
  receipts, and QGLake handoff as standard behavior, LakeCat implementation,
  additive governance/integration proof, or narrow future Iceberg-adjacent
  profile candidates. It also adds a stronger performance and correctness
  argument for pushing table-format and planning work into Sail.
- Added service replay-admission coverage proving projection receipts cannot
  carry malformed OpenLineage hashes before acknowledgement, graph projection,
  or lineage projection.
- Added service replay-admission coverage proving projection receipts cannot
  repeat replay event hashes before graph or OpenLineage projection, matching
  the existing duplicate-free OpenLineage hash guard.
- Expanded the LakeCat book with a concrete governed-agent workflow that maps
  Iceberg, TypeSec, Sail, LakeCat, and QueryGraph responsibilities across one
  request, reinforcing why table-format interpretation belongs in Sail while
  LakeCat persists catalog authority and proof.
- Added QGLake verifier coverage proving policy-binding upsert proof cannot
  shed principal subject/kind evidence in raw lineage-drain replay or compact
  handoff summaries.
- Required `credentials.vend-attempted` replay with governed
  `read-restriction` evidence to carry nonblank purpose and positive
  `max-credential-ttl-seconds` proof before acknowledgement, graph projection,
  or OpenLineage projection.
- Extended governed fetch-scan-task proof so returned residual extensions and
  durable audit/outbox replay carry requested and effective stats-field
  evidence. Service admission now rejects missing, widened, or duplicate
  fetched stats-field proof before acknowledgement, graph projection, or
  OpenLineage projection.
- Added catalog config replay coverage for duplicate advertised endpoint
  entries, proving malformed config-read evidence fails before acknowledgement,
  graph projection, or OpenLineage projection.
- Expanded the LakeCat book's catalog-concepts chapter with a reader-facing
  naming discipline and proposal filter that separates standard Iceberg
  behavior from LakeCat control-plane proof, QueryGraph/TypeSec integration,
  and future Iceberg-adjacent profile candidates. The same slice strengthens
  the argument for pushing Iceberg table-format interpretation into Sail.
- Added QGLake lineage-drain verifier coverage for missing governed scan
  authorization receipt hashes, proving fetched scan replay cannot shed the
  TypeSec-style receipt digest before compact handoff proof is accepted.
- Added QGLake lineage-drain verifier coverage for malformed governed scan
  authorization receipt hashes, proving compact scan replay cannot preserve a
  short receipt digest even when replay/OpenLineage hashes are well-shaped.
- Extended governed scan replay regressions so `table.scan-planned` and
  `table.scan-tasks-fetched` outbox admission also rejects authorization
  receipts missing action, allowed decision, engine, or `checked_at` evidence
  before acknowledgement, graph projection, or OpenLineage projection.
- Added governed scan replay regressions that prove `table.scan-planned` and
  `table.scan-tasks-fetched` outbox admission rejects denied authorization
  receipts, blank receipt engines, and malformed receipt timestamps before
  acknowledgement, graph projection, or OpenLineage projection.
- Expanded the LakeCat book's catalog concept guidance with an ownership map
  and standards filter that delineates standard Iceberg behavior, LakeCat
  implementation, Sail engine responsibilities, TypeSec governance,
  Grust graph ownership, QueryGraph/QGLake integration, and narrow future
  Iceberg-adjacent proposal candidates.
- Required generic audit authorization receipts to carry nonblank action
  evidence before memory or Turso stores persist audit rows or enqueue outbox
  work. The regressions reject payload-hash-valid audit events whose receipts
  omit the action.
- Bound generic audit authorization receipts back to the top-level audit
  principal before memory or Turso stores persist audit rows or enqueue outbox
  work. The regressions create payload-hash-valid audit events whose receipt
  principal differs from the event principal and prove audit/outbox state stays
  empty.
- Required string-form table scopes in generic audit payloads to carry
  warehouse and namespace anchors before memory or Turso stores persist audit
  rows or enqueue outbox work. The regressions reject table-scoped audit
  payloads that say only `"table": "events"`.
- Bound generic audit payload table scope to the top-level audit event table
  before memory or Turso stores persist audit rows or enqueue outbox work. The
  regressions create payload-hash-valid audit events whose payload table points
  at a different table and prove both stores keep audit/outbox state empty.
- Hardened outbox delivery acknowledgement so memory and Turso stores reject
  malformed delivery event IDs before marking rows delivered. The regressions
  attempt to acknowledge `sha256:short` after creating a real pending outbox
  event and prove the event remains pending.
- Required generic audit events to carry request-hash evidence before memory
  or Turso stores can persist audit rows or enqueue outbox work. The new
  regressions mutate constructor-valid audit events to remove `request_hash`
  and prove both stores leave audit/outbox state untouched.
- Hardened generic audit recording so memory and Turso stores reject audit
  events whose row event type no longer matches the decoded payload before
  writing audit rows or enqueueing outbox projection work. The regressions
  mutate a constructor-valid `CatalogAuditEvent` after creation and prove both
  stores keep audit/outbox state empty on drift.
- Expanded the LakeCat book with a concrete catalog-concepts-in-practice
  chapter. The new material walks PySpark, standard commit, Turso store,
  governed agent scan, credential, QueryGraph/QGLake handoff, and Sail-planned
  proof workflows while clearly separating standard Iceberg parlance from
  LakeCat implementation, TypeSec governance extensions, QueryGraph
  integration surfaces, and narrow future Iceberg-adjacent profile candidates.
- Hardened Turso soft-delete restore so durable `soft_deletes` row scope,
  metadata location, version, and timestamp columns must match the decoded
  soft-delete record before a table can be restored. The regression tampers the
  row namespace while leaving `record_json` valid, proving LakeCat refuses to
  drop corrupted soft-delete evidence.
- Hardened Turso idempotency replay so `idempotency_records.table_key` must
  match the requested table before either direct replay probing or the normal
  idempotent commit path can return a stored response. The regression tampers
  only the durable idempotency row scope while leaving the replay response JSON
  valid, proving LakeCat rejects row-scope drift before replay.
- Tightened compact QGLake handoff verification so `warehouse`, `namespace`,
  and `table` scope anchors must be non-blank, not merely non-empty. The
  regressions mirror whitespace-only scope into the compact QueryGraph
  verified-table IDs, proving blank catalog scope is rejected before QueryGraph
  handoff evidence can accept meaningless table anchors.
- Tightened compact QGLake handoff verification so the top-level accepted
  `principal` must be non-blank before request identity, QueryGraph bootstrap,
  governed scan, commit-history, or credential proof can mirror it. The
  regression rejects whitespace-only handoff principal anchors even when every
  dependent compact proof field agrees with the same whitespace value.
- Expanded the LakeCat book's current-surface explanation with a detailed
  concept ledger. The new matrix classifies the Rust service spine,
  Turso-backed store, Iceberg REST namespace/table paths, commit CAS,
  idempotency, pointer logs, audit/outbox, replay validation, governed scans,
  credential decisions, QueryGraph/QGLake handoff, OpenLineage, and
  Croissant/CDIF/OSI/ODRL surfaces as standard Iceberg parlance, LakeCat
  implementation, LakeCat/QueryGraph/TypeSec extensions, or narrow future
  Iceberg-adjacent profile candidates. It also states why table-format and
  planning work should move into Sail instead of becoming LakeCat-local
  engine logic.
- Tightened compact QGLake request-identity proof so `requestIdentitySource`
  and `requestIdentityState` must be non-blank, not merely non-empty. The
  regression rejects whitespace-only provenance before QueryGraph bootstrap
  proof can mirror it into archived handoff evidence.
- Tightened compact QGLake handoff verification so
  `queryGraphBootstrapProof.viewVersionReceiptHashes` must contain full
  SHA-256-shaped, duplicate-free receipt hashes before structural view receipt
  binding runs. The regression rejects short bootstrap receipt hashes instead
  of allowing weak `sha256:`-prefix evidence into the archived handoff proof.
- Hardened request identity admission so explicit `x-lakecat-principal-kind:
  anonymous` is rejected. Anonymous access is now represented only by omitting
  identity headers, preventing explicit subjects from entering the TypeDID,
  governance, Sail, audit, or outbox paths with anonymous principal semantics.
- Hardened request identity admission so `Authorization` cannot be combined
  with `x-lakecat-principal`, `x-lakecat-agent-did`, or
  `x-lakecat-typedid`. Mixed primary identity sources now fail before bearer
  token hashing, governance, TypeSec verification, Sail calls, audit, or outbox
  evidence, and diagnostics avoid echoing the competing principal, DID, or
  token material.
- Hardened bearer identity admission so `Authorization: Bearer ...` accepts a
  single opaque token only. Bearer values containing embedded or trailing
  whitespace are rejected before governance, TypeSec verification, Sail calls,
  audit, or outbox evidence, and diagnostics avoid echoing the token material.
- Hardened request identity admission so `x-lakecat-principal-kind` is accepted
  only with `x-lakecat-principal`. Orphan principal-kind hints are now rejected
  before bearer, agent DID, TypeDID, governance, Sail, audit, or outbox paths
  can reinterpret them, and diagnostics avoid echoing the competing token or
  DID material.
- Hardened bearer identity admission so `Authorization: Bearer` headers with
  empty or whitespace-only tokens are rejected before governance, TypeSec
  verification, Sail calls, audit, or outbox evidence. The regression keeps the
  error generic and proves LakeCat does not mint a service principal from the
  hash of an empty token.
- Expanded the LakeCat book with a claim-by-claim catalog concept ledger. The
  new section classifies the Rust service spine, Turso store, Iceberg REST
  namespace/table paths, commit CAS plus idempotency/pointer-log/audit/outbox
  hardening, TypeSec-style governed scan and credential receipts, and
  QueryGraph/QGLake/OpenLineage proof surfaces as standard Iceberg behavior,
  LakeCat implementation, governance/application extensions, or narrow future
  Iceberg-adjacent profile candidates. It also makes the Sail argument
  explicit: proof-carrying scans are credible only when field ids, projections,
  predicates, manifests, deletes, and scan tasks are interpreted by the engine
  path rather than by catalog-local JSON shortcuts.
- Hardened request identity admission so duplicate identity-bearing headers
  such as `x-lakecat-principal`, `x-lakecat-agent-did`, TypeDID headers, agent
  proof headers, or `Authorization` are rejected before governance, TypeSec
  verification, Sail calls, audit, or outbox evidence. Diagnostics identify the
  duplicated header without echoing conflicting principal, DID, token, or proof
  values.
- Hardened REST commit idempotency key admission so duplicate
  `x-lakecat-idempotency-key` headers are rejected before identity,
  authorization, Sail commit preparation, pointer movement, audit, or outbox
  evidence. The regression keeps the diagnostic generic and proves duplicate
  retry keys do not leak raw key values.
- Pinned the first-release ledger and standards-boundary book guidance in the
  local dependency contract. The contract now fails if `DESIGN.md` drops the
  release-blocking/deferred scope, the local release and QGLake proof commands,
  or the honest `typed-sail=unavailable` v4 posture, or if the book loses the
  standard/extension/proposal taxonomy, LakeCat/Sail responsibility ledger, or
  first-release readiness section.
- Expanded the front-of-book catalog concept guide with a canonical
  standard/implementation/extension/proposal taxonomy. The new text
  distinguishes standard Iceberg namespace/table/CAS semantics from LakeCat's
  Rust/Turso/idempotency/pointer-log/audit/outbox/replay implementation,
  TypeSec governance receipts, Grust graph ownership, QueryGraph/QGLake
  integration, and the narrow proof behaviors that could become future
  Iceberg-adjacent optional profiles. The Sail section now includes a compact
  LakeCat/Sail responsibility ledger for table load, governed scan,
  fetch-scan-task, commit, and metadata-as-data work.
- Added a first-release readiness ledger to the living design, README, and
  book. The new text separates release-blocking catalog substrate behavior
  from deferred Sail/Grust/TypeSec/QueryGraph work, and names the local
  evidence commands that prove the release claim: release-readiness, QGLake
  handoff, book build, and dependency-contract checks.
- Hardened `table.commit` replay admission so optional payload-level
  warehouse, namespace, and table-name scope hints must match the durable
  outbox table identity before graph/OpenLineage projection. The book now
  explains the catalog concepts in that same standards vocabulary: Rust/Turso
  are implementation choices, REST namespace/table paths and CAS are Iceberg
  compatibility, replay/audit/outbox proof is LakeCat catalog evidence,
  TypeSec receipts are governance extensions, QueryGraph/QGLake is an
  application handoff, and narrow proof behaviors are the future
  Iceberg-adjacent proposal candidates. It also adds a detailed Sail-first
  argument for keeping table-format semantics in the engine.
- Hardened the live QGLake handoff harness so compact proof extraction now
  requires replay/OpenLineage hash arrays for QueryGraph bootstrap, management,
  credentials, governed scans, table commit history, and view receipts to carry
  full SHA-256-shaped values without duplicates before the archived handoff
  summary is accepted.
- Added a book classification test for new catalog work. The new section asks
  which layer would be wrong without a feature, then routes standard Iceberg
  compatibility to the REST/table boundary, durable catalog proof to LakeCat,
  table-format semantics to Sail, authorization semantics to TypeSec, graph
  semantics to Grust, and application semantics to QueryGraph. It also narrows
  future Iceberg-adjacent proposal candidates to portable behavior profiles
  rather than LakeCat's implementation stack.
- Promoted the full local release-readiness gate from all-features workspace
  library tests to the complete `cargo test --workspace --all-features` command
  and tightened the dependency contract to pin that exact row. This keeps the
  release proof aligned with the local gate that caught QGLake fixture drift.
- Fixed QGLake accepted replay fixtures so policy-list evidence is paired with
  `policy-binding.upserted` content proof and the accepted lineage-drain
  manifest derives `delivered`, `eventTypes`, `graphEvents`, and
  `lineageEvents` from the actual replay summaries. This keeps the full local
  workspace gate aligned with the verifier's policy-upsert and event-order
  invariants.
- Expanded the book's release-ledger explanation with a standards-document
  reading guide. The new section separates Iceberg-standard namespace/table
  and CAS behavior from LakeCat Rust/Turso implementation choices,
  TypeSec-governed scan and credential extensions, QueryGraph/QGLake
  application surfaces, and narrow future Iceberg-adjacent profile candidates,
  while making the Sail-first table-semantics boundary explicit.
- Added a TypeSec-gated file-backed production secret-ref resolver for
  AWS/GCP/Azure-style `aws-sm://`, `gcp-sm://`, and `azure-kv://` providers.
  Provider roots are configured by environment variable, credential JSON files
  are addressed by the SHA-256 digest of the exact secret ref, denied TypeSec
  decisions do not read files, and parse failures remain hash-only.
- Expanded the book's Sail engine-boundary argument with a responsibility
  ledger that separates catalog authority, TypeSec governance, Sail table
  semantics, and QueryGraph handoff proof. The new text makes the engine
  pushdown rule concrete for schema/field-id interpretation, manifest metrics,
  delete handling, commit validation, v4 metadata, and replayable LakeCat proof.
- Added the `lakecat-store` no-default-feature library test to the full local
  release-readiness gate and dependency contract so warning-free default-store
  hygiene remains part of first-release proof while cloud CI is manual-only.
- Feature-gated Turso-only store row-scope validators so default and
  no-default-feature `lakecat-store` builds no longer emit dead-code warnings
  while Turso-backed row/content validation remains compiled and tested under
  `turso-local`.
- Redacted configured production secret-ref backend failures. TypeSec-gated
  external secret resolvers now wrap configured AWS/GCP/Azure-style backend
  errors with only the provider label, `secret-ref-hash`, and
  `error-detail-hash`, preventing raw secret refs, account paths, tokens, ARNs,
  or backend exception text from crossing LakeCat's credential boundary.
- Expanded the LakeCat book's catalog-concepts explanation with a
  claim-by-claim delineation of what is standard Iceberg parlance, what is
  LakeCat implementation, what belongs to TypeSec/QueryGraph extensions, and
  what might become a future optional Iceberg-adjacent profile. The book now
  makes the Sail argument more explicit: generated Iceberg REST models, schema
  and field-id interpretation, manifests, metrics, deletes, metadata-as-data,
  scan tasks, commit validation, and v3/v4 table-format interpretation belong
  in Sail so LakeCat can persist compact proof instead of becoming a shadow
  engine.
- Preserved full QGLake handoff proof surfaces in the local acceptance loop.
  The handoff script now requires and carries authorization receipt actions
  for request identity and QueryGraph bootstrap evidence, passes validated
  LakeCat replay proof objects through to the handoff summary instead of
  thinning them, and the CLI verifier now emits full management proof semantics
  for verifier-output artifact self-checks.
- Hardened lineage-drain projection receipt evidence. Outbox drains now reject
  projection receipts whose replay/OpenLineage hash arrays are count-drifted,
  malformed, or duplicate before returning raw lineage-drain summaries or
  acknowledging delivery, keeping QGLake from inheriting inflated receipt proof
  from a sink boundary.
- Expanded the book's standards/extension guidance with a direct decision test
  for what is standard Iceberg, LakeCat implementation, governance extension,
  QueryGraph application surface, or future optional Iceberg-adjacent proposal.
  The new text deepens the Sail-first argument: table-format interpretation,
  manifest metrics, delete planning, metadata-as-data, scan task generation,
  commit validation, and typed v4 work belong in Sail, while LakeCat persists
  compact proof and replayable catalog state.
- Bound credential response secret-reference hash proof. Secret-ref-backed
  credential responses now carry catalog-derived `secret-ref-hash` evidence
  beside `secret-ref-provider`, backend-supplied shadow values are replaced,
  and replay rejects missing or drifted response-level secret-ref hashes before
  acknowledgement, graph projection, or OpenLineage projection.
- Hardened store-level commit idempotency evidence. Memory and Turso stores now
  reject blank or malformed table-commit idempotency keys, reject caller-supplied
  idempotency request hashes without a key, require those request hashes to be
  full SHA-256 evidence, and apply the same shape checks to replay probes before
  pointer movement, pointer-log insertion, audit, or outbox emission.
- Expanded the book's catalog-concepts explanation with a clearer distinction
  between standard Iceberg parlance, LakeCat implementation, QueryGraph and
  TypeSec extension surfaces, possible Iceberg-adjacent proposal profiles, and
  the architectural case for pushing table-format work into Sail.
- Hardened management upsert replay for tenant roots. `server.upserted` now
  recomputes `endpoint-url-hash` from `endpoint-url`, and
  `warehouse.upserted` recomputes `storage-root-hash` from `storage-root`,
  before acknowledgement, graph projection, OpenLineage projection, or QGLake
  handoff can accept those management events.
- Hardened QueryGraph bootstrap manifest verification so table/view projections
  and table/view artifact manifests must be duplicate-free by stable id before
  LakeCat accepts the bundle as QGLake import proof. This prevents duplicated
  stable IDs from satisfying counts while weakening verified table/view
  evidence.
- Advertised QueryGraph/OpenLineage integration endpoints in catalog config
  discovery and replay evidence. `CatalogConfigResponse` now includes
  `/querygraph/v1/bootstrap` and `/management/v1/lineage/drain`, and
  config-read replay requires those additive integration surfaces before graph,
  OpenLineage, or QGLake projection can accept the config event.
- Hardened config-read governed-access endpoint replay. Service replay now
  requires advertised plan, fetch-scan-tasks, and credential endpoints for both
  default and warehouse-prefixed catalog routes before config-read evidence can
  reach graph/OpenLineage projection or QGLake handoff, keeping governed Sail
  planning and audited credential decisions visible in compatibility proof.
- Advertised standard table-create endpoints in catalog config discovery and
  replay evidence. `CatalogConfigResponse` now includes default and
  warehouse-prefixed `POST .../tables` routes, config replay requires those
  endpoints before graph/OpenLineage projection or QGLake handoff, and tests
  pin the route list so standard Iceberg table creation is not under-reported.
- Hardened catalog-config endpoint replay: `catalog.config-read` audit payloads
  now record the advertised endpoint list, service replay validates endpoint
  arrays as non-empty duplicate-free strings, and replay rejects config evidence
  that omits the standard config, namespace, table-load, or table-commit REST
  endpoints for default and warehouse-prefixed routes before graph/OpenLineage
  projection or QGLake handoff.
- Hardened catalog-config override replay: `catalog.config-read` evidence now
  validates optional `overrides` as structured string key/value entries and
  rejects any `lakecat.format.v4*` override claims before acknowledgement,
  graph projection, OpenLineage projection, or QGLake handoff. The config-read
  audit payload now records the response's override array, and the book expands
  the workflow explanation from PySpark through Sail, agents, and QueryGraph.
- Hardened catalog-config v4 bridge replay: `catalog.config-read` evidence now
  rejects unsupported extra `lakecat.format.v4*` default keys, even when the
  required `extension-ready`, `json-passthrough`, and `typed-sail=unavailable`
  claims are present. This keeps replay evidence from smuggling future typed
  Sail claims before Sail exposes stable typed v4 support.
- Hardened table lifecycle replay evidence: `table.created`, `table.loaded`,
  and `table.restored` events now carry positive Iceberg `format-version`
  evidence, and `table.deleted` carries the same table-format proof through the
  durable soft-delete record. Service replay admission rejects missing,
  non-integer, or non-positive lifecycle format-version evidence before
  acknowledgement, graph projection, OpenLineage projection, or QGLake handoff,
  while leaving actual format interpretation to Sail.
- Expanded the LakeCat book with a detailed catalog concept map that explains
  the Rust service/catalog spine, Turso-backed local store, Iceberg
  REST-compatible namespace/table paths, commit CAS hardening, governed
  scan/credential receipts, audit/outbox/OpenLineage replay, QueryGraph/QGLake
  handoff, and Iceberg v4 typed interpretation as separate standard,
  implementation, extension, and future-profile categories. The new section
  makes the Sail boundary explicit: field ids, schema and partition evolution,
  manifests, metrics, deletes, scan tasks, metadata-as-data, row lineage, and
  v4 interpretation belong in Sail rather than a LakeCat-local shadow engine.
- Hardened table commit proof at the store boundary: table and commit metadata
  now require positive Iceberg `format-version` evidence before memory or Turso
  state changes can produce durable commit records, and commits without a
  current Iceberg snapshot now emit explicit `snapshot_id: 0` proof instead of
  omitting snapshot evidence. This keeps store-produced `table.commit` outbox
  events aligned with service replay admission before graph/OpenLineage/QGLake
  projection.
- Added a dedicated book section, `The Standards Boundary For The Current
  Release`, that classifies the Rust service spine, Turso store, Iceberg REST
  paths, commit CAS hardening, governed scan/credential receipts,
  OpenLineage/outbox replay, QueryGraph/QGLake handoff, standards vocabularies,
  and typed v4 work as standard Iceberg behavior, LakeCat implementation,
  LakeCat/TypeSec/QueryGraph extensions, or future optional
  Iceberg-adjacent profile candidates. The paired Sail section makes the
  engine-first argument explicit for PySpark, Rust engine, agentic, QueryGraph,
  and v4 workflows.
- Hardened Turso namespace reads so decoded `namespace_json` must match the
  selected warehouse row and namespace path before LakeCat lists, loads, or
  drops a namespace. This keeps standard Iceberg namespace routes and QGLake
  bootstrap from consuming namespace evidence spliced from another durable row.
- Hardened active view reads so memory and Turso keyed reads, plus Turso
  namespace view lists, bind decoded `record_json` back to the selected
  warehouse, namespace, and view name before returning, updating, or dropping
  view state. This prevents active-view JSON spliced from another durable view
  row from feeding QGLake view proof or later view mutations.
- Hardened Turso server, project, and warehouse management-row reads so decoded
  `record_json` must match the selecting row identity before LakeCat returns
  tenant roots or warehouse inventory. This prevents QueryGraph/QGLake
  bootstrap and management proof paths from consuming tenant evidence spliced
  from another durable management row.
- Hardened Turso storage-profile reads so decoded `profile_json` must match the
  row/query warehouse and profile id before LakeCat lists profiles or matches a
  table to a credential root. This prevents governed credential and QGLake
  proof paths from consuming storage-profile evidence spliced from another
  durable profile row.
- Hardened Turso policy-binding reads so decoded `binding_json` must match the
  row/query warehouse and policy id before LakeCat lists policies or matches
  policies for a table. This prevents QGLake and governed scan paths from
  consuming policy evidence spliced from another durable policy row.
- Hardened Turso table record reads and idempotency replay so decoded
  `record_json` / `response_json` must match the row/query table identity
  before LakeCat returns table records, replays idempotent commit responses,
  commits over an existing row, or soft-deletes the table. This keeps standard
  Iceberg REST table access from trusting spliced durable table JSON.
- Hardened Turso view-version receipt reads and mutation history extension so
  decoded `receipt_json` must match the row/query warehouse, namespace, and
  view identity before LakeCat returns receipts, validates namespace receipt
  chains, or appends another view receipt. This rejects row/content scope drift
  before QGLake or later mutations can consume spliced view-history evidence.
- Hardened memory and Turso view mutations so they validate the existing
  durable view-version receipt chain before appending a new upsert or drop
  receipt. A forged `previous-receipt-hash` in existing durable receipt history
  now rejects later mutations before changing active view state or extending the
  receipt chain.
- Expanded the LakeCat book with a front-of-book catalog concept guide that
  explicitly classifies the Rust service spine, Turso store, Iceberg REST
  namespace/table paths, commit CAS hardening, TypeSec-governed scans and
  credentials, OpenLineage, QueryGraph/QGLake handoff, and Iceberg v4 typed
  interpretation as standard Iceberg behavior, LakeCat implementation,
  LakeCat/TypeSec/QueryGraph extensions, or future optional
  Iceberg-adjacent profile candidates. The new text makes a detailed case for
  pushing field-id projection, manifest metrics, delete handling,
  metadata-as-data, scan-task generation, row lineage, and typed v4 work into
  Sail instead of a LakeCat-local shadow engine.
- Added memory and Turso store read validation for view receipt chains, so
  forged `previous-receipt-hash` links are rejected before service replay,
  graph/OpenLineage projection, or QueryGraph/QGLake handoff can consume
  durable view-history evidence.
- Added service replay regression coverage proving `table.scan-planned` and
  `table.scan-tasks-fetched` reject mismatched authorization receipt actions
  before acknowledgement, graph projection, or OpenLineage projection. The
  design and book now state that governed scan replay must prove the
  event-matching `table-plan-scan` action, not a table-load, commit, or other
  valid catalog action.
- Added service replay regression coverage proving
  `credentials.vend-attempted` rejects a mismatched authorization receipt
  action before acknowledgement, graph projection, or OpenLineage projection.
  The design and book now state that credential-vend replay must prove the
  event-matching `credentials-vend` action, not a read, commit, or other valid
  catalog action.
- Expanded the LakeCat book with a front-loaded release vocabulary chapter
  that thoroughly classifies the Rust service spine, Turso store, Iceberg REST
  paths, commit CAS/idempotency/pointer-log/audit/outbox/replay hardening,
  TypeSec-governed scan and credential proof, OpenLineage projection, and
  QueryGraph/QGLake handoff as standard Iceberg behavior, LakeCat
  implementation, optional LakeCat/QueryGraph/TypeSec extensions, or future
  Iceberg-adjacent profile candidates. The new Sail section makes the engine
  boundary explicit for PySpark, agentic, QueryGraph, and v4 workflows.
- Added service replay regression coverage proving `querygraph.bootstrap`
  rejects a mismatched authorization receipt action before acknowledgement,
  graph projection, or OpenLineage projection. The design and book now state
  that QueryGraph bootstrap replay must prove the `graph-read` action, not a
  lineage-read or other proof.
- Added service replay regression coverage proving `table.commits-listed`
  rejects a mismatched authorization receipt action before acknowledgement,
  graph projection, or OpenLineage projection. The design and book now state
  that commit-history reads must prove the read-side `table-load` action, not a
  mutation action such as `table-commit`.
- Added service replay regression coverage proving
  `view.version-receipts-listed` and
  `view.version-receipt-chains-listed` reject mismatched authorization receipt
  actions before acknowledgement, graph projection, or OpenLineage projection.
  The design and book now state that governed view receipt read surfaces must
  prove the read-side `view-load` action.
- Added service replay regression coverage proving `namespace.listed`,
  `namespace.created`, `namespace.loaded`, and `namespace.dropped` reject
  mismatched authorization receipt actions before acknowledgement, graph
  projection, or OpenLineage projection. The design and book now state the
  namespace action contract: `namespace-list`, `namespace-create`,
  `namespace-load`, and `namespace-drop`.
- Expanded the LakeCat book's catalog-concept explanation with a sharper
  standards boundary: Rust/Turso are implementation choices, REST
  table/namespace paths and commit CAS are standard Iceberg catalog behavior,
  LakeCat replay/audit/outbox surfaces are optional catalog hardening,
  TypeSec receipts are governance extensions, QueryGraph/QGLake handoff is an
  application extension, and only small behavior profiles such as idempotent
  replay, pointer history, governed credentials, lineage binding, view proof,
  and proof-carrying scans should be treated as future Iceberg-adjacent
  candidates. The Sail argument now explains why field-id projection, manifest
  pruning, deletes, row lineage, metadata-as-data, and typed v4 interpretation
  belong in Sail rather than a catalog-side shadow engine.
- Added service replay regression coverage proving `view.upserted`,
  `view.loaded`, and `view.dropped` reject mismatched authorization receipt
  actions before acknowledgement, graph projection, or OpenLineage projection.
  The design and book now state the view lifecycle action contract:
  `view-manage`, `view-load`, and `view-drop`.
- Added service replay regression coverage proving `table.created`,
  `table.loaded`, `table.deleted`, and `table.restored` reject mismatched
  authorization receipt actions before acknowledgement, graph projection, or
  OpenLineage projection. The design and book now state that table lifecycle
  replay is bound to the matching catalog action as well as the actor.
- Added service replay regression coverage proving policy-binding, project,
  server, storage-profile, and warehouse management-list reads reject
  mismatched authorization receipt actions before acknowledgement, graph
  projection, or OpenLineage projection. The design and book now describe
  management-list proof as ID/count evidence plus an event-matching receipt
  action, not just actor evidence.
- Added service replay regression coverage proving policy-binding, project,
  server, storage-profile, and warehouse upserts reject mismatched
  authorization receipt actions before acknowledgement, graph projection, or
  OpenLineage projection. The design and book now spell out that management
  mutations need event-matching TypeSec-style receipt action evidence, not only
  a principal.
- Hardened raw QGLake management-list replay for server, project, warehouse,
  policy-binding, and storage-profile inventory: lineage drains now require
  principal subject/kind evidence, full authorization receipt hashes, and full
  replay/OpenLineage SHA-256 hashes before compact management proof can be
  built. The book and design now classify this as LakeCat/QGLake/TypeSec
  control-plane proof around standard Iceberg catalog behavior, not an Iceberg
  metadata extension.
- Bound compact QGLake `storageProfileUpsertProof` to storage-profile
  management authorization evidence, requiring principal subject/kind, full
  authorization receipt hash, and the `storage-profile-manage` action across
  raw lineage drains, captured LakeCat replay, and archived handoff summaries.
  The book now explains this as LakeCat/TypeSec credential-root governance
  evidence beside standard Iceberg, and sharpens why field-id, manifest,
  delete, pruning, credential, and typed v4 interpretation should be pushed
  into Sail.
- Bound compact QGLake `policyUpsertProof` to policy-management
  authorization evidence, requiring principal subject/kind, full authorization
  receipt hash, and the `policy-manage` action across raw lineage drains,
  captured LakeCat replay, and archived handoff summaries.
- Bound compact QGLake management proof to `policy-binding.upserted` content
  evidence: lineage drains and captured LakeCat replay now require
  `policyUpsertProof` with a listed policy id, full ODRL content hash, graph
  proof, replay hashes, and OpenLineage hashes before archived handoff proof is
  accepted.
- Expanded the LakeCat book with a release-claim ledger that separates
  standard Iceberg parlance, LakeCat Rust/Turso implementation choices,
  optional LakeCat/QueryGraph/TypeSec extensions, and future
  Iceberg-adjacent proposal candidates, plus a stronger argument that
  field-id, manifest, delete, pruning, task-lineage, and typed v4 semantics
  belong in Sail.
- Required `policy-binding.upserted` producers and replay admission to carry an
  `odrl-hash` matching the captured ODRL policy material before
  acknowledgement, graph projection, or OpenLineage projection, so QueryGraph
  policy anchors cannot drift from the policy document LakeCat recorded.
- Expanded the LakeCat book with concrete PySpark, governed scan, credential,
  and QueryGraph bootstrap walkthroughs that show exactly where standard
  Iceberg behavior ends, where LakeCat's Rust/Turso/CAS/audit/outbox spine
  begins, how TypeSec receipts and credential posture remain governance
  extensions, and why Sail should carry field-id, manifest, delete, pruning,
  and v4 table-format work.
- Required `table.deleted` replay admission to carry a soft-delete object with
  positive version evidence before acknowledgement, graph projection, or
  OpenLineage projection, so delete replay cannot drop the catalog
  pointer-generation proof.
- Required `credentials.vend-attempted` replay admission to carry top-level
  boolean `secret-ref-present` evidence matching the nested storage-profile
  proof before acknowledgement, graph projection, or OpenLineage projection,
  so credential proof cannot omit whether the selected credential root depends
  on an external secret reference.
- Required service `table.commit` replay admission to carry an RFC3339
  `committed_at` timestamp before acknowledgement, graph projection, or
  OpenLineage projection, so individual pointer-transition replay cannot drop
  when the catalog accepted the commit.
- Expanded the LakeCat book's catalog-concepts chapter with a dedicated
  extension/proposal boundary, classifying Rust/Turso as implementation
  choices, REST table/namespace paths and CAS as standard Iceberg behavior,
  LakeCat replay/audit/outbox and TypeSec receipt evidence as additive
  catalog/governance extensions, QueryGraph/QGLake as application handoff, and
  only the narrow portable proof shapes as future Iceberg-adjacent proposal
  candidates. The Sail section now more directly argues that engine-shaped work
  such as manifest metrics, delete planning, metadata-as-data, commit
  validation, and typed v4 interpretation should move into Sail.
- Required service `table.commit` replay admission to carry positive Iceberg
  format-version evidence and non-negative snapshot-id evidence before
  acknowledgement, graph projection, or OpenLineage projection.
- Required service `table.commit` replay admission to carry full request,
  response, and idempotency-key hash evidence before acknowledgement, graph
  projection, or OpenLineage projection, leaving only `policy_hash` optional
  for commits that did not involve a policy.
- Bound compact QGLake credential-vending proof to branch-level authorization
  receipt evidence, requiring restricted-agent and trusted-human credential
  branches plus captured LakeCat replay JSON to carry full authorization
  receipt hashes and the `credentials-vend` action before archived handoff
  proof is accepted.
- Expanded the LakeCat book's concept guidance with audience-specific language
  for Iceberg users, operators, governed-agent designers, QueryGraph readers,
  and standards readers, including a direct "what to say / what not to say"
  ledger for Rust, Turso, REST paths, commit hardening, governed scans,
  credential proof, and QGLake handoff.
- Added compact QGLake table commit-history authorization receipt proof,
  requiring `tableCommitHistoryProof` and captured LakeCat replay evidence to
  carry a full authorization receipt hash and the `table-load` action before
  archived pointer-history evidence is accepted.
- Added QGLake handoff self-verifier regression coverage proving saved
  `lakecat-handoff-verify.json` top-level `requestIdentityProof` and
  `queryGraphBootstrapProof` copies cannot drift their authorization receipt
  actions away from the compact summary.
- Required compact QGLake `requestIdentityProof` and
  `queryGraphBootstrapProof` authorization receipt actions to be
  `lineage-read` and `graph-read` respectively, so archived handoff summaries
  cannot preserve valid receipt hashes while drifting the catalog action that
  was authorized.
- Expanded the LakeCat book's release-claim ledger with workflow-specific
  guidance for PySpark users, operators, governed agents, QueryGraph importers,
  standards readers, and Iceberg v4 compatibility, clarifying that LakeCat's
  proof surfaces live beside standard Iceberg while reusable table-format
  semantics should move into Sail.
- Bound QGLake governed scan proof to compact planned/fetched scan receipt
  identity evidence, requiring principal subject/kind, full authorization
  receipt hashes, and `table-plan-scan` actions to survive source replay,
  captured replay, and archived handoff summary verification.
- Required `table.scan-planned` and `table.scan-tasks-fetched` outbox replay
  admission to carry valid authorization receipt principal, action, allowed,
  engine, and checked-at evidence before acknowledgement, graph projection, or
  OpenLineage projection.
- Tightened service replay admission for `view.listed` events to require the
  read-side `view-load` authorization receipt action, aligning the durable
  outbox boundary with QGLake handoff verification and reserving `view-manage`
  for view mutations.
- Bound saved `lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics`
  authorization receipt actions to the compact request-identity proof, so a
  rehashed handoff self-verifier artifact cannot drift the drain read away from
  `lineage-read`.
- Required QGLake lineage-drain `eventTypes` manifests to match replay summary
  order, not only count or multiplicity, so compact handoff artifacts prove the
  replay sequence and cannot reorder catalog event summaries after drain.
- Added compact lineage-drain authorization action evidence to LakeCat replay
  summaries and QGLake verification, requiring the drain read to prove
  `lineage-read` and each replayed event summary to carry the receipt action
  that matches its event type before archived handoff proof is accepted.
- Expanded the LakeCat book's catalog concepts chapter with a detailed
  standard-Iceberg versus LakeCat/QueryGraph/TypeSec classification matrix,
  concrete PySpark, Rust engine, operator, governed-agent, and QueryGraph
  workflows, and a stronger argument for pushing Iceberg-heavy validation,
  pruning, delete handling, metadata-as-data, and v4 work into Sail.
- Required outbox replay admission to reject valid authorization receipt
  `action` values when they do not match the outbox event type, so replay
  cannot use a `table-load` receipt to project `table.commit` or similar
  action drift.
- Required outbox replay admission to reject unknown authorization receipt
  `action` values that do not deserialize as LakeCat's canonical
  `CatalogAction` enum before acknowledgement, graph projection, or
  OpenLineage projection.
- Required outbox replay admission to reject missing or blank authorization
  receipt `action` evidence before acknowledgement, graph projection, or
  OpenLineage projection, covering shared catalog receipt validation plus
  specialized table commit and commit-history replay paths.
- Expanded the LakeCat book with a front-loaded catalog-concepts contract
  explaining why the Rust service spine and Turso store are implementation
  choices, Iceberg REST namespace/table paths and commit CAS are standard
  catalog parlance, LakeCat audit/outbox/replay surfaces are optional
  control-plane extensions, TypeSec governed scan and credential receipts are
  governance extensions, and QueryGraph/QGLake handoff belongs above the
  catalog as an integration surface.
- Required outbox replay admission to reject missing, blank, or malformed
  authorization receipt `checked_at` timestamps before acknowledgement, graph
  projection, or OpenLineage projection, covering shared catalog receipt
  validation plus specialized table commit and commit-history replay paths.
- Required outbox replay admission to reject missing or denied authorization
  receipt `allowed` decisions before acknowledgement, graph projection, or
  OpenLineage projection, covering shared catalog receipt validation plus
  specialized table commit and commit-history replay paths.
- Required outbox replay admission to reject missing or blank authorization
  receipt engines before acknowledgement, graph projection, or OpenLineage
  projection, covering shared catalog receipt validation plus specialized
  table commit and commit-history replay paths.
- Renamed the default allow-all governance receipt engine to
  `lakecat-allow-all-local`, replacing stale placeholder wording with an
  explicit local compatibility-engine label, and added regression coverage
  proving default receipts no longer advertise placeholder policy semantics.
- Expanded the LakeCat book with a release-claim ledger that explicitly
  classifies the Rust service spine, Turso store, Iceberg REST paths, commit
  CAS/idempotency/pointer logs/audit/outbox/replay validation, governed
  scan/credential receipts, and QueryGraph/QGLake handoff as standard Iceberg
  parlance, LakeCat implementation, TypeSec/QueryGraph extensions, or future
  Iceberg-adjacent profile candidates, and strengthened the argument for
  pushing Iceberg-heavy planning and validation into Sail.
- Added a service-level `grust-local` outbox projection row to the full
  release-readiness gate and pinned it in the dependency contract, proving
  LakeCat's service boundary still projects catalog events through the Grust
  feature path.
- Recorded a full local release-readiness pass, including default workspace
  tests, feature-gated Turso/Sail/TypeSec/Grust rows, all-features workspace
  library tests, book rebuild, and live QGLake handoff verification.
- Expanded the LakeCat book's catalog-concepts guidance with a dedicated
  standards position that separates implementation details, optional
  LakeCat/QueryGraph control-plane extensions, and future Iceberg-adjacent
  profile candidates, and sharpened the argument for keeping reusable Iceberg
  semantics in Sail.
- Added QGLake compact management proof for warehouse-list `project-id` scope,
  requiring saved handoff summaries and raw lineage-drain replay to reject
  malformed or unlisted warehouse project scopes.
- Required `warehouse.listed` replay evidence to reject blank or syntactically
  invalid `project-id` scope before acknowledgement, graph projection, or
  OpenLineage projection.
- Required `server.upserted` and `warehouse.upserted` replay evidence to carry
  full hash proof whenever endpoint URLs or storage roots are present, and
  changed live management upsert producers to persist redacted hash evidence
  before graph or OpenLineage projection.
- Expanded the LakeCat book's front matter with an explicit standard Iceberg
  versus LakeCat implementation versus LakeCat/QueryGraph/TypeSec extension
  guide, including which pieces are future Iceberg-adjacent profile candidates
  and why Iceberg table-format work should be pushed into Sail.
- Required `querygraph.bootstrap` replay evidence to carry a valid
  authorization receipt principal before acknowledgement, graph projection, or
  OpenLineage projection, with missing/malformed-principal coverage proving
  QueryGraph bootstrap handoff cannot become actorless catalog graph material.
- Required `view.upserted`, `view.loaded`, and `view.dropped` replay evidence
  to carry a valid view name and positive `view-version`, and required guarded
  lifecycle replay to reject non-positive `expected-view-version` values before
  acknowledgement, graph projection, or OpenLineage projection.
- Expanded the LakeCat book with a detailed catalog-concept chapter that
  separates standard Iceberg parlance from LakeCat implementation,
  LakeCat/QueryGraph optional surfaces, TypeSec governance receipts, and
  future Iceberg-adjacent profile candidates, with PySpark, agentic, and
  QueryGraph bootstrap workflows explaining why reusable table-format work
  belongs in Sail.
- Required `credentials.vend-attempted` replay evidence to carry a valid
  authorization receipt principal before acknowledgement, graph projection, or
  OpenLineage projection, with zero-credential missing/malformed-principal
  coverage proving blocked credential decisions cannot become actorless
  credential-root evidence.
- Required service `table.commits-listed` replay evidence to carry
  `principal-subject` and `principal-kind` fields that match the authorization
  receipt principal before acknowledgement, graph projection, or OpenLineage
  projection, and added missing/drifted principal-summary coverage for
  pointer-log reads.
- Expanded the LakeCat book's catalog concept explanation with a release-facing
  status matrix covering the Rust service spine, Turso store, Iceberg REST
  paths, commit CAS/idempotency/pointer logs/audit/outbox/replay validation,
  governed scan and credential receipts, QueryGraph/QGLake handoff surfaces,
  and which pieces are standard Iceberg, LakeCat implementation,
  TypeSec/QueryGraph extensions, or future Iceberg-adjacent candidates.
- Required `view.version-receipt-chains-listed` replay evidence to carry valid
  warehouse, namespace, authorization receipt principal, and count-aligned
  chain/receipt/tombstone totals before acknowledgement, graph projection, or
  OpenLineage projection.
- Required `view.version-receipts-listed` replay evidence to carry valid
  warehouse, namespace, view, and authorization receipt principal proof before
  acknowledgement, graph projection, or OpenLineage projection.
- Required `table.created`, `table.loaded`, and `table.restored` replay
  evidence to carry an unsigned table `version` before acknowledgement, graph
  projection, or OpenLineage projection, with malformed-version coverage for
  create, load, and restore lifecycle events.
- Required table lifecycle replay evidence to carry a valid authorization
  receipt principal before acknowledgement, graph projection, or OpenLineage
  projection, with missing and malformed principal coverage across
  `table.created`, `table.loaded`, `table.deleted`, and `table.restored`.
- Required table lifecycle replay location evidence to be non-empty when
  present, with service admission coverage proving blank table and soft-delete
  locations fail before acknowledgement, graph projection, or OpenLineage
  projection; updated the book and regenerated the checked-in book artifacts.
- Added view-list replay evidence for count-aligned, syntactically valid,
  duplicate-free `view-names`, with service admission coverage proving
  malformed view-list name evidence fails before acknowledgement, graph
  projection, or OpenLineage projection; updated the book and regenerated the
  checked-in book artifacts.
- Added namespace-list replay evidence for count-aligned, syntactically valid,
  duplicate-free `namespace-paths`, with service admission coverage proving
  malformed namespace-list path evidence fails before acknowledgement, graph
  projection, or OpenLineage projection; updated the book and regenerated the
  checked-in book artifacts.
- Added service replay regression coverage proving management-list evidence
  rejects count-mismatched ID arrays for policy bindings, projects, servers,
  storage profiles, and warehouses before acknowledgement, graph projection, or
  OpenLineage projection.
- Added service replay regression coverage proving returned
  `credentials.vend-attempted` credential-response evidence rejects entries
  that omit catalog-derived `storage-profile-id`, `catalog-profile-id`,
  `storage-provider`, or `credential-mode` proof before acknowledgement, graph
  projection, or OpenLineage projection.
- Added service replay regression coverage proving returned
  `credentials.vend-attempted` credential-response evidence rejects entries
  that omit `authorization-principal` or `receipt-principal` before
  acknowledgement, graph projection, or OpenLineage projection.
- Added service replay regression coverage proving returned
  `credentials.vend-attempted` credential-response evidence rejects entries
  that omit `issuer-config-hash` before acknowledgement, graph projection, or
  OpenLineage projection.
- Added service replay regression coverage proving returned
  `credentials.vend-attempted` credential-response evidence rejects entries
  that omit `prefix-hash` before acknowledgement, graph projection, or
  OpenLineage projection.
- Required governed scan authorization-receipt read-restriction replay evidence
  to carry `policy-hashes` at service admission, with planned and fetched scan
  regression coverage proving missing receipt-side policy digest proof fails
  before acknowledgement, graph projection, or OpenLineage projection.
- Added service replay regression coverage proving governed `table.scan-planned`
  replay rejects term-based `row-predicate` read-restriction evidence that
  omits the narrowed `term` before acknowledgement, graph projection, or
  OpenLineage projection.
- Required governed scan read-restriction replay evidence to carry
  `policy-hashes` at service admission, with planned and fetched scan
  regression coverage proving missing policy digest proof fails before
  acknowledgement, graph projection, or OpenLineage projection.
- Added service replay regression coverage proving governed scan replay rejects
  non-object `row-predicate` read-restriction evidence for both planned and
  fetched scan events before acknowledgement, graph projection, or OpenLineage
  projection.
- Added service replay regression coverage proving governed scan replay rejects
  `eq` row-predicate read-restriction evidence that omits the required `value`
  for both planned and fetched scan events before acknowledgement, graph
  projection, or OpenLineage projection.
- Expanded the LakeCat book with a workflow-focused catalog concepts chapter
  that traces PySpark, notebook/service, governed-agent, operator, lineage,
  graph, and QueryGraph bootstrap flows while separating standard Iceberg
  parlance from LakeCat implementation, TypeSec governance proof,
  LakeCat/QueryGraph extensions, and future Iceberg-adjacent proposal
  candidates; regenerated the checked-in PDF, EPUB, and MOBI artifacts.
- Added service replay regression coverage proving governed scan replay rejects
  blank `row-predicate.type` read-restriction evidence for both planned and
  fetched scan events before acknowledgement, graph projection, or OpenLineage
  projection.
- Added service replay regression coverage proving governed scan replay rejects
  empty `row-predicate` read-restriction evidence for both planned and fetched
  scan events before acknowledgement, graph projection, or OpenLineage
  projection.
- Added service replay regression coverage proving governed
  `table.scan-tasks-fetched` events reject missing `row-predicate`
  read-restriction evidence before acknowledgement, graph projection, or
  OpenLineage projection.
- Added service replay regression coverage proving governed scan read
  restrictions reject malformed non-integer `max-credential-ttl-seconds`
  evidence before acknowledgement, graph projection, or OpenLineage projection.
- Expanded service replay regression coverage proving malformed standard
  catalog authorization receipt principals are rejected for catalog config
  reads before acknowledgement, graph projection, or OpenLineage projection.
- Added service replay regression coverage proving management list events
  reject malformed authorization receipt principals before acknowledgement,
  graph projection, or OpenLineage projection.
- Expanded the LakeCat book with a front-loaded catalog concept field guide
  explaining which concepts are standard Iceberg parlance, LakeCat
  implementation, optional LakeCat/QueryGraph extensions, TypeSec governance
  proof, or possible future Iceberg-adjacent profiles, with a stronger argument
  for pushing table-format semantics and governed planning into Sail.
- Added service replay regression coverage proving management upsert events
  reject malformed authorization receipt principals before acknowledgement,
  graph projection, or OpenLineage projection.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects credential response `max-credential-ttl-seconds` drift from the
  read-restriction receipt before acknowledgement, graph projection, or
  OpenLineage projection.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects credential response `governed-read-required` drift from the
  read-restriction receipt before acknowledgement, graph projection, or
  OpenLineage projection.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects non-unsigned credential response issuer-config entry counts before
  acknowledgement, graph projection, or OpenLineage projection.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects credential-count drift from credential response evidence before
  acknowledgement, graph projection, or OpenLineage projection.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects blocked raw-credential replay evidence that still carries credential
  response entries before acknowledgement, graph projection, or OpenLineage
  projection.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects blocked raw-credential exception evidence whose allowed=false proof
  omits a non-empty reason before acknowledgement, graph projection, or
  OpenLineage projection.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects malformed non-boolean raw-credential exception `allowed` evidence
  before acknowledgement, graph projection, or OpenLineage projection.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects credential block-reason evidence when the raw-credential exception
  receipt says raw credentials were allowed.
- Added service replay regression coverage proving `credentials.vend-attempted`
  rejects object-shaped nested storage-profile secret-ref provider evidence
  when `secret-ref-present` is false before acknowledgement, graph projection,
  or OpenLineage projection.
- Added service replay regression coverage proving `storage-profile.upserted`
  rejects object-shaped secret-ref hash evidence when `secret-ref-present` is
  false before acknowledgement, graph projection, or OpenLineage projection.
- Expanded the LakeCat book with a release-ledger treatment of catalog
  concepts, explicitly separating standard Iceberg parlance from LakeCat
  implementation details, LakeCat/QueryGraph optional extensions, TypeSec
  governance proof, and possible future Iceberg-adjacent profiles; also
  sharpened the argument that reusable Iceberg planning, pruning, metadata, and
  validation work belongs in Sail.
- Added service replay regression coverage proving `storage-profile.upserted`
  rejects object-shaped secret-ref provider evidence when `secret-ref-present`
  is false before acknowledgement, graph projection, or OpenLineage projection.
- Added service replay regression coverage proving `catalog.config-read`
  rejects stale structured v4 typed-Sail defaults before acknowledgement,
  graph projection, or OpenLineage projection.
- Added service replay regression coverage proving blocked credential-vend
  events reject blank credential block reasons before acknowledgement, graph
  projection, or OpenLineage projection.
- Added service replay regression coverage proving `table.commit` rejects blank
  new metadata pointer evidence before acknowledgement, graph projection, or
  OpenLineage projection.
- Added QGLake handoff artifact and import-plan verifier regression coverage
  proving QueryGraph graph-edge counts reject drift the same way graph-node
  counts already do.
- Added compact QGLake handoff-summary and raw lineage-drain verifier
  regression coverage proving duplicate bootstrap OpenLineage receipt hashes
  are rejected before archived QueryGraph proof can inflate bootstrap evidence.
- Added QGLake handoff artifact verifier regression coverage proving saved
  `lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics` rejects drifted
  delivered and graph-event counts even when the verifier-output artifact hash
  is updated.
- Added service replay regression coverage proving namespace and view standard
  catalog events reject malformed authorization-receipt principals before
  acknowledgement, graph projection, or OpenLineage projection.
- Added TypeSec-local credential issuer regression coverage proving blank
  config keys in environment and Vault secret payloads fail after authorization
  with hash-only diagnostics before any secret-backed credential is returned.
- Added service replay regression coverage proving `table.commit` rejects
  malformed commit and authorization-receipt principals before acknowledgement,
  graph projection, or OpenLineage projection.
- Added service replay regression coverage proving `table.scan-planned` and
  `table.scan-tasks-fetched` reject duplicate governed read-restriction
  `allowed-columns` evidence before acknowledgement, graph projection, or
  OpenLineage projection.
- Expanded the LakeCat book with a detailed standard-word versus
  LakeCat-mechanism section covering the Rust service spine, Turso store,
  Iceberg REST paths, commit CAS, pointer logs, audit/outbox, replay
  validation, governed scan/credential proof, and QueryGraph/QGLake handoff
  surfaces; also sharpened the Sail argument around PySpark, governed-agent,
  and operator workflows.
- Added service replay regression coverage proving `table.scan-planned`
  rejects duplicate effective-stats-fields evidence before acknowledgement,
  graph projection, or OpenLineage projection.
- Added service replay regression coverage proving `table.scan-planned`
  rejects duplicate requested-stats-fields evidence before acknowledgement,
  graph projection, or OpenLineage projection.
- Added service replay regression coverage proving `table.scan-planned`
  rejects duplicate effective-projection evidence before acknowledgement, graph
  projection, or OpenLineage projection.
- Added service replay regression coverage proving `table.scan-tasks-fetched`
  rejects duplicate effective-projection evidence before acknowledgement, graph
  projection, or OpenLineage projection.
- Added service replay regression coverage proving `table.scan-tasks-fetched`
  rejects duplicate required-projection evidence before acknowledgement, graph
  projection, or OpenLineage projection.
- Added explicit Sail-local v4 bridge partition-literal regression coverage to
  the full local release-readiness gate and dependency contract, keeping null
  and nested partition values in the local-first release proof.
- Deepened the LakeCat book's catalog-concepts and Sail-boundary explanation
  with a five-part standard-vs-extension classification and concrete governed
  read, commit, and QueryGraph bootstrap examples showing why Iceberg semantics
  belong in Sail while LakeCat owns trust, transactions, and evidence.
- Added explicit `lakecat-api` unit-test coverage to the full local
  release-readiness gate and required that row in the dependency contract, so
  API-owned compatibility and v4 bridge contracts remain part of the
  local-first release proof.
- Promoted the catalog configuration compatibility and v4 bridge posture
  strings to `lakecat-api` constants, added API-level coverage for the default
  response, and wired service replay validation to the same constants so
  catalog config responses and outbox admission cannot drift independently.
- Strengthened the workflow-trigger contract self-test with multi-file
  `.yml`/`.yaml` cases, proving the manual-only guard accepts multiple manual
  workflows and rejects an automatic trigger hidden in a secondary workflow
  file before release-readiness can pass.
- Expanded the LakeCat book with an implementation-status ledger that separates
  standard Iceberg parlance from LakeCat implementation details,
  LakeCat/QueryGraph optional catalog extensions, TypeSec governance proof, and
  future Iceberg-adjacent candidates; also strengthened the Sail rationale with
  a concrete engine/catalog/graph/security responsibility rule.
- Wired the workflow-trigger self-test into the release-readiness gate and
  dependency contract, adding single-quoted automatic-trigger regressions while
  preserving harmless nested job/step text that mentions automatic event names.
- Added management-route regression coverage proving recreated views surface as
  the next active durable version and as verified non-tombstoned receipt chains
  through the governed view receipt endpoints.
- Continued durable view-version receipt chains across drop/recreate in both
  memory and Turso stores: recreating a dropped view now advances after the
  latest tombstone receipt and links to that receipt instead of starting a
  second version-1-looking chain.
- Refreshed the generated LakeCat book distribution artifacts after rebuilding
  the current catalog-concepts and Sail-rationale manuscript.
- Bounded the local release-readiness feature matrix to package unit targets
  where package doc-tests add no LakeCat coverage and can hang in rustdoc after
  the Turso store tests pass; the gate now keeps explicit CLI all-features,
  workspace all-features library, book, and QGLake handoff checks.
- Required service `table.scan-planned` and `table.scan-tasks-fetched` replay
  admission to reject governed read-restriction evidence whose purpose is
  missing/blank or whose `max-credential-ttl-seconds` cap is missing or
  non-positive before acknowledgement, graph projection, or OpenLineage
  projection.
- Expanded the LakeCat book's catalog-concepts material into a detailed
  standard-vs-LakeCat-vs-QueryGraph/TypeSec ledger covering the Rust service
  spine, Turso store, Iceberg REST paths, commit CAS, idempotency, pointer
  logs, audit/outbox, replay validation, governed scan and credential proof,
  OpenLineage, QGLake handoff, and which surfaces are implementation details,
  optional extensions, or future Iceberg-adjacent candidates.
- Expanded the book's Sail argument with a concrete Sail-shaped versus
  catalog-shaped responsibility split, explaining why reusable Iceberg
  planning, manifest, delete, metadata-as-data, v3/v4, and commit-validation
  work belongs in the Rust engine while LakeCat owns trust, transactions, and
  evidence.
- Added `scripts/check-release-readiness.sh` as the local-first first-release
  gate, with full and quick modes covering dependency contracts, formatting,
  core feature tests, book rebuilds, and QGLake handoff proof.
- Required standard catalog replay admission for catalog config reads,
  namespace list/lifecycle events, and view list/lifecycle events to carry a
  valid authorization receipt principal before acknowledgement, graph
  projection, or OpenLineage projection.
- Required service management-upsert replay admission for policy bindings,
  projects, servers, storage profiles, and warehouses to carry a valid
  authorization receipt principal before acknowledgement, graph projection, or
  OpenLineage projection.
- Required service management-list replay admission to carry a valid
  authorization receipt principal before acknowledgement, graph projection, or
  OpenLineage projection.
- Required service `credentials.vend-attempted` replay admission to bind the
  payload table hint to the durable outbox table identity before
  acknowledgement, graph projection, or OpenLineage projection.
- Required service `table.commits-listed` replay admission to bind
  warehouse/namespace/table evidence to the durable outbox table identity
  before acknowledgement, graph projection, or OpenLineage projection.
- Required service `table.commits-listed` replay admission to carry a valid
  authorization receipt principal before acknowledgement, graph projection, or
  OpenLineage projection.
- Added raw QGLake lineage-drain regression coverage for missing or drifted
  table commit-history principal kind, keeping the source replay verifier
  locked to the accepted agent actor before compact handoff proof is generated.
- Required raw QGLake lineage-drain table commit-history replay to carry the
  accepted replay principal subject and agent kind before compact handoff proof
  can be generated.
- Bound compact QGLake table commit-history proof and captured LakeCat replay
  output to the accepted replay principal subject and kind, rejecting saved
  handoffs that drop or drift commit-history actor evidence.
- Added regression coverage requiring `table.commit` replay evidence to carry
  the authorization receipt principal before acknowledgement, graph projection,
  or OpenLineage projection.
- Required `table.commit` replay evidence to carry a valid commit principal
  matching the authorization receipt principal before acknowledgement, graph
  projection, or OpenLineage projection.
- Required management-list replay to carry count-aligned, syntactically valid,
  duplicate-free ID arrays before acknowledgement, graph projection, or
  OpenLineage projection, instead of accepting omitted identity arrays.
- Aligned compact QGLake handoff secret-reference verification with service
  replay admission: storage-profile and credential proofs now reject blank
  secret-ref providers, accept omitted provider/hash fields only when
  `secretRefPresent` is false, and reject any non-null provider/hash evidence
  when no secret ref is present.
- Required storage-profile and credential-vend replay admission to reject blank
  secret-ref providers and any unexpected secret-ref evidence when
  `secret-ref-present` is false before graph or OpenLineage projection.
- Required `table.commit` outbox admission to reject commit principal evidence
  that drifts from the authorization receipt principal before acknowledgement,
  graph projection, or OpenLineage projection.
- Rejected empty or blank ODRL allowed-column lists and blank ODRL purposes
  while composing `ReadRestriction`, before policy material can reach
  credential issuance or governed Sail planning/fetch paths.
- Rejected blank credential config keys in TypeSec environment and Vault
  secret-ref resolver payloads before secret-backed credentials can be issued.
- Required `table.scan-tasks-fetched` outbox admission to reject empty or
  drifted `required-filters` proof when governed row-predicate evidence is
  present, before acknowledgement, graph projection, or OpenLineage projection.
- Required scan-planned and scan-tasks-fetched outbox admission to reject empty
  projection/stat proof arrays before acknowledgement, graph projection, or
  OpenLineage projection.
- Required `credentials.vend-attempted` outbox admission to enforce the nested
  storage-profile provider/issuance-mode and secret-ref/mode compatibility
  proof even when credential count is zero.
- Required `storage-profile.upserted` outbox admission to enforce
  provider/issuance-mode compatibility before acknowledgement, graph
  projection, or OpenLineage projection.
- Required `storage-profile.upserted` outbox admission to keep
  `secret-ref-present` evidence consistent with credential issuance mode before
  acknowledgement, graph projection, or OpenLineage projection.
- Aligned QueryGraph bootstrap duplicate verified-table/view assertions with
  the generic duplicate-free string-array admission boundary used by
  all-features service tests.
- Required `storage-profile.upserted` outbox admission to validate
  credential-root identity fields, nested warehouse scope, provider, and
  issuance mode before acknowledgement, graph projection, or OpenLineage
  projection.
- Emitted hash-only `storage-profile.upserted` audit evidence for storage
  roots, and required service outbox admission to reject raw
  `location-prefix` values before acknowledgement, graph projection, or
  OpenLineage projection.
- Required blocked `credentials.vend-attempted` replay evidence to carry zero
  credentials plus a non-empty block reason matching the raw-credential
  exception receipt context before graph or OpenLineage projection.
- Required service `table.commit` outbox admission to carry non-empty new
  metadata pointer evidence, and rejected blank previous metadata pointer
  evidence when present, before acknowledgement, graph projection, or
  OpenLineage projection.
- Required service `table.commit` outbox admission to reject zero commit
  sequence numbers before acknowledgement, graph projection, or OpenLineage
  projection.
- Required service outbox admission to reject duplicate governed
  read-restriction `policy-hashes` for both top-level scan evidence and
  authorization-receipt contexts before graph or OpenLineage projection.
- Required service scan-planned and scan-tasks-fetched outbox admission to
  reject missing or malformed governed read-restriction `row-predicate`
  evidence before graph or OpenLineage projection.
- Rejected blank and duplicate scan projection/stat field arrays at service
  outbox admission before graph or OpenLineage projection, aligning live
  scan-planned and scan-tasks-fetched replay evidence with compact QGLake proof
  validation.
- Required compact governed scan projection/stat evidence and raw
  lineage-drain scan replay evidence to use non-empty, duplicate-free field
  arrays before proving policy narrowing, preventing repeated requested fields
  from inflating archived QGLake scan proof.
- Rejected empty and malformed `row-predicate` objects in compact governed
  read-restriction proof verification for QGLake lineage-drain and
  handoff-summary replay, requiring non-empty predicate type evidence and
  term/value evidence for term-based predicates before archived proof is
  accepted.
- Rejected empty and duplicate `allowed-columns` values in compact governed
  read-restriction proof verification for QGLake lineage-drain and
  handoff-summary replay, keeping scan and credential restriction evidence
  syntactically meaningful before archived proof is accepted.
- Rejected syntactically invalid compact management IDs in QGLake lineage-drain
  and handoff-summary proof verification, preventing path/query-decorated
  server, project, warehouse, policy, or storage-profile identities from
  entering archived management replay evidence.
- Revalidated soft-delete recovery records at memory and Turso delete/restore
  boundaries, rejecting corrupted durable restore evidence before recovery can
  consume the soft-delete marker or expose mismatched table pointer state.
- Revalidated view records and view-version receipts at memory and Turso
  load/list/drop/read boundaries, rejecting corrupted durable view replay JSON
  and malformed receipt hash/identity evidence before view receipt chains can
  advance.
- Revalidated policy-binding governance records at memory and Turso list/table
  read boundaries, rejecting corrupted durable binding JSON before governance
  replay can expose table-scoped bindings without namespace evidence.
- Revalidated storage-profile credential roots at memory and Turso list/match
  read boundaries, rejecting corrupted durable profile JSON and replacing raw
  invalid storage-profile identifiers with hash evidence.
- Revalidated warehouse management records at memory and Turso load/list-read
  boundaries, rejecting corrupted durable storage-root JSON before management
  replay can expose invalid roots or leak decorated storage-root material.
- Revalidated project management records at memory and Turso list-read
  boundaries, rejecting corrupted durable hierarchy JSON and replacing raw
  invalid project/server identifiers in validation errors with hash evidence.
- Revalidated server management records at memory and Turso list-read
  boundaries, rejecting corrupted durable `record_json` before management
  replay can expose invalid endpoint URLs or leak decorated endpoint material.
- Validated pending outbox events at memory and Turso read boundaries, rejecting
  corrupted event ids, missing or drifted payload event types, empty sinks, and
  already-delivered rows before graph or lineage projection can observe them.
- Cross-checked Turso metadata pointer-log row fields against decoded
  `TableCommitRecord` JSON when reading commit history, rejecting row/JSON
  drift before commit replay can observe contradictory sequence, pointer,
  request-hash, or timestamp evidence.
- Validated table commit-history records at memory and Turso read boundaries,
  rejecting malformed durable pointer-log JSON whose table, sequence, pointer,
  request, response, or idempotency-hash evidence would make replay unsafe.
- Tightened `TableCommit` validation so memory and Turso commit paths reject
  empty metadata-pointer strings and non-object replacement metadata before
  malformed direct commits can mutate tables, commit history, or outbox state.
- Tightened `TableRecord` validation so memory and Turso table creation reject
  empty metadata-location strings and non-object table metadata before
  malformed direct records can enter durable catalog state.
- Revalidated table records at memory and Turso `create_table` boundaries,
  rejecting deserialized records with empty table locations before they can
  create namespaces or enter durable catalog state.
- Revalidated policy bindings at the `CatalogStore` upsert boundary for the
  default, memory, and Turso stores, rejecting deserialized table-scoped policy
  bindings that omit a namespace before they can enter durable catalog state.
- Added QGLake handoff artifact regression coverage proving saved
  `lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics.lineageEvents`
  cannot drift from the accepted lineage-drain artifact while still passing
  artifact verification.
- Gated the service test-only `CapturingSailEngine` helper to non-`sail-local`
  builds, removing all-features dead-code warning noise while preserving the
  default feature tests that exercise the captured scan/fetch request path.
- Reconciled `AGENTS.md` and `GOAL.md` so durable guidance names the
  `qglake-fixture` feature boundary, CLI fixture verification, dependency
  contract checks, and book rebuild expectations.
- Extended the local dependency-contract audit so the explicit
  `lakecat-cli qglake-fixture` feature boundary is protected in CLI manifests,
  the local handoff script, and the manual-only CI matrix.
- Added REST-level commit coverage proving decorated metadata object locations
  are rejected with hash-only evidence and without leaking raw query strings,
  tokens, object names, or paths.
- Gated `lakecat-cli qglake-fixture` behind an explicit
  `qglake-fixture` feature so default CLI verification and management commands
  no longer pull Sail's local Iceberg fixture writer into every build.
- Tightened service-level outbox drain coverage so pending batches are selected
  by `created_at,event_id` before applying the drain limit, and only that
  ordered prefix is projected and acknowledged.
- Refreshed the full local QGLake handoff proof after captured replay
  hardening, confirming the live harness still verifies the LakeCat,
  QueryGraph, graph, and OpenLineage artifact chain end to end.
- Required captured QGLake scan replay-line recomputation to reject empty
  planned or fetched `allowed-columns`, so archived operator replay text cannot
  treat an empty governed projection as unrestricted access.
- Tightened captured QGLake table commit-history replay-line recomputation so
  saved LakeCat replay artifacts must keep positive, strictly increasing commit
  sequence proof before operator text is accepted.
- Tightened QGLake operator replay-line generation so storage-profile and
  credential replay summaries require full SHA-256 secret-ref hashes instead
  of rendering prefix-only placeholder credential-root evidence.
- Added positive QGLake acceptance coverage for secret-ref-backed credential
  roots: compact handoff summaries and operator replay lines now prove matching
  redacted provider/hash evidence for production credential profiles.
- Recorded `lakecat.secret-ref-provider` in redacted
  `credential-response-evidence` and required credential-vend replay admission
  to match that provider against the selected storage profile before graph or
  OpenLineage projection.
- Canonicalized `lakecat.secret-ref-provider` on issued credential responses
  from the selected storage profile, so TypeSec-gated production backends
  cannot shadow which external secret-ref provider authorized a credential.
- Encoded null and nested Sail Iceberg partition literals into Iceberg REST
  JSON instead of rejecting them in the LakeCat Sail adapter, keeping manifest
  expansion compatible with richer partition tuples while typed v4 Sail work
  continues upstream.
- Bound captured LakeCat replay `credentialPrefixHashes` to the compact QGLake
  `credentialVendingProof`, so archived handoffs are rejected when captured
  restricted-agent or trusted-human credential prefix evidence drifts from the
  accepted compact proof.
- Added redacted `credentialPrefixHashes` to QGLake credential replay proof
  and required raw lineage-drain artifacts plus compact handoff summaries to
  keep returned credential prefix hashes count-aligned, full SHA-256-shaped,
  and duplicate-free before credential proof can inflate replay counts.
- Required raw QGLake lineage-drain view-history summaries to reject duplicate
  `viewVersionReceiptHashes` and `viewVersionReceiptChainHashes`, preventing
  tombstone and namespace receipt-chain replay from inflating view proof before
  compact handoff generation.
- Required raw QGLake lineage-drain replay summaries to reject duplicate
  `replayEventHashes` and `openLineageHashes` before compact handoff proof is
  regenerated, keeping source replay and saved handoff summaries aligned.
- Required compact QGLake replay and OpenLineage hash arrays to be
  duplicate-free in saved handoff summaries, so archived handoffs cannot
  inflate bootstrap, scan, management, commit-history, view, storage-profile,
  or credential proof evidence by repeating an already accepted full digest.
- Required `querygraph.bootstrap` outbox replay to reject duplicate
  `verified-tables` and `verified-views` manifests before graph/OpenLineage
  projection, matching compact QGLake handoff verification.
- Required QGLake lineage-drain `eventTypes` to match replay summary event
  types as a multiset, so repeated valid event types such as credential or
  scan-task replay cannot hide missing or extra replay summaries.
- Required QGLake `verifiedTables` and `verifiedViews` manifests to be
  duplicate-free in compact handoff summaries, so table/view counts cannot be
  inflated by repeating already accepted stable IDs.
- Required QGLake compact management proof IDs to be duplicate-free in saved
  handoff summaries and lineage-drain replay verification, matching the
  management-list service replay invariant for server, project, warehouse,
  policy, and storage-profile reads.
- Required QGLake compact table commit-history proof `commitHashes` to be
  duplicate-free in saved handoff summaries and lineage-drain replay
  verification.
- Required table commit-history replay `commit-hashes` to be duplicate-free
  before graph/OpenLineage projection, so repeated compact commit proof cannot
  inflate pointer-log replay counts.
- Required management-list replay ID arrays to be duplicate-free before
  graph/OpenLineage projection, so server, project, warehouse, policy, or
  storage-profile list counts cannot be inflated with repeated identifiers.
- Required table commit-history replay `sequence-numbers` to be positive and
  strictly increasing before graph/OpenLineage projection, so duplicated or
  reordered pointer-log evidence cannot become accepted replay.
- Required credential-vend replay `credential-response-evidence` entries to be
  duplicate-free by returned credential `prefix-hash` before graph/OpenLineage
  projection, so replay cannot inflate credential counts with repeated
  redacted credential evidence.
- Required view receipt-list and receipt-chain replay hash arrays to be
  duplicate-free before graph/OpenLineage projection, so duplicated receipt,
  drop-receipt, or chain hashes cannot inflate view-history proof evidence
  before outbox delivery.
- Bound `querygraph.bootstrap` replay table/view artifact stable IDs and
  view-version receipt stable IDs to the `verified-tables` and
  `verified-views` manifests before graph/OpenLineage projection, rejecting
  spliced QueryGraph bootstrap evidence before outbox delivery.
- Required catalog config-read replay defaults to be structured string
  key/value entries with duplicate-free keys before graph/OpenLineage
  projection, so v4 bridge posture cannot be replayed with contradictory
  config claims.
- Added explicit catalog config defaults for the Iceberg v4 JSON bridge:
  `lakecat.format.v4.bridge=json-passthrough` and
  `lakecat.format.v4.typed-sail=unavailable`, and required catalog config
  replay evidence to carry those claims before graph/OpenLineage projection.
- Tightened the workflow-trigger dependency-contract audit so automatic GitHub
  events are rejected only when they appear under `on:`, added block-map and
  inline-list/map trigger coverage, and proved harmless workflow keys such as a
  `jobs.push` job id remain accepted.
- Redacted credential issuer scope-rejection errors to only
  `credential-prefix-hash` and `storage-profile-prefix-hash` evidence, and
  pinned that out-of-scope issuer credentials fail before credential-vend replay
  evidence is recorded.
- Added explicit service regression coverage proving both planned and fetched
  governed scan replay reject top-level `read-restriction` evidence that drifts
  from `authorization-receipt.context.read-restriction` before graph,
  OpenLineage, or delivery acknowledgement.
- Required QGLake lineage-drain artifacts to reconcile `delivered`,
  `eventTypes`, replay summary count, `graphEvents`, and `lineageEvents` before
  acceptance, and corrected accepted fixtures to carry honest aggregate drain
  totals.
- Bound saved `lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics`
  `delivered`, `eventTypes`, `graphEvents`, and `lineageEvents` back to the
  actual lineage-drain artifact whenever a verifier-output hash is present.
- Required QGLake lineage-drain replay summaries to be covered by the
  drain-level `eventTypes` list, and corrected accepted verifier fixtures to
  declare `storage-profile.upserted` whenever they carry that replay summary.
- Pinned pending outbox batch limits to apply after deterministic
  `created_at,event_id` ordering in both embedded and Turso stores, so
  batched drains replay the same prefix across durable backends.
- Added fetched scan-task replay coverage for empty
  `read-restriction.allowed-columns`, proving both planned and fetched governed
  scan replay fail closed before outbox delivery.
- Rejected governed scan replay with empty `read-restriction.allowed-columns`
  before outbox delivery, matching live scan planning's fail-closed behavior
  when a policy leaves no readable columns.
- Required governed scan-planned replay `effective-stats-fields` to stay inside
  `read-restriction.allowed-columns` before outbox delivery, so stats evidence
  cannot preserve a forbidden column after projection has been narrowed.
- Required credential-vend replay nested storage-profile `warehouse` evidence to
  match the event table warehouse before outbox delivery, so zero-credential
  blocked paths cannot replay a credential root under a forged warehouse anchor.
- Required credential-vend replay top-level `secret-ref-present` evidence, when
  present, to match the nested storage-profile secret-reference state before
  outbox delivery, so credential replay cannot project contradictory
  credential-root secret posture.
- Required credential-vend replay top-level `storage-profile-id` evidence to
  match the nested storage-profile `profile-id` before outbox delivery, so
  zero-credential blocked paths cannot project contradictory credential-root
  anchors.
- Required credential-vend replay response evidence to match catalog-derived
  storage profile, provider, credential mode, principal, governed-read, and TTL
  fields before outbox delivery, so forged credential metadata cannot become
  graph or OpenLineage evidence.
- Hardened the local dependency-contract workflow audit to reject quoted
  `on` keys and quoted automatic event names, and added a workflow-trigger
  self-test so manual-only CI cannot be bypassed through YAML quoting.
- Required credential-vend replay raw-credential exception evidence to match
  the authorization receipt context before outbox delivery, so blocked-agent
  and trusted-human exception replay cannot drift from the durable TypeSec
  decision receipt.
- Required credential-vend replay read restrictions to match the authorization
  receipt context before outbox delivery, so credential replay cannot preserve
  policy-derived TTL or blocked-read evidence outside the durable receipt.
- Required scan replay read restrictions to match the authorization receipt
  context before outbox delivery, so governed planned/fetched scan evidence
  cannot claim policy narrowing that the durable receipt did not capture.
- Labeled metadata object-store setup diagnostics with
  `backend-error-hash`, preserving hash-only backend evidence while matching
  the commit-hardening design contract for invalid URI and unsupported backend
  failures.
- Redacted storage-profile ids from metadata-object prefix rejection messages,
  keeping commit-location failures anchored by metadata-location and
  storage-profile-prefix hashes without echoing tenant or storage profile names.
- Rejected empty authorization-receipt read-restriction `policy-hashes` arrays
  during outbox drain validation, so replay receipts cannot carry weaker
  governed-read policy evidence than the top-level scan event before graph or
  lineage projection.
- Rejected empty `read-restriction.policy-hashes` arrays during outbox drain
  validation, so governed read replay evidence with a policy field must carry
  at least one full policy digest before projection or acknowledgement.
- Rejected unsupported outbox event types before graph or lineage projection,
  leaving malformed future/custom events pending instead of silently
  acknowledging them with an empty projection receipt.
- Bound captured LakeCat `management-replay` and
  `table-commit-history-replay` text lines to compact QGLake management,
  storage-profile, and table commit-history proof fields, closing the remaining
  operator-facing replay-line drift gap in saved handoff artifacts.
- Bound captured LakeCat `scan-replay` and `credential-replay` text lines to
  the compact QGLake scan and credential proof fields, so saved handoff
  artifacts cannot drift operator-facing replay text away from the verified
  purpose, TTL cap, and credential storage-scope evidence.
- Added an outbox-drain regression proving that when a later projection in a
  multi-event batch fails, earlier successfully projected events are not
  acknowledged and the whole batch remains retryable from durable outbox state.
- Bound compact QGLake structural `receiptHash` values to the same
  content-derived view-version receipt digest that LakeCat service emits by
  carrying view hash, principal identity, principal kind, and recorded timestamp
  in compact receipt bodies.
- Bound compact QGLake structural `chainHash` values to the same
  content-derived receipt-chain digest that LakeCat service emits, so saved
  handoff summaries cannot pair valid-looking ordered receipts with a forged
  accepted view receipt-chain hash.
- Required compact QGLake namespace view `chainHashes` and `receiptHashes`
  arrays to be duplicate-free, rejected duplicate structural chain hashes, and
  required declared receipt hashes to match the structural
  `receiptChains[].chains[]` proof bodies exactly, so saved handoff summaries
  cannot carry extra, omitted, or duplicated view-history hash evidence.
- Bound compact QGLake accepted receipt-chain hashes and tombstone receipt
  hashes to structural receipt-chain evidence for the same stable view, so
  saved handoff summaries reject cross-view hash splicing within a namespace.
- Bound compact QGLake tombstone receipt stable IDs to their warehouse,
  namespace, and view-name components, so saved handoff summaries reject
  deletion-proof identity drift before accepting tombstone guard evidence.
- Bound compact QGLake accepted-view and structural receipt-chain stable IDs
  to their warehouse, namespace, and view-name components, so saved handoff
  summaries reject component drift even when the top-level verified view set
  and receipt-chain hashes still look valid.
- Bound compact QGLake view receipt-chain identities across namespace
  receipt-chain groups, chain summaries, and per-receipt entries, so saved
  handoff summaries reject warehouse, namespace, stable ID, or view-name drift
  inside otherwise hash-shaped view history evidence.
- Added compact view receipt-chain structure to QGLake replay and handoff
  proofs, including per-receipt versions, operations, hashes, and previous-link
  evidence, and rejected handoff summaries with invalid chain heads, forged
  previous links, skipped upsert versions, unsupported operations, or chain
  heads that do not match the latest receipt.
- Required saved `lakecat-handoff-verify.json` artifacts to preserve every
  captured LakeCat replay proof section, including management ID arrays,
  governed scan proof, table commit history, view receipt chains, storage
  profile evidence, and credential-vending proof.
- Required captured LakeCat replay output to match compact QGLake
  `managementProof` ID arrays for servers, projects, warehouses, policies, and
  storage profiles, so saved handoff summaries cannot drift from the captured
  replay artifact while keeping artifact hashes valid.
- Lifted redacted management-list ID arrays into lineage-drain summaries and
  compact QGLake `managementProof`, and rejected source replay or saved handoff
  summaries when those arrays are missing, empty, or drift from the recorded
  counts.
- Rejected outbox delivery for malformed `querygraph.bootstrap` evidence whose
  warehouse, table/view counts, verified ids, manifest hashes, artifact hashes,
  view receipt hashes, standards, or optional TypeDID/agent proof hashes are
  missing or malformed, so invalid QueryGraph bootstrap replay fails before
  graph/lineage projection acknowledgement.
- Rejected outbox delivery for malformed `table.scan-planned` and
  `table.scan-tasks-fetched` evidence whose table identity, projection/stat
  arrays, task counts, fetched filters, or governed read-restriction projection
  constraints are missing, widened, or contradictory, so invalid scan replay
  fails before graph/lineage projection acknowledgement.
- Rejected outbox delivery for `table.commit` events whose commit object,
  sequence number, table identity, or commit-table identity evidence is missing
  or contradictory, so invalid commit replay fails before graph/lineage
  projection acknowledgement.
- Rejected outbox delivery for table lifecycle events whose table identity,
  optional scope hints, or soft-delete evidence is missing or contradictory, so
  invalid table create/load/delete/restore replay fails before graph/lineage
  projection acknowledgement.
- Rejected outbox delivery for view list and lifecycle events whose warehouse,
  namespace, name, or count evidence is malformed, so invalid view replay fails
  before graph/lineage projection acknowledgement.
- Added redacted stable ID arrays to management-list audit/outbox reads and
  rejected outbox delivery when those optional ID arrays are malformed or drift
  from the recorded count, so invalid server/project/warehouse/policy/profile
  list replay fails before lineage acknowledgement.
- Rejected outbox delivery for catalog read events whose warehouse or namespace
  list-count evidence is malformed, so invalid config-read and namespace-list
  replay fails before graph/lineage projection acknowledgement.
- Rejected outbox delivery for namespace lifecycle events whose warehouse or
  namespace evidence is malformed, so invalid namespace create/load/drop replay
  fails before graph/lineage projection acknowledgement.
- Rejected outbox delivery for `project.upserted` events whose project evidence
  has mismatched project ids, malformed server scope, invalid public
  properties, or malformed identifiers, so invalid project replay fails before
  graph/lineage projection acknowledgement.
- Rejected outbox delivery for `server.upserted` and `warehouse.upserted`
  events whose tenant-root evidence has malformed endpoint URLs, storage roots,
  identifiers, properties, or redacted hash anchors, so invalid server and
  warehouse replay fails before graph/lineage projection acknowledgement.
- Rejected outbox delivery for `policy-binding.upserted` events whose
  policy-binding evidence has malformed identifiers, warehouse scope,
  namespace/table scope, or missing enforcement/ODRL fields, so invalid policy
  replay fails before graph/lineage projection acknowledgement.
- Rejected outbox delivery for `storage-profile.upserted` events whose
  storage-profile evidence carries a raw `secret-ref`, malformed
  secret-reference provider/hash state, or neither a location prefix nor a
  full `location-prefix-hash`, so storage-profile replay fails before
  graph/lineage projection when its redaction proof is unsafe.
- Rejected outbox delivery for `credentials.vend-attempted` events whose
  credential count, response-evidence hashes, storage-profile
  `location-prefix-hash`, or secret-reference state are malformed, so invalid
  credential replay evidence fails before graph/lineage projection
  acknowledgement.
- Rejected outbox delivery for `table.commits-listed` events whose commit
  count, commit hashes, or sequence-number arrays are malformed, so invalid
  pointer-log replay evidence fails before graph/lineage projection
  acknowledgement.
- Rejected outbox delivery for `view.version-receipts-listed` events whose
  receipt count, receipt hashes, or drop receipt hashes are malformed, so
  invalid view receipt-list evidence fails before graph/lineage projection
  acknowledgement.
- Rejected outbox delivery for verified
  `view.version-receipt-chains-listed` chains whose chain hash, receipt hashes,
  verified-chain count, first receipt, previous links, or upsert/drop version
  transitions are malformed, so invalid view receipt-chain evidence fails
  before graph/lineage projection acknowledgement.
- Required QGLake table commit-history record verification to reject compact
  pointer-log request, response, idempotency-key, commit, and optional policy
  hashes unless they are full `sha256:`-prefixed 64-hex digests.
- Rejected outbox delivery for `table.commit` events whose request, response,
  idempotency-key, or policy hash evidence is not a full `sha256:`-prefixed
  64-hex digest, so malformed REST commit receipts fail before projection.
- Rejected outbox delivery for pending events whose `read-restriction`
  `policy-hashes` are not full `sha256:`-prefixed 64-hex digests, so malformed
  governed-read evidence fails before graph/lineage projection acknowledgement.
- Required governed scan read-restriction `policy-hashes` in QGLake source
  replay and compact handoff proof to contain full `sha256:`-prefixed 64-hex
  digests, rejecting short placeholder policy anchors.
- Required QGLake lineage-drain source replay for storage-profile upsert and
  credential-vending `secretRefHash` evidence to contain full
  `sha256:`-prefixed 64-hex digests before compact handoff proof generation.
- Preserved accepted view receipt-chain hashes in generated QGLake namespace
  `receiptChains[].chainHashes` evidence, so the live handoff summary covers
  each `acceptedReceiptChainHash` with the namespace receipt-chain proof it
  later verifies.
- Required QGLake handoff artifact manifest hashes, service-log hashes, and
  optional self-verifier output hashes to contain full `sha256:`-prefixed
  64-hex digests before file content comparison, rejecting short placeholder
  artifact integrity anchors.
- Required compact QGLake storage-profile and credential-vending
  `secretRefHash` evidence to contain full `sha256:`-prefixed 64-hex digests
  when a secret reference is present, rejecting short placeholder credential
  root proof anchors in saved handoff summaries.
- Required compact QGLake TypeDID envelope/proof hash slots to contain full
  `sha256:`-prefixed 64-hex digests when present, rejecting short placeholder
  TypeDID proof anchors in saved handoff summaries.
- Required compact QGLake request-identity and bootstrap authorization,
  delegation, and summary-signature proof hashes to contain full
  `sha256:`-prefixed 64-hex digests, rejecting short placeholder proof anchors.
- Required compact QGLake view receipt-chain proof receipt/replay/OpenLineage
  hashes to contain full `sha256:`-prefixed 64-hex digests, rejecting short
  placeholder accepted-view, tombstone, and namespace chain evidence.
- Required compact QGLake credential-vending proof replay/OpenLineage hash
  arrays to contain full `sha256:`-prefixed 64-hex digests for both restricted
  agent and trusted-human branches, rejecting short placeholder receipt hashes.
- Required compact QGLake storage-profile upsert replay/OpenLineage hash
  arrays to contain full `sha256:`-prefixed 64-hex digests, rejecting short
  placeholder credential-root replay anchors in saved handoff summaries.
- Required compact QGLake QueryGraph bootstrap core and replay/OpenLineage
  hashes to contain full `sha256:`-prefixed 64-hex digests, rejecting short
  placeholder bundle/import/replay anchors in saved handoff summaries.
- Required compact QGLake table commit-history proof commit/replay/OpenLineage
  hash arrays to contain full `sha256:`-prefixed 64-hex digests, rejecting
  short placeholder pointer-history receipt hashes in saved handoff summaries.
- Required compact QGLake management proof replay/OpenLineage hash arrays to
  contain full `sha256:`-prefixed 64-hex digests, rejecting short placeholder
  control-plane read receipt hashes in saved handoff summaries.
- Required compact QGLake governed scan proof replay/OpenLineage hash arrays
  to contain full `sha256:`-prefixed 64-hex digests, rejecting short
  placeholder scan receipt hashes in saved handoff summaries.
- Required governed scan replay receipt arrays in QGLake lineage-drain
  verification to contain full `sha256:`-prefixed 64-hex digests, rejecting
  short placeholder scan replay and OpenLineage hashes.
- Preserved full view receipt coverage in lineage-drain summaries for
  `view.version-receipts-listed` and nested
  `view.version-receipt-chains-listed` payloads, so QGLake replay evidence can
  prove namespace chains cover every upsert and tombstone receipt.
- Tightened compact QGLake handoff verification so tombstoned accepted views
  must still have their `acceptedReceiptChainHash` covered by namespace
  `receiptChains[].chainHashes`, binding deletion evidence to the accepted
  view's receipt chain rather than a tombstone-only proof.
- Pinned service table commit-history coverage so produced request, response,
  idempotency-key, and commit hashes are full SHA-256 digests across the route,
  outbox, lineage-drain summary, and graph projection.
- Pinned service view receipt route coverage so produced receipt hashes,
  view-content hashes, and namespace chain hashes are full SHA-256 digests
  before QGLake consumes view receipt-chain evidence.
- Pinned service-side storage-profile replay and credential audit coverage so
  producer evidence emits full SHA-256 `location-prefix-hash` values before
  QGLake accepts the corresponding `locationPrefixHash` proof.
- Required QGLake storage-profile and credential storage-scope
  `locationPrefixHash` evidence to be full `sha256:`-prefixed 64-hex digests
  in compact handoff summaries and lineage-drain replay checks.
- Pinned QueryGraph bootstrap producer coverage so durable tenant root evidence
  emitted by `lakecat-querygraph` and the service route is proven to be full
  SHA-256 hash evidence, not merely prefix-shaped redaction text.
- Required QGLake bootstrap tenant-root hash evidence to be full
  `sha256:`-prefixed 64-hex digests for `endpointUrlHash` and
  `storageRootHash`, rejecting short placeholder hashes even when the bundle
  hashes are recomputed.
- Required server endpoint URLs to be absolute `http` or `https` URLs before
  memory or Turso persistence, returning only `server-endpoint-url-hash`
  evidence for invalid or non-HTTP endpoint submissions.
- Tightened QGLake bootstrap verification so self-consistent bundles are
  rejected if tenant `Server` or `Warehouse` graph nodes expose raw
  `endpointUrl` or `storageRoot` values instead of hash-only evidence.
- Redacted QueryGraph bootstrap tenant graph roots so durable management
  `Server` and `Warehouse` nodes emit `endpointUrlHash` and `storageRootHash`
  rather than raw endpoint URLs or warehouse storage roots.
- Rejected decorated server endpoint URLs before memory or Turso persistence and
  redacted legacy/imported endpoint URLs during `server.upserted` replay,
  emitting only `server-endpoint-url-hash` or `endpoint-url-hash` evidence
  instead of query tokens, fragments, or URI userinfo.
- Refreshed the live QGLake handoff after warehouse-root hardening, proving the
  local harness still verifies 1 table, 1 view, 26 drained replay events,
  QueryGraph verify/import, and compact handoff self-checks with redacted
  management and storage-profile evidence.
- Rejected decorated and traversal-shaped warehouse storage roots before memory
  or Turso persistence, returning `warehouse-storage-root-hash` evidence instead
  of raw local paths, bucket roots, query tokens, or URI userinfo.
- Applied warehouse storage-root replay redaction to catalog config-read
  projections as well, so any warehouse record attached to config replay emits
  only `storage-root-hash` evidence to graph and lineage sinks.
- Redacted warehouse storage roots before graph and lineage replay, replacing
  raw `storage-root` values with `storage-root-hash` evidence while leaving the
  authorized management response shape unchanged.
- Extended the manual-only workflow audit to reject YAML block-list event
  triggers such as `on:\n  - push`, closing another compact GitHub Actions
  syntax path around the local-first CI policy.
- Hardened the local dependency-contract workflow audit so manual-only CI checks
  reject compact GitHub trigger forms such as `on: push`,
  `on: [push, pull_request]`, and inline event maps, not only mapping-style
  trigger blocks.
- Rejected decorated credential resolver secret refs at the resolver boundary
  itself, so query strings, fragments, and URI userinfo fail with only
  `secret-ref-hash` evidence even for legacy/imported profiles that bypass
  storage-profile constructor validation.
- Refreshed the live QGLake handoff verification after tombstone receipt-chain
  binding, proving the local harness still verifies 1 table, 1 view, 26 drained
  replay events, QueryGraph verify/import, and compact handoff self-checks.
- Bound QGLake dropped-view tombstone receipts to namespace receipt-chain
  evidence in both live lineage-drain replay and compact handoff verification,
  rejecting handoffs whose tombstone hashes are not covered by the chain read.
- Pinned route-level stale view mutation guards so rejected guarded view upserts
  and drops do not emit new replay outbox events or extend QueryGraph receipt
  evidence.
- Pinned service replay-summary coverage for `table.scan-planned` so requested
  and effective projection/statistics evidence survives outbox drain before
  QGLake handoff verification consumes it.
- Pinned service outbox-drain coverage for `table.scan-tasks-fetched`
  summaries so fetched `effective-projection` survives before QGLake replay and
  handoff verification consume it.
- Made `scripts/qglake-handoff-local.sh` require and embed governed-scan
  planned/fetched projection evidence, including fetched `effectiveProjection`,
  before writing the compact QGLake handoff summary.
- Required QGLake fetch replay and compact handoff verification to include
  fetched `effective-projection` evidence matching the server-derived read
  restriction.
- Added explicit `effective-projection` evidence to `fetchScanTasks` response
  extensions and scan-task-fetched audit/outbox payloads, mirroring the
  server-derived required projection for replay.
- Redacted the TypeDID verifier trait boundary so custom verifier failures
  preserve the HTTP error class while exposing only envelope and error-detail
  hash evidence before governance dispatch.
- Redacted live TypeDID verifier failure boundaries so malformed/rejected
  envelopes and verified-subject mismatches expose only envelope, error-detail,
  and principal hash evidence before governance dispatch.
- Redacted unsupported TypeSec credential resolver provider-scheme
  diagnostics so provider detection returns only `secret-ref-hash` evidence.
- Redacted configured TypeSec environment and Vault credential resolver
  failures so backend lookup and secret payload parse errors expose only
  `secret-ref-hash` and `error-detail-hash` evidence.
- Hardened metadata cleanup conflict reporting so cleanup failures appended to
  preserved commit conflicts include only `error-detail-hash` evidence.
- Added hash evidence to malformed outbox table/principal decode diagnostics,
  proving corrupt pending records fail without echoing raw event IDs.
- Redacted corrupt pending outbox event diagnostics so malformed projection
  payloads report only an outbox event-id hash and fail before graph, lineage,
  or acknowledgement side effects.
- Rejected live agent delegation and agent summary proof headers unless the
  request carries an agent-shaped identity, returning only hash evidence and
  proving catalog config reads fail before governance dispatch.
- Rejected live request-identity TypeDID proof headers without a matching
  TypeDID envelope header, returning only `typedid-proof-hash` evidence and
  proving catalog config reads fail before governance dispatch.
- Added requested/effective scan projection evidence to scan-planned replay
  summaries and QGLake handoff verification, so durable replay proves policy
  narrowing instead of relying only on the HTTP plan response.
- Made outbox draining defensively order pending batches by `created_at,event_id`
  before projection and acknowledgement, keeping QueryGraph/OpenLineage replay
  deterministic even if a store implementation returns an unsorted batch.
- Pinned storage-profile issuance/provider mismatch diagnostics so local-vs-
  remote credential-mode errors carry `storage-profile-prefix-hash` evidence and
  management routes do not echo raw prefixes or secret references.
- Added management-route coverage proving decorated storage-profile location
  prefixes fail with hash-only evidence and do not echo raw prefixes, query
  tokens, or userinfo.
- Rejected storage-profile location prefixes with query strings, fragments, or
  URI userinfo before memory or Turso persistence, returning only
  `storage-profile-prefix-hash` evidence.
- Pinned Turso idempotency mismatch redaction so durable-store reused-key
  conflicts and replay probes do not echo raw idempotency keys, mismatched
  request hashes, or mismatched metadata object locations.
- Added REST credential-vending coverage proving malformed JSON-LD ODRL
  allowed-column lists fail before credential issuer dispatch and before
  `credentials.vend-attempted` replay evidence is emitted.
- Added REST `fetchScanTasks` coverage proving malformed JSON-LD ODRL
  allowed-column lists fail before Sail fetch execution and before
  `table.scan-tasks-fetched` replay evidence is emitted.
- Added REST scan-planning coverage proving malformed JSON-LD ODRL allowed-column
  lists fail before Sail planning and before `table.scan-planned` replay
  evidence is emitted.
- Accepted compact JSON-LD `@value` and `@list` right operands for bounded
  ODRL allowed-column, purpose, and credential-TTL constraints while keeping
  malformed JSON-LD allowed-column lists fail-closed.
- Rejected duplicate pending outbox event IDs before graph or lineage
  projection, returning only a duplicate event-id hash so a corrupted pending
  batch cannot duplicate downstream side effects.
- Tightened the local dependency-contract audit so every GitHub workflow file,
  not only `.github/workflows/ci.yml`, is scanned for forbidden automatic cloud
  triggers while CI remains manual-only.
- Tightened view receipt-chain verifier coverage for chain-head invariants:
  chains must start with a version-1 upsert, no previous-link fields, and no
  zero-version or first-receipt tombstone.
- Tightened view receipt-chain verifier coverage so forged previous-receipt
  hashes and unsupported operations fail the compact QueryGraph/QGLake chain
  check.
- Added service coverage proving view mutations reject
  `expected-view-version=0` before changing the active view or appending
  version receipts.
- Added route-level `fetchScanTasks` coverage proving malformed active ODRL
  restrictions fail before Sail fetch execution and before scan-task fetch
  replay evidence is emitted.
- Pinned metadata-object store setup redaction so invalid metadata URI parsing
  and unsupported backend setup failures return only metadata-location and
  backend-error hashes, not raw paths, object names, schemes, or backend text.
- Pinned metadata-object storage-root protection so planned metadata writes must
  target child objects under the selected storage profile root, returning only
  metadata-location and storage-profile-prefix hashes when the root itself is
  submitted.
- Accepted compact JSON-LD `@id` term objects for bounded ODRL constraint
  `leftOperand` and `operator` values, keeping LakeCat compatible with common
  ODRL encodings without broadening catalog-local policy semantics.
- Accepted prefixed JSON-LD ODRL constraint operand keys
  (`odrl:leftOperand`, `odrl:rightOperand`) for the enforceable read-restriction
  subset while preserving the same fail-closed missing-right-operand checks.
- Made recognized ODRL read-restriction constraints fail closed when
  `rightOperand`/`right-operand` is missing, so allowed-column, row-predicate,
  purpose, and credential-TTL constraints cannot be silently ignored. The scan
  and credential routes now prove malformed active policy fails before Sail
  planning, credential issuer dispatch, or outbox emission.
- Pinned metadata cleanup safety so rejected-commit cleanup skips the previous
  committed metadata pointer instead of deleting the table's current metadata
  object when a future plan accidentally reports that location as a staged
  write.
- Redacted storage-profile public-config validation failures so secret-looking,
  reserved, or value-rejected public config entries report
  `public-config-key-hash` evidence without echoing submitted keys or values.
- Redacted production resolver secret-ref parse failures for TypeSec-gated
  credential issuance, so provider detection plus Vault and TypeSec environment
  resolver parsing return `secret-ref-hash` evidence without echoing malformed
  credential-root strings.
- Made storage-profile secret-ref validation consistently hash-only for invalid
  URI, decorated URI, and embedded-secret failures, returning
  `secret-ref-hash` evidence without echoing the submitted credential-root URI.
- Redacted storage-profile provider/location mismatch errors so unsupported or
  contradictory credential roots report provider labels and
  `storage-profile-prefix-hash` evidence without echoing raw storage prefixes.
- Made outbox draining reject partial acknowledgement mismatches after
  successful graph/lineage projection, so a drain cannot silently report success
  when fewer events were marked delivered than the projected batch.
- Composed all ODRL `max-credential-ttl-seconds` sources within each policy
  document to the tightest cap, including direct read-restriction fields and
  constraint forms, before composing caps across active bindings.
- Made ODRL read-restriction purpose composition fail closed when a policy
  document or multiple active policy bindings carry conflicting purposes,
  preventing first-purpose-wins parsing from authorizing agent reads under an
  ambiguous purpose.
- Made storage-profile selection fail closed when multiple profiles in the same
  warehouse match a table with the same longest location prefix, returning only
  profile ids and a redacted location-prefix hash instead of choosing an
  arbitrary credential/metadata root.
- Bound saved QGLake handoff-verifier lineage-drain identity semantics back to
  the compact request-identity proof, rejecting rehashed verifier artifacts
  whose saved drain principal, authorization receipt, source/state, or TypeDID
  hash slots drift from accepted replay evidence.
- Made QGLake handoff verifier output preserve lineage-drain request identity
  semantics: `lineageDrainArtifactSemantics` now reports the accepted
  request-identity source/state and TypeDID envelope/proof hash slots alongside
  the already verified principal, authorization receipt, and QueryGraph hashes.
- Reconciled live QGLake handoff verification with replay semantics: preserved
  failed drain artifacts for diagnosis, suppressed restricted-agent raw
  credential exception reasons in lineage-drain summaries, kept request and
  bootstrap receipt/TypeDID proofs independently shaped rather than forced
  equal, and made handoff summaries carry explicit `secretRefHash: null`
  evidence for no-secret storage profiles.
- Tightened the local dependency-contract audit so manual-only CI also rejects
  `pull_request_target`, merge-queue, repository-dispatch, and reusable-workflow
  triggers.
- Tightened the local dependency-contract audit so manual-only CI also rejects
  scheduled and workflow-chained GitHub Actions triggers.
- Pinned REST idempotency side-effect behavior so exact replay and mismatched
  reused-key conflicts do not enqueue extra table-commit outbox events.
- Pinned REST idempotency mismatch redaction so reused-key conflicts do not
  echo the raw idempotency key or mismatched metadata object location.
- Pinned outbox drain retryability for graph projection failures: a failing
  graph sink now has focused coverage proving lineage is not emitted and the
  pending outbox event is not acknowledged.
- Pinned compact QGLake `credentialVendingProof` validation with negative
  coverage for credential-branch secret-reference provider/hash evidence.
- Tightened QGLake source replay credential-root validation so each credential
  replay branch rejects malformed secret-reference provider/hash evidence
  directly instead of relying on later storage-profile mismatch checks.
- Tightened QGLake source replay verification to reject restricted-agent
  credential replay events that carry a raw credential exception reason.
- Tightened compact QGLake `credentialVendingProof` verification to reject a
  restricted-agent proof that carries a raw credential exception reason; only
  the trusted-human audited exception path may carry that reason.
- Tightened QGLake source replay verification to reject effective scan
  stats-field evidence that was not present in the requested stats fields.
- Tightened compact QGLake handoff verification to reject effective scan
  stats-field evidence that was not present in the requested stats fields.
- Tightened compact QGLake handoff verification to reject governed scan proofs
  that omit effective stats-field evidence.
- Tightened compact QGLake handoff verification to reject governed scan proofs
  that omit fetched required-filter evidence.
- Tightened QGLake `fetchScanTasks` verification to reject fetched scan-task
  responses that omit the required filter proof for the server-derived row
  predicate.
- Pinned default REST `fetchScanTasks` coverage proving LakeCat re-sends the
  required policy projection and mandatory filters to Sail while auditing the
  replay proof.
- Pinned default REST scan-planning coverage proving LakeCat sends only the
  effective policy projection and mandatory filters to Sail while auditing both
  requested and effective replay evidence.
- Enforced storage-profile scope at the `loadCredentials` service boundary so
  custom credential issuers cannot return broader prefixes than the catalog
  profile selected for the table.
- Rejected metadata-object commit locations containing URI query strings,
  fragments, or URI userinfo before object-store writes, returning hash-only
  metadata-location evidence.
- Pinned the blocked-agent credential-vend outbox contract so governed
  Sail-planned-read decisions record an explicit empty
  `credential-response-evidence` array instead of omitting response proof.
- Pinned route-level credential-vend outbox coverage proving trusted-human raw
  credential exceptions record redacted canonical `credential-response-evidence`
  without storing the raw credential prefix in replay proof.
- Added redacted credential response evidence to credential-vend audit/outbox
  payloads, hashing credential prefixes and issuer-owned config so replay can
  prove LakeCat's canonical `loadCredentials` evidence without leaking raw
  session credentials.
- Canonicalized LakeCat-owned credential response evidence so an issuer cannot
  shadow `lakecat.storage-profile-id`, provider, mode, principal, governed-read
  requirement, or TTL proof in the public `loadCredentials` response.
- Rejected reserved storage-profile `public-config` keys such as
  `lakecat.storage-profile-id`, so operator-supplied routing hints cannot
  shadow LakeCat-owned credential evidence in memory, Turso, or management API
  paths.
- Added REST-level credential-vending coverage proving the public
  `loadCredentials` response collapses duplicate backend TTL config entries to
  one effective `lakecat.max-credential-ttl-seconds` value.
- Normalized credential-vending TTL evidence so every returned credential has
  exactly one `lakecat.max-credential-ttl-seconds` entry, preserving stricter
  issuer TTLs while removing duplicate or wider backend-supplied TTL entries.
- Pinned the REST stale-commit cleanup contract: a metadata-object commit that
  loses compare-and-swap still removes its uncommitted object and returns
  hashed expected/actual metadata-pointer evidence without raw metadata paths.
- Tightened the local dependency-contract audit to prove `grust-cypher` 0.9.0
  resolves from crates.io under `--all-features`, covering the Grust Cypher
  graph-boundary used by `grust-local` tests instead of only checking
  `grust-graph` directly.
- Redacted metadata-object backend error details: object-store setup, write,
  and cleanup failures now expose `error-detail-hash=sha256:...` evidence
  instead of raw backend error text that may contain paths or configuration.
- Pinned lineage/graph outbox draining to an all-or-retry acknowledgement
  contract: if projection fails, the drain fails before marking any event
  delivered, leaving committed catalog events pending for retry.
- Made pending outbox replay deterministic across embedded memory and Turso
  stores by ordering undelivered events by `created_at,event_id`, and made
  delivery marking duplicate-safe so repeated event IDs cannot inflate receipt
  counts.
- Bound storage-profile secret-reference hashes into lineage-drain and QGLake
  replay proof: storage-profile upsert evidence now carries `secretRefHash`
  when a secret reference is present, and compact handoff verification rejects
  missing, malformed, or contradictory secret-ref hash evidence.
- Redacted storage-profile management responses: upsert/list responses now
  return secret-reference presence, provider, and hash evidence instead of the
  raw `secret-ref` locator.
- Rejected storage-profile location prefixes containing literal or
  percent-encoded dot path segments before memory/Turso persistence, reporting
  only a storage-profile-prefix hash for traversal-shaped credential roots.
- Required storage-profile selection to respect location-prefix boundaries, so
  a profile for `s3://bucket/events` can match that root or child paths but not
  a sibling such as `s3://bucket/events-shadow`.
- Re-audited OPUS consolidation: `DESIGN.md` now records the current archive
  invariant, `docs/completed/README.md` records the latest audit, and the
  active tree keeps all OPUS files frozen under `docs/completed/`.
- Rejected storage-profile secret references containing literal or
  percent-encoded dot path segments before persistence or resolver dispatch,
  keeping credential roots as clean external secret-store locators with
  hash-only error evidence.
- Rejected metadata-object commit locations containing literal or
  percent-encoded dot path segments before object-store writes, keeping
  create-only metadata writes constrained to plainly addressed child objects
  under the selected storage profile.
- Bound requested/effective scan stats-field evidence into QGLake replay and
  handoff verification: scan-planned audit summaries now carry both arrays,
  `qglake-verify-replay` rejects loss or widening, the local handoff bridge
  preserves the proof, and compact handoff verification compares captured
  replay against the accepted summary.
- Preserved requested and effective scan stats-field evidence in governed scan
  plan extensions, so replay can prove policy narrowed metadata/stat requests
  without losing what the client originally asked for.
- Rejected metadata-object commit plans whose new metadata location is the
  selected storage-profile root instead of a child object, preventing the commit
  path from attempting create-only writes at the table/storage root.
- Rejected TypeSec-authorized secret-manager credentials whose returned prefix
  falls outside the selected LakeCat storage-profile prefix, keeping production
  credential backends from widening catalog-owned storage scope after
  authorization.
- Completed the OPUS consolidation routing by adding an adjacent-document merge
  ledger to `DESIGN.md` and pointing the completed-review archive README at
  that ledger, so OPUS-derived guidance now has one active home across design,
  architecture, goal, agent, status, and book docs.
- Tightened REST commit idempotency-key validation so non-ASCII and invalid
  header bytes fail under the documented ASCII key contract before
  authorization, Sail commit preparation, table loading, or metadata-object
  writes can run.
- Bound saved QGLake handoff-verifier semantic sections back to the compact
  summary, rejecting self-verifier output whose captured replay semantics,
  bootstrap-bundle semantics, QueryGraph import-plan semantics, lineage-drain
  semantics, or saved graph counts drift from the accepted handoff.
- Bound saved QGLake handoff-verifier artifact hashes back to the compact
  summary, rejecting self-verifier output whose bundle, lineage-drain,
  QueryGraph import-plan, captured-output, or service-log hashes drift from the
  accepted handoff artifact manifest.
- Bound saved QGLake handoff-verifier output back to compact summary semantics:
  when `lakecatHandoffVerifyOutputHash` is present, the verifier now rejects
  saved self-verifier output whose table/view ids, standards, request identity,
  or QueryGraph bootstrap proof drift from the summary.
- Bound QueryGraph import-plan graph counts to the verified bootstrap bundle
  graph counts during QGLake handoff verification, rejecting saved import plans
  that preserve table/view ids and hashes while dropping graph material.
- Collapsed the repeated OPUS archive/consolidation notes in `DESIGN.md` into a
  single canonical OPUS consolidation section with one source ledger and archive
  lock, and refreshed `docs/completed/README.md` to point historical OPUS review
  files back to that living design section.
- Required compact QGLake bootstrap proof to preserve the same request identity
  source and verification state as `requestIdentityProof`, rejecting summaries
  that splice bootstrap evidence across identity paths.
- Bound saved QGLake handoff-verifier output as a compact summary artifact:
  final local handoff summaries now carry `lakecatHandoffVerifyOutputHash`, and
  the verifier checks that JSON artifact is a verified result for the same
  catalog scope.
- Preserved live QGLake replay evidence in the local handoff summary bridge,
  carrying governed scan graph/restriction proof, management graph proof,
  storage-profile graph proof, credential exception/blocking proof, and table
  commit-history graph proof into the compact Rust verifier input.
- Added service-level coverage for invalid REST commit idempotency keys,
  proving illegal or overlong `x-lakecat-idempotency-key` values fail before
  catalog commit work begins.
- Required compact QGLake handoff summaries to bind `catalogUrl` to an
  absolute HTTP(S) endpoint instead of accepting any non-empty string.
- Required QGLake handoff summaries to carry and verify a SHA-256 service log
  hash so saved operational logs cannot drift from the compact artifact
  manifest.
- Recorded the final OPUS archive reconciliation in `DESIGN.md` and
  `docs/completed/README.md`, making the root design the sole active synthesis
  and keeping `docs/completed/OPUS*.md` as provenance-only artifacts.
- Required compact QGLake handoff summaries to carry SHA-256-shaped core
  QueryGraph bundle, graph, OpenLineage, and import proof anchors before
  accepting matching verify/import/bootstrap sections.
- Required compact QGLake handoff summaries to bind
  `queryGraphBootstrapProof.viewVersionReceiptHashes` exactly to
  `viewReceiptChainProof.views[].acceptedReceiptHash`, rejecting spliced view
  receipt evidence without needing the full replay tree.
- Re-audited the OPUS review/design consolidation, refreshed `DESIGN.md` and
  `docs/completed/README.md`, and marked each archived OPUS file as historical
  provenance rather than a live backlog.
- Added scan-plan graph event evidence to QGLake governed scan source replay,
  compact `governedScanProof`, captured replay agreement, and the
  operator-readable scan replay line.
- Added management-list graph event counts to compact QGLake `managementProof`
  and captured replay agreement, so handoff proof preserves the graph
  projection evidence required by source replay.
- Required QGLake lineage-drain management-list source replay to carry catalog
  graph projection evidence before server, project, warehouse, policy, or
  storage-profile list proof can feed compact handoff verification.
- Required QGLake lineage-drain bootstrap source replay to match accepted
  QueryGraph view-version receipt hashes exactly, rejecting receipt-hash drift
  before view proof can feed compact handoff verification.
- Required QGLake lineage-drain dropped-view source replay to bind namespace
  receipt-chain evidence to the accepted view's warehouse/namespace and reject
  verified-chain count or coverage drift before compact handoff proof.
- Required QGLake lineage-drain credential source replay to carry complete
  read-restriction evidence on both restricted-agent and trusted-human branches
  and to reject drift between those credential restrictions before handoff
  proof.
- Required QGLake lineage-drain request identity and bootstrap source replay to
  preserve SHA-256-shaped authorization, QueryGraph, agent, and TypeDID proof
  hashes before compact handoff proof.
- Required QGLake lineage-drain scan source replay to preserve matching planned
  and fetched read restrictions and to prove fetched projection/filter
  requirements exactly match the fetched restriction before compact handoff
  proof.
- Required QGLake lineage-drain table commit-history source replay to match the
  compact commit count against sequence-number and commit-hash evidence and to
  reject non-positive or non-increasing commit sequences before handoff proof.
- Consolidated the archived OPUS review/design corpus into `DESIGN.md` with a
  source ledger and reaffirmed `docs/completed/` as provenance-only archive
  storage.
- Required QGLake lineage-drain table commit-history replay to preserve
  SHA-256-shaped commit hashes before pointer-history proof can feed compact
  handoff verification.
- Required QGLake lineage-drain bootstrap, tombstone, and namespace
  receipt-chain replay to preserve SHA-256-shaped view receipt and receipt-chain
  hashes before accepted-view proof can feed compact handoff verification.
- Tightened QGLake lineage-drain replay so bootstrap, scan, credential, view,
  receipt-chain, and table commit-history receipt arrays must contain
  SHA-256-shaped hashes before compact handoff proof can consume them.
- Tightened QGLake lineage-drain management-list replay so server, project,
  warehouse, policy-binding, storage-profile, and storage-profile-upsert
  receipt arrays must contain SHA-256-shaped hashes before compact handoff
  proof is accepted.
- Required compact QGLake `managementProof` and captured replay agreement to
  carry replay and OpenLineage hash arrays for server, project, warehouse,
  policy-binding, and storage-profile list evidence.
- Locked the OPUS consolidation state by recording that the root tree has no
  active `OPUS*.md` files, the four historical OPUS reviews live only under
  `docs/completed/`, and `DESIGN.md` is the implementation-ready synthesis.
- Added compact QGLake `managementProof` verification for server, project,
  warehouse, policy-binding, and storage-profile replay counts, with captured
  replay drift checks.
- Required QGLake accepted-view replay and compact handoff proof to carry
  positive graph event evidence alongside view receipt-chain and OpenLineage
  proof.
- Required QGLake table commit-history replay and compact handoff proof to
  carry positive graph event evidence alongside commit, replay, and OpenLineage
  hashes, and surfaced that evidence in the operator-readable replay line.
- Added the final OPUS consolidation digest and archive audit commands to the
  canonical design/archive docs so future work no longer needs to reopen OPUS
  files for active guidance.
- Required QGLake storage-profile upsert proof to carry and replay-check a
  positive graph event count.
- Required QGLake trusted-human credential proof to carry and replay-check a
  null `blockReason` beside the audited raw-credential exception.
- Required QGLake restricted-agent credential proof to carry and replay-check
  `rawCredentialExceptionAllowed: false`.
- Required QGLake credential replay and compact handoff proof to reject drift
  between restricted-agent and trusted-human credential TTL caps.
- Bound captured QGLake scan replay semantics to the compact
  `fetchedRequiredProjection` and `fetchedRequiredFilters` evidence so terminal
  replay output cannot drift from governed fetch proof.
- Required compact QGLake governed scan proof to carry positive delete-file and
  child-plan-task counts alongside plan-task and file-task counts.
- Required compact QGLake governed scan proof to reject extra
  `fetchedRequiredFilters` beyond the mandatory row predicate.
- Required compact QGLake governed scan proof to reject drift between planned
  and fetched `max-credential-ttl-seconds` values.
- Required compact QGLake handoff summaries to include the full QGLake
  standards set instead of merely agreeing across QueryGraph verify/import and
  LakeCat bootstrap proof sections.
- Cross-checked QGLake bootstrap embedded ODRL policy bindings against the
  structured policy-binding projection so QueryGraph import evidence cannot
  drift from LakeCat-verified restriction evidence.
- Required QGLake bootstrap policy projection verification to preserve the
  policy-derived `max-credential-ttl-seconds` cap before bootstrap evidence can
  feed replay proof.
- Tightened QGLake scan planning and `fetchScanTasks` verification so live
  plan/fetch read-restriction evidence must preserve the policy-derived
  `max-credential-ttl-seconds` cap before replay proof is accepted.
- Completed the OPUS document consolidation audit by recording the full
  OPUS1/OPUS2 corpus-to-canonical-doc mapping in `DESIGN.md` and tightening the
  completed-review archive rules under `docs/completed/`.
- Required QGLake bootstrap policy projection, scan planning, and
  `fetchScanTasks` verification to preserve the read-restriction purpose before
  compact replay or handoff proof can be accepted.
- Surfaced QGLake scan restriction purpose in the operator-readable scan replay
  line so captured terminal evidence shows the same planned and fetched purpose
  required by compact handoff proof.
- Required QGLake governed scan replay and compact handoff proof to carry the
  server-derived restriction purpose, and documented that scan proof must bind
  allowed columns, row predicate, purpose, policy hashes, and credential TTL.
- Bound QGLake lineage-drain credential replay back to the storage-profile
  upsert replay so source replay verification rejects credential events whose
  profile identity, storage-scope hash, or secret-reference state is spliced
  from a different storage profile.
- Bound QGLake credential-vending proof back to the storage-profile upsert proof
  so compact handoff verification rejects credentials whose storage profile,
  storage-scope hash, or secret-reference state drifts from the management
  replay evidence.
- Centralized QGLake verifier SHA-256 hash-shape checks so required hash
  fields, optional hash fields, and hash arrays all use the same shared
  predicate when validating compact handoff and replay evidence.
- Tightened QGLake management replay secret-reference evidence so
  storage-profile upsert replay rejects contradictory secret-ref
  presence/provider fields and the operator-readable management line prints the
  redacted secret-reference state.
- Updated the LakeCat book's saved QGLake replay transcript example so the
  management and credential replay lines show the same storage-scope hash fields
  required by the CLI verifiers.
- Tightened QGLake management replay output so storage-profile upsert replay
  must carry a SHA-256 `location-prefix-hash` and the operator-readable
  management replay line prints the same redacted credential-root storage-scope
  anchor as the structured proof.
- Tightened QGLake credential replay verification so lineage-drain credential
  evidence must carry a redacted storage-scope hash before compact handoff
  summary generation, and surfaced that hash in the operator-readable credential
  replay line.
- Consolidated the OPUS review log and dev-manager working plan into
  `DESIGN.md`, making `docs/completed/OPUS*.md` provenance-only archive inputs
  and refreshing the completed-doc ledger to point at the canonical design
  sections.
- Bound credential-vend replay evidence to a redacted storage scope by adding
  `location-prefix-hash` to credential storage-profile proof and requiring it
  in QGLake compact handoff verification.
- Redacted storage-profile outbox graph and lineage projections so replayed
  payloads carry `location-prefix-hash` instead of raw storage roots while
  preserving management API access to the configured prefix.
- Hardened storage-profile validation so deserialized or manually constructed
  profiles cannot bypass public-config secret-material checks before memory or
  Turso persistence.
- Finalized OPUS archive consolidation by adding a canonical document map and
  archive policy to `DESIGN.md`, expanding `docs/completed/README.md` with a
  consolidation ledger, and marking each OPUS file as historical audit
  provenance.
- Redacted metadata-object commit validation and write errors so current-pointer
  overwrite, existing-object overwrite, and storage-profile-prefix failures
  report metadata/prefix hashes instead of raw object paths.
- Redacted storage-profile secret-reference validation errors for unsupported
  credential-root schemes so catalog persistence rejects bad roots with
  `secret-ref-hash=sha256:...` evidence instead of echoing the submitted URI.
- Redacted Vault and TypeSec environment secret-ref resolver validation errors
  so malformed credential roots report `secret-ref-hash=sha256:...` evidence
  instead of echoing the raw secret URI.
- Redacted production secret-ref resolver not-configured errors so operators get
  the provider label and `secret-ref-hash=sha256:...` evidence without exposing
  the raw Vault/AWS/GCP/Azure secret URI.
- Proved configured production secret-ref credential backends receive
  policy-derived TTL caps by exercising AWS/GCP/Azure provider dispatch with a
  `max-credential-ttl-seconds` cap and requiring the returned credential config
  to preserve it.
- Consolidated the durable OPUS review decisions into `DESIGN.md` with a
  closure map for OPUS1/OPUS2 findings, and added an archive index under
  `docs/completed/` so the OPUS files remain audit history rather than active
  design instructions.
- Tightened the live QGLake `fetchScanTasks` verifier to require the
  `required-projection` and `required-filters` evidence emitted by LakeCat's
  fetch response extension.
- Added fetch-scan-task replay evidence for the exact required projection and
  required filters LakeCat reapplies from the table scan capability, so a
  stateless fetch response and compact QGLake handoff prove the narrowed read,
  not just the policy input.
- Redacted rejected-commit metadata cleanup failures so cleanup context reports
  a metadata-location hash instead of echoing the uncommitted object path.
- Fixed archive-relative links inside the completed OPUS review documents so
  `docs/completed/` remains a usable historical audit trail after consolidation
  into `DESIGN.md`.
- Added audit-safe expected/actual metadata-location hashes to stale pointer
  conflict errors in both memory and Turso stores, without echoing raw metadata
  object locations.
- Made metadata-object cleanup idempotent when a rejected commit's uncommitted
  object is already absent, while preserving the original commit error plus
  cleanup context for real cleanup failures.
- Made the embedded in-memory catalog store emit the same `table.commit`
  audit/outbox evidence as the Turso commit path, including redacted
  idempotency-key hash and authorization receipt, while keeping idempotent
  replay side-effect free.
- Added the policy-derived credential TTL cap to the compact QGLake scan replay
  operator line, so terminal captures now show the TTL preserved by both
  scan-planning and scan-task-fetch read restrictions.
- Bound QGLake credential replay and handoff evidence to the policy-derived
  credential TTL cap, requiring both restricted-agent and trusted-human compact
  credential proofs to carry `maxCredentialTtlSeconds` and rejecting lineage
  replay that omits the underlying `max-credential-ttl-seconds` restriction.
- Carried policy-derived `max-credential-ttl-seconds` into the credential
  issuance contract and returned credential config, so audited raw credential
  exceptions and secret-ref issuers receive a concrete TTL cap instead of only
  recording the cap in the authorization receipt.
- Hardened ODRL restriction parsing so enforceable constraint forms for allowed
  columns, row predicates, purpose, and credential TTL fail closed when they use
  missing or unsupported operators, preventing deny-shaped constraints from
  being interpreted as allowed read restrictions.
- Consolidated the active OPUS review/design guidance into root `DESIGN.md`,
  archived the original OPUS files under `docs/completed/`, and rewired
  `AGENTS.md`, `GOAL.md`, `ARCHITECTURE.md`, and `STATUS.md` so the OPUS files
  are historical audit inputs rather than active instructions.
- Extended the local dependency-contract audit to require the sibling
  QueryGraph Rust importer to preserve and validate LakeCat
  `receipt-chain-hash` view evidence, preventing stale QueryGraph consumers
  from silently dropping accepted view chain proof.
- Required compact QGLake view proofs to bind active accepted-view
  `acceptedReceiptChainHash` values to namespace `receiptChains[].chainHashes`
  evidence, while allowing tombstoned accepted views only when the tombstone
  proof preserves the accepted view version; the local handoff harness enforces
  the same check before writing a summary.
- Bound QueryGraph view import evidence to the durable view receipt chain by
  adding per-view `receipt-chain-hash` evidence to the import compatibility
  contract, bootstrap verification, QGLake replay proof, and handoff verifier.
- Added a pluggable production secret-ref backend dispatch seam for
  `aws-sm://`, `gcp-sm://`, and `azure-kv://` credential roots, keeping TypeSec
  authorization ahead of any backend call and preserving fail-closed behavior
  when no provider backend is configured.
- Made the compact QGLake credential replay operator line include the
  redacted restricted-agent and trusted-human storage-profile anchors plus
  credential-root graph event counts, so captured replay text exposes the same
  compatibility proof required by the structured handoff verifier.
- Required compact QGLake `credentialVendingProof` branches to include the
  redacted credential storage-profile graph evidence, and made saved
  lineage-drain verification reject credential replay that lacks that
  credential-root graph projection.
- Projected `credentials.vend-attempted` replay into redacted
  catalog-facing `StorageProfile` graph events, so QueryGraph can see
  credential-root access attempts without exposing secret references or
  credential material.
- Projected `table.commits-listed` replay into catalog-facing `Commit` graph
  events keyed by table and sequence number, so QueryGraph can see governed
  pointer-log inspection through Grust without LakeCat adding graph query
  behavior.
- Made `lakecat-cli qglake-verify-handoff` verify the legacy handoff artifact
  path aliases against the hashed `capturedOutputs` paths, while requiring the
  service log path to exist and keeping the self-referential verifier-output
  path as a declared output.
- Made `lakecat-cli qglake-verify-handoff` parse and verify the saved
  lineage-drain artifact, regenerating LakeCat replay evidence from the
  archived drain JSON and rejecting handoffs whose saved outbox/lineage replay
  no longer matches the compact replay proof.
- Made `lakecat-cli qglake-verify-handoff` parse and verify the saved
  QueryGraph import-plan artifact, binding archived import plans to the compact
  QueryGraph import proof, accepted table/view ids, semantic hashes, standards,
  and graph node/edge evidence.
- Made `lakecat-cli qglake-verify-handoff` parse and re-verify the saved
  QueryGraph bootstrap bundle artifact, binding archived handoffs to the same
  tenant graph proof, hashes, counts, standards, and verified table/view ids as
  the compact summary.
- Tightened QGLake bootstrap verification so accepted bundles must prove the
  graph path from Catalog to Server, Project, Warehouse, Namespace, and the
  table, rejecting handoffs that detach a namespace from its tenant spine.
- Made QueryGraph bootstrap graphs prefer durable LakeCat management records
  for the Server > Project > Warehouse tenant spine, while preserving the
  deterministic default spine as a compatibility fallback when management rows
  are absent.
- Added a manifest-hashed QueryGraph bootstrap tenant spine so exported catalog
  graphs now carry deterministic Server, Project, and Warehouse anchors plus
  Warehouse-to-Namespace edges before table/view import.
- Added a catalog-facing `Server` graph anchor for `server.upserted` outbox
  replay so the durable tenant root now reaches Grust through the same thin
  catalog-event boundary as Project, Warehouse, StorageProfile, and the table
  graph anchors.
- Added a catalog-facing `StorageProfile` graph anchor for
  `storage-profile.upserted` outbox replay, using a stable warehouse-scoped
  subject and the same redacted secret-reference evidence as OpenLineage so
  QueryGraph can reason about credential roots without LakeCat exposing secret
  URIs or owning graph query behavior.
- Recorded the reconciled sibling Sail state after scoped local commits:
  Iceberg REST model exposure, manifest-bound Avro preservation, and the Sail
  Cypher graph query extension are committed on `/Users/alexy/src/sail`
  `codex/graph`; only untracked Sail artifact/book directories remain, and
  upstream push is still blocked by HTTPS GitHub authentication.
- Strengthened the local dependency-contract audit so it now verifies the
  Sail helper API surface LakeCat depends on in the local Sail checkout, not
  only the presence of checked-in Sail patch files.
- Made compact QGLake handoff import proofs self-contained by embedding
  `querygraphImportVerification` table/view ids, counts, hashes, and standards
  and requiring them to match `querygraphVerification` plus the captured
  QueryGraph import output.
- Made captured QueryGraph verify/import semantics compare their
  `verified-tables` and `verified-views` arrays exactly against compact
  `querygraphVerification.verifiedTables` and `verifiedViews`, rejecting
  handoffs where the summary and saved captures name different id sets.
- Made compact QGLake handoff summaries self-contained for QueryGraph scope by
  embedding `querygraphVerification.verifiedTables` and `verifiedViews`,
  validating their counts against `tableCount`/`viewCount`, and requiring them
  to include the declared table scope plus every accepted replayed view id.
- Bound compact QGLake handoff view scope to QueryGraph verification by making
  `lakecat-cli qglake-verify-handoff` and the local handoff harness require
  every accepted LakeCat view stable id from `viewReceiptChainProof` to appear
  in QueryGraph `verified-views`.
- Pinned the latest user-supplied LakeCat `AGENTS.md` operating contract in
  `GOAL.md`, including the thin catalog boundary, sibling-repo placement,
  QueryGraph integration target, Turso preference, local-first verification,
  book workflow, and changelog-before-commit discipline.
- Bound compact QGLake handoff table scope to QueryGraph verification by making
  `lakecat-cli qglake-verify-handoff` and the local handoff harness require the
  declared `warehouse`/`namespace`/`table` to appear in QueryGraph
  `verified-tables`.
- Made compact QGLake handoff summaries require non-empty `catalogUrl`,
  `warehouse`, `namespace`, and `table` scope fields, and made
  `lakecat-cli qglake-verify-handoff` reject captured QueryGraph verify/import
  outputs whose warehouse drifts from the summary.
- Tightened compact `requestIdentityProof` and `queryGraphBootstrapProof`
  TypeDID hash-slot validation in `lakecat-cli qglake-verify-handoff` and the
  local QGLake handoff harness so optional TypeDID envelope/proof hashes must
  be null or SHA-256 values, and a proof hash cannot appear without an
  envelope hash.
- Consolidated the pinned `AGENTS.md` guidance in `GOAL.md` so future LakeCat
  resumes treat the user-supplied repo boundary, QueryGraph integration, Turso,
  book, verification, and commit rules as durable goal state.
- Tightened compact `storageProfileUpsertProof` validation in
  `lakecat-cli qglake-verify-handoff` and the local QGLake handoff harness so
  `secretRefProvider` is required when `secretRefPresent` is true and must be
  null when `secretRefPresent` is false.
- Tightened compact `viewReceiptChainProof` validation in
  `lakecat-cli qglake-verify-handoff` and the local QGLake handoff harness so
  namespace receipt-chain evidence must align `verifiedChainCount` with the
  number of chain hashes and carry enough receipt hashes to cover them.
- Strengthened governed namespace view receipt-chain verification so
  `chain-verified` now requires ordered view-version transitions as well as
  hash links: the first receipt must be a version-1 upsert, later upserts must
  advance exactly one version, and drop tombstones must preserve the accepted
  version while linking to the previous receipt.
- Incorporated the current user-supplied
  `AGENTS.md instructions for /Users/alexy/src/lakecat` block into `GOAL.md`
  as standing goal guidance, including sibling-repo placement, QueryGraph
  integration, Turso, local verification, book, changelog, and commit
  discipline.
- Switched LakeCat's Grust and TypeSec workspace dependencies from sibling
  path pins to the published `grust-graph` 0.9.0 and `typesec` 0.8.0 crates,
  removed the manual CI Grust/TypeSec checkouts, and updated the dependency
  contract audit to keep only the Sail helper bridge local.
- Clarified `GOAL.md` that `/Users/alexy/src/lakecat/AGENTS.md` is durable
  goal state and must be reconciled with the goal across resumes and future
  implementation slices.
- Hardened storage-profile secret-reference validation so LakeCat rejects
  external secret-store URIs with query strings, fragments, or userinfo before
  persisting them in memory or Turso.
- Made compact `governedScanProof` validation require planned and fetched
  OpenLineage hashes in `lakecat-cli qglake-verify-handoff`, matching the live
  QGLake handoff harness's scan replay evidence contract.
- Made compact `tableCommitHistoryProof` validation self-sufficient in
  `lakecat-cli qglake-verify-handoff`, requiring commit-count alignment with
  sequence numbers and commit hashes, positive strictly increasing sequences,
  and replay/OpenLineage hashes.
- Reconciled `GOAL.md` so the current user-supplied `LakeCat Agent Guidance`
  from `AGENTS.md` is pinned as durable goal state across resumes, context
  compaction, and future implementation slices.
- Made compact `storageProfileUpsertProof` validation require a SHA-256
  location-prefix hash and a non-empty secret-reference provider whenever the
  redacted proof says a secret reference is present.
- Made compact `credentialVendingProof` validation self-sufficient in
  `lakecat-cli qglake-verify-handoff`, requiring restricted-agent identity,
  Sail-planned-read block reason, trusted-human identity, audited exception
  reason, and replay/OpenLineage hashes.
- Made `lakecat-cli qglake-verify-handoff` independently reject compact
  `viewReceiptChainProof` summaries whose view receipt-chain evidence lacks
  matching view counts, accepted view identity, accepted view-version proof,
  receipt-chain namespace identity, accepted receipt hashes, tombstone receipt
  hashes, verified chain counts, namespace chain hashes, or replay/OpenLineage
  hashes.
- Mirrored the active `AGENTS.md` contract directly into `GOAL.md`, including
  the repo boundaries, compatibility rules, Turso preference, local verification
  gates, and commit discipline.
- Lifted governed scan read-restriction evidence into QGLake replay and
  `governedScanProof`, and made the handoff verifier reject missing or drifted
  plan/fetch restriction proof.
- Made `lakecat-cli qglake-verify-handoff` independently reject compact
  `viewReceiptChainProof` tombstone receipts whose `expectedViewVersion` is
  missing or does not match the accepted view version.
- Clarified `GOAL.md` that the latest pasted `AGENTS.md` block is the current
  standing operating contract for LakeCat work across resumes and context
  compaction.
- Made the live QGLake fixture drop its transient accepted view with
  `expected-view-version`, lifted the guarded tombstone value into
  `viewReceiptChainProof.tombstoneReceipts`, and made the local handoff harness
  reject tombstone replay that does not prove the accepted expected view
  version.
- Carried guarded view `expected-view-version` evidence through view mutation
  audit/outbox payloads, lineage-drain summaries, and QGLake view replay JSON
  so QueryGraph handoffs can prove optimistic view guards were replayed.
- Pinned the active-thread `AGENTS.md` guidance in `GOAL.md` as standing goal
  input and made the book workflow an explicit part of LakeCat's normal
  development loop.
- Added optional `expected-view-version` guarded view upserts and drops for
  management and catalog REST view routes, with atomic memory/Turso store
  checks that reject stale view replacements or tombstones before appending a
  new view receipt.
- Made `lakecat-cli qglake-verify-handoff` compare the compact
  `tableCommitHistoryProof` and `viewReceiptChainProof` values against the
  captured LakeCat replay JSON, so commit-history and durable view-receipt
  evidence cannot drift between replay artifact and handoff summary.
- Reaffirmed in `GOAL.md` that the current repo-local `AGENTS.md` guidance is
  the canonical standing contract for LakeCat work and must stay synchronized
  with the goal.
- Made `lakecat-cli qglake-verify-handoff` compare compact governed scan proof
  fields against captured LakeCat replay JSON, so Sail-planned read task counts
  and replay/OpenLineage hashes cannot drift between replay and summary.
- Made `lakecat-cli qglake-verify-handoff` compare compact request-identity and
  QueryGraph bootstrap proofs against the captured LakeCat replay JSON, so
  principal, authorization, delegation, summary-signature, and bootstrap hash
  evidence cannot drift between replay and handoff summary.
- Made `lakecat-cli qglake-verify-handoff` compare compact
  `credentialVendingProof` branches against the captured LakeCat replay JSON,
  and updated the local handoff harness to include storage-profile issuance
  mode and location-prefix hash in generated summaries.
- Made `lakecat-cli qglake-verify-handoff` compare the compact
  `storageProfileUpsertProof` against the captured LakeCat replay JSON, so a
  handoff is rejected when credential-root evidence drifts between the replay
  artifact and the summary.
- Mirrored the current `AGENTS.md` CLI formatting verification gate in the
  pinned `GOAL.md` guidance.
- Added a redacted storage-profile location-prefix hash to lineage-drain
  summaries, QGLake replay evidence, and handoff-summary verification so
  credential-root proofs bind to the configured storage scope without exposing
  full location prefixes in the compact proof.
- Added storage-profile issuance mode to lineage-drain summaries, QGLake replay
  evidence, and handoff-summary verification so credential-root proofs preserve
  the configured vending mode without exposing secret material.
- Added a `grust-local` boundary test proving LakeCat `Column` and `Snapshot`
  catalog-event labels survive through the Grust adapter and can be matched
  through Grust Cypher without adding graph query behavior to LakeCat.
- Clarified `GOAL.md` so the current 2026-06-19 `AGENTS.md` instruction block
  is explicitly mirrored as durable goal guidance.
- Made REST metadata-object commits use create-only object-store writes, so an
  existing non-current metadata file is treated as a conflict instead of being
  overwritten.
- Made `lakecat-cli qglake-verify-handoff` parse the captured LakeCat replay
  and QueryGraph verify/import JSON outputs and reject summaries whose saved
  captures no longer agree on schema/status, table/view counts, semantic
  hashes, or standards.
- Consolidated `GOAL.md` so the current `AGENTS.md` guidance is pinned once as
  durable goal direction instead of repeated imported snapshots.
- Added `capturedOutputs` hashes to QGLake `handoff-summary.json` for the
  LakeCat replay, QueryGraph verify, and QueryGraph import captures, and made
  `qglake-verify-handoff` reject tampered captured output files.
- Made `lakecat-cli qglake-verify-handoff` verify the raw bundle,
  lineage-drain, and QueryGraph import-plan artifact file hashes recorded in
  `handoff-summary.json`, so stale or tampered handoff files fail acceptance.
- Added `lakecat-cli qglake-verify-handoff --summary ... [--json]` to validate
  the compact QGLake handoff summary schema and proof objects, and made the
  local handoff harness run it after writing `handoff-summary.json`.
- Added compact QueryGraph bootstrap replay evidence to `lakecat-cli
  qglake-verify-replay --json` and lifted it into
  `lakecatReplayVerification.queryGraphBootstrapProof` in
  `handoff-summary.json`, proving QueryGraph bootstrap/import hashes, artifact
  counts, policy count, standards, agent proof hashes, and replay/OpenLineage
  sink hashes.
- Added a dedicated pinned LakeCat agent-guidance section near the top of
  `GOAL.md`, mirroring the current repo boundaries, compatibility rules,
  implementation priorities, verification gates, and commit discipline from
  `AGENTS.md`.
- Added a compact `requestIdentityProof` object to QGLake
  `handoff-summary.json`, proving the replay principal, principal kind,
  request-identity source/state, authorization receipt hash, and sanitized
  TypeDID envelope/proof hash slots.
- Added a compact `viewReceiptChainProof` object to QGLake
  `handoff-summary.json`, proving accepted view versions, tombstone receipts,
  namespace receipt-chain hashes, and replay/OpenLineage hashes.
- Added the current LakeCat `AGENTS.md` guidance snapshot to `GOAL.md`,
  including repo boundaries, compatibility rules, Turso direction, verification
  gates, and commit discipline.
- Added a compact `tableCommitHistoryProof` object to QGLake
  `handoff-summary.json`, proving pointer-log commit-history replay with
  sequence numbers, commit hashes, and replay/OpenLineage hashes.
- Added a compact `governedScanProof` object to QGLake `handoff-summary.json`,
  proving scan planning and scan-task fetch replay with file/delete task counts
  plus replay/OpenLineage hashes.
- Added a compact `credentialVendingProof` object to QGLake
  `handoff-summary.json`, proving restricted agents were blocked onto
  Sail-planned reads while trusted humans used the audited raw-credential
  exception.
- Added a compact `storageProfileUpsertProof` object to QGLake
  `handoff-summary.json`, so operators and QueryGraph can verify the
  credential-root proof without parsing the full replay evidence tree.
- Added a dedicated `GOAL.md` book-workflow section requiring substantial
  workflow examples as LakeCat behavior lands.
- Made the local QGLake handoff harness require redacted storage-profile upsert
  replay evidence before writing `handoff-summary.json`.
- Printed redacted storage-profile upsert proof in QGLake management replay
  output and structured replay JSON.
- Added compact redacted storage-profile upsert evidence to lineage-drain
  summaries and QGLake replay verification.
- Redacted storage-profile secret references from upsert audit/outbox replay
  payloads, preserving only presence and provider evidence for lineage.
- Rejected storage-profile `public-config` values that appear to embed raw
  secret material, and documented public config as non-secret routing metadata.
- Pinned the supplied `/Users/alexy/src/lakecat/AGENTS.md` operating guidance
  directly into `GOAL.md` as durable goal execution guidance.
- Rejected unsafe storage-profile issuance/provider combinations, including
  remote `local-file-no-secret` profiles and local `short-lived-secret-ref`
  profiles.
- Rejected storage profiles whose declared provider does not match the location
  prefix provider, and refreshed the book's storage-profile examples to use the
  current management API vocabulary.
- Bound metadata-object commit locations to the table's matched storage profile
  prefix, rejecting out-of-profile metadata writes before object storage is
  touched.
- Preserved the original store/CAS commit error when uncommitted metadata-object
  cleanup also fails, appending cleanup context without changing the commit
  error class.
- Rejected metadata-write commit plans that require writing table metadata but
  do not carry a concrete new metadata location, preventing catalog-pointer
  commits from succeeding without a corresponding metadata object.
- Rejected metadata-object commits that would write new metadata to the
  table's current metadata pointer, preventing current metadata files from
  being overwritten before CAS/store validation.
- Made `GOAL.md` explicitly import the user-supplied
  `/Users/alexy/src/lakecat/AGENTS.md` instructions as permanent goal
  constraints rather than temporary chat context.
- Refactored QGLake replay verification JSON behind a testable helper and
  added coverage for the schema version plus structured replay evidence fields.
- Added explicit schema-version fields for QGLake replay verification JSON and
  the local handoff summary, and made the handoff harness require the replay
  schema before accepting artifacts.
- Added structured scan, management, credential, and table-commit replay
  evidence to `lakecat-cli qglake-verify-replay --json` and embedded that
  object in the local handoff summary.
- Extended QGLake replay JSON and the local handoff summary to cross-check
  graph/OpenLineage hashes and the standards list across LakeCat replay,
  QueryGraph verify, and QueryGraph import.
- Added JSON output to `lakecat-cli qglake-verify-replay` and made the local
  handoff summary cross-check LakeCat replay hashes against QueryGraph verify
  and import hashes before accepting the artifact set.
- Made the local QGLake handoff summary fail closed unless QueryGraph
  `lakecat-verify` and `lakecat-import` agree on table/view counts and semantic
  bundle/graph/OpenLineage/import hashes.
- Embedded QueryGraph verification counts and semantic bundle/graph/OpenLineage
  import hashes directly in `handoff-summary.json`, while retaining raw file
  hashes for the generated artifacts.
- Made `scripts/qglake-handoff-local.sh` write a machine-readable
  `handoff-summary.json` plus captured LakeCat replay and QueryGraph
  verify/import outputs for operator and automation handoff.
- Added `scripts/qglake-handoff-local.sh`, a local-first handoff harness that
  starts LakeCat, generates paired QGLake bootstrap/drain artifacts, verifies
  saved replay with LakeCat, and runs QueryGraph's `lakecat-verify` and
  `lakecat-import` over the same bundle without writing into the QueryGraph
  checkout.
- Added compact scan/fetch task counts to lineage-drain event summaries and
  made QGLake saved replay require `table.scan-planned` plus
  `table.scan-tasks-fetched` evidence, including delete-file counts for
  governed Sail-planned reads.
- Clarified `GOAL.md` so the latest `/Users/alexy/src/lakecat/AGENTS.md`
  guidance is imported as permanent operating direction for repo boundaries,
  QueryGraph integration, Turso, feature gates, verification, and
  CHANGELOG-before-commit discipline.
- Tightened QGLake saved replay acceptance to require the trusted-human raw
  credential exception reason, and made `qglake-verify-replay` print compact
  restricted-agent and trusted-human credential replay evidence.
- Made `lakecat-cli qglake-verify-replay` print compact management replay
  counts for servers, projects, warehouses, policy bindings, and storage
  profiles after accepting a saved QGLake drain.
- Made `lakecat-cli qglake-verify-replay` print the verified table
  commit-history replay summary, including compact commit count, sequence
  numbers, and commit hashes for QueryGraph/operator handoff.
- Pinned the latest LakeCat `AGENTS.md` operating contract into `GOAL.md` with
  explicit repo-boundary, compatibility, implementation-priority, verification,
  Turso, graph-placement, and commit-discipline sections.
- Added compact table commit-history count, sequence-number, and commit-hash
  fields to lineage-drain event summaries, and made QGLake reject
  `table.commits-listed` replay that omits that typed summary evidence.
- Tightened QGLake commit-history acceptance to require Iceberg format-version
  and snapshot summary evidence in the compact pointer-log record, with a
  focused CLI regression for missing summary fields.
- Made QGLake acceptance perform an idempotent table commit-history probe and
  require lineage-drain replay to include `table.commits-listed` receipt
  evidence, binding the pointer-log management read into the end-to-end
  QueryGraph handoff.
- Added a governed table commit-history management read that exposes compact
  pointer-log evidence and records replayable lineage/outbox proof for
  QueryGraph and operators.
- Added a service regression proving exact REST commit retries replay before
  metadata-object writes by preserving the committed object unchanged on
  idempotent replay.
- Added `format_version`, `snapshot_id`, and `policy_hash` to durable table
  commit records so pointer-log, audit/outbox, graph, and lineage replay carry
  compact Iceberg and governance summary evidence for each committed pointer.
- Added a durable `response_hash` to table commit records so pointer-log,
  audit/outbox, graph, and lineage replay can prove the exact stored commit
  response alongside the request hash used for idempotency.
- Added a pre-Sail commit idempotency replay probe on the `CatalogStore` seam so
  exact REST commit retries return the stored response before Sail validation or
  metadata-object writes, with direct Turso coverage and a stale-requirement
  replay regression test.
- Added focused `lakecat-sail` v4 fetch-token fixtures proving that the JSON
  bridge accepts signed manifest-list plan tasks during `fetchScanTasks` while
  rejecting drifted manifest-list metadata without claiming typed v4 support.
- Added a local dependency-contract audit script and wired it into manual CI so
  the Grust/TypeSec versioned path pins, Sail path/patch bridge, and
  manual-only workflow state fail fast when they drift.
- Added focused `lakecat-sail` v4 extension fixtures for JSON-summary
  inspection, manifest-list scan planning, and stable commit-requirement
  validation while keeping typed v4 metadata explicitly pending on Sail.
- Added `chain-verified` validation to governed namespace view receipt chains,
  replayed the verified-chain count through lineage-drain summaries, and made
  QGLake dropped-view acceptance require a verified namespace chain proof.
- Added deterministic `chain-hash` proofs to governed namespace view receipt
  chains, replayed those chain hashes through lineage-drain summaries, and made
  QGLake dropped-view acceptance require compact namespace receipt-chain proof.
- Pinned the pasted `/Users/alexy/src/lakecat/AGENTS.md` operating contract in
  `GOAL.md` so future work keeps goal guidance, repo boundaries, verification,
  and commit discipline synchronized.
- Chained durable view-version receipts by adding `previous-receipt-hash` to
  upsert and drop receipts in memory/Turso stores and exposing the link through
  governed view receipt management responses.
- Made QGLake policy fixtures use canonical Iceberg REST filter spelling
  (`not-eq`), taught lineage-drain summaries to read current TypeSec
  request-identity receipts under `authorization-receipt.context`, deduplicated
  shared namespace nodes in QueryGraph bootstrap graph projections, and proved a
  regenerated Sail-backed QGLake bundle through QueryGraph's Rust
  `lakecat-verify` and `lakecat-import` commands.
- Reconciled `GOAL.md` with the current pasted LakeCat `AGENTS.md` guidance,
  including the `/Users/alexy/src/lakecat/AGENTS.md` permanence rule and the
  CLI-specific local verification gate for QGLake fixture changes.
- Added `lakecat-cli qglake-verify-replay` to verify a saved QueryGraph
  bootstrap bundle together with a saved lineage-drain response, and added
  `qglake-fixture --drain-output` so local QGLake runs can emit both artifacts
  for offline handoff proof.
- Reconciled `GOAL.md` with the current `AGENTS.md` operating contract so the
  durable goal carries one canonical copy of the LakeCat boundary, sibling-repo
  placement, Turso, feature-gate, verification, and commit-discipline guidance.
- Added view receipt evidence to the QueryGraph import compatibility contract,
  making view-bearing bootstrap verification require a compact receipt hash for
  each exported view version.
- Made QGLake consume the namespace-level `view-version-receipt-chains` read
  after dropping its transient view and reject lineage drains that do not replay
  the chain read as compact tombstone evidence.
- Mirrored the current `AGENTS.md` operating contract into `GOAL.md` as a
  permanent goal constraint, including sibling-repo placement, compatibility,
  Turso, feature-gate, outbox, and commit-discipline guidance.
- Added a namespace-level governed `view-version-receipt-chains` read that
  groups active and tombstoned view receipt chains for QueryGraph/operators and
  replays the read as compact lineage evidence.
- Bound QGLake lineage-drain acceptance to view tombstone receipt evidence by
  replaying governed `view.version-receipts-listed` reads into lineage and
  making the fixture create, bootstrap, drop, and receipt-check a transient
  catalog view.
- Recorded compact view drop/tombstone receipts in memory and Turso stores so
  governed receipt-chain reads preserve the last durable view version and
  content hash after the current view row has been deleted.
- Tightened `GOAL.md` with the permanent `AGENTS.md` operating constraints for
  Grust/Sail/TypeSec placement, QueryGraph evidence boundaries, Turso, feature
  gates, and local commit discipline.
- Added a governed management read endpoint for compact view-version receipts
  so QueryGraph and operators can inspect the durable receipt chain without
  using non-standard Iceberg metadata or backend-specific storage access.
- Persisted compact view-version receipts in memory and Turso stores, exposed
  matching receipt hashes through QueryGraph bootstrap replay summaries, and
  required QGLake view-bearing replay to preserve those receipt hashes.
- Added compact `view-version` replay evidence to lineage-drain event
  summaries and made QGLake view replay acceptance compare it with the accepted
  QueryGraph bootstrap view artifact version.
- Mirrored the LakeCat `AGENTS.md` operating guidance into `GOAL.md` so the
  long-running goal permanently carries the thin catalog boundary, sibling-repo
  placement rules, QueryGraph target, Turso preference, and commit/status
  discipline.
- Added a durable, store-assigned `view-version` counter to LakeCat view
  records and responses, and surfaced it through QueryGraph view graph, OSI,
  and OpenLineage handoff artifacts as the first step toward full Iceberg view
  commit semantics.
- Made QGLake acceptance establish and list its durable server, project, and
  warehouse tenant spine, then require lineage-drain replay to expose
  `server.listed`, `project.listed`, and `warehouse.listed` count evidence.
- Made QGLake acceptance exercise the governed storage-profile-list management
  read and require lineage-drain replay to expose matching compact
  `storage-profile.listed` count/scope evidence alongside policy-list evidence.
- Added durable `GOAL.md` guidance that keeps QueryGraph's OSI, OpenLineage,
  Croissant, ODRL, and TypeSec composition as a catalog-facing LakeCat contract
  plus richer QueryGraph integration layer, with local verification before
  cloud CI.
- Made QGLake acceptance exercise the governed policy-list management read and
  require lineage-drain replay to expose matching compact `policy-binding.listed`
  evidence before accepting the workflow.
- Added compact management-list count fields to lineage-drain event summaries
  so QueryGraph can verify replayed policy, project, server, storage-profile,
  and warehouse list evidence without parsing raw lineage payloads.
- Replayed management list outbox events for policy bindings, projects,
  servers, storage profiles, and warehouses into LakeCat OpenLineage receipts
  so control-plane read paths carry durable replay evidence without inventing
  list-specific graph nodes in LakeCat.
- Clarified `GOAL.md` with path-qualified Sail, Grust, TypeSec, and QueryGraph
  boundaries plus explicit feature-gate and Turso durable-spine guidance.
- Replayed `table.restored` outbox events into catalog-facing Table graph
  events alongside the existing LakeCat OpenLineage restore receipt, keeping
  restore evidence durable without adding restore-specific graph semantics in
  LakeCat.
- Replayed `catalog.config-read` outbox events into warehouse-scoped catalog
  graph events and LakeCat OpenLineage receipts so the standard Iceberg REST
  configuration entrypoint carries durable replay evidence.
- Replayed `namespace.listed` and `namespace.loaded` outbox events into
  warehouse/namespace-scoped catalog graph events and LakeCat OpenLineage
  receipts so standard namespace reads carry durable replay evidence.
- Replayed `view.listed` outbox events into namespace-scoped catalog graph
  events and LakeCat OpenLineage receipts so standard view listing reads carry
  durable replay evidence without pretending a list response is a single view.
- Expanded `GOAL.md` with the permanent LakeCat agent guidance: current design
  sources, Turso preference, Iceberg compatibility rules, sibling-repo
  ownership boundaries, feature-gate expectations, and local verification gates.
- Replayed `view.loaded` outbox events into catalog-facing View graph events
  and LakeCat OpenLineage receipts so standard catalog view reads carry the
  same replayable evidence as view management changes.
- Added compact view replay identity to lineage-drain event summaries and made
  QGLake lineage-drain acceptance require replayed view evidence to match the
  accepted QueryGraph bootstrap view artifacts.
- Replayed `view.upserted` and `view.dropped` outbox events into catalog-facing
  View graph events and LakeCat OpenLineage receipts, and added focused
  coverage for Grust ingestion plus service replay counts.
- Made the LakeCat book part of the active development workflow in `GOAL.md`,
  added a substantial workflow-examples chapter spanning service startup,
  warehouse/project/storage-profile setup, PySpark reads, credential vending,
  view management, QueryGraph bootstrap, outbox draining, and agentic QGLake
  flows, then rebuilt the PDF, EPUB, and MOBI artifacts.
- Projected `storage-profile.upserted` outbox events into LakeCat
  lineage/OpenLineage receipts so credential-root management changes carry
  replayable evidence from durable outbox replay.
- Projected `server.upserted` outbox events into LakeCat lineage/OpenLineage
  receipts while leaving reusable server graph hierarchy work to Grust.
- Projected policy-binding, project, and warehouse upsert outbox events into
  LakeCat lineage/OpenLineage receipts so control-plane graph anchors also carry
  replayable lineage evidence.
- Exposed lineage-drain request authorization proof on the management response,
  printed it in the CLI, and required QGLake lineage-drain acceptance to prove
  the drain itself was gated by lineage-read authorization evidence.
- Renamed the docs/book publishing surface from LakeSail to LakeCat, expanded
  the book with catalog-market context, Polaris positioning, Rust-first
  Sail/Iceberg v3-v4 evolution, Responsible Semantic Layer handoff, and
  QueryGraph.ai architecture, then rebuilt the PDF, EPUB, and MOBI artifacts.
- Extended the QGLake fixture metadata to include a position-delete manifest and
  required governed `fetchScanTasks` acceptance to prove Sail attaches
  delete-file refs to data tasks and handles delete-manifest child tasks.
- Projected credential-vend outbox events into LakeCat lineage/OpenLineage sink
  receipts and required QGLake lineage-drain acceptance to prove both credential
  probes emitted replay hashes.
- Added credential-vend replay evidence to lineage-drain summaries and required
  QGLake lineage-drain acceptance to prove both the restricted agent block and
  trusted-human raw credential exception survived outbox replay.
- Allowed trusted human principals to receive audited standard credential
  responses for restricted QGLake tables while keeping agents on the governed
  Sail-planned path, and added QGLake acceptance checks for the contrast.
- Mirrored QueryGraph bootstrap proof hashes, including the import-compatibility
  hash, into the OpenLineage bootstrap facet and pinned lineage replay payload
  tests to preserve the import hash.
- Added QueryGraph import-hash evidence to `querygraph.bootstrap`
  outbox/replay summaries and required QGLake lineage-drain acceptance to match
  it against the accepted bootstrap import contract.
- Added a QueryGraph import-compatibility contract to bootstrap manifests with a
  table-only bundle hash for the current QueryGraph Rust importer and required
  QGLake bootstrap acceptance to preserve that import evidence.
- Added QGLake agent delegation and signed-summary hash evidence to
  lineage-drain replay summaries and required explicit QGLake agent runs to
  preserve both hashes.
- Made QGLake fixture requests use agent-DID identity headers for explicit
  principals and required lineage-drain replay evidence to preserve the
  accepted bootstrap principal kind.
- Added compact request-identity attestation-state evidence to lineage-drain
  replay summaries and required QGLake acceptance to reject bootstrap replay
  that drops the request identity state.
- Added compact authorization-receipt hash evidence to lineage-drain replay
  summaries and required QGLake acceptance to reject unattributed bootstrap
  replay.
- Exposed QueryGraph bootstrap policy-binding counts in lineage-drain replay
  evidence and required QGLake acceptance to match them against the accepted
  bootstrap bundle.
- Exposed the QueryGraph bootstrap replay principal in lineage-drain evidence
  and required QGLake acceptance to match it against the principal used for the
  accepted handoff.
- Exposed QueryGraph bootstrap standards in lineage-drain replay evidence and
  required QGLake acceptance to match them against the accepted bootstrap
  bundle.
- Added per-event graph and lineage projection counts to lineage-drain evidence
  and required QGLake acceptance to prove the drain replayed graph projections
  plus a bootstrap lineage projection.
- Bound QGLake lineage-drain acceptance to the exact QueryGraph bootstrap bundle
  it accepted, rejecting replay evidence whose bundle, graph, OpenLineage, or
  artifact-count hashes drift from the exported handoff.
- Exposed compact lineage-drain replay evidence for QueryGraph bootstrap events
  and required QGLake acceptance to verify bundle, graph, OpenLineage, table
  artifact, and sink receipt hashes.
- Persisted QueryGraph manifest table/view artifact hashes in the
  `querygraph.bootstrap` audit/outbox payload and verified lineage-drain replay
  preserves them.
- Verified the `/querygraph/v1/bootstrap` response exposes OpenLineage
  semantic-bundle table/view artifact hashes that match the QueryGraph manifest.
- Required QGLake bootstrap acceptance to verify every table and view artifact
  listed in the QueryGraph manifest has matching OpenLineage semantic-bundle
  hash evidence, not only the selected fixture table.
- Added QueryGraph artifact-hash evidence to the bootstrap OpenLineage semantic
  bundle facet and required QGLake bootstrap acceptance to verify those hashes
  against the manifest.
- Required QGLake bootstrap acceptance to run the QueryGraph bundle manifest
  verifier, rejecting tampered Croissant/CDIF/OSI/ODRL, graph, OpenLineage, or
  bundle-hash content before accepting the handoff.
- Required QGLake bootstrap OpenLineage acceptance to verify the event type,
  QueryGraph bootstrap job identity, and output data-source URI before accepting
  the handoff bundle.
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
