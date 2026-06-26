use std::future::Future;
use std::pin::Pin;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::Utc;
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName, WarehouseName,
    content_hash_bytes, content_hash_json,
};
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use turso::{Connection, Database, Row, Value as TursoValue};

use crate::{
    CatalogAuditEvent, CatalogStore, OutboxEvent, PolicyBinding, ProjectRecord, ServerRecord,
    SoftDeleteRecord, StorageProfile, TableCommit, TableCommitRecord, TableRecord, ViewRecord,
    ViewVersionReceipt, WarehouseRecord, metadata_pointer_conflict, namespace_not_empty,
    namespace_not_found, policy_binding_key, policy_bindings_for_table,
    require_expected_view_version, storage_profile_key, storage_profile_match, table_key,
    validate_expected_view_version, validate_project_id, validate_view_receipt_chains, view_key,
    view_key_parts, view_receipt_hash,
};

#[derive(Debug, Clone)]
pub struct TursoCatalogStore {
    db: Database,
    /// Pool of pragma-warmed write connections, reused across commits so the
    /// per-commit path skips `connect()` + re-applying `journal_mode=mvcc` /
    /// `busy_timeout`. Each concurrent writer still checks out a *distinct*
    /// connection and runs its own `BEGIN CONCURRENT`, so MVCC concurrency is
    /// unchanged — only the per-commit connection setup is amortized. `Arc` so a
    /// cloned store shares the pool (and the underlying database).
    write_pool: Arc<Mutex<Vec<Connection>>>,
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
        let store = Arc::new(Self {
            db,
            write_pool: Arc::new(Mutex::new(Vec::new())),
        });
        store.migrate().await?;
        Ok(store)
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Run `body` inside an MVCC `BEGIN CONCURRENT` write transaction on a fresh
    /// connection, committing on success.
    ///
    /// Turso MVCC (`journal_mode=mvcc`) gives snapshot isolation with eager
    /// write-write conflict detection, so writes to *different* rows commit
    /// concurrently — no global write lock is needed. A write-write conflict (or
    /// transient `Busy`) at commit is rolled back and the body retried with
    /// bounded backoff. A genuine same-row logical race (two commits to one
    /// table) converges to the metadata-pointer CAS `Conflict`: the retried body
    /// re-reads the winner's snapshot, the conditional UPDATE then matches zero
    /// rows, and the existing `Conflict` is returned.
    ///
    /// The body MUST be re-runnable: it can be invoked more than once, so it must
    /// borrow (not consume) any state it needs across attempts. Owned data used
    /// in the body should be reborrowed (`let x = &x;`) before the call so the
    /// `async move` future copies the reference rather than moving the value.
    async fn write_txn<T, F>(&self, mut body: F) -> LakeCatResult<T>
    where
        T: Send,
        F: for<'c> FnMut(&'c Connection) -> WriteTxnFuture<'c, T> + Send,
    {
        let conn = self.checkout_write_conn().await?;
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            // A failure to even begin the transaction means the connection is in
            // an unknown state; drop it (do not return it to the pool).
            if let Err(err) = conn.execute_batch("BEGIN CONCURRENT").await {
                return Err(turso_error(err));
            }
            match body(&conn).await {
                Ok(value) => match conn.execute_batch("COMMIT").await {
                    Ok(()) => {
                        self.return_write_conn(conn);
                        return Ok(value);
                    }
                    Err(err) if is_retryable_conflict(&err) && attempt < WRITE_TXN_MAX_ATTEMPTS => {
                        let _ = conn.execute_batch("ROLLBACK").await;
                        backoff(attempt).await;
                    }
                    Err(err) => {
                        let _ = conn.execute_batch("ROLLBACK").await;
                        self.return_write_conn(conn);
                        return Err(turso_error(err));
                    }
                },
                Err(err) => {
                    let _ = conn.execute_batch("ROLLBACK").await;
                    // This pre-release surfaces write-write conflicts at COMMIT,
                    // but a conflict observed mid-body is still retryable;
                    // everything else (including the unique-violation `Conflict`)
                    // is terminal.
                    if is_retryable_lakecat(&err) && attempt < WRITE_TXN_MAX_ATTEMPTS {
                        backoff(attempt).await;
                    } else {
                        self.return_write_conn(conn);
                        return Err(err);
                    }
                }
            }
        }
    }

    /// Check out a pragma-warmed write connection from the pool, or create and
    /// warm a fresh one if the pool is empty.
    async fn checkout_write_conn(&self) -> LakeCatResult<Connection> {
        if let Some(conn) = self
            .write_pool
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .pop()
        {
            return Ok(conn);
        }
        let conn = self.connect()?;
        apply_write_pragmas(&conn).await;
        Ok(conn)
    }

    /// Return a clean (post-COMMIT or post-ROLLBACK) connection to the pool for
    /// reuse, up to a bounded size; excess connections are dropped.
    fn return_write_conn(&self, conn: Connection) {
        let mut pool = self
            .write_pool
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if pool.len() < WRITE_POOL_MAX_IDLE {
            pool.push(conn);
        }
    }

    async fn migrate(&self) -> LakeCatResult<()> {
        let conn = self.connect()?;
        apply_write_pragmas(&conn).await;
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
        let warehouse = warehouse.clone();
        self.write_txn(move |conn| {
            let warehouse = warehouse.clone();
            let namespace = namespace.clone();
            Box::pin(async move {
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
            })
        })
        .await
    }

    async fn list_namespaces(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select namespace_json, warehouse, namespace_path from namespaces
                     where warehouse = ?1
                     order by namespace_path",
                (warehouse.as_str(),),
            )
            .await
            .map_err(turso_error)?;
        let mut namespaces = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let namespace = decode_namespace(row_string(&row, 0)?)?;
            let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
            let row_namespace_path = row_string(&row, 2)?;
            crate::validate_namespace_scope(
                &namespace,
                warehouse,
                &row_warehouse,
                row_namespace_path.as_str(),
            )?;
            namespaces.push(namespace);
        }
        Ok(namespaces)
    }

    async fn load_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select namespace_json, warehouse, namespace_path from namespaces
                     where warehouse = ?1 and namespace_path = ?2",
                (warehouse.as_str(), namespace.path()),
            )
            .await
            .map_err(turso_error)?;
        let Some(row) = rows.next().await.map_err(turso_error)? else {
            return Err(namespace_not_found(namespace));
        };
        let decoded = decode_namespace(row_string(&row, 0)?)?;
        let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
        let row_namespace_path = row_string(&row, 2)?;
        crate::validate_namespace_scope(
            &decoded,
            warehouse,
            &row_warehouse,
            row_namespace_path.as_str(),
        )?;
        Ok(decoded)
    }

    async fn drop_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        let namespace = self.load_namespace(warehouse, namespace).await?;
        let warehouse = warehouse.clone();
        self.write_txn(move |conn| {
            let warehouse = warehouse.clone();
            let namespace = namespace.clone();
            Box::pin(async move {
                let namespace_path = namespace.path();
                if count_matching_rows(conn, "tables", warehouse.as_str(), namespace_path.as_str())
                    .await?
                    > 0
                {
                    return Err(namespace_not_empty(&namespace, "tables"));
                }
                if count_matching_rows(conn, "views", warehouse.as_str(), namespace_path.as_str())
                    .await?
                    > 0
                {
                    return Err(namespace_not_empty(&namespace, "views"));
                }
                if count_matching_rows(
                    conn,
                    "policy_bindings",
                    warehouse.as_str(),
                    namespace_path.as_str(),
                )
                .await?
                    > 0
                {
                    return Err(namespace_not_empty(&namespace, "policy bindings"));
                }
                conn.execute(
                    "delete from namespaces where warehouse = ?1 and namespace_path = ?2",
                    (warehouse.as_str(), namespace_path),
                )
                .await
                .map_err(turso_error)?;
                Ok(namespace)
            })
        })
        .await
    }

    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select record_json, table_key, warehouse, namespace_path, table_name
                     from tables t
                     where warehouse = ?1
                       and not exists (
                         select 1 from soft_deletes d where d.table_key = t.table_key
                       )
                     order by namespace_path, table_name",
                (warehouse.as_str(),),
            )
            .await
            .map_err(turso_error)?;
        let mut tables = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let table: TableRecord = decode_json(row_string(&row, 0)?)?;
            let ident = TableIdent::new(
                WarehouseName::new(row_string(&row, 2)?)?,
                row_string(&row, 3)?.parse()?,
                TableName::new(row_string(&row, 4)?)?,
            );
            crate::validate_table_record_scope(
                &table,
                &ident,
                &row_string(&row, 1)?,
                &row_string(&row, 2)?,
                &row_string(&row, 3)?,
                &row_string(&row, 4)?,
            )?;
            tables.push(table);
        }
        Ok(tables)
    }

    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
        table.validate()?;
        self.write_txn(move |conn| {
            let table = table.clone();
            Box::pin(async move {
                conn.execute(
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

                let result = conn
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
                    Ok(_) => Ok(table),
                    Err(err) if is_unique_violation(&err) => Err(LakeCatError::Conflict(format!(
                        "table already exists: {}",
                        table.ident.stable_id()
                    ))),
                    Err(err) => Err(turso_error(err)),
                }
            })
        })
        .await
    }

    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select record_json, table_key, warehouse, namespace_path, table_name
                     from tables t
                     where t.table_key = ?1
                       and not exists (
                         select 1 from soft_deletes d where d.table_key = t.table_key
                       )",
                (table_key(ident),),
            )
            .await
            .map_err(turso_error)?;
        rows.next()
            .await
            .map_err(turso_error)?
            .map(|row| {
                let table: TableRecord = decode_json(row_string(&row, 0)?)?;
                crate::validate_table_record_scope(
                    &table,
                    ident,
                    &row_string(&row, 1)?,
                    &row_string(&row, 2)?,
                    &row_string(&row, 3)?,
                    &row_string(&row, 4)?,
                )?;
                Ok(table)
            })
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
        commit.validate()?;
        let ident = ident.clone();
        self.write_txn(move |conn| {
            let ident = ident.clone();
            let commit = commit.clone();
            Box::pin(async move {
                let ident = &ident;
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
            let idem_key = idempotency_record_key(ident, idempotency_key);
            let mut rows = conn
                    .query(
                        "select table_key, request_hash, response_json from idempotency_records where idem_key = ?1",
                        (idem_key,),
                    )
                    .await
                    .map_err(turso_error)?;
            if let Some(row) = rows.next().await.map_err(turso_error)? {
                crate::validate_idempotency_record_table_key(&row_string(&row, 0)?, ident)?;
                let replay_hash = row_string(&row, 1)?;
                crate::validate_idempotency_record_request_hash(&replay_hash)?;
                if replay_hash != idempotency_request_hash {
                    return Err(LakeCatError::Conflict(format!(
                        "idempotency key reused with different commit request for {}",
                        ident.stable_id()
                    )));
                }
                let table = decode_json(row_string(&row, 2)?)?;
                crate::validate_table_record_identity(&table, ident)?;
                return Ok(table);
            }
        }

        let mut rows = conn
            .query(
                "select record_json, table_key, warehouse, namespace_path, table_name
                     from tables t
                     where t.table_key = ?1
                       and not exists (
                         select 1 from soft_deletes d where d.table_key = t.table_key
                       )",
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
        crate::validate_table_record_scope(
            &table,
            ident,
            &row_string(&row, 1)?,
            &row_string(&row, 2)?,
            &row_string(&row, 3)?,
            &row_string(&row, 4)?,
        )?;
        let previous_metadata_location = table.metadata_location.clone();
        let idempotency_key_sha256 = commit
            .idempotency_key
            .as_ref()
            .map(|key| content_hash_bytes(key.as_bytes()));
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

        let updated_rows = conn
            .execute(
                "update tables
                 set metadata_location = ?2, version = ?3, record_json = ?4, updated_at = ?5
                 where table_key = ?1
                   and (
                     (metadata_location is null and ?6 is null)
                     or metadata_location = ?7
                   )",
                (
                    table_key(ident),
                    table.metadata_location.as_deref(),
                    checked_i64(table.version, "table version")?,
                    encode_json(&table)?,
                    table.updated_at.to_rfc3339(),
                    commit.expected_previous_metadata_location.as_deref(),
                    commit.expected_previous_metadata_location.as_deref(),
                ),
            )
            .await
            .map_err(turso_error)?;
        if updated_rows == 0 {
            return Err(metadata_pointer_conflict(
                ident,
                commit.expected_previous_metadata_location.as_deref(),
                previous_metadata_location.as_deref(),
            ));
        }

        let record = TableCommitRecord {
            table: ident.clone(),
            previous_metadata_location,
            new_metadata_location: table.metadata_location.clone(),
            sequence_number: table.version,
            principal: commit.principal.clone(),
            format_version: crate::table_commit_format_version(&table),
            snapshot_id: crate::table_commit_snapshot_id(&table),
            policy_hash: crate::table_commit_policy_hash(commit.authorization_receipt.as_ref()),
            request_hash,
            response_hash: crate::table_response_hash(&table)?,
            idempotency_key_sha256,
            committed_at: table.updated_at,
        };
        conn.execute(
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
            "authorization-receipt": commit.authorization_receipt,
        });
        let audit_payload_hash = content_hash_json(&audit_payload)?;
        conn.execute(
            "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                audit_payload_hash.as_str(),
                "table.commit",
                table_key(ident),
                encode_json(&commit.principal)?,
                audit_payload_hash.as_str(),
                encode_json(&audit_payload)?,
                table.updated_at.to_rfc3339(),
            ),
        )
        .await
        .map_err(turso_error)?;

        let outbox_payload = serde_json::json!({
            "audit-event-id": audit_payload_hash,
            "event-type": "table.commit",
            "table": ident,
            "commit": record,
            "authorization-receipt": audit_payload["authorization-receipt"].clone(),
        });
        conn.execute(
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
            conn.execute(
                "insert into idempotency_records (
                        idem_key, table_key, request_hash, response_json, created_at
                     )
                     values (?1, ?2, ?3, ?4, ?5)",
                (
                    idempotency_record_key(ident, &idempotency_key),
                    table_key(ident),
                    commit
                        .idempotency_request_hash
                        .as_deref()
                        .unwrap_or(record.request_hash.as_str()),
                    encode_json(&table)?,
                    table.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
        }

        Ok(table)
            })
        })
        .await
    }

    async fn replay_table_commit(
        &self,
        ident: &TableIdent,
        idempotency_key: &str,
        idempotency_request_hash: &str,
    ) -> LakeCatResult<Option<TableRecord>> {
        crate::validate_idempotency_key_shape(idempotency_key)?;
        crate::validate_idempotency_request_hash_shape(idempotency_request_hash)?;
        let conn = self.connect()?;
        let mut rows = conn
                .query(
                    "select table_key, request_hash, response_json from idempotency_records where idem_key = ?1",
                    (idempotency_record_key(ident, idempotency_key),),
                )
                .await
                .map_err(turso_error)?;
        let Some(row) = rows.next().await.map_err(turso_error)? else {
            return Ok(None);
        };
        crate::validate_idempotency_record_table_key(&row_string(&row, 0)?, ident)?;
        let replay_hash = row_string(&row, 1)?;
        crate::validate_idempotency_record_request_hash(&replay_hash)?;
        if replay_hash != idempotency_request_hash {
            return Err(LakeCatError::Conflict(format!(
                "idempotency key reused with different commit request for {}",
                ident.stable_id()
            )));
        }
        let table = decode_json(row_string(&row, 2)?)?;
        crate::validate_table_record_identity(&table, ident)?;
        Ok(Some(table))
    }

    async fn soft_delete_table(
        &self,
        ident: &TableIdent,
        principal: lakecat_core::Principal,
        authorization_receipt: Option<JsonValue>,
    ) -> LakeCatResult<TableRecord> {
        let ident = ident.clone();
        self.write_txn(move |conn| {
            let ident = ident.clone();
            let principal = principal.clone();
            let authorization_receipt = authorization_receipt.clone();
            Box::pin(async move {
                let ident = &ident;
                let mut rows = conn
                    .query(
                        "select record_json, table_key, warehouse, namespace_path, table_name
                     from tables t
                     where t.table_key = ?1
                       and not exists (
                         select 1 from soft_deletes d where d.table_key = t.table_key
                       )",
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
                let table: TableRecord = decode_json(row_string(&row, 0)?)?;
                crate::validate_table_record_scope(
                    &table,
                    ident,
                    &row_string(&row, 1)?,
                    &row_string(&row, 2)?,
                    &row_string(&row, 3)?,
                    &row_string(&row, 4)?,
                )?;
                let deleted_at = Utc::now();
                let record = SoftDeleteRecord {
                    table: ident.clone(),
                    metadata_location: table.metadata_location.clone(),
                    version: table.version,
                    format_version: crate::table_commit_format_version(&table),
                    principal: principal.clone(),
                    authorization_receipt,
                    deleted_at,
                };
                record.validate_for_table(ident, &table)?;
                conn.execute(
                    "insert into soft_deletes (
                    table_key, warehouse, namespace_path, table_name,
                    metadata_location, version, principal_json,
                    authorization_receipt_json, record_json, deleted_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    (
                        table_key(ident),
                        ident.warehouse.as_str(),
                        ident.namespace.path(),
                        ident.name.as_str(),
                        table.metadata_location.as_deref(),
                        checked_i64(table.version, "table version")?,
                        encode_json(&principal)?,
                        record
                            .authorization_receipt
                            .as_ref()
                            .map(encode_json)
                            .transpose()?,
                        encode_json(&record)?,
                        deleted_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;

                let audit_payload = serde_json::json!({
                    "event-type": "table.deleted",
                    "table": ident,
                    "soft-delete": record,
                    "authorization-receipt": record.authorization_receipt,
                });
                let audit_event_id = content_hash_json(&audit_payload)?;
                conn.execute(
                    "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    (
                        audit_event_id.as_str(),
                        "table.deleted",
                        table_key(ident),
                        encode_json(&principal)?,
                        audit_event_id.as_str(),
                        encode_json(&audit_payload)?,
                        deleted_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                let outbox_payload = serde_json::json!({
                    "audit-event-id": audit_event_id,
                    "event-type": "table.deleted",
                    "table": ident,
                    "soft-delete": audit_payload["soft-delete"].clone(),
                    "authorization-receipt": audit_payload["authorization-receipt"].clone(),
                });
                conn.execute(
                    "insert into outbox_events (
                    event_id, sink, event_type, payload_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)",
                    (
                        content_hash_json(&outbox_payload)?,
                        "lakecat.lineage-and-graph",
                        "table.deleted",
                        encode_json(&outbox_payload)?,
                        deleted_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                Ok(table)
            })
        })
        .await
    }

    async fn restore_table(
        &self,
        ident: &TableIdent,
        principal: lakecat_core::Principal,
        authorization_receipt: Option<JsonValue>,
    ) -> LakeCatResult<TableRecord> {
        let ident = ident.clone();
        self.write_txn(move |conn| {
            let ident = ident.clone();
            let principal = principal.clone();
            let authorization_receipt = authorization_receipt.clone();
            Box::pin(async move {
                let ident = &ident;
                let mut rows = conn
                    .query(
                        "select t.record_json, t.table_key, t.warehouse, t.namespace_path,
                            t.table_name, d.table_key, d.warehouse, d.namespace_path,
                            d.table_name, d.metadata_location, d.version,
                            d.deleted_at, d.record_json
                     from tables t
                     join soft_deletes d on d.table_key = t.table_key
                     where t.table_key = ?1",
                        (table_key(ident),),
                    )
                    .await
                    .map_err(turso_error)?;
                let Some(row) = rows.next().await.map_err(turso_error)? else {
                    return Err(LakeCatError::NotFound {
                        object: "soft-deleted table",
                        name: ident.stable_id(),
                    });
                };
                let table: TableRecord = decode_json(row_string(&row, 0)?)?;
                crate::validate_table_record_scope(
                    &table,
                    ident,
                    &row_string(&row, 1)?,
                    &row_string(&row, 2)?,
                    &row_string(&row, 3)?,
                    &row_string(&row, 4)?,
                )?;
                let soft_delete: SoftDeleteRecord = decode_json(row_string(&row, 12)?)?;
                soft_delete.validate_for_table(ident, &table)?;
                validate_turso_soft_delete_row(&soft_delete, ident, &row, 5)?;
                let restored_at = Utc::now();
                let changed = conn
                    .execute(
                        "delete from soft_deletes where table_key = ?1",
                        (table_key(ident),),
                    )
                    .await
                    .map_err(turso_error)?;
                if changed == 0 {
                    return Err(LakeCatError::NotFound {
                        object: "soft-deleted table",
                        name: ident.stable_id(),
                    });
                }

                let audit_payload = serde_json::json!({
                    "event-type": "table.restored",
                    "table": ident,
                    "authorization-receipt": authorization_receipt,
                    "metadata-location": table.metadata_location,
                    "format-version": crate::table_commit_format_version(&table),
                    "version": table.version,
                });
                let audit_event_id = content_hash_json(&audit_payload)?;
                conn.execute(
                    "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    (
                        audit_event_id.as_str(),
                        "table.restored",
                        table_key(ident),
                        encode_json(&principal)?,
                        audit_event_id.as_str(),
                        encode_json(&audit_payload)?,
                        restored_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                let outbox_payload = serde_json::json!({
                    "audit-event-id": audit_event_id,
                    "event-type": "table.restored",
                    "table": ident,
                    "payload": audit_payload,
                    "authorization-receipt": audit_payload["authorization-receipt"].clone(),
                });
                conn.execute(
                    "insert into outbox_events (
                    event_id, sink, event_type, payload_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)",
                    (
                        content_hash_json(&outbox_payload)?,
                        "lakecat.lineage-and-graph",
                        "table.restored",
                        encode_json(&outbox_payload)?,
                        restored_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                Ok(table)
            })
        })
        .await
    }

    async fn table_commit_records(
        &self,
        ident: &TableIdent,
        start_version: u64,
        end_version: Option<u64>,
    ) -> LakeCatResult<Vec<TableCommitRecord>> {
        let conn = self.connect()?;
        let end_version = end_version.unwrap_or(i64::MAX as u64);
        let mut rows = conn
            .query(
                "select table_key, sequence_number, previous_metadata_location,
                            new_metadata_location, request_hash, principal_json, committed_at,
                            record_json
                     from metadata_pointer_log
                     where table_key = ?1
                       and sequence_number >= ?2
                       and sequence_number <= ?3
                     order by sequence_number",
                (
                    table_key(ident),
                    checked_i64(start_version, "start version")?,
                    checked_i64(end_version, "end version")?,
                ),
            )
            .await
            .map_err(turso_error)?;
        let mut commits = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let commit: TableCommitRecord = decode_json(row_string(&row, 7)?)?;
            commit.validate_for_table(ident)?;
            validate_turso_commit_record_row(&commit, ident, &row)?;
            commits.push(commit);
        }
        Ok(commits)
    }

    async fn upsert_server(&self, server: ServerRecord) -> LakeCatResult<ServerRecord> {
        server.validate()?;
        self.write_txn(move |conn| {
            let server = server.clone();
            Box::pin(async move {
                let mut rows = conn
                    .query(
                        "select record_json, server_id from servers where server_id = ?1",
                        (server.server_id.as_str(),),
                    )
                    .await
                    .map_err(turso_error)?;
                if let Some(row) = rows.next().await.map_err(turso_error)? {
                    let existing: ServerRecord = decode_json(row_string(&row, 0)?)?;
                    crate::validate_server_record_scope(&existing, &row_string(&row, 1)?)?;
                }
                conn.execute(
                    "insert into servers (
                    server_id, display_name, endpoint_url, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)
                 on conflict(server_id) do update set
                    display_name = excluded.display_name,
                    endpoint_url = excluded.endpoint_url,
                    record_json = excluded.record_json,
                    updated_at = excluded.updated_at",
                    (
                        server.server_id.as_str(),
                        server.display_name.as_deref(),
                        server.endpoint_url.as_deref(),
                        encode_json(&server)?,
                        server.updated_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                Ok(server)
            })
        })
        .await
    }

    async fn list_servers(&self) -> LakeCatResult<Vec<ServerRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select record_json, server_id from servers
                     order by server_id",
                (),
            )
            .await
            .map_err(turso_error)?;
        let mut servers = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let server: ServerRecord = decode_json(row_string(&row, 0)?)?;
            crate::validate_server_record_scope(&server, &row_string(&row, 1)?)?;
            servers.push(server);
        }
        Ok(servers)
    }

    async fn upsert_project(&self, project: ProjectRecord) -> LakeCatResult<ProjectRecord> {
        project.validate()?;
        self.write_txn(move |conn| {
            let project = project.clone();
            Box::pin(async move {
                if let Some(server_id) = project.server_id.as_deref() {
                    let mut rows = conn
                .query(
                    "select record_json, server_id from servers where server_id = ?1 limit 1",
                    (server_id,),
                )
                .await
                .map_err(turso_error)?;
                    let Some(row) = rows.next().await.map_err(turso_error)? else {
                        return Err(LakeCatError::NotFound {
                            object: "server",
                            name: server_id.to_string(),
                        });
                    };
                    let parent: ServerRecord = decode_json(row_string(&row, 0)?)?;
                    crate::validate_server_record_scope(&parent, &row_string(&row, 1)?)?;
                }
                let mut rows = conn
                    .query(
                        "select record_json, project_id from projects where project_id = ?1",
                        (project.project_id.as_str(),),
                    )
                    .await
                    .map_err(turso_error)?;
                if let Some(row) = rows.next().await.map_err(turso_error)? {
                    let existing: ProjectRecord = decode_json(row_string(&row, 0)?)?;
                    crate::validate_project_record_scope(&existing, &row_string(&row, 1)?)?;
                }
                conn.execute(
                    "insert into projects (
                    project_id, display_name, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4)
                 on conflict(project_id) do update set
                    display_name = excluded.display_name,
                    record_json = excluded.record_json,
                    updated_at = excluded.updated_at",
                    (
                        project.project_id.as_str(),
                        project.display_name.as_deref(),
                        encode_json(&project)?,
                        project.updated_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                Ok(project)
            })
        })
        .await
    }

    async fn list_projects(&self) -> LakeCatResult<Vec<ProjectRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select record_json, project_id from projects
                     order by project_id",
                (),
            )
            .await
            .map_err(turso_error)?;
        let mut projects = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let project: ProjectRecord = decode_json(row_string(&row, 0)?)?;
            crate::validate_project_record_scope(&project, &row_string(&row, 1)?)?;
            projects.push(project);
        }
        Ok(projects)
    }

    async fn upsert_warehouse(&self, warehouse: WarehouseRecord) -> LakeCatResult<WarehouseRecord> {
        warehouse.validate()?;
        self.write_txn(move |conn| {
            let warehouse = warehouse.clone();
            Box::pin(async move {
                {
                    let mut rows = conn
                .query(
                    "select record_json, project_id from projects where project_id = ?1 limit 1",
                    (warehouse.project_id.as_str(),),
                )
                .await
                .map_err(turso_error)?;
                    let Some(row) = rows.next().await.map_err(turso_error)? else {
                        return Err(LakeCatError::NotFound {
                            object: "project",
                            name: warehouse.project_id.clone(),
                        });
                    };
                    let parent: ProjectRecord = decode_json(row_string(&row, 0)?)?;
                    crate::validate_project_record_scope(&parent, &row_string(&row, 1)?)?;
                }
                let mut rows = conn
                    .query(
                        "select record_json, warehouse, project_id, storage_root
                     from warehouses
                     where warehouse = ?1",
                        (warehouse.warehouse.as_str(),),
                    )
                    .await
                    .map_err(turso_error)?;
                if let Some(row) = rows.next().await.map_err(turso_error)? {
                    let existing: WarehouseRecord = decode_json(row_string(&row, 0)?)?;
                    let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                    crate::validate_warehouse_record_scope(
                        &existing,
                        &row_warehouse,
                        &row_string(&row, 2)?,
                        row_optional_string(&row, 3)?.as_deref(),
                    )?;
                }
                conn.execute(
                    "insert into warehouses (
                    warehouse, project_id, storage_root, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)
                 on conflict(warehouse) do update set
                    project_id = excluded.project_id,
                    storage_root = excluded.storage_root,
                    record_json = excluded.record_json,
                    updated_at = excluded.updated_at",
                    (
                        warehouse.warehouse.as_str(),
                        warehouse.project_id.as_str(),
                        warehouse.storage_root.as_deref(),
                        encode_json(&warehouse)?,
                        warehouse.updated_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                Ok(warehouse)
            })
        })
        .await
    }

    async fn load_warehouse(&self, warehouse: &WarehouseName) -> LakeCatResult<WarehouseRecord> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select record_json, warehouse, project_id, storage_root from warehouses
                     where warehouse = ?1",
                (warehouse.as_str(),),
            )
            .await
            .map_err(turso_error)?;
        rows.next()
            .await
            .map_err(turso_error)?
            .map(|row| {
                let record: WarehouseRecord = decode_json(row_string(&row, 0)?)?;
                let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                crate::validate_warehouse_record_scope(
                    &record,
                    &row_warehouse,
                    &row_string(&row, 2)?,
                    row_optional_string(&row, 3)?.as_deref(),
                )?;
                Ok(record)
            })
            .transpose()?
            .ok_or_else(|| LakeCatError::NotFound {
                object: "warehouse",
                name: warehouse.as_str().to_string(),
            })
    }

    async fn list_warehouses(&self) -> LakeCatResult<Vec<WarehouseRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select record_json, warehouse, project_id, storage_root from warehouses
                     order by warehouse",
                (),
            )
            .await
            .map_err(turso_error)?;
        let mut warehouses = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let record: WarehouseRecord = decode_json(row_string(&row, 0)?)?;
            let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
            crate::validate_warehouse_record_scope(
                &record,
                &row_warehouse,
                &row_string(&row, 2)?,
                row_optional_string(&row, 3)?.as_deref(),
            )?;
            warehouses.push(record);
        }
        Ok(warehouses)
    }

    async fn list_project_warehouses(
        &self,
        project_id: &str,
    ) -> LakeCatResult<Vec<WarehouseRecord>> {
        validate_project_id(project_id)?;
        let conn = self.connect()?;
        let project_exists = {
            let mut rows = conn
                .query(
                    "select 1 from projects where project_id = ?1 limit 1",
                    (project_id,),
                )
                .await
                .map_err(turso_error)?;
            rows.next().await.map_err(turso_error)?.is_some()
        };
        if !project_exists {
            return Err(LakeCatError::NotFound {
                object: "project",
                name: project_id.to_string(),
            });
        }
        let mut rows = conn
            .query(
                "select record_json, warehouse, project_id, storage_root from warehouses
                     where project_id = ?1
                     order by warehouse",
                (project_id,),
            )
            .await
            .map_err(turso_error)?;
        let mut warehouses = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let record: WarehouseRecord = decode_json(row_string(&row, 0)?)?;
            let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
            crate::validate_warehouse_record_scope(
                &record,
                &row_warehouse,
                &row_string(&row, 2)?,
                row_optional_string(&row, 3)?.as_deref(),
            )?;
            warehouses.push(record);
        }
        Ok(warehouses)
    }

    async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
        self.upsert_view_if_version(view, None).await
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
        self.write_txn(move |conn| {
            let view = view.clone();
            Box::pin(async move {
                let view_key = view_key(&view);
                let principal = view.created.principal.clone();
                let previous = conn
                    .query(
                        "select record_json, warehouse, namespace_path, view_name from views
                     where view_key = ?1
                     limit 1",
                        (view_key.as_str(),),
                    )
                    .await
                    .map_err(turso_error)?
                    .next()
                    .await
                    .map_err(turso_error)?
                    .map(|row| {
                        let view = decode_json::<ViewRecord>(row_string(&row, 0)?)?;
                        let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                        let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
                        let row_view = TableName::new(row_string(&row, 3)?)?;
                        crate::validate_view_record_scope(
                            &view,
                            &row_warehouse,
                            &row_namespace,
                            &row_view,
                        )?;
                        Ok(view)
                    })
                    .transpose()?;
                if let Some(expected) = expected_view_version {
                    require_expected_view_version(previous.as_ref(), expected)?;
                }
                let latest_receipt = latest_turso_view_receipt_evidence(
                    conn,
                    view_key.as_str(),
                    &view.warehouse,
                    &view.namespace,
                    &view.name,
                )
                .await?;
                let latest_receipt_version = latest_receipt
                    .as_ref()
                    .map(|(view_version, _)| *view_version);
                let previous_receipt_hash = latest_receipt.map(|(_, receipt_hash)| receipt_hash);
                let previous_view_version = previous
                    .as_ref()
                    .map(|view| view.view_version)
                    .or(latest_receipt_version);
                let view = view
                    .with_next_version_after_history(previous.as_ref(), latest_receipt_version)?;
                conn.execute(
                    "insert into views (
                    view_key, warehouse, namespace_path, view_name, dialect, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 on conflict(view_key) do update set
                    dialect = excluded.dialect,
                    record_json = excluded.record_json,
                    updated_at = excluded.updated_at",
                    (
                        view_key.as_str(),
                        view.warehouse.as_str(),
                        view.namespace.path().as_str(),
                        view.name.as_str(),
                        view.dialect.as_str(),
                        encode_json(&view)?,
                        view.updated_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                let receipt = ViewVersionReceipt::upsert(
                    previous_view_version,
                    previous_receipt_hash,
                    &view,
                    principal,
                )?;
                let receipt_id = view_receipt_hash(&receipt)?;
                let previous_view_version = receipt
                    .previous_view_version
                    .map(|version| checked_i64(version, "previous view version"))
                    .transpose()?;
                conn.execute(
                    "insert into view_version_receipts (
                    receipt_id, view_key, warehouse, namespace_path, view_name,
                    view_version, previous_view_version, operation, view_hash,
                    principal_json, receipt_json, recorded_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    (
                        receipt_id.as_str(),
                        view_key.as_str(),
                        receipt.warehouse.as_str(),
                        receipt.namespace.path().as_str(),
                        receipt.name.as_str(),
                        checked_i64(receipt.view_version, "view version")?,
                        previous_view_version,
                        "upsert",
                        receipt.view_hash.as_str(),
                        encode_json(&receipt.principal)?,
                        encode_json(&receipt)?,
                        receipt.recorded_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                Ok(view)
            })
        })
        .await
    }

    async fn list_view_version_receipts(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        let conn = self.connect()?;
        let view_key = view_key_parts(warehouse, namespace, view);
        let mut rows = conn
                .query(
                    "select receipt_json, warehouse, namespace_path, view_name from view_version_receipts
                     where view_key = ?1
                     order by view_version, recorded_at, receipt_id",
                    (view_key.as_str(),),
                )
                .await
                .map_err(turso_error)?;
        let mut receipts = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let receipt: ViewVersionReceipt = decode_json(row_string(&row, 0)?)?;
            let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
            let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
            let row_view = TableName::new(row_string(&row, 3)?)?;
            crate::validate_view_receipt_scope(
                &receipt,
                &row_warehouse,
                &row_namespace,
                Some(&row_view),
            )?;
            crate::validate_view_receipt_scope(&receipt, warehouse, namespace, Some(view))?;
            receipts.push(receipt);
        }
        validate_view_receipt_chains(&receipts)?;
        Ok(receipts)
    }

    async fn list_namespace_view_version_receipts(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select receipt_json, view_name from view_version_receipts
                     where warehouse = ?1 and namespace_path = ?2
                     order by view_name, view_version, recorded_at, receipt_id",
                (warehouse.as_str(), namespace.path().as_str()),
            )
            .await
            .map_err(turso_error)?;
        let mut receipts = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let receipt: ViewVersionReceipt = decode_json(row_string(&row, 0)?)?;
            let row_view = TableName::new(row_string(&row, 1)?)?;
            crate::validate_view_receipt_scope(&receipt, warehouse, namespace, Some(&row_view))?;
            receipts.push(receipt);
        }
        validate_view_receipt_chains(&receipts)?;
        Ok(receipts)
    }

    async fn load_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<ViewRecord> {
        let conn = self.connect()?;
        let view_key = view_key_parts(warehouse, namespace, view);
        conn.query(
            "select record_json, warehouse, namespace_path, view_name from views
                 where view_key = ?1
                 limit 1",
            (view_key.as_str(),),
        )
        .await
        .map_err(turso_error)?
        .next()
        .await
        .map_err(turso_error)?
        .map(|row| {
            let view = decode_json::<ViewRecord>(row_string(&row, 0)?)?;
            let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
            let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
            let row_view = TableName::new(row_string(&row, 3)?)?;
            crate::validate_view_record_scope(&view, &row_warehouse, &row_namespace, &row_view)?;
            Ok(view)
        })
        .transpose()?
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
        let warehouse = warehouse.clone();
        let namespace = namespace.clone();
        let view = view.clone();
        self.write_txn(move |conn| {
            let warehouse = warehouse.clone();
            let namespace = namespace.clone();
            let view = view.clone();
            let principal = principal.clone();
            Box::pin(async move {
                let warehouse = &warehouse;
                let namespace = &namespace;
                let view = &view;
                let view_key = view_key_parts(warehouse, namespace, view);
                let record = conn
                    .query(
                        "select record_json, warehouse, namespace_path, view_name from views
                     where view_key = ?1
                     limit 1",
                        (view_key.as_str(),),
                    )
                    .await
                    .map_err(turso_error)?
                    .next()
                    .await
                    .map_err(turso_error)?
                    .map(|row| {
                        let view = decode_json::<ViewRecord>(row_string(&row, 0)?)?;
                        let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                        let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
                        let row_view = TableName::new(row_string(&row, 3)?)?;
                        crate::validate_view_record_scope(
                            &view,
                            &row_warehouse,
                            &row_namespace,
                            &row_view,
                        )?;
                        Ok(view)
                    })
                    .transpose()?
                    .ok_or_else(|| LakeCatError::NotFound {
                        object: "view",
                        name: view.as_str().to_string(),
                    })?;
                if let Some(expected) = expected_view_version {
                    require_expected_view_version(Some(&record), expected)?;
                }
                let previous_receipt_hash = latest_turso_view_receipt_hash(
                    conn,
                    view_key.as_str(),
                    warehouse,
                    namespace,
                    view,
                )
                .await?;
                let receipt = ViewVersionReceipt::drop(&record, previous_receipt_hash, principal)?;
                let receipt_id = view_receipt_hash(&receipt)?;
                let previous_view_version = receipt
                    .previous_view_version
                    .map(|version| checked_i64(version, "previous view version"))
                    .transpose()?;
                conn.execute(
                    "delete from views where view_key = ?1",
                    (view_key.as_str(),),
                )
                .await
                .map_err(turso_error)?;
                conn.execute(
                    "insert into view_version_receipts (
                    receipt_id, view_key, warehouse, namespace_path, view_name,
                    view_version, previous_view_version, operation, view_hash,
                    principal_json, receipt_json, recorded_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    (
                        receipt_id.as_str(),
                        view_key.as_str(),
                        receipt.warehouse.as_str(),
                        receipt.namespace.path().as_str(),
                        receipt.name.as_str(),
                        checked_i64(receipt.view_version, "view version")?,
                        previous_view_version,
                        "drop",
                        receipt.view_hash.as_str(),
                        encode_json(&receipt.principal)?,
                        encode_json(&receipt)?,
                        receipt.recorded_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                Ok(record)
            })
        })
        .await
    }

    async fn list_views(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select record_json, warehouse, namespace_path, view_name from views
                     where warehouse = ?1 and namespace_path = ?2
                     order by view_name",
                (warehouse.as_str(), namespace.path().as_str()),
            )
            .await
            .map_err(turso_error)?;
        let mut views = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let view: ViewRecord = decode_json(row_string(&row, 0)?)?;
            let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
            let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
            let row_view = TableName::new(row_string(&row, 3)?)?;
            crate::validate_view_record_scope(&view, &row_warehouse, &row_namespace, &row_view)?;
            views.push(view);
        }
        Ok(views)
    }

    async fn record_audit_event(&self, event: CatalogAuditEvent) -> LakeCatResult<()> {
        event.validate_recordable()?;
        self.write_txn(move |conn| {
            let event = event.clone();
            Box::pin(async move {
                let event_id = crate::audit_event_id(&event)?;
                conn.execute(
                    "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    (
                        event_id.as_str(),
                        event.event_type.as_str(),
                        event.table.as_ref().map(table_key),
                        encode_json(&event.principal)?,
                        event.request_hash.as_deref(),
                        encode_json(&event.payload)?,
                        event.created_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;

                let outbox_payload = crate::audit_outbox_payload(&event_id, &event);
                insert_outbox_event(conn, &outbox_payload, event.created_at).await?;
                Ok(())
            })
        })
        .await
    }

    async fn pending_outbox_events(
        &self,
        sink: Option<&str>,
        limit: usize,
    ) -> LakeCatResult<Vec<OutboxEvent>> {
        let conn = self.connect()?;
        let limit = checked_i64(limit as u64, "outbox event limit")?;
        let mut rows = if let Some(sink) = sink {
            conn.query(
                "select event_id, sink, event_type, payload_json, created_at, delivered_at
                     from outbox_events
                     where delivered_at is null and sink = ?1
                     order by created_at, event_id
                     limit ?2",
                (sink, limit),
            )
            .await
            .map_err(turso_error)?
        } else {
            conn.query(
                "select event_id, sink, event_type, payload_json, created_at, delivered_at
                     from outbox_events
                     where delivered_at is null
                     order by created_at, event_id
                     limit ?1",
                (limit,),
            )
            .await
            .map_err(turso_error)?
        };

        let mut events = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let event = outbox_event_from_row(&row)?;
            event.validate_pending()?;
            events.push(event);
        }
        Ok(events)
    }

    async fn mark_outbox_delivered(&self, event_ids: &[String]) -> LakeCatResult<usize> {
        if event_ids.is_empty() {
            return Ok(0);
        }
        for event_id in event_ids {
            crate::validate_outbox_event_id_shape(event_id)?;
        }
        let event_ids = event_ids.iter().cloned().collect::<BTreeSet<String>>();
        self.write_txn(move |conn| {
            let event_ids = event_ids.clone();
            Box::pin(async move {
                let delivered_at = Utc::now().to_rfc3339();
                let mut validated_event_ids = Vec::new();
                for event_id in &event_ids {
                    let mut rows = conn
                        .query(
                            "select event_id, sink, event_type, payload_json, created_at, delivered_at
                         from outbox_events
                         where event_id = ?1 and delivered_at is null",
                            (event_id.as_str(),),
                        )
                        .await
                        .map_err(turso_error)?;
                    let Some(row) = rows.next().await.map_err(turso_error)? else {
                        continue;
                    };
                    let event = outbox_event_from_row(&row)?;
                    event.validate_pending()?;
                    validated_event_ids.push(event_id);
                }
                let mut delivered = 0usize;
                for event_id in validated_event_ids {
                    let changed = conn
                        .execute(
                            "update outbox_events
                         set delivered_at = ?2
                         where event_id = ?1 and delivered_at is null",
                            (event_id.as_str(), delivered_at.as_str()),
                        )
                        .await
                        .map_err(turso_error)?;
                    delivered += changed as usize;
                }
                Ok(delivered)
            })
        })
        .await
    }

    async fn upsert_storage_profile(
        &self,
        profile: StorageProfile,
    ) -> LakeCatResult<StorageProfile> {
        profile.validate()?;
        self.write_txn(move |conn| {
            let profile = profile.clone();
            Box::pin(async move {
        let profile_key = storage_profile_key(&profile.warehouse, &profile.profile_id);
        let mut rows = conn
                .query(
                    "select profile_json, profile_key, profile_id, location_prefix, provider, issuance_mode
                     from storage_profiles
                     where profile_key = ?1",
                    (profile_key.as_str(),),
                )
                .await
                .map_err(turso_error)?;
        if let Some(row) = rows.next().await.map_err(turso_error)? {
            let existing: StorageProfile = decode_json(row_string(&row, 0)?)?;
            crate::validate_storage_profile_scope(
                &existing,
                &profile.warehouse,
                &row_string(&row, 1)?,
                &row_string(&row, 2)?,
                &row_string(&row, 3)?,
                &row_string(&row, 4)?,
                &row_string(&row, 5)?,
            )?;
        }
        conn.execute(
            "insert into storage_profiles (
                    profile_key, profile_id, warehouse, location_prefix,
                    provider, issuance_mode, profile_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 on conflict(profile_key) do update set
                    location_prefix = excluded.location_prefix,
                    provider = excluded.provider,
                    issuance_mode = excluded.issuance_mode,
                    profile_json = excluded.profile_json,
                    updated_at = excluded.updated_at",
            (
                profile_key.as_str(),
                profile.profile_id.as_str(),
                profile.warehouse.as_str(),
                profile.location_prefix.as_str(),
                profile.provider.as_str(),
                profile.issuance_mode.as_str(),
                encode_json(&profile)?,
                Utc::now().to_rfc3339(),
            ),
        )
        .await
        .map_err(turso_error)?;
        Ok(profile)
            })
        })
        .await
    }

    async fn list_storage_profiles(
        &self,
        warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<StorageProfile>> {
        let conn = self.connect()?;
        let mut rows = conn
                .query(
                    "select profile_json, profile_key, profile_id, location_prefix, provider, issuance_mode
                     from storage_profiles
                     where warehouse = ?1
                     order by profile_id",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
        let mut profiles = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let profile: StorageProfile = decode_json(row_string(&row, 0)?)?;
            crate::validate_storage_profile_scope(
                &profile,
                warehouse,
                &row_string(&row, 1)?,
                &row_string(&row, 2)?,
                &row_string(&row, 3)?,
                &row_string(&row, 4)?,
                &row_string(&row, 5)?,
            )?;
            profiles.push(profile);
        }
        Ok(profiles)
    }

    async fn storage_profile_for_table(
        &self,
        table: &TableRecord,
    ) -> LakeCatResult<StorageProfile> {
        let profiles = self.list_storage_profiles(&table.ident.warehouse).await?;
        Ok(storage_profile_match(profiles.iter(), table)?
            .unwrap_or_else(|| StorageProfile::inferred_for_table(table)))
    }

    async fn upsert_policy_binding(&self, binding: PolicyBinding) -> LakeCatResult<PolicyBinding> {
        binding.validate()?;
        self.write_txn(move |conn| {
            let binding = binding.clone();
            Box::pin(async move {
                let policy_key = policy_binding_key(&binding.warehouse, &binding.policy_id);
                let mut rows = conn
                    .query(
                        "select binding_json, policy_id, namespace_path, table_name, enforced
                     from policy_bindings
                     where policy_key = ?1",
                        (policy_key.as_str(),),
                    )
                    .await
                    .map_err(turso_error)?;
                if let Some(row) = rows.next().await.map_err(turso_error)? {
                    let existing: PolicyBinding = decode_json(row_string(&row, 0)?)?;
                    crate::validate_policy_binding_scope(
                        &existing,
                        &binding.warehouse,
                        row_string(&row, 1)?.as_str(),
                        row_optional_string(&row, 2)?.as_deref(),
                        row_optional_string(&row, 3)?.as_deref(),
                        row_i64(&row, 4)? != 0,
                    )?;
                }
                conn.execute(
                    "insert into policy_bindings (
                    policy_key, policy_id, warehouse, namespace_path, table_name,
                    enforced, binding_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 on conflict(policy_key) do update set
                    namespace_path = excluded.namespace_path,
                    table_name = excluded.table_name,
                    enforced = excluded.enforced,
                    binding_json = excluded.binding_json,
                    updated_at = excluded.updated_at",
                    (
                        policy_key.as_str(),
                        binding.policy_id.as_str(),
                        binding.warehouse.as_str(),
                        binding.namespace.as_ref().map(Namespace::path),
                        binding.table.as_ref().map(TableName::as_str),
                        if binding.enforced { 1_i64 } else { 0_i64 },
                        encode_json(&binding)?,
                        binding.updated_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
                Ok(binding)
            })
        })
        .await
    }

    async fn list_policy_bindings(
        &self,
        warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "select binding_json, policy_id, namespace_path, table_name, enforced
                     from policy_bindings
                     where warehouse = ?1
                     order by policy_id",
                (warehouse.as_str(),),
            )
            .await
            .map_err(turso_error)?;
        let mut bindings = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let binding: PolicyBinding = decode_json(row_string(&row, 0)?)?;
            crate::validate_policy_binding_scope(
                &binding,
                warehouse,
                row_string(&row, 1)?.as_str(),
                row_optional_string(&row, 2)?.as_deref(),
                row_optional_string(&row, 3)?.as_deref(),
                row_i64(&row, 4)? != 0,
            )?;
            bindings.push(binding);
        }
        Ok(bindings)
    }

    async fn policy_bindings_for_table(
        &self,
        table: &TableIdent,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        let bindings = self.list_policy_bindings(&table.warehouse).await?;
        Ok(policy_bindings_for_table(bindings.iter(), table))
    }
}

