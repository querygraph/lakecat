use std::sync::Arc;

use arrow_schema::{DataType, Field, Fields, TimeUnit};
use lakecat_core::{LakeCatError, Namespace, Principal, TableIdent, TableName, WarehouseName};
use lakecat_security::{
    AuthorizationRequest, CatalogAction, GovernanceEngine, NamespaceDropCapability,
    ReadRestriction, TableCommitCapability, TableCreateCapability, TableDropCapability,
    TableLoadCapability, TableScanCapability, ViewDropCapability, ViewLoadCapability,
    ViewManageCapability,
};
use lakecat_store::{
    CatalogStore, PolicyBinding, TableCommit, TableRecord, ViewColumnRecord, ViewRecord,
};
use sail_catalog::error::{CatalogError, CatalogObject, CatalogResult};
use sail_catalog::provider::{
    AlterTableOptions, CatalogProvider, CommitTableOptions, CreateDatabaseOptions,
    CreateTableOptions, CreateViewColumnOptions, CreateViewOptions, DropDatabaseOptions,
    DropTableOptions, DropViewOptions, GetTableCommitsOptions, GetTableCommitsResponse,
    Namespace as SailNamespace, TableCommitInfo,
};
use sail_catalog_iceberg::{LoadTableResult, load_table_result_to_status};
use sail_common_datafusion::catalog::{
    CatalogPartitionField, CatalogTableConstraint, CatalogTableSort, DatabaseStatus,
    PartitionTransform, TableColumnStatus, TableKind, TableStatus,
};
use serde_json::json;

use crate::{
    CommitPreparationRequest, FetchScanTasksPlan, FetchScanTasksRequest, SailCatalogEngine,
    ScanPlan, ScanPlanningRequest,
};

#[derive(Debug, Clone, Default)]
pub struct ProviderScanPlanningRequest {
    pub projection: Vec<String>,
    pub filters: Vec<serde_json::Value>,
    pub limit: Option<u64>,
    pub snapshot_id: Option<i64>,
    pub start_snapshot_id: Option<i64>,
    pub end_snapshot_id: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderFetchScanTasksRequest {
    pub plan_task: String,
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
        receipt
            .with_read_restriction_policy_hash()
            .map_err(catalog_error)
    }

    pub async fn authorize_table_scan(
        &self,
        database: &SailNamespace,
        table: &str,
    ) -> CatalogResult<TableScanCapability> {
        let ident = self.ident(database, table)?;
        self.authorize_table_scan_for_ident(&ident).await
    }

    pub async fn authorize_table_scan_for_ident(
        &self,
        ident: &TableIdent,
    ) -> CatalogResult<TableScanCapability> {
        let policy_bindings = self
            .store
            .policy_bindings_for_table(ident)
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
        TableScanCapability::from_receipt(receipt, ident.clone()).map_err(catalog_error)
    }

    pub async fn plan_table_scan(
        &self,
        database: &SailNamespace,
        table: &str,
        request: ProviderScanPlanningRequest,
    ) -> CatalogResult<ScanPlan> {
        let capability = self.authorize_table_scan(database, table).await?;
        self.plan_table_scan_with_capability(capability, request)
            .await
    }

    pub async fn plan_table_scan_for_ident(
        &self,
        table: &TableIdent,
        request: ProviderScanPlanningRequest,
    ) -> CatalogResult<ScanPlan> {
        let capability = self.authorize_table_scan_for_ident(table).await?;
        self.plan_table_scan_with_capability(capability, request)
            .await
    }

    /// Plans through Sail using a capability that the HTTP/catalog boundary
    /// already authorized. This preserves the exact receipt and restriction
    /// that LakeCat will record as scan evidence.
    pub async fn plan_authorized_table_scan(
        &self,
        capability: &TableScanCapability,
        request: ProviderScanPlanningRequest,
    ) -> CatalogResult<ScanPlan> {
        self.ensure_capability_warehouse(capability)?;
        self.plan_table_scan_with_capability(capability.clone(), request)
            .await
    }

