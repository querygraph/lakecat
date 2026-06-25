use std::collections::BTreeMap;

use lakecat_core::{AuditStamp, Principal, TableName};

use crate::{
    CredentialIssuanceMode, MemoryCatalogStore, PolicyBinding, ServerRecord, StorageProvider,
    ViewColumnRecord, ViewRecord, ViewVersionOperation,
};

use super::*;

#[tokio::test]
async fn turso_store_persists_server_records() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    assert_eq!(store.list_servers().await.unwrap(), vec![]);

    let record = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        Some("http://127.0.0.1:8181".to_string()),
        BTreeMap::from([("deployment".to_string(), "local".to_string())]),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(record).await.unwrap();

    let updated = ServerRecord::new(
        "lakecat-local",
        Some("Local QueryGraph LakeCat".to_string()),
        Some("http://127.0.0.1:8182".to_string()),
        BTreeMap::from([("deployment".to_string(), "dev".to_string())]),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(updated.clone()).await.unwrap();

    assert_eq!(store.list_servers().await.unwrap(), vec![updated]);
}

#[tokio::test]
async fn turso_store_rejects_corrupt_server_records_on_read() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let mut record = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        Some("http://127.0.0.1:8181".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(record.clone()).await.unwrap();
    record.endpoint_url = Some("http://127.0.0.1:8181?token=secret".to_string());

    let conn = store.connect().unwrap();
    conn.execute(
        "update servers set record_json = ?2 where server_id = ?1",
        ("lakecat-local", encode_json(&record).unwrap()),
    )
    .await
    .unwrap();

    let err = store.list_servers().await.unwrap_err();
    let message = err.to_string();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("server endpoint URL")
                || message.contains("server-endpoint-url-hash=sha256:")
    ));
    assert!(!message.contains("token=secret"));
}

#[tokio::test]
async fn turso_store_rejects_server_record_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let mut record = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        Some("http://127.0.0.1:8181".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(record.clone()).await.unwrap();
    record.server_id = "lakecat-other".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update servers set record_json = ?2 where server_id = ?1",
        ("lakecat-local", encode_json(&record).unwrap()),
    )
    .await
    .unwrap();

    let err = store.list_servers().await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("server row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_server_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let record = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        Some("http://127.0.0.1:8181".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(record).await.unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update servers set server_id = ?2 where server_id = ?1",
        ("lakecat-local", "lakecat-other"),
    )
    .await
    .unwrap();

    let err = store.list_servers().await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
        if message.contains("server row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_server_scope_drift_before_upsert() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let mut record = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        Some("http://127.0.0.1:8181".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(record.clone()).await.unwrap();
    record.server_id = "lakecat-other".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update servers set record_json = ?2 where server_id = ?1",
        ("lakecat-local", encode_json(&record).unwrap()),
    )
    .await
    .unwrap();

    let replacement = ServerRecord::new(
        "lakecat-local",
        Some("Updated LakeCat".to_string()),
        Some("http://127.0.0.1:8182".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store.upsert_server(replacement).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("server row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_persists_warehouse_records() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    assert_eq!(store.list_warehouses().await.unwrap(), vec![]);
    let project = ProjectRecord::new(
        "default",
        None,
        Some("Default Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();

    let warehouse = WarehouseName::new("local").unwrap();
    let record = WarehouseRecord::new(
        warehouse.clone(),
        "default",
        Some("file:///tmp/lakecat".to_string()),
        BTreeMap::from([("region".to_string(), "local".to_string())]),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(record).await.unwrap();

    let updated = WarehouseRecord::new(
        warehouse.clone(),
        "default",
        Some("file:///tmp/lakecat-updated".to_string()),
        BTreeMap::from([("region".to_string(), "test".to_string())]),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(updated.clone()).await.unwrap();

    assert_eq!(store.load_warehouse(&warehouse).await.unwrap(), updated);
    assert!(matches!(
        store
            .load_warehouse(&WarehouseName::new("missing").unwrap())
            .await,
        Err(LakeCatError::NotFound { object, name })
            if object == "warehouse" && name == "missing"
    ));
    assert_eq!(
        store.list_warehouses().await.unwrap(),
        vec![updated.clone()]
    );
    assert_eq!(
        store.list_project_warehouses("default").await.unwrap(),
        vec![updated.clone()]
    );
    assert!(matches!(
        store.list_project_warehouses("missing-project").await,
        Err(LakeCatError::NotFound { object, name })
            if object == "project" && name == "missing-project"
    ));

    let missing_project = WarehouseRecord::new(
        WarehouseName::new("orphaned").unwrap(),
        "missing-project",
        Some("file:///tmp/orphaned".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    assert!(matches!(
        store.upsert_warehouse(missing_project).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "project" && name == "missing-project"
    ));
}

#[tokio::test]
async fn turso_store_rejects_corrupt_project_parent_before_warehouse_upsert() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let mut project = ProjectRecord::new(
        "default",
        None,
        Some("Default Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project.clone()).await.unwrap();
    project.project_id = "other-project".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update projects set record_json = ?2 where project_id = ?1",
        ("default", encode_json(&project).unwrap()),
    )
    .await
    .unwrap();

    let warehouse = WarehouseRecord::new(
        WarehouseName::new("local").unwrap(),
        "default",
        Some("file:///tmp/lakecat".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store.upsert_warehouse(warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("project row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_corrupt_warehouse_records_on_read() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let project = ProjectRecord::new(
        "default",
        None,
        Some("Default Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let mut record = WarehouseRecord::new(
        warehouse.clone(),
        "default",
        Some("file:///tmp/lakecat".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(record.clone()).await.unwrap();
    record.storage_root = Some("file:///tmp/lakecat?token=secret".to_string());

    let conn = store.connect().unwrap();
    conn.execute(
        "update warehouses set record_json = ?2 where warehouse = ?1",
        ("local", encode_json(&record).unwrap()),
    )
    .await
    .unwrap();

    let err = store.load_warehouse(&warehouse).await.unwrap_err();
    let message = err.to_string();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("warehouse storage root")
                || message.contains("warehouse-storage-root-hash=sha256:")
    ));
    assert!(!message.contains("token=secret"));

    let err = store.list_warehouses().await.unwrap_err();
    assert!(
        err.to_string()
            .contains("warehouse-storage-root-hash=sha256:")
    );
}

#[tokio::test]
async fn turso_store_rejects_warehouse_record_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let project = ProjectRecord::new(
        "default",
        None,
        Some("Default Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let mut record = WarehouseRecord::new(
        warehouse.clone(),
        "default",
        Some("file:///tmp/lakecat".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(record.clone()).await.unwrap();
    record.warehouse = WarehouseName::new("other").unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update warehouses set record_json = ?2 where warehouse = ?1",
        ("local", encode_json(&record).unwrap()),
    )
    .await
    .unwrap();

    for err in [
        store.load_warehouse(&warehouse).await.unwrap_err(),
        store.list_warehouses().await.unwrap_err(),
        store.list_project_warehouses("default").await.unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("warehouse row scope does not match")
        ));
    }
}

#[tokio::test]
async fn turso_store_rejects_warehouse_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    store
        .upsert_project(
            ProjectRecord::new(
                "default",
                None,
                Some("Default Project".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    store
        .upsert_project(
            ProjectRecord::new(
                "other-project",
                None,
                Some("Other Project".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let record = WarehouseRecord::new(
        warehouse.clone(),
        "default",
        Some("file:///tmp/lakecat".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(record).await.unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update warehouses
                 set project_id = ?2, storage_root = ?3
                 where warehouse = ?1",
        ("local", "other-project", "file:///tmp/other-lakecat"),
    )
    .await
    .unwrap();

    for err in [
        store.load_warehouse(&warehouse).await.unwrap_err(),
        store.list_warehouses().await.unwrap_err(),
        store
            .list_project_warehouses("other-project")
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("warehouse row scope does not match")
        ));
    }
}

#[tokio::test]
async fn turso_store_rejects_warehouse_scope_drift_before_upsert() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let project = ProjectRecord::new(
        "default",
        None,
        Some("Default Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let mut record = WarehouseRecord::new(
        warehouse.clone(),
        "default",
        Some("file:///tmp/lakecat".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(record.clone()).await.unwrap();
    record.warehouse = WarehouseName::new("other").unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update warehouses set record_json = ?2 where warehouse = ?1",
        ("local", encode_json(&record).unwrap()),
    )
    .await
    .unwrap();

    let replacement = WarehouseRecord::new(
        warehouse,
        "default",
        Some("file:///tmp/lakecat-updated".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store.upsert_warehouse(replacement).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("warehouse row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_persists_project_records() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    assert_eq!(store.list_projects().await.unwrap(), vec![]);

    let record = ProjectRecord::new(
        "default",
        Some("lakecat-local".to_string()),
        Some("Default Project".to_string()),
        BTreeMap::from([("owner".to_string(), "querygraph".to_string())]),
        Principal::anonymous(),
    )
    .unwrap();
    store
        .upsert_server(
            ServerRecord::new(
                "lakecat-local",
                Some("Local LakeCat".to_string()),
                None,
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    store.upsert_project(record).await.unwrap();

    let updated = ProjectRecord::new(
        "default",
        Some("lakecat-local".to_string()),
        Some("QueryGraph Project".to_string()),
        BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(updated.clone()).await.unwrap();

    assert_eq!(store.list_projects().await.unwrap(), vec![updated]);

    let missing_server = ProjectRecord::new(
        "orphaned",
        Some("missing-server".to_string()),
        Some("Orphaned Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    assert!(matches!(
        store.upsert_project(missing_server).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "server" && name == "missing-server"
    ));
}

#[tokio::test]
async fn turso_store_rejects_corrupt_server_parent_before_project_upsert() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let mut server = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        None,
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(server.clone()).await.unwrap();
    server.server_id = "lakecat-other".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update servers set record_json = ?2 where server_id = ?1",
        ("lakecat-local", encode_json(&server).unwrap()),
    )
    .await
    .unwrap();

    let project = ProjectRecord::new(
        "default",
        Some("lakecat-local".to_string()),
        Some("QueryGraph Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store.upsert_project(project).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("server row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_corrupt_project_records_on_read() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    store
        .upsert_server(
            ServerRecord::new(
                "lakecat-local",
                Some("Local LakeCat".to_string()),
                None,
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let mut project = ProjectRecord::new(
        "default",
        Some("lakecat-local".to_string()),
        Some("QueryGraph Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project.clone()).await.unwrap();
    project.server_id = Some("lakecat-local?token=secret".to_string());

    let conn = store.connect().unwrap();
    conn.execute(
        "update projects set record_json = ?2 where project_id = ?1",
        ("default", encode_json(&project).unwrap()),
    )
    .await
    .unwrap();

    let err = store.list_projects().await.unwrap_err();
    let message = err.to_string();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("project") || message.contains("identifier")
    ));
    assert!(!message.contains("token=secret"));
}

#[tokio::test]
async fn turso_store_rejects_project_record_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let mut project = ProjectRecord::new(
        "default",
        None,
        Some("QueryGraph Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project.clone()).await.unwrap();
    project.project_id = "other-project".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update projects set record_json = ?2 where project_id = ?1",
        ("default", encode_json(&project).unwrap()),
    )
    .await
    .unwrap();

    let err = store.list_projects().await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("project row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_project_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let project = ProjectRecord::new(
        "default",
        None,
        Some("QueryGraph Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update projects set project_id = ?2 where project_id = ?1",
        ("default", "other-project"),
    )
    .await
    .unwrap();

    let err = store.list_projects().await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
        if message.contains("project row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_project_scope_drift_before_upsert() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let mut project = ProjectRecord::new(
        "default",
        None,
        Some("QueryGraph Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project.clone()).await.unwrap();
    project.project_id = "other-project".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update projects set record_json = ?2 where project_id = ?1",
        ("default", encode_json(&project).unwrap()),
    )
    .await
    .unwrap();

    let replacement = ProjectRecord::new(
        "default",
        None,
        Some("Updated Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store.upsert_project(replacement).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("project row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_persists_view_records() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    assert_eq!(
        store.list_views(&warehouse, &namespace).await.unwrap(),
        vec![]
    );

    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("active_customers").unwrap(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::from([("owner".to_string(), "querygraph".to_string())]),
        Principal::anonymous(),
    )
    .unwrap();
    let view = store.upsert_view(view).await.unwrap();
    assert_eq!(view.view_version, 1);

    let updated = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("active_customers").unwrap(),
        "select id, email from customers where active",
        "sql",
        Some(2),
        BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
        Principal::anonymous(),
    )
    .unwrap()
    .with_columns(vec![ViewColumnRecord {
        name: "id".to_string(),
        data_type: serde_json::json!("long"),
        nullable: false,
        comment: Some("Customer identifier".to_string()),
    }])
    .unwrap();
    let updated = store
        .upsert_view_if_version(updated, Some(1))
        .await
        .unwrap();
    assert_eq!(updated.view_version, 2);
    let stale = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("active_customers").unwrap(),
        "select id from customers where active",
        "sql",
        Some(3),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store
        .upsert_view_if_version(stale, Some(1))
        .await
        .expect_err("stale expected view version must conflict");
    assert!(matches!(
        err,
        LakeCatError::Conflict(message) if message.contains("expected version 1")
    ));
    let receipts = store
        .list_view_version_receipts(
            &warehouse,
            &namespace,
            &TableName::new("active_customers").unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(
        receipts[0].stable_id,
        "lakecat:view:local:default:active_customers"
    );
    assert_eq!(receipts[0].view_version, 1);
    assert_eq!(receipts[0].previous_view_version, None);
    assert_eq!(receipts[0].previous_receipt_hash, None);
    assert_eq!(receipts[0].operation, ViewVersionOperation::Upsert);
    assert!(!receipts[0].view_hash.is_empty());
    let first_receipt_hash = view_receipt_hash(&receipts[0]).unwrap();
    assert_eq!(receipts[1].view_version, 2);
    assert_eq!(receipts[1].previous_view_version, Some(1));
    assert_eq!(
        receipts[1].previous_receipt_hash.as_deref(),
        Some(first_receipt_hash.as_str())
    );
    assert_ne!(receipts[0].view_hash, receipts[1].view_hash);
    let second_receipt_hash = view_receipt_hash(&receipts[1]).unwrap();

    assert_eq!(
        store
            .load_view(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap()
            )
            .await
            .unwrap(),
        updated.clone()
    );
    assert!(matches!(
        store
            .load_view(&warehouse, &namespace, &TableName::new("missing_view").unwrap())
            .await,
        Err(LakeCatError::NotFound { object, name })
            if object == "view" && name == "missing_view"
    ));
    assert_eq!(
        store.list_views(&warehouse, &namespace).await.unwrap(),
        vec![updated.clone()]
    );
    let err = store
        .drop_view_if_version(
            &warehouse,
            &namespace,
            &TableName::new("active_customers").unwrap(),
            Principal::anonymous(),
            Some(1),
        )
        .await
        .expect_err("stale expected view version must not drop the view");
    assert!(matches!(
        err,
        LakeCatError::Conflict(message) if message.contains("expected version 1")
    ));
    assert_eq!(
        store
            .list_view_version_receipts(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap(),
            )
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        store
            .drop_view_if_version(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap(),
                Principal::anonymous(),
                Some(2)
            )
            .await
            .unwrap(),
        updated
    );
    let receipts = store
        .list_view_version_receipts(
            &warehouse,
            &namespace,
            &TableName::new("active_customers").unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(receipts.len(), 3);
    assert_eq!(receipts[2].stable_id, receipts[1].stable_id);
    assert_eq!(receipts[2].view_version, 2);
    assert_eq!(receipts[2].previous_view_version, Some(2));
    assert_eq!(
        receipts[2].previous_receipt_hash.as_deref(),
        Some(second_receipt_hash.as_str())
    );
    assert_eq!(receipts[2].operation, ViewVersionOperation::Drop);
    assert_eq!(receipts[2].view_hash, receipts[1].view_hash);
    let namespace_receipts = store
        .list_namespace_view_version_receipts(&warehouse, &namespace)
        .await
        .unwrap();
    assert_eq!(namespace_receipts, receipts);
    assert_eq!(
        store.list_views(&warehouse, &namespace).await.unwrap(),
        Vec::<ViewRecord>::new()
    );
    assert!(matches!(
        store
            .drop_view(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap(),
                Principal::anonymous()
            )
            .await,
        Err(LakeCatError::NotFound { object, name })
            if object == "view" && name == "active_customers"
    ));
    let recreated = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("active_customers").unwrap(),
        "select id from customers where active",
        "sql",
        Some(3),
        BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
        Principal::anonymous(),
    )
    .unwrap();
    let recreated = store.upsert_view(recreated).await.unwrap();
    assert_eq!(recreated.view_version, 3);
    let receipts = store
        .list_view_version_receipts(
            &warehouse,
            &namespace,
            &TableName::new("active_customers").unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(receipts.len(), 4);
    let drop_receipt_hash = view_receipt_hash(&receipts[2]).unwrap();
    assert_eq!(receipts[3].stable_id, receipts[2].stable_id);
    assert_eq!(receipts[3].view_version, 3);
    assert_eq!(receipts[3].previous_view_version, Some(2));
    assert_eq!(
        receipts[3].previous_receipt_hash.as_deref(),
        Some(drop_receipt_hash.as_str())
    );
    assert_eq!(receipts[3].operation, ViewVersionOperation::Upsert);
    assert_ne!(receipts[3].view_hash, receipts[2].view_hash);
    assert_eq!(
        store
            .load_view(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap()
            )
            .await
            .unwrap(),
        recreated
    );
}

#[tokio::test]
async fn turso_store_rejects_corrupt_view_records_on_read() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let view_name = TableName::new("active_customers").unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let mut view = store.upsert_view(view).await.unwrap();
    view.sql = "   ".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update views set record_json = ?2 where view_key = ?1",
        (view_key(&view), encode_json(&view).unwrap()),
    )
    .await
    .unwrap();

    let err = store
        .load_view(&warehouse, &namespace, &view_name)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("view SQL must not be empty")
    ));

    let err = store.list_views(&warehouse, &namespace).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("view SQL must not be empty")
    ));

    let err = store
        .drop_view(&warehouse, &namespace, &view_name, Principal::anonymous())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("view SQL must not be empty")
    ));
}

#[tokio::test]
async fn turso_store_rejects_view_record_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let view_name = TableName::new("active_customers").unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let mut view = store.upsert_view(view).await.unwrap();
    let original_view_key = view_key(&view);
    view.name = TableName::new("other_view").unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update views set record_json = ?2 where view_key = ?1",
        (original_view_key.as_str(), encode_json(&view).unwrap()),
    )
    .await
    .unwrap();

    let replacement = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id from customers where active",
        "sql",
        Some(2),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    for err in [
        store
            .load_view(&warehouse, &namespace, &view_name)
            .await
            .unwrap_err(),
        store.list_views(&warehouse, &namespace).await.unwrap_err(),
        store
            .upsert_view_if_version(replacement, Some(1))
            .await
            .unwrap_err(),
        store
            .drop_view(&warehouse, &namespace, &view_name, Principal::anonymous())
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("view record row scope does not match")
        ));
    }
}

#[tokio::test]
async fn turso_store_rejects_view_record_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let view_name = TableName::new("active_customers").unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let view = store.upsert_view(view).await.unwrap();
    let original_view_key = view_key(&view);

    let conn = store.connect().unwrap();
    conn.execute(
        "update views
                 set namespace_path = ?2, view_name = ?3
                 where view_key = ?1",
        (
            original_view_key.as_str(),
            "tenant_shadow",
            "shadow_active_customers",
        ),
    )
    .await
    .unwrap();

    let replacement = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id from customers where active",
        "sql",
        Some(2),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    for err in [
        store
            .load_view(&warehouse, &namespace, &view_name)
            .await
            .unwrap_err(),
        store
            .list_views(&warehouse, &"tenant_shadow".parse::<Namespace>().unwrap())
            .await
            .unwrap_err(),
        store
            .upsert_view_if_version(replacement, Some(1))
            .await
            .unwrap_err(),
        store
            .drop_view(&warehouse, &namespace, &view_name, Principal::anonymous())
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("view record row scope does not match")
        ));
    }
}

#[tokio::test]
async fn turso_store_rejects_corrupt_view_receipts_on_read() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let view_name = TableName::new("active_customers").unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_view(view).await.unwrap();

    let mut receipts = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap();
    let receipt_id = view_receipt_hash(&receipts[0]).unwrap();
    receipts[0].view_hash = "sha256:short".to_string();
    let conn = store.connect().unwrap();
    conn.execute(
        "update view_version_receipts set receipt_json = ?2 where receipt_id = ?1",
        (receipt_id, encode_json(&receipts[0]).unwrap()),
    )
    .await
    .unwrap();

    let err = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt hash must be a SHA-256 digest")
    ));

    let err = store
        .list_namespace_view_version_receipts(&warehouse, &namespace)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt hash must be a SHA-256 digest")
    ));
}

#[tokio::test]
async fn turso_store_rejects_corrupt_view_receipt_chain_links_on_read() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let view_name = TableName::new("active_customers").unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_view(view).await.unwrap();
    let updated = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id from customers where active",
        "sql",
        Some(2),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store
        .upsert_view_if_version(updated, Some(1))
        .await
        .unwrap();

    let mut receipts = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap();
    let receipt_id = view_receipt_hash(&receipts[1]).unwrap();
    receipts[1].previous_receipt_hash =
        Some(content_hash_json(&serde_json::json!({"forged": "previous"})).unwrap());
    let conn = store.connect().unwrap();
    conn.execute(
        "update view_version_receipts set receipt_json = ?2 where receipt_id = ?1",
        (receipt_id, encode_json(&receipts[1]).unwrap()),
    )
    .await
    .unwrap();

    let err = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt chain previous links must match")
    ));

    let err = store
        .list_namespace_view_version_receipts(&warehouse, &namespace)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt chain previous links must match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_corrupt_view_receipt_chain_before_mutation() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let view_name = TableName::new("active_customers").unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_view(view).await.unwrap();
    let updated = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id from customers where active",
        "sql",
        Some(2),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store
        .upsert_view_if_version(updated, Some(1))
        .await
        .unwrap();

    let mut receipts = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap();
    let receipt_id = view_receipt_hash(&receipts[1]).unwrap();
    receipts[1].previous_receipt_hash =
        Some(content_hash_json(&serde_json::json!({"forged": "previous"})).unwrap());
    let conn = store.connect().unwrap();
    conn.execute(
        "update view_version_receipts set receipt_json = ?2 where receipt_id = ?1",
        (receipt_id, encode_json(&receipts[1]).unwrap()),
    )
    .await
    .unwrap();

    let attempted = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id, email from customers where active",
        "sql",
        Some(3),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store
        .upsert_view_if_version(attempted, Some(2))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt chain previous links must match")
    ));

    let active = store
        .load_view(&warehouse, &namespace, &view_name)
        .await
        .unwrap();
    assert_eq!(active.view_version, 2);
    assert_eq!(store.count_rows("view_version_receipts").await.unwrap(), 2);
}

#[tokio::test]
async fn turso_store_rejects_view_receipt_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let view_name = TableName::new("active_customers").unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_view(view).await.unwrap();
    let updated = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id from customers where active",
        "sql",
        Some(2),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store
        .upsert_view_if_version(updated, Some(1))
        .await
        .unwrap();

    let mut receipts = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap();
    let receipt_id = view_receipt_hash(&receipts[1]).unwrap();
    let other_view_name = TableName::new("other_customers").unwrap();
    receipts[1].name = other_view_name.clone();
    receipts[1].stable_id = format!(
        "lakecat:view:{}:{}:{}",
        warehouse.as_str(),
        namespace.path(),
        other_view_name.as_str()
    );
    let conn = store.connect().unwrap();
    conn.execute(
        "update view_version_receipts set receipt_json = ?2 where receipt_id = ?1",
        (receipt_id, encode_json(&receipts[1]).unwrap()),
    )
    .await
    .unwrap();

    let err = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt row scope does not match")
    ));
    let err = store
        .list_namespace_view_version_receipts(&warehouse, &namespace)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt row scope does not match")
    ));

    let attempted = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id, email from customers where active",
        "sql",
        Some(3),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store
        .upsert_view_if_version(attempted, Some(2))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt row scope does not match")
    ));
    assert_eq!(
        store
            .load_view(&warehouse, &namespace, &view_name)
            .await
            .unwrap()
            .view_version,
        2
    );
    assert_eq!(store.count_rows("view_version_receipts").await.unwrap(), 2);
}