const TURSO_MIGRATION: &[&str] = &[
    "create table if not exists servers (
            server_id text primary key,
            display_name text,
            endpoint_url text,
            record_json text not null,
            updated_at text not null
        )",
    "create table if not exists projects (
            project_id text primary key,
            display_name text,
            record_json text not null,
            updated_at text not null
        )",
    "create table if not exists warehouses (
            warehouse text primary key,
            project_id text not null,
            storage_root text,
            record_json text not null,
            updated_at text not null
        )",
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
    "create table if not exists storage_profiles (
            profile_key text primary key,
            profile_id text not null,
            warehouse text not null,
            location_prefix text not null,
            provider text not null,
            issuance_mode text not null,
            profile_json text not null,
            updated_at text not null
        )",
    "create index if not exists idx_storage_profiles_warehouse
            on storage_profiles (warehouse, profile_id)",
    "create table if not exists views (
            view_key text primary key,
            warehouse text not null,
            namespace_path text not null,
            view_name text not null,
            dialect text not null,
            record_json text not null,
            updated_at text not null
        )",
    "create index if not exists idx_views_warehouse_namespace
            on views (warehouse, namespace_path, view_name)",
    "create table if not exists view_version_receipts (
            receipt_id text primary key,
            view_key text not null,
            warehouse text not null,
            namespace_path text not null,
            view_name text not null,
            view_version integer not null,
            previous_view_version integer,
            operation text not null,
            view_hash text not null,
            principal_json text not null,
            receipt_json text not null,
            recorded_at text not null
        )",
    "create index if not exists idx_view_version_receipts_view
            on view_version_receipts (view_key, view_version)",
    "create table if not exists policy_bindings (
            policy_key text primary key,
            policy_id text not null,
            warehouse text not null,
            namespace_path text,
            table_name text,
            enforced integer not null,
            binding_json text not null,
            updated_at text not null
        )",
    "create index if not exists idx_policy_bindings_warehouse
            on policy_bindings (warehouse, policy_id)",
    "create table if not exists soft_deletes (
            table_key text primary key,
            warehouse text not null,
            namespace_path text not null,
            table_name text not null,
            metadata_location text,
            version integer not null,
            principal_json text not null,
            authorization_receipt_json text,
            record_json text not null,
            deleted_at text not null
        )",
    "create index if not exists idx_soft_deletes_warehouse
            on soft_deletes (warehouse, namespace_path, table_name)",
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
        TursoValue::Text(value) => Ok(value),
        value => Err(LakeCatError::Internal(format!(
            "Turso catalog store expected text at column {idx}, got {value:?}"
        ))),
    }
}

