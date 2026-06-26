use super::*;
use crate::{
    CommitPlan, DeferredSailCatalogEngine, FetchScanTasksPlan, FetchScanTasksRequest,
    SailCatalogEngine,
};
use lakecat_core::{LakeCatError, LakeCatResult};
use lakecat_security::{
    AllowAllGovernanceEngine, AuthorizationReceipt, AuthorizationRequest, CatalogAction,
    GovernanceEngine, TableScanCapability,
};
use lakecat_store::{MemoryCatalogStore, TableRecord};
use sail_catalog::provider::CatalogProvider;
use tokio::sync::Mutex;

#[derive(Debug, Default)]
struct RecordingSailEngine {
    last_scan: Mutex<Option<ScanPlanningRequest>>,
    last_fetch: Mutex<Option<FetchScanTasksRequest>>,
}

#[derive(Debug)]
struct RejectingGovernance;

#[async_trait::async_trait]
impl GovernanceEngine for RejectingGovernance {
    async fn authorize(
        &self,
        _request: AuthorizationRequest,
    ) -> LakeCatResult<AuthorizationReceipt> {
        Err(LakeCatError::Internal(
            "provider governance must not be consulted for a pre-authorized scan".to_string(),
        ))
    }
}

#[async_trait::async_trait]
impl SailCatalogEngine for RecordingSailEngine {
    async fn prepare_commit(
        &self,
        _request: CommitPreparationRequest,
    ) -> LakeCatResult<CommitPlan> {
        Err(LakeCatError::NotSupported(
            "recording provider test engine does not prepare commits".to_string(),
        ))
    }

    async fn plan_scan(&self, request: ScanPlanningRequest) -> LakeCatResult<ScanPlan> {
        *self.last_scan.lock().await = Some(request);
        Ok(ScanPlan {
            planned_by: "recording-provider-test".to_string(),
            snapshot_id: Some(42),
            scan_tasks: vec![json!({"plan-task": "recorded"})],
            residual_filter: None,
        })
    }

    async fn fetch_scan_tasks(
        &self,
        request: FetchScanTasksRequest,
    ) -> LakeCatResult<FetchScanTasksPlan> {
        *self.last_fetch.lock().await = Some(request);
        Ok(FetchScanTasksPlan {
            planned_by: "recording-provider-test".to_string(),
            plan_task: "recorded-plan-task".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![json!({"file-path": "file:///tmp/events/data.parquet"})],
            delete_files: Vec::new(),
            plan_tasks: Vec::new(),
            residual_filter: None,
        })
    }
}