#[tokio::test]
async fn turso_store_rejects_view_receipt_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let shadow_namespace = "tenant_shadow".parse::<Namespace>().unwrap();
    let view_name = TableName::new("active_customers").unwrap();
    let shadow_view_name = TableName::new("shadow_active_customers").unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select * from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_view(view).await.unwrap();
    let updated = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id from customers where active",
        "sql",
        Some(2),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store
        .upsert_view_if_version(updated, Some(1))
        .await
        .unwrap();

    let receipts = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap();
    let receipt_id = view_receipt_hash(&receipts[1]).unwrap();
    let conn = store.connect().unwrap();
    conn.execute(
        "update view_version_receipts
                 set namespace_path = ?2, view_name = ?3
                 where receipt_id = ?1",
        (
            receipt_id.as_str(),
            shadow_namespace.path().as_str(),
            shadow_view_name.as_str(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt row scope does not match")
    ));
    let err = store
        .list_namespace_view_version_receipts(&warehouse, &shadow_namespace)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt row scope does not match")
    ));

    let attempted = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        view_name.clone(),
        "select id, email from customers where active",
        "sql",
        Some(3),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let err = store
        .upsert_view_if_version(attempted, Some(2))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt row scope does not match")
    ));
    assert_eq!(
        store
            .load_view(&warehouse, &namespace, &view_name)
            .await
            .unwrap()
            .view_version,
        2
    );
    assert_eq!(store.count_rows("view_version_receipts").await.unwrap(), 2);
}

#[tokio::test]
async fn turso_store_loads_and_drops_namespaces() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "empty".parse::<Namespace>().unwrap();

    assert!(matches!(
        store.load_namespace(&warehouse, &namespace).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "namespace" && name == "empty"
    ));
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    assert_eq!(
        store.load_namespace(&warehouse, &namespace).await.unwrap(),
        namespace.clone()
    );
    assert_eq!(
        store.drop_namespace(&warehouse, &namespace).await.unwrap(),
        namespace
    );
    assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);

    let occupied_namespace = "occupied".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        occupied_namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let table = TableRecord::new(
        ident,
        "file:///tmp/occupied".to_string(),
        Some("file:///tmp/occupied/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    store.create_table(table).await.unwrap();
    assert!(matches!(
        store.drop_namespace(&warehouse, &occupied_namespace).await,
        Err(LakeCatError::Conflict(message)) if message.contains("tables")
    ));
}

#[tokio::test]
async fn turso_store_rejects_namespace_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();

    let drifted = "other".parse::<Namespace>().unwrap();
    let conn = store.connect().unwrap();
    conn.execute(
        "update namespaces set namespace_json = ?3 where warehouse = ?1 and namespace_path = ?2",
        (
            warehouse.as_str(),
            namespace.path().as_str(),
            encode_json(drifted.parts()).unwrap(),
        ),
    )
    .await
    .unwrap();

    for err in [
        store.list_namespaces(&warehouse).await.unwrap_err(),
        store
            .load_namespace(&warehouse, &namespace)
            .await
            .unwrap_err(),
        store
            .drop_namespace(&warehouse, &namespace)
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("namespace row scope does not match")
        ));
    }
}

#[tokio::test]
async fn turso_store_rejects_namespace_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let shadow_namespace = "tenant_shadow".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update namespaces
                 set namespace_path = ?3
                 where warehouse = ?1 and namespace_path = ?2",
        (
            warehouse.as_str(),
            namespace.path().as_str(),
            shadow_namespace.path().as_str(),
        ),
    )
    .await
    .unwrap();

    for err in [
        store.list_namespaces(&warehouse).await.unwrap_err(),
        store
            .load_namespace(&warehouse, &shadow_namespace)
            .await
            .unwrap_err(),
        store
            .drop_namespace(&warehouse, &shadow_namespace)
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("namespace row scope does not match")
        ));
    }
}

#[tokio::test]
async fn turso_store_rejects_deserialized_empty_table_locations() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let table = TableRecord {
        ident: ident.clone(),
        location: "   ".to_string(),
        metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
        metadata: serde_json::json!({"format-version": 3}),
        created: AuditStamp::now(Principal::anonymous()),
        updated_at: Utc::now(),
        version: 0,
    };

    let err = store.create_table(table).await.unwrap_err();

    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table location must not be empty")
    ));
    assert!(matches!(
        store.load_table(&ident).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "table" && name == ident.stable_id()
    ));
    assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);
}

#[tokio::test]
async fn turso_store_rejects_deserialized_invalid_table_metadata() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let base = TableRecord {
        ident: ident.clone(),
        location: "file:///tmp/events".to_string(),
        metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
        metadata: serde_json::json!({"format-version": 3}),
        created: AuditStamp::now(Principal::anonymous()),
        updated_at: Utc::now(),
        version: 0,
    };

    let mut empty_metadata_location = base.clone();
    empty_metadata_location.metadata_location = Some("  ".to_string());
    let err = store
        .create_table(empty_metadata_location)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table metadata location must not be empty")
    ));

    let mut non_object_metadata = base;
    non_object_metadata.metadata = serde_json::json!("not metadata");
    let err = store.create_table(non_object_metadata).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table metadata must be a JSON object")
    ));

    let mut missing_format_version = TableRecord {
        ident: ident.clone(),
        location: "file:///tmp/events".to_string(),
        metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
        metadata: serde_json::json!({"current-snapshot-id": 42}),
        created: AuditStamp::now(Principal::anonymous()),
        updated_at: Utc::now(),
        version: 0,
    };
    let err = store
        .create_table(missing_format_version.clone())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table metadata format-version must be present")
    ));

    missing_format_version.metadata = serde_json::json!({"format-version": 0});
    let err = store
        .create_table(missing_format_version)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table metadata format-version must be positive")
    ));

    assert!(matches!(
        store.load_table(&ident).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "table" && name == ident.stable_id()
    ));
    assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);
}

