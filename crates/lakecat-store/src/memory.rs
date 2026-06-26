use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName, WarehouseName,
    content_hash_bytes, content_hash_json,
};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::*;

#[derive(Debug, Default)]
pub struct MemoryCatalogStore {
    pub(crate) state: RwLock<MemoryState>,
}

impl MemoryCatalogStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[derive(Debug, Default)]
pub(crate) struct MemoryState {
    pub(crate) servers: BTreeMap<String, ServerRecord>,
    pub(crate) projects: BTreeMap<String, ProjectRecord>,
    pub(crate) warehouses: BTreeMap<String, WarehouseRecord>,
    pub(crate) namespaces: BTreeMap<String, BTreeSet<Namespace>>,
    pub(crate) tables: BTreeMap<String, TableRecord>,
    pub(crate) commits: Vec<MemoryCommitRecord>,
    pub(crate) audit_events: Vec<CatalogAuditEvent>,
    pub(crate) outbox_events: Vec<OutboxEvent>,
    pub(crate) idempotency: BTreeMap<String, IdempotencyReplay>,
    pub(crate) storage_profiles: BTreeMap<String, StorageProfile>,
    pub(crate) views: BTreeMap<String, ViewRecord>,
    pub(crate) view_version_receipts: Vec<MemoryViewVersionReceipt>,
    pub(crate) policy_bindings: BTreeMap<String, PolicyBinding>,
    pub(crate) soft_deletes: BTreeMap<String, SoftDeleteRecord>,
}

