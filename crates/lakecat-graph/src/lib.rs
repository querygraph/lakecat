use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, WarehouseName};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait CatalogGraphSink: Send + Sync + 'static {
    async fn emit(&self, event: GraphEvent) -> LakeCatResult<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphEvent {
    pub event_id: Option<String>,
    pub subject: String,
    pub label: GraphNodeLabel,
    pub action: GraphAction,
    pub table: Option<TableIdent>,
    pub properties: Value,
    pub emitted_at: DateTime<Utc>,
}

impl GraphEvent {
    pub fn server(action: GraphAction, server_id: impl Into<String>, properties: Value) -> Self {
        let server_id = server_id.into();
        Self {
            event_id: None,
            subject: server_stable_id(&server_id),
            label: GraphNodeLabel::Server,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn project(action: GraphAction, project_id: impl Into<String>, properties: Value) -> Self {
        let project_id = project_id.into();
        Self {
            event_id: None,
            subject: project_stable_id(&project_id),
            label: GraphNodeLabel::Project,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn table(action: GraphAction, table: TableIdent, properties: Value) -> Self {
        Self {
            event_id: None,
            subject: table.stable_id(),
            label: GraphNodeLabel::Table,
            action,
            table: Some(table),
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn namespace(
        action: GraphAction,
        warehouse: WarehouseName,
        namespace: Namespace,
        properties: Value,
    ) -> Self {
        Self {
            event_id: None,
            subject: namespace_stable_id(&warehouse, &namespace),
            label: GraphNodeLabel::Namespace,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn warehouse(action: GraphAction, warehouse: WarehouseName, properties: Value) -> Self {
        Self {
            event_id: None,
            subject: warehouse_stable_id(&warehouse),
            label: GraphNodeLabel::Warehouse,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn view(
        action: GraphAction,
        warehouse: WarehouseName,
        namespace: Namespace,
        name: impl Into<String>,
        properties: Value,
    ) -> Self {
        let name = name.into();
        Self {
            event_id: None,
            subject: view_stable_id(&warehouse, &namespace, &name),
            label: GraphNodeLabel::View,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn policy(
        action: GraphAction,
        warehouse: WarehouseName,
        policy_id: impl Into<String>,
        properties: Value,
    ) -> Self {
        let policy_id = policy_id.into();
        Self {
            event_id: None,
            subject: policy_stable_id(&warehouse, &policy_id),
            label: GraphNodeLabel::Policy,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn storage_profile(
        action: GraphAction,
        warehouse: WarehouseName,
        profile_id: impl Into<String>,
        properties: Value,
    ) -> Self {
        let profile_id = profile_id.into();
        Self {
            event_id: None,
            subject: storage_profile_stable_id(&warehouse, &profile_id),
            label: GraphNodeLabel::StorageProfile,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn scan_plan(action: GraphAction, plan_id: impl Into<String>, properties: Value) -> Self {
        let plan_id = plan_id.into();
        Self {
            event_id: None,
            subject: scan_plan_stable_id(&plan_id),
            label: GraphNodeLabel::ScanPlan,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn commit(
        action: GraphAction,
        table: &TableIdent,
        sequence_number: u64,
        properties: Value,
    ) -> Self {
        Self {
            event_id: None,
            subject: commit_stable_id(table, sequence_number),
            label: GraphNodeLabel::Commit,
            action,
            table: Some(table.clone()),
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn column(
        action: GraphAction,
        table: &TableIdent,
        column_id: impl Into<String>,
        properties: Value,
    ) -> Self {
        let column_id = column_id.into();
        Self {
            event_id: None,
            subject: column_stable_id(table, &column_id),
            label: GraphNodeLabel::Column,
            action,
            table: Some(table.clone()),
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn snapshot(
        action: GraphAction,
        table: &TableIdent,
        snapshot_id: impl Into<String>,
        properties: Value,
    ) -> Self {
        let snapshot_id = snapshot_id.into();
        Self {
            event_id: None,
            subject: snapshot_stable_id(table, &snapshot_id),
            label: GraphNodeLabel::Snapshot,
            action,
            table: Some(table.clone()),
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn principal(action: GraphAction, principal: &Principal, properties: Value) -> Self {
        Self {
            event_id: None,
            subject: principal_stable_id(principal),
            label: GraphNodeLabel::Principal,
            action,
            table: None,
            properties,
            emitted_at: Utc::now(),
        }
    }

    pub fn with_event_id(mut self, event_id: impl Into<String>) -> Self {
        self.event_id = Some(event_id.into());
        self
    }

    pub fn validate(&self) -> LakeCatResult<()> {
        if self.subject.trim().is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "catalog graph event subject must not be blank".to_string(),
            ));
        }
        if let Some(event_id) = self.event_id.as_deref()
            && event_id.trim().is_empty()
        {
            return Err(LakeCatError::InvalidArgument(
                "catalog graph event id must not be blank".to_string(),
            ));
        }
        if !self.properties.is_object() {
            return Err(LakeCatError::InvalidArgument(
                "catalog graph event properties must be a JSON object".to_string(),
            ));
        }
        if self.label.requires_table() && self.table.is_none() {
            return Err(LakeCatError::InvalidArgument(format!(
                "catalog graph event label {} requires table identity",
                self.label.as_str()
            )));
        }
        Ok(())
    }
}

pub fn server_stable_id(server_id: &str) -> String {
    format!("lakecat:server:{server_id}")
}

pub fn project_stable_id(project_id: &str) -> String {
    format!("lakecat:project:{project_id}")
}

pub fn warehouse_stable_id(warehouse: &WarehouseName) -> String {
    format!("lakecat:warehouse:{}", warehouse.as_str())
}

pub fn namespace_stable_id(warehouse: &WarehouseName, namespace: &Namespace) -> String {
    format!(
        "lakecat:warehouse:{}:namespace:{}",
        warehouse.as_str(),
        namespace.path()
    )
}

pub fn view_stable_id(warehouse: &WarehouseName, namespace: &Namespace, name: &str) -> String {
    format!(
        "{}:view:{}",
        namespace_stable_id(warehouse, namespace),
        name
    )
}

pub fn policy_stable_id(warehouse: &WarehouseName, policy_id: &str) -> String {
    format!(
        "lakecat:warehouse:{}:policy:{}",
        warehouse.as_str(),
        policy_id
    )
}

pub fn storage_profile_stable_id(warehouse: &WarehouseName, profile_id: &str) -> String {
    format!(
        "lakecat:warehouse:{}:storage-profile:{}",
        warehouse.as_str(),
        profile_id
    )
}

pub fn scan_plan_stable_id(plan_id: &str) -> String {
    format!("lakecat:scan-plan:{plan_id}")
}

pub fn commit_stable_id(table: &TableIdent, sequence_number: u64) -> String {
    format!("lakecat:commit:{}:{sequence_number}", table.stable_id())
}

pub fn column_stable_id(table: &TableIdent, column_id: &str) -> String {
    format!("lakecat:column:{}:{column_id}", table.stable_id())
}

pub fn snapshot_stable_id(table: &TableIdent, snapshot_id: &str) -> String {
    format!("lakecat:snapshot:{}:{snapshot_id}", table.stable_id())
}

pub fn principal_stable_id(principal: &Principal) -> String {
    format!("lakecat:principal:{}", principal.subject)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GraphNodeLabel {
    Server,
    Project,
    Warehouse,
    Namespace,
    Table,
    View,
    Column,
    Snapshot,
    Manifest,
    DataFile,
    DeleteFile,
    Policy,
    StorageProfile,
    Principal,
    ScanPlan,
    Commit,
    LineageRun,
    QueryGraphModel,
}

impl GraphNodeLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            GraphNodeLabel::Server => "Server",
            GraphNodeLabel::Project => "Project",
            GraphNodeLabel::Warehouse => "Warehouse",
            GraphNodeLabel::Namespace => "Namespace",
            GraphNodeLabel::Table => "Table",
            GraphNodeLabel::View => "View",
            GraphNodeLabel::Column => "Column",
            GraphNodeLabel::Snapshot => "Snapshot",
            GraphNodeLabel::Manifest => "Manifest",
            GraphNodeLabel::DataFile => "DataFile",
            GraphNodeLabel::DeleteFile => "DeleteFile",
            GraphNodeLabel::Policy => "Policy",
            GraphNodeLabel::StorageProfile => "StorageProfile",
            GraphNodeLabel::Principal => "Principal",
            GraphNodeLabel::ScanPlan => "ScanPlan",
            GraphNodeLabel::Commit => "Commit",
            GraphNodeLabel::LineageRun => "LineageRun",
            GraphNodeLabel::QueryGraphModel => "QueryGraphModel",
        }
    }

    fn requires_table(&self) -> bool {
        matches!(
            self,
            GraphNodeLabel::Table
                | GraphNodeLabel::Column
                | GraphNodeLabel::Snapshot
                | GraphNodeLabel::Manifest
                | GraphNodeLabel::DataFile
                | GraphNodeLabel::DeleteFile
                | GraphNodeLabel::Commit
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GraphAction {
    Created,
    Upserted,
    Loaded,
    PlannedScan,
    Committed,
    Deleted,
}

#[derive(Debug, Default)]
pub struct NoopCatalogGraphSink;

impl NoopCatalogGraphSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl CatalogGraphSink for NoopCatalogGraphSink {
    async fn emit(&self, event: GraphEvent) -> LakeCatResult<()> {
        event.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(feature = "grust-local")]
pub mod grust_integration;