#[tokio::test]
async fn turso_store_rejects_deserialized_invalid_table_commits() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();

    let base_commit = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "noop"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
        new_metadata: Some(serde_json::json!({"format-version": 3})),
        idempotency_key: None,
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: None,
    };

    let mut blank_idempotency_key = base_commit.clone();
    blank_idempotency_key.idempotency_key = Some("  ".to_string());
    let err = store
        .commit_table(&ident, blank_idempotency_key)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table commit idempotency key may only contain")
    ));

    let mut request_hash_without_key = base_commit.clone();
    request_hash_without_key.idempotency_request_hash =
        Some(content_hash_bytes("commit-request".as_bytes()));
    let err = store
        .commit_table(&ident, request_hash_without_key)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains(
                "table commit idempotency request hash requires an idempotency key"
            )
    ));

    let mut malformed_request_hash = base_commit.clone();
    malformed_request_hash.idempotency_key = Some("commit-1".to_string());
    malformed_request_hash.idempotency_request_hash = Some("sha256:short".to_string());
    let err = store
        .commit_table(&ident, malformed_request_hash)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains(
                "table commit idempotency request hash must be full SHA-256 evidence"
            )
    ));

    let err = store
        .replay_table_commit(
            &ident,
            " ",
            &content_hash_bytes("commit-request".as_bytes()),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table commit idempotency key may only contain")
    ));

    let err = store
        .replay_table_commit(&ident, "commit-1", "sha256:short")
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains(
                "table commit idempotency request hash must be full SHA-256 evidence"
            )
    ));
    let table = store.load_table(&ident).await.unwrap();
    assert_eq!(table.version, 0);
    assert_eq!(
        table.metadata_location.as_deref(),
        Some("file:///tmp/events/metadata/00000.json")
    );
    assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("idempotency_records").await.unwrap(), 0);

    let mut empty_expected_location = base_commit.clone();
    empty_expected_location.expected_previous_metadata_location = Some("  ".to_string());
    let err = store
        .commit_table(&ident, empty_expected_location)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("expected table metadata location must not be empty")
    ));

    let mut empty_new_location = base_commit.clone();
    empty_new_location.new_metadata_location = Some("  ".to_string());
    let err = store
        .commit_table(&ident, empty_new_location)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("new table metadata location must not be empty")
    ));

    let mut non_object_metadata = base_commit;
    non_object_metadata.new_metadata = Some(serde_json::json!("not metadata"));
    let err = store
        .commit_table(&ident, non_object_metadata)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("new table metadata must be a JSON object")
    ));

    let missing_format_version = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "noop"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
        new_metadata: Some(serde_json::json!({"current-snapshot-id": 42})),
        idempotency_key: None,
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: None,
    };
    let err = store
        .commit_table(&ident, missing_format_version)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("new table metadata format-version must be present")
    ));

    let zero_format_version = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "noop"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
        new_metadata: Some(serde_json::json!({"format-version": 0})),
        idempotency_key: None,
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: None,
    };
    let err = store
        .commit_table(&ident, zero_format_version)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("new table metadata format-version must be positive")
    ));

    let table = store.load_table(&ident).await.unwrap();
    assert_eq!(table.version, 0);
    assert_eq!(
        table.metadata_location.as_deref(),
        Some("file:///tmp/events/metadata/00000.json")
    );
    assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("idempotency_records").await.unwrap(), 0);
}

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
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
        new_metadata: None,
        idempotency_key: Some("commit-1".to_string()),
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: Some(serde_json::json!({
            "engine": "typesec",
            "allowed": true,
            "action": "table.commit"
        })),
    };
    let committed = store.commit_table(&ident, commit.clone()).await.unwrap();
    assert_eq!(committed.version, 1);
    assert_eq!(
        committed.metadata_location.as_deref(),
        Some("file:///tmp/events/metadata/00001.json")
    );
    let replayed = store.commit_table(&ident, commit).await.unwrap();
    assert_eq!(replayed.version, 1);

    let mismatched = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "noop"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00002.json".to_string()),
        new_metadata: None,
        idempotency_key: Some("commit-1".to_string()),
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: Some(serde_json::json!({
            "engine": "typesec",
            "allowed": true,
            "action": "table.commit"
        })),
    };
    let err = store.commit_table(&ident, mismatched).await.unwrap_err();
    let message = err.to_string();
    assert!(message.contains("idempotency key reused with different commit request"));
    assert!(!message.contains("commit-1"));
    assert!(!message.contains("00002.json"));
    assert!(!message.contains("file:///tmp/events/metadata/00002.json"));

    let commit_count = store.count_rows("metadata_pointer_log").await.unwrap();
    assert_eq!(commit_count, 1);
    let commit_records = store.table_commit_records(&ident, 1, None).await.unwrap();
    assert_eq!(commit_records.len(), 1);
    assert_eq!(commit_records[0].sequence_number, 1);
    let replayed_probe = store
        .replay_table_commit(&ident, "commit-1", &commit_records[0].request_hash)
        .await
        .unwrap()
        .expect("idempotency replay should be available before commit planning");
    assert_eq!(replayed_probe.version, 1);
    assert_eq!(
        commit_records[0].response_hash,
        crate::table_response_hash(&replayed_probe).unwrap()
    );
    assert_eq!(commit_records[0].format_version, Some(3));
    assert_eq!(commit_records[0].snapshot_id, Some(0));
    assert_eq!(commit_records[0].policy_hash, None);
    let different_request_hash = content_hash_bytes("different-request".as_bytes());
    let replay_mismatch = store
        .replay_table_commit(&ident, "commit-1", &different_request_hash)
        .await
        .unwrap_err();
    let message = replay_mismatch.to_string();
    assert!(message.contains("idempotency key reused with different commit request"));
    assert!(!message.contains("commit-1"));
    assert!(!message.contains(different_request_hash.as_str()));
    assert_eq!(
        commit_records[0].idempotency_key_sha256.as_deref(),
        Some(content_hash_bytes("commit-1".as_bytes()).as_str())
    );
    assert_eq!(
        commit_records[0].new_metadata_location.as_deref(),
        Some("file:///tmp/events/metadata/00001.json")
    );
    assert_eq!(
        store.table_commit_records(&ident, 2, None).await.unwrap(),
        vec![]
    );
    let audit_count = store.count_rows("audit_events").await.unwrap();
    assert_eq!(audit_count, 1);
    let conn = store.connect().unwrap();
    let mut audit_rows = conn
        .query("select request_hash, event_json from audit_events", ())
        .await
        .unwrap();
    let audit_row = audit_rows.next().await.unwrap().unwrap();
    let audit_request_hash = row_string(&audit_row, 0).unwrap();
    let audit_payload =
        decode_json::<serde_json::Value>(row_string(&audit_row, 1).unwrap()).unwrap();
    assert_eq!(
        audit_request_hash,
        content_hash_json(&audit_payload).unwrap()
    );
    assert_ne!(
        audit_request_hash, commit_records[0].request_hash,
        "audit request hash must bind the audit payload, not the inner commit request"
    );
    let outbox_count = store.count_rows("outbox_events").await.unwrap();
    assert_eq!(outbox_count, 1);

    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event_type, "table.commit");
    assert_eq!(
        pending[0].payload["commit"]["new_metadata_location"],
        serde_json::json!("file:///tmp/events/metadata/00001.json")
    );
    assert_eq!(
        pending[0].payload["commit"]["idempotency_key_sha256"],
        serde_json::json!(content_hash_bytes("commit-1".as_bytes()))
    );
    assert_eq!(
        pending[0].payload["commit"]["response_hash"],
        serde_json::json!(commit_records[0].response_hash)
    );
    assert_eq!(
        pending[0].payload["commit"]["format_version"],
        serde_json::json!(3)
    );
    assert_eq!(
        pending[0].payload["commit"]["snapshot_id"],
        serde_json::json!(0)
    );
    assert!(!pending[0].payload.to_string().contains("commit-1"));
    assert_eq!(
        pending[0].payload["authorization-receipt"]["engine"],
        serde_json::json!("typesec")
    );
    let event_ids = pending
        .iter()
        .map(|event| event.event_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(store.mark_outbox_delivered(&event_ids).await.unwrap(), 1);
    assert!(
        store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.mark_outbox_delivered(&event_ids).await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rejects_malformed_commit_history_records() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    store
        .commit_table(
            &ident,
            TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 3})),
                idempotency_key: Some("commit-1".to_string()),
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();
    let mut records = store.table_commit_records(&ident, 1, None).await.unwrap();
    assert_eq!(records.len(), 1);
    let base_record = records.remove(0);
    let conn = store.connect().unwrap();

    let mut missing_format_record = base_record.clone();
    missing_format_record.format_version = None;
    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (
            table_key(&ident),
            encode_json(&missing_format_record).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table commit record format version must be present")
    ));

    let mut negative_snapshot_record = base_record.clone();
    negative_snapshot_record.snapshot_id = Some(-1);
    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (
            table_key(&ident),
            encode_json(&negative_snapshot_record).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table commit record snapshot id must be non-negative")
    ));

    let mut missing_snapshot_record = base_record.clone();
    missing_snapshot_record.snapshot_id = None;
    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (
            table_key(&ident),
            encode_json(&missing_snapshot_record).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table commit record snapshot id must be present")
    ));

    let mut decorated_new_metadata_record = base_record.clone();
    decorated_new_metadata_record.new_metadata_location =
        Some("s3://lakecat-demo/events/metadata/00001.json?token=secret".to_string());
    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (
            table_key(&ident),
            encode_json(&decorated_new_metadata_record).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains(
                "table commit record new metadata location must not contain decorated location material"
            ) && message.contains("metadata-location-hash=sha256:")
            && !message.contains("token=secret")
    ));

    let mut credential_previous_metadata_record = base_record.clone();
    credential_previous_metadata_record.previous_metadata_location =
        Some("s3://lakecat-demo/events/metadata/access_key=secret.json".to_string());
    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (
            table_key(&ident),
            encode_json(&credential_previous_metadata_record).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains(
                "table commit record previous metadata location must not contain credential material"
            ) && message.contains("metadata-location-hash=sha256:")
            && !message.contains("access_key=secret")
    ));

    let mut malformed_idempotency_record = base_record.clone();
    malformed_idempotency_record.idempotency_key_sha256 = Some("sha256:short".to_string());

    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (
            table_key(&ident),
            encode_json(&malformed_idempotency_record).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains(
                "table commit record idempotency key hash must be full SHA-256 evidence"
            )
    ));

    let mut malformed_policy_record = base_record;
    malformed_policy_record.policy_hash = Some("sha256:short".to_string());

    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (
            table_key(&ident),
            encode_json(&malformed_policy_record).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains(
                "table commit record policy hash must be full SHA-256 evidence"
            )
    ));
}

#[tokio::test]
async fn turso_store_rejects_commit_history_row_json_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    store
        .commit_table(
            &ident,
            TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 3})),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();
    let mut records = store.table_commit_records(&ident, 1, None).await.unwrap();
    assert_eq!(records.len(), 1);
    records[0].sequence_number = 2;

    let conn = store.connect().unwrap();
    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (table_key(&ident), encode_json(&records[0]).unwrap()),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains(
                "table commit record sequence number does not match pointer log row"
            )
    ));
}

#[tokio::test]
async fn turso_store_rejects_commit_history_record_table_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    store
        .commit_table(
            &ident,
            TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 3})),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();
    let mut records = store.table_commit_records(&ident, 1, None).await.unwrap();
    assert_eq!(records.len(), 1);
    records[0].table = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("other_events").unwrap(),
    );

    let conn = store.connect().unwrap();
    conn.execute(
        "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
        (table_key(&ident), encode_json(&records[0]).unwrap()),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table commit record table does not match requested table")
    ));
}

#[tokio::test]
async fn turso_store_rejects_commit_history_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let other_ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("other_events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    for table_ident in [&ident, &other_ident] {
        store
            .create_table(TableRecord::new(
                table_ident.clone(),
                format!("file:///tmp/{}", table_ident.name),
                Some(format!(
                    "file:///tmp/{}/metadata/00000.json",
                    table_ident.name
                )),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            ))
            .await
            .unwrap();
    }
    store
        .commit_table(
            &ident,
            TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 3})),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update metadata_pointer_log set table_key = ?2 where table_key = ?1",
        (table_key(&ident), table_key(&other_ident)),
    )
    .await
    .unwrap();

    assert_eq!(
        store.table_commit_records(&ident, 0, None).await.unwrap(),
        vec![]
    );
    let err = store
        .table_commit_records(&other_ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table commit record table does not match requested table")
    ));
}

#[tokio::test]
async fn turso_store_rejects_commit_history_principal_row_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let writer = Principal::new("did:example:writer", lakecat_core::PrincipalKind::Agent).unwrap();
    let shadow = Principal::new("did:example:shadow", lakecat_core::PrincipalKind::Agent).unwrap();
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            writer.clone(),
        ))
        .await
        .unwrap();
    store
        .commit_table(
            &ident,
            TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 3})),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: writer,
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update metadata_pointer_log set principal_json = ?2 where table_key = ?1",
        (table_key(&ident), encode_json(&shadow).unwrap()),
    )
    .await
    .unwrap();

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains(
                "table commit record principal does not match pointer log row"
            )
    ));
}

#[tokio::test]
async fn turso_store_rejects_table_record_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    let mut table = store.load_table(&ident).await.unwrap();
    table.ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("other_events").unwrap(),
    );
    let conn = store.connect().unwrap();
    conn.execute(
        "update tables set record_json = ?2 where table_key = ?1",
        (table_key(&ident), encode_json(&table).unwrap()),
    )
    .await
    .unwrap();

    let err = store.load_table(&ident).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
    let err = store.list_tables(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
    let err = store
        .commit_table(
            &ident,
            TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 3})),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
    let err = store
        .soft_delete_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
    assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
    assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rejects_table_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update tables
                 set namespace_path = ?2, table_name = ?3
                 where table_key = ?1",
        (table_key(&ident), "other", "other_events"),
    )
    .await
    .unwrap();

    for err in [
        store.load_table(&ident).await.unwrap_err(),
        store.list_tables(&warehouse).await.unwrap_err(),
        store
            .commit_table(
                &ident,
                TableCommit {
                    requirements: vec![],
                    updates: vec![serde_json::json!({"action": "noop"})],
                    expected_previous_metadata_location: Some(
                        "file:///tmp/events/metadata/00000.json".to_string(),
                    ),
                    new_metadata_location: Some(
                        "file:///tmp/events/metadata/00001.json".to_string(),
                    ),
                    new_metadata: Some(serde_json::json!({"format-version": 3})),
                    idempotency_key: None,
                    idempotency_request_hash: None,
                    principal: Principal::anonymous(),
                    authorization_receipt: None,
                },
            )
            .await
            .unwrap_err(),
        store
            .soft_delete_table(&ident, Principal::anonymous(), None)
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("table record row scope does not match")
        ));
    }
    assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
    assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 0);

    let restore_store = TursoCatalogStore::in_memory().await.unwrap();
    restore_store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    restore_store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    restore_store
        .soft_delete_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap();
    let conn = restore_store.connect().unwrap();
    conn.execute(
        "update tables
                 set namespace_path = ?2, table_name = ?3
                 where table_key = ?1",
        (table_key(&ident), "other", "other_events"),
    )
    .await
    .unwrap();

    let err = restore_store
        .restore_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_soft_delete_row_column_drift_on_restore() {
    async fn soft_deleted_table() -> (std::sync::Arc<TursoCatalogStore>, TableIdent) {
        let store = TursoCatalogStore::in_memory().await.unwrap();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        );
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .unwrap();
        store
            .create_table(TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            ))
            .await
            .unwrap();
        store
            .soft_delete_table(&ident, Principal::anonymous(), None)
            .await
            .unwrap();
        (store, ident)
    }

    let (store, ident) = soft_deleted_table().await;
    let conn = store.connect().unwrap();
    conn.execute(
        "update soft_deletes set warehouse = ?2 where table_key = ?1",
        (table_key(&ident), "other"),
    )
    .await
    .unwrap();
    let err = store
        .restore_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("soft-delete row scope does not match record identity")
    ));

    let (store, ident) = soft_deleted_table().await;
    let conn = store.connect().unwrap();
    conn.execute(
        "update soft_deletes set metadata_location = ?2 where table_key = ?1",
        (table_key(&ident), "file:///tmp/events/metadata/99999.json"),
    )
    .await
    .unwrap();
    let err = store
        .restore_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("soft-delete metadata location does not match row")
    ));

    let (store, ident) = soft_deleted_table().await;
    let conn = store.connect().unwrap();
    conn.execute(
        "update soft_deletes set version = ?2 where table_key = ?1",
        (table_key(&ident), 9_i64),
    )
    .await
    .unwrap();
    let err = store
        .restore_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("soft-delete version does not match row")
    ));

    let (store, ident) = soft_deleted_table().await;
    let conn = store.connect().unwrap();
    conn.execute(
        "update soft_deletes set deleted_at = ?2 where table_key = ?1",
        (table_key(&ident), "2026-01-01T00:00:00Z"),
    )
    .await
    .unwrap();
    let err = store
        .restore_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("soft-delete timestamp does not match row")
    ));
}