    async fn plan_table_scan_with_capability(
        &self,
        capability: TableScanCapability,
        request: ProviderScanPlanningRequest,
    ) -> CatalogResult<ScanPlan> {
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

    pub async fn fetch_table_scan_tasks(
        &self,
        database: &SailNamespace,
        table: &str,
        request: ProviderFetchScanTasksRequest,
    ) -> CatalogResult<FetchScanTasksPlan> {
        let capability = self.authorize_table_scan(database, table).await?;
        self.fetch_table_scan_tasks_with_capability(capability, request)
            .await
    }

    pub async fn fetch_table_scan_tasks_for_ident(
        &self,
        table: &TableIdent,
        request: ProviderFetchScanTasksRequest,
    ) -> CatalogResult<FetchScanTasksPlan> {
        let capability = self.authorize_table_scan_for_ident(table).await?;
        self.fetch_table_scan_tasks_with_capability(capability, request)
            .await
    }

    /// Fetches through Sail using the same capability that authorized the
    /// plan. A task fetch must not silently mint a second policy decision.
    pub async fn fetch_authorized_table_scan_tasks(
        &self,
        capability: &TableScanCapability,
        request: ProviderFetchScanTasksRequest,
    ) -> CatalogResult<FetchScanTasksPlan> {
        self.ensure_capability_warehouse(capability)?;
        self.fetch_table_scan_tasks_with_capability(capability.clone(), request)
            .await
    }

    async fn fetch_table_scan_tasks_with_capability(
        &self,
        capability: TableScanCapability,
        request: ProviderFetchScanTasksRequest,
    ) -> CatalogResult<FetchScanTasksPlan> {
        let restriction = capability.read_restriction().map_err(catalog_error)?;
        let record = self
            .store
            .load_table(capability.table())
            .await
            .map_err(catalog_error)?;
        self.sail
            .fetch_scan_tasks(FetchScanTasksRequest {
                table: capability.table().clone(),
                principal: capability.receipt().principal.clone(),
                metadata_location: record.metadata_location,
                table_metadata: record.metadata,
                plan_task: request.plan_task,
                required_projection: restriction
                    .effective_projection(&[])
                    .map_err(catalog_error)?,
                required_filters: restriction.mandatory_filters(),
            })
            .await
            .map_err(catalog_error)
    }

    fn ensure_capability_warehouse(&self, capability: &TableScanCapability) -> CatalogResult<()> {
        if capability.table().warehouse == self.warehouse {
            Ok(())
        } else {
            Err(CatalogError::Conflict(
                "LakeCat scan capability does not target this Sail warehouse".to_string(),
            ))
        }
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
        database: &SailNamespace,
        options: DropDatabaseOptions,
    ) -> CatalogResult<()> {
        let receipt = self.authorize_catalog(CatalogAction::NamespaceDrop).await?;
        let _capability = NamespaceDropCapability::from_receipt(receipt).map_err(catalog_error)?;
        let namespace = lakecat_namespace(database)?;
        let DropDatabaseOptions { if_exists, cascade } = options;
        if cascade {
            return Err(CatalogError::NotSupported(
                "LakeCat Sail namespace drop does not support cascade".to_string(),
            ));
        }
        match self.store.drop_namespace(&self.warehouse, &namespace).await {
            Ok(_) => Ok(()),
            Err(error) if if_exists && error.to_string().contains("not found") => Ok(()),
            Err(error) => Err(catalog_error(error)),
        }
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
        let (fields, column_ids, last_column_id) = iceberg_fields_from_columns(&options.columns);
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

    async fn get_table(&self, database: &SailNamespace, table: &str) -> CatalogResult<TableStatus> {
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
        let capability =
            TableCommitCapability::from_receipt(receipt, ident.clone()).map_err(catalog_error)?;
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
                "LakeCat Sail commit discovery requires a non-negative start version".to_string(),
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
        database: &SailNamespace,
        view: &str,
        options: CreateViewOptions,
    ) -> CatalogResult<TableStatus> {
        let receipt = self.authorize_catalog(CatalogAction::ViewManage).await?;
        let capability = ViewManageCapability::from_receipt(receipt).map_err(catalog_error)?;
        let namespace = lakecat_namespace(database)?;
        self.store
            .load_namespace(&self.warehouse, &namespace)
            .await
            .map_err(catalog_error)?;
        let view_name = TableName::new(view).map_err(catalog_error)?;
        let existing = self
            .store
            .load_view(&self.warehouse, &namespace, &view_name)
            .await;
        match existing {
            Ok(record) if options.if_not_exists => return view_status(&self.name, &record),
            Ok(_) if !options.replace => {
                return Err(CatalogError::AlreadyExists(
                    CatalogObject::View,
                    view.to_string(),
                ));
            }
            Ok(_) | Err(LakeCatError::NotFound { .. }) => {}
            Err(error) => return Err(catalog_error(error)),
        }
        let CreateViewOptions {
            columns,
            definition,
            if_not_exists: _,
            replace: _,
            comment,
            properties,
        } = options;
        let mut properties = properties
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        if let Some(comment) = comment {
            properties.insert(VIEW_COMMENT_PROPERTY.to_string(), comment);
        }
        let record = ViewRecord::new(
            self.warehouse.clone(),
            namespace,
            view_name,
            definition,
            "sql",
            None,
            properties,
            capability.receipt().principal.clone(),
        )
        .map_err(catalog_error)?
        .with_columns(view_columns_from_sail(columns)?)
        .map_err(catalog_error)?;
        let record = self
            .store
            .upsert_view(record)
            .await
            .map_err(catalog_error)?;
        view_status(&self.name, &record)
    }

    async fn get_view(&self, database: &SailNamespace, view: &str) -> CatalogResult<TableStatus> {
        let receipt = self.authorize_catalog(CatalogAction::ViewLoad).await?;
        let _capability = ViewLoadCapability::from_receipt(receipt).map_err(catalog_error)?;
        let namespace = lakecat_namespace(database)?;
        let view_name = TableName::new(view).map_err(catalog_error)?;
        let record = self
            .store
            .load_view(&self.warehouse, &namespace, &view_name)
            .await
            .map_err(catalog_error)?;
        view_status(&self.name, &record)
    }

    async fn list_views(&self, database: &SailNamespace) -> CatalogResult<Vec<TableStatus>> {
        let receipt = self.authorize_catalog(CatalogAction::ViewLoad).await?;
        let _capability = ViewLoadCapability::from_receipt(receipt).map_err(catalog_error)?;
        let namespace = lakecat_namespace(database)?;
        self.store
            .list_views(&self.warehouse, &namespace)
            .await
            .map_err(catalog_error)?
            .iter()
            .map(|record| view_status(&self.name, record))
            .collect()
    }

    async fn drop_view(
        &self,
        database: &SailNamespace,
        view: &str,
        options: DropViewOptions,
    ) -> CatalogResult<()> {
        let receipt = self.authorize_catalog(CatalogAction::ViewDrop).await?;
        let capability =
            ViewDropCapability::from_receipt(receipt.clone()).map_err(catalog_error)?;
        let namespace = lakecat_namespace(database)?;
        let view_name = TableName::new(view).map_err(catalog_error)?;
        match self
            .store
            .drop_view(
                &self.warehouse,
                &namespace,
                &view_name,
                capability.receipt().principal.clone(),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(LakeCatError::NotFound { .. }) if options.if_exists => Ok(()),
            Err(error) => Err(catalog_error(error)),
        }
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

const VIEW_COMMENT_PROPERTY: &str = "lakecat:view-comment";

fn view_columns_from_sail(
    columns: Vec<CreateViewColumnOptions>,
) -> CatalogResult<Vec<ViewColumnRecord>> {
    columns
        .into_iter()
        .map(|column| {
            Ok(ViewColumnRecord {
                name: column.name,
                data_type: serde_json::to_value(column.data_type).map_err(catalog_error)?,
                nullable: column.nullable,
                comment: column.comment,
            })
        })
        .collect()
}

fn view_status(catalog: &str, record: &ViewRecord) -> CatalogResult<TableStatus> {
    let columns = record
        .columns
        .iter()
        .map(|column| {
            Ok(TableColumnStatus {
                name: column.name.clone(),
                data_type: serde_json::from_value(column.data_type.clone())
                    .map_err(catalog_error)?,
                nullable: column.nullable,
                comment: column.comment.clone(),
                default: None,
                generated_always_as: None,
                identity: None,
                is_partition: false,
                is_bucket: false,
                is_cluster: false,
            })
        })
        .collect::<CatalogResult<Vec<_>>>()?;
    Ok(TableStatus {
        catalog: Some(catalog.to_string()),
        database: record.namespace.parts().to_vec(),
        name: record.name.as_str().to_string(),
        kind: TableKind::View {
            definition: record.sql.clone(),
            columns,
            comment: record.properties.get(VIEW_COMMENT_PROPERTY).cloned(),
            properties: record
                .properties
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        },
    })
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
            specs
                .iter()
                .find(|spec| spec.get("spec-id").and_then(serde_json::Value::as_i64) == Some(id))
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
            orders
                .iter()
                .find(|order| order.get("order-id").and_then(serde_json::Value::as_i64) == Some(id))
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

fn datafusion_type_to_iceberg(data_type: &DataType, next_field_id: &mut i32) -> serde_json::Value {
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

fn iceberg_nested_field_from_arrow(field: &Field, next_field_id: &mut i32) -> serde_json::Value {
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
        None | Some(PartitionTransform::Identity) => ("identity".to_string(), field.column.clone()),
        Some(PartitionTransform::Year) => ("year".to_string(), format!("{}_year", field.column)),
        Some(PartitionTransform::Month) => ("month".to_string(), format!("{}_month", field.column)),
        Some(PartitionTransform::Day) => ("day".to_string(), format!("{}_day", field.column)),
        Some(PartitionTransform::Hour) => ("hour".to_string(), format!("{}_hour", field.column)),
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

fn table_commit_info(record: lakecat_store::TableCommitRecord) -> CatalogResult<TableCommitInfo> {
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
    if message.contains("invalid argument") {
        CatalogError::InvalidArgument(message)
    } else if message.contains("not found") {
        CatalogError::NotFound(CatalogObject::Table, message)
    } else if message.contains("already exists") {
        CatalogError::AlreadyExists(CatalogObject::Table, message)
    } else if message.contains("conflict") {
        CatalogError::Conflict(message)
    } else if message.contains("not supported") {
        CatalogError::NotSupported(message)
    } else if message.contains("internal error") {
        CatalogError::Internal(message)
    } else {
        CatalogError::External(message)
    }
}

#[cfg(test)]
mod tests;
