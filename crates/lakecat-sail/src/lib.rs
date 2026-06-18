use std::sync::Arc;

use async_trait::async_trait;
use lakecat_core::{LakeCatError, LakeCatResult, Principal, TableIdent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait SailCatalogEngine: Send + Sync + 'static {
    async fn prepare_commit(&self, request: CommitPreparationRequest) -> LakeCatResult<CommitPlan>;
    async fn plan_scan(&self, request: ScanPlanningRequest) -> LakeCatResult<ScanPlan>;
    async fn fetch_scan_tasks(
        &self,
        request: FetchScanTasksRequest,
    ) -> LakeCatResult<FetchScanTasksPlan>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitPreparationRequest {
    pub table: TableIdent,
    pub principal: Principal,
    pub current_metadata_location: Option<String>,
    pub new_metadata_location: Option<String>,
    pub current_metadata: Value,
    pub new_metadata: Option<Value>,
    pub requirements: Vec<Value>,
    pub updates: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitPlan {
    pub prepared_by: String,
    pub requirements: Vec<Value>,
    pub updates: Vec<Value>,
    pub new_metadata_location: Option<String>,
    pub new_metadata: Value,
    pub metadata_write_required: bool,
    pub metadata_patch: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanPlanningRequest {
    pub table: TableIdent,
    pub principal: Principal,
    pub metadata_location: Option<String>,
    pub table_metadata: Value,
    pub projection: Vec<String>,
    pub filters: Vec<Value>,
    pub limit: Option<u64>,
    pub snapshot_id: Option<i64>,
    pub start_snapshot_id: Option<i64>,
    pub end_snapshot_id: Option<i64>,
}

impl ScanPlanningRequest {
    pub fn is_incremental_scan(&self) -> bool {
        self.start_snapshot_id.is_some() || self.end_snapshot_id.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanPlan {
    pub planned_by: String,
    pub snapshot_id: Option<i64>,
    pub scan_tasks: Vec<Value>,
    pub residual_filter: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FetchScanTasksRequest {
    pub table: TableIdent,
    pub principal: Principal,
    pub metadata_location: Option<String>,
    pub table_metadata: Value,
    pub plan_task: String,
    #[serde(default)]
    pub required_projection: Vec<String>,
    #[serde(default)]
    pub required_filters: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FetchScanTasksPlan {
    pub planned_by: String,
    pub plan_task: String,
    pub snapshot_id: Option<i64>,
    pub file_scan_tasks: Vec<Value>,
    pub delete_files: Vec<Value>,
    pub plan_tasks: Vec<Value>,
    pub residual_filter: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SailMetadataSummary {
    pub format_version: i32,
    pub table_uuid: Option<String>,
    pub table_location: Option<String>,
    pub current_schema_id: Option<i32>,
    pub current_snapshot_id: Option<i64>,
    pub sequence_number: Option<i64>,
    pub last_assigned_field_id: Option<i32>,
    pub last_assigned_partition_id: Option<i32>,
    pub default_spec_id: Option<i32>,
    pub default_sort_order_id: Option<i64>,
    pub manifest_list: Option<String>,
    pub v4_extension_mode: bool,
    pub fields: Vec<SailFieldSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SailFieldSummary {
    pub id: i32,
    pub name: String,
    pub data_type: String,
    pub required: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SailScanTask {
    pub task_type: String,
    pub table: String,
    pub snapshot_id: i64,
    pub plan_task: String,
    pub metadata_location: Option<String>,
    pub manifest_list: Option<String>,
    pub manifest_path: Option<String>,
    pub content: Option<String>,
    pub sequence_number: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SailFilterSummary {
    pub expression_type: String,
    pub references: Vec<String>,
    pub filter: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IcebergFormatSupport {
    pub stable_versions: Vec<i32>,
    pub v4_ready: bool,
}

impl Default for IcebergFormatSupport {
    fn default() -> Self {
        Self {
            stable_versions: vec![1, 2, 3],
            v4_ready: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct DeferredSailCatalogEngine;

impl DeferredSailCatalogEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl SailCatalogEngine for DeferredSailCatalogEngine {
    async fn prepare_commit(&self, request: CommitPreparationRequest) -> LakeCatResult<CommitPlan> {
        let metadata_write_required =
            request.new_metadata_location.is_some() || request.new_metadata.is_some();
        let new_metadata_location = request
            .new_metadata_location
            .clone()
            .or_else(|| request.current_metadata_location.clone());
        let new_metadata = request.new_metadata.unwrap_or(request.current_metadata);
        Ok(CommitPlan {
            prepared_by: "lakecat-sail-deferred".to_string(),
            requirements: request.requirements,
            updates: request.updates,
            new_metadata_location,
            new_metadata,
            metadata_write_required,
            metadata_patch: serde_json::json!({
                "lakecat:sail-delegation": "deferred",
                "lakecat:sail-target": "sail-catalog + sail-iceberg",
                "lakecat:format-support": IcebergFormatSupport::default(),
            }),
        })
    }

    async fn plan_scan(&self, request: ScanPlanningRequest) -> LakeCatResult<ScanPlan> {
        if request.metadata_location.is_none() {
            return Err(LakeCatError::NotSupported(
                "Sail scan planning needs an Iceberg metadata location".to_string(),
            ));
        }
        validate_lakecat_metadata_format(&request.table_metadata)?;
        Ok(ScanPlan {
            planned_by: "lakecat-sail-deferred".to_string(),
            snapshot_id: request.snapshot_id.or(request.end_snapshot_id),
            scan_tasks: Vec::new(),
            residual_filter: None,
        })
    }

    async fn fetch_scan_tasks(
        &self,
        request: FetchScanTasksRequest,
    ) -> LakeCatResult<FetchScanTasksPlan> {
        Err(LakeCatError::NotSupported(format!(
            "Sail fetchScanTasks is not wired yet for {}",
            request.table.stable_id()
        )))
    }
}

pub fn validate_lakecat_metadata_format(metadata: &Value) -> LakeCatResult<IcebergFormatSupport> {
    let support = IcebergFormatSupport::default();
    let Some(version) = metadata.get("format-version").and_then(Value::as_i64) else {
        return Ok(support);
    };
    if support.stable_versions.contains(&(version as i32)) {
        return Ok(support);
    }
    if version == 4 && support.v4_ready {
        return Ok(support);
    }
    Err(LakeCatError::NotSupported(format!(
        "unsupported Iceberg table format version v{version}"
    )))
}

#[cfg(feature = "catalog-provider")]
pub mod catalog_provider {
    use std::sync::Arc;

    use arrow_schema::{DataType, Field, Fields, TimeUnit};
    use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
    use lakecat_security::{
        AuthorizationRequest, CatalogAction, GovernanceEngine, ReadRestriction,
        TableCommitCapability, TableCreateCapability, TableDropCapability, TableLoadCapability,
        TableScanCapability,
    };
    use lakecat_store::{CatalogStore, PolicyBinding, TableCommit, TableRecord};
    use sail_catalog::error::{CatalogError, CatalogObject, CatalogResult};
    use sail_catalog::provider::{
        AlterTableOptions, CatalogProvider, CommitTableOptions, CreateDatabaseOptions,
        CreateTableOptions, CreateViewOptions, DropDatabaseOptions, DropTableOptions,
        DropViewOptions, GetTableCommitsOptions, GetTableCommitsResponse,
        Namespace as SailNamespace, TableCommitInfo,
    };
    use sail_catalog_iceberg::{LoadTableResult, load_table_result_to_status};
    use sail_common_datafusion::catalog::{
        CatalogPartitionField, CatalogTableConstraint, CatalogTableSort, DatabaseStatus,
        PartitionTransform, TableColumnStatus, TableKind, TableStatus,
    };
    use serde_json::json;

    use crate::{CommitPreparationRequest, SailCatalogEngine, ScanPlan, ScanPlanningRequest};

    #[derive(Debug, Clone, Default)]
    pub struct ProviderScanPlanningRequest {
        pub projection: Vec<String>,
        pub filters: Vec<serde_json::Value>,
        pub limit: Option<u64>,
        pub snapshot_id: Option<i64>,
        pub start_snapshot_id: Option<i64>,
        pub end_snapshot_id: Option<i64>,
    }

    pub struct LakeCatCatalogProvider {
        name: String,
        warehouse: WarehouseName,
        store: Arc<dyn CatalogStore>,
        sail: Arc<dyn SailCatalogEngine>,
        governance: Arc<dyn GovernanceEngine>,
        principal: Principal,
    }

    impl LakeCatCatalogProvider {
        pub fn new(
            name: impl Into<String>,
            warehouse: WarehouseName,
            store: Arc<dyn CatalogStore>,
            sail: Arc<dyn SailCatalogEngine>,
            governance: Arc<dyn GovernanceEngine>,
            principal: Principal,
        ) -> Self {
            Self {
                name: name.into(),
                warehouse,
                store,
                sail,
                governance,
                principal,
            }
        }

        fn ident(&self, database: &SailNamespace, table: &str) -> CatalogResult<TableIdent> {
            Ok(TableIdent::new(
                self.warehouse.clone(),
                lakecat_namespace(database)?,
                TableName::new(table).map_err(catalog_error)?,
            ))
        }

        async fn authorize_table(
            &self,
            action: CatalogAction,
            table: TableIdent,
        ) -> CatalogResult<lakecat_security::AuthorizationReceipt> {
            self.authorize_table_with_context(
                action,
                table,
                json!({"lakecat:sail-provider": self.name}),
            )
            .await
        }

        async fn authorize_table_with_context(
            &self,
            action: CatalogAction,
            table: TableIdent,
            context: serde_json::Value,
        ) -> CatalogResult<lakecat_security::AuthorizationReceipt> {
            let receipt = self
                .governance
                .authorize(AuthorizationRequest {
                    principal: self.principal.clone(),
                    action,
                    table: Some(table),
                    context,
                })
                .await
                .map_err(catalog_error)?;
            if !receipt.allowed {
                return Err(CatalogError::Conflict(
                    "LakeCat governance denied Sail catalog operation".to_string(),
                ));
            }
            Ok(receipt)
        }

        pub async fn authorize_table_scan(
            &self,
            database: &SailNamespace,
            table: &str,
        ) -> CatalogResult<TableScanCapability> {
            let ident = self.ident(database, table)?;
            let policy_bindings = self
                .store
                .policy_bindings_for_table(&ident)
                .await
                .map_err(catalog_error)?;
            let read_restriction = if policy_bindings.is_empty() {
                None
            } else {
                Some(
                    ReadRestriction::from_odrl_policies(
                        policy_bindings.iter().map(|binding| &binding.odrl),
                    )
                    .map_err(catalog_error)?,
                )
            };
            let mut context = json!({
                "lakecat:sail-provider": self.name,
                "policy-bindings": policy_bindings
                    .iter()
                    .map(policy_binding_context)
                    .collect::<Vec<_>>(),
            });
            if let Some(restriction) = read_restriction {
                context["read-restriction"] =
                    serde_json::to_value(restriction).map_err(catalog_error)?;
            }
            let receipt = self
                .authorize_table_with_context(CatalogAction::TablePlanScan, ident.clone(), context)
                .await?;
            TableScanCapability::from_receipt(receipt, ident).map_err(catalog_error)
        }

        pub async fn plan_table_scan(
            &self,
            database: &SailNamespace,
            table: &str,
            request: ProviderScanPlanningRequest,
        ) -> CatalogResult<ScanPlan> {
            let capability = self.authorize_table_scan(database, table).await?;
            let restriction = capability.read_restriction().map_err(catalog_error)?;
            let projection = restriction
                .effective_projection(&request.projection)
                .map_err(catalog_error)?;
            let mut filters = request.filters;
            filters.extend(restriction.mandatory_filters());
            let record = self
                .store
                .load_table(capability.table())
                .await
                .map_err(catalog_error)?;
            self.sail
                .plan_scan(ScanPlanningRequest {
                    table: capability.table().clone(),
                    principal: capability.receipt().principal.clone(),
                    metadata_location: record.metadata_location,
                    table_metadata: record.metadata,
                    projection,
                    filters,
                    limit: request.limit,
                    snapshot_id: request.snapshot_id,
                    start_snapshot_id: request.start_snapshot_id,
                    end_snapshot_id: request.end_snapshot_id,
                })
                .await
                .map_err(catalog_error)
        }

        async fn authorize_catalog(
            &self,
            action: CatalogAction,
        ) -> CatalogResult<lakecat_security::AuthorizationReceipt> {
            let receipt = self
                .governance
                .authorize(AuthorizationRequest {
                    principal: self.principal.clone(),
                    action,
                    table: None,
                    context: json!({"lakecat:sail-provider": self.name}),
                })
                .await
                .map_err(catalog_error)?;
            if !receipt.allowed {
                return Err(CatalogError::Conflict(
                    "LakeCat governance denied Sail catalog operation".to_string(),
                ));
            }
            Ok(receipt)
        }
    }

    #[async_trait::async_trait]
    impl CatalogProvider for LakeCatCatalogProvider {
        fn get_name(&self) -> &str {
            &self.name
        }

        async fn create_database(
            &self,
            database: &SailNamespace,
            _options: CreateDatabaseOptions,
        ) -> CatalogResult<DatabaseStatus> {
            self.authorize_catalog(CatalogAction::NamespaceCreate)
                .await?;
            let namespace = lakecat_namespace(database)?;
            self.store
                .create_namespace(&self.warehouse, namespace.clone())
                .await
                .map_err(catalog_error)?;
            Ok(database_status(&self.name, &namespace))
        }

        async fn get_database(&self, database: &SailNamespace) -> CatalogResult<DatabaseStatus> {
            self.authorize_catalog(CatalogAction::NamespaceList).await?;
            let namespace = lakecat_namespace(database)?;
            let namespaces = self
                .store
                .list_namespaces(&self.warehouse)
                .await
                .map_err(catalog_error)?;
            if namespaces.iter().any(|existing| existing == &namespace) {
                Ok(database_status(&self.name, &namespace))
            } else {
                Err(CatalogError::NotFound(
                    CatalogObject::Database,
                    namespace.path(),
                ))
            }
        }

        async fn list_databases(
            &self,
            prefix: Option<&SailNamespace>,
        ) -> CatalogResult<Vec<DatabaseStatus>> {
            self.authorize_catalog(CatalogAction::NamespaceList).await?;
            let prefix = prefix.map(lakecat_namespace).transpose()?;
            Ok(self
                .store
                .list_namespaces(&self.warehouse)
                .await
                .map_err(catalog_error)?
                .into_iter()
                .filter(|namespace| {
                    prefix
                        .as_ref()
                        .is_none_or(|prefix| starts_with_namespace(namespace, prefix))
                })
                .map(|namespace| database_status(&self.name, &namespace))
                .collect())
        }

        async fn drop_database(
            &self,
            _database: &SailNamespace,
            _options: DropDatabaseOptions,
        ) -> CatalogResult<()> {
            Err(CatalogError::NotSupported(
                "LakeCat namespace drop is not implemented".to_string(),
            ))
        }

        async fn create_table(
            &self,
            database: &SailNamespace,
            table: &str,
            options: CreateTableOptions,
        ) -> CatalogResult<TableStatus> {
            let ident = self.ident(database, table)?;
            let receipt = self
                .authorize_table(CatalogAction::TableCreate, ident.clone())
                .await?;
            let capability = TableCreateCapability::from_receipt(receipt.clone(), ident.clone())
                .map_err(catalog_error)?;
            let location = options.location.clone().ok_or_else(|| {
                CatalogError::InvalidArgument(
                    "LakeCat Sail table creation requires a location".to_string(),
                )
            })?;
            validate_constraints_supported(&options.constraints)?;
            let (fields, column_ids, last_column_id) =
                iceberg_fields_from_columns(&options.columns);
            let identifier_field_ids = identifier_field_ids(&column_ids, &options.constraints);
            let metadata = json!({
                "format-version": 3,
                "last-column-id": last_column_id,
                "location": location,
                "current-schema-id": 1,
                "schemas": [{
                    "schema-id": 1,
                    "identifier-field-ids": identifier_field_ids,
                    "fields": fields,
                }],
                "default-spec-id": 0,
                "partition-specs": [{
                    "spec-id": 0,
                    "fields": partition_spec_fields(&column_ids, &options.partition_by),
                }],
                "default-sort-order-id": if options.sort_by.is_empty() { 0 } else { 1 },
                "sort-orders": if options.sort_by.is_empty() {
                    json!([{"order-id": 0, "fields": []}])
                } else {
                    json!([
                        {"order-id": 0, "fields": []},
                        {"order-id": 1, "fields": sort_order_fields(&column_ids, &options.sort_by)},
                    ])
                },
                "properties": options.properties,
                "lakecat:sail-provider": self.name,
            });
            let record = TableRecord::new(
                capability.table().clone(),
                location,
                None,
                metadata,
                receipt.principal,
            );
            let record = self
                .store
                .create_table(record)
                .await
                .map_err(catalog_error)?;
            Ok(table_status(&self.name, &record))
        }

        async fn get_table(
            &self,
            database: &SailNamespace,
            table: &str,
        ) -> CatalogResult<TableStatus> {
            let ident = self.ident(database, table)?;
            let receipt = self
                .authorize_table(CatalogAction::TableLoad, ident.clone())
                .await?;
            let capability =
                TableLoadCapability::from_receipt(receipt, ident.clone()).map_err(catalog_error)?;
            let record = self
                .store
                .load_table(capability.table())
                .await
                .map_err(catalog_error)?;
            Ok(table_status(&self.name, &record))
        }

        async fn list_tables(&self, database: &SailNamespace) -> CatalogResult<Vec<TableStatus>> {
            self.authorize_catalog(CatalogAction::NamespaceList).await?;
            let namespace = lakecat_namespace(database)?;
            Ok(self
                .store
                .list_tables(&self.warehouse)
                .await
                .map_err(catalog_error)?
                .into_iter()
                .filter(|record| record.ident.namespace == namespace)
                .map(|record| table_status(&self.name, &record))
                .collect())
        }

        async fn drop_table(
            &self,
            database: &SailNamespace,
            table: &str,
            _options: DropTableOptions,
        ) -> CatalogResult<()> {
            let ident = self.ident(database, table)?;
            let receipt = self
                .authorize_table(CatalogAction::TableDrop, ident.clone())
                .await?;
            let capability =
                TableDropCapability::from_receipt(receipt, ident.clone()).map_err(catalog_error)?;
            self.store
                .soft_delete_table(
                    capability.table(),
                    capability.receipt().principal.clone(),
                    Some(serde_json::to_value(capability.receipt()).map_err(catalog_error)?),
                )
                .await
                .map_err(catalog_error)?;
            Ok(())
        }

        async fn alter_table(
            &self,
            _database: &SailNamespace,
            _table: &str,
            _options: AlterTableOptions,
        ) -> CatalogResult<()> {
            Err(CatalogError::NotSupported(
                "LakeCat Sail table alter is not implemented".to_string(),
            ))
        }

        async fn commit_table(
            &self,
            database: &SailNamespace,
            table: &str,
            options: CommitTableOptions,
        ) -> CatalogResult<TableStatus> {
            if options.format != "iceberg" {
                return Err(CatalogError::NotSupported(format!(
                    "LakeCat Sail provider only commits Iceberg tables, got {}",
                    options.format
                )));
            }
            let ident = self.ident(database, table)?;
            let receipt = self
                .authorize_table(CatalogAction::TableCommit, ident.clone())
                .await?;
            let capability = TableCommitCapability::from_receipt(receipt, ident.clone())
                .map_err(catalog_error)?;
            let current = self
                .store
                .load_table(capability.table())
                .await
                .map_err(catalog_error)?;
            let plan = self
                .sail
                .prepare_commit(CommitPreparationRequest {
                    table: capability.table().clone(),
                    principal: capability.receipt().principal.clone(),
                    current_metadata_location: current.metadata_location.clone(),
                    new_metadata_location: current.metadata_location.clone(),
                    current_metadata: current.metadata.clone(),
                    new_metadata: None,
                    requirements: options.requirements,
                    updates: options.updates,
                })
                .await
                .map_err(catalog_error)?;
            let updated = self
                .store
                .commit_table(
                    capability.table(),
                    TableCommit {
                        requirements: plan.requirements,
                        updates: plan.updates,
                        expected_previous_metadata_location: current.metadata_location,
                        new_metadata_location: plan.new_metadata_location,
                        new_metadata: Some(plan.new_metadata),
                        idempotency_key: None,
                        idempotency_request_hash: None,
                        principal: capability.receipt().principal.clone(),
                        authorization_receipt: Some(
                            serde_json::to_value(capability.receipt()).map_err(catalog_error)?,
                        ),
                    },
                )
                .await
                .map_err(catalog_error)?;
            Ok(table_status(&self.name, &updated))
        }

        async fn get_table_commits(
            &self,
            database: &SailNamespace,
            table: &str,
            options: GetTableCommitsOptions,
        ) -> CatalogResult<GetTableCommitsResponse> {
            if options.format != "iceberg" {
                return Err(CatalogError::NotSupported(format!(
                    "LakeCat Sail provider only discovers Iceberg commits, got {}",
                    options.format
                )));
            }
            let start_version = u64::try_from(options.start_version).map_err(|_| {
                CatalogError::InvalidArgument(
                    "LakeCat Sail commit discovery requires a non-negative start version"
                        .to_string(),
                )
            })?;
            let end_version = options
                .end_version
                .map(|version| {
                    u64::try_from(version).map_err(|_| {
                        CatalogError::InvalidArgument(
                            "LakeCat Sail commit discovery requires a non-negative end version"
                                .to_string(),
                        )
                    })
                })
                .transpose()?;
            let ident = self.ident(database, table)?;
            let records = self
                .store
                .table_commit_records(&ident, 0, None)
                .await
                .map_err(catalog_error)?;
            let latest_table_version = records
                .iter()
                .map(|record| record.sequence_number)
                .max()
                .unwrap_or(0);
            let commits = records
                .into_iter()
                .filter(|record| record.sequence_number >= start_version)
                .filter(|record| end_version.is_none_or(|end| record.sequence_number <= end))
                .map(table_commit_info)
                .collect::<CatalogResult<Vec<_>>>()?;
            Ok(GetTableCommitsResponse {
                latest_table_version: checked_i64(latest_table_version, "latest table version")?,
                commits,
            })
        }

        async fn create_view(
            &self,
            _database: &SailNamespace,
            _view: &str,
            _options: CreateViewOptions,
        ) -> CatalogResult<TableStatus> {
            Err(CatalogError::NotSupported(
                "LakeCat Sail views are not implemented".to_string(),
            ))
        }

        async fn get_view(
            &self,
            _database: &SailNamespace,
            _view: &str,
        ) -> CatalogResult<TableStatus> {
            Err(CatalogError::NotSupported(
                "LakeCat Sail views are not implemented".to_string(),
            ))
        }

        async fn list_views(&self, _database: &SailNamespace) -> CatalogResult<Vec<TableStatus>> {
            Ok(Vec::new())
        }

        async fn drop_view(
            &self,
            _database: &SailNamespace,
            _view: &str,
            _options: DropViewOptions,
        ) -> CatalogResult<()> {
            Err(CatalogError::NotSupported(
                "LakeCat Sail views are not implemented".to_string(),
            ))
        }
    }

    fn lakecat_namespace(namespace: &SailNamespace) -> CatalogResult<Namespace> {
        let parts: Vec<String> = namespace.clone().into();
        Namespace::new(parts).map_err(catalog_error)
    }

    fn starts_with_namespace(namespace: &Namespace, prefix: &Namespace) -> bool {
        namespace.parts().starts_with(prefix.parts())
    }

    fn database_status(catalog: &str, namespace: &Namespace) -> DatabaseStatus {
        DatabaseStatus {
            catalog: catalog.to_string(),
            database: namespace.parts().to_vec(),
            comment: None,
            location: None,
            properties: Vec::new(),
        }
    }

    fn policy_binding_context(binding: &PolicyBinding) -> serde_json::Value {
        json!({
            "policy-id": binding.policy_id,
            "warehouse": binding.warehouse.as_str(),
            "namespace": binding
                .namespace
                .as_ref()
                .map(|namespace| namespace.parts().to_vec()),
            "table": binding
                .table
                .as_ref()
                .map(|table| table.as_str().to_string()),
            "enforced": binding.enforced,
            "odrl": binding.odrl,
        })
    }

    fn table_status(catalog: &str, record: &TableRecord) -> TableStatus {
        if let Some(mut status) = sail_table_status(catalog, record) {
            if let TableKind::Table {
                location,
                properties,
                ..
            } = &mut status.kind
            {
                if location.is_none() {
                    *location = Some(record.location.clone());
                }
                properties.push(("lakecat:table-id".to_string(), record.ident.stable_id()));
                properties.push(("lakecat:version".to_string(), record.version.to_string()));
            }
            return status;
        }

        TableStatus {
            catalog: Some(catalog.to_string()),
            database: record.ident.namespace.parts().to_vec(),
            name: record.ident.name.as_str().to_string(),
            kind: TableKind::Table {
                columns: table_columns(record),
                comment: None,
                constraints: table_constraints(record),
                location: Some(record.location.clone()),
                format: "iceberg".to_string(),
                partition_by: table_partition_fields(record),
                sort_by: table_sort_fields(record),
                bucket_by: None,
                properties: vec![
                    ("lakecat:table-id".to_string(), record.ident.stable_id()),
                    ("lakecat:version".to_string(), record.version.to_string()),
                ],
                is_external: true,
            },
        }
    }

    fn sail_table_status(catalog: &str, record: &TableRecord) -> Option<TableStatus> {
        let metadata = serde_json::from_value(record.metadata.clone()).ok()?;
        let database = SailNamespace::try_from(record.ident.namespace.parts().to_vec()).ok()?;
        load_table_result_to_status(
            catalog,
            record.ident.name.as_str(),
            &database,
            LoadTableResult {
                metadata_location: record.metadata_location.clone(),
                metadata: Box::new(metadata),
                config: None,
                storage_credentials: None,
            },
        )
        .ok()
    }

    fn table_columns(record: &TableRecord) -> Vec<TableColumnStatus> {
        let partition_columns = partition_source_column_names(&record.metadata);
        current_schema(&record.metadata)
            .and_then(|schema| schema.get("fields").and_then(serde_json::Value::as_array))
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|field| table_column_status(field, &partition_columns))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    fn current_schema(metadata: &serde_json::Value) -> Option<&serde_json::Value> {
        let schemas = metadata.get("schemas")?.as_array()?;
        let current_schema_id = metadata
            .get("current-schema-id")
            .and_then(serde_json::Value::as_i64);
        current_schema_id
            .and_then(|id| {
                schemas.iter().find(|schema| {
                    schema.get("schema-id").and_then(serde_json::Value::as_i64) == Some(id)
                })
            })
            .or_else(|| schemas.last())
    }

    fn table_column_status(
        field: &serde_json::Value,
        partition_columns: &[String],
    ) -> Option<TableColumnStatus> {
        let name = field.get("name")?.as_str()?.to_string();
        let is_partition = partition_columns.iter().any(|column| column == &name);
        Some(TableColumnStatus {
            name,
            data_type: iceberg_type_to_datafusion(field.get("type")?),
            nullable: !field
                .get("required")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            comment: field
                .get("doc")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            default: None,
            generated_always_as: None,
            identity: None,
            is_partition,
            is_bucket: false,
            is_cluster: false,
        })
    }

    fn table_partition_fields(record: &TableRecord) -> Vec<CatalogPartitionField> {
        let Some(spec) = current_partition_spec(&record.metadata) else {
            return Vec::new();
        };
        let Some(fields) = spec.get("fields").and_then(serde_json::Value::as_array) else {
            return Vec::new();
        };
        let id_to_name = schema_id_to_name(&record.metadata);
        fields
            .iter()
            .filter_map(|field| {
                let source_id = field
                    .get("source-id")
                    .and_then(serde_json::Value::as_i64)
                    .and_then(|id| i32::try_from(id).ok())?;
                let column = id_to_name.get(&source_id)?.clone();
                let transform = field
                    .get("transform")
                    .and_then(serde_json::Value::as_str)
                    .and_then(partition_transform_from_iceberg);
                Some(CatalogPartitionField { column, transform })
            })
            .collect()
    }

    fn partition_source_column_names(metadata: &serde_json::Value) -> Vec<String> {
        let id_to_name = schema_id_to_name(metadata);
        current_partition_spec(metadata)
            .and_then(|spec| spec.get("fields").and_then(serde_json::Value::as_array))
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|field| {
                        field
                            .get("source-id")
                            .and_then(serde_json::Value::as_i64)
                            .and_then(|id| i32::try_from(id).ok())
                            .and_then(|id| id_to_name.get(&id).cloned())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn table_sort_fields(record: &TableRecord) -> Vec<CatalogTableSort> {
        let Some(order) = current_sort_order(&record.metadata) else {
            return Vec::new();
        };
        let Some(fields) = order.get("fields").and_then(serde_json::Value::as_array) else {
            return Vec::new();
        };
        let id_to_name = schema_id_to_name(&record.metadata);
        fields
            .iter()
            .filter_map(|field| {
                let source_id = field
                    .get("source-id")
                    .and_then(serde_json::Value::as_i64)
                    .and_then(|id| i32::try_from(id).ok())?;
                let column = id_to_name.get(&source_id)?.clone();
                let direction = field.get("direction").and_then(serde_json::Value::as_str)?;
                let ascending = match direction.to_ascii_lowercase().as_str() {
                    "asc" | "ascending" => true,
                    "desc" | "descending" => false,
                    _ => return None,
                };
                Some(CatalogTableSort { column, ascending })
            })
            .collect()
    }

    fn table_constraints(record: &TableRecord) -> Vec<CatalogTableConstraint> {
        let Some(schema) = current_schema(&record.metadata) else {
            return Vec::new();
        };
        let Some(ids) = schema
            .get("identifier-field-ids")
            .and_then(serde_json::Value::as_array)
        else {
            return Vec::new();
        };
        if ids.is_empty() {
            return Vec::new();
        }
        let id_to_name = schema_id_to_name(&record.metadata);
        let columns = ids
            .iter()
            .filter_map(|id| {
                id.as_i64()
                    .and_then(|id| i32::try_from(id).ok())
                    .and_then(|id| id_to_name.get(&id).cloned())
            })
            .collect::<Vec<_>>();
        if columns.is_empty() {
            Vec::new()
        } else {
            vec![CatalogTableConstraint::PrimaryKey {
                name: None,
                columns,
            }]
        }
    }

    fn current_partition_spec(metadata: &serde_json::Value) -> Option<&serde_json::Value> {
        let specs = metadata.get("partition-specs")?.as_array()?;
        let default_spec_id = metadata
            .get("default-spec-id")
            .and_then(serde_json::Value::as_i64);
        default_spec_id
            .and_then(|id| {
                specs.iter().find(|spec| {
                    spec.get("spec-id").and_then(serde_json::Value::as_i64) == Some(id)
                })
            })
            .or_else(|| specs.last())
    }

    fn current_sort_order(metadata: &serde_json::Value) -> Option<&serde_json::Value> {
        let orders = metadata.get("sort-orders")?.as_array()?;
        let default_order_id = metadata
            .get("default-sort-order-id")
            .and_then(serde_json::Value::as_i64);
        default_order_id
            .and_then(|id| {
                orders.iter().find(|order| {
                    order.get("order-id").and_then(serde_json::Value::as_i64) == Some(id)
                })
            })
            .or_else(|| orders.last())
    }

    fn schema_id_to_name(metadata: &serde_json::Value) -> std::collections::BTreeMap<i32, String> {
        current_schema(metadata)
            .and_then(|schema| schema.get("fields").and_then(serde_json::Value::as_array))
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|field| {
                        Some((
                            i32::try_from(field.get("id")?.as_i64()?).ok()?,
                            field.get("name")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn iceberg_type_to_datafusion(value: &serde_json::Value) -> DataType {
        match value {
            serde_json::Value::String(kind) => primitive_iceberg_type(kind),
            serde_json::Value::Object(object) => {
                match object.get("type").and_then(serde_json::Value::as_str) {
                    Some("struct") => DataType::Struct(Fields::from(
                        object
                            .get("fields")
                            .and_then(serde_json::Value::as_array)
                            .map(|fields| {
                                fields
                                    .iter()
                                    .filter_map(iceberg_nested_field_to_datafusion)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default(),
                    )),
                    Some("list") => {
                        let element_type = object
                            .get("element")
                            .map(iceberg_type_to_datafusion)
                            .unwrap_or(DataType::Utf8);
                        let element_required = object
                            .get("element-required")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        DataType::List(Arc::new(Field::new_list_field(
                            element_type,
                            !element_required,
                        )))
                    }
                    Some("map") => {
                        let key_type = object
                            .get("key")
                            .map(iceberg_type_to_datafusion)
                            .unwrap_or(DataType::Utf8);
                        let value_type = object
                            .get("value")
                            .map(iceberg_type_to_datafusion)
                            .unwrap_or(DataType::Utf8);
                        let value_required = object
                            .get("value-required")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        DataType::Map(
                            Arc::new(Field::new(
                                "entries",
                                DataType::Struct(Fields::from(vec![
                                    Field::new("key", key_type, false),
                                    Field::new("value", value_type, !value_required),
                                ])),
                                false,
                            )),
                            false,
                        )
                    }
                    Some(kind) => primitive_iceberg_type(kind),
                    None => DataType::Utf8,
                }
            }
            _ => DataType::Utf8,
        }
    }

    fn iceberg_nested_field_to_datafusion(field: &serde_json::Value) -> Option<Field> {
        Some(Field::new(
            field.get("name")?.as_str()?,
            iceberg_type_to_datafusion(field.get("type")?),
            !field
                .get("required")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        ))
    }

    fn primitive_iceberg_type(kind: &str) -> DataType {
        match kind {
            "boolean" => DataType::Boolean,
            "int" => DataType::Int32,
            "long" => DataType::Int64,
            "float" => DataType::Float32,
            "double" => DataType::Float64,
            "date" => DataType::Date32,
            "time" => DataType::Time64(TimeUnit::Microsecond),
            "timestamp" | "timestamptz" => DataType::Timestamp(TimeUnit::Microsecond, None),
            "binary" | "fixed" => DataType::Binary,
            "string" | "uuid" => DataType::Utf8,
            _ if kind.starts_with("decimal") => DataType::Decimal128(38, 18),
            _ => DataType::Utf8,
        }
    }

    fn datafusion_type_to_iceberg(
        data_type: &DataType,
        next_field_id: &mut i32,
    ) -> serde_json::Value {
        match data_type {
            DataType::Boolean => json!("boolean"),
            DataType::Int8 | DataType::Int16 | DataType::Int32 => json!("int"),
            DataType::UInt8 | DataType::UInt16 | DataType::UInt32 => json!("int"),
            DataType::Int64 | DataType::UInt64 => json!("long"),
            DataType::Float16 | DataType::Float32 => json!("float"),
            DataType::Float64 => json!("double"),
            DataType::Date32 | DataType::Date64 => json!("date"),
            DataType::Time32(_) | DataType::Time64(_) => json!("time"),
            DataType::Timestamp(_, _) => json!("timestamp"),
            DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) => {
                json!("binary")
            }
            DataType::Decimal128(_, _) | DataType::Decimal256(_, _) => json!("decimal(38,18)"),
            DataType::Struct(fields) => json!({
                "type": "struct",
                "fields": fields
                    .iter()
                    .map(|field| iceberg_nested_field_from_arrow(field, next_field_id))
                    .collect::<Vec<_>>(),
            }),
            DataType::List(field) | DataType::LargeList(field) => {
                let element_id = allocate_field_id(next_field_id);
                json!({
                    "type": "list",
                    "element-id": element_id,
                    "element": datafusion_type_to_iceberg(field.data_type(), next_field_id),
                    "element-required": !field.is_nullable(),
                })
            }
            DataType::Map(entry, _) => {
                let (key_type, value_type, value_required) =
                    map_entry_types(entry).unwrap_or((DataType::Utf8, DataType::Utf8, false));
                let key_id = allocate_field_id(next_field_id);
                let value_id = allocate_field_id(next_field_id);
                json!({
                    "type": "map",
                    "key-id": key_id,
                    "key": datafusion_type_to_iceberg(&key_type, next_field_id),
                    "value-id": value_id,
                    "value": datafusion_type_to_iceberg(&value_type, next_field_id),
                    "value-required": value_required,
                })
            }
            _ => json!("string"),
        }
    }

    fn iceberg_field_from_column(
        column: &sail_catalog::provider::CreateTableColumnOptions,
        next_field_id: &mut i32,
    ) -> serde_json::Value {
        json!({
            "id": allocate_field_id(next_field_id),
            "name": column.name,
            "type": datafusion_type_to_iceberg(&column.data_type, next_field_id),
            "required": !column.nullable,
            "doc": column.comment,
        })
    }

    fn iceberg_nested_field_from_arrow(
        field: &Field,
        next_field_id: &mut i32,
    ) -> serde_json::Value {
        json!({
            "id": allocate_field_id(next_field_id),
            "name": field.name(),
            "type": datafusion_type_to_iceberg(field.data_type(), next_field_id),
            "required": !field.is_nullable(),
        })
    }

    fn allocate_field_id(next_field_id: &mut i32) -> i32 {
        let field_id = *next_field_id;
        *next_field_id = (*next_field_id).saturating_add(1);
        field_id
    }

    fn map_entry_types(entry: &Field) -> Option<(DataType, DataType, bool)> {
        let DataType::Struct(fields) = entry.data_type() else {
            return None;
        };
        let key = fields.iter().find(|field| field.name() == "key")?;
        let value = fields.iter().find(|field| field.name() == "value")?;
        Some((
            key.data_type().clone(),
            value.data_type().clone(),
            !value.is_nullable(),
        ))
    }

    fn partition_spec_fields(
        column_ids: &std::collections::BTreeMap<String, i32>,
        partition_by: &[CatalogPartitionField],
    ) -> Vec<serde_json::Value> {
        partition_by
            .iter()
            .enumerate()
            .filter_map(|(idx, field)| {
                let source_id = *column_ids.get(&field.column)?;
                let (transform, name) = partition_transform_to_iceberg(field);
                Some(json!({
                    "source-id": source_id,
                    "field-id": i32::try_from(idx + 1000).unwrap_or(i32::MAX),
                    "name": name,
                    "transform": transform,
                }))
            })
            .collect()
    }

    fn sort_order_fields(
        column_ids: &std::collections::BTreeMap<String, i32>,
        sort_by: &[CatalogTableSort],
    ) -> Vec<serde_json::Value> {
        sort_by
            .iter()
            .filter_map(|field| {
                let source_id = *column_ids.get(&field.column)?;
                Some(json!({
                    "source-id": source_id,
                    "transform": "identity",
                    "direction": if field.ascending { "asc" } else { "desc" },
                    "null-order": if field.ascending { "nulls-first" } else { "nulls-last" },
                }))
            })
            .collect()
    }

    fn identifier_field_ids(
        column_ids: &std::collections::BTreeMap<String, i32>,
        constraints: &[CatalogTableConstraint],
    ) -> Vec<i32> {
        constraints
            .iter()
            .filter_map(|constraint| match constraint {
                CatalogTableConstraint::PrimaryKey { columns, .. } => Some(columns),
                CatalogTableConstraint::Unique { .. } => None,
            })
            .flat_map(|columns| {
                columns
                    .iter()
                    .filter_map(|column| column_ids.get(column).copied())
            })
            .collect()
    }

    fn validate_constraints_supported(constraints: &[CatalogTableConstraint]) -> CatalogResult<()> {
        if constraints
            .iter()
            .any(|constraint| matches!(constraint, CatalogTableConstraint::Unique { .. }))
        {
            return Err(CatalogError::InvalidArgument(
                "LakeCat Iceberg table creation does not support UNIQUE constraints".to_string(),
            ));
        }
        Ok(())
    }

    fn iceberg_fields_from_columns(
        columns: &[sail_catalog::provider::CreateTableColumnOptions],
    ) -> (
        Vec<serde_json::Value>,
        std::collections::BTreeMap<String, i32>,
        i32,
    ) {
        let mut next_field_id = 1_i32;
        let fields = columns
            .iter()
            .map(|column| iceberg_field_from_column(column, &mut next_field_id))
            .collect::<Vec<_>>();
        let column_ids = fields
            .iter()
            .filter_map(|field| {
                Some((
                    field.get("name")?.as_str()?.to_string(),
                    i32::try_from(field.get("id")?.as_i64()?).ok()?,
                ))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        (fields, column_ids, next_field_id - 1)
    }

    fn partition_transform_to_iceberg(field: &CatalogPartitionField) -> (String, String) {
        match field.transform {
            None | Some(PartitionTransform::Identity) => {
                ("identity".to_string(), field.column.clone())
            }
            Some(PartitionTransform::Year) => {
                ("year".to_string(), format!("{}_year", field.column))
            }
            Some(PartitionTransform::Month) => {
                ("month".to_string(), format!("{}_month", field.column))
            }
            Some(PartitionTransform::Day) => ("day".to_string(), format!("{}_day", field.column)),
            Some(PartitionTransform::Hour) => {
                ("hour".to_string(), format!("{}_hour", field.column))
            }
            Some(PartitionTransform::Bucket(n)) => {
                (format!("bucket[{n}]"), format!("{}_bucket", field.column))
            }
            Some(PartitionTransform::Truncate(w)) => {
                (format!("truncate[{w}]"), format!("{}_trunc", field.column))
            }
        }
    }

    fn partition_transform_from_iceberg(transform: &str) -> Option<PartitionTransform> {
        match transform {
            "identity" => None,
            "year" => Some(PartitionTransform::Year),
            "month" => Some(PartitionTransform::Month),
            "day" => Some(PartitionTransform::Day),
            "hour" => Some(PartitionTransform::Hour),
            value if value.starts_with("bucket[") && value.ends_with(']') => value
                .trim_start_matches("bucket[")
                .trim_end_matches(']')
                .parse()
                .ok()
                .map(PartitionTransform::Bucket),
            value if value.starts_with("truncate[") && value.ends_with(']') => value
                .trim_start_matches("truncate[")
                .trim_end_matches(']')
                .parse()
                .ok()
                .map(PartitionTransform::Truncate),
            _ => None,
        }
    }

    fn table_commit_info(
        record: lakecat_store::TableCommitRecord,
    ) -> CatalogResult<TableCommitInfo> {
        let metadata_location = record
            .new_metadata_location
            .or(record.previous_metadata_location)
            .unwrap_or_else(|| format!("lakecat-commit-{}", record.sequence_number));
        Ok(TableCommitInfo {
            version: checked_i64(record.sequence_number, "commit sequence number")?,
            timestamp: record.committed_at.timestamp_millis(),
            file_name: metadata_file_name(&metadata_location),
            file_size: 0,
            file_modification_timestamp: record.committed_at.timestamp_millis(),
        })
    }

    fn metadata_file_name(location: &str) -> String {
        location
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(location)
            .to_string()
    }

    fn checked_i64(value: u64, name: &str) -> CatalogResult<i64> {
        i64::try_from(value)
            .map_err(|_| CatalogError::InvalidArgument(format!("{name} exceeds i64 range")))
    }

    fn catalog_error(error: impl std::fmt::Display) -> CatalogError {
        let message = error.to_string();
        if message.contains("not found") {
            CatalogError::NotFound(CatalogObject::Table, message)
        } else if message.contains("already exists") {
            CatalogError::AlreadyExists(CatalogObject::Table, message)
        } else if message.contains("conflict") {
            CatalogError::Conflict(message)
        } else if message.contains("not supported") {
            CatalogError::NotSupported(message)
        } else {
            CatalogError::External(message)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{
            CommitPlan, DeferredSailCatalogEngine, FetchScanTasksPlan, FetchScanTasksRequest,
            SailCatalogEngine,
        };
        use lakecat_core::{LakeCatError, LakeCatResult};
        use lakecat_security::AllowAllGovernanceEngine;
        use lakecat_store::{MemoryCatalogStore, TableRecord};
        use sail_catalog::provider::CatalogProvider;
        use tokio::sync::Mutex;

        #[derive(Debug, Default)]
        struct RecordingSailEngine {
            last_scan: Mutex<Option<ScanPlanningRequest>>,
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
                _request: FetchScanTasksRequest,
            ) -> LakeCatResult<FetchScanTasksPlan> {
                Err(LakeCatError::NotSupported(
                    "recording provider test engine does not fetch scan tasks".to_string(),
                ))
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
                        if_not_exists: false,
                        replace: false,
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
                        if_not_exists: false,
                        replace: false,
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
                        if_not_exists: false,
                        replace: false,
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
    }
}

#[cfg(feature = "sail-local")]
pub mod sail_integration {
    use std::sync::Arc;

    use async_trait::async_trait;
    use hmac::{Hmac, Mac};
    use lakecat_core::{LakeCatError, LakeCatResult, TableIdent};
    use object_store::local::LocalFileSystem;
    use sail_catalog_iceberg::{
        completed_planning_with_id_result_from_values, fetch_scan_tasks_result_from_values, models,
    };
    use sail_iceberg::io::{StoreContext, load_manifest, load_manifest_list};
    use sail_iceberg::spec::{
        DataContentType, DataFile as SailDataFile, DataFileFormat, Datum, DeleteFileIndex,
        DeleteFileRef, Literal, MAIN_BRANCH, ManifestContentType, ManifestStatus, PrimitiveLiteral,
        Snapshot, TableMetadata, TableRequirement,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use sha2::Sha256;
    use url::Url;

    use crate::{
        CommitPlan, CommitPreparationRequest, FetchScanTasksPlan, FetchScanTasksRequest,
        IcebergFormatSupport, SailCatalogEngine, SailFieldSummary, SailFilterSummary,
        SailMetadataSummary, SailScanTask, ScanPlan, ScanPlanningRequest,
        validate_lakecat_metadata_format,
    };

    #[derive(Debug, Default)]
    pub struct SailRestModelCatalogEngine;

    type HmacSha256 = Hmac<Sha256>;
    const PLAN_TASK_SIGNING_KEY_ENV: &str = "LAKECAT_PLAN_TASK_SIGNING_KEY";
    const DEFAULT_PLAN_TASK_SIGNING_KEY: &[u8] = b"lakecat-local-plan-task-signing-key-v1";

    impl SailRestModelCatalogEngine {
        pub fn new() -> Arc<Self> {
            Arc::new(Self)
        }
    }

    #[async_trait]
    impl SailCatalogEngine for SailRestModelCatalogEngine {
        async fn prepare_commit(
            &self,
            request: CommitPreparationRequest,
        ) -> LakeCatResult<CommitPlan> {
            let metadata_write_required =
                request.new_metadata_location.is_some() || request.new_metadata.is_some();
            let (metadata_summary, typed_metadata) =
                inspect_sail_table_metadata_with_typed(&request.current_metadata)?;
            let validated_requirements = match typed_metadata.as_ref() {
                Some(metadata) => {
                    validate_stable_commit_requirements(metadata, &request.requirements)?
                }
                None => validate_v4_extension_commit_requirements(
                    &metadata_summary,
                    &request.requirements,
                )?,
            };
            let sail_request = json!({
                "requirements": request.requirements,
                "updates": request.updates,
            });
            // Keep raw JSON updates for v4/extension work, but prove the envelope is
            // compatible with Sail's generated REST catalog model.
            let _: models::CommitTableRequest = serde_json::from_value(sail_request.clone())
                .map_err(|err| {
                    LakeCatError::InvalidArgument(format!(
                        "invalid Iceberg REST commit request for Sail: {err}"
                    ))
                })?;
            let requirements = sail_request["requirements"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let updates = sail_request["updates"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let new_metadata_location = request
                .new_metadata_location
                .clone()
                .or_else(|| request.current_metadata_location.clone());
            let new_metadata = request.new_metadata.unwrap_or(request.current_metadata);
            validate_lakecat_metadata_format(&new_metadata)?;
            Ok(CommitPlan {
                prepared_by: "sail-rest-models".to_string(),
                requirements,
                updates,
                new_metadata_location: new_metadata_location.clone(),
                new_metadata,
                metadata_write_required,
                metadata_patch: json!({
                    "lakecat:sail-delegation": "sail-catalog-iceberg-rest-models",
                    "lakecat:format-support": IcebergFormatSupport::default(),
                    "lakecat:sail-metadata": metadata_summary,
                    "lakecat:validated-requirements": validated_requirements,
                    "previous-metadata-location": request.current_metadata_location,
                    "new-metadata-location": new_metadata_location,
                }),
            })
        }

        async fn plan_scan(&self, request: ScanPlanningRequest) -> LakeCatResult<ScanPlan> {
            let (metadata_summary, typed_metadata) =
                inspect_sail_table_metadata_with_typed(&request.table_metadata)?;
            validate_projection(&metadata_summary, &request.projection)?;
            let validated_filters = validate_scan_filters(&metadata_summary, &request.filters)?;
            let combined_filter = combined_scan_filter(&request.filters);
            let metadata_location = request.metadata_location.clone().ok_or_else(|| {
                LakeCatError::NotSupported(
                    "Sail scan planning needs an Iceberg metadata location".to_string(),
                )
            })?;
            let scan_tasks = match typed_metadata.as_ref() {
                Some(metadata) => {
                    stable_manifest_plan_tasks(
                        metadata,
                        &metadata_summary,
                        &request,
                        Some(metadata_location.clone()),
                    )
                    .await?
                }
                None => {
                    if request.is_incremental_scan() {
                        return Err(LakeCatError::NotSupported(
                            "Iceberg v4 extension incremental scan planning needs typed Sail metadata"
                                .to_string(),
                        ));
                    }
                    v4_extension_manifest_plan_tasks(
                        &request,
                        &metadata_summary,
                        Some(metadata_location.clone()),
                    )?
                }
            };
            let planned_snapshot_id = scan_tasks
                .first()
                .and_then(|task| task.get("snapshot-id"))
                .and_then(Value::as_i64)
                .or(request.snapshot_id)
                .or(metadata_summary.current_snapshot_id);
            let plan_task_count = scan_tasks.len();
            let plan_task_tokens = plan_task_tokens_from_values(&scan_tasks)?;
            completed_planning_with_id_result_from_values(
                None,
                plan_task_tokens,
                Vec::new(),
                Vec::new(),
            )
            .map_err(iceberg_rest_model_error)?;
            let scan_request = json!({
                "snapshot-id": planned_snapshot_id,
                "select": (!request.projection.is_empty()).then_some(request.projection.clone()),
                "filter": combined_filter,
                "case-sensitive": true,
            });
            let sail_scan: models::PlanTableScanRequest = serde_json::from_value(scan_request)
                .map_err(|err| {
                    LakeCatError::InvalidArgument(format!(
                        "invalid Iceberg REST scan-planning request for Sail: {err}"
                    ))
                })?;
            Ok(ScanPlan {
                planned_by: "sail-rest-models".to_string(),
                snapshot_id: sail_scan.snapshot_id,
                scan_tasks,
                residual_filter: Some(json!({
                    "metadata-location": metadata_location,
                    "select": sail_scan.select,
                    "filters-accepted-by-sail": validated_filters,
                    "limit-deferred-to-sail": request.limit,
                    "scan-mode": if request.is_incremental_scan() { "incremental" } else { "point-in-time" },
                    "start-snapshot-id": request.start_snapshot_id,
                    "end-snapshot-id": request.end_snapshot_id,
                    "plan-task-count": plan_task_count,
                    "sail-metadata": metadata_summary,
                })),
            })
        }

        async fn fetch_scan_tasks(
            &self,
            request: FetchScanTasksRequest,
        ) -> LakeCatResult<FetchScanTasksPlan> {
            let (metadata_summary, typed_metadata) =
                inspect_sail_table_metadata_with_typed(&request.table_metadata)?;
            let decoded = decode_plan_task(&request.plan_task)?;
            validate_decoded_plan_task(
                &request,
                &metadata_summary,
                typed_metadata.as_ref(),
                &decoded,
            )
            .await?;
            let task = decoded.to_scan_task(
                request.table.stable_id(),
                request.metadata_location.clone(),
                metadata_summary.sequence_number,
            );
            let expanded = expand_fetch_plan_task_with_sail_io(
                &decoded,
                &metadata_summary,
                typed_metadata.as_ref(),
                &request,
            )
            .await?;
            let (file_scan_tasks, delete_files, plan_tasks) = match expanded {
                Some(expanded) => (
                    expanded.file_scan_tasks,
                    expanded.delete_files,
                    expanded.plan_tasks,
                ),
                None => (Vec::new(), Vec::new(), vec![scan_task_value(task)?]),
            };
            fetch_scan_tasks_result_from_values(
                plan_task_tokens_from_values(&plan_tasks)?,
                file_scan_tasks.clone(),
                delete_files.clone(),
            )
            .map_err(iceberg_rest_model_error)?;
            Ok(FetchScanTasksPlan {
                planned_by: "sail-rest-models".to_string(),
                plan_task: request.plan_task,
                snapshot_id: Some(decoded.snapshot_id),
                file_scan_tasks,
                delete_files,
                plan_tasks,
                residual_filter: Some(json!({
                    "lakecat:sail-delegation": "sail-iceberg-manifest-reader",
                    "lakecat:sail-target": match decoded.kind.as_str() {
                        "manifest-list" => "sail_iceberg::io::load_manifest_list",
                        "manifest" => "sail_iceberg::io::load_manifest",
                        _ => "sail_iceberg::io",
                    },
                    "metadata-location": request.metadata_location,
                    "plan-task": decoded.raw,
                    "task-kind": decoded.kind,
                    "manifest-path": decoded.path,
                    "projection": decoded.projection,
                    "filters": decoded.filters,
                    "sail-metadata": metadata_summary,
                })),
            })
        }
    }

    fn plan_task_tokens_from_values(tasks: &[Value]) -> LakeCatResult<Vec<String>> {
        tasks
            .iter()
            .map(|task| {
                task.get("plan-task")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .ok_or_else(|| {
                        LakeCatError::Internal(
                            "Sail-generated Iceberg plan task is missing plan-task".to_string(),
                        )
                    })
            })
            .collect()
    }

    fn iceberg_rest_model_error(error: impl std::fmt::Display) -> LakeCatError {
        LakeCatError::Internal(format!(
            "Sail generated an invalid Iceberg REST planning payload: {error}"
        ))
    }

    pub fn validate_sail_table_metadata(metadata: &Value) -> LakeCatResult<IcebergFormatSupport> {
        inspect_sail_table_metadata(metadata).map(|_| IcebergFormatSupport::default())
    }

    pub fn inspect_sail_table_metadata(metadata: &Value) -> LakeCatResult<SailMetadataSummary> {
        inspect_sail_table_metadata_with_typed(metadata).map(|(summary, _)| summary)
    }

    fn inspect_sail_table_metadata_with_typed(
        metadata: &Value,
    ) -> LakeCatResult<(SailMetadataSummary, Option<TableMetadata>)> {
        validate_lakecat_metadata_format(metadata)?;
        let format_version = metadata
            .get("format-version")
            .and_then(Value::as_i64)
            .unwrap_or(2) as i32;
        if format_version == 4 {
            return Ok((inspect_v4_extension_metadata(metadata), None));
        }
        let bytes = serde_json::to_vec(metadata).map_err(|err| {
            LakeCatError::InvalidArgument(format!("invalid Iceberg table metadata JSON: {err}"))
        })?;
        let sail_metadata = TableMetadata::from_json(&bytes).map_err(|err| {
            LakeCatError::InvalidArgument(format!(
                "invalid Sail Iceberg table metadata model: {err}"
            ))
        })?;
        let schema = sail_metadata.current_schema();
        let snapshot = sail_metadata.current_snapshot();
        let summary = SailMetadataSummary {
            format_version: sail_metadata.format_version as i32,
            table_uuid: sail_metadata.table_uuid.map(|uuid| uuid.to_string()),
            table_location: Some(sail_metadata.location.clone()),
            current_schema_id: schema.map(|schema| schema.schema_id()),
            current_snapshot_id: snapshot.map(|snapshot| snapshot.snapshot_id()),
            sequence_number: snapshot.map(|snapshot| snapshot.sequence_number()),
            last_assigned_field_id: Some(sail_metadata.last_column_id),
            last_assigned_partition_id: Some(sail_metadata.last_partition_id),
            default_spec_id: Some(sail_metadata.default_spec_id),
            default_sort_order_id: Some(
                sail_metadata
                    .default_sort_order_id
                    .map(i64::from)
                    .unwrap_or(0),
            ),
            manifest_list: snapshot
                .map(|snapshot| snapshot.manifest_list().to_string())
                .filter(|manifest_list| !manifest_list.is_empty()),
            v4_extension_mode: false,
            fields: schema
                .map(|schema| {
                    schema
                        .fields()
                        .iter()
                        .map(|field| SailFieldSummary {
                            id: field.id,
                            name: field.name.clone(),
                            data_type: field.field_type.to_string(),
                            required: field.required,
                            doc: field.doc.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        Ok((summary, Some(sail_metadata)))
    }

    fn inspect_v4_extension_metadata(metadata: &Value) -> SailMetadataSummary {
        SailMetadataSummary {
            format_version: 4,
            table_uuid: metadata
                .get("table-uuid")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            table_location: metadata
                .get("location")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            current_schema_id: metadata
                .get("current-schema-id")
                .and_then(Value::as_i64)
                .map(|id| id as i32),
            current_snapshot_id: metadata.get("current-snapshot-id").and_then(Value::as_i64),
            sequence_number: metadata.get("last-sequence-number").and_then(Value::as_i64),
            last_assigned_field_id: metadata
                .get("last-column-id")
                .and_then(Value::as_i64)
                .map(|id| id as i32),
            last_assigned_partition_id: metadata
                .get("last-partition-id")
                .and_then(Value::as_i64)
                .map(|id| id as i32),
            default_spec_id: metadata
                .get("default-spec-id")
                .and_then(Value::as_i64)
                .map(|id| id as i32),
            default_sort_order_id: metadata
                .get("default-sort-order-id")
                .and_then(Value::as_i64)
                .or(Some(0)),
            manifest_list: metadata
                .get("snapshots")
                .and_then(Value::as_array)
                .and_then(|snapshots| {
                    let current_snapshot_id =
                        metadata.get("current-snapshot-id").and_then(Value::as_i64);
                    snapshots.iter().find(|snapshot| {
                        snapshot.get("snapshot-id").and_then(Value::as_i64) == current_snapshot_id
                    })
                })
                .and_then(|snapshot| snapshot.get("manifest-list"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            v4_extension_mode: true,
            fields: iceberg_json_fields(metadata),
        }
    }

    fn validate_stable_commit_requirements(
        metadata: &TableMetadata,
        requirements: &[Value],
    ) -> LakeCatResult<usize> {
        let requirements = parse_table_requirements(requirements)?;
        for requirement in &requirements {
            validate_stable_requirement(metadata, requirement)?;
        }
        Ok(requirements.len())
    }

    fn parse_table_requirements(requirements: &[Value]) -> LakeCatResult<Vec<TableRequirement>> {
        serde_json::from_value(Value::Array(requirements.to_vec())).map_err(|err| {
            LakeCatError::InvalidArgument(format!(
                "invalid Iceberg REST table requirement for Sail: {err}"
            ))
        })
    }

    fn validate_stable_requirement(
        metadata: &TableMetadata,
        requirement: &TableRequirement,
    ) -> LakeCatResult<()> {
        match requirement {
            TableRequirement::NotExist => Err(LakeCatError::Conflict(
                "Iceberg table already exists but commit asserted non-existence".to_string(),
            )),
            TableRequirement::UuidMatch { uuid } => {
                if metadata.table_uuid.as_ref() == Some(uuid) {
                    Ok(())
                } else {
                    Err(LakeCatError::Conflict(format!(
                        "Iceberg commit failed: expected table UUID {uuid} but found {:?}",
                        metadata.table_uuid
                    )))
                }
            }
            TableRequirement::RefSnapshotIdMatch { r#ref, snapshot_id } => {
                let actual = if r#ref == MAIN_BRANCH {
                    metadata.current_snapshot_id
                } else {
                    metadata
                        .refs
                        .get(r#ref)
                        .map(|ref_entry| ref_entry.snapshot_id)
                };
                let actual = actual.filter(|snapshot_id| *snapshot_id >= 0);
                if actual == *snapshot_id {
                    Ok(())
                } else {
                    Err(LakeCatError::Conflict(format!(
                        "Iceberg commit failed: reference '{ref}' expected snapshot {snapshot_id:?} but found {actual:?}"
                    )))
                }
            }
            TableRequirement::LastAssignedFieldIdMatch {
                last_assigned_field_id,
            } => {
                if metadata.last_column_id == *last_assigned_field_id {
                    Ok(())
                } else {
                    Err(LakeCatError::Conflict(format!(
                        "Iceberg commit failed: expected last assigned field id {last_assigned_field_id} but found {}",
                        metadata.last_column_id
                    )))
                }
            }
            TableRequirement::CurrentSchemaIdMatch { current_schema_id } => {
                if metadata.current_schema_id == *current_schema_id {
                    Ok(())
                } else {
                    Err(LakeCatError::Conflict(format!(
                        "Iceberg commit failed: expected current schema id {current_schema_id} but found {}",
                        metadata.current_schema_id
                    )))
                }
            }
            TableRequirement::LastAssignedPartitionIdMatch {
                last_assigned_partition_id,
            } => {
                if metadata.last_partition_id == *last_assigned_partition_id {
                    Ok(())
                } else {
                    Err(LakeCatError::Conflict(format!(
                        "Iceberg commit failed: expected last assigned partition id {last_assigned_partition_id} but found {}",
                        metadata.last_partition_id
                    )))
                }
            }
            TableRequirement::DefaultSpecIdMatch { default_spec_id } => {
                if metadata.default_spec_id == *default_spec_id {
                    Ok(())
                } else {
                    Err(LakeCatError::Conflict(format!(
                        "Iceberg commit failed: expected default partition spec id {default_spec_id} but found {}",
                        metadata.default_spec_id
                    )))
                }
            }
            TableRequirement::DefaultSortOrderIdMatch {
                default_sort_order_id,
            } => {
                let actual = metadata.default_sort_order_id.map(i64::from).unwrap_or(0);
                if actual == *default_sort_order_id {
                    Ok(())
                } else {
                    Err(LakeCatError::Conflict(format!(
                        "Iceberg commit failed: expected default sort order id {default_sort_order_id} but found {actual}"
                    )))
                }
            }
        }
    }

    fn validate_v4_extension_commit_requirements(
        metadata_summary: &SailMetadataSummary,
        requirements: &[Value],
    ) -> LakeCatResult<usize> {
        let mut validated = 0;
        for requirement in requirements {
            let Some(requirement_type) = requirement.get("type").and_then(Value::as_str) else {
                return Err(LakeCatError::InvalidArgument(
                    "Iceberg table requirement is missing a type".to_string(),
                ));
            };
            match requirement_type {
                "assert-create" => {
                    return Err(LakeCatError::Conflict(
                        "Iceberg table already exists but commit asserted non-existence"
                            .to_string(),
                    ));
                }
                "assert-table-uuid" => {
                    validate_json_requirement(
                        "table UUID",
                        metadata_summary.table_uuid.as_deref(),
                        requirement.get("uuid").and_then(Value::as_str),
                    )?;
                    validated += 1;
                }
                "assert-current-schema-id" => {
                    validate_json_i64_requirement(
                        "current schema id",
                        metadata_summary.current_schema_id.map(i64::from),
                        requirement.get("current-schema-id").and_then(Value::as_i64),
                    )?;
                    validated += 1;
                }
                "assert-ref-snapshot-id" => {
                    let reference = requirement
                        .get("ref")
                        .and_then(Value::as_str)
                        .unwrap_or(MAIN_BRANCH);
                    if reference == MAIN_BRANCH {
                        validate_json_i64_requirement(
                            "main snapshot id",
                            metadata_summary.current_snapshot_id,
                            requirement.get("snapshot-id").and_then(Value::as_i64),
                        )?;
                    }
                    validated += 1;
                }
                "assert-last-assigned-field-id" => {
                    validate_json_i64_requirement(
                        "last assigned field id",
                        metadata_summary.last_assigned_field_id.map(i64::from),
                        requirement
                            .get("last-assigned-field-id")
                            .and_then(Value::as_i64),
                    )?;
                    validated += 1;
                }
                "assert-last-assigned-partition-id" => {
                    validate_json_i64_requirement(
                        "last assigned partition id",
                        metadata_summary.last_assigned_partition_id.map(i64::from),
                        requirement
                            .get("last-assigned-partition-id")
                            .and_then(Value::as_i64),
                    )?;
                    validated += 1;
                }
                "assert-default-spec-id" => {
                    validate_json_i64_requirement(
                        "default partition spec id",
                        metadata_summary.default_spec_id.map(i64::from),
                        requirement.get("default-spec-id").and_then(Value::as_i64),
                    )?;
                    validated += 1;
                }
                "assert-default-sort-order-id" => {
                    validate_json_i64_requirement(
                        "default sort order id",
                        metadata_summary.default_sort_order_id,
                        requirement
                            .get("default-sort-order-id")
                            .and_then(Value::as_i64),
                    )?;
                    validated += 1;
                }
                _ => {}
            }
        }
        Ok(validated)
    }

    fn validate_json_requirement(
        label: &str,
        actual: Option<&str>,
        expected: Option<&str>,
    ) -> LakeCatResult<()> {
        if actual == expected {
            Ok(())
        } else {
            Err(LakeCatError::Conflict(format!(
                "Iceberg commit failed: expected {label} {expected:?} but found {actual:?}"
            )))
        }
    }

    fn validate_json_i64_requirement(
        label: &str,
        actual: Option<i64>,
        expected: Option<i64>,
    ) -> LakeCatResult<()> {
        if actual == expected {
            Ok(())
        } else {
            Err(LakeCatError::Conflict(format!(
                "Iceberg commit failed: expected {label} {expected:?} but found {actual:?}"
            )))
        }
    }

    async fn stable_manifest_plan_tasks(
        metadata: &TableMetadata,
        metadata_summary: &SailMetadataSummary,
        request: &ScanPlanningRequest,
        metadata_location: Option<String>,
    ) -> LakeCatResult<Vec<Value>> {
        if request.is_incremental_scan() {
            return incremental_manifest_plan_tasks(
                metadata,
                metadata_summary,
                request,
                metadata_location,
            )
            .await;
        }
        let snapshot = selected_snapshot(metadata, request.snapshot_id)?;
        full_snapshot_manifest_plan_tasks(snapshot, request, metadata_location)
    }

    fn full_snapshot_manifest_plan_tasks(
        snapshot: &Snapshot,
        request: &ScanPlanningRequest,
        metadata_location: Option<String>,
    ) -> LakeCatResult<Vec<Value>> {
        let mut tasks = Vec::new();
        push_manifest_list_scan_task(&mut tasks, snapshot, request, metadata_location.clone())?;
        push_v1_manifest_scan_tasks(&mut tasks, snapshot, request, metadata_location)?;
        Ok(tasks)
    }

    async fn incremental_manifest_plan_tasks(
        metadata: &TableMetadata,
        metadata_summary: &SailMetadataSummary,
        request: &ScanPlanningRequest,
        metadata_location: Option<String>,
    ) -> LakeCatResult<Vec<Value>> {
        let snapshots = incremental_snapshot_chain(
            metadata,
            request.start_snapshot_id,
            request.end_snapshot_id,
        )?;
        let Some(store_ctx) = local_store_context(metadata_summary)? else {
            return Err(LakeCatError::NotSupported(
                "Iceberg incremental scan planning currently requires local file metadata so Sail can inspect manifest lists"
                    .to_string(),
            ));
        };
        let mut tasks = Vec::new();
        for snapshot in snapshots {
            if snapshot.summary().operation.as_str() != "append" {
                return Err(LakeCatError::NotSupported(format!(
                    "Iceberg incremental scan planning only supports append snapshots, but snapshot {} is {}",
                    snapshot.snapshot_id(),
                    snapshot.summary().operation.as_str()
                )));
            }
            if !snapshot.manifest_list().is_empty() {
                let manifest_list = load_manifest_list(&store_ctx, snapshot.manifest_list())
                    .await
                    .map_err(|err| {
                        LakeCatError::Internal(format!(
                            "failed to load Iceberg manifest list for incremental planning: {err}"
                        ))
                    })?;
                let has_added_manifests = manifest_list
                    .entries()
                    .iter()
                    .any(|manifest| manifest.added_snapshot_id == snapshot.snapshot_id());
                if has_added_manifests {
                    tasks.push(scan_task_value(SailScanTask {
                        task_type: "incremental-manifest-list".to_string(),
                        table: request.table.stable_id(),
                        snapshot_id: snapshot.snapshot_id(),
                        plan_task: opaque_plan_task_with_filters(
                            &request.table,
                            "incremental-manifest-list",
                            snapshot.snapshot_id(),
                            snapshot.manifest_list(),
                            &request.projection,
                            &request.filters,
                        )?,
                        metadata_location: metadata_location.clone(),
                        manifest_list: Some(snapshot.manifest_list().to_string()),
                        manifest_path: None,
                        content: None,
                        sequence_number: Some(snapshot.sequence_number()),
                    })?);
                }
            }
            push_v1_manifest_scan_tasks(&mut tasks, snapshot, request, metadata_location.clone())?;
        }
        Ok(tasks)
    }

    fn push_manifest_list_scan_task(
        tasks: &mut Vec<Value>,
        snapshot: &Snapshot,
        request: &ScanPlanningRequest,
        metadata_location: Option<String>,
    ) -> LakeCatResult<()> {
        if !snapshot.manifest_list().is_empty() {
            tasks.push(scan_task_value(SailScanTask {
                task_type: "manifest-list".to_string(),
                table: request.table.stable_id(),
                snapshot_id: snapshot.snapshot_id(),
                plan_task: opaque_plan_task_with_filters(
                    &request.table,
                    "manifest-list",
                    snapshot.snapshot_id(),
                    snapshot.manifest_list(),
                    &request.projection,
                    &request.filters,
                )?,
                metadata_location,
                manifest_list: Some(snapshot.manifest_list().to_string()),
                manifest_path: None,
                content: None,
                sequence_number: Some(snapshot.sequence_number()),
            })?);
        }
        Ok(())
    }

    fn push_v1_manifest_scan_tasks(
        tasks: &mut Vec<Value>,
        snapshot: &Snapshot,
        request: &ScanPlanningRequest,
        metadata_location: Option<String>,
    ) -> LakeCatResult<()> {
        if let Some(manifests) = snapshot.manifests() {
            for manifest_path in manifests {
                tasks.push(scan_task_value(SailScanTask {
                    task_type: "manifest".to_string(),
                    table: request.table.stable_id(),
                    snapshot_id: snapshot.snapshot_id(),
                    plan_task: opaque_plan_task_with_filters(
                        &request.table,
                        "manifest",
                        snapshot.snapshot_id(),
                        manifest_path,
                        &request.projection,
                        &request.filters,
                    )?,
                    metadata_location: metadata_location.clone(),
                    manifest_list: None,
                    manifest_path: Some(manifest_path.clone()),
                    content: Some("data".to_string()),
                    sequence_number: Some(snapshot.sequence_number()),
                })?);
            }
        }
        Ok(())
    }

    fn selected_snapshot(
        metadata: &TableMetadata,
        requested_snapshot_id: Option<i64>,
    ) -> LakeCatResult<&Snapshot> {
        if let Some(snapshot_id) = requested_snapshot_id {
            metadata
                .snapshots
                .iter()
                .find(|snapshot| snapshot.snapshot_id() == snapshot_id)
                .ok_or_else(|| {
                    LakeCatError::InvalidArgument(format!(
                        "Iceberg snapshot {snapshot_id} not found"
                    ))
                })
        } else {
            metadata.current_snapshot().ok_or_else(|| {
                LakeCatError::InvalidArgument(
                    "Iceberg table metadata is missing a current snapshot".to_string(),
                )
            })
        }
    }

    fn incremental_snapshot_chain(
        metadata: &TableMetadata,
        start_snapshot_id: Option<i64>,
        end_snapshot_id: Option<i64>,
    ) -> LakeCatResult<Vec<&Snapshot>> {
        let start_snapshot_id = start_snapshot_id.ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "Iceberg incremental scan planning requires start-snapshot-id".to_string(),
            )
        })?;
        let end_snapshot_id = end_snapshot_id.ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "Iceberg incremental scan planning requires end-snapshot-id".to_string(),
            )
        })?;
        if start_snapshot_id == end_snapshot_id {
            return Ok(Vec::new());
        }

        let mut snapshots = Vec::new();
        let mut cursor = selected_snapshot(metadata, Some(end_snapshot_id))?;
        loop {
            if cursor.snapshot_id() == start_snapshot_id {
                snapshots.reverse();
                return Ok(snapshots);
            }
            snapshots.push(cursor);
            let Some(parent_snapshot_id) = cursor.parent_snapshot_id() else {
                return Err(LakeCatError::InvalidArgument(format!(
                    "Iceberg snapshot {start_snapshot_id} is not an ancestor of snapshot {end_snapshot_id}"
                )));
            };
            cursor = selected_snapshot(metadata, Some(parent_snapshot_id))?;
        }
    }

    fn v4_extension_manifest_plan_tasks(
        request: &ScanPlanningRequest,
        metadata_summary: &SailMetadataSummary,
        metadata_location: Option<String>,
    ) -> LakeCatResult<Vec<Value>> {
        let snapshot_id = request
            .snapshot_id
            .or(metadata_summary.current_snapshot_id)
            .ok_or_else(|| {
                LakeCatError::InvalidArgument(
                    "Iceberg v4 extension metadata is missing a current snapshot".to_string(),
                )
            })?;
        let mut tasks = Vec::new();
        if let Some(manifest_list) = &metadata_summary.manifest_list {
            tasks.push(scan_task_value(SailScanTask {
                task_type: "manifest-list".to_string(),
                table: request.table.stable_id(),
                snapshot_id,
                plan_task: opaque_plan_task_with_filters(
                    &request.table,
                    "manifest-list",
                    snapshot_id,
                    manifest_list,
                    &request.projection,
                    &request.filters,
                )?,
                metadata_location,
                manifest_list: Some(manifest_list.clone()),
                manifest_path: None,
                content: None,
                sequence_number: metadata_summary.sequence_number,
            })?);
        }
        Ok(tasks)
    }

    fn opaque_plan_task_with_filters(
        table: &TableIdent,
        kind: &str,
        snapshot_id: i64,
        path: &str,
        projection: &[String],
        filters: &[Value],
    ) -> LakeCatResult<String> {
        let payload = EncodedPlanTask {
            table: table.stable_id(),
            kind: kind.to_string(),
            snapshot_id,
            path: path.to_string(),
            projection: projection.to_vec(),
            filters: filters.to_vec(),
        };
        let bytes = serde_json::to_vec(&payload).map_err(|err| {
            LakeCatError::Internal(format!("failed to encode LakeCat/Sail plan task: {err}"))
        })?;
        let signature = sign_plan_task_payload(&bytes)?;
        Ok(format!(
            "lakecat:sail-json-hmac:{}:{}",
            signature,
            hex::encode(bytes)
        ))
    }

    fn scan_task_value(task: SailScanTask) -> LakeCatResult<Value> {
        serde_json::to_value(task).map_err(|err| {
            LakeCatError::Internal(format!("failed to encode Sail scan task: {err}"))
        })
    }

    #[derive(Debug, Default)]
    struct FetchExpansion {
        file_scan_tasks: Vec<Value>,
        delete_files: Vec<Value>,
        plan_tasks: Vec<Value>,
    }

    #[derive(Debug, Clone, Copy)]
    enum ManifestListExpansionMode {
        FullSnapshot,
        AddedBySnapshot,
    }

    impl ManifestListExpansionMode {
        fn includes(
            &self,
            decoded: &DecodedPlanTask,
            manifest: &sail_iceberg::spec::ManifestFile,
        ) -> bool {
            self.includes_snapshot(manifest, decoded.snapshot_id)
        }

        fn includes_snapshot(
            &self,
            manifest: &sail_iceberg::spec::ManifestFile,
            snapshot_id: i64,
        ) -> bool {
            match self {
                Self::FullSnapshot => true,
                Self::AddedBySnapshot => manifest.added_snapshot_id == snapshot_id,
            }
        }
    }

    async fn expand_fetch_plan_task_with_sail_io(
        decoded: &DecodedPlanTask,
        metadata_summary: &SailMetadataSummary,
        typed_metadata: Option<&TableMetadata>,
        request: &FetchScanTasksRequest,
    ) -> LakeCatResult<Option<FetchExpansion>> {
        if !local_file_url_exists(&decoded.path) {
            return Ok(None);
        }
        let Some(store_ctx) = local_store_context(metadata_summary)? else {
            return Ok(None);
        };
        match decoded.kind.as_str() {
            "manifest-list" | "incremental-manifest-list" => {
                let mode = match decoded.kind.as_str() {
                    "incremental-manifest-list" => ManifestListExpansionMode::AddedBySnapshot,
                    _ => ManifestListExpansionMode::FullSnapshot,
                };
                expand_manifest_list_task(&store_ctx, decoded, request, typed_metadata, mode)
                    .await
                    .map(Some)
            }
            "manifest" => expand_manifest_task(&store_ctx, decoded).await.map(Some),
            _ => Ok(None),
        }
    }

    fn local_store_context(
        metadata_summary: &SailMetadataSummary,
    ) -> LakeCatResult<Option<StoreContext>> {
        let Some(table_location) = metadata_summary.table_location.as_deref() else {
            return Ok(None);
        };
        let table_url = match Url::parse(table_location) {
            Ok(url) if url.scheme() == "file" => url,
            _ => return Ok(None),
        };
        StoreContext::new(Arc::new(LocalFileSystem::new()), &table_url)
            .map(Some)
            .map_err(|err| {
                LakeCatError::Internal(format!("failed to create Sail store context: {err}"))
            })
    }

    async fn expand_manifest_list_task(
        store_ctx: &StoreContext,
        decoded: &DecodedPlanTask,
        request: &FetchScanTasksRequest,
        typed_metadata: Option<&TableMetadata>,
        mode: ManifestListExpansionMode,
    ) -> LakeCatResult<FetchExpansion> {
        let manifest_list = load_manifest_list(store_ctx, &decoded.path)
            .await
            .map_err(|err| {
                LakeCatError::Internal(format!("failed to load Iceberg manifest list: {err}"))
            })?;
        let plan_tasks = manifest_list
            .entries()
            .iter()
            .filter(|manifest| mode.includes(decoded, manifest))
            .map(|manifest| {
                scan_task_value(SailScanTask {
                    task_type: "manifest".to_string(),
                    table: request.table.stable_id(),
                    snapshot_id: decoded.snapshot_id,
                    plan_task: opaque_plan_task_with_filters(
                        &request.table,
                        "manifest",
                        decoded.snapshot_id,
                        &manifest.manifest_path,
                        &decoded.projection,
                        &decoded.filters,
                    )?,
                    metadata_location: request.metadata_location.clone(),
                    manifest_list: Some(decoded.path.clone()),
                    manifest_path: Some(manifest.manifest_path.clone()),
                    content: Some(
                        match &manifest.content {
                            ManifestContentType::Data => "data",
                            ManifestContentType::Deletes => "deletes",
                        }
                        .to_string(),
                    ),
                    sequence_number: Some(manifest.sequence_number),
                })
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        let mut expansion = FetchExpansion {
            plan_tasks,
            ..FetchExpansion::default()
        };
        if let Some(metadata) = typed_metadata {
            expand_manifest_list_files_with_delete_refs(
                store_ctx,
                &manifest_list,
                metadata,
                &mut expansion,
                mode,
                decoded.snapshot_id,
                &decoded.filters,
            )
            .await?;
        }
        validate_fetch_tasks_shape(&expansion)?;
        Ok(expansion)
    }

    async fn expand_manifest_list_files_with_delete_refs(
        store_ctx: &StoreContext,
        manifest_list: &sail_iceberg::spec::ManifestList,
        metadata: &TableMetadata,
        expansion: &mut FetchExpansion,
        mode: ManifestListExpansionMode,
        snapshot_id: i64,
        filters: &[Value],
    ) -> LakeCatResult<()> {
        let mut data_files = Vec::new();
        let mut delete_index = DeleteFileIndex::new();
        for manifest_file in manifest_list.entries() {
            if !mode.includes_snapshot(manifest_file, snapshot_id) {
                continue;
            }
            let manifest = load_manifest(store_ctx, &manifest_file.manifest_path)
                .await
                .map_err(|err| {
                    LakeCatError::Internal(format!(
                        "failed to load Iceberg manifest from manifest list: {err}"
                    ))
                })?;
            let partition_spec_id = manifest_file.partition_spec_id;
            let parent_seq = manifest_file.sequence_number;
            let mut inherited_next_row_id = manifest_file.first_row_id;
            for entry in manifest.entries() {
                if !matches!(
                    entry.status,
                    ManifestStatus::Added | ManifestStatus::Existing
                ) {
                    continue;
                }
                let mut file = entry.data_file.clone();
                file.partition_spec_id = partition_spec_id;
                let sequence_number = entry.sequence_number.unwrap_or(parent_seq);
                match manifest_file.content {
                    ManifestContentType::Data => {
                        if file.first_row_id.is_none() {
                            file.first_row_id = inherited_next_row_id;
                        }
                        if let Some(next_row_id) = &mut inherited_next_row_id {
                            *next_row_id += checked_i64(file.record_count, "record count")?;
                        }
                        if file.content == DataContentType::Data {
                            data_files.push((file, sequence_number));
                        }
                    }
                    ManifestContentType::Deletes => {
                        let file_ref = DeleteFileRef {
                            data_file: file,
                            data_sequence_number: sequence_number,
                            partition_spec_id,
                            is_unpartitioned_spec: metadata
                                .partition_specs
                                .iter()
                                .find(|spec| spec.spec_id() == partition_spec_id)
                                .map(|spec| spec.is_unpartitioned())
                                .unwrap_or(false),
                        };
                        delete_index.insert(file_ref).map_err(|err| {
                            LakeCatError::NotSupported(format!(
                                "failed to index Iceberg delete file with Sail: {err}"
                            ))
                        })?;
                    }
                }
            }
        }

        data_files = data_files
            .into_iter()
            .filter_map(|(data_file, sequence_number)| {
                match data_file_may_match_filters(metadata, &data_file, filters) {
                    Ok(true) => Some(Ok((data_file, sequence_number))),
                    Ok(false) => None,
                    Err(err) => Some(Err(err)),
                }
            })
            .collect::<LakeCatResult<Vec<_>>>()?;

        let mut delete_key_to_index = std::collections::BTreeMap::new();
        for (data_file, sequence_number) in data_files {
            let matched = delete_index.for_data_file(&data_file, sequence_number);
            let mut delete_refs = Vec::new();
            for delete_file in matched.positional.iter().chain(matched.equality.iter()) {
                delete_refs.push(delete_file_reference_index(
                    delete_file,
                    &mut delete_key_to_index,
                    &mut expansion.delete_files,
                )?);
            }
            let mut scan_task = json!({
                "data-file": rest_data_file_value(&data_file)?,
            });
            if !delete_refs.is_empty() {
                scan_task["delete-file-references"] = json!(delete_refs);
            }
            expansion.file_scan_tasks.push(scan_task);
        }
        Ok(())
    }

    fn delete_file_reference_index(
        delete_file: &DeleteFileRef,
        delete_key_to_index: &mut std::collections::BTreeMap<String, i32>,
        delete_files: &mut Vec<Value>,
    ) -> LakeCatResult<i32> {
        let key = delete_file_key(delete_file);
        if let Some(index) = delete_key_to_index.get(&key) {
            return Ok(*index);
        }
        let value = rest_delete_file_value(&delete_file.data_file)?;
        validate_delete_file_shape(&value)?;
        let index = i32::try_from(delete_files.len()).map_err(|_| {
            LakeCatError::NotSupported(
                "Iceberg delete file reference index exceeds i32 range".to_string(),
            )
        })?;
        delete_files.push(value);
        delete_key_to_index.insert(key, index);
        Ok(index)
    }

    fn delete_file_key(delete_file: &DeleteFileRef) -> String {
        format!(
            "{}:{}:{}",
            delete_file.data_sequence_number,
            delete_file.partition_spec_id,
            delete_file.data_file.file_path
        )
    }

    fn data_file_may_match_filters(
        metadata: &TableMetadata,
        data_file: &SailDataFile,
        filters: &[Value],
    ) -> LakeCatResult<bool> {
        for filter in filters {
            if !data_file_may_match_filter(metadata, data_file, filter)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn data_file_may_match_filter(
        metadata: &TableMetadata,
        data_file: &SailDataFile,
        filter: &Value,
    ) -> LakeCatResult<bool> {
        let expression_type = filter_type(filter)?;
        match expression_type {
            "true" | "always-true" => Ok(true),
            "false" | "always-false" => Ok(false),
            "and" => Ok(data_file_may_match_filter(
                metadata,
                data_file,
                required_filter_child(filter, "left")?,
            )? && data_file_may_match_filter(
                metadata,
                data_file,
                required_filter_child(filter, "right")?,
            )?),
            "or" => Ok(data_file_may_match_filter(
                metadata,
                data_file,
                required_filter_child(filter, "left")?,
            )? || data_file_may_match_filter(
                metadata,
                data_file,
                required_filter_child(filter, "right")?,
            )?),
            // Negation over file-level bounds is easy to make unsound. Keep the file.
            "not" => Ok(true),
            "in" => {
                let expression =
                    validate_filter_model::<models::SetExpression>(filter, expression_type)?;
                let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                    return Ok(true);
                };
                for value in &expression.values {
                    if literal_predicate_may_match(metadata, data_file, field, "eq", value)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            "not-in" => {
                let expression =
                    validate_filter_model::<models::SetExpression>(filter, expression_type)?;
                let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                    return Ok(true);
                };
                if let Some(single_value) = exact_file_value(data_file, field.id) {
                    let mut excluded = false;
                    for value in &expression.values {
                        let literal = filter_literal_for_field(field, value)?;
                        if Some(&literal) == Some(single_value) {
                            excluded = true;
                            break;
                        }
                    }
                    Ok(!excluded)
                } else {
                    Ok(true)
                }
            }
            "lt" | "lt-eq" | "gt" | "gt-eq" | "eq" | "not-eq" | "starts-with"
            | "not-starts-with" => {
                let expression =
                    validate_filter_model::<models::LiteralExpression>(filter, expression_type)?;
                let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                    return Ok(true);
                };
                literal_predicate_may_match(
                    metadata,
                    data_file,
                    field,
                    expression_type,
                    &expression.value,
                )
            }
            "is-null" => {
                let expression =
                    validate_filter_model::<models::UnaryExpression>(filter, expression_type)?;
                let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                    return Ok(true);
                };
                Ok(data_file
                    .null_value_counts
                    .get(&field.id)
                    .copied()
                    .unwrap_or(0)
                    > 0)
            }
            "not-null" => {
                let expression =
                    validate_filter_model::<models::UnaryExpression>(filter, expression_type)?;
                let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                    return Ok(true);
                };
                let nulls = data_file
                    .null_value_counts
                    .get(&field.id)
                    .copied()
                    .unwrap_or(0);
                Ok(nulls < data_file.record_count)
            }
            "is-nan" | "not-nan" => Ok(true),
            _ => Ok(true),
        }
    }

    fn literal_predicate_may_match(
        _metadata: &TableMetadata,
        data_file: &SailDataFile,
        field: &sail_iceberg::spec::NestedFieldRef,
        op: &str,
        value: &Value,
    ) -> LakeCatResult<bool> {
        let literal = filter_literal_for_field(field, value)?;
        let lower = data_file
            .lower_bounds
            .get(&field.id)
            .map(|datum| &datum.literal);
        let upper = data_file
            .upper_bounds
            .get(&field.id)
            .map(|datum| &datum.literal);
        Ok(match op {
            "eq" => value_may_overlap_bounds(&literal, lower, upper),
            "not-eq" => exact_file_value(data_file, field.id) != Some(&literal),
            "lt" => lower.is_none_or(|lower| lower < &literal),
            "lt-eq" => lower.is_none_or(|lower| lower <= &literal),
            "gt" => upper.is_none_or(|upper| upper > &literal),
            "gt-eq" => upper.is_none_or(|upper| upper >= &literal),
            "starts-with" => string_prefix_may_overlap_bounds(&literal, lower, upper),
            "not-starts-with" => exact_file_value(data_file, field.id)
                .is_none_or(|value| !primitive_starts_with(value, &literal)),
            _ => true,
        })
    }

    fn value_may_overlap_bounds(
        value: &PrimitiveLiteral,
        lower: Option<&PrimitiveLiteral>,
        upper: Option<&PrimitiveLiteral>,
    ) -> bool {
        lower.is_none_or(|lower| lower <= value) && upper.is_none_or(|upper| value <= upper)
    }

    fn exact_file_value(data_file: &SailDataFile, field_id: i32) -> Option<&PrimitiveLiteral> {
        let lower = data_file
            .lower_bounds
            .get(&field_id)
            .map(|datum| &datum.literal)?;
        let upper = data_file
            .upper_bounds
            .get(&field_id)
            .map(|datum| &datum.literal)?;
        (lower == upper).then_some(lower)
    }

    fn string_prefix_may_overlap_bounds(
        prefix: &PrimitiveLiteral,
        lower: Option<&PrimitiveLiteral>,
        upper: Option<&PrimitiveLiteral>,
    ) -> bool {
        let PrimitiveLiteral::String(prefix) = prefix else {
            return true;
        };
        let lower = match lower {
            Some(PrimitiveLiteral::String(value)) => Some(value),
            Some(_) => return true,
            None => None,
        };
        let upper = match upper {
            Some(PrimitiveLiteral::String(value)) => Some(value),
            Some(_) => return true,
            None => None,
        };
        if upper.is_some_and(|upper| upper.as_str() < prefix.as_str()) {
            return false;
        }
        if let Some(next_prefix) = next_lexicographic_prefix(prefix) {
            if lower.is_some_and(|lower| lower.as_str() >= next_prefix.as_str()) {
                return false;
            }
        }
        true
    }

    fn primitive_starts_with(value: &PrimitiveLiteral, prefix: &PrimitiveLiteral) -> bool {
        match (value, prefix) {
            (PrimitiveLiteral::String(value), PrimitiveLiteral::String(prefix)) => {
                value.starts_with(prefix)
            }
            _ => false,
        }
    }

    fn next_lexicographic_prefix(prefix: &str) -> Option<String> {
        let mut bytes = prefix.as_bytes().to_vec();
        for index in (0..bytes.len()).rev() {
            if bytes[index] != u8::MAX {
                bytes[index] += 1;
                bytes.truncate(index + 1);
                return String::from_utf8(bytes).ok();
            }
        }
        None
    }

    fn filter_reference_field<'a>(
        metadata: &'a TableMetadata,
        term: &models::Term,
    ) -> Option<&'a sail_iceberg::spec::NestedFieldRef> {
        let reference = match term {
            models::Term::Reference(reference) => reference.as_str(),
            models::Term::TransformTerm(_) => return None,
        };
        metadata
            .current_schema()?
            .fields()
            .iter()
            .find(|field| field.name == reference)
    }

    fn filter_literal_for_field(
        field: &sail_iceberg::spec::NestedFieldRef,
        value: &Value,
    ) -> LakeCatResult<PrimitiveLiteral> {
        let literal = Literal::try_from_json(value.clone(), &field.field_type).map_err(|err| {
            LakeCatError::InvalidArgument(format!(
                "invalid Iceberg REST filter literal for column {}: {err}",
                field.name
            ))
        })?;
        match literal {
            Some(Literal::Primitive(literal)) => Ok(literal),
            Some(_) => Err(LakeCatError::NotSupported(format!(
                "Iceberg REST filter literal for column {} is not primitive",
                field.name
            ))),
            None => Err(LakeCatError::NotSupported(format!(
                "Iceberg REST null literal pruning for column {} is not supported",
                field.name
            ))),
        }
    }

    async fn expand_manifest_task(
        store_ctx: &StoreContext,
        decoded: &DecodedPlanTask,
    ) -> LakeCatResult<FetchExpansion> {
        let manifest = load_manifest(store_ctx, &decoded.path)
            .await
            .map_err(|err| {
                LakeCatError::Internal(format!("failed to load Iceberg manifest: {err}"))
            })?;
        let mut expansion = FetchExpansion::default();
        for entry in manifest.entries() {
            let file = &entry.data_file;
            match file.content {
                DataContentType::Data => {
                    expansion.file_scan_tasks.push(json!({
                        "data-file": rest_data_file_value(file)?,
                    }));
                }
                DataContentType::PositionDeletes | DataContentType::EqualityDeletes => {
                    expansion.delete_files.push(rest_delete_file_value(file)?);
                }
            }
        }
        validate_fetch_tasks_shape(&expansion)?;
        Ok(expansion)
    }

    fn validate_fetch_tasks_shape(expansion: &FetchExpansion) -> LakeCatResult<()> {
        for delete_file in &expansion.delete_files {
            validate_delete_file_shape(delete_file)?;
        }
        let rest_result = json!({
            "file-scan-tasks": (!expansion.file_scan_tasks.is_empty()).then_some(&expansion.file_scan_tasks),
            "plan-tasks": (!expansion.plan_tasks.is_empty()).then_some(Vec::<String>::new()),
        });
        let _: models::FetchScanTasksResult = serde_json::from_value(rest_result).map_err(|err| {
            LakeCatError::Internal(format!(
                "Sail manifest expansion produced an invalid Iceberg fetchScanTasks result: {err}"
            ))
        })?;
        Ok(())
    }

    fn validate_delete_file_shape(delete_file: &Value) -> LakeCatResult<()> {
        match delete_file.get("content").and_then(Value::as_str) {
            Some("position-deletes") => {
                let _: models::PositionDeleteFile =
                    serde_json::from_value(delete_file.clone()).map_err(|err| {
                        LakeCatError::Internal(format!(
                            "Sail manifest expansion produced an invalid Iceberg position delete file: {err}"
                        ))
                    })?;
                Ok(())
            }
            Some("equality-deletes") => {
                let _: models::EqualityDeleteFile =
                    serde_json::from_value(delete_file.clone()).map_err(|err| {
                        LakeCatError::Internal(format!(
                            "Sail manifest expansion produced an invalid Iceberg equality delete file: {err}"
                        ))
                    })?;
                Ok(())
            }
            Some(other) => Err(LakeCatError::Internal(format!(
                "invalid Iceberg delete file content: {other}"
            ))),
            None => Err(LakeCatError::Internal(
                "Iceberg delete file is missing content".to_string(),
            )),
        }
    }

    fn rest_data_file_value(file: &SailDataFile) -> LakeCatResult<Value> {
        let mut value = rest_base_file_value(file)?;
        value["content"] = json!("data");
        if let Some(first_row_id) = file.first_row_id {
            value["first-row-id"] = json!(first_row_id);
        }
        insert_count_map(&mut value, "column-sizes", &file.column_sizes)?;
        insert_count_map(&mut value, "value-counts", &file.value_counts)?;
        insert_count_map(&mut value, "null-value-counts", &file.null_value_counts)?;
        insert_count_map(&mut value, "nan-value-counts", &file.nan_value_counts)?;
        insert_value_map(&mut value, "lower-bounds", &file.lower_bounds)?;
        insert_value_map(&mut value, "upper-bounds", &file.upper_bounds)?;
        Ok(value)
    }

    fn rest_delete_file_value(file: &SailDataFile) -> LakeCatResult<Value> {
        match file.content {
            DataContentType::PositionDeletes => {
                let mut value = rest_base_file_value(file)?;
                value["content"] = json!("position-deletes");
                if let Some(content_offset) = file.content_offset {
                    value["content-offset"] = json!(content_offset);
                }
                if let Some(content_size) = file.content_size_in_bytes {
                    value["content-size-in-bytes"] = json!(content_size);
                }
                Ok(value)
            }
            DataContentType::EqualityDeletes => {
                let mut value = rest_base_file_value(file)?;
                value["content"] = json!("equality-deletes");
                if !file.equality_ids.is_empty() {
                    value["equality-ids"] = json!(file.equality_ids);
                }
                Ok(value)
            }
            DataContentType::Data => Err(LakeCatError::Internal(
                "data file cannot be encoded as an Iceberg delete file".to_string(),
            )),
        }
    }

    fn rest_base_file_value(file: &SailDataFile) -> LakeCatResult<Value> {
        let mut value = json!({
            "file-path": file.file_path,
            "file-format": rest_file_format(file.file_format),
            "spec-id": file.partition_spec_id,
            "partition": rest_partition_values(&file.partition)?,
            "file-size-in-bytes": checked_i64(file.file_size_in_bytes, "file size")?,
            "record-count": checked_i64(file.record_count, "record count")?,
        });
        if !file.split_offsets.is_empty() {
            value["split-offsets"] = json!(file.split_offsets);
        }
        if let Some(sort_order_id) = file.sort_order_id {
            value["sort-order-id"] = json!(sort_order_id);
        }
        Ok(value)
    }

    fn rest_file_format(format: DataFileFormat) -> &'static str {
        match format {
            DataFileFormat::Avro => "avro",
            DataFileFormat::Orc => "orc",
            DataFileFormat::Parquet => "parquet",
            DataFileFormat::Puffin => "puffin",
        }
    }

    fn rest_partition_values(
        partition: &[Option<sail_iceberg::spec::Literal>],
    ) -> LakeCatResult<Vec<Value>> {
        partition
            .iter()
            .map(|literal| {
                match literal {
                Some(literal) => rest_literal_value(literal),
                None => Err(LakeCatError::NotSupported(
                    "Iceberg REST partition null values are not yet supported by LakeCat conversion"
                        .to_string(),
                )),
            }
            })
            .collect()
    }

    fn rest_literal_value(literal: &sail_iceberg::spec::Literal) -> LakeCatResult<Value> {
        match literal {
            sail_iceberg::spec::Literal::Primitive(value) => rest_primitive_literal_value(value),
            sail_iceberg::spec::Literal::Struct(_)
            | sail_iceberg::spec::Literal::List(_)
            | sail_iceberg::spec::Literal::Map(_) => Err(LakeCatError::NotSupported(
                "nested Iceberg partition values are not yet supported by LakeCat conversion"
                    .to_string(),
            )),
        }
    }

    fn rest_primitive_literal_value(value: &PrimitiveLiteral) -> LakeCatResult<Value> {
        match value {
            PrimitiveLiteral::Boolean(value) => Ok(json!(value)),
            PrimitiveLiteral::Int(value) => Ok(json!(value)),
            PrimitiveLiteral::Long(value) => Ok(json!(value)),
            PrimitiveLiteral::Float(value) => Ok(json!(value.into_inner())),
            PrimitiveLiteral::Double(value) => Ok(json!(value.into_inner())),
            PrimitiveLiteral::Int128(value) => Ok(json!(value.to_string())),
            PrimitiveLiteral::String(value) => Ok(json!(value)),
            PrimitiveLiteral::UInt128(value) => Ok(json!(uuid::Uuid::from_u128(*value))),
            PrimitiveLiteral::Binary(value) => Ok(json!(hex::encode_upper(value))),
        }
    }

    fn insert_count_map(
        value: &mut Value,
        field: &str,
        map: &std::collections::HashMap<i32, u64>,
    ) -> LakeCatResult<()> {
        if map.is_empty() {
            return Ok(());
        }
        let mut entries = map.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(key, _)| **key);
        value[field] = json!({
            "keys": entries.iter().map(|(key, _)| **key).collect::<Vec<_>>(),
            "values": entries
                .iter()
                .map(|(_, count)| checked_i64(**count, field))
                .collect::<LakeCatResult<Vec<_>>>()?,
        });
        Ok(())
    }

    fn insert_value_map(
        value: &mut Value,
        field: &str,
        map: &std::collections::HashMap<i32, Datum>,
    ) -> LakeCatResult<()> {
        if map.is_empty() {
            return Ok(());
        }
        let mut entries = map.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(key, _)| **key);
        value[field] = json!({
            "keys": entries.iter().map(|(key, _)| **key).collect::<Vec<_>>(),
            "values": entries
                .iter()
                .map(|(_, datum)| rest_primitive_literal_value(&datum.literal))
                .collect::<LakeCatResult<Vec<_>>>()?,
        });
        Ok(())
    }

    fn checked_i64(value: u64, label: &str) -> LakeCatResult<i64> {
        i64::try_from(value).map_err(|_| {
            LakeCatError::NotSupported(format!(
                "Iceberg {label} value {value} exceeds the REST model i64 range"
            ))
        })
    }

    fn local_file_url_exists(raw: &str) -> bool {
        Url::parse(raw)
            .ok()
            .filter(|url| url.scheme() == "file")
            .and_then(|url| url.to_file_path().ok())
            .is_some_and(|path| path.exists())
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DecodedPlanTask {
        raw: String,
        table: Option<String>,
        kind: String,
        snapshot_id: i64,
        path: String,
        projection: Vec<String>,
        filters: Vec<Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    struct EncodedPlanTask {
        table: String,
        kind: String,
        snapshot_id: i64,
        path: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        projection: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        filters: Vec<Value>,
    }

    impl DecodedPlanTask {
        fn to_scan_task(
            &self,
            table: String,
            metadata_location: Option<String>,
            sequence_number: Option<i64>,
        ) -> SailScanTask {
            SailScanTask {
                task_type: self.kind.clone(),
                table,
                snapshot_id: self.snapshot_id,
                plan_task: self.raw.clone(),
                metadata_location,
                manifest_list: matches!(
                    self.kind.as_str(),
                    "manifest-list" | "incremental-manifest-list"
                )
                .then(|| self.path.clone()),
                manifest_path: (self.kind == "manifest").then(|| self.path.clone()),
                content: (self.kind == "manifest").then(|| "data".to_string()),
                sequence_number,
            }
        }
    }

    fn decode_plan_task(plan_task: &str) -> LakeCatResult<DecodedPlanTask> {
        if let Some(rest) = plan_task.strip_prefix("lakecat:sail-json-hmac:") {
            let mut parts = rest.splitn(2, ':');
            let Some(signature) = parts.next() else {
                return invalid_plan_task(plan_task);
            };
            let Some(encoded) = parts.next() else {
                return invalid_plan_task(plan_task);
            };
            let bytes = hex::decode(encoded).map_err(|_| {
                LakeCatError::InvalidArgument(format!(
                    "invalid LakeCat/Sail signed plan task: {plan_task}"
                ))
            })?;
            verify_plan_task_signature(&bytes, signature)?;
            return decode_structured_plan_task(plan_task, &bytes);
        }
        if let Some(encoded) = plan_task.strip_prefix("lakecat:sail-json:") {
            let bytes = hex::decode(encoded).map_err(|_| {
                LakeCatError::InvalidArgument(format!(
                    "invalid LakeCat/Sail structured plan task: {plan_task}"
                ))
            })?;
            return decode_structured_plan_task(plan_task, &bytes);
        }
        let mut parts = plan_task.splitn(5, ':');
        let Some(prefix) = parts.next() else {
            return invalid_plan_task(plan_task);
        };
        let Some(engine) = parts.next() else {
            return invalid_plan_task(plan_task);
        };
        let Some(kind) = parts.next() else {
            return invalid_plan_task(plan_task);
        };
        let Some(snapshot_id) = parts.next() else {
            return invalid_plan_task(plan_task);
        };
        let Some(path) = parts.next() else {
            return invalid_plan_task(plan_task);
        };
        if prefix != "lakecat" || engine != "sail" {
            return invalid_plan_task(plan_task);
        }
        if !matches!(
            kind,
            "manifest-list" | "incremental-manifest-list" | "manifest"
        ) {
            return invalid_plan_task(plan_task);
        }
        let snapshot_id = snapshot_id.parse::<i64>().map_err(|_| {
            LakeCatError::InvalidArgument(format!("invalid LakeCat/Sail plan task: {plan_task}"))
        })?;
        Ok(DecodedPlanTask {
            raw: plan_task.to_string(),
            table: None,
            kind: kind.to_string(),
            snapshot_id,
            path: path.to_string(),
            projection: Vec::new(),
            filters: Vec::new(),
        })
    }

    fn decode_structured_plan_task(
        plan_task: &str,
        bytes: &[u8],
    ) -> LakeCatResult<DecodedPlanTask> {
        let task: EncodedPlanTask = serde_json::from_slice(bytes).map_err(|err| {
            LakeCatError::InvalidArgument(format!(
                "invalid LakeCat/Sail structured plan task payload: {err}"
            ))
        })?;
        if !matches!(
            task.kind.as_str(),
            "manifest-list" | "incremental-manifest-list" | "manifest"
        ) {
            return invalid_plan_task(plan_task);
        }
        Ok(DecodedPlanTask {
            raw: plan_task.to_string(),
            table: Some(task.table),
            kind: task.kind,
            snapshot_id: task.snapshot_id,
            path: task.path,
            projection: task.projection,
            filters: task.filters,
        })
    }

    fn sign_plan_task_payload(bytes: &[u8]) -> LakeCatResult<String> {
        let mut mac = HmacSha256::new_from_slice(&plan_task_signing_key()).map_err(|err| {
            LakeCatError::Internal(format!("failed to initialize plan-task HMAC: {err}"))
        })?;
        mac.update(bytes);
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    fn verify_plan_task_signature(bytes: &[u8], signature: &str) -> LakeCatResult<()> {
        let signature = hex::decode(signature).map_err(|_| {
            LakeCatError::InvalidArgument("invalid LakeCat/Sail plan task signature".to_string())
        })?;
        let mut mac = HmacSha256::new_from_slice(&plan_task_signing_key()).map_err(|err| {
            LakeCatError::Internal(format!("failed to initialize plan-task HMAC: {err}"))
        })?;
        mac.update(bytes);
        mac.verify_slice(&signature).map_err(|_| {
            LakeCatError::InvalidArgument("invalid LakeCat/Sail plan task signature".to_string())
        })
    }

    fn plan_task_signing_key() -> Vec<u8> {
        std::env::var(PLAN_TASK_SIGNING_KEY_ENV)
            .map(|value| value.into_bytes())
            .unwrap_or_else(|_| DEFAULT_PLAN_TASK_SIGNING_KEY.to_vec())
    }

    fn invalid_plan_task<T>(plan_task: &str) -> LakeCatResult<T> {
        Err(LakeCatError::InvalidArgument(format!(
            "invalid LakeCat/Sail plan task: {plan_task}"
        )))
    }

    async fn validate_decoded_plan_task(
        request: &FetchScanTasksRequest,
        metadata_summary: &SailMetadataSummary,
        typed_metadata: Option<&TableMetadata>,
        decoded: &DecodedPlanTask,
    ) -> LakeCatResult<()> {
        if let Some(table) = &decoded.table
            && table != &request.table.stable_id()
        {
            return Err(LakeCatError::InvalidArgument(format!(
                "plan task table does not match requested table {}",
                request.table.stable_id()
            )));
        }
        validate_decoded_projection(request, decoded)?;
        validate_decoded_filters(request, decoded)?;
        if let Some(metadata) = typed_metadata {
            let snapshot = selected_snapshot(metadata, Some(decoded.snapshot_id))?;
            match decoded.kind.as_str() {
                "manifest-list" | "incremental-manifest-list" => {
                    if snapshot.manifest_list() == decoded.path {
                        Ok(())
                    } else {
                        Err(LakeCatError::InvalidArgument(format!(
                            "plan task manifest list does not match snapshot {}",
                            decoded.snapshot_id
                        )))
                    }
                }
                "manifest" => {
                    if snapshot
                        .manifests()
                        .map(|manifests| manifests.iter().any(|path| path == &decoded.path))
                        .unwrap_or(false)
                        || manifest_path_matches_local_manifest_list(
                            metadata_summary,
                            snapshot,
                            &decoded.path,
                        )
                        .await?
                    {
                        Ok(())
                    } else {
                        Err(LakeCatError::InvalidArgument(format!(
                            "plan task manifest does not match snapshot {}",
                            decoded.snapshot_id
                        )))
                    }
                }
                _ => invalid_plan_task(&decoded.raw),
            }
        } else {
            let snapshot_matches =
                request.snapshot_id_matches(decoded.snapshot_id, metadata_summary);
            let manifest_matches = metadata_summary.manifest_list.as_ref() == Some(&decoded.path);
            if snapshot_matches && decoded.kind == "manifest-list" && manifest_matches {
                Ok(())
            } else {
                Err(LakeCatError::InvalidArgument(format!(
                    "plan task does not match Iceberg v4 extension metadata: {}",
                    decoded.raw
                )))
            }
        }
    }

    fn validate_decoded_projection(
        request: &FetchScanTasksRequest,
        decoded: &DecodedPlanTask,
    ) -> LakeCatResult<()> {
        if request.required_projection.is_empty() {
            return Ok(());
        }
        if decoded.projection.is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "plan task omits the required governed projection".to_string(),
            ));
        }
        if decoded.projection.iter().all(|column| {
            request
                .required_projection
                .iter()
                .any(|required| required == column)
        }) {
            Ok(())
        } else {
            Err(LakeCatError::InvalidArgument(
                "plan task projection widens the governed read restriction".to_string(),
            ))
        }
    }

    fn validate_decoded_filters(
        request: &FetchScanTasksRequest,
        decoded: &DecodedPlanTask,
    ) -> LakeCatResult<()> {
        for required in &request.required_filters {
            if !decoded.filters.iter().any(|filter| filter == required) {
                return Err(LakeCatError::InvalidArgument(
                    "plan task omits a required governed filter".to_string(),
                ));
            }
        }
        Ok(())
    }

    async fn manifest_path_matches_local_manifest_list(
        metadata_summary: &SailMetadataSummary,
        snapshot: &Snapshot,
        manifest_path: &str,
    ) -> LakeCatResult<bool> {
        let manifest_list_path = snapshot.manifest_list();
        if manifest_list_path.is_empty() || !local_file_url_exists(manifest_list_path) {
            return Ok(false);
        }
        let Some(store_ctx) = local_store_context(metadata_summary)? else {
            return Ok(false);
        };
        let manifest_list = load_manifest_list(&store_ctx, manifest_list_path)
            .await
            .map_err(|err| {
                LakeCatError::Internal(format!(
                    "failed to validate manifest against Iceberg manifest list: {err}"
                ))
            })?;
        Ok(manifest_list
            .entries()
            .iter()
            .any(|manifest| manifest.manifest_path == manifest_path))
    }

    trait FetchScanTasksRequestExt {
        fn snapshot_id_matches(
            &self,
            snapshot_id: i64,
            metadata_summary: &SailMetadataSummary,
        ) -> bool;
    }

    impl FetchScanTasksRequestExt for FetchScanTasksRequest {
        fn snapshot_id_matches(
            &self,
            snapshot_id: i64,
            metadata_summary: &SailMetadataSummary,
        ) -> bool {
            metadata_summary.current_snapshot_id == Some(snapshot_id)
        }
    }

    fn validate_projection(
        metadata_summary: &SailMetadataSummary,
        projection: &[String],
    ) -> LakeCatResult<()> {
        if metadata_summary.v4_extension_mode || metadata_summary.fields.is_empty() {
            return Ok(());
        }
        let missing = projection
            .iter()
            .filter(|column| {
                !metadata_summary
                    .fields
                    .iter()
                    .any(|field| field.name == **column)
            })
            .collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(LakeCatError::InvalidArgument(format!(
                "unknown Iceberg projection column(s): {}",
                missing
                    .into_iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    }

    fn validate_scan_filters(
        metadata_summary: &SailMetadataSummary,
        filters: &[Value],
    ) -> LakeCatResult<Vec<SailFilterSummary>> {
        filters
            .iter()
            .map(|filter| validate_scan_filter(metadata_summary, filter))
            .collect()
    }

    fn validate_scan_filter(
        metadata_summary: &SailMetadataSummary,
        filter: &Value,
    ) -> LakeCatResult<SailFilterSummary> {
        let mut references = Vec::new();
        let expression_type =
            validate_filter_expression(metadata_summary, filter, &mut references)?;
        references.sort();
        references.dedup();
        Ok(SailFilterSummary {
            expression_type,
            references,
            filter: filter.clone(),
        })
    }

    fn validate_filter_expression(
        metadata_summary: &SailMetadataSummary,
        filter: &Value,
        references: &mut Vec<String>,
    ) -> LakeCatResult<String> {
        let expression_type = filter_type(filter)?;
        match expression_type {
            "true" | "always-true" => {
                validate_filter_model::<models::TrueExpression>(filter, expression_type)?;
            }
            "false" | "always-false" => {
                validate_filter_model::<models::FalseExpression>(filter, expression_type)?;
            }
            "and" | "or" => {
                validate_filter_model::<models::AndOrExpression>(filter, expression_type)?;
                validate_filter_expression(
                    metadata_summary,
                    required_filter_child(filter, "left")?,
                    references,
                )?;
                validate_filter_expression(
                    metadata_summary,
                    required_filter_child(filter, "right")?,
                    references,
                )?;
            }
            "not" => {
                validate_filter_model::<models::NotExpression>(filter, expression_type)?;
                validate_filter_expression(
                    metadata_summary,
                    required_filter_child(filter, "child")?,
                    references,
                )?;
            }
            "in" | "not-in" => {
                let expression =
                    validate_filter_model::<models::SetExpression>(filter, expression_type)?;
                validate_filter_term(metadata_summary, expression.term.as_ref(), references)?;
            }
            "lt" | "lt-eq" | "gt" | "gt-eq" | "eq" | "not-eq" | "starts-with"
            | "not-starts-with" => {
                let expression =
                    validate_filter_model::<models::LiteralExpression>(filter, expression_type)?;
                validate_filter_term(metadata_summary, expression.term.as_ref(), references)?;
            }
            "is-null" | "not-null" | "is-nan" | "not-nan" => {
                let expression =
                    validate_filter_model::<models::UnaryExpression>(filter, expression_type)?;
                validate_filter_term(metadata_summary, expression.term.as_ref(), references)?;
            }
            other => {
                return Err(LakeCatError::NotSupported(format!(
                    "unsupported Iceberg REST filter expression type: {other}"
                )));
            }
        }
        Ok(expression_type.to_string())
    }

    fn validate_filter_model<T>(filter: &Value, expression_type: &str) -> LakeCatResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(filter.clone()).map_err(|err| {
            LakeCatError::InvalidArgument(format!(
                "invalid Iceberg REST {expression_type} filter expression: {err}"
            ))
        })
    }

    fn filter_type(filter: &Value) -> LakeCatResult<&str> {
        filter.get("type").and_then(Value::as_str).ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "Iceberg REST filter expression is missing string field 'type'".to_string(),
            )
        })
    }

    fn required_filter_child<'a>(filter: &'a Value, field: &str) -> LakeCatResult<&'a Value> {
        filter.get(field).ok_or_else(|| {
            LakeCatError::InvalidArgument(format!(
                "Iceberg REST filter expression is missing field '{field}'"
            ))
        })
    }

    fn validate_filter_term(
        metadata_summary: &SailMetadataSummary,
        term: &models::Term,
        references: &mut Vec<String>,
    ) -> LakeCatResult<()> {
        match term {
            models::Term::Reference(reference) => {
                validate_filter_reference(metadata_summary, reference)?;
                references.push(reference.clone());
                Ok(())
            }
            models::Term::TransformTerm(transform) => {
                validate_filter_reference(metadata_summary, &transform.term)?;
                references.push(transform.term.clone());
                Ok(())
            }
        }
    }

    fn validate_filter_reference(
        metadata_summary: &SailMetadataSummary,
        reference: &str,
    ) -> LakeCatResult<()> {
        if metadata_summary.v4_extension_mode || metadata_summary.fields.is_empty() {
            return Ok(());
        }
        if metadata_summary
            .fields
            .iter()
            .any(|field| field.name == reference)
        {
            Ok(())
        } else {
            Err(LakeCatError::InvalidArgument(format!(
                "unknown Iceberg filter column: {reference}"
            )))
        }
    }

    fn combined_scan_filter(filters: &[Value]) -> Option<Value> {
        let mut filters = filters.iter().cloned();
        let first = filters.next()?;
        Some(filters.fold(first, |left, right| {
            json!({
                "type": "and",
                "left": left,
                "right": right,
            })
        }))
    }

    fn iceberg_json_fields(metadata: &Value) -> Vec<SailFieldSummary> {
        let current_schema_id = metadata.get("current-schema-id").and_then(Value::as_i64);
        metadata
            .get("schemas")
            .and_then(Value::as_array)
            .and_then(|schemas| {
                schemas.iter().find(|schema| {
                    schema.get("schema-id").and_then(Value::as_i64) == current_schema_id
                })
            })
            .or_else(|| metadata.get("schema"))
            .and_then(|schema| schema.get("fields"))
            .and_then(Value::as_array)
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|field| {
                        Some(SailFieldSummary {
                            id: field.get("id").and_then(Value::as_i64)? as i32,
                            name: field.get("name").and_then(Value::as_str)?.to_string(),
                            data_type: match field.get("type") {
                                Some(Value::String(value)) => value.clone(),
                                Some(value) => value.to_string(),
                                None => "unknown".to_string(),
                            },
                            required: field
                                .get("required")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            doc: field
                                .get("doc")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    mod tests {
        use std::collections::HashMap;
        use std::fs;
        use std::sync::Arc;
        use std::time::{SystemTime, UNIX_EPOCH};

        use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
        use sail_iceberg::spec::{
            DataContentType, DataFile, DataFileFormat, FormatVersion, ManifestContentType,
            ManifestFile, ManifestListWriter, ManifestMetadata, ManifestWriterBuilder,
            PrimitiveLiteral, PrimitiveType,
        };
        use url::Url;

        use super::*;

        #[tokio::test]
        async fn validates_scan_with_sail_rest_models() {
            let engine = SailRestModelCatalogEngine;
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let plan = engine
                .plan_scan(ScanPlanningRequest {
                    table,
                    principal: Principal::anonymous(),
                    metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                    table_metadata: sample_metadata(3),
                    projection: vec!["id".to_string()],
                    filters: Vec::new(),
                    limit: Some(10),
                    snapshot_id: Some(42),
                    start_snapshot_id: None,
                    end_snapshot_id: None,
                })
                .await
                .expect("scan request should validate");
            assert_eq!(plan.planned_by, "sail-rest-models");
            assert_eq!(plan.snapshot_id, Some(42));
            assert_eq!(plan.scan_tasks.len(), 1);
            assert_eq!(
                plan.scan_tasks[0].pointer("/task-type"),
                Some(&json!("manifest-list"))
            );
            assert_eq!(
                plan.scan_tasks[0].pointer("/manifest-list"),
                Some(&json!("file:///tmp/events/metadata/snap-42.avro"))
            );
            assert_eq!(
                plan.residual_filter
                    .unwrap()
                    .pointer("/sail-metadata/fields/0/name"),
                Some(&json!("id"))
            );
        }

        #[tokio::test]
        async fn rejects_unknown_format_versions_but_allows_v4_extension_mode() {
            assert!(validate_sail_table_metadata(&json!({"format-version": 4})).is_ok());
            assert!(validate_sail_table_metadata(&json!({"format-version": 9})).is_err());
        }

        #[tokio::test]
        async fn validates_projected_columns_against_sail_schema() {
            let engine = SailRestModelCatalogEngine;
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let err = engine
                .plan_scan(ScanPlanningRequest {
                    table,
                    principal: Principal::anonymous(),
                    metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                    table_metadata: sample_metadata(3),
                    projection: vec!["missing".to_string()],
                    filters: Vec::new(),
                    limit: None,
                    snapshot_id: None,
                    start_snapshot_id: None,
                    end_snapshot_id: None,
                })
                .await
                .expect_err("missing projected column should fail");
            assert!(err.to_string().contains("unknown Iceberg projection"));
        }

        #[tokio::test]
        async fn validates_scan_filters_against_sail_rest_models_and_schema() {
            let engine = SailRestModelCatalogEngine;
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let plan = engine
                .plan_scan(ScanPlanningRequest {
                    table: table.clone(),
                    principal: Principal::anonymous(),
                    metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                    table_metadata: sample_metadata(3),
                    projection: vec!["id".to_string()],
                    filters: vec![json!({
                        "type": "eq",
                        "term": "id",
                        "value": "evt-1"
                    })],
                    limit: None,
                    snapshot_id: Some(42),
                    start_snapshot_id: None,
                    end_snapshot_id: None,
                })
                .await
                .expect("filter should validate against Sail REST models");
            let residual = plan.residual_filter.unwrap();
            assert_eq!(
                residual.pointer("/filters-accepted-by-sail/0/expression-type"),
                Some(&json!("eq"))
            );
            assert_eq!(
                residual.pointer("/filters-accepted-by-sail/0/references/0"),
                Some(&json!("id"))
            );

            let err = engine
                .plan_scan(ScanPlanningRequest {
                    table,
                    principal: Principal::anonymous(),
                    metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                    table_metadata: sample_metadata(3),
                    projection: Vec::new(),
                    filters: vec![json!({
                        "type": "eq",
                        "term": "missing",
                        "value": "evt-1"
                    })],
                    limit: None,
                    snapshot_id: Some(42),
                    start_snapshot_id: None,
                    end_snapshot_id: None,
                })
                .await
                .expect_err("unknown filter column should fail");
            assert!(err.to_string().contains("unknown Iceberg filter column"));
        }

        #[tokio::test]
        async fn validates_commit_requirements_against_sail_metadata() {
            let engine = SailRestModelCatalogEngine;
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let plan = engine
                .prepare_commit(CommitPreparationRequest {
                    table,
                    principal: Principal::anonymous(),
                    current_metadata_location: Some(
                        "file:///tmp/events/metadata/00000.json".to_string(),
                    ),
                    new_metadata_location: None,
                    current_metadata: sample_metadata(3),
                    new_metadata: None,
                    requirements: vec![
                        json!({
                            "type": "assert-table-uuid",
                            "uuid": "11111111-1111-1111-1111-111111111111"
                        }),
                        json!({
                            "type": "assert-current-schema-id",
                            "current-schema-id": 1
                        }),
                        json!({
                            "type": "assert-ref-snapshot-id",
                            "ref": "main",
                            "snapshot-id": 42
                        }),
                    ],
                    updates: Vec::new(),
                })
                .await
                .expect("requirements should match current Sail metadata");
            assert_eq!(
                plan.metadata_patch["lakecat:validated-requirements"],
                json!(3)
            );
            assert_eq!(
                plan.new_metadata_location.as_deref(),
                Some("file:///tmp/events/metadata/00000.json")
            );
        }

        #[tokio::test]
        async fn commit_plan_accepts_lakecat_metadata_location_extension() {
            let engine = SailRestModelCatalogEngine;
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let plan = engine
                .prepare_commit(CommitPreparationRequest {
                    table,
                    principal: Principal::anonymous(),
                    current_metadata_location: Some(
                        "file:///tmp/events/metadata/00000.json".to_string(),
                    ),
                    new_metadata_location: Some(
                        "file:///tmp/events/metadata/00001.json".to_string(),
                    ),
                    current_metadata: sample_metadata(3),
                    new_metadata: Some(sample_metadata(3)),
                    requirements: Vec::new(),
                    updates: Vec::new(),
                })
                .await
                .expect("metadata location extension should plan");

            assert_eq!(
                plan.new_metadata_location.as_deref(),
                Some("file:///tmp/events/metadata/00001.json")
            );
            assert_eq!(
                plan.metadata_patch["new-metadata-location"],
                json!("file:///tmp/events/metadata/00001.json")
            );
            assert!(plan.metadata_write_required);
        }

        #[tokio::test]
        async fn rejects_stale_commit_requirements() {
            let engine = SailRestModelCatalogEngine;
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let err = engine
                .prepare_commit(CommitPreparationRequest {
                    table,
                    principal: Principal::anonymous(),
                    current_metadata_location: Some(
                        "file:///tmp/events/metadata/00000.json".to_string(),
                    ),
                    new_metadata_location: None,
                    current_metadata: sample_metadata(3),
                    new_metadata: None,
                    requirements: vec![json!({
                        "type": "assert-current-schema-id",
                        "current-schema-id": 9
                    })],
                    updates: Vec::new(),
                })
                .await
                .expect_err("stale schema requirement should fail");
            assert!(err.to_string().contains("expected current schema id 9"));
        }

        #[tokio::test]
        async fn expands_local_manifest_list_with_sail_io() {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("lakecat-manifest-list-{unique}"));
            let table_dir = root.join("table");
            let metadata_dir = table_dir.join("metadata");
            fs::create_dir_all(&metadata_dir).unwrap();

            let manifest_list_path = metadata_dir.join("snap-42.avro");
            let manifest_path = Url::from_file_path(metadata_dir.join("manifest-1.avro"))
                .unwrap()
                .to_string();
            let delete_manifest_path =
                Url::from_file_path(metadata_dir.join("delete-manifest-1.avro"))
                    .unwrap()
                    .to_string();
            let data_file_path = Url::from_file_path(table_dir.join("data").join("part-1.parquet"))
                .unwrap()
                .to_string();
            let delete_file_path =
                Url::from_file_path(table_dir.join("delete").join("pos-delete-1.parquet"))
                    .unwrap()
                    .to_string();
            let table_location = Url::from_directory_path(&table_dir).unwrap().to_string();
            let manifest_list = Url::from_file_path(&manifest_list_path)
                .unwrap()
                .to_string();
            let metadata_location = format!("{table_location}metadata/00000.json");
            let metadata =
                sample_metadata_with_locations(3, table_location.clone(), manifest_list.clone());
            let table_metadata =
                TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
            let manifest_metadata = ManifestMetadata::new(
                Arc::new(table_metadata.current_schema().unwrap().clone()),
                table_metadata.current_schema_id,
                table_metadata.default_partition_spec().unwrap().clone(),
                FormatVersion::V2,
                ManifestContentType::Data,
            );
            let data_file = DataFile {
                content: DataContentType::Data,
                file_path: data_file_path.clone(),
                file_format: DataFileFormat::Parquet,
                partition: Vec::new(),
                record_count: 3,
                file_size_in_bytes: 123,
                column_sizes: HashMap::new(),
                value_counts: HashMap::new(),
                null_value_counts: HashMap::new(),
                nan_value_counts: HashMap::new(),
                lower_bounds: HashMap::new(),
                upper_bounds: HashMap::new(),
                block_size_in_bytes: None,
                key_metadata: None,
                split_offsets: vec![4],
                equality_ids: Vec::new(),
                sort_order_id: None,
                first_row_id: Some(0),
                partition_spec_id: 0,
                referenced_data_file: None,
                content_offset: None,
                content_size_in_bytes: None,
            };
            let mut manifest_writer =
                ManifestWriterBuilder::new(Some(42), None, manifest_metadata).build();
            manifest_writer.add(data_file);
            let manifest_bytes = manifest_writer.to_avro_bytes_v2().unwrap();
            fs::write(
                Url::parse(&manifest_path).unwrap().to_file_path().unwrap(),
                manifest_bytes,
            )
            .unwrap();
            let delete_manifest_metadata = ManifestMetadata::new(
                Arc::new(table_metadata.current_schema().unwrap().clone()),
                table_metadata.current_schema_id,
                table_metadata.default_partition_spec().unwrap().clone(),
                FormatVersion::V2,
                ManifestContentType::Deletes,
            );
            let delete_file = DataFile {
                content: DataContentType::PositionDeletes,
                file_path: delete_file_path.clone(),
                file_format: DataFileFormat::Parquet,
                partition: Vec::new(),
                record_count: 1,
                file_size_in_bytes: 55,
                column_sizes: HashMap::new(),
                value_counts: HashMap::new(),
                null_value_counts: HashMap::new(),
                nan_value_counts: HashMap::new(),
                lower_bounds: HashMap::new(),
                upper_bounds: HashMap::new(),
                block_size_in_bytes: None,
                key_metadata: None,
                split_offsets: Vec::new(),
                equality_ids: Vec::new(),
                sort_order_id: None,
                first_row_id: None,
                partition_spec_id: 0,
                referenced_data_file: Some(data_file_path.clone()),
                content_offset: None,
                content_size_in_bytes: None,
            };
            let mut delete_manifest_writer =
                ManifestWriterBuilder::new(Some(42), None, delete_manifest_metadata).build();
            delete_manifest_writer.add(delete_file);
            let delete_manifest_bytes = delete_manifest_writer.to_avro_bytes_v2().unwrap();
            fs::write(
                Url::parse(&delete_manifest_path)
                    .unwrap()
                    .to_file_path()
                    .unwrap(),
                delete_manifest_bytes,
            )
            .unwrap();

            let manifest = ManifestFile::builder()
                .with_manifest_path(&manifest_path)
                .with_manifest_length(10)
                .with_partition_spec_id(0)
                .with_content(ManifestContentType::Data)
                .with_sequence_number(7)
                .with_min_sequence_number(7)
                .with_added_snapshot_id(42)
                .with_file_counts(1, 0, 0)
                .with_row_counts(3, 0, 0)
                .build()
                .unwrap();
            let delete_manifest = ManifestFile::builder()
                .with_manifest_path(&delete_manifest_path)
                .with_manifest_length(10)
                .with_partition_spec_id(0)
                .with_content(ManifestContentType::Deletes)
                .with_sequence_number(8)
                .with_min_sequence_number(8)
                .with_added_snapshot_id(42)
                .with_file_counts(1, 0, 0)
                .with_row_counts(1, 0, 0)
                .build()
                .unwrap();
            let mut writer = ManifestListWriter::new();
            writer.append(manifest);
            writer.append(delete_manifest);
            let bytes = writer.to_bytes(FormatVersion::V2).unwrap();
            fs::write(&manifest_list_path, bytes).unwrap();

            let engine = SailRestModelCatalogEngine;
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );

            let plan = engine
                .plan_scan(ScanPlanningRequest {
                    table: table.clone(),
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location.clone()),
                    table_metadata: metadata.clone(),
                    projection: vec!["id".to_string()],
                    filters: Vec::new(),
                    limit: None,
                    snapshot_id: Some(42),
                    start_snapshot_id: None,
                    end_snapshot_id: None,
                })
                .await
                .expect("scan planning should produce a manifest-list plan task");
            let plan_task = plan.scan_tasks[0]["plan-task"]
                .as_str()
                .unwrap()
                .to_string();

            let fetched = engine
                .fetch_scan_tasks(FetchScanTasksRequest {
                    table: table.clone(),
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location.clone()),
                    table_metadata: metadata.clone(),
                    plan_task,
                    required_projection: Vec::new(),
                    required_filters: Vec::new(),
                })
                .await
                .expect("fetch should expand manifest list through Sail I/O");

            assert_eq!(fetched.plan_tasks.len(), 2);
            assert_eq!(fetched.plan_tasks[0]["task-type"], json!("manifest"));
            assert_eq!(fetched.plan_tasks[0]["manifest-list"], json!(manifest_list));
            assert_eq!(fetched.plan_tasks[0]["manifest-path"], json!(manifest_path));
            assert_eq!(fetched.file_scan_tasks.len(), 1);
            assert_eq!(fetched.delete_files.len(), 1);
            assert_eq!(
                fetched.file_scan_tasks[0].pointer("/delete-file-references/0"),
                Some(&json!(0))
            );
            assert_eq!(
                fetched.delete_files[0].pointer("/content"),
                Some(&json!("position-deletes"))
            );
            assert_eq!(
                fetched.delete_files[0].pointer("/file-path"),
                Some(&json!(delete_file_path))
            );
            let manifest_plan_task = fetched.plan_tasks[0]["plan-task"]
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(
                fetched
                    .residual_filter
                    .unwrap()
                    .pointer("/lakecat:sail-target"),
                Some(&json!("sail_iceberg::io::load_manifest_list"))
            );

            let manifest_fetched = engine
                .fetch_scan_tasks(FetchScanTasksRequest {
                    table,
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location),
                    table_metadata: metadata,
                    plan_task: manifest_plan_task,
                    required_projection: Vec::new(),
                    required_filters: Vec::new(),
                })
                .await
                .expect("fetch should expand manifest through Sail I/O");

            assert_eq!(manifest_fetched.plan_tasks.len(), 0);
            assert_eq!(manifest_fetched.delete_files.len(), 0);
            assert_eq!(manifest_fetched.file_scan_tasks.len(), 1);
            assert_eq!(
                manifest_fetched.file_scan_tasks[0].pointer("/data-file/file-path"),
                Some(&json!(data_file_path))
            );
            assert_eq!(
                manifest_fetched.file_scan_tasks[0].pointer("/data-file/file-format"),
                Some(&json!("parquet"))
            );
            assert_eq!(
                manifest_fetched.file_scan_tasks[0].pointer("/data-file/split-offsets/0"),
                Some(&json!(4))
            );

            let _ = fs::remove_dir_all(root);
        }

        #[tokio::test]
        async fn preserves_filter_context_and_prunes_loaded_file_bounds() {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("lakecat-filter-prune-{unique}"));
            let table_dir = root.join("table");
            let metadata_dir = table_dir.join("metadata");
            fs::create_dir_all(&metadata_dir).unwrap();

            let manifest_list_path = metadata_dir.join("snap-42.avro");
            let manifest_path = Url::from_file_path(metadata_dir.join("manifest-1.avro"))
                .unwrap()
                .to_string();
            let table_location = Url::from_file_path(&table_dir).unwrap().to_string();
            let metadata_location = Url::from_file_path(metadata_dir.join("00000.json"))
                .unwrap()
                .to_string();
            let manifest_list = Url::from_file_path(&manifest_list_path)
                .unwrap()
                .to_string();
            let metadata = sample_metadata_with_locations(3, table_location, manifest_list.clone());
            let table_metadata =
                TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
            let manifest_metadata = ManifestMetadata::new(
                Arc::new(table_metadata.current_schema().unwrap().clone()),
                table_metadata.current_schema_id,
                table_metadata.default_partition_spec().unwrap().clone(),
                FormatVersion::V2,
                ManifestContentType::Data,
            );
            let mut manifest_writer =
                ManifestWriterBuilder::new(Some(42), None, manifest_metadata).build();
            let filter = json!({
                "type": "eq",
                "term": "id",
                "value": "evt-1"
            });
            for id in ["evt-1", "evt-2"] {
                let mut lower_bounds = HashMap::new();
                lower_bounds.insert(
                    1,
                    Datum::new(
                        PrimitiveType::String,
                        PrimitiveLiteral::String(id.to_string()),
                    ),
                );
                let upper_bounds = lower_bounds.clone();
                let data_file = DataFile {
                    content: DataContentType::Data,
                    file_path: Url::from_file_path(
                        table_dir.join("data").join(format!("{id}.parquet")),
                    )
                    .unwrap()
                    .to_string(),
                    file_format: DataFileFormat::Parquet,
                    partition: Vec::new(),
                    record_count: 3,
                    file_size_in_bytes: 123,
                    column_sizes: HashMap::new(),
                    value_counts: HashMap::new(),
                    null_value_counts: HashMap::new(),
                    nan_value_counts: HashMap::new(),
                    lower_bounds,
                    upper_bounds,
                    block_size_in_bytes: None,
                    key_metadata: None,
                    split_offsets: Vec::new(),
                    equality_ids: Vec::new(),
                    sort_order_id: None,
                    first_row_id: None,
                    partition_spec_id: 0,
                    referenced_data_file: None,
                    content_offset: None,
                    content_size_in_bytes: None,
                };
                assert_eq!(
                    data_file_may_match_filter(&table_metadata, &data_file, &filter).unwrap(),
                    id == "evt-1"
                );
                manifest_writer.add(data_file);
            }
            fs::write(
                Url::parse(&manifest_path).unwrap().to_file_path().unwrap(),
                manifest_writer.to_avro_bytes_v2().unwrap(),
            )
            .unwrap();
            let parsed_manifest = sail_iceberg::spec::Manifest::parse_avro(
                &fs::read(Url::parse(&manifest_path).unwrap().to_file_path().unwrap()).unwrap(),
            )
            .expect("manifest bounds should round-trip through Sail Avro");
            assert_eq!(
                parsed_manifest
                    .entries()
                    .first()
                    .unwrap()
                    .data_file
                    .lower_bounds()
                    .get(&1)
                    .unwrap()
                    .literal,
                PrimitiveLiteral::String("evt-1".to_string())
            );

            let manifest = ManifestFile::builder()
                .with_manifest_path(&manifest_path)
                .with_manifest_length(10)
                .with_partition_spec_id(0)
                .with_content(ManifestContentType::Data)
                .with_sequence_number(7)
                .with_min_sequence_number(7)
                .with_added_snapshot_id(42)
                .with_file_counts(2, 0, 0)
                .with_row_counts(6, 0, 0)
                .build()
                .unwrap();
            let mut writer = ManifestListWriter::new();
            writer.append(manifest);
            fs::write(
                &manifest_list_path,
                writer.to_bytes(FormatVersion::V2).unwrap(),
            )
            .unwrap();

            let engine = SailRestModelCatalogEngine;
            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let plan = engine
                .plan_scan(ScanPlanningRequest {
                    table: table.clone(),
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location.clone()),
                    table_metadata: metadata.clone(),
                    projection: vec!["id".to_string()],
                    filters: vec![filter],
                    limit: None,
                    snapshot_id: Some(42),
                    start_snapshot_id: None,
                    end_snapshot_id: None,
                })
                .await
                .expect("filtered scan planning should validate");
            assert!(
                plan.scan_tasks[0]["plan-task"]
                    .as_str()
                    .unwrap()
                    .starts_with("lakecat:sail-json-hmac:")
            );
            let decoded = decode_plan_task(plan.scan_tasks[0]["plan-task"].as_str().unwrap())
                .expect("structured plan task should decode");
            assert_eq!(decoded.table.as_deref(), Some(table.stable_id().as_str()));
            assert_eq!(decoded.projection, vec!["id".to_string()]);
            assert_eq!(decoded.filters.len(), 1);
            let plan_task = plan.scan_tasks[0]["plan-task"].as_str().unwrap();
            let mut tampered_plan_task = plan_task.to_string();
            let signature_start = "lakecat:sail-json-hmac:".len();
            let replacement = if &tampered_plan_task[signature_start..signature_start + 1] == "0" {
                "1"
            } else {
                "0"
            };
            tampered_plan_task.replace_range(signature_start..signature_start + 1, replacement);
            assert!(
                decode_plan_task(&tampered_plan_task)
                    .expect_err("tampered plan task should fail signature verification")
                    .to_string()
                    .contains("signature")
            );
            let other_table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("other_events").unwrap(),
            );
            let rejected = engine
                .fetch_scan_tasks(FetchScanTasksRequest {
                    table: other_table,
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location.clone()),
                    table_metadata: metadata.clone(),
                    plan_task: plan.scan_tasks[0]["plan-task"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    required_projection: Vec::new(),
                    required_filters: Vec::new(),
                })
                .await
                .expect_err("structured plan tasks should be bound to the planned table");
            assert!(
                rejected
                    .to_string()
                    .contains("does not match requested table")
            );
            let rejected = engine
                .fetch_scan_tasks(FetchScanTasksRequest {
                    table: table.clone(),
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location.clone()),
                    table_metadata: metadata.clone(),
                    plan_task: format!("lakecat:sail:manifest-list:42:{manifest_list}"),
                    required_projection: vec!["id".to_string()],
                    required_filters: Vec::new(),
                })
                .await
                .expect_err("legacy plan task should not satisfy a governed projection");
            assert!(
                rejected
                    .to_string()
                    .contains("required governed projection")
            );

            let fetched = engine
                .fetch_scan_tasks(FetchScanTasksRequest {
                    table,
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location),
                    table_metadata: metadata,
                    plan_task: plan.scan_tasks[0]["plan-task"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    required_projection: vec!["id".to_string()],
                    required_filters: vec![json!({
                        "type": "eq",
                        "term": "id",
                        "value": "evt-1"
                    })],
                })
                .await
                .expect("fetch should prune files with Sail metadata bounds");
            assert_eq!(fetched.file_scan_tasks.len(), 1);
            assert!(
                fetched.file_scan_tasks[0]["data-file"]["file-path"]
                    .as_str()
                    .unwrap()
                    .ends_with("/evt-1.parquet")
            );

            let _ = fs::remove_dir_all(root);
        }

        #[tokio::test]
        async fn plans_incremental_append_chain_with_sail_manifest_io() {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("lakecat-incremental-{unique}"));
            let table_dir = root.join("table");
            let metadata_dir = table_dir.join("metadata");
            fs::create_dir_all(&metadata_dir).unwrap();

            let manifest_list_path = metadata_dir.join("snap-42.avro");
            let manifest_path = Url::from_file_path(metadata_dir.join("manifest-42.avro"))
                .unwrap()
                .to_string();
            let delete_manifest_path =
                Url::from_file_path(metadata_dir.join("delete-manifest-42.avro"))
                    .unwrap()
                    .to_string();
            let data_file_path =
                Url::from_file_path(table_dir.join("data").join("part-42.parquet"))
                    .unwrap()
                    .to_string();
            let delete_file_path = Url::from_file_path(
                table_dir
                    .join("deletes")
                    .join("part-42-pos-deletes.parquet"),
            )
            .unwrap()
            .to_string();
            let table_location = Url::from_file_path(&table_dir).unwrap().to_string();
            let metadata_location = Url::from_file_path(metadata_dir.join("00001.json"))
                .unwrap()
                .to_string();
            let manifest_list = Url::from_file_path(&manifest_list_path)
                .unwrap()
                .to_string();
            let metadata = json!({
                "format-version": 3,
                "table-uuid": "11111111-1111-1111-1111-111111111111",
                "location": table_location,
                "last-sequence-number": 8,
                "last-updated-ms": 1710000000000_i64,
                "last-column-id": 1,
                "schemas": [{
                    "type": "struct",
                    "schema-id": 1,
                    "fields": [{
                        "id": 1,
                        "name": "id",
                        "type": "string",
                        "required": true
                    }]
                }],
                "current-schema-id": 1,
                "partition-specs": [{"spec-id": 0, "fields": []}],
                "default-spec-id": 0,
                "current-snapshot-id": 42,
                "snapshots": [{
                    "snapshot-id": 41,
                    "sequence-number": 7,
                    "timestamp-ms": 1709999999000_i64,
                    "manifest-list": "file:///tmp/lakecat-parent-not-loaded.avro",
                    "summary": {"operation": "append"},
                    "schema-id": 1
                }, {
                    "snapshot-id": 42,
                    "parent-snapshot-id": 41,
                    "sequence-number": 8,
                    "timestamp-ms": 1710000000000_i64,
                    "manifest-list": manifest_list,
                    "summary": {"operation": "append"},
                    "schema-id": 1
                }],
                "snapshot-log": [{
                    "timestamp-ms": 1709999999000_i64,
                    "snapshot-id": 41
                }, {
                    "timestamp-ms": 1710000000000_i64,
                    "snapshot-id": 42
                }]
            });
            let table_metadata =
                TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
            let manifest_metadata = ManifestMetadata::new(
                Arc::new(table_metadata.current_schema().unwrap().clone()),
                table_metadata.current_schema_id,
                table_metadata.default_partition_spec().unwrap().clone(),
                FormatVersion::V2,
                ManifestContentType::Data,
            );
            let mut manifest_writer =
                ManifestWriterBuilder::new(Some(42), None, manifest_metadata).build();
            manifest_writer.add(DataFile {
                content: DataContentType::Data,
                file_path: data_file_path.clone(),
                file_format: DataFileFormat::Parquet,
                partition: Vec::new(),
                record_count: 3,
                file_size_in_bytes: 123,
                column_sizes: HashMap::new(),
                value_counts: HashMap::new(),
                null_value_counts: HashMap::new(),
                nan_value_counts: HashMap::new(),
                lower_bounds: HashMap::new(),
                upper_bounds: HashMap::new(),
                block_size_in_bytes: None,
                key_metadata: None,
                split_offsets: Vec::new(),
                equality_ids: Vec::new(),
                sort_order_id: None,
                first_row_id: None,
                partition_spec_id: 0,
                referenced_data_file: None,
                content_offset: None,
                content_size_in_bytes: None,
            });
            std::fs::write(
                Url::parse(&manifest_path).unwrap().to_file_path().unwrap(),
                manifest_writer.to_avro_bytes_v2().unwrap(),
            )
            .unwrap();

            let delete_manifest_metadata = ManifestMetadata::new(
                Arc::new(table_metadata.current_schema().unwrap().clone()),
                table_metadata.current_schema_id,
                table_metadata.default_partition_spec().unwrap().clone(),
                FormatVersion::V2,
                ManifestContentType::Deletes,
            );
            let mut delete_manifest_writer =
                ManifestWriterBuilder::new(Some(42), None, delete_manifest_metadata).build();
            delete_manifest_writer.add(DataFile {
                content: DataContentType::PositionDeletes,
                file_path: delete_file_path.clone(),
                file_format: DataFileFormat::Parquet,
                partition: Vec::new(),
                record_count: 1,
                file_size_in_bytes: 64,
                column_sizes: HashMap::new(),
                value_counts: HashMap::new(),
                null_value_counts: HashMap::new(),
                nan_value_counts: HashMap::new(),
                lower_bounds: HashMap::new(),
                upper_bounds: HashMap::new(),
                block_size_in_bytes: None,
                key_metadata: None,
                split_offsets: Vec::new(),
                equality_ids: Vec::new(),
                sort_order_id: None,
                first_row_id: None,
                partition_spec_id: 0,
                referenced_data_file: Some(data_file_path.clone()),
                content_offset: None,
                content_size_in_bytes: None,
            });
            std::fs::write(
                Url::parse(&delete_manifest_path)
                    .unwrap()
                    .to_file_path()
                    .unwrap(),
                delete_manifest_writer.to_avro_bytes_v2().unwrap(),
            )
            .unwrap();

            let mut list_writer = ManifestListWriter::new();
            list_writer.append(
                ManifestFile::builder()
                    .with_manifest_path("file:///tmp/lakecat-inherited-not-loaded.avro")
                    .with_manifest_length(10)
                    .with_partition_spec_id(0)
                    .with_content(ManifestContentType::Data)
                    .with_sequence_number(7)
                    .with_min_sequence_number(7)
                    .with_added_snapshot_id(41)
                    .with_file_counts(0, 1, 0)
                    .with_row_counts(0, 3, 0)
                    .build()
                    .unwrap(),
            );
            list_writer.append(
                ManifestFile::builder()
                    .with_manifest_path(&manifest_path)
                    .with_manifest_length(10)
                    .with_partition_spec_id(0)
                    .with_content(ManifestContentType::Data)
                    .with_sequence_number(8)
                    .with_min_sequence_number(8)
                    .with_added_snapshot_id(42)
                    .with_file_counts(1, 0, 0)
                    .with_row_counts(3, 0, 0)
                    .build()
                    .unwrap(),
            );
            list_writer.append(
                ManifestFile::builder()
                    .with_manifest_path(&delete_manifest_path)
                    .with_manifest_length(10)
                    .with_partition_spec_id(0)
                    .with_content(ManifestContentType::Deletes)
                    .with_sequence_number(8)
                    .with_min_sequence_number(8)
                    .with_added_snapshot_id(42)
                    .with_file_counts(1, 0, 0)
                    .with_row_counts(1, 0, 0)
                    .build()
                    .unwrap(),
            );
            fs::write(
                &manifest_list_path,
                list_writer.to_bytes(FormatVersion::V2).unwrap(),
            )
            .unwrap();

            let table = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let engine = SailRestModelCatalogEngine;
            let plan = engine
                .plan_scan(ScanPlanningRequest {
                    table: table.clone(),
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location.clone()),
                    table_metadata: metadata.clone(),
                    projection: vec!["id".to_string()],
                    filters: Vec::new(),
                    limit: None,
                    snapshot_id: None,
                    start_snapshot_id: Some(41),
                    end_snapshot_id: Some(42),
                })
                .await
                .expect("incremental append planning should use Sail manifest-list I/O");

            assert_eq!(plan.snapshot_id, Some(42));
            assert_eq!(plan.scan_tasks.len(), 1);
            assert_eq!(
                plan.scan_tasks[0].pointer("/task-type"),
                Some(&json!("incremental-manifest-list"))
            );
            assert_eq!(
                plan.scan_tasks[0].pointer("/manifest-list"),
                Some(&json!(manifest_list))
            );
            assert_eq!(
                plan.residual_filter.as_ref().unwrap().pointer("/scan-mode"),
                Some(&json!("incremental"))
            );

            let fetched = engine
                .fetch_scan_tasks(FetchScanTasksRequest {
                    table,
                    principal: Principal::anonymous(),
                    metadata_location: Some(metadata_location),
                    table_metadata: metadata,
                    plan_task: plan.scan_tasks[0]["plan-task"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    required_projection: Vec::new(),
                    required_filters: Vec::new(),
                })
                .await
                .expect("incremental manifest-list task should expand through Sail I/O");
            assert_eq!(fetched.plan_tasks.len(), 2);
            assert_eq!(
                fetched.file_scan_tasks[0].pointer("/data-file/file-path"),
                Some(&json!(data_file_path))
            );
            assert_eq!(
                fetched.file_scan_tasks[0].pointer("/delete-file-references/0"),
                Some(&json!(0))
            );
            assert_eq!(
                fetched.delete_files[0].pointer("/file-path"),
                Some(&json!(delete_file_path))
            );

            let _ = fs::remove_dir_all(root);
        }

        #[test]
        fn encodes_delete_files_as_generated_iceberg_rest_models() {
            let delete_file = DataFile {
                content: DataContentType::PositionDeletes,
                file_path: "file:///tmp/events/delete-1.parquet".to_string(),
                file_format: DataFileFormat::Parquet,
                partition: Vec::new(),
                record_count: 1,
                file_size_in_bytes: 64,
                column_sizes: HashMap::new(),
                value_counts: HashMap::new(),
                null_value_counts: HashMap::new(),
                nan_value_counts: HashMap::new(),
                lower_bounds: HashMap::new(),
                upper_bounds: HashMap::new(),
                block_size_in_bytes: None,
                key_metadata: None,
                split_offsets: Vec::new(),
                equality_ids: Vec::new(),
                sort_order_id: None,
                first_row_id: None,
                partition_spec_id: 0,
                referenced_data_file: Some("file:///tmp/events/data-1.parquet".to_string()),
                content_offset: Some(10),
                content_size_in_bytes: Some(20),
            };

            let encoded = rest_delete_file_value(&delete_file).unwrap();
            let _: models::PositionDeleteFile = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(encoded["content"], json!("position-deletes"));
            assert_eq!(encoded["content-offset"], json!(10));
            assert_eq!(encoded["content-size-in-bytes"], json!(20));
        }

        fn sample_metadata(format_version: i32) -> Value {
            sample_metadata_with_locations(
                format_version,
                "file:///tmp/events".to_string(),
                "file:///tmp/events/metadata/snap-42.avro".to_string(),
            )
        }

        fn sample_metadata_with_locations(
            format_version: i32,
            table_location: String,
            manifest_list: String,
        ) -> Value {
            json!({
                "format-version": format_version,
                "table-uuid": "11111111-1111-1111-1111-111111111111",
                "location": table_location,
                "last-sequence-number": 7,
                "last-updated-ms": 1710000000000_i64,
                "last-column-id": 1,
                "schemas": [{
                    "type": "struct",
                    "schema-id": 1,
                    "fields": [{
                        "id": 1,
                        "name": "id",
                        "type": "string",
                        "required": true,
                        "doc": "Event identifier."
                    }]
                }],
                "current-schema-id": 1,
                "partition-specs": [{"spec-id": 0, "fields": []}],
                "default-spec-id": 0,
                "current-snapshot-id": 42,
                "snapshots": [{
                    "snapshot-id": 42,
                    "sequence-number": 7,
                    "timestamp-ms": 1710000000000_i64,
                    "manifest-list": manifest_list,
                    "summary": {"operation": "append"},
                    "schema-id": 1
                }],
                "snapshot-log": [{
                    "timestamp-ms": 1710000000000_i64,
                    "snapshot-id": 42
                }]
            })
        }
    }
}