#[derive(Debug, Clone)]
pub(crate) struct IdempotencyReplay {
    pub(crate) table_key: String,
    pub(crate) request_hash: String,
    pub(crate) response: TableRecord,
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryCommitRecord {
    pub(crate) table_key: String,
    pub(crate) record: TableCommitRecord,
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryViewVersionReceipt {
    pub(crate) view_key: String,
    pub(crate) receipt: ViewVersionReceipt,
}

#[async_trait]
impl CatalogStore for MemoryCatalogStore {
    async fn create_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: Namespace,
    ) -> LakeCatResult<()> {
        let mut state = self.state.write().await;
        state
            .namespaces
            .entry(warehouse.as_str().to_string())
            .or_default()
            .insert(namespace);
        Ok(())
    }

    async fn list_namespaces(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>> {
        let state = self.state.read().await;
        Ok(state
            .namespaces
            .get(warehouse.as_str())
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default())
    }

    async fn load_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        let state = self.state.read().await;
        state
            .namespaces
            .get(warehouse.as_str())
            .and_then(|set| set.get(namespace))
            .cloned()
            .ok_or_else(|| namespace_not_found(namespace))
    }

    async fn drop_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        let mut state = self.state.write().await;
        if !state
            .namespaces
            .get(warehouse.as_str())
            .is_some_and(|set| set.contains(namespace))
        {
            return Err(namespace_not_found(namespace));
        }
        if state
            .tables
            .iter()
            .map(|(table_key, table)| {
                validate_table_record_map_scope(table, table_key)?;
                Ok(table)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .any(|table| table.ident.warehouse == *warehouse && table.ident.namespace == *namespace)
        {
            return Err(namespace_not_empty(namespace, "tables"));
        }
        if state
            .views
            .iter()
            .map(|(view_key, view)| {
                validate_view_record_map_scope(view, view_key)?;
                Ok(view)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .any(|view| view.warehouse == *warehouse && view.namespace == *namespace)
        {
            return Err(namespace_not_empty(namespace, "views"));
        }
        if state
            .policy_bindings
            .iter()
            .map(|(binding_key, binding)| {
                validate_policy_binding_map_scope(binding, binding_key)?;
                Ok(binding)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .any(|binding| {
                binding.warehouse == *warehouse && binding.namespace.as_ref() == Some(namespace)
            })
        {
            return Err(namespace_not_empty(namespace, "policy bindings"));
        }
        let namespaces = state
            .namespaces
            .get_mut(warehouse.as_str())
            .ok_or_else(|| namespace_not_found(namespace))?;
        namespaces.remove(namespace);
        Ok(namespace.clone())
    }

    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
        let state = self.state.read().await;
        Ok(state
            .tables
            .iter()
            .map(|(table_key, table)| {
                validate_table_record_map_scope(table, table_key)?;
                Ok(table)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .filter(|table| table.ident.warehouse == *warehouse)
            .filter(|table| !state.soft_deletes.contains_key(&table_key(&table.ident)))
            .cloned()
            .collect())
    }

    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
        table.validate()?;
        let mut state = self.state.write().await;
        let warehouse = table.ident.warehouse.as_str().to_string();
        let namespace = table.ident.namespace.clone();
        state
            .namespaces
            .entry(warehouse)
            .or_default()
            .insert(namespace);

        let key = table_key(&table.ident);
        if state.tables.contains_key(&key) {
            return Err(LakeCatError::Conflict(format!(
                "table already exists: {}",
                table.ident.stable_id()
            )));
        }
        state.tables.insert(key, table.clone());
        Ok(table)
    }

    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord> {
        let state = self.state.read().await;
        state
            .tables
            .get(&table_key(ident))
            .filter(|_| !state.soft_deletes.contains_key(&table_key(ident)))
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })
            .and_then(|table| {
                validate_table_record_map_scope(&table, &table_key(ident))?;
                validate_table_record_identity(&table, ident)?;
                Ok(table)
            })
    }

    async fn commit_table(
        &self,
        ident: &TableIdent,
        commit: TableCommit,
    ) -> LakeCatResult<TableRecord> {
        commit.validate()?;
        let mut state = self.state.write().await;
        let key = table_key(ident);
        if state.soft_deletes.contains_key(&key) {
            return Err(LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            });
        }

        let request_hash = content_hash_json(&serde_json::json!({
            "requirements": &commit.requirements,
            "updates": &commit.updates,
            "expected_previous_metadata_location": &commit.expected_previous_metadata_location,
            "new_metadata_location": &commit.new_metadata_location,
            "new_metadata": &commit.new_metadata,
        }))?;
        let idempotency_request_hash = commit
            .idempotency_request_hash
            .clone()
            .unwrap_or_else(|| request_hash.clone());
        if let Some(idempotency_key) = &commit.idempotency_key {
            let idem_key = format!("{}:{idempotency_key}", ident.stable_id());
            if let Some(replay) = state.idempotency.get(&idem_key) {
                validate_idempotency_record_table_key(&replay.table_key, ident)?;
                validate_idempotency_record_request_hash(&replay.request_hash)?;
                if replay.request_hash == idempotency_request_hash {
                    validate_table_record_identity(&replay.response, ident)?;
                    return Ok(replay.response.clone());
                }
                return Err(LakeCatError::Conflict(format!(
                    "idempotency key reused with different commit request for {}",
                    ident.stable_id()
                )));
            }
        }
        let idempotency_key_sha256 = commit
            .idempotency_key
            .as_ref()
            .map(|key| content_hash_bytes(key.as_bytes()));
        let (
            previous_metadata_location,
            new_metadata_location,
            sequence_number,
            committed_at,
            table,
        ) = {
            let table = state
                .tables
                .get_mut(&key)
                .ok_or_else(|| LakeCatError::NotFound {
                    object: "table",
                    name: ident.stable_id(),
                })?;
            validate_table_record_map_scope(table, &key)?;
            validate_table_record_identity(table, ident)?;
            let previous_metadata_location = table.metadata_location.clone();
            if previous_metadata_location != commit.expected_previous_metadata_location {
                return Err(metadata_pointer_conflict(
                    ident,
                    commit.expected_previous_metadata_location.as_deref(),
                    previous_metadata_location.as_deref(),
                ));
            }
            table.metadata_location = commit.new_metadata_location.clone();
            if let Some(new_metadata) = commit.new_metadata {
                table.metadata = new_metadata;
            }
            table.version += 1;
            table.updated_at = Utc::now();
            table.metadata["lakecat:version"] = serde_json::json!(table.version);
            table.metadata["lakecat:last-request-hash"] = serde_json::json!(request_hash);
            (
                previous_metadata_location,
                commit.new_metadata_location.clone(),
                table.version,
                table.updated_at,
                table.clone(),
            )
        };

        let record = TableCommitRecord {
            table: ident.clone(),
            previous_metadata_location,
            new_metadata_location,
            sequence_number,
            principal: commit.principal.clone(),
            format_version: table_commit_format_version(&table),
            snapshot_id: table_commit_snapshot_id(&table),
            policy_hash: table_commit_policy_hash(commit.authorization_receipt.as_ref()),
            request_hash,
            response_hash: table_response_hash(&table)?,
            idempotency_key_sha256,
            committed_at,
        };
        let replay_request_hash = record.request_hash.clone();
        let audit_payload = serde_json::json!({
            "event-type": "table.commit",
            "table": ident.clone(),
            "commit": &record,
            "authorization-receipt": commit.authorization_receipt,
        });
        let audit_payload_hash = content_hash_json(&audit_payload)?;
        let outbox_payload = serde_json::json!({
            "audit-event-id": audit_payload_hash,
            "event-type": "table.commit",
            "table": ident.clone(),
            "commit": &record,
            "authorization-receipt": audit_payload["authorization-receipt"].clone(),
        });
        let outbox_event = outbox_event_from_payload(&outbox_payload, committed_at)?;
        state.audit_events.push(CatalogAuditEvent {
            event_type: "table.commit".to_string(),
            table: Some(ident.clone()),
            principal: commit.principal.clone(),
            request_hash: Some(audit_payload_hash),
            payload: audit_payload,
            created_at: committed_at,
        });
        state.outbox_events.push(outbox_event);
        state.commits.push(MemoryCommitRecord {
            table_key: table_key(ident),
            record,
        });

        if let Some(idempotency_key) = commit.idempotency_key {
            state.idempotency.insert(
                format!("{}:{idempotency_key}", ident.stable_id()),
                IdempotencyReplay {
                    table_key: table_key(ident),
                    request_hash: commit
                        .idempotency_request_hash
                        .unwrap_or(replay_request_hash),
                    response: table.clone(),
                },
            );
        }
        Ok(table)
    }

    async fn replay_table_commit(
        &self,
        ident: &TableIdent,
        idempotency_key: &str,
        idempotency_request_hash: &str,
    ) -> LakeCatResult<Option<TableRecord>> {
        validate_idempotency_key_shape(idempotency_key)?;
        validate_idempotency_request_hash_shape(idempotency_request_hash)?;
        let state = self.state.read().await;
        let idem_key = format!("{}:{idempotency_key}", ident.stable_id());
        let Some(replay) = state.idempotency.get(&idem_key) else {
            return Ok(None);
        };
        validate_idempotency_record_table_key(&replay.table_key, ident)?;
        validate_idempotency_record_request_hash(&replay.request_hash)?;
        if replay.request_hash != idempotency_request_hash {
            return Err(LakeCatError::Conflict(format!(
                "idempotency key reused with different commit request for {}",
                ident.stable_id()
            )));
        }
        validate_table_record_identity(&replay.response, ident)?;
        Ok(Some(replay.response.clone()))
    }

    async fn table_commit_records(
        &self,
        ident: &TableIdent,
        start_version: u64,
        end_version: Option<u64>,
    ) -> LakeCatResult<Vec<TableCommitRecord>> {
        let state = self.state.read().await;
        let key = table_key(ident);
        state
            .commits
            .iter()
            .filter(|commit| commit.table_key == key)
            .filter(|commit| commit.record.sequence_number >= start_version)
            .filter(|commit| end_version.is_none_or(|end| commit.record.sequence_number <= end))
            .map(|commit| {
                validate_table_commit_record_memory_scope(commit, ident)?;
                Ok(commit.record.clone())
            })
            .collect()
    }

    async fn upsert_server(&self, server: ServerRecord) -> LakeCatResult<ServerRecord> {
        server.validate()?;
        let mut state = self.state.write().await;
        if let Some(existing) = state.servers.get(&server.server_id) {
            validate_server_record_map_scope(existing, &server.server_id)?;
        }
        state
            .servers
            .insert(server.server_id.clone(), server.clone());
        Ok(server)
    }

    async fn list_servers(&self) -> LakeCatResult<Vec<ServerRecord>> {
        let state = self.state.read().await;
        let mut servers = state
            .servers
            .iter()
            .map(|(server_id, server)| {
                validate_server_record_map_scope(server, server_id)?;
                Ok(server.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        servers.sort_by(|left, right| left.server_id.cmp(&right.server_id));
        Ok(servers)
    }

    async fn upsert_project(&self, project: ProjectRecord) -> LakeCatResult<ProjectRecord> {
        project.validate()?;
        let mut state = self.state.write().await;
        if let Some(server_id) = project.server_id.as_deref() {
            let Some(server) = state.servers.get(server_id) else {
                return Err(LakeCatError::NotFound {
                    object: "server",
                    name: server_id.to_string(),
                });
            };
            validate_server_record_map_scope(server, server_id)?;
        }
        if let Some(existing) = state.projects.get(&project.project_id) {
            validate_project_record_map_scope(existing, &project.project_id)?;
        }
        state
            .projects
            .insert(project.project_id.clone(), project.clone());
        Ok(project)
    }

    async fn list_projects(&self) -> LakeCatResult<Vec<ProjectRecord>> {
        let state = self.state.read().await;
        let mut projects = state
            .projects
            .iter()
            .map(|(project_id, project)| {
                validate_project_record_map_scope(project, project_id)?;
                Ok(project.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        projects.sort_by(|left, right| left.project_id.cmp(&right.project_id));
        Ok(projects)
    }

    async fn upsert_warehouse(&self, warehouse: WarehouseRecord) -> LakeCatResult<WarehouseRecord> {
        warehouse.validate()?;
        let mut state = self.state.write().await;
        let Some(project) = state.projects.get(&warehouse.project_id) else {
            return Err(LakeCatError::NotFound {
                object: "project",
                name: warehouse.project_id.clone(),
            });
        };
        validate_project_record_map_scope(project, &warehouse.project_id)?;
        let warehouse_key = warehouse.warehouse.as_str().to_string();
        if let Some(existing) = state.warehouses.get(&warehouse_key) {
            validate_warehouse_record_map_scope(existing, &warehouse_key)?;
        }
        state.warehouses.insert(warehouse_key, warehouse.clone());
        Ok(warehouse)
    }

    async fn load_warehouse(&self, warehouse: &WarehouseName) -> LakeCatResult<WarehouseRecord> {
        let state = self.state.read().await;
        let warehouse_key = warehouse.as_str().to_string();
        let warehouse = state
            .warehouses
            .get(warehouse_key.as_str())
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "warehouse",
                name: warehouse.as_str().to_string(),
            })?;
        validate_warehouse_record_map_scope(&warehouse, warehouse_key.as_str())?;
        Ok(warehouse)
    }

    async fn list_warehouses(&self) -> LakeCatResult<Vec<WarehouseRecord>> {
        let state = self.state.read().await;
        let mut warehouses = state
            .warehouses
            .iter()
            .map(|(warehouse_key, warehouse)| {
                validate_warehouse_record_map_scope(warehouse, warehouse_key)?;
                Ok(warehouse.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        warehouses.sort_by(|left, right| left.warehouse.as_str().cmp(right.warehouse.as_str()));
        Ok(warehouses)
    }

    async fn list_project_warehouses(
        &self,
        project_id: &str,
    ) -> LakeCatResult<Vec<WarehouseRecord>> {
        validate_project_id(project_id)?;
        let state = self.state.read().await;
        let project = state
            .projects
            .get(project_id)
            .ok_or_else(|| LakeCatError::NotFound {
                object: "project",
                name: project_id.to_string(),
            })?;
        validate_project_record_map_scope(project, project_id)?;
        let mut warehouses = state
            .warehouses
            .iter()
            .map(|(warehouse_key, warehouse)| {
                validate_warehouse_record_map_scope(warehouse, warehouse_key)?;
                Ok(warehouse)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .filter(|warehouse| warehouse.project_id == project_id)
            .cloned()
            .collect::<Vec<_>>();
        warehouses.sort_by(|left, right| left.warehouse.as_str().cmp(right.warehouse.as_str()));
        Ok(warehouses)
    }

    async fn soft_delete_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord> {
        let mut state = self.state.write().await;
        let key = table_key(ident);
        if state.soft_deletes.contains_key(&key) {
            return Err(LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            });
        }
        let table = state
            .tables
            .get(&key)
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })?;
        validate_table_record_map_scope(&table, &key)?;
        validate_table_record_identity(&table, ident)?;
        let record = SoftDeleteRecord {
            table: ident.clone(),
            metadata_location: table.metadata_location.clone(),
            version: table.version,
            format_version: table_commit_format_version(&table),
            principal,
            authorization_receipt,
            deleted_at: Utc::now(),
        };
        record.validate_for_table(ident, &table)?;
        let audit_payload = serde_json::json!({
            "event-type": "table.deleted",
            "table": ident,
            "soft-delete": &record,
            "authorization-receipt": &record.authorization_receipt,
        });
        let audit_payload_hash = content_hash_json(&audit_payload)?;
        let outbox_payload = serde_json::json!({
            "audit-event-id": audit_payload_hash,
            "event-type": "table.deleted",
            "table": ident,
            "soft-delete": audit_payload["soft-delete"].clone(),
            "authorization-receipt": audit_payload["authorization-receipt"].clone(),
        });
        let outbox_event = outbox_event_from_payload(&outbox_payload, record.deleted_at)?;
        let audit_principal = record.principal.clone();
        state.soft_deletes.insert(key, record);
        state.audit_events.push(CatalogAuditEvent {
            event_type: "table.deleted".to_string(),
            table: Some(ident.clone()),
            principal: audit_principal,
            request_hash: Some(audit_payload_hash),
            payload: audit_payload,
            created_at: outbox_event.created_at,
        });
        state.outbox_events.push(outbox_event);
        Ok(table)
    }

    async fn restore_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord> {
        let mut state = self.state.write().await;
        let key = table_key(ident);
        let Some(record) = state.soft_deletes.get(&key) else {
            return Err(LakeCatError::NotFound {
                object: "soft-deleted table",
                name: ident.stable_id(),
            });
        };
        validate_soft_delete_record_map_scope(record, &key)?;
        let table = state
            .tables
            .get(&key)
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })?;
        validate_table_record_map_scope(&table, &key)?;
        validate_table_record_identity(&table, ident)?;
        record.validate_for_table(ident, &table)?;
        let restored_at = Utc::now();
        let audit_payload = serde_json::json!({
            "event-type": "table.restored",
            "table": ident,
            "authorization-receipt": authorization_receipt,
            "metadata-location": table.metadata_location,
            "format-version": table_commit_format_version(&table),
            "version": table.version,
        });
        let audit_payload_hash = content_hash_json(&audit_payload)?;
        let outbox_payload = serde_json::json!({
            "audit-event-id": audit_payload_hash,
            "event-type": "table.restored",
            "table": ident,
            "payload": audit_payload.clone(),
            "authorization-receipt": audit_payload["authorization-receipt"].clone(),
        });
        let outbox_event = outbox_event_from_payload(&outbox_payload, restored_at)?;
        state.soft_deletes.remove(&key);
        state.audit_events.push(CatalogAuditEvent {
            event_type: "table.restored".to_string(),
            table: Some(ident.clone()),
            principal,
            request_hash: Some(audit_payload_hash),
            payload: audit_payload,
            created_at: restored_at,
        });
        state.outbox_events.push(outbox_event);
        Ok(table)
    }

    async fn upsert_storage_profile(
        &self,
        profile: StorageProfile,
    ) -> LakeCatResult<StorageProfile> {
        profile.validate()?;
        let mut state = self.state.write().await;
        let key = storage_profile_key(&profile.warehouse, &profile.profile_id);
        if let Some(existing) = state.storage_profiles.get(&key) {
            validate_storage_profile_map_scope(existing, &key)?;
        }
        state.storage_profiles.insert(key, profile.clone());
        Ok(profile)
    }

    async fn list_storage_profiles(
        &self,
        warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<StorageProfile>> {
        let state = self.state.read().await;
        let mut profiles = state
            .storage_profiles
            .iter()
            .map(|(profile_key, profile)| {
                validate_storage_profile_map_scope(profile, profile_key)?;
                Ok(profile)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .filter(|profile| profile.warehouse == *warehouse)
            .cloned()
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
        Ok(profiles)
    }

    async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
        view.validate()?;
        let mut state = self.state.write().await;
        let view_key = view_key(&view);
        let principal = view.created.principal.clone();
        let previous = state.views.get(&view_key);
        if let Some(previous) = previous {
            validate_view_record_map_scope(previous, &view_key)?;
        }
        let latest_receipt = latest_view_receipt_evidence(
            state
                .view_version_receipts
                .iter()
                .filter(|receipt| receipt.view_key == view_key)
                .map(|receipt| {
                    validate_memory_view_receipt_scope(
                        receipt,
                        &view.warehouse,
                        &view.namespace,
                        Some(&view.name),
                    )?;
                    Ok(&receipt.receipt)
                })
                .collect::<LakeCatResult<Vec<_>>>()?
                .into_iter(),
        )?;
        let latest_receipt_version = latest_receipt
            .as_ref()
            .map(|(view_version, _)| *view_version);
        let previous_receipt_hash = latest_receipt.map(|(_, receipt_hash)| receipt_hash);
        let previous_view_version = previous
            .map(|view| view.view_version)
            .or(latest_receipt_version);
        let view = view.with_next_version_after_history(previous, latest_receipt_version)?;
        let receipt = ViewVersionReceipt::upsert(
            previous_view_version,
            previous_receipt_hash,
            &view,
            principal,
        )?;
        state.views.insert(view_key.clone(), view.clone());
        state
            .view_version_receipts
            .push(MemoryViewVersionReceipt { view_key, receipt });
        Ok(view)
    }

    async fn upsert_view_if_version(
        &self,
        view: ViewRecord,
        expected_view_version: Option<u64>,
    ) -> LakeCatResult<ViewRecord> {
        view.validate()?;
        if let Some(expected) = expected_view_version {
            validate_expected_view_version(expected)?;
        }
        let mut state = self.state.write().await;
        let view_key = view_key(&view);
        let principal = view.created.principal.clone();
        let previous = state.views.get(&view_key);
        if let Some(previous) = previous {
            validate_view_record_map_scope(previous, &view_key)?;
        }
        if let Some(expected) = expected_view_version {
            require_expected_view_version(previous, expected)?;
        }
        let latest_receipt = latest_view_receipt_evidence(
            state
                .view_version_receipts
                .iter()
                .filter(|receipt| receipt.view_key == view_key)
                .map(|receipt| {
                    validate_memory_view_receipt_scope(
                        receipt,
                        &view.warehouse,
                        &view.namespace,
                        Some(&view.name),
                    )?;
                    Ok(&receipt.receipt)
                })
                .collect::<LakeCatResult<Vec<_>>>()?
                .into_iter(),
        )?;
        let latest_receipt_version = latest_receipt
            .as_ref()
            .map(|(view_version, _)| *view_version);
        let previous_receipt_hash = latest_receipt.map(|(_, receipt_hash)| receipt_hash);
        let previous_view_version = previous
            .map(|view| view.view_version)
            .or(latest_receipt_version);
        let view = view.with_next_version_after_history(previous, latest_receipt_version)?;
        let receipt = ViewVersionReceipt::upsert(
            previous_view_version,
            previous_receipt_hash,
            &view,
            principal,
        )?;
        state.views.insert(view_key.clone(), view.clone());
        state
            .view_version_receipts
            .push(MemoryViewVersionReceipt { view_key, receipt });
        Ok(view)
    }

    async fn list_view_version_receipts(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        let state = self.state.read().await;
        let receipts = state
            .view_version_receipts
            .iter()
            .filter(|receipt| receipt.view_key == view_key_parts(warehouse, namespace, view))
            .map(|receipt| {
                validate_memory_view_receipt_scope(receipt, warehouse, namespace, Some(view))?;
                Ok(receipt.receipt.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        validate_view_receipt_chains(&receipts)?;
        Ok(receipts)
    }

    async fn list_namespace_view_version_receipts(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        let state = self.state.read().await;
        let receipts = state
            .view_version_receipts
            .iter()
            .filter(|receipt| {
                memory_view_receipt_key_matches_namespace(&receipt.view_key, warehouse, namespace)
            })
            .map(|receipt| {
                validate_memory_view_receipt_scope(receipt, warehouse, namespace, None)?;
                Ok(receipt.receipt.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        validate_view_receipt_chains(&receipts)?;
        Ok(receipts)
    }

    async fn load_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<ViewRecord> {
        let state = self.state.read().await;
        state
            .views
            .get(&view_key_parts(warehouse, namespace, view))
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "view",
                name: view.as_str().to_string(),
            })
            .and_then(|record| {
                validate_view_record_map_scope(
                    &record,
                    &view_key_parts(warehouse, namespace, view),
                )?;
                validate_view_record_scope(&record, warehouse, namespace, view)?;
                Ok(record)
            })
    }

    async fn drop_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
        principal: Principal,
    ) -> LakeCatResult<ViewRecord> {
        self.drop_view_if_version(warehouse, namespace, view, principal, None)
            .await
    }

    async fn drop_view_if_version(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
        principal: Principal,
        expected_view_version: Option<u64>,
    ) -> LakeCatResult<ViewRecord> {
        if let Some(expected) = expected_view_version {
            validate_expected_view_version(expected)?;
        }
        let mut state = self.state.write().await;
        let view_key = view_key_parts(warehouse, namespace, view);
        let current = state
            .views
            .get(&view_key)
            .ok_or_else(|| LakeCatError::NotFound {
                object: "view",
                name: view.as_str().to_string(),
            })?;
        validate_view_record_map_scope(current, &view_key)?;
        validate_view_record_scope(current, warehouse, namespace, view)?;
        if let Some(expected) = expected_view_version {
            require_expected_view_version(Some(current), expected)?;
        }
        let record = state.views.remove(&view_key).ok_or_else(|| {
            LakeCatError::Internal("view disappeared during guarded drop".to_string())
        })?;
        let previous_receipt_hash = latest_view_receipt_hash(
            state
                .view_version_receipts
                .iter()
                .filter(|receipt| receipt.view_key == view_key)
                .map(|receipt| {
                    validate_memory_view_receipt_scope(receipt, warehouse, namespace, Some(view))?;
                    Ok(&receipt.receipt)
                })
                .collect::<LakeCatResult<Vec<_>>>()?
                .into_iter(),
        )?;
        let receipt = ViewVersionReceipt::drop(&record, previous_receipt_hash, principal)?;
        state
            .view_version_receipts
            .push(MemoryViewVersionReceipt { view_key, receipt });
        Ok(record)
    }

    async fn list_views(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewRecord>> {
        let state = self.state.read().await;
        let mut views = state
            .views
            .iter()
            .map(|(view_key, view)| {
                validate_view_record_map_scope(view, view_key)?;
                Ok(view)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .filter(|view| view.warehouse == *warehouse && view.namespace == *namespace)
            .cloned()
            .collect::<Vec<_>>();
        views.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
        Ok(views)
    }

    async fn storage_profile_for_table(
        &self,
        table: &TableRecord,
    ) -> LakeCatResult<StorageProfile> {
        let state = self.state.read().await;
        let profiles = state
            .storage_profiles
            .iter()
            .map(|(profile_key, profile)| {
                validate_storage_profile_map_scope(profile, profile_key)?;
                Ok(profile)
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        Ok(storage_profile_match(profiles.into_iter(), table)?
            .unwrap_or_else(|| StorageProfile::inferred_for_table(table)))
    }

    async fn upsert_policy_binding(&self, binding: PolicyBinding) -> LakeCatResult<PolicyBinding> {
        binding.validate()?;
        let mut state = self.state.write().await;
        let key = policy_binding_key(&binding.warehouse, &binding.policy_id);
        if let Some(existing) = state.policy_bindings.get(&key) {
            validate_policy_binding_map_scope(existing, &key)?;
        }
        state.policy_bindings.insert(key, binding.clone());
        Ok(binding)
    }

    async fn list_policy_bindings(
        &self,
        warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        let state = self.state.read().await;
        let mut bindings = state
            .policy_bindings
            .iter()
            .map(|(binding_key, binding)| {
                validate_policy_binding_map_scope(binding, binding_key)?;
                Ok(binding)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .filter(|binding| binding.warehouse == *warehouse)
            .cloned()
            .collect::<Vec<_>>();
        bindings.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));
        Ok(bindings)
    }

    async fn policy_bindings_for_table(
        &self,
        table: &TableIdent,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        let state = self.state.read().await;
        let bindings = state
            .policy_bindings
            .iter()
            .map(|(binding_key, binding)| {
                validate_policy_binding_map_scope(binding, binding_key)?;
                Ok(binding)
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        Ok(policy_bindings_for_table(bindings.into_iter(), table))
    }

    async fn record_audit_event(&self, event: CatalogAuditEvent) -> LakeCatResult<()> {
        event.validate_recordable()?;
        let event_id = audit_event_id(&event)?;
        let outbox_payload = audit_outbox_payload(&event_id, &event);
        let outbox_event = outbox_event_from_payload(&outbox_payload, event.created_at)?;
        let mut state = self.state.write().await;
        if state
            .outbox_events
            .iter()
            .any(|candidate| candidate.event_id == outbox_event.event_id)
        {
            return Err(LakeCatError::Internal(
                "duplicate audit event id would duplicate outbox replay evidence".to_string(),
            ));
        }
        state.audit_events.push(event);
        state.outbox_events.push(outbox_event);
        Ok(())
    }

    async fn pending_outbox_events(
        &self,
        sink: Option<&str>,
        limit: usize,
    ) -> LakeCatResult<Vec<OutboxEvent>> {
        let state = self.state.read().await;
        let mut events = state
            .outbox_events
            .iter()
            .filter(|event| event.delivered_at.is_none())
            .filter(|event| sink.is_none_or(|sink| event.sink == sink))
            .cloned()
            .collect::<Vec<_>>();
        events.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        events.truncate(limit);
        for event in &events {
            event.validate_pending()?;
        }
        Ok(events)
    }

    async fn mark_outbox_delivered(&self, event_ids: &[String]) -> LakeCatResult<usize> {
        if event_ids.is_empty() {
            return Ok(0);
        }
        for event_id in event_ids {
            validate_outbox_event_id_shape(event_id)?;
        }
        let event_ids = event_ids.iter().collect::<BTreeSet<_>>();
        let mut state = self.state.write().await;
        let delivered_at = Utc::now();
        for event in &state.outbox_events {
            if event.delivered_at.is_none() && event_ids.contains(&event.event_id) {
                event.validate_pending()?;
            }
        }
        let mut delivered = 0usize;
        for event in &mut state.outbox_events {
            if event.delivered_at.is_none() && event_ids.contains(&event.event_id) {
                event.delivered_at = Some(delivered_at);
                delivered += 1;
            }
        }
        Ok(delivered)
    }
}
