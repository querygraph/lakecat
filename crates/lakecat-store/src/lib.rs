use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{
    AuditStamp, LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName,
    WarehouseName, content_hash_json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

#[async_trait]
pub trait CatalogStore: Send + Sync + 'static {
    async fn create_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: Namespace,
    ) -> LakeCatResult<()>;
    async fn list_namespaces(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>>;
    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>>;
    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord>;
    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord>;
    async fn commit_table(
        &self,
        ident: &TableIdent,
        commit: TableCommit,
    ) -> LakeCatResult<TableRecord>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableRecord {
    pub ident: TableIdent,
    pub location: String,
    pub metadata_location: Option<String>,
    pub metadata: Value,
    pub created: AuditStamp,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
}

impl TableRecord {
    pub fn new(
        ident: TableIdent,
        location: String,
        metadata_location: Option<String>,
        metadata: Value,
        principal: Principal,
    ) -> Self {
        let created = AuditStamp::now(principal);
        Self {
            ident,
            location,
            metadata_location,
            metadata,
            updated_at: created.at,
            created,
            version: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableCommit {
    pub requirements: Vec<Value>,
    pub updates: Vec<Value>,
    pub idempotency_key: Option<String>,
    pub principal: Principal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableCommitRecord {
    pub table: TableIdent,
    pub previous_metadata_location: Option<String>,
    pub new_metadata_location: Option<String>,
    pub sequence_number: u64,
    pub principal: Principal,
    pub request_hash: String,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct MemoryCatalogStore {
    state: RwLock<MemoryState>,
}

impl MemoryCatalogStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[derive(Debug, Default)]
struct MemoryState {
    namespaces: BTreeMap<String, BTreeSet<Namespace>>,
    tables: BTreeMap<String, TableRecord>,
    commits: Vec<TableCommitRecord>,
    idempotency: BTreeMap<String, TableRecord>,
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

    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
        let state = self.state.read().await;
        Ok(state
            .tables
            .values()
            .filter(|table| table.ident.warehouse == *warehouse)
            .cloned()
            .collect())
    }

    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
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
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })
    }

    async fn commit_table(
        &self,
        ident: &TableIdent,
        commit: TableCommit,
    ) -> LakeCatResult<TableRecord> {
        let mut state = self.state.write().await;
        let key = table_key(ident);
        if let Some(idempotency_key) = &commit.idempotency_key {
            let idem_key = format!("{}:{idempotency_key}", ident.stable_id());
            if let Some(record) = state.idempotency.get(&idem_key) {
                return Ok(record.clone());
            }
        }

        let request_hash = content_hash_json(&serde_json::json!({
            "requirements": commit.requirements,
            "updates": commit.updates,
        }))?;
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
            let previous_metadata_location = table.metadata_location.clone();
            table.version += 1;
            table.updated_at = Utc::now();
            table.metadata["lakecat:version"] = serde_json::json!(table.version);
            table.metadata["lakecat:last-request-hash"] = serde_json::json!(request_hash);
            (
                previous_metadata_location,
                table.metadata_location.clone(),
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
            request_hash,
            committed_at,
        };
        state.commits.push(record);

        if let Some(idempotency_key) = commit.idempotency_key {
            state.idempotency.insert(
                format!("{}:{idempotency_key}", ident.stable_id()),
                table.clone(),
            );
        }
        Ok(table)
    }
}

pub fn table_ident(
    warehouse: impl Into<String>,
    namespace: impl AsRef<str>,
    table: impl Into<String>,
) -> LakeCatResult<TableIdent> {
    Ok(TableIdent::new(
        WarehouseName::new(warehouse.into())?,
        namespace.as_ref().parse()?,
        TableName::new(table.into())?,
    ))
}

fn table_key(ident: &TableIdent) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        ident.warehouse, ident.namespace, ident.name
    )
}

#[cfg(feature = "turso-local")]
pub mod turso_store {
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use lakecat_core::{
        LakeCatError, LakeCatResult, Namespace, TableIdent, WarehouseName, content_hash_json,
    };
    use serde::de::DeserializeOwned;
    use turso::{Connection, Database, Row, Value};

    use crate::{CatalogStore, TableCommit, TableCommitRecord, TableRecord, table_key};

    #[derive(Debug, Clone)]
    pub struct TursoCatalogStore {
        db: Database,
    }