fn row_optional_string(row: &Row, idx: usize) -> LakeCatResult<Option<String>> {
    match row.get_value(idx).map_err(turso_error)? {
        TursoValue::Null => Ok(None),
        TursoValue::Text(value) => Ok(Some(value)),
        value => Err(LakeCatError::Internal(format!(
            "Turso catalog store expected nullable text at column {idx}, got {value:?}"
        ))),
    }
}

fn row_i64(row: &Row, idx: usize) -> LakeCatResult<i64> {
    match row.get_value(idx).map_err(turso_error)? {
        TursoValue::Integer(value) => Ok(value),
        value => Err(LakeCatError::Internal(format!(
            "Turso catalog store expected integer at column {idx}, got {value:?}"
        ))),
    }
}

async fn latest_turso_view_receipt_evidence(
    conn: &Connection,
    view_key: &str,
    warehouse: &WarehouseName,
    namespace: &Namespace,
    view: &TableName,
) -> LakeCatResult<Option<(u64, String)>> {
    let mut rows = conn
        .query(
            "select receipt_json, warehouse, namespace_path, view_name from view_version_receipts
             where view_key = ?1
             order by view_version, recorded_at, receipt_id",
            (view_key,),
        )
        .await
        .map_err(turso_error)?;
    let mut receipts = Vec::new();
    while let Some(row) = rows.next().await.map_err(turso_error)? {
        let receipt = decode_json::<ViewVersionReceipt>(row_string(&row, 0)?)?;
        let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
        let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
        let row_view = TableName::new(row_string(&row, 3)?)?;
        crate::validate_view_receipt_scope(
            &receipt,
            &row_warehouse,
            &row_namespace,
            Some(&row_view),
        )?;
        crate::validate_view_receipt_scope(&receipt, warehouse, namespace, Some(view))?;
        receipts.push(receipt);
    }
    crate::latest_view_receipt_evidence(receipts.iter())
}

