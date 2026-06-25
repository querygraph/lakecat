#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};

pub(crate) const READ_RESTRICTION_EVIDENCE_FIELDS: &[&str] = &[
    "allowed-columns",
    "row-predicate",
    "purpose",
    "policy-hashes",
    "max-credential-ttl-seconds",
];
pub(crate) const ROW_PREDICATE_EVIDENCE_FIELDS: &[&str] = &["type", "term", "value"];
pub(crate) const PRINCIPAL_EVIDENCE_FIELDS: &[&str] = &["subject", "kind"];
pub(crate) const AUTHORIZATION_RECEIPT_EVIDENCE_FIELDS: &[&str] = &[
    "principal",
    "action",
    "table",
    "allowed",
    "engine",
    "policy_hash",
    "context",
    "request-identity",
    "checked_at",
];
pub(crate) const AUTHORIZATION_RECEIPT_CONTEXT_EVIDENCE_FIELDS: &[&str] = &[
    "warehouse",
    "policy-bindings",
    "read-restriction",
    "lakecat:raw-credential-exception",
    "request-identity",
];
pub(crate) const AUTHORIZATION_RECEIPT_CONTEXT_POLICY_BINDING_FIELDS: &[&str] = &[
    "policy-id",
    "warehouse",
    "namespace",
    "table",
    "enforced",
    "odrl",
];
pub(crate) const TABLE_COMMIT_HISTORY_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "table", "payload"];
pub(crate) const SCAN_PLANNED_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "table",
    "authorization-receipt",
    "planned-by",
    "snapshot-id",
    "plan-task",
    "scan-task-count",
    "storage-location",
    "metadata-location",
    "read-restriction",
    "requested-projection",
    "effective-projection",
    "requested-stats-fields",
    "effective-stats-fields",
    "required-projection",
    "required-filters",
];
pub(crate) const SCAN_PLANNED_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "table", "payload"];
pub(crate) const SCAN_TASKS_FETCHED_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "table",
    "authorization-receipt",
    "planned-by",
    "snapshot-id",
    "plan-task",
    "file-scan-task-count",
    "delete-file-count",
    "child-plan-task-count",
    "storage-location",
    "metadata-location",
    "read-restriction",
    "required-projection",
    "effective-projection",
    "requested-stats-fields",
    "effective-stats-fields",
    "stats-fields",
    "required-filters",
];
pub(crate) const SCAN_TASKS_FETCHED_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "table", "payload"];
pub(crate) const CREDENTIAL_RESPONSE_EVIDENCE_FIELDS: &[&str] = &[
    "prefix-hash",
    "storage-profile-id",
    "storage-provider",
    "credential-mode",
    "authorization-principal",
    "governed-read-required",
    "max-credential-ttl-seconds",
    "secret-ref-provider",
    "secret-ref-hash",
    "issuer-config-entry-count",
    "issuer-config-hash",
    "catalog-profile-id",
    "receipt-principal",
];
pub(crate) const CREDENTIAL_VEND_EVIDENCE_FIELDS: &[&str] = &[
    "audit-event-id",
    "event-type",
    "table",
    "authorization-receipt",
    "storage-location",
    "storage-profile-id",
    "storage-profile",
    "secret-ref-present",
    "credential-count",
    "credential-response-evidence",
    "mode",
    "read-restriction",
    "lakecat:raw-credential-exception",
    "lakecat:credential-block-reason",
];
pub(crate) const CREDENTIAL_VEND_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "table", "payload"];
pub(crate) const RAW_CREDENTIAL_EXCEPTION_EVIDENCE_FIELDS: &[&str] =
    &["requested", "allowed", "reason"];
pub(crate) const STORAGE_PROFILE_EVIDENCE_FIELDS: &[&str] = &[
    "profile-id",
    "warehouse",
    "location-prefix-hash",
    "provider",
    "issuance-mode",
    "secret-ref-present",
    "public-config",
    "secret-ref-provider",
    "secret-ref-hash",
];
pub(crate) const STORAGE_PROFILE_UPSERT_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "warehouse",
    "storage-profile",
];
pub(crate) const STORAGE_PROFILE_UPSERT_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "payload"];
pub(crate) const MANAGEMENT_UPSERT_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "payload"];
pub(crate) const RESERVED_STORAGE_PROFILE_PUBLIC_CONFIG_KEYS: &[&str] = &[
    "lakecat.storage-profile-id",
    "lakecat.storage-provider",
    "lakecat.credential-mode",
    "lakecat.governed-read-required",
    "lakecat.authorization-principal",
    "lakecat.max-credential-ttl-seconds",
    "lakecat.credential-kind",
    "lakecat.secret-ref-provider",
    "lakecat.secret-ref-hash",
];
pub(crate) const POLICY_BINDING_EVIDENCE_FIELDS: &[&str] = &[
    "policy-id",
    "warehouse",
    "namespace",
    "table",
    "enforced",
    "odrl",
    "odrl-hash",
];
pub(crate) const POLICY_BINDING_UPSERT_EVIDENCE_FIELDS: &[&str] =
    &["event-type", "authorization-receipt", "warehouse", "policy"];