#[tokio::test]
async fn turso_store_rejects_table_idempotency_response_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    let commit = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "noop"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
        new_metadata: Some(serde_json::json!({"format-version": 3})),
        idempotency_key: Some("commit-1".to_string()),
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: None,
    };
    store.commit_table(&ident, commit.clone()).await.unwrap();
    let record = store
        .table_commit_records(&ident, 1, Some(1))
        .await
        .unwrap()
        .pop()
        .unwrap();
    let mut response = store.load_table(&ident).await.unwrap();
    response.ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("other_events").unwrap(),
    );
    let conn = store.connect().unwrap();
    conn.execute(
        "update idempotency_records set response_json = ?2 where idem_key = ?1",
        (
            idempotency_record_key(&ident, "commit-1"),
            encode_json(&response).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .replay_table_commit(&ident, "commit-1", &record.request_hash)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
    let err = store.commit_table(&ident, commit).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_table_idempotency_row_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    let commit = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "noop"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
        new_metadata: Some(serde_json::json!({"format-version": 3})),
        idempotency_key: Some("commit-1".to_string()),
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: None,
    };
    store.commit_table(&ident, commit.clone()).await.unwrap();
    let record = store
        .table_commit_records(&ident, 1, Some(1))
        .await
        .unwrap()
        .pop()
        .unwrap();
    let other_ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("other_events").unwrap(),
    );
    let conn = store.connect().unwrap();
    conn.execute(
        "update idempotency_records set table_key = ?2 where idem_key = ?1",
        (
            idempotency_record_key(&ident, "commit-1"),
            crate::table_key(&other_ident),
        ),
    )
    .await
    .unwrap();

    let err = store
        .replay_table_commit(&ident, "commit-1", &record.request_hash)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("idempotency record row scope does not match")
    ));
    let err = store.commit_table(&ident, commit).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("idempotency record row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_table_idempotency_request_hash_row_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    let commit = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "noop"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
        new_metadata: Some(serde_json::json!({"format-version": 3})),
        idempotency_key: Some("commit-1".to_string()),
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: None,
    };
    store.commit_table(&ident, commit.clone()).await.unwrap();
    let record = store
        .table_commit_records(&ident, 1, Some(1))
        .await
        .unwrap()
        .pop()
        .unwrap();
    let conn = store.connect().unwrap();
    conn.execute(
        "update idempotency_records set request_hash = ?2 where idem_key = ?1",
        (idempotency_record_key(&ident, "commit-1"), "sha256:short"),
    )
    .await
    .unwrap();

    for err in [
        store
            .replay_table_commit(&ident, "commit-1", &record.request_hash)
            .await
            .unwrap_err(),
        store.commit_table(&ident, commit).await.unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains(
                    "idempotency record request hash must be full SHA-256 evidence"
                )
        ));
    }
}

#[tokio::test]
async fn turso_store_orders_pending_outbox_events_deterministically() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let mut events = Vec::new();
    for event_type in ["querygraph.bootstrap.b", "querygraph.bootstrap.a"] {
        let mut event = CatalogAuditEvent::new(
            event_type,
            Some(ident.clone()),
            Principal::anonymous(),
            serde_json::json!({
                "event-type": event_type,
                "table": ident.clone(),
                "sequence": event_type,
            }),
        )
        .unwrap();
        event.created_at = "2026-01-01T00:00:00Z".parse().unwrap();
        let audit_event_id = crate::audit_event_id(&event).unwrap();
        let outbox_payload = crate::audit_outbox_payload(&audit_event_id, &event);
        let outbox_event = crate::outbox_event_from_payload(&outbox_payload, event.created_at)
            .expect("test event should produce an outbox event");
        events.push((outbox_event.event_id, event));
    }
    events.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, event) in events {
        store.record_audit_event(event).await.unwrap();
    }

    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let event_ids = pending
        .iter()
        .map(|event| event.event_id.clone())
        .collect::<Vec<_>>();
    let mut sorted_event_ids = event_ids.clone();
    sorted_event_ids.sort();
    assert_eq!(event_ids, sorted_event_ids);
    assert_eq!(
        store
            .mark_outbox_delivered(&[event_ids[0].clone(), event_ids[0].clone()])
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn turso_store_omits_table_from_unscoped_audit_outbox_events() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "catalog.config-read",
                None,
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "catalog.config-read",
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "catalog-config"
                    },
                    "warehouse": "local"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "table.loaded",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "table.loaded",
                    "table": ident,
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-load"
                    },
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 1
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    let config = pending
        .iter()
        .find(|event| event.event_type == "catalog.config-read")
        .expect("config-read event");
    assert!(
        config.payload.get("table").is_none(),
        "unscoped config-read wrapper must not carry table evidence"
    );
    let table = pending
        .iter()
        .find(|event| event.event_type == "table.loaded")
        .expect("table-loaded event");
    assert!(
        table.payload.get("table").is_some(),
        "table-scoped wrapper must preserve table evidence"
    );
}

#[tokio::test]
async fn turso_store_rejects_malformed_outbox_delivery_ids() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let err = store
        .mark_outbox_delivered(&["sha256:short".to_string()])
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("outbox event id must be full SHA-256 evidence")
    ));
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].delivered_at.is_none());
}

#[tokio::test]
async fn turso_store_validates_pending_outbox_before_delivery() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let event_id = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap()[0]
        .event_id
        .clone();
    let conn = store.connect().unwrap();
    conn.execute(
        "update outbox_events set payload_json = ?2 where event_id = ?1",
        (
            event_id.as_str(),
            encode_json(&serde_json::json!({
                "event-type": "querygraph.bootstrap.drifted",
                "manifest-hash": "lakecat:test"
            }))
            .unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .mark_outbox_delivered(std::slice::from_ref(&event_id))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox event type does not match payload")
                && message.contains("event-id-hash=sha256:")
                && message.contains("payload-hash=sha256:")
    ));
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 1);
    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox event type does not match payload")
    ));
}

#[tokio::test]
async fn turso_store_rejects_partial_outbox_delivery_on_batch_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    for event_type in ["querygraph.bootstrap.first", "querygraph.bootstrap.second"] {
        store
            .record_audit_event(
                CatalogAuditEvent::new(
                    event_type,
                    Some(ident.clone()),
                    Principal::anonymous(),
                    serde_json::json!({
                        "event-type": event_type,
                        "table": ident.clone(),
                        "manifest-hash": event_type
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
    let event_ids = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap()
        .into_iter()
        .map(|event| event.event_id)
        .collect::<Vec<_>>();
    let conn = store.connect().unwrap();
    conn.execute(
        "update outbox_events set payload_json = ?2 where event_id = ?1",
        (
            event_ids[1].as_str(),
            encode_json(&serde_json::json!({
                "event-type": "querygraph.bootstrap.drifted",
                "manifest-hash": "lakecat:test"
            }))
            .unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store.mark_outbox_delivered(&event_ids).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox event type does not match payload")
    ));
    let mut rows = conn
        .query(
            "select count(*) from outbox_events where delivered_at is not null",
            (),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert_eq!(row_i64(&row, 0).unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rolls_back_audit_when_outbox_insert_fails() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let mut event = CatalogAuditEvent::new(
        "querygraph.bootstrap",
        Some(ident.clone()),
        Principal::anonymous(),
        serde_json::json!({
            "event-type": "querygraph.bootstrap",
            "table": ident,
            "manifest-hash": "lakecat:test"
        }),
    )
    .unwrap();
    event.created_at = "2026-01-01T00:00:00Z".parse().unwrap();
    let audit_event_id = crate::audit_event_id(&event).unwrap();
    let outbox_payload = crate::audit_outbox_payload(&audit_event_id, &event);
    let outbox_event = crate::outbox_event_from_payload(&outbox_payload, event.created_at)
        .expect("test event should produce an outbox event");

    let conn = store.connect().unwrap();
    conn.execute(
        "insert into outbox_events (
                    event_id, sink, event_type, payload_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)",
        (
            outbox_event.event_id.as_str(),
            outbox_event.sink.as_str(),
            outbox_event.event_type.as_str(),
            encode_json(&outbox_event.payload).unwrap(),
            outbox_event.created_at.to_rfc3339(),
        ),
    )
    .await
    .unwrap();

    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(
        matches!(&err, LakeCatError::Internal(message) if message.contains("UNIQUE") || message.contains("PRIMARY KEY")),
        "unexpected error: {err:?}"
    );
    assert_eq!(
        store.count_rows("audit_events").await.unwrap(),
        0,
        "audit insert must roll back when transactional outbox insert fails"
    );
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 1);
}

#[tokio::test]
async fn turso_store_duplicate_audit_write_does_not_duplicate_outbox() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let mut event = CatalogAuditEvent::new(
        "querygraph.bootstrap",
        Some(ident.clone()),
        Principal::anonymous(),
        serde_json::json!({
            "event-type": "querygraph.bootstrap",
            "table": ident,
            "manifest-hash": "lakecat:test"
        }),
    )
    .unwrap();
    event.created_at = "2026-01-01T00:00:00Z".parse().unwrap();

    store.record_audit_event(event.clone()).await.unwrap();
    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(
        matches!(&err, LakeCatError::Internal(message) if message.contains("UNIQUE") || message.contains("PRIMARY KEY")),
        "unexpected error: {err:?}"
    );
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 1);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 1);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event_type, "querygraph.bootstrap");
}