async fn latest_turso_view_receipt_hash(
    conn: &Connection,
    view_key: &str,
    warehouse: &WarehouseName,
    namespace: &Namespace,
    view: &TableName,
) -> LakeCatResult<Option<String>> {
    latest_turso_view_receipt_evidence(conn, view_key, warehouse, namespace, view)
        .await
        .map(|evidence| evidence.map(|(_, hash)| hash))
}

async fn count_matching_rows(
    conn: &Connection,
    table: &str,
    warehouse: &str,
    namespace_path: &str,
) -> LakeCatResult<i64> {
    let sql = match table {
        "tables" => "select count(*) from tables where warehouse = ?1 and namespace_path = ?2",
        "views" => "select count(*) from views where warehouse = ?1 and namespace_path = ?2",
        "policy_bindings" => {
            "select count(*) from policy_bindings where warehouse = ?1 and namespace_path = ?2"
        }
        table => {
            return Err(LakeCatError::Internal(format!(
                "unsupported Turso count table: {table}"
            )));
        }
    };
    let mut rows = conn
        .query(sql, (warehouse, namespace_path))
        .await
        .map_err(turso_error)?;
    let row = rows.next().await.map_err(turso_error)?.ok_or_else(|| {
        LakeCatError::Internal(format!("Turso catalog store returned no count for {table}"))
    })?;
    row_i64(&row, 0)
}