#[tokio::test]
async fn provider_resolves_governed_tables_in_process() {
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
        DeferredSailCatalogEngine::new(),
        AllowAllGovernanceEngine::new(),
        Principal::anonymous(),
    );
    let namespace = SailNamespace::try_from(vec!["default"]).unwrap();
    provider
        .create_database(
            &namespace,
            CreateDatabaseOptions {
                comment: None,
                location: None,
                if_not_exists: true,
                properties: Vec::new(),
            },
        )
        .await
        .unwrap();
    let created = provider
        .create_table(
            &namespace,
            "events",
            CreateTableOptions {
                columns: vec![
                    sail_catalog::provider::CreateTableColumnOptions {
                        name: "event_id".to_string(),
                        data_type: DataType::Utf8,
                        nullable: false,
                        comment: Some("Event identifier".to_string()),
                        default: None,
                        generated_always_as: None,
                        identity: None,
                    },
                    sail_catalog::provider::CreateTableColumnOptions {
                        name: "payload".to_string(),
                        data_type: DataType::Struct(Fields::from(vec![
                            Field::new("region", DataType::Utf8, true),
                            Field::new(
                                "scores",
                                DataType::List(Arc::new(Field::new_list_field(
                                    DataType::Int32,
                                    false,
                                ))),
                                true,
                            ),
                        ])),
                        nullable: true,
                        comment: None,
                        default: None,
                        generated_always_as: None,
                        identity: None,
                    },
                    sail_catalog::provider::CreateTableColumnOptions {
                        name: "count".to_string(),
                        data_type: DataType::Int32,
                        nullable: true,
                        comment: None,
                        default: None,
                        generated_always_as: None,
                        identity: None,
                    },
                ],
                comment: None,
                constraints: vec![CatalogTableConstraint::PrimaryKey {
                    name: None,
                    columns: vec!["event_id".to_string()],
                }],
                location: Some("file:///tmp/events".to_string()),
                format: "iceberg".to_string(),
                partition_by: vec![CatalogPartitionField {
                    column: "event_id".to_string(),
                    transform: None,
                }],
                sort_by: vec![
                    CatalogTableSort {
                        column: "event_id".to_string(),
                        ascending: true,
                    },
                    CatalogTableSort {
                        column: "count".to_string(),
                        ascending: false,
                    },
                ],
                bucket_by: None,
                mode: sail_catalog::provider::CreateTableMode::Create,
                properties: Vec::new(),
                is_external: true,
                is_write_precondition: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(created.name, "events");
    let loaded = provider.get_table(&namespace, "events").await.unwrap();
    assert_eq!(loaded.name, "events");
    let TableKind::Table {
        columns,
        constraints,
        partition_by,
        sort_by,
        ..
    } = loaded.kind
    else {
        panic!("expected Sail table status")
    };
    assert_eq!(columns.len(), 3);
    assert_eq!(columns[0].name, "event_id");
    assert_eq!(columns[0].data_type, DataType::Utf8);
    assert!(!columns[0].nullable);
    assert!(columns[0].is_partition);
    assert_eq!(columns[0].comment.as_deref(), Some("Event identifier"));
    assert_eq!(columns[1].name, "payload");
    assert!(columns[1].nullable);
    assert!(!columns[1].is_partition);
    match &columns[1].data_type {
        DataType::Struct(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name(), "region");
            assert_eq!(fields[0].data_type(), &DataType::Utf8);
            assert!(fields[0].is_nullable());
            assert_eq!(fields[1].name(), "scores");
            assert!(matches!(fields[1].data_type(), DataType::List(_)));
        }
        other => panic!("expected nested payload struct, got {other:?}"),
    }
    assert_eq!(columns[2].name, "count");
    assert_eq!(columns[2].data_type, DataType::Int32);
    assert!(columns[2].nullable);
    assert!(!columns[2].is_partition);
    assert_eq!(
        constraints,
        vec![CatalogTableConstraint::PrimaryKey {
            name: None,
            columns: vec!["event_id".to_string()],
        }]
    );
    assert_eq!(
        partition_by,
        vec![CatalogPartitionField {
            column: "event_id".to_string(),
            transform: None,
        }]
    );
    assert_eq!(
        sort_by,
        vec![
            CatalogTableSort {
                column: "event_id".to_string(),
                ascending: true,
            },
            CatalogTableSort {
                column: "count".to_string(),
                ascending: false,
            },
        ]
    );
    assert_eq!(provider.list_tables(&namespace).await.unwrap().len(), 1);
    provider
        .commit_table(
            &namespace,
            "events",
            CommitTableOptions {
                format: "iceberg".to_string(),
                requirements: Vec::new(),
                updates: vec![json!({"action": "metadata-only"})],
            },
        )
        .await
        .unwrap();
    let commits = provider
        .get_table_commits(
            &namespace,
            "events",
            GetTableCommitsOptions {
                format: "iceberg".to_string(),
                table_uri: "file:///tmp/events".to_string(),
                start_version: 1,
                end_version: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(commits.latest_table_version, 1);
    assert_eq!(commits.commits.len(), 1);
    assert_eq!(commits.commits[0].version, 1);
    assert_eq!(commits.commits[0].file_name, "lakecat-commit-1");
    let filtered = provider
        .get_table_commits(
            &namespace,
            "events",
            GetTableCommitsOptions {
                format: "iceberg".to_string(),
                table_uri: "file:///tmp/events".to_string(),
                start_version: 2,
                end_version: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(filtered.latest_table_version, 1);
    assert_eq!(filtered.commits, vec![]);
    provider
        .drop_table(
            &namespace,
            "events",
            DropTableOptions {
                if_exists: false,
                purge: false,
            },
        )
        .await
        .unwrap();
    assert!(provider.get_table(&namespace, "events").await.is_err());
}

#[tokio::test]
async fn provider_drops_durable_namespaces() {
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
        DeferredSailCatalogEngine::new(),
        AllowAllGovernanceEngine::new(),
        Principal::anonymous(),
    );
    let namespace = SailNamespace::try_from(vec!["default"]).unwrap();
    provider
        .create_database(
            &namespace,
            CreateDatabaseOptions {
                comment: None,
                location: None,
                if_not_exists: true,
                properties: Vec::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(provider.list_databases(None).await.unwrap().len(), 1);

    provider
        .drop_database(
            &namespace,
            DropDatabaseOptions {
                if_exists: false,
                cascade: false,
            },
        )
        .await
        .unwrap();
    assert!(provider.list_databases(None).await.unwrap().is_empty());
    provider
        .drop_database(
            &namespace,
            DropDatabaseOptions {
                if_exists: true,
                cascade: false,
            },
        )
        .await
        .unwrap();

    let cascade_error = provider
        .drop_database(
            &namespace,
            DropDatabaseOptions {
                if_exists: true,
                cascade: true,
            },
        )
        .await
        .expect_err("LakeCat provider should reject cascading namespace drops");
    assert!(matches!(cascade_error, CatalogError::NotSupported(_)));
}

#[tokio::test]
async fn provider_manages_durable_views_with_typed_columns() {
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
        DeferredSailCatalogEngine::new(),
        AllowAllGovernanceEngine::new(),
        Principal::anonymous(),
    );
    let namespace = SailNamespace::try_from(vec!["default"]).unwrap();
    provider
        .create_database(
            &namespace,
            CreateDatabaseOptions {
                comment: None,
                location: None,
                if_not_exists: true,
                properties: Vec::new(),
            },
        )
        .await
        .unwrap();

    let created = provider
        .create_view(
            &namespace,
            "active_customers",
            CreateViewOptions {
                columns: vec![
                    CreateViewColumnOptions {
                        name: "id".to_string(),
                        data_type: DataType::Int64,
                        nullable: false,
                        comment: Some("Customer identifier".to_string()),
                    },
                    CreateViewColumnOptions {
                        name: "email".to_string(),
                        data_type: DataType::Utf8,
                        nullable: true,
                        comment: None,
                    },
                ],
                definition: "select id, email from customers where active".to_string(),
                if_not_exists: false,
                replace: false,
                comment: Some("Active customer view".to_string()),
                properties: vec![("semantic-domain".to_string(), "customer".to_string())],
            },
        )
        .await
        .unwrap();
    let TableKind::View {
        definition,
        columns,
        comment,
        properties,
    } = created.kind
    else {
        panic!("expected Sail view status");
    };
    assert_eq!(
        definition,
        "select id, email from customers where active".to_string()
    );
    assert_eq!(comment.as_deref(), Some("Active customer view"));
    assert_eq!(columns.len(), 2);
    assert_eq!(columns[0].name, "id");
    assert_eq!(columns[0].data_type, DataType::Int64);
    assert!(!columns[0].nullable);
    assert_eq!(columns[0].comment.as_deref(), Some("Customer identifier"));
    assert_eq!(columns[1].name, "email");
    assert_eq!(columns[1].data_type, DataType::Utf8);
    assert!(columns[1].nullable);
    assert!(
        properties
            .iter()
            .any(|(key, value)| key == "semantic-domain" && value == "customer")
    );

    let existing = provider
        .create_view(
            &namespace,
            "active_customers",
            CreateViewOptions {
                columns: Vec::new(),
                definition: "select 1".to_string(),
                if_not_exists: true,
                replace: false,
                comment: None,
                properties: Vec::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(existing.name, "active_customers");
    assert_eq!(provider.list_views(&namespace).await.unwrap().len(), 1);

    let loaded = provider
        .get_view(&namespace, "active_customers")
        .await
        .unwrap();
    assert_eq!(loaded.name, "active_customers");
    assert!(matches!(loaded.kind, TableKind::View { .. }));

    provider
        .drop_view(
            &namespace,
            "active_customers",
            DropViewOptions { if_exists: false },
        )
        .await
        .unwrap();
    assert!(provider.list_views(&namespace).await.unwrap().is_empty());
    provider
        .drop_view(
            &namespace,
            "active_customers",
            DropViewOptions { if_exists: true },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn provider_scan_authorization_carries_policy_restriction() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table_name = TableName::new("events").unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "policy-provider-scan",
                warehouse.clone(),
                Some(namespace.clone()),
                Some(table_name.clone()),
                true,
                json!({
                    "uid": "policy:provider-scan",
                    "purpose": "provider-routing",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "equal",
                            "term": "region",
                            "value": "west"
                        }
                    }
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        warehouse,
        store,
        DeferredSailCatalogEngine::new(),
        AllowAllGovernanceEngine::new(),
        Principal::anonymous(),
    );
    let sail_namespace = SailNamespace::try_from(vec!["default"]).unwrap();

    let capability = provider
        .authorize_table_scan(&sail_namespace, "events")
        .await
        .unwrap();
    let restriction = capability.read_restriction().unwrap();

    assert_eq!(
        restriction.allowed_columns,
        Some(vec!["event_id".to_string()])
    );
    assert_eq!(restriction.purpose.as_deref(), Some("provider-routing"));
    assert_eq!(
        restriction.row_predicate,
        Some(json!({
            "type": "equal",
            "term": "region",
            "value": "west"
        }))
    );
    assert_eq!(
        capability.receipt().context["policy-bindings"][0]["policy-id"],
        json!("policy-provider-scan")
    );
    assert_eq!(
        capability.receipt().context["lakecat:sail-provider"],
        json!("lakecat")
    );
    assert!(
        capability.receipt().policy_hash.is_some(),
        "provider scan receipt should summarize enforced policy hashes"
    );
}

#[tokio::test]
async fn provider_scan_planning_applies_policy_restriction_before_sail() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table_name = TableName::new("events").unwrap();
    let ident = TableIdent::new(warehouse.clone(), namespace.clone(), table_name.clone());
    store
        .create_table(TableRecord::new(
            ident,
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            json!({
                "format-version": 3,
                "location": "file:///tmp/events",
                "current-schema-id": 1,
                "schemas": [{
                    "schema-id": 1,
                    "fields": [
                        {"id": 1, "name": "event_id", "type": "string", "required": true},
                        {"id": 2, "name": "payload", "type": "string", "required": false}
                    ]
                }],
                "default-spec-id": 0,
                "partition-specs": [{"spec-id": 0, "fields": []}],
                "current-snapshot-id": 42,
                "snapshots": [{
                    "snapshot-id": 42,
                    "sequence-number": 7,
                    "timestamp-ms": 1710000000000_i64,
                    "manifest-list": "file:///tmp/events/metadata/snap-42.avro",
                    "summary": {"operation": "append"},
                    "schema-id": 1
                }]
            }),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "policy-provider-plan",
                warehouse.clone(),
                Some(namespace),
                Some(table_name),
                true,
                json!({
                    "uid": "policy:provider-plan",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "equal",
                            "term": "event_id",
                            "value": "evt-1"
                        }
                    }
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let sail = Arc::new(RecordingSailEngine::default());
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        warehouse,
        store,
        sail.clone(),
        AllowAllGovernanceEngine::new(),
        Principal::anonymous(),
    );
    let sail_namespace = SailNamespace::try_from(vec!["default"]).unwrap();

    let plan = provider
        .plan_table_scan(
            &sail_namespace,
            "events",
            ProviderScanPlanningRequest {
                projection: vec!["event_id".to_string(), "payload".to_string()],
                filters: vec![json!({
                    "type": "not-null",
                    "term": "event_id"
                })],
                limit: Some(10),
                snapshot_id: Some(42),
                start_snapshot_id: None,
                end_snapshot_id: None,
            },
        )
        .await
        .unwrap();
    let recorded = sail.last_scan.lock().await.clone().unwrap();

    assert_eq!(plan.planned_by, "recording-provider-test");
    assert_eq!(recorded.projection, vec!["event_id".to_string()]);
    assert_eq!(
        recorded.filters,
        vec![
            json!({
                "type": "not-null",
                "term": "event_id"
            }),
            json!({
                "type": "equal",
                "term": "event_id",
                "value": "evt-1"
            }),
        ]
    );
    assert_eq!(
        recorded.metadata_location.as_deref(),
        Some("file:///tmp/events/metadata/00000.json")
    );
    assert_eq!(recorded.limit, Some(10));
    assert_eq!(recorded.snapshot_id, Some(42));
}

#[tokio::test]
async fn provider_fetch_scan_tasks_applies_policy_requirements_before_sail() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table_name = TableName::new("events").unwrap();
    let ident = TableIdent::new(warehouse.clone(), namespace.clone(), table_name.clone());
    store
        .create_table(TableRecord::new(
            ident,
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            json!({
                "format-version": 3,
                "location": "file:///tmp/events",
                "current-schema-id": 1,
                "schemas": [{
                    "schema-id": 1,
                    "fields": [
                        {"id": 1, "name": "event_id", "type": "string", "required": true},
                        {"id": 2, "name": "payload", "type": "string", "required": false}
                    ]
                }],
                "default-spec-id": 0,
                "partition-specs": [{"spec-id": 0, "fields": []}],
                "current-snapshot-id": 42,
                "snapshots": []
            }),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "policy-provider-fetch",
                warehouse.clone(),
                Some(namespace),
                Some(table_name),
                true,
                json!({
                    "uid": "policy:provider-fetch",
                    "lakecat:read-restriction": {
                        "allowed-columns": ["event_id"],
                        "row-predicate": {
                            "type": "equal",
                            "term": "event_id",
                            "value": "evt-1"
                        }
                    }
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let sail = Arc::new(RecordingSailEngine::default());
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        warehouse,
        store,
        sail.clone(),
        AllowAllGovernanceEngine::new(),
        Principal::anonymous(),
    );
    let sail_namespace = SailNamespace::try_from(vec!["default"]).unwrap();

    let fetched = provider
        .fetch_table_scan_tasks(
            &sail_namespace,
            "events",
            ProviderFetchScanTasksRequest {
                plan_task: "manifest-list-token".to_string(),
            },
        )
        .await
        .unwrap();
    let recorded = sail.last_fetch.lock().await.clone().unwrap();

    assert_eq!(fetched.planned_by, "recording-provider-test");
    assert_eq!(recorded.plan_task, "manifest-list-token");
    assert_eq!(recorded.required_projection, vec!["event_id".to_string()]);
    assert_eq!(
        recorded.required_filters,
        vec![json!({
            "type": "equal",
            "term": "event_id",
            "value": "evt-1"
        })]
    );
    assert_eq!(
        recorded.metadata_location.as_deref(),
        Some("file:///tmp/events/metadata/00000.json")
    );
}

#[tokio::test]
async fn provider_uses_pre_authorized_scan_capability_without_reauthorizing() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table_name = TableName::new("events").unwrap();
    let ident = TableIdent::new(warehouse.clone(), namespace, table_name);
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    let receipt = AllowAllGovernanceEngine::new()
        .authorize(AuthorizationRequest {
            principal: Principal::anonymous(),
            action: CatalogAction::TablePlanScan,
            table: Some(ident.clone()),
            context: json!({
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": {
                        "type": "equal",
                        "term": "region",
                        "value": "west"
                    }
                }
            }),
        })
        .await
        .unwrap();
    let capability = TableScanCapability::from_receipt(receipt, ident).unwrap();
    let sail = Arc::new(RecordingSailEngine::default());
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        warehouse,
        store,
        sail.clone(),
        Arc::new(RejectingGovernance),
        Principal::anonymous(),
    );

    provider
        .plan_authorized_table_scan(
            &capability,
            ProviderScanPlanningRequest {
                projection: vec!["event_id".to_string(), "payload".to_string()],
                ..Default::default()
            },
        )
        .await
        .expect("pre-authorized plan should not ask provider governance again");
    let planned = sail.last_scan.lock().await.clone().unwrap();
    assert_eq!(planned.projection, vec!["event_id".to_string()]);
    assert_eq!(
        planned.filters,
        vec![json!({
            "type": "equal",
            "term": "region",
            "value": "west"
        })]
    );

    provider
        .fetch_authorized_table_scan_tasks(
            &capability,
            ProviderFetchScanTasksRequest {
                plan_task: "manifest-list-token".to_string(),
            },
        )
        .await
        .expect("pre-authorized fetch should not ask provider governance again");
    let fetched = sail.last_fetch.lock().await.clone().unwrap();
    assert_eq!(fetched.required_projection, vec!["event_id".to_string()]);
    assert_eq!(
        fetched.required_filters,
        vec![json!({
            "type": "equal",
            "term": "region",
            "value": "west"
        })]
    );
}

#[tokio::test]
async fn unsorted_table_uses_sort_order_id_zero() {
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
        DeferredSailCatalogEngine::new(),
        AllowAllGovernanceEngine::new(),
        Principal::anonymous(),
    );
    let namespace = SailNamespace::try_from(vec!["default"]).unwrap();
    provider
        .create_database(
            &namespace,
            CreateDatabaseOptions {
                comment: None,
                location: None,
                if_not_exists: true,
                properties: Vec::new(),
            },
        )
        .await
        .unwrap();
    let created = provider
        .create_table(
            &namespace,
            "unsorted",
            CreateTableOptions {
                columns: vec![sail_catalog::provider::CreateTableColumnOptions {
                    name: "id".to_string(),
                    data_type: DataType::Int64,
                    nullable: false,
                    comment: None,
                    default: None,
                    generated_always_as: None,
                    identity: None,
                }],
                comment: None,
                constraints: Vec::new(),
                location: Some("file:///tmp/unsorted".to_string()),
                format: "iceberg".to_string(),
                partition_by: Vec::new(),
                sort_by: Vec::new(),
                bucket_by: None,
                mode: sail_catalog::provider::CreateTableMode::Create,
                properties: Vec::new(),
                is_external: true,
                is_write_precondition: false,
            },
        )
        .await
        .unwrap();
    let TableKind::Table { sort_by, .. } = created.kind else {
        panic!("expected table kind")
    };
    assert!(
        sort_by.is_empty(),
        "unsorted table should have no sort fields"
    );
    let loaded = provider.get_table(&namespace, "unsorted").await.unwrap();
    let TableKind::Table {
        sort_by: loaded_sort,
        ..
    } = loaded.kind
    else {
        panic!("expected table kind")
    };
    assert!(
        loaded_sort.is_empty(),
        "round-tripped unsorted table should have no sort fields"
    );
}

#[tokio::test]
async fn unique_constraints_are_rejected_instead_of_dropped() {
    let provider = LakeCatCatalogProvider::new(
        "lakecat",
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
        DeferredSailCatalogEngine::new(),
        AllowAllGovernanceEngine::new(),
        Principal::anonymous(),
    );
    let namespace = SailNamespace::try_from(vec!["default"]).unwrap();
    provider
        .create_database(
            &namespace,
            CreateDatabaseOptions {
                comment: None,
                location: None,
                if_not_exists: true,
                properties: Vec::new(),
            },
        )
        .await
        .unwrap();
    let error = provider
        .create_table(
            &namespace,
            "unique_events",
            CreateTableOptions {
                columns: vec![sail_catalog::provider::CreateTableColumnOptions {
                    name: "event_id".to_string(),
                    data_type: DataType::Utf8,
                    nullable: false,
                    comment: None,
                    default: None,
                    generated_always_as: None,
                    identity: None,
                }],
                comment: None,
                constraints: vec![CatalogTableConstraint::Unique {
                    name: Some("unique_event_id".to_string()),
                    columns: vec!["event_id".to_string()],
                }],
                location: Some("file:///tmp/unique-events".to_string()),
                format: "iceberg".to_string(),
                partition_by: Vec::new(),
                sort_by: Vec::new(),
                bucket_by: None,
                mode: sail_catalog::provider::CreateTableMode::Create,
                properties: Vec::new(),
                is_external: true,
                is_write_precondition: false,
            },
        )
        .await
        .expect_err("unique constraints should not be silently dropped");
    assert!(matches!(error, CatalogError::InvalidArgument(_)));
    assert!(
        provider
            .get_table(&namespace, "unique_events")
            .await
            .is_err()
    );
}

fn make_table_record(metadata: serde_json::Value) -> lakecat_store::TableRecord {
    let ident = lakecat_core::TableIdent::new(
        lakecat_core::WarehouseName::new("test").unwrap(),
        lakecat_core::Namespace::new(vec!["default".to_string()]).unwrap(),
        lakecat_core::TableName::new("t").unwrap(),
    );
    lakecat_store::TableRecord::new(
        ident,
        "file:///tmp/t".to_string(),
        None,
        metadata,
        Principal::anonymous(),
    )
}

#[test]
fn descending_long_form_parses_correctly() {
    let metadata = serde_json::json!({
        "schemas": [{"schema-id": 1, "fields": [
            {"id": 1, "name": "ts", "type": "long", "required": false},
        ]}],
        "current-schema-id": 1,
        "sort-orders": [{
            "order-id": 1,
            "fields": [
                {"source-id": 1, "transform": "identity", "direction": "DESCENDING", "null-order": "nulls-last"},
            ],
        }],
        "default-sort-order-id": 1,
    });
    let record = make_table_record(metadata);
    let sort_fields = table_sort_fields(&record);
    assert_eq!(sort_fields.len(), 1);
    assert_eq!(sort_fields[0].column, "ts");
    assert!(
        !sort_fields[0].ascending,
        "DESCENDING should map to ascending=false"
    );
}

#[test]
fn missing_direction_skips_sort_field() {
    let metadata = serde_json::json!({
        "schemas": [{"schema-id": 1, "fields": [
            {"id": 1, "name": "id", "type": "int", "required": false},
        ]}],
        "current-schema-id": 1,
        "sort-orders": [{
            "order-id": 1,
            "fields": [
                {"source-id": 1, "transform": "identity"},
            ],
        }],
        "default-sort-order-id": 1,
    });
    let record = make_table_record(metadata);
    let sort_fields = table_sort_fields(&record);
    assert!(
        sort_fields.is_empty(),
        "field with no direction should be skipped, not treated as ascending"
    );
}

#[test]
fn nested_iceberg_types_project_to_arrow_types() {
    let struct_type = iceberg_type_to_datafusion(&serde_json::json!({
        "type": "struct",
        "fields": [
            {"id": 1, "name": "region", "type": "string", "required": true},
            {"id": 2, "name": "scores", "type": {
                "type": "list",
                "element-id": 3,
                "element": "int",
                "element-required": false
            }, "required": false}
        ]
    }));
    let DataType::Struct(fields) = struct_type else {
        panic!("expected struct projection")
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name(), "region");
    assert!(!fields[0].is_nullable());
    assert_eq!(fields[1].name(), "scores");
    assert!(fields[1].is_nullable());
    assert!(matches!(fields[1].data_type(), DataType::List(_)));

    let map_type = iceberg_type_to_datafusion(&serde_json::json!({
        "type": "map",
        "key-id": 4,
        "key": "string",
        "value-id": 5,
        "value": "long",
        "value-required": false
    }));
    let DataType::Map(entry, false) = map_type else {
        panic!("expected map projection")
    };
    let DataType::Struct(entry_fields) = entry.data_type() else {
        panic!("expected map entry struct")
    };
    assert_eq!(entry_fields[0].name(), "key");
    assert_eq!(entry_fields[0].data_type(), &DataType::Utf8);
    assert!(!entry_fields[0].is_nullable());
    assert_eq!(entry_fields[1].name(), "value");
    assert_eq!(entry_fields[1].data_type(), &DataType::Int64);
    assert!(entry_fields[1].is_nullable());
}