#[tokio::test]
async fn turso_store_rejects_audit_event_type_drift_before_outbox() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let mut event = CatalogAuditEvent::new(
        "querygraph.bootstrap",
        Some(ident.clone()),
        Principal::anonymous(),
        serde_json::json!({
            "event-type": "querygraph.bootstrap",
            "table": ident,
            "manifest-hash": "lakecat:test"
        }),
    )
    .unwrap();
    event.event_type = "querygraph.bootstrap.drifted".to_string();

    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("audit event type does not match payload")
    ));
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rejects_audit_events_without_request_hash() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let mut event = CatalogAuditEvent::new(
        "querygraph.bootstrap",
        Some(ident.clone()),
        Principal::anonymous(),
        serde_json::json!({
            "event-type": "querygraph.bootstrap",
            "table": ident,
            "manifest-hash": "lakecat:test"
        }),
    )
    .unwrap();
    event.request_hash = None;

    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("audit event request hash is required")
    ));
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rejects_audit_request_hash_drift_before_outbox() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let mut event = CatalogAuditEvent::new(
        "querygraph.bootstrap",
        Some(ident.clone()),
        Principal::anonymous(),
        serde_json::json!({
            "event-type": "querygraph.bootstrap",
            "table": ident,
            "manifest-hash": "lakecat:test"
        }),
    )
    .unwrap();
    event.request_hash = Some(content_hash_bytes(b"drifted-audit-request"));

    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("audit event request hash does not match payload")
    ));
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rejects_audit_payload_table_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let other_ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("other_events").unwrap(),
    );
    let event = CatalogAuditEvent::new(
        "querygraph.bootstrap",
        Some(ident),
        Principal::anonymous(),
        serde_json::json!({
            "event-type": "querygraph.bootstrap",
            "table": other_ident,
            "manifest-hash": "lakecat:test"
        }),
    )
    .unwrap();

    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("audit event payload table scope does not match")
    ));
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rejects_bare_table_name_audit_payload_scope() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let event = CatalogAuditEvent::new(
        "table.commits-listed",
        Some(ident),
        Principal::anonymous(),
        serde_json::json!({
            "event-type": "table.commits-listed",
            "table": "events",
            "commit-count": 0
        }),
    )
    .unwrap();

    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("audit event payload missing warehouse scope for table")
    ));
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rejects_audit_authorization_principal_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let event_principal =
        Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent).unwrap();
    let receipt_principal =
        Principal::new("human:operator", lakecat_core::PrincipalKind::Human).unwrap();
    let event = CatalogAuditEvent::new(
        "querygraph.bootstrap",
        None,
        event_principal,
        serde_json::json!({
            "event-type": "querygraph.bootstrap",
            "authorization-receipt": {
                "engine": "typesec",
                "allowed": true,
                "principal": receipt_principal,
                "action": "querygraph.bootstrap"
            },
            "manifest-hash": "lakecat:test"
        }),
    )
    .unwrap();

    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains(
                "audit event authorization receipt principal does not match event principal"
            )
    ));
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_rejects_audit_authorization_receipts_without_action() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let principal =
        Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent).unwrap();
    let event = CatalogAuditEvent::new(
        "querygraph.bootstrap",
        None,
        principal.clone(),
        serde_json::json!({
            "event-type": "querygraph.bootstrap",
            "authorization-receipt": {
                "engine": "typesec",
                "allowed": true,
                "principal": principal
            },
            "manifest-hash": "lakecat:test"
        }),
    )
    .unwrap();

    let err = store.record_audit_event(event).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("audit event authorization receipt action is required")
    ));
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_limits_pending_outbox_after_deterministic_ordering() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let mut expected = Vec::new();
    let mut events = Vec::new();
    for (event_type, created_at) in [
        ("querygraph.bootstrap.late", "2026-01-01T00:00:02Z"),
        ("querygraph.bootstrap.tie-b", "2026-01-01T00:00:01Z"),
        ("querygraph.bootstrap.tie-a", "2026-01-01T00:00:01Z"),
    ] {
        let mut event = CatalogAuditEvent::new(
            event_type,
            Some(ident.clone()),
            Principal::anonymous(),
            serde_json::json!({
                "event-type": event_type,
                "table": ident.clone(),
                "sequence": event_type,
            }),
        )
        .unwrap();
        event.created_at = created_at.parse().unwrap();
        let audit_event_id = crate::audit_event_id(&event).unwrap();
        let outbox_payload = crate::audit_outbox_payload(&audit_event_id, &event);
        let outbox_event = crate::outbox_event_from_payload(&outbox_payload, event.created_at)
            .expect("test event should produce an outbox event");
        expected.push((outbox_event.created_at, outbox_event.event_id.clone()));
        events.push((outbox_event.event_id, event));
    }
    events.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, event) in events {
        store.record_audit_event(event).await.unwrap();
    }
    expected.sort();

    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 2)
        .await
        .unwrap();
    let event_ids = pending
        .iter()
        .map(|event| event.event_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        event_ids,
        expected
            .iter()
            .take(2)
            .map(|(_, event_id)| event_id.clone())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn turso_store_rejects_stale_metadata_pointer_commits() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(warehouse, namespace, TableName::new("events").unwrap());
    let table = TableRecord::new(
        ident.clone(),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    store.create_table(table).await.unwrap();

    let err = store
        .commit_table(
            &ident,
            TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/stale.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: None,
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .expect_err("stale metadata pointer must conflict");

    let LakeCatError::Conflict(message) = err else {
        panic!("stale metadata pointer must return conflict");
    };
    assert!(message.contains("expected-metadata-location-hash=sha256:"));
    assert!(message.contains("actual-metadata-location-hash=sha256:"));
    assert!(!message.contains("stale.json"));
    assert!(!message.contains("00000.json"));
    assert_eq!(store.load_table(&ident).await.unwrap().version, 0);
    assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
}

#[tokio::test]
async fn turso_store_records_governed_scan_audit_outbox_events() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "table.scan-planned",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "table.scan-planned",
                    "table": ident,
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-plan-scan"
                    },
                    "planned-by": "lakecat-sail",
                    "scan-task-count": 2
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(store.count_rows("audit_events").await.unwrap(), 1);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event_type, "table.scan-planned");
    assert_eq!(
        pending[0].payload["payload"]["authorization-receipt"]["engine"],
        serde_json::json!("typesec")
    );
    assert_eq!(
        pending[0].payload["payload"]["scan-task-count"],
        serde_json::json!(2)
    );

    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "table.scan-tasks-fetched",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "table.scan-tasks-fetched",
                    "table": ident,
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-plan-scan"
                    },
                    "planned-by": "lakecat-sail",
                    "plan-task": "lakecat:plan:abc",
                    "file-scan-task-count": 3,
                    "delete-file-count": 1
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(store.count_rows("audit_events").await.unwrap(), 2);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 2);
    let fetched = pending
        .iter()
        .find(|event| event.event_type == "table.scan-tasks-fetched")
        .expect("scan task fetch event");
    assert_eq!(
        fetched.payload["payload"]["file-scan-task-count"],
        serde_json::json!(3)
    );
    assert_eq!(
        fetched.payload["payload"]["authorization-receipt"]["engine"],
        serde_json::json!("typesec")
    );

    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "table.loaded",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "table.loaded",
                    "table": ident,
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-load"
                    },
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "version": 7
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(store.count_rows("audit_events").await.unwrap(), 3);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 3);
    let loaded = pending
        .iter()
        .find(|event| event.event_type == "table.loaded")
        .expect("table loaded event");
    assert_eq!(
        loaded.payload["payload"]["metadata-location"],
        serde_json::json!("file:///tmp/events/metadata/00000.json")
    );
    assert_eq!(
        loaded.payload["payload"]["authorization-receipt"]["action"],
        serde_json::json!("table-load")
    );

    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "table.created",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "table.created",
                    "table": ident,
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-create"
                    },
                    "metadata-location": "file:///tmp/events/metadata/00000.json",
                    "location": "file:///tmp/events",
                    "version": 0
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(store.count_rows("audit_events").await.unwrap(), 4);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 4);
    let created = pending
        .iter()
        .find(|event| event.event_type == "table.created")
        .expect("table created event");
    assert_eq!(
        created.payload["payload"]["location"],
        serde_json::json!("file:///tmp/events")
    );
    assert_eq!(
        created.payload["payload"]["authorization-receipt"]["action"],
        serde_json::json!("table-create")
    );

    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                None,
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "graph-read"
                    },
                    "warehouse": "local",
                    "table-count": 1
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(store.count_rows("audit_events").await.unwrap(), 5);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 5);
    let bootstrap = pending
        .iter()
        .find(|event| event.event_type == "querygraph.bootstrap")
        .expect("querygraph bootstrap event");
    assert_eq!(
        bootstrap.payload["payload"]["authorization-receipt"]["action"],
        serde_json::json!("graph-read")
    );
    assert_eq!(
        bootstrap.payload["payload"]["table-count"],
        serde_json::json!(1)
    );

    for (event_type, payload) in [
        (
            "catalog.config-read",
            serde_json::json!({
                "event-type": "catalog.config-read",
                "authorization-receipt": {
                    "engine": "typesec",
                    "allowed": true,
                    "action": "catalog-config"
                },
                "warehouse": "local"
            }),
        ),
        (
            "namespace.created",
            serde_json::json!({
                "event-type": "namespace.created",
                "authorization-receipt": {
                    "engine": "typesec",
                    "allowed": true,
                    "action": "namespace-create"
                },
                "warehouse": "local",
                "namespace": ["default"]
            }),
        ),
        (
            "namespace.listed",
            serde_json::json!({
                "event-type": "namespace.listed",
                "authorization-receipt": {
                    "engine": "typesec",
                    "allowed": true,
                    "action": "namespace-list"
                },
                "warehouse": "local",
                "namespace-count": 1
            }),
        ),
    ] {
        store
            .record_audit_event(
                CatalogAuditEvent::new(event_type, None, Principal::anonymous(), payload).unwrap(),
            )
            .await
            .unwrap();
    }

    assert_eq!(store.count_rows("audit_events").await.unwrap(), 8);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 8);
    let namespace_listed = pending
        .iter()
        .find(|event| event.event_type == "namespace.listed")
        .expect("namespace listed event");
    assert_eq!(
        namespace_listed.payload["payload"]["namespace-count"],
        serde_json::json!(1)
    );

    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "credentials.vend-attempted",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "credentials.vend-attempted",
                    "table": ident,
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "credentials-vend"
                    },
                    "storage-location": "file:///tmp/events",
                    "credential-count": 0,
                    "mode": "governed-read-required"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(store.count_rows("audit_events").await.unwrap(), 9);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 9);
    let credentials = pending
        .iter()
        .find(|event| event.event_type == "credentials.vend-attempted")
        .expect("credentials vend attempted event");
    assert_eq!(
        credentials.payload["payload"]["credential-count"],
        serde_json::json!(0)
    );
    assert_eq!(
        credentials.payload["payload"]["authorization-receipt"]["action"],
        serde_json::json!("credentials-vend")
    );
}

#[tokio::test]
async fn turso_store_rejects_corrupt_pending_outbox_payloads() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let mut drifted_payload = pending[0].payload.clone();
    drifted_payload["event-type"] = serde_json::json!("querygraph.bootstrap.drifted");
    let conn = store.connect().unwrap();
    conn.execute(
        "update outbox_events set payload_json = ?2 where event_id = ?1",
        (
            pending[0].event_id.as_str(),
            encode_json(&drifted_payload).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    let LakeCatError::Internal(message) = err else {
        panic!("expected internal pending outbox validation error");
    };
    assert!(
        message.contains("pending outbox event type does not match payload")
            || message.contains("pending outbox event id does not match payload hash"),
        "{message}"
    );
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(message.contains("event-type-hash=sha256:"), "{message}");
    assert!(message.contains("payload-hash=sha256:"), "{message}");
    assert!(!message.contains(pending[0].event_id.as_str()), "{message}");
    assert!(!message.contains("querygraph.bootstrap"), "{message}");
    assert!(
        !message.contains("querygraph.bootstrap.drifted"),
        "{message}"
    );
    assert!(!message.contains("manifest-hash"), "{message}");
    assert!(!message.contains("lakecat:test"), "{message}");
}

#[tokio::test]
async fn turso_store_rejects_corrupt_pending_outbox_event_ids() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let conn = store.connect().unwrap();
    conn.execute(
        "update outbox_events set event_id = ?2 where event_id = ?1",
        (pending[0].event_id.as_str(), "sha256:short"),
    )
    .await
    .unwrap();

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    let LakeCatError::Internal(message) = err else {
        panic!("expected internal pending outbox validation error");
    };
    assert!(message.contains("pending outbox event id does not match payload hash"));
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(message.contains("event-type-hash=sha256:"), "{message}");
    assert!(message.contains("payload-hash=sha256:"), "{message}");
    assert!(!message.contains("sha256:short"), "{message}");
    assert!(!message.contains("manifest-hash"), "{message}");
    assert!(!message.contains("lakecat:test"), "{message}");
}