fn outbox_event_from_row(row: &Row) -> LakeCatResult<OutboxEvent> {
    Ok(OutboxEvent {
        event_id: row_string(row, 0)?,
        sink: row_string(row, 1)?,
        event_type: row_string(row, 2)?,
        payload: decode_json::<JsonValue>(row_string(row, 3)?)?,
        created_at: parse_turso_datetime(row_string(row, 4)?, "outbox created_at")?,
        delivered_at: row_optional_string(row, 5)?
            .map(|value| parse_turso_datetime(value, "outbox delivered_at"))
            .transpose()?,
    })
}

fn validate_turso_commit_record_row(
    record: &TableCommitRecord,
    ident: &TableIdent,
    row: &Row,
) -> LakeCatResult<()> {
    if row_string(row, 0)? != table_key(ident) {
        return Err(LakeCatError::Internal(
            "table commit record row scope does not match requested table".to_string(),
        ));
    }
    let row_sequence_number = u64::try_from(row_i64(row, 1)?).map_err(|_| {
        LakeCatError::Internal(
            "Turso metadata pointer log sequence number must be positive".to_string(),
        )
    })?;
    if record.sequence_number != row_sequence_number {
        return Err(LakeCatError::Internal(
            "table commit record sequence number does not match pointer log row".to_string(),
        ));
    }
    if record.previous_metadata_location != row_optional_string(row, 2)? {
        return Err(LakeCatError::Internal(
            "table commit record previous metadata location does not match pointer log row"
                .to_string(),
        ));
    }
    if record.new_metadata_location != row_optional_string(row, 3)? {
        return Err(LakeCatError::Internal(
            "table commit record new metadata location does not match pointer log row".to_string(),
        ));
    }
    if record.request_hash != row_string(row, 4)? {
        return Err(LakeCatError::Internal(
            "table commit record request hash does not match pointer log row".to_string(),
        ));
    }
    if record.principal != decode_json::<Principal>(row_string(row, 5)?)? {
        return Err(LakeCatError::Internal(
            "table commit record principal does not match pointer log row".to_string(),
        ));
    }
    if record.committed_at != parse_turso_datetime(row_string(row, 6)?, "commit committed_at")? {
        return Err(LakeCatError::Internal(
            "table commit record timestamp does not match pointer log row".to_string(),
        ));
    }
    Ok(())
}

