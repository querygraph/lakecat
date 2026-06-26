use async_trait::async_trait;
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName, WarehouseName,
};
use serde_json::Value;

#[async_trait]
pub trait CatalogStore: Send + Sync + 'static {
    async fn create_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: Namespace,
    ) -> LakeCatResult<()>;
    async fn list_namespaces(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>>;
    async fn load_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        self.list_namespaces(warehouse)
            .await?
            .into_iter()
            .find(|candidate| candidate == namespace)
            .ok_or_else(|| namespace_not_found(namespace))
    }
    async fn drop_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        let namespace = self.load_namespace(warehouse, namespace).await?;
        if self
            .list_tables(warehouse)
            .await?
            .iter()
            .any(|table| table.ident.namespace == namespace)
        {
            return Err(namespace_not_empty(&namespace, "tables"));
        }
        if !self.list_views(warehouse, &namespace).await?.is_empty() {
            return Err(namespace_not_empty(&namespace, "views"));
        }
        if self
            .list_policy_bindings(warehouse)
            .await?
            .iter()
            .any(|binding| binding.namespace.as_ref() == Some(&namespace))
        {
            return Err(namespace_not_empty(&namespace, "policy bindings"));
        }
        Ok(namespace)
    }
    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>>;
    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord>;
    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord>;
    async fn commit_table(
        &self,
        ident: &TableIdent,
        commit: TableCommit,
    ) -> LakeCatResult<TableRecord>;
    async fn replay_table_commit(
        &self,
        _ident: &TableIdent,
        _idempotency_key: &str,
        _idempotency_request_hash: &str,
    ) -> LakeCatResult<Option<TableRecord>> {
        Ok(None)
    }
    async fn table_commit_records(
        &self,
        ident: &TableIdent,
        start_version: u64,
        end_version: Option<u64>,
    ) -> LakeCatResult<Vec<TableCommitRecord>>;
    async fn upsert_server(&self, server: ServerRecord) -> LakeCatResult<ServerRecord> {
        server.validate()?;
        Ok(server)
    }
    async fn list_servers(&self) -> LakeCatResult<Vec<ServerRecord>> {
        Ok(Vec::new())
    }
    async fn upsert_project(&self, project: ProjectRecord) -> LakeCatResult<ProjectRecord> {
        project.validate()?;
        Ok(project)
    }
    async fn list_projects(&self) -> LakeCatResult<Vec<ProjectRecord>> {
        Ok(Vec::new())
    }
    async fn upsert_warehouse(&self, warehouse: WarehouseRecord) -> LakeCatResult<WarehouseRecord> {
        warehouse.validate()?;
        Ok(warehouse)
    }
    async fn load_warehouse(&self, warehouse: &WarehouseName) -> LakeCatResult<WarehouseRecord> {
        self.list_warehouses()
            .await?
            .into_iter()
            .find(|record| record.warehouse == *warehouse)
            .ok_or_else(|| LakeCatError::NotFound {
                object: "warehouse",
                name: warehouse.as_str().to_string(),
            })
    }
    async fn list_warehouses(&self) -> LakeCatResult<Vec<WarehouseRecord>> {
        Ok(Vec::new())
    }
    async fn list_project_warehouses(
        &self,
        project_id: &str,
    ) -> LakeCatResult<Vec<WarehouseRecord>> {
        validate_project_id(project_id)?;
        if !self
            .list_projects()
            .await?
            .iter()
            .any(|project| project.project_id == project_id)
        {
            return Err(LakeCatError::NotFound {
                object: "project",
                name: project_id.to_string(),
            });
        }
        Ok(self
            .list_warehouses()
            .await?
            .into_iter()
            .filter(|warehouse| warehouse.project_id == project_id)
            .collect())
    }
    async fn soft_delete_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord>;
    async fn restore_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord>;
    async fn upsert_storage_profile(
        &self,
        profile: StorageProfile,
    ) -> LakeCatResult<StorageProfile> {
        profile.validate()?;
        Ok(profile)
    }
    async fn list_storage_profiles(
        &self,
        _warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<StorageProfile>> {
        Ok(Vec::new())
    }
    async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
        view.validate()?;
        Ok(view)
    }
    async fn upsert_view_if_version(
        &self,
        view: ViewRecord,
        expected_view_version: Option<u64>,
    ) -> LakeCatResult<ViewRecord> {
        if let Some(expected) = expected_view_version {
            validate_expected_view_version(expected)?;
            let current = self
                .load_view(&view.warehouse, &view.namespace, &view.name)
                .await?;
            require_expected_view_version(Some(&current), expected)?;
        }
        self.upsert_view(view).await
    }
    async fn list_view_version_receipts(
        &self,
        _warehouse: &WarehouseName,
        _namespace: &Namespace,
        _view: &TableName,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        Ok(Vec::new())
    }
    async fn list_namespace_view_version_receipts(
        &self,
        _warehouse: &WarehouseName,
        _namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        Ok(Vec::new())
    }
    async fn load_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<ViewRecord> {
        self.list_views(warehouse, namespace)
            .await?
            .into_iter()
            .find(|record| record.name == *view)
            .ok_or_else(|| LakeCatError::NotFound {
                object: "view",
                name: view.as_str().to_string(),
            })
    }
    async fn drop_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
        _principal: Principal,
    ) -> LakeCatResult<ViewRecord> {
        self.drop_view_if_version(warehouse, namespace, view, _principal, None)
            .await
    }
    async fn drop_view_if_version(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
        _principal: Principal,
        expected_view_version: Option<u64>,
    ) -> LakeCatResult<ViewRecord> {
        if let Some(expected) = expected_view_version {
            validate_expected_view_version(expected)?;
            let current = self.load_view(warehouse, namespace, view).await?;
            require_expected_view_version(Some(&current), expected)?;
        }
        let record = self.load_view(warehouse, namespace, view).await?;
        Ok(record)
    }
    async fn list_views(
        &self,
        _warehouse: &WarehouseName,
        _namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewRecord>> {
        Ok(Vec::new())
    }
    async fn upsert_policy_binding(&self, binding: PolicyBinding) -> LakeCatResult<PolicyBinding> {
        binding.validate()?;
        Ok(binding)
    }
    async fn list_policy_bindings(
        &self,
        _warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        Ok(Vec::new())
    }
    async fn policy_bindings_for_table(
        &self,
        _table: &TableIdent,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        Ok(Vec::new())
    }
    async fn storage_profile_for_table(
        &self,
        table: &TableRecord,
    ) -> LakeCatResult<StorageProfile> {
        Ok(StorageProfile::inferred_for_table(table))
    }
    async fn record_audit_event(&self, _event: CatalogAuditEvent) -> LakeCatResult<()> {
        Ok(())
    }
    async fn pending_outbox_events(
        &self,
        _sink: Option<&str>,
        _limit: usize,
    ) -> LakeCatResult<Vec<OutboxEvent>> {
        Ok(Vec::new())
    }
    async fn mark_outbox_delivered(&self, _event_ids: &[String]) -> LakeCatResult<usize> {
        Ok(0)
    }
}

mod helpers;
mod memory;
mod records;

pub use helpers::table_ident;
pub(crate) use helpers::*;
pub use memory::*;
pub use records::*;

#[cfg(test)]
mod memory_tests;

#[cfg(feature = "turso-local")]
pub mod turso_store;