#[tokio::test]
async fn turso_store_rejects_blank_pending_outbox_event_types() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let blank_payload = serde_json::json!({
        "event-type": " ",
        "manifest-hash": "lakecat:test"
    });
    let blank_payload_hash = crate::content_hash_json(&blank_payload).unwrap();
    let conn = store.connect().unwrap();
    conn.execute(
        "update outbox_events
                 set event_id = ?2, event_type = ?3, payload_json = ?4
                 where event_id = ?1",
        (
            pending[0].event_id.as_str(),
            blank_payload_hash.as_str(),
            " ",
            encode_json(&blank_payload).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    let LakeCatError::Internal(message) = err else {
        panic!("expected internal pending outbox validation error");
    };
    assert!(message.contains("pending outbox event type must not be empty"));
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(message.contains("event-type-hash=sha256:"), "{message}");
    assert!(message.contains("payload-hash=sha256:"), "{message}");
    assert!(!message.contains("manifest-hash"), "{message}");
    assert!(!message.contains("lakecat:test"), "{message}");
}

#[tokio::test]
async fn turso_store_rejects_blank_pending_outbox_payload_event_types() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let blank_payload = serde_json::json!({
        "event-type": " ",
        "manifest-hash": "lakecat:test"
    });
    let blank_payload_hash = crate::content_hash_json(&blank_payload).unwrap();
    let conn = store.connect().unwrap();
    conn.execute(
        "update outbox_events
                 set event_id = ?2, payload_json = ?3
                 where event_id = ?1",
        (
            pending[0].event_id.as_str(),
            blank_payload_hash.as_str(),
            encode_json(&blank_payload).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    let LakeCatError::Internal(message) = err else {
        panic!("expected internal pending outbox validation error");
    };
    assert!(message.contains("pending outbox payload event-type must not be empty"));
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(message.contains("event-type-hash=sha256:"), "{message}");
    assert!(
        message.contains("payload-event-type-hash=sha256:"),
        "{message}"
    );
    assert!(message.contains("payload-hash=sha256:"), "{message}");
    assert!(!message.contains("manifest-hash"), "{message}");
    assert!(!message.contains("lakecat:test"), "{message}");
}

#[tokio::test]
async fn turso_store_rejects_missing_pending_outbox_payload_event_types() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let missing_payload = serde_json::json!({
        "manifest-hash": "lakecat:test"
    });
    let missing_payload_hash = crate::content_hash_json(&missing_payload).unwrap();
    let conn = store.connect().unwrap();
    conn.execute(
        "update outbox_events
                 set event_id = ?2, payload_json = ?3
                 where event_id = ?1",
        (
            pending[0].event_id.as_str(),
            missing_payload_hash.as_str(),
            encode_json(&missing_payload).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    let LakeCatError::Internal(message) = err else {
        panic!("expected internal pending outbox validation error");
    };
    assert!(message.contains("pending outbox payload missing event-type"));
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(message.contains("event-type-hash=sha256:"), "{message}");
    assert!(message.contains("payload-hash=sha256:"), "{message}");
    assert!(!message.contains("manifest-hash"), "{message}");
    assert!(!message.contains("lakecat:test"), "{message}");
}

#[tokio::test]
async fn turso_store_rejects_blank_pending_outbox_sinks() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    store
        .record_audit_event(
            CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let conn = store.connect().unwrap();
    conn.execute(
        "update outbox_events set sink = ?2 where event_id = ?1",
        (pending[0].event_id.as_str(), " "),
    )
    .await
    .unwrap();

    let err = store.pending_outbox_events(None, 10).await.unwrap_err();
    let LakeCatError::Internal(message) = err else {
        panic!("expected internal pending outbox validation error");
    };
    assert!(message.contains("pending outbox event sink must not be empty"));
    assert!(message.contains("event-id-hash=sha256:"), "{message}");
    assert!(message.contains("event-type-hash=sha256:"), "{message}");
    assert!(message.contains("payload-hash=sha256:"), "{message}");
    assert!(!message.contains("manifest-hash"), "{message}");
    assert!(!message.contains("lakecat:test"), "{message}");
}

#[tokio::test]
async fn turso_store_allows_only_one_concurrent_metadata_pointer_commit() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(warehouse, namespace, TableName::new("events").unwrap());
    let table = TableRecord::new(
        ident.clone(),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    store.create_table(table).await.unwrap();

    let commit_a = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "append", "writer": "a"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001-a.json".to_string()),
        new_metadata: None,
        idempotency_key: None,
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: None,
    };
    let commit_b = TableCommit {
        requirements: vec![],
        updates: vec![serde_json::json!({"action": "append", "writer": "b"})],
        expected_previous_metadata_location: Some(
            "file:///tmp/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some("file:///tmp/events/metadata/00001-b.json".to_string()),
        new_metadata: None,
        idempotency_key: None,
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: None,
    };

    let (result_a, result_b) = tokio::join!(
        store.commit_table(&ident, commit_a),
        store.commit_table(&ident, commit_b)
    );
    let results = [result_a, result_b];
    let success_count = results.iter().filter(|result| result.is_ok()).count();
    let conflict_count = results
        .iter()
        .filter(|result| matches!(result, Err(LakeCatError::Conflict(_))))
        .count();
    let conflict_message = results
        .iter()
        .find_map(|result| match result {
            Err(LakeCatError::Conflict(message)) => Some(message.as_str()),
            _ => None,
        })
        .expect("one concurrent commit should conflict");

    assert_eq!(success_count, 1);
    assert_eq!(conflict_count, 1);
    assert!(conflict_message.contains("expected-metadata-location-hash=sha256:"));
    assert!(conflict_message.contains("actual-metadata-location-hash=sha256:"));
    assert!(!conflict_message.contains("00000.json"));
    assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
    assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 1);
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 1);
    assert_eq!(store.count_rows("outbox_events").await.unwrap(), 1);
}

#[tokio::test]
async fn turso_store_persists_storage_profiles_and_matches_longest_prefix() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace,
        TableName::new("events").unwrap(),
    );
    let table = TableRecord::new(
        ident,
        "file:///tmp/events/tenant-a/table".to_string(),
        Some("file:///tmp/events/tenant-a/table/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );

    let broad = StorageProfile::new(
        "local-broad",
        warehouse.clone(),
        "file:///tmp/events",
        StorageProvider::File,
        CredentialIssuanceMode::LocalFileNoSecret,
        None,
        BTreeMap::new(),
    )
    .unwrap();
    let narrow = StorageProfile::new(
        "local-tenant-a",
        warehouse.clone(),
        "file:///tmp/events/tenant-a",
        StorageProvider::File,
        CredentialIssuanceMode::LocalFileNoSecret,
        None,
        BTreeMap::from([("lakecat.endpoint".to_string(), "local".to_string())]),
    )
    .unwrap();

    store.upsert_storage_profile(broad).await.unwrap();
    store.upsert_storage_profile(narrow.clone()).await.unwrap();

    let profiles = store.list_storage_profiles(&warehouse).await.unwrap();
    assert_eq!(profiles.len(), 2);
    let matched = store.storage_profile_for_table(&table).await.unwrap();
    assert_eq!(matched.profile_id, narrow.profile_id);
    assert_eq!(
        matched.public_config["lakecat.endpoint"],
        narrow.public_config["lakecat.endpoint"]
    );
}

#[tokio::test]
async fn turso_store_rejects_corrupt_storage_profiles_on_read() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let table = TableRecord::new(
        TableIdent::new(
            warehouse.clone(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "s3://lakecat-demo/events/table".to_string(),
        None,
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let mut profile = StorageProfile::new(
        "s3-events",
        warehouse.clone(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events".to_string()),
        BTreeMap::new(),
    )
    .unwrap();
    store.upsert_storage_profile(profile.clone()).await.unwrap();
    profile.profile_id = "s3-events?token=secret".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update storage_profiles set profile_json = ?2 where profile_key = ?1",
        (
            storage_profile_key(&warehouse, "s3-events"),
            encode_json(&profile).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
    let message = err.to_string();
    assert!(message.contains("storage-profile-id-hash=sha256:"));
    assert!(!message.contains("token=secret"));

    let err = store.storage_profile_for_table(&table).await.unwrap_err();
    let message = err.to_string();
    assert!(message.contains("storage-profile-id-hash=sha256:"));
    assert!(!message.contains("token=secret"));
}

#[tokio::test]
async fn turso_store_rejects_storage_profile_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let table = TableRecord::new(
        TableIdent::new(
            warehouse.clone(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "s3://lakecat-demo/events/table".to_string(),
        None,
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let mut profile = StorageProfile::new(
        "s3-events",
        warehouse.clone(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events".to_string()),
        BTreeMap::new(),
    )
    .unwrap();
    store.upsert_storage_profile(profile.clone()).await.unwrap();
    profile.profile_id = "other-profile".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update storage_profiles set profile_json = ?2 where profile_key = ?1",
        (
            storage_profile_key(&warehouse, "s3-events"),
            encode_json(&profile).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("storage profile row scope does not match")
    ));

    let err = store.storage_profile_for_table(&table).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("storage profile row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_storage_profile_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let table = TableRecord::new(
        TableIdent::new(
            warehouse.clone(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "s3://lakecat-demo/events/table".to_string(),
        None,
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let profile = StorageProfile::new(
        "s3-events",
        warehouse.clone(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events".to_string()),
        BTreeMap::new(),
    )
    .unwrap();
    store.upsert_storage_profile(profile).await.unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update storage_profiles
                 set location_prefix = ?2, provider = ?3, issuance_mode = ?4
                 where profile_key = ?1",
        (
            storage_profile_key(&warehouse, "s3-events"),
            "s3://lakecat-demo/other",
            "gcs",
            "governed-read-required",
        ),
    )
    .await
    .unwrap();

    let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("storage profile row scope does not match")
    ));

    let err = store.storage_profile_for_table(&table).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("storage profile row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_storage_profile_key_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let table = TableRecord::new(
        TableIdent::new(
            warehouse.clone(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        "s3://lakecat-demo/events/table".to_string(),
        None,
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    let profile = StorageProfile::new(
        "s3-events",
        warehouse.clone(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events".to_string()),
        BTreeMap::new(),
    )
    .unwrap();
    store.upsert_storage_profile(profile).await.unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update storage_profiles set profile_key = ?2 where profile_key = ?1",
        (
            storage_profile_key(&warehouse, "s3-events"),
            storage_profile_key(&warehouse, "other-profile"),
        ),
    )
    .await
    .unwrap();

    let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("storage profile row scope does not match")
    ));

    let err = store.storage_profile_for_table(&table).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("storage profile row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_storage_profile_scope_drift_before_upsert() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let profile = StorageProfile::new(
        "s3-events",
        warehouse.clone(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events".to_string()),
        BTreeMap::new(),
    )
    .unwrap();
    store.upsert_storage_profile(profile.clone()).await.unwrap();

    let mut corrupt = profile;
    corrupt.profile_id = "other-profile".to_string();
    let conn = store.connect().unwrap();
    conn.execute(
        "update storage_profiles set profile_json = ?2 where profile_key = ?1",
        (
            storage_profile_key(&warehouse, "s3-events"),
            encode_json(&corrupt).unwrap(),
        ),
    )
    .await
    .unwrap();

    let replacement = StorageProfile::new(
        "s3-events",
        warehouse.clone(),
        "s3://lakecat-demo/events/new",
        StorageProvider::S3,
        CredentialIssuanceMode::GovernedReadRequired,
        None,
        BTreeMap::new(),
    )
    .unwrap();
    let err = store.upsert_storage_profile(replacement).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("storage profile row scope does not match")
    ));

    let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("storage profile row scope does not match")
    ));
}

#[tokio::test]
async fn storage_profile_matching_rejects_ambiguous_same_prefix_profiles() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table = TableRecord::new(
        TableIdent::new(
            warehouse.clone(),
            namespace,
            TableName::new("events").unwrap(),
        ),
        "s3://lakecat-demo/events/tenant-a/table".to_string(),
        None,
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    for profile_id in ["events-a", "events-b"] {
        store
            .upsert_storage_profile(
                StorageProfile::new(
                    profile_id,
                    warehouse.clone(),
                    "s3://lakecat-demo/events",
                    StorageProvider::S3,
                    CredentialIssuanceMode::GovernedReadRequired,
                    None,
                    BTreeMap::new(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }

    let err = store.storage_profile_for_table(&table).await.unwrap_err();
    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("ambiguous storage profile match"));
    assert!(message.contains("location-prefix-hash=sha256:"));
    assert!(message.contains("events-a"));
    assert!(message.contains("events-b"));
    assert!(!message.contains("s3://lakecat-demo/events"));
}

#[tokio::test]
async fn turso_storage_profile_matching_rejects_ambiguous_same_prefix_profiles() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table = TableRecord::new(
        TableIdent::new(
            warehouse.clone(),
            namespace,
            TableName::new("events").unwrap(),
        ),
        "s3://lakecat-demo/events/tenant-a/table".to_string(),
        None,
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    for profile_id in ["events-a", "events-b"] {
        store
            .upsert_storage_profile(
                StorageProfile::new(
                    profile_id,
                    warehouse.clone(),
                    "s3://lakecat-demo/events",
                    StorageProvider::S3,
                    CredentialIssuanceMode::GovernedReadRequired,
                    None,
                    BTreeMap::new(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }

    let err = store.storage_profile_for_table(&table).await.unwrap_err();
    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("ambiguous storage profile match"));
    assert!(message.contains("location-prefix-hash=sha256:"));
    assert!(message.contains("events-a"));
    assert!(message.contains("events-b"));
    assert!(!message.contains("s3://lakecat-demo/events"));
}

#[tokio::test]
async fn storage_profile_matching_respects_location_boundaries() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let profile = StorageProfile::new(
        "events-root",
        warehouse.clone(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::GovernedReadRequired,
        None,
        BTreeMap::from([("lakecat.scope".to_string(), "events".to_string())]),
    )
    .unwrap();
    store.upsert_storage_profile(profile.clone()).await.unwrap();

    for (table_name, location, expected_profile_id) in [
        ("events-exact", "s3://lakecat-demo/events", "events-root"),
        (
            "events-child",
            "s3://lakecat-demo/events/tenant-a/table",
            "events-root",
        ),
        (
            "events-sibling",
            "s3://lakecat-demo/events-shadow/table",
            "local:s3",
        ),
    ] {
        let table = TableRecord::new(
            TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new(table_name).unwrap(),
            ),
            location.to_string(),
            None,
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );

        let matched = store.storage_profile_for_table(&table).await.unwrap();
        assert_eq!(matched.profile_id, expected_profile_id, "{location}");
    }
}

#[tokio::test]
async fn turso_storage_profile_matching_respects_trailing_slash_boundaries() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let profile = StorageProfile::new(
        "events-root",
        warehouse.clone(),
        "s3://lakecat-demo/events/",
        StorageProvider::S3,
        CredentialIssuanceMode::GovernedReadRequired,
        None,
        BTreeMap::from([("lakecat.scope".to_string(), "events".to_string())]),
    )
    .unwrap();
    store.upsert_storage_profile(profile.clone()).await.unwrap();

    for (table_name, location, expected_profile_id) in [
        (
            "events-child",
            "s3://lakecat-demo/events/tenant-a/table",
            "events-root",
        ),
        (
            "events-sibling",
            "s3://lakecat-demo/events-shadow/table",
            "local:s3",
        ),
    ] {
        let table = TableRecord::new(
            TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new(table_name).unwrap(),
            ),
            location.to_string(),
            None,
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );

        let matched = store.storage_profile_for_table(&table).await.unwrap();
        assert_eq!(matched.profile_id, expected_profile_id, "{location}");
    }
}

#[test]
fn storage_profiles_reject_dot_segment_location_prefixes() {
    let warehouse = WarehouseName::new("local").unwrap();
    for location_prefix in [
        "s3://lakecat-demo/events/../private",
        "s3://lakecat-demo/events/%2e%2e/private",
        "file:///tmp/lakecat/%2E/events",
    ] {
        let err = StorageProfile::new(
            "dot-prefix",
            warehouse.clone(),
            location_prefix,
            StorageProvider::from_location(location_prefix),
            CredentialIssuanceMode::GovernedReadRequired,
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("dot path segments"));
        assert!(message.contains("storage-profile-prefix-hash=sha256:"));
        assert!(
            !message.contains(location_prefix),
            "dot-segment location-prefix validation must not expose raw storage roots"
        );
    }
}

#[test]
fn location_prefix_dot_segment_detection_allows_ordinary_dotted_names() {
    assert!(crate::location_prefix_has_dot_path_segment(
        "s3://lakecat-demo/events/../private"
    ));
    assert!(crate::location_prefix_has_dot_path_segment(
        "s3://lakecat-demo/events/%2e%2e/private"
    ));
    assert!(!crate::location_prefix_has_dot_path_segment(
        "s3://lakecat-demo/events/service.v1/table"
    ));
}

#[test]
fn storage_profiles_reject_decorated_location_prefixes() {
    let warehouse = WarehouseName::new("local").unwrap();
    for (location_prefix, provider) in [
        ("s3://lakecat-demo/events?token=abc", StorageProvider::S3),
        ("s3://lakecat-demo/events#current", StorageProvider::S3),
        ("s3://user:secret@lakecat-demo/events", StorageProvider::S3),
        (
            "file:///tmp/lakecat/events?debug=true",
            StorageProvider::File,
        ),
    ] {
        let err = StorageProfile::new(
            "decorated-prefix",
            warehouse.clone(),
            location_prefix,
            provider,
            CredentialIssuanceMode::GovernedReadRequired,
            None,
            BTreeMap::new(),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("query strings, fragments, or userinfo"));
        assert!(message.contains("storage-profile-prefix-hash=sha256:"));
        assert!(
            !message.contains(location_prefix),
            "decorated location-prefix validation must not expose raw storage roots"
        );
        assert!(!message.contains("token=abc"));
        assert!(!message.contains("user:secret"));
    }
}

#[tokio::test]
async fn storage_profile_upsert_rejects_deserialized_decorated_location_prefixes() {
    let warehouse = WarehouseName::new("local").unwrap();
    let profile = StorageProfile {
        profile_id: "decorated-prefix".to_string(),
        warehouse: warehouse.clone(),
        location_prefix: "s3://lakecat-demo/events?token=abc".to_string(),
        provider: StorageProvider::S3,
        issuance_mode: CredentialIssuanceMode::GovernedReadRequired,
        secret_ref: None,
        public_config: BTreeMap::new(),
    };

    let memory_err = MemoryCatalogStore::new()
        .upsert_storage_profile(profile.clone())
        .await
        .unwrap_err();
    let message = memory_err.to_string();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo/events?token=abc"));
    assert!(!message.contains("token=abc"));

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
    let message = turso_err.to_string();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo/events?token=abc"));
    assert!(!message.contains("token=abc"));
    assert_eq!(
        turso.list_storage_profiles(&warehouse).await.unwrap(),
        vec![]
    );
}

#[tokio::test]
async fn storage_profile_upsert_rejects_deserialized_dot_segment_location_prefixes() {
    let warehouse = WarehouseName::new("local").unwrap();
    let profile = StorageProfile {
        profile_id: "dot-prefix".to_string(),
        warehouse: warehouse.clone(),
        location_prefix: "s3://lakecat-demo/events/../private".to_string(),
        provider: StorageProvider::S3,
        issuance_mode: CredentialIssuanceMode::GovernedReadRequired,
        secret_ref: None,
        public_config: BTreeMap::new(),
    };

    let memory_err = MemoryCatalogStore::new()
        .upsert_storage_profile(profile.clone())
        .await
        .unwrap_err();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    assert!(memory_err.to_string().contains("dot path segments"));
    assert!(
        memory_err
            .to_string()
            .contains("storage-profile-prefix-hash=sha256:")
    );

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    assert!(turso_err.to_string().contains("dot path segments"));
    assert!(
        turso_err
            .to_string()
            .contains("storage-profile-prefix-hash=sha256:")
    );
    assert_eq!(
        turso.list_storage_profiles(&warehouse).await.unwrap(),
        vec![]
    );
}

#[test]
fn warehouses_reject_decorated_storage_roots() {
    let warehouse = WarehouseName::new("local").unwrap();
    for storage_root in [
        "file:///tmp/lakecat?token=abc",
        "s3://lakecat-demo/root#current",
        "s3://user:secret@lakecat-demo/root",
    ] {
        let err = WarehouseRecord::new(
            warehouse.clone(),
            "default",
            Some(storage_root.to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("query strings, fragments, or userinfo"));
        assert!(message.contains("warehouse-storage-root-hash=sha256:"));
        assert!(
            !message.contains(storage_root),
            "warehouse storage-root validation must not expose raw storage roots"
        );
        assert!(!message.contains("token=abc"));
        assert!(!message.contains("user:secret"));
    }
}

#[test]
fn servers_reject_decorated_endpoint_urls() {
    for endpoint_url in [
        "https://lakecat.example.com?token=abc",
        "https://lakecat.example.com/catalog#frag",
        "https://user:secret@lakecat.example.com/catalog",
    ] {
        let err = ServerRecord::new(
            "prod",
            Some("Production".to_string()),
            Some(endpoint_url.to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("query strings, fragments, or userinfo"));
        assert!(message.contains("server-endpoint-url-hash=sha256:"));
        assert!(
            !message.contains(endpoint_url),
            "server endpoint validation must not expose raw endpoint URLs"
        );
        assert!(!message.contains("token=abc"));
        assert!(!message.contains("user:secret"));
    }
}

#[test]
fn servers_reject_invalid_endpoint_urls() {
    for (endpoint_url, expected) in [
        ("lakecat.example.com/catalog", "absolute http or https URL"),
        ("not a url", "absolute http or https URL"),
        ("file:///tmp/lakecat", "http or https scheme"),
        ("s3://lakecat-demo/catalog", "http or https scheme"),
    ] {
        let err = ServerRecord::new(
            "prod",
            Some("Production".to_string()),
            Some(endpoint_url.to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains(expected));
        assert!(message.contains("server-endpoint-url-hash=sha256:"));
        assert!(
            !message.contains(endpoint_url),
            "server endpoint validation must not expose raw endpoint URLs"
        );
    }
}

#[tokio::test]
async fn server_upsert_rejects_deserialized_invalid_endpoint_urls() {
    let record = ServerRecord {
        server_id: "prod".to_string(),
        display_name: Some("Production".to_string()),
        endpoint_url: Some("s3://lakecat-demo/catalog".to_string()),
        properties: BTreeMap::new(),
        created: AuditStamp::now(Principal::anonymous()),
        updated_at: Utc::now(),
    };

    let memory_err = MemoryCatalogStore::new()
        .upsert_server(record.clone())
        .await
        .unwrap_err();
    let message = memory_err.to_string();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("http or https scheme"));
    assert!(message.contains("server-endpoint-url-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo/catalog"));

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_server(record).await.unwrap_err();
    let message = turso_err.to_string();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("http or https scheme"));
    assert!(message.contains("server-endpoint-url-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo/catalog"));
    assert_eq!(turso.list_servers().await.unwrap(), vec![]);
}

#[tokio::test]
async fn server_upsert_rejects_deserialized_decorated_endpoint_urls() {
    let record = ServerRecord {
        server_id: "prod".to_string(),
        display_name: Some("Production".to_string()),
        endpoint_url: Some("https://lakecat.example.com?token=abc".to_string()),
        properties: BTreeMap::new(),
        created: AuditStamp::now(Principal::anonymous()),
        updated_at: Utc::now(),
    };

    let memory_err = MemoryCatalogStore::new()
        .upsert_server(record.clone())
        .await
        .unwrap_err();
    let message = memory_err.to_string();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("server-endpoint-url-hash=sha256:"));
    assert!(!message.contains("https://lakecat.example.com?token=abc"));
    assert!(!message.contains("token=abc"));

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_server(record).await.unwrap_err();
    let message = turso_err.to_string();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("server-endpoint-url-hash=sha256:"));
    assert!(!message.contains("https://lakecat.example.com?token=abc"));
    assert!(!message.contains("token=abc"));
    assert_eq!(turso.list_servers().await.unwrap(), vec![]);
}

#[test]
fn warehouses_reject_dot_segment_storage_roots() {
    let warehouse = WarehouseName::new("local").unwrap();
    for storage_root in [
        "file:///tmp/lakecat/../private",
        "file:///tmp/lakecat/%2e%2e/private",
        "s3://lakecat-demo/root/%2E/private",
    ] {
        let err = WarehouseRecord::new(
            warehouse.clone(),
            "default",
            Some(storage_root.to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("dot path segments"));
        assert!(message.contains("warehouse-storage-root-hash=sha256:"));
        assert!(
            !message.contains(storage_root),
            "warehouse dot-segment storage-root validation must not expose raw storage roots"
        );
    }
}

#[tokio::test]
async fn warehouse_upsert_rejects_deserialized_decorated_storage_roots() {
    let warehouse = WarehouseName::new("decorated_root").unwrap();
    let record = WarehouseRecord {
        warehouse: warehouse.clone(),
        project_id: "default".to_string(),
        storage_root: Some("file:///tmp/lakecat?token=abc".to_string()),
        properties: BTreeMap::new(),
        created: AuditStamp::now(Principal::anonymous()),
        updated_at: Utc::now(),
    };

    let memory_err = MemoryCatalogStore::new()
        .upsert_warehouse(record.clone())
        .await
        .unwrap_err();
    let message = memory_err.to_string();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("warehouse-storage-root-hash=sha256:"));
    assert!(!message.contains("file:///tmp/lakecat?token=abc"));
    assert!(!message.contains("token=abc"));

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_warehouse(record).await.unwrap_err();
    let message = turso_err.to_string();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("query strings, fragments, or userinfo"));
    assert!(message.contains("warehouse-storage-root-hash=sha256:"));
    assert!(!message.contains("file:///tmp/lakecat?token=abc"));
    assert!(!message.contains("token=abc"));
    assert!(matches!(
        turso.load_warehouse(&warehouse).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "warehouse" && name == "decorated_root"
    ));
}

#[tokio::test]
async fn warehouse_upsert_rejects_deserialized_dot_segment_storage_roots() {
    let warehouse = WarehouseName::new("dot_root").unwrap();
    let record = WarehouseRecord {
        warehouse: warehouse.clone(),
        project_id: "default".to_string(),
        storage_root: Some("file:///tmp/lakecat/../private".to_string()),
        properties: BTreeMap::new(),
        created: AuditStamp::now(Principal::anonymous()),
        updated_at: Utc::now(),
    };

    let memory_err = MemoryCatalogStore::new()
        .upsert_warehouse(record.clone())
        .await
        .unwrap_err();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    assert!(memory_err.to_string().contains("dot path segments"));
    assert!(
        memory_err
            .to_string()
            .contains("warehouse-storage-root-hash=sha256:")
    );

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_warehouse(record).await.unwrap_err();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    assert!(turso_err.to_string().contains("dot path segments"));
    assert!(
        turso_err
            .to_string()
            .contains("warehouse-storage-root-hash=sha256:")
    );
    assert!(matches!(
        turso.load_warehouse(&warehouse).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "warehouse" && name == "dot_root"
    ));
}

#[tokio::test]
async fn turso_store_persists_secret_ref_profiles_without_secret_material() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let profile = StorageProfile::new(
        "s3-events",
        warehouse.clone(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events".to_string()),
        BTreeMap::from([("lakecat.region".to_string(), "us-west-2".to_string())]),
    )
    .unwrap();

    store.upsert_storage_profile(profile).await.unwrap();
    let profiles = store.list_storage_profiles(&warehouse).await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(
        profiles[0].secret_ref.as_deref(),
        Some("typesec://lakecat/local/s3-events")
    );

    let embedded_secret = StorageProfile::new(
        "bad-s3-events",
        warehouse,
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events?token=secret".to_string()),
        BTreeMap::new(),
    );
    assert!(embedded_secret.is_err());
}

#[test]
fn storage_profiles_reject_decorated_secret_ref_uris() {
    let warehouse = WarehouseName::new("local").unwrap();
    for (secret_ref, expected) in [
        (
            "typesec://lakecat/local/s3-events?version=1",
            "query strings",
        ),
        ("vault://token@secret/data/lakecat/s3-events", "userinfo"),
        ("aws-sm://lakecat/s3-events#current", "fragments"),
    ] {
        let err = StorageProfile::new(
            "decorated-secret-ref",
            warehouse.clone(),
            "s3://lakecat-demo/events",
            StorageProvider::S3,
            CredentialIssuanceMode::ShortLivedSecretRef,
            Some(secret_ref.to_string()),
            BTreeMap::new(),
        )
        .unwrap_err();

        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        let message = err.to_string();
        assert!(
            message.contains(expected),
            "expected {secret_ref} to reject {expected}, got {err}"
        );
        assert!(message.contains("secret-ref-hash=sha256:"));
        assert!(
            !message.contains(secret_ref),
            "decorated secret-ref validation must not expose raw secret refs"
        );
    }
}

#[test]
fn storage_profiles_redact_invalid_secret_ref_uris() {
    let warehouse = WarehouseName::new("local").unwrap();
    for secret_ref in [
        "not a uri with secret=abc",
        "file:///tmp/raw-secret",
        "postgres://user:secret@example.test/credentials",
    ] {
        let err = StorageProfile::new(
            "invalid-secret-ref",
            warehouse.clone(),
            "s3://lakecat-demo/events",
            StorageProvider::S3,
            CredentialIssuanceMode::ShortLivedSecretRef,
            Some(secret_ref.to_string()),
            BTreeMap::new(),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("secret-ref-hash=sha256:"));
        assert!(
            !message.contains(secret_ref),
            "storage profile validation errors must not expose raw secret refs"
        );
    }
}

#[test]
fn storage_profiles_reject_dot_segment_secret_refs() {
    let warehouse = WarehouseName::new("local").unwrap();
    for secret_ref in [
        "vault://secret/data/lakecat/../s3-events",
        "aws-sm://lakecat/%2e%2e/s3-events",
        "gcp-sm://lakecat/%2E/s3-events",
    ] {
        let err = StorageProfile::new(
            "dot-secret-ref",
            warehouse.clone(),
            "s3://lakecat-demo/events",
            StorageProvider::S3,
            CredentialIssuanceMode::ShortLivedSecretRef,
            Some(secret_ref.to_string()),
            BTreeMap::new(),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("dot path segments"));
        assert!(message.contains("secret-ref-hash=sha256:"));
        assert!(
            !message.contains(secret_ref),
            "dot-segment secret-ref validation must not expose raw secret refs"
        );
    }
}

#[test]
fn storage_profiles_redact_embedded_secret_ref_material() {
    let warehouse = WarehouseName::new("local").unwrap();
    let secret_ref = "vault://secret/data/lakecat/s3-events/password=abc";
    let err = StorageProfile::new(
        "embedded-secret-ref",
        warehouse,
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some(secret_ref.to_string()),
        BTreeMap::new(),
    )
    .unwrap_err();

    let message = err.to_string();
    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    assert!(message.contains("must not embed raw secret material"));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(
        !message.contains(secret_ref),
        "embedded secret-ref validation must not expose raw secret refs"
    );
    assert!(!message.contains("password=abc"));
}

#[test]
fn secret_ref_dot_segment_detection_allows_ordinary_dotted_names() {
    assert!(crate::secret_ref_has_dot_path_segment(
        "vault://secret/data/lakecat/../s3-events"
    ));
    assert!(crate::secret_ref_has_dot_path_segment(
        "vault://secret/data/lakecat/%2e%2e/s3-events"
    ));
    assert!(!crate::secret_ref_has_dot_path_segment(
        "vault://secret/data/lakecat/service.v1/s3-events"
    ));
}

#[test]
fn storage_profiles_reject_provider_location_mismatch() {
    let warehouse = WarehouseName::new("local").unwrap();
    let err = StorageProfile::new(
        "wrong-provider",
        warehouse,
        "s3://lakecat-demo/events",
        StorageProvider::File,
        CredentialIssuanceMode::LocalFileNoSecret,
        None,
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("does not match location prefix provider"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("s3://lakecat-demo/events"));
    assert!(!message.contains("lakecat-demo"));
}

#[test]
fn storage_profiles_redact_unsupported_provider_location_prefixes() {
    let warehouse = WarehouseName::new("local").unwrap();
    let err = StorageProfile::new(
        "unsupported-prefix",
        warehouse,
        "https://lakecat-demo.example/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("vault://kv/lakecat/events".to_string()),
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("is not supported by provider 's3'"));
    assert!(message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!message.contains("https://lakecat-demo.example/events"));
    assert!(!message.contains("lakecat-demo"));
}

#[test]
fn storage_profiles_reject_provider_issuance_mismatch() {
    let warehouse = WarehouseName::new("local").unwrap();
    let remote_no_secret = StorageProfile::new(
        "remote-no-secret",
        warehouse.clone(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::LocalFileNoSecret,
        None,
        BTreeMap::new(),
    )
    .unwrap_err();
    assert!(matches!(remote_no_secret, LakeCatError::InvalidArgument(_)));
    assert!(
        remote_no_secret
            .to_string()
            .contains("local-file-no-secret issuance mode requires file provider")
    );
    let remote_message = remote_no_secret.to_string();
    assert!(remote_message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!remote_message.contains("s3://lakecat-demo/events"));
    assert!(!remote_message.contains("lakecat-demo"));

    let local_secret_ref = StorageProfile::new(
        "local-secret-ref",
        warehouse,
        "file:///tmp/events",
        StorageProvider::File,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/events".to_string()),
        BTreeMap::new(),
    )
    .unwrap_err();
    assert!(matches!(local_secret_ref, LakeCatError::InvalidArgument(_)));
    assert!(
        local_secret_ref
            .to_string()
            .contains("short-lived-secret-ref issuance mode requires s3, gcs, or azure")
    );
    let local_message = local_secret_ref.to_string();
    assert!(local_message.contains("storage-profile-prefix-hash=sha256:"));
    assert!(!local_message.contains("file:///tmp/events"));
    assert!(!local_message.contains("typesec://lakecat/local/events"));
}

#[test]
fn storage_profiles_reject_secret_refs_without_secret_ref_issuance_mode() {
    let secret_ref = "typesec://lakecat/local/s3-events";
    let err = StorageProfile::new(
        "governed-with-secret-ref",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::GovernedReadRequired,
        Some(secret_ref.to_string()),
        BTreeMap::new(),
    )
    .unwrap_err();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains(
        "storage profile secret reference requires short-lived-secret-ref issuance mode"
    ));
    assert!(message.contains("secret-ref-hash=sha256:"));
    assert!(!message.contains(secret_ref));
}

#[test]
fn storage_profiles_reject_public_config_secret_values() {
    let warehouse = WarehouseName::new("local").unwrap();
    let err = StorageProfile::new(
        "secret-public-config",
        warehouse,
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some("typesec://lakecat/local/s3-events".to_string()),
        BTreeMap::from([(
            "lakecat.endpoint".to_string(),
            "https://storage.example.invalid?token=raw-secret".to_string(),
        )]),
    )
    .unwrap_err();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("public config value may expose secret material"));
    assert!(message.contains("public-config-key-hash=sha256:"));
    assert!(!message.contains("lakecat.endpoint"));
    assert!(!message.contains("raw-secret"));
}

#[test]
fn storage_profile_validate_rejects_public_config_secret_values() {
    let profile = StorageProfile {
        profile_id: "secret-public-config".to_string(),
        warehouse: WarehouseName::new("local").unwrap(),
        location_prefix: "s3://lakecat-demo/events".to_string(),
        provider: StorageProvider::S3,
        issuance_mode: CredentialIssuanceMode::ShortLivedSecretRef,
        secret_ref: Some("typesec://lakecat/local/s3-events".to_string()),
        public_config: BTreeMap::from([(
            "lakecat.endpoint".to_string(),
            "https://storage.example.invalid?token=raw-secret".to_string(),
        )]),
    };

    let err = profile.validate().unwrap_err();
    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("public config value may expose secret material"));
    assert!(message.contains("public-config-key-hash=sha256:"));
    assert!(!message.contains("lakecat.endpoint"));
    assert!(!message.contains("raw-secret"));
}

#[test]
fn storage_profiles_redact_secret_like_public_config_keys() {
    let err = StorageProfile::new(
        "secret-key-public-config",
        WarehouseName::new("local").unwrap(),
        "file:///tmp/events",
        StorageProvider::File,
        CredentialIssuanceMode::LocalFileNoSecret,
        None,
        BTreeMap::from([(
            "customer-secret-token".to_string(),
            "metadata-only".to_string(),
        )]),
    )
    .unwrap_err();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("public config key may expose secret material"));
    assert!(message.contains("public-config-key-hash=sha256:"));
    assert!(!message.contains("customer-secret-token"));
}

#[test]
fn storage_profiles_reject_reserved_public_config_keys() {
    let err = StorageProfile::new(
        "reserved-public-config",
        WarehouseName::new("local").unwrap(),
        "file:///tmp/events",
        StorageProvider::File,
        CredentialIssuanceMode::LocalFileNoSecret,
        None,
        BTreeMap::from([(
            "lakecat.storage-profile-id".to_string(),
            "shadow-profile".to_string(),
        )]),
    )
    .unwrap_err();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("reserved for LakeCat credential evidence"));
    assert!(message.contains("public-config-key-hash=sha256:"));
    assert!(!message.contains("lakecat.storage-profile-id"));
}

#[tokio::test]
async fn storage_profile_upsert_rejects_deserialized_public_config_secrets() {
    let warehouse = WarehouseName::new("local").unwrap();
    let profile = StorageProfile {
        profile_id: "secret-public-config".to_string(),
        warehouse: warehouse.clone(),
        location_prefix: "s3://lakecat-demo/events".to_string(),
        provider: StorageProvider::S3,
        issuance_mode: CredentialIssuanceMode::ShortLivedSecretRef,
        secret_ref: Some("typesec://lakecat/local/s3-events".to_string()),
        public_config: BTreeMap::from([(
            "lakecat.endpoint".to_string(),
            "https://storage.example.invalid?token=raw-secret".to_string(),
        )]),
    };

    let memory_err = MemoryCatalogStore::new()
        .upsert_storage_profile(profile.clone())
        .await
        .unwrap_err();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    let memory_message = memory_err.to_string();
    assert!(memory_message.contains("public config value may expose secret material"));
    assert!(memory_message.contains("public-config-key-hash=sha256:"));
    assert!(!memory_message.contains("lakecat.endpoint"));
    assert!(!memory_message.contains("raw-secret"));

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    let turso_message = turso_err.to_string();
    assert!(turso_message.contains("public config value may expose secret material"));
    assert!(turso_message.contains("public-config-key-hash=sha256:"));
    assert!(!turso_message.contains("lakecat.endpoint"));
    assert!(!turso_message.contains("raw-secret"));
    assert_eq!(
        turso.list_storage_profiles(&warehouse).await.unwrap(),
        vec![]
    );
}

#[tokio::test]
async fn storage_profile_upsert_rejects_deserialized_secret_refs_without_secret_mode() {
    let warehouse = WarehouseName::new("local").unwrap();
    let secret_ref = "typesec://lakecat/local/s3-events";
    let profile = StorageProfile {
        profile_id: "governed-with-secret-ref".to_string(),
        warehouse: warehouse.clone(),
        location_prefix: "s3://lakecat-demo/events".to_string(),
        provider: StorageProvider::S3,
        issuance_mode: CredentialIssuanceMode::GovernedReadRequired,
        secret_ref: Some(secret_ref.to_string()),
        public_config: BTreeMap::new(),
    };

    let memory = MemoryCatalogStore::new();
    let memory_err = memory
        .upsert_storage_profile(profile.clone())
        .await
        .unwrap_err();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    let memory_message = memory_err.to_string();
    assert!(memory_message.contains(
        "storage profile secret reference requires short-lived-secret-ref issuance mode"
    ));
    assert!(memory_message.contains("secret-ref-hash=sha256:"));
    assert!(!memory_message.contains(secret_ref));
    assert_eq!(
        memory.list_storage_profiles(&warehouse).await.unwrap(),
        vec![]
    );

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    let turso_message = turso_err.to_string();
    assert!(turso_message.contains(
        "storage profile secret reference requires short-lived-secret-ref issuance mode"
    ));
    assert!(turso_message.contains("secret-ref-hash=sha256:"));
    assert!(!turso_message.contains(secret_ref));
    assert_eq!(
        turso.list_storage_profiles(&warehouse).await.unwrap(),
        vec![]
    );
}

#[tokio::test]
async fn storage_profile_upsert_rejects_reserved_public_config_keys() {
    let warehouse = WarehouseName::new("local").unwrap();
    let profile = StorageProfile {
        profile_id: "reserved-public-config".to_string(),
        warehouse: warehouse.clone(),
        location_prefix: "file:///tmp/events".to_string(),
        provider: StorageProvider::File,
        issuance_mode: CredentialIssuanceMode::LocalFileNoSecret,
        secret_ref: None,
        public_config: BTreeMap::from([(
            "lakecat.storage-profile-id".to_string(),
            "shadow-profile".to_string(),
        )]),
    };

    let memory_err = MemoryCatalogStore::new()
        .upsert_storage_profile(profile.clone())
        .await
        .unwrap_err();
    assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
    let memory_message = memory_err.to_string();
    assert!(memory_message.contains("reserved for LakeCat credential evidence"));
    assert!(memory_message.contains("public-config-key-hash=sha256:"));
    assert!(!memory_message.contains("lakecat.storage-profile-id"));

    let turso = TursoCatalogStore::in_memory().await.unwrap();
    let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
    assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
    let turso_message = turso_err.to_string();
    assert!(turso_message.contains("reserved for LakeCat credential evidence"));
    assert!(turso_message.contains("public-config-key-hash=sha256:"));
    assert!(!turso_message.contains("lakecat.storage-profile-id"));
    assert_eq!(
        turso.list_storage_profiles(&warehouse).await.unwrap(),
        vec![]
    );
}

#[tokio::test]
async fn turso_store_persists_policy_bindings_and_matches_table_scope() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let binding = PolicyBinding::new(
        "agent-read",
        warehouse.clone(),
        Some(namespace),
        Some(TableName::new("events").unwrap()),
        true,
        serde_json::json!({
            "uid": "policy:agent-read",
            "permission": [{"action": "read"}]
        }),
    )
    .unwrap();
    let inactive = PolicyBinding::new(
        "inactive",
        warehouse.clone(),
        None,
        None,
        false,
        serde_json::json!({"uid": "policy:inactive"}),
    )
    .unwrap();

    store.upsert_policy_binding(binding.clone()).await.unwrap();
    store.upsert_policy_binding(inactive).await.unwrap();

    let policies = store.list_policy_bindings(&warehouse).await.unwrap();
    assert_eq!(policies.len(), 2);
    let active = store.policy_bindings_for_table(&table).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].policy_id, binding.policy_id);
    assert_eq!(
        active[0].odrl["uid"],
        serde_json::json!("policy:agent-read")
    );
}