fn validate_turso_soft_delete_row(
    record: &SoftDeleteRecord,
    ident: &TableIdent,
    row: &Row,
    offset: usize,
) -> LakeCatResult<()> {
    if row_string(row, offset)? != table_key(ident)
        || row_string(row, offset + 1)? != record.table.warehouse.as_str()
        || row_string(row, offset + 2)? != record.table.namespace.path()
        || row_string(row, offset + 3)? != record.table.name.as_str()
    {
        return Err(LakeCatError::Internal(
            "soft-delete row scope does not match record identity".to_string(),
        ));
    }
    if record.metadata_location != row_optional_string(row, offset + 4)? {
        return Err(LakeCatError::Internal(
            "soft-delete metadata location does not match row".to_string(),
        ));
    }
    let row_version = u64::try_from(row_i64(row, offset + 5)?).map_err(|_| {
        LakeCatError::Internal("soft-delete row version must be non-negative".to_string())
    })?;
    if record.version != row_version {
        return Err(LakeCatError::Internal(
            "soft-delete version does not match row".to_string(),
        ));
    }
    if record.deleted_at
        != parse_turso_datetime(row_string(row, offset + 6)?, "soft-delete deleted_at")?
    {
        return Err(LakeCatError::Internal(
            "soft-delete timestamp does not match row".to_string(),
        ));
    }
    Ok(())
}