    impl TursoCatalogStore {
        pub async fn connect_local(path: &str) -> LakeCatResult<Arc<Self>> {
            let db = turso::Builder::new_local(path)
                .build()
                .await
                .map_err(turso_error)?;
            Self::from_database(db).await
        }

        pub async fn in_memory() -> LakeCatResult<Arc<Self>> {
            let db = turso::Builder::new_local(":memory:")
                .build()
                .await
                .map_err(turso_error)?;
            Self::from_database(db).await
        }

        pub async fn from_database(db: Database) -> LakeCatResult<Arc<Self>> {
            let store = Arc::new(Self { db });
            store.migrate().await?;
            Ok(store)
        }

        pub fn database(&self) -> &Database {
            &self.db
        }

        async fn migrate(&self) -> LakeCatResult<()> {
            let conn = self.connect()?;
            conn.execute_batch(TURSO_MIGRATION.join(";\n"))
                .await
                .map_err(turso_error)?;
            Ok(())
        }

        fn connect(&self) -> LakeCatResult<Connection> {
            self.db.connect().map_err(turso_error)
        }

        #[cfg(test)]
        async fn count_rows(&self, table: &str) -> LakeCatResult<i64> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(format!("select count(*) from {table}"), ())
                .await
                .map_err(turso_error)?;
            let row = rows.next().await.map_err(turso_error)?.ok_or_else(|| {
                LakeCatError::Internal(format!("Turso catalog store returned no count for {table}"))
            })?;
            row_i64(&row, 0)
        }
    }

    #[async_trait]
    impl CatalogStore for TursoCatalogStore {
        async fn create_namespace(
            &self,
            warehouse: &WarehouseName,
            namespace: Namespace,
        ) -> LakeCatResult<()> {
            let conn = self.connect()?;
            conn.execute(
                "insert or ignore into namespaces (warehouse, namespace_path, namespace_json)
                 values (?1, ?2, ?3)",
                (
                    warehouse.as_str(),
                    namespace.path(),
                    encode_json(namespace.parts())?,
                ),
            )
            .await
            .map_err(turso_error)?;
            Ok(())
        }

        async fn list_namespaces(
            &self,
            warehouse: &WarehouseName,
        ) -> LakeCatResult<Vec<Namespace>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select namespace_json from namespaces
                     where warehouse = ?1
                     order by namespace_path",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut namespaces = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                namespaces.push(decode_namespace(row_string(&row, 0)?)?);
            }
            Ok(namespaces)
        }

        async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from tables
                     where warehouse = ?1
                     order by namespace_path, table_name",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut tables = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                tables.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(tables)
        }

        async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            tx.execute(
                "insert or ignore into namespaces (warehouse, namespace_path, namespace_json)
                 values (?1, ?2, ?3)",
                (
                    table.ident.warehouse.as_str(),
                    table.ident.namespace.path(),
                    encode_json(table.ident.namespace.parts())?,
                ),
            )
            .await
            .map_err(turso_error)?;

            let result = tx
                .execute(
                    "insert into tables (
                    table_key, warehouse, namespace_path, table_name,
                    metadata_location, version, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    (
                        table_key(&table.ident),
                        table.ident.warehouse.as_str(),
                        table.ident.namespace.path(),
                        table.ident.name.as_str(),
                        table.metadata_location.as_deref(),
                        checked_i64(table.version, "table version")?,
                        encode_json(&table)?,
                        table.updated_at.to_rfc3339(),
                    ),
                )
                .await;

            match result {
                Ok(_) => {
                    tx.commit().await.map_err(turso_error)?;
                    Ok(table)
                }
                Err(err) if is_unique_violation(&err) => Err(LakeCatError::Conflict(format!(
                    "table already exists: {}",
                    table.ident.stable_id()
                ))),
                Err(err) => Err(turso_error(err)),
            }
        }

        async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from tables where table_key = ?1",
                    (table_key(ident),),
                )
                .await
                .map_err(turso_error)?;
            rows.next()
                .await
                .map_err(turso_error)?
                .map(|row| decode_json(row_string(&row, 0)?))
                .transpose()?
                .ok_or_else(|| LakeCatError::NotFound {
                    object: "table",
                    name: ident.stable_id(),
                })
        }

        async fn commit_table(
            &self,
            ident: &TableIdent,
            commit: TableCommit,
        ) -> LakeCatResult<TableRecord> {
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            if let Some(idempotency_key) = &commit.idempotency_key {
                let idem_key = idempotency_record_key(ident, idempotency_key);
                let mut rows = tx
                    .query(
                        "select response_json from idempotency_records where idem_key = ?1",
                        (idem_key,),
                    )
                    .await
                    .map_err(turso_error)?;
                if let Some(row) = rows.next().await.map_err(turso_error)? {
                    let table = decode_json(row_string(&row, 0)?)?;
                    tx.commit().await.map_err(turso_error)?;
                    return Ok(table);
                }
            }

            let mut rows = tx
                .query(
                    "select record_json from tables where table_key = ?1",
                    (table_key(ident),),
                )
                .await
                .map_err(turso_error)?;
            let Some(row) = rows.next().await.map_err(turso_error)? else {
                return Err(LakeCatError::NotFound {
                    object: "table",
                    name: ident.stable_id(),
                });
            };
            let mut table: TableRecord = decode_json(row_string(&row, 0)?)?;
            let previous_metadata_location = table.metadata_location.clone();
            let request_hash = content_hash_json(&serde_json::json!({
                "requirements": &commit.requirements,
                "updates": &commit.updates,
            }))?;
            table.version += 1;
            table.updated_at = Utc::now();
            table.metadata["lakecat:version"] = serde_json::json!(table.version);
            table.metadata["lakecat:last-request-hash"] = serde_json::json!(request_hash);

            tx.execute(
                "update tables
                 set metadata_location = ?2, version = ?3, record_json = ?4, updated_at = ?5
                 where table_key = ?1",
                (
                    table_key(ident),
                    table.metadata_location.as_deref(),
                    checked_i64(table.version, "table version")?,
                    encode_json(&table)?,
                    table.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;

            let record = TableCommitRecord {
                table: ident.clone(),
                previous_metadata_location,
                new_metadata_location: table.metadata_location.clone(),
                sequence_number: table.version,
                principal: commit.principal.clone(),
                request_hash,
                committed_at: table.updated_at,
            };
            tx.execute(
                "insert into metadata_pointer_log (
                    table_key, sequence_number, previous_metadata_location,
                    new_metadata_location, principal_json, request_hash,
                    committed_at, record_json
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                (
                    table_key(ident),
                    checked_i64(record.sequence_number, "sequence number")?,
                    record.previous_metadata_location.as_deref(),
                    record.new_metadata_location.as_deref(),
                    encode_json(&record.principal)?,
                    record.request_hash.as_str(),
                    record.committed_at.to_rfc3339(),
                    encode_json(&record)?,
                ),
            )
            .await
            .map_err(turso_error)?;

            let audit_payload = serde_json::json!({
                "event-type": "table.commit",
                "table": ident,
                "commit": record,
            });
            let audit_event_id = content_hash_json(&audit_payload)?;
            tx.execute(
                "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                (
                    audit_event_id.as_str(),
                    "table.commit",
                    table_key(ident),
                    encode_json(&commit.principal)?,
                    record.request_hash.as_str(),
                    encode_json(&audit_payload)?,
                    table.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;

            let outbox_payload = serde_json::json!({
                "audit-event-id": audit_event_id,
                "event-type": "table.commit",
                "table": ident,
                "commit": record,
            });
            tx.execute(
                "insert into outbox_events (
                    event_id, sink, event_type, payload_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)",
                (
                    content_hash_json(&outbox_payload)?,
                    "lakecat.lineage-and-graph",
                    "table.commit",
                    encode_json(&outbox_payload)?,
                    table.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;

            if let Some(idempotency_key) = commit.idempotency_key {
                tx.execute(
                    "insert into idempotency_records (
                        idem_key, table_key, request_hash, response_json, created_at
                     )
                     values (?1, ?2, ?3, ?4, ?5)",
                    (
                        idempotency_record_key(ident, &idempotency_key),
                        table_key(ident),
                        record.request_hash.as_str(),
                        encode_json(&table)?,
                        table.updated_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
            }

            tx.commit().await.map_err(turso_error)?;
            Ok(table)
        }
    }

    const TURSO_MIGRATION: &[&str] = &[
        "create table if not exists namespaces (
            warehouse text not null,
            namespace_path text not null,
            namespace_json text not null,
            primary key (warehouse, namespace_path)
        )",
        "create table if not exists tables (
            table_key text primary key,
            warehouse text not null,
            namespace_path text not null,
            table_name text not null,
            metadata_location text,
            version integer not null,
            record_json text not null,
            updated_at text not null
        )",
        "create index if not exists idx_tables_warehouse_namespace
            on tables (warehouse, namespace_path, table_name)",
        "create table if not exists metadata_pointer_log (
            table_key text not null,
            sequence_number integer not null,
            previous_metadata_location text,
            new_metadata_location text,
            principal_json text not null,
            request_hash text not null,
            committed_at text not null,
            record_json text not null,
            primary key (table_key, sequence_number)
        )",
        "create table if not exists idempotency_records (
            idem_key text primary key,
            table_key text not null,
            request_hash text not null,
            response_json text not null,
            created_at text not null
        )",
        "create table if not exists audit_events (
            event_id text primary key,
            event_type text not null,
            table_key text,
            principal_json text not null,
            request_hash text,
            event_json text not null,
            created_at text not null
        )",
        "create table if not exists outbox_events (
            event_id text primary key,
            sink text not null,
            event_type text not null,
            payload_json text not null,
            created_at text not null,
            delivered_at text
        )",
    ];

    fn encode_json(value: impl serde::Serialize) -> LakeCatResult<String> {
        serde_json::to_string(&value)
            .map_err(|err| LakeCatError::Internal(format!("failed to encode store JSON: {err}")))
    }

    fn decode_json<T: DeserializeOwned>(value: String) -> LakeCatResult<T> {
        serde_json::from_str(&value)
            .map_err(|err| LakeCatError::Internal(format!("failed to decode store JSON: {err}")))
    }

    fn decode_namespace(value: String) -> LakeCatResult<Namespace> {
        Namespace::new(decode_json::<Vec<String>>(value)?)
    }

    fn idempotency_record_key(ident: &TableIdent, idempotency_key: &str) -> String {
        format!("{}:{idempotency_key}", ident.stable_id())
    }

    fn checked_i64(value: u64, name: &str) -> LakeCatResult<i64> {
        i64::try_from(value)
            .map_err(|_| LakeCatError::InvalidArgument(format!("{name} exceeds i64 range")))
    }

    fn row_string(row: &Row, idx: usize) -> LakeCatResult<String> {
        match row.get_value(idx).map_err(turso_error)? {
            Value::Text(value) => Ok(value),
            value => Err(LakeCatError::Internal(format!(
                "Turso catalog store expected text at column {idx}, got {value:?}"
            ))),
        }
    }

    #[cfg(test)]
    fn row_i64(row: &Row, idx: usize) -> LakeCatResult<i64> {
        match row.get_value(idx).map_err(turso_error)? {
            Value::Integer(value) => Ok(value),
            value => Err(LakeCatError::Internal(format!(
                "Turso catalog store expected integer at column {idx}, got {value:?}"
            ))),
        }
    }

    fn is_unique_violation(err: &turso::Error) -> bool {
        matches!(err, turso::Error::Constraint(message) if message.contains("UNIQUE") || message.contains("PRIMARY KEY"))
    }

    fn turso_error(err: turso::Error) -> LakeCatError {
        LakeCatError::Internal(format!("Turso catalog store error: {err}"))
    }

    #[cfg(test)]
    mod tests {
        use lakecat_core::{Principal, TableName};

        use super::*;

        #[tokio::test]
        async fn turso_store_round_trips_namespaces_tables_and_idempotent_commits() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);

            let namespace = "default".parse::<Namespace>().unwrap();
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            assert_eq!(
                store.list_namespaces(&warehouse).await.unwrap(),
                vec![namespace.clone()]
            );

            let ident = TableIdent::new(
                warehouse.clone(),
                namespace,
                TableName::new("events").unwrap(),
            );
            let table = TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            store.create_table(table).await.unwrap();
            assert_eq!(store.load_table(&ident).await.unwrap().version, 0);

            let commit = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                idempotency_key: Some("commit-1".to_string()),
                principal: Principal::anonymous(),
            };
            let committed = store.commit_table(&ident, commit.clone()).await.unwrap();
            assert_eq!(committed.version, 1);
            let replayed = store.commit_table(&ident, commit).await.unwrap();
            assert_eq!(replayed.version, 1);

            let commit_count = store.count_rows("metadata_pointer_log").await.unwrap();
            assert_eq!(commit_count, 1);
            let audit_count = store.count_rows("audit_events").await.unwrap();
            assert_eq!(audit_count, 1);
            let outbox_count = store.count_rows("outbox_events").await.unwrap();
            assert_eq!(outbox_count, 1);
        }
    }
}