#[tokio::test]
async fn turso_store_rejects_deserialized_invalid_policy_bindings() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let binding = PolicyBinding {
        policy_id: "table-policy".to_string(),
        warehouse: warehouse.clone(),
        namespace: None,
        table: Some(TableName::new("events").unwrap()),
        enforced: true,
        odrl: serde_json::json!({"uid": "policy:table-policy"}),
        updated_at: Utc::now(),
    };

    let err = store.upsert_policy_binding(binding).await.unwrap_err();

    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table-scoped policy binding requires namespace")
    ));
    assert_eq!(
        store.list_policy_bindings(&warehouse).await.unwrap(),
        vec![]
    );
}

#[tokio::test]
async fn turso_store_rejects_corrupt_policy_bindings_on_read() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let mut binding = PolicyBinding::new(
        "table-policy",
        warehouse.clone(),
        Some(namespace),
        Some(TableName::new("events").unwrap()),
        true,
        serde_json::json!({"uid": "policy:table-policy"}),
    )
    .unwrap();
    store.upsert_policy_binding(binding.clone()).await.unwrap();
    binding.namespace = None;

    let conn = store.connect().unwrap();
    conn.execute(
        "update policy_bindings set binding_json = ?2 where policy_key = ?1",
        (
            policy_binding_key(&warehouse, "table-policy"),
            encode_json(&binding).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store.list_policy_bindings(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table-scoped policy binding requires namespace")
    ));

    let err = store.policy_bindings_for_table(&table).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("table-scoped policy binding requires namespace")
    ));
}