fn parse_turso_datetime(value: String, name: &str) -> LakeCatResult<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(&value)
        .map(|datetime| datetime.with_timezone(&Utc))
        .map_err(|err| LakeCatError::Internal(format!("failed to parse {name} timestamp: {err}")))
}

async fn insert_outbox_event(
    conn: &Connection,
    payload: &JsonValue,
    created_at: chrono::DateTime<Utc>,
) -> LakeCatResult<()> {
    conn.execute(
        "insert into outbox_events (
                event_id, sink, event_type, payload_json, created_at
             )
             values (?1, ?2, ?3, ?4, ?5)",
        (
            content_hash_json(payload)?,
            "lakecat.lineage-and-graph",
            payload["event-type"].as_str(),
            encode_json(payload)?,
            created_at.to_rfc3339(),
        ),
    )
    .await
    .map_err(turso_error)?;
    Ok(())
}

fn is_unique_violation(err: &turso::Error) -> bool {
    matches!(err, turso::Error::Constraint(message) if message.contains("UNIQUE") || message.contains("PRIMARY KEY"))
}

fn turso_error(err: turso::Error) -> LakeCatError {
    LakeCatError::Internal(format!("Turso catalog store error: {err}"))
}

/// The future returned by a [`TursoCatalogStore::write_txn`] body for a given
/// connection borrow `'c`. Boxed + `Send` so the retry loop can re-invoke the
/// body and the enclosing `#[async_trait]` method future stays `Send`.
type WriteTxnFuture<'c, T> = Pin<Box<dyn Future<Output = LakeCatResult<T>> + Send + 'c>>;