pub(crate) const PROJECT_RECORD_EVIDENCE_FIELDS: &[&str] =
    &["project-id", "server-id", "display-name", "properties"];
pub(crate) const PROJECT_UPSERT_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "project-id",
    "project-record",
];
pub(crate) const SERVER_RECORD_EVIDENCE_FIELDS: &[&str] = &[
    "server-id",
    "display-name",
    "endpoint-url",
    "endpoint-url-hash",
    "properties",
];
pub(crate) const SERVER_UPSERT_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "server-id",
    "server-record",
];
pub(crate) const WAREHOUSE_RECORD_EVIDENCE_FIELDS: &[&str] = &[
    "warehouse",
    "project-id",
    "storage-root",
    "storage-root-hash",
    "properties",
];
pub(crate) const WAREHOUSE_UPSERT_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "project-id",
    "warehouse",
    "warehouse-record",
];
pub(crate) const VIEW_RECORD_EVIDENCE_FIELDS: &[&str] = &[
    "warehouse",
    "namespace",
    "name",
    "view-version",
    "sql",
    "dialect",
    "schema-version",
    "columns",
    "properties",
];
pub(crate) const VIEW_LIFECYCLE_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "interface",
    "authorization-receipt",
    "warehouse",
    "namespace",
    "view",
    "expected-view-version",
];
pub(crate) const VIEW_LIFECYCLE_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "payload"];
pub(crate) const CATALOG_CONFIG_READ_EVIDENCE_FIELDS: &[&str] = &[
    "audit-event-id",
    "event-type",
    "authorization-receipt",
    "warehouse",
    "defaults",
    "overrides",
    "endpoints",
    "warehouse-record",
    "project-record",
    "server-record",
];
pub(crate) const CATALOG_CONFIG_ENTRY_EVIDENCE_FIELDS: &[&str] = &["key", "value"];
pub(crate) const QUERYGRAPH_BOOTSTRAP_EVIDENCE_FIELDS: &[&str] = &[
    "audit-event-id",
    "event-type",
    "authorization-receipt",
    "warehouse",
    "table-count",
    "view-count",
    "policy-binding-count",
    "verified-tables",
    "verified-views",
    "verified-view-versions",
    "view-version-receipts",
    "bundle-hash",
    "graph-hash",
    "open-lineage-hash",
    "querygraph-import-hash",
    "table-artifacts",
    "view-artifacts",
    "standards",
];
pub(crate) const QUERYGRAPH_VIEW_RECEIPT_EVIDENCE_FIELDS: &[&str] = &[
    "stable-id",
    "view-version",
    "receipt-hash",
    "receipt-chain-hash",
];
pub(crate) const REQUEST_IDENTITY_EVIDENCE_FIELDS: &[&str] = &[
    "type",
    "principal",
    "source",
    "agent-did",
    "typedid",
    "typedid-envelope-sha256",
    "typedid-proof-sha256",
    "agent-delegation-sha256",
    "agent-summary-signature-sha256",
    "bearer-token-sha256",
    "attestation-state",
    "raw-secret-material",
];
pub(crate) const VIEW_RECEIPT_CHAIN_EVIDENCE_FIELDS: &[&str] = &[
    "stable-id",
    "warehouse",
    "namespace",
    "name",
    "chain-hash",
    "chain-verified",
    "latest-view-version",
    "latest-operation",
    "tombstoned",
    "receipt-count",
    "receipts",
];
pub(crate) const VIEW_RECEIPT_CHAIN_RECEIPT_EVIDENCE_FIELDS: &[&str] = &[
    "stable-id",
    "warehouse",
    "namespace",
    "name",
    "view-version",
    "previous-view-version",
    "previous-receipt-hash",
    "operation",
    "view-hash",
    "receipt-hash",
    "principal-subject",
    "principal-kind",
    "recorded-at",
];
pub(crate) const VIEW_RECEIPT_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "warehouse",
    "namespace",
    "view",
    "receipt-count",
    "receipt-hashes",
    "drop-receipt-hashes",
];
pub(crate) const VIEW_RECEIPT_READ_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "payload"];
pub(crate) const VIEW_RECEIPT_CHAIN_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "warehouse",
    "namespace",
    "chain-count",
    "receipt-count",
    "tombstone-count",
    "chain-verified-count",
    "view-version-receipt-chains",
    "chain-hashes",
    "receipt-hashes",
    "drop-receipt-hashes",
];
pub(crate) const TABLE_IDENTITY_EVIDENCE_FIELDS: &[&str] = &["warehouse", "namespace", "name"];
pub(crate) const TABLE_COMMIT_HISTORY_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "warehouse",
    "namespace",
    "table",
    "commit-count",
    "commit-hashes",
    "sequence-numbers",
    "principal-subject",
    "principal-kind",
];
pub(crate) const TABLE_COMMIT_PAYLOAD_EVIDENCE_FIELDS: &[&str] = &[
    "audit-event-id",
    "event-type",
    "authorization-receipt",
    "warehouse",
    "namespace",
    "table",
    "commit",
];
pub(crate) const TABLE_COMMIT_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "table", "payload"];
pub(crate) const TABLE_COMMIT_EVIDENCE_FIELDS: &[&str] = &[
    "table",
    "previous_metadata_location",
    "previous-metadata-location",
    "new_metadata_location",
    "new-metadata-location",
    "sequence_number",
    "sequence-number",
    "principal",
    "format_version",
    "format-version",
    "snapshot_id",
    "snapshot-id",
    "policy_hash",
    "policy-hash",
    "request_hash",
    "request-hash",
    "response_hash",
    "response-hash",
    "idempotency_key_sha256",
    "idempotency-key-sha256",
    "committed_at",
    "committed-at",
];
pub(crate) const TABLE_LIFECYCLE_EVIDENCE_FIELDS: &[&str] = &[
    "audit-event-id",
    "event-type",
    "authorization-receipt",
    "warehouse",
    "namespace",
    "table",
    "metadata-location",
    "location",
    "format-version",
    "version",
    "soft-delete",
    "metadata-graph",
];
pub(crate) const TABLE_LIFECYCLE_OUTBOX_PAYLOAD_FIELDS: &[&str] = &[
    "audit-event-id",
    "event-type",
    "table",
    "soft-delete",
    "payload",
];
pub(crate) const TABLE_METADATA_GRAPH_EVIDENCE_FIELDS: &[&str] = &[
    "current-schema-id",
    "fields",
    "current-snapshot-id",
    "current-snapshot",
];
pub(crate) const TABLE_LIFECYCLE_SOFT_DELETE_EVIDENCE_FIELDS: &[&str] = &[
    "table",
    "metadata-location",
    "version",
    "format-version",
    "format_version",
    "principal",
    "authorization-receipt",
    "deleted-at",
];
pub(crate) const NAMESPACE_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "warehouse",
    "namespace-count",
    "namespace-paths",
];
pub(crate) const VIEW_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "interface",
    "authorization-receipt",
    "warehouse",
    "namespace",
    "view-count",
    "view-names",
];
pub(crate) const POLICY_BINDING_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "warehouse",
    "policy-count",
    "policy-ids",
];
pub(crate) const PROJECT_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "project-count",
    "project-ids",
];
pub(crate) const SERVER_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "server-count",
    "server-ids",
];
pub(crate) const STORAGE_PROFILE_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "warehouse",
    "storage-profile-count",
    "storage-profile-ids",
];
pub(crate) const WAREHOUSE_LIST_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "project-id",
    "warehouse-count",
    "warehouse-names",
];
pub(crate) const LIST_OUTBOX_PAYLOAD_FIELDS: &[&str] = &["audit-event-id", "event-type", "payload"];
pub(crate) const NAMESPACE_LIFECYCLE_OUTBOX_PAYLOAD_FIELDS: &[&str] =
    &["audit-event-id", "event-type", "payload"];
pub(crate) const NAMESPACE_LIFECYCLE_EVIDENCE_FIELDS: &[&str] = &[
    "event-type",
    "authorization-receipt",
    "warehouse",
    "namespace",
];