#[tokio::test]
async fn turso_store_rejects_policy_binding_json_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let mut binding = PolicyBinding::new(
        "table-policy",
        warehouse.clone(),
        Some(namespace),
        Some(TableName::new("events").unwrap()),
        true,
        serde_json::json!({"uid": "policy:table-policy"}),
    )
    .unwrap();
    store.upsert_policy_binding(binding.clone()).await.unwrap();
    binding.policy_id = "other-policy".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update policy_bindings set binding_json = ?2 where policy_key = ?1",
        (
            policy_binding_key(&warehouse, "table-policy"),
            encode_json(&binding).unwrap(),
        ),
    )
    .await
    .unwrap();

    let err = store.list_policy_bindings(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("policy binding row scope does not match")
    ));

    let err = store.policy_bindings_for_table(&table).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("policy binding row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_policy_binding_row_column_scope_drift() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let table = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let binding = PolicyBinding::new(
        "table-policy",
        warehouse.clone(),
        Some(namespace),
        Some(TableName::new("events").unwrap()),
        true,
        serde_json::json!({"uid": "policy:table-policy"}),
    )
    .unwrap();
    store.upsert_policy_binding(binding).await.unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update policy_bindings
                 set namespace_path = ?2, table_name = ?3, enforced = ?4
                 where policy_key = ?1",
        (
            policy_binding_key(&warehouse, "table-policy"),
            "other",
            "other_events",
            0_i64,
        ),
    )
    .await
    .unwrap();

    let err = store.list_policy_bindings(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("policy binding row scope does not match")
    ));

    let err = store.policy_bindings_for_table(&table).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("policy binding row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_rejects_policy_binding_scope_drift_before_upsert() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let mut binding = PolicyBinding::new(
        "table-policy",
        warehouse.clone(),
        Some(namespace.clone()),
        Some(TableName::new("events").unwrap()),
        true,
        serde_json::json!({"uid": "policy:table-policy"}),
    )
    .unwrap();
    store.upsert_policy_binding(binding.clone()).await.unwrap();
    binding.policy_id = "other-policy".to_string();

    let conn = store.connect().unwrap();
    conn.execute(
        "update policy_bindings set binding_json = ?2 where policy_key = ?1",
        (
            policy_binding_key(&warehouse, "table-policy"),
            encode_json(&binding).unwrap(),
        ),
    )
    .await
    .unwrap();

    let replacement = PolicyBinding::new(
        "table-policy",
        warehouse.clone(),
        Some(namespace),
        Some(TableName::new("events").unwrap()),
        false,
        serde_json::json!({"uid": "policy:table-policy", "mode": "observe"}),
    )
    .unwrap();
    let err = store.upsert_policy_binding(replacement).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("policy binding row scope does not match")
    ));

    let err = store.list_policy_bindings(&warehouse).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("policy binding row scope does not match")
    ));
}

#[tokio::test]
async fn turso_store_soft_deletes_tables_from_normal_catalog_reads() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
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
    assert_eq!(store.list_tables(&warehouse).await.unwrap().len(), 1);

    let deleted = store
        .soft_delete_table(
            &ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-drop"
            })),
        )
        .await
        .unwrap();
    assert_eq!(deleted.ident, ident);
    assert!(matches!(
        store.load_table(&ident).await,
        Err(LakeCatError::NotFound { .. })
    ));
    assert_eq!(store.list_tables(&warehouse).await.unwrap(), vec![]);
    assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 1);
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 1);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event_type, "table.deleted");
}

#[tokio::test]
async fn turso_store_restores_soft_deleted_tables() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
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
    store
        .soft_delete_table(
            &ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-drop"
            })),
        )
        .await
        .unwrap();
    assert!(matches!(
        store.load_table(&ident).await,
        Err(LakeCatError::NotFound { .. })
    ));

    let restored = store
        .restore_table(
            &ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-restore"
            })),
        )
        .await
        .unwrap();
    assert_eq!(restored.ident, ident);
    assert_eq!(store.load_table(&ident).await.unwrap().ident, ident);
    assert_eq!(store.list_tables(&warehouse).await.unwrap().len(), 1);
    assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 0);
    assert_eq!(store.count_rows("audit_events").await.unwrap(), 2);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 2);
    assert!(
        pending
            .iter()
            .any(|event| event.event_type == "table.restored")
    );
}

#[tokio::test]
async fn turso_store_rejects_corrupt_soft_delete_records_on_restore() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
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
    store
        .soft_delete_table(
            &ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-drop"
            })),
        )
        .await
        .unwrap();
    assert!(matches!(
        store.load_table(&ident).await,
        Err(LakeCatError::NotFound { .. })
    ));

    let corrupt = SoftDeleteRecord {
        table: ident.clone(),
        metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
        version: 1,
        format_version: Some(3),
        principal: Principal::anonymous(),
        authorization_receipt: Some(serde_json::json!({
            "engine": "typesec",
            "allowed": true,
            "action": "table-drop"
        })),
        deleted_at: Utc::now(),
    };
    let conn = store.connect().unwrap();
    conn.execute(
        "update soft_deletes set record_json = ?2 where table_key = ?1",
        (table_key(&ident), encode_json(&corrupt).unwrap()),
    )
    .await
    .unwrap();

    let err = store
        .restore_table(
            &ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-restore"
            })),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("soft-delete version does not match table record")
    ));
    assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 1);
    assert!(matches!(
        store.load_table(&ident).await,
        Err(LakeCatError::NotFound { .. })
    ));
}

#[tokio::test]
async fn turso_store_rejects_soft_delete_row_scope_drift_on_restore() {
    let store = TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
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
    store
        .soft_delete_table(
            &ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-drop"
            })),
        )
        .await
        .unwrap();

    let conn = store.connect().unwrap();
    conn.execute(
        "update soft_deletes set namespace_path = ?2 where table_key = ?1",
        (table_key(&ident), "other_namespace"),
    )
    .await
    .unwrap();

    let err = store
        .restore_table(
            &ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-restore"
            })),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("soft-delete row scope does not match")
    ));
    assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 1);
    assert!(matches!(
        store.load_table(&ident).await,
        Err(LakeCatError::NotFound { .. })
    ));

    let row_key_store = TursoCatalogStore::in_memory().await.unwrap();
    let other_ident = TableIdent::new(
        warehouse.clone(),
        namespace,
        TableName::new("other_events").unwrap(),
    );
    for table_ident in [&ident, &other_ident] {
        row_key_store
            .create_table(TableRecord::new(
                table_ident.clone(),
                format!("file:///tmp/{}", table_ident.name),
                Some(format!(
                    "file:///tmp/{}/metadata/00000.json",
                    table_ident.name
                )),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            ))
            .await
            .unwrap();
    }
    row_key_store
        .soft_delete_table(
            &ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-drop"
            })),
        )
        .await
        .unwrap();

    let conn = row_key_store.connect().unwrap();
    conn.execute(
        "update soft_deletes set table_key = ?2 where table_key = ?1",
        (table_key(&ident), table_key(&other_ident)),
    )
    .await
    .unwrap();

    assert!(matches!(
        row_key_store
            .restore_table(
                &ident,
                Principal::anonymous(),
                Some(serde_json::json!({
                    "engine": "typesec",
                    "allowed": true,
                    "action": "table-restore"
                })),
            )
            .await,
        Err(LakeCatError::NotFound { object, name })
            if object == "soft-deleted table" && name == ident.stable_id()
    ));
    let err = row_key_store
        .restore_table(
            &other_ident,
            Principal::anonymous(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-restore"
            })),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("soft-delete record table does not match requested table")
    ));
    assert_eq!(row_key_store.count_rows("soft_deletes").await.unwrap(), 1);
}