/// Bounded attempts for a write transaction that loses an MVCC write-write
/// conflict (or hits a transient `Busy`) at commit. This only bounds livelock:
/// a genuine same-row logical race converges to the metadata-pointer CAS
/// `Conflict` within a couple of attempts.
const WRITE_TXN_MAX_ATTEMPTS: u32 = 8;

/// Maximum idle write connections retained in the pool. Caps memory/file handles
/// while comfortably covering the expected concurrent-writer count; connections
/// beyond this are dropped on return rather than pooled.
const WRITE_POOL_MAX_IDLE: usize = 16;

/// Best-effort per-connection pragmas for write transactions. `journal_mode`
/// returns a row, so `execute_batch` reports it as an error even though the mode
/// is applied; ignore it (as the prior WAL path always did). `busy_timeout`
/// bounds how long a connection waits on a contended page before erroring.
async fn apply_write_pragmas(conn: &Connection) {
    let _ = conn.execute_batch("PRAGMA journal_mode=mvcc;").await;
    let _ = conn.execute_batch("PRAGMA busy_timeout=10000;").await;
}

/// True for the raw `turso::Error`s that a `BEGIN CONCURRENT` commit raises when
/// it loses an MVCC write-write race or hits transient contention — all safe to
/// retry on a fresh snapshot.
fn is_retryable_conflict(err: &turso::Error) -> bool {
    matches!(err, turso::Error::Busy(_) | turso::Error::BusySnapshot(_))
        || matches!(
            err,
            turso::Error::Error(message)
                if message.contains("Write-write conflict")
                    || message.contains("Commit dependency aborted")
        )
}

/// True for a `LakeCatError` that wraps a retryable MVCC conflict surfaced mid-
/// body (every error goes through `turso_error` -> `Internal`, so match on the
/// preserved Display text). The unique-violation `Conflict` is deliberately NOT
/// matched — it is a terminal logical conflict, not a retryable race.
fn is_retryable_lakecat(err: &LakeCatError) -> bool {
    matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("Write-write conflict")
                || message.contains("Commit dependency aborted")
    )
}

async fn backoff(attempt: u32) {
    // Exponential and capped: 2, 4, 8, ... 64ms. Small enough for tests, enough
    // to de-correlate retries of a genuinely contended commit.
    let millis = 1u64 << attempt.min(6);
    tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
}

#[cfg(test)]
mod tests;
