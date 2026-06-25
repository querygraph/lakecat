use std::collections::BTreeMap;

use lakecat_core::{Principal, TableName};

use super::*;

#[tokio::test]
async fn memory_store_persists_server_records() {
    let store = MemoryCatalogStore::new();
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
async fn memory_store_rejects_corrupt_server_records_on_read() {
    let store = MemoryCatalogStore::new();
    let record = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        Some("http://127.0.0.1:8181".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(record).await.unwrap();

    store
        .state
        .write()
        .await
        .servers
        .get_mut("lakecat-local")
        .unwrap()
        .endpoint_url = Some("http://127.0.0.1:8181?token=secret".to_string());

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
async fn memory_store_rejects_server_record_map_scope_drift() {
    let store = MemoryCatalogStore::new();
    let record = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        Some("http://127.0.0.1:8181".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(record).await.unwrap();

    store
        .state
        .write()
        .await
        .servers
        .get_mut("lakecat-local")
        .unwrap()
        .server_id = "lakecat-other".to_string();

    let err = store.list_servers().await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("server row scope does not match")
    ));
}

#[tokio::test]
async fn memory_store_rejects_server_scope_drift_before_upsert() {
    let store = MemoryCatalogStore::new();
    let record = ServerRecord::new(
        "lakecat-local",
        Some("Local LakeCat".to_string()),
        Some("http://127.0.0.1:8181".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_server(record).await.unwrap();

    store
        .state
        .write()
        .await
        .servers
        .get_mut("lakecat-local")
        .unwrap()
        .server_id = "lakecat-other".to_string();

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
async fn memory_store_persists_warehouse_records() {
    let store = MemoryCatalogStore::new();
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
async fn memory_store_rejects_corrupt_project_parent_before_warehouse_upsert() {
    let store = MemoryCatalogStore::new();
    let project = ProjectRecord::new(
        "default",
        None,
        Some("Default Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();

    store
        .state
        .write()
        .await
        .projects
        .get_mut("default")
        .unwrap()
        .project_id = "other-project".to_string();

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
async fn memory_store_rejects_corrupt_warehouse_records_on_read() {
    let store = MemoryCatalogStore::new();
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
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(record).await.unwrap();

    store
        .state
        .write()
        .await
        .warehouses
        .get_mut("local")
        .unwrap()
        .storage_root = Some("file:///tmp/lakecat?token=secret".to_string());

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
async fn memory_store_rejects_warehouse_record_map_scope_drift() {
    let store = MemoryCatalogStore::new();
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
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(record).await.unwrap();

    store
        .state
        .write()
        .await
        .warehouses
        .get_mut("local")
        .unwrap()
        .warehouse = WarehouseName::new("other").unwrap();

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
async fn memory_store_rejects_warehouse_scope_drift_before_upsert() {
    let store = MemoryCatalogStore::new();
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
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_warehouse(record).await.unwrap();

    store
        .state
        .write()
        .await
        .warehouses
        .get_mut("local")
        .unwrap()
        .warehouse = WarehouseName::new("other").unwrap();

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
async fn memory_store_rejects_corrupt_storage_profiles_on_read() {
    let store = MemoryCatalogStore::new();
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

    let key = storage_profile_key(&warehouse, "s3-events");
    store
        .state
        .write()
        .await
        .storage_profiles
        .get_mut(&key)
        .unwrap()
        .profile_id = "s3-events?token=secret".to_string();

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
async fn memory_store_rejects_storage_profile_map_scope_drift() {
    let store = MemoryCatalogStore::new();
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

    let key = storage_profile_key(&warehouse, "s3-events");
    store
        .state
        .write()
        .await
        .storage_profiles
        .get_mut(&key)
        .unwrap()
        .profile_id = "other-profile".to_string();

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
async fn memory_store_rejects_storage_profile_scope_drift_before_upsert() {
    let store = MemoryCatalogStore::new();
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
    store.upsert_storage_profile(profile).await.unwrap();

    let key = storage_profile_key(&warehouse, "s3-events");
    store
        .state
        .write()
        .await
        .storage_profiles
        .get_mut(&key)
        .unwrap()
        .profile_id = "other-profile".to_string();

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
async fn memory_store_persists_project_records() {
    let store = MemoryCatalogStore::new();
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
async fn memory_store_rejects_corrupt_server_parent_before_project_upsert() {
    let store = MemoryCatalogStore::new();
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

    store
        .state
        .write()
        .await
        .servers
        .get_mut("lakecat-local")
        .unwrap()
        .server_id = "lakecat-other".to_string();

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
async fn memory_store_rejects_corrupt_project_records_on_read() {
    let store = MemoryCatalogStore::new();
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
    let project = ProjectRecord::new(
        "default",
        Some("lakecat-local".to_string()),
        Some("QueryGraph Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();

    store
        .state
        .write()
        .await
        .projects
        .get_mut("default")
        .unwrap()
        .server_id = Some("lakecat-local?token=secret".to_string());

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
async fn memory_store_rejects_project_record_map_scope_drift() {
    let store = MemoryCatalogStore::new();
    let project = ProjectRecord::new(
        "default",
        None,
        Some("QueryGraph Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();

    store
        .state
        .write()
        .await
        .projects
        .get_mut("default")
        .unwrap()
        .project_id = "other-project".to_string();

    for err in [
        store.list_projects().await.unwrap_err(),
        store.list_project_warehouses("default").await.unwrap_err(),
    ] {
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("project row scope does not match")
        ));
    }
}

#[tokio::test]
async fn memory_store_rejects_project_scope_drift_before_upsert() {
    let store = MemoryCatalogStore::new();
    let project = ProjectRecord::new(
        "default",
        None,
        Some("QueryGraph Project".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    store.upsert_project(project).await.unwrap();

    store
        .state
        .write()
        .await
        .projects
        .get_mut("default")
        .unwrap()
        .project_id = "other-project".to_string();

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
async fn memory_store_loads_and_drops_namespaces() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let empty_namespace = "empty".parse::<Namespace>().unwrap();

    assert!(matches!(
        store.load_namespace(&warehouse, &empty_namespace).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "namespace" && name == "empty"
    ));

    store
        .create_namespace(&warehouse, empty_namespace.clone())
        .await
        .unwrap();
    assert_eq!(
        store
            .load_namespace(&warehouse, &empty_namespace)
            .await
            .unwrap(),
        empty_namespace.clone()
    );
    assert_eq!(
        store
            .drop_namespace(&warehouse, &empty_namespace)
            .await
            .unwrap(),
        empty_namespace
    );
    assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);

    let table_namespace = "has_table".parse::<Namespace>().unwrap();
    let table = TableRecord::new(
        TableIdent::new(
            warehouse.clone(),
            table_namespace.clone(),
            TableName::new("events").unwrap(),
        ),
        "file:///tmp/has_table".to_string(),
        Some("file:///tmp/has_table/metadata/00000.json".to_string()),
        serde_json::json!({"format-version": 3}),
        Principal::anonymous(),
    );
    store.create_table(table).await.unwrap();
    assert!(matches!(
        store.drop_namespace(&warehouse, &table_namespace).await,
        Err(LakeCatError::Conflict(message)) if message.contains("tables")
    ));

    let view_namespace = "has_view".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, view_namespace.clone())
        .await
        .unwrap();
    store
        .upsert_view(
            ViewRecord::new(
                warehouse.clone(),
                view_namespace.clone(),
                TableName::new("active_customers").unwrap(),
                "select * from customers",
                "duckdb",
                None,
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(matches!(
        store.drop_namespace(&warehouse, &view_namespace).await,
        Err(LakeCatError::Conflict(message)) if message.contains("views")
    ));

    let policy_namespace = "has_policy".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, policy_namespace.clone())
        .await
        .unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "namespace-policy",
                warehouse.clone(),
                Some(policy_namespace.clone()),
                None,
                true,
                serde_json::json!({"permission": []}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(matches!(
        store.drop_namespace(&warehouse, &policy_namespace).await,
        Err(LakeCatError::Conflict(message)) if message.contains("policy bindings")
    ));
}

#[tokio::test]
async fn memory_store_rejects_corrupt_namespace_drop_dependencies() {
    let warehouse = WarehouseName::new("local").unwrap();

    let store = MemoryCatalogStore::new();
    let table_namespace = "table_scope".parse::<Namespace>().unwrap();
    let table_ident = TableIdent::new(
        warehouse.clone(),
        table_namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store
        .create_table(TableRecord::new(
            table_ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    let drifted_table_ident = TableIdent::new(
        warehouse.clone(),
        table_namespace.clone(),
        TableName::new("other_events").unwrap(),
    );
    store
        .state
        .write()
        .await
        .tables
        .get_mut(&table_key(&table_ident))
        .unwrap()
        .ident = drifted_table_ident;

    let err = store
        .drop_namespace(&warehouse, &table_namespace)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
    assert_eq!(
        store
            .load_namespace(&warehouse, &table_namespace)
            .await
            .unwrap(),
        table_namespace
    );

    let store = MemoryCatalogStore::new();
    let view_namespace = "view_scope".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, view_namespace.clone())
        .await
        .unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        view_namespace.clone(),
        TableName::new("active_customers").unwrap(),
        "select * from customers",
        "duckdb",
        None,
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let view = store.upsert_view(view).await.unwrap();
    store
        .state
        .write()
        .await
        .views
        .get_mut(&view_key(&view))
        .unwrap()
        .name = TableName::new("other_view").unwrap();

    let err = store
        .drop_namespace(&warehouse, &view_namespace)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view record row scope does not match")
    ));
    assert_eq!(
        store
            .load_namespace(&warehouse, &view_namespace)
            .await
            .unwrap(),
        view_namespace
    );

    let store = MemoryCatalogStore::new();
    let policy_namespace = "policy_scope".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, policy_namespace.clone())
        .await
        .unwrap();
    let binding = PolicyBinding::new(
        "namespace-policy",
        warehouse.clone(),
        Some(policy_namespace.clone()),
        None,
        true,
        serde_json::json!({"permission": []}),
    )
    .unwrap();
    store.upsert_policy_binding(binding).await.unwrap();
    store
        .state
        .write()
        .await
        .policy_bindings
        .get_mut(&policy_binding_key(&warehouse, "namespace-policy"))
        .unwrap()
        .policy_id = "other-policy".to_string();

    let err = store
        .drop_namespace(&warehouse, &policy_namespace)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("policy binding row scope does not match")
    ));
    assert_eq!(
        store
            .load_namespace(&warehouse, &policy_namespace)
            .await
            .unwrap(),
        policy_namespace
    );
}

#[tokio::test]
async fn memory_store_rejects_deserialized_invalid_policy_bindings() {
    let store = MemoryCatalogStore::new();
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

#[test]
fn policy_bindings_redact_invalid_policy_ids() {
    let invalid_policy_id = "table-policy?token=secret";
    let err = PolicyBinding::new(
        invalid_policy_id,
        WarehouseName::new("local").unwrap(),
        Some("default".parse::<Namespace>().unwrap()),
        Some(TableName::new("events").unwrap()),
        true,
        serde_json::json!({"uid": "policy:table-policy"}),
    )
    .unwrap_err();

    assert!(matches!(err, LakeCatError::InvalidArgument(_)));
    let message = err.to_string();
    assert!(message.contains("policy id contains unsupported characters"));
    assert!(message.contains("policy-id-hash=sha256:"));
    assert!(!message.contains(invalid_policy_id));
    assert!(!message.contains("token=secret"));
}

#[tokio::test]
async fn memory_store_rejects_corrupt_policy_bindings_on_read() {
    let store = MemoryCatalogStore::new();
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

    let key = policy_binding_key(&warehouse, "table-policy");
    store
        .state
        .write()
        .await
        .policy_bindings
        .get_mut(&key)
        .unwrap()
        .namespace = None;

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
async fn memory_store_rejects_policy_binding_map_scope_drift() {
    let store = MemoryCatalogStore::new();
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

    let key = policy_binding_key(&warehouse, "table-policy");
    store
        .state
        .write()
        .await
        .policy_bindings
        .get_mut(&key)
        .unwrap()
        .policy_id = "other-policy".to_string();

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
async fn memory_store_rejects_policy_binding_scope_drift_before_upsert() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let binding = PolicyBinding::new(
        "table-policy",
        warehouse.clone(),
        Some(namespace.clone()),
        Some(TableName::new("events").unwrap()),
        true,
        serde_json::json!({"uid": "policy:table-policy"}),
    )
    .unwrap();
    store.upsert_policy_binding(binding).await.unwrap();

    let key = policy_binding_key(&warehouse, "table-policy");
    store
        .state
        .write()
        .await
        .policy_bindings
        .get_mut(&key)
        .unwrap()
        .policy_id = "other-policy".to_string();

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
async fn memory_store_persists_view_records() {
    let store = MemoryCatalogStore::new();
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
async fn memory_store_rejects_corrupt_view_records_on_read() {
    let store = MemoryCatalogStore::new();
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

    store
        .state
        .write()
        .await
        .views
        .get_mut(&view_key(&view))
        .unwrap()
        .sql = "   ".to_string();

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
async fn memory_store_rejects_view_record_map_scope_drift() {
    let store = MemoryCatalogStore::new();
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
    store
        .state
        .write()
        .await
        .views
        .get_mut(&original_view_key)
        .unwrap()
        .name = TableName::new("other_view").unwrap();

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

    let state = store.state.read().await;
    assert_eq!(state.view_version_receipts.len(), 1);
}

#[tokio::test]
async fn memory_store_rejects_corrupt_view_receipts_on_read() {
    let store = MemoryCatalogStore::new();
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

    store
        .state
        .write()
        .await
        .view_version_receipts
        .first_mut()
        .unwrap()
        .receipt
        .view_hash = "sha256:short".to_string();

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
async fn memory_store_rejects_corrupt_view_receipt_chain_links_on_read() {
    let store = MemoryCatalogStore::new();
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

    let forged_hash = content_hash_json(&serde_json::json!({"forged": "previous"})).unwrap();
    store
        .state
        .write()
        .await
        .view_version_receipts
        .get_mut(1)
        .unwrap()
        .receipt
        .previous_receipt_hash = Some(forged_hash);

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
async fn memory_store_rejects_view_receipt_row_scope_drift() {
    let store = MemoryCatalogStore::new();
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

    store
        .state
        .write()
        .await
        .view_version_receipts
        .first_mut()
        .unwrap()
        .receipt
        .name = TableName::new("shadow_customers").unwrap();

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
    let err = store
        .upsert_view_if_version(replacement, Some(1))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt row scope does not match")
    ));

    let err = store
        .drop_view(&warehouse, &namespace, &view_name, Principal::anonymous())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("view receipt row scope does not match")
    ));
}

#[tokio::test]
async fn memory_store_rejects_corrupt_view_receipt_chain_before_mutation() {
    let store = MemoryCatalogStore::new();
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

    let forged_hash = content_hash_json(&serde_json::json!({"forged": "previous"})).unwrap();
    store
        .state
        .write()
        .await
        .view_version_receipts
        .get_mut(1)
        .unwrap()
        .receipt
        .previous_receipt_hash = Some(forged_hash);

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

    let state = store.state.read().await;
    let active = state
        .views
        .get(&view_key_parts(&warehouse, &namespace, &view_name))
        .unwrap();
    assert_eq!(active.view_version, 2);
    assert_eq!(state.view_version_receipts.len(), 2);
}

#[tokio::test]
async fn memory_store_rejects_corrupt_soft_delete_records_on_restore() {
    let store = MemoryCatalogStore::new();
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

    let key = table_key(&ident);
    store
        .state
        .write()
        .await
        .soft_deletes
        .get_mut(&key)
        .unwrap()
        .version += 1;

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
    assert!(store.state.read().await.soft_deletes.contains_key(&key));
    assert!(matches!(
        store.load_table(&ident).await,
        Err(LakeCatError::NotFound { .. })
    ));
}

#[tokio::test]
async fn memory_store_rejects_soft_delete_map_scope_drift_on_restore() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    let other_ident = TableIdent::new(
        warehouse,
        namespace,
        TableName::new("other_events").unwrap(),
    );
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
        .soft_delete_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap();

    let key = table_key(&ident);
    let other_key = table_key(&other_ident);
    let record = store.state.write().await.soft_deletes.remove(&key).unwrap();
    store
        .state
        .write()
        .await
        .soft_deletes
        .insert(other_key, record);

    assert!(matches!(
        store
            .restore_table(&ident, Principal::anonymous(), None)
            .await,
        Err(LakeCatError::NotFound { object, name })
            if object == "soft-deleted table" && name == ident.stable_id()
    ));
    let err = store
        .restore_table(&other_ident, Principal::anonymous(), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("soft-delete row scope does not match")
    ));
    assert_eq!(store.state.read().await.soft_deletes.len(), 1);
}

#[tokio::test]
async fn memory_store_records_table_lifecycle_audit_outbox_events() {
    let store = MemoryCatalogStore::new();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace,
        TableName::new("events").unwrap(),
    );
    let deleter =
        Principal::new("did:example:deleter", lakecat_core::PrincipalKind::Agent).unwrap();
    let restorer =
        Principal::new("did:example:restorer", lakecat_core::PrincipalKind::Agent).unwrap();
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
        .soft_delete_table(
            &ident,
            deleter.clone(),
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
    {
        let state = store.state.read().await;
        assert_eq!(state.soft_deletes.len(), 1);
        assert_eq!(state.audit_events.len(), 1);
        assert_eq!(state.audit_events[0].event_type, "table.deleted");
        assert_eq!(state.audit_events[0].principal, deleter);
        assert_eq!(
            state.audit_events[0].request_hash.as_deref(),
            Some(
                content_hash_json(&state.audit_events[0].payload)
                    .unwrap()
                    .as_str()
            )
        );
        assert_eq!(state.outbox_events.len(), 1);
        assert_eq!(state.outbox_events[0].event_type, "table.deleted");
    }

    store
        .restore_table(
            &ident,
            restorer.clone(),
            Some(serde_json::json!({
                "engine": "typesec",
                "allowed": true,
                "action": "table-restore"
            })),
        )
        .await
        .unwrap();
    assert_eq!(store.load_table(&ident).await.unwrap().ident, ident);
    let state = store.state.read().await;
    assert!(state.soft_deletes.is_empty());
    assert_eq!(state.audit_events.len(), 2);
    assert_eq!(state.audit_events[1].event_type, "table.restored");
    assert_eq!(state.audit_events[1].principal, restorer);
    assert_eq!(
        state.audit_events[1].request_hash.as_deref(),
        Some(
            content_hash_json(&state.audit_events[1].payload)
                .unwrap()
                .as_str()
        )
    );
    assert_eq!(state.outbox_events.len(), 2);
    assert!(
        state
            .outbox_events
            .iter()
            .any(|event| event.event_type == "table.restored")
    );
    drop(state);
    let pending = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap();
    assert_eq!(pending.len(), 2);
}

#[tokio::test]
async fn memory_store_records_and_marks_audit_outbox_events() {
    let store = MemoryCatalogStore::new();
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
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "action": "querygraph.bootstrap"
                    },
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
    assert_eq!(pending[0].sink, "lakecat.lineage-and-graph");
    assert_eq!(pending[0].event_type, "querygraph.bootstrap");
    assert_eq!(
        pending[0].payload["payload"]["authorization-receipt"]["engine"],
        serde_json::json!("typesec")
    );
    assert_eq!(
        pending[0].payload["payload"]["manifest-hash"],
        serde_json::json!("lakecat:test")
    );

    let unrelated = store
        .pending_outbox_events(Some("lakecat.unrelated"), 10)
        .await
        .unwrap();
    assert!(unrelated.is_empty());

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
async fn memory_store_omits_table_from_unscoped_audit_outbox_events() {
    let store = MemoryCatalogStore::new();
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
async fn memory_store_duplicate_audit_write_does_not_duplicate_outbox() {
    let store = MemoryCatalogStore::new();
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
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("duplicate audit event id would duplicate outbox replay evidence")
    ));
    let state = store.state.read().await;
    assert_eq!(state.audit_events.len(), 1);
    assert_eq!(state.outbox_events.len(), 1);
}

#[tokio::test]
async fn memory_store_rejects_malformed_outbox_delivery_ids() {
    let store = MemoryCatalogStore::new();
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
async fn memory_store_validates_pending_outbox_before_delivery() {
    let store = MemoryCatalogStore::new();
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
    let event_id = store.state.read().await.outbox_events[0].event_id.clone();
    store.state.write().await.outbox_events[0].payload["event-type"] =
        serde_json::json!("querygraph.bootstrap.drifted");

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
    assert!(
        store.state.read().await.outbox_events[0]
            .delivered_at
            .is_none()
    );
}

#[tokio::test]
async fn memory_store_rejects_partial_outbox_delivery_on_batch_drift() {
    let store = MemoryCatalogStore::new();
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
        .state
        .read()
        .await
        .outbox_events
        .iter()
        .map(|event| event.event_id.clone())
        .collect::<Vec<_>>();
    store.state.write().await.outbox_events[1].payload["event-type"] =
        serde_json::json!("querygraph.bootstrap.drifted");

    let err = store.mark_outbox_delivered(&event_ids).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox event type does not match payload")
    ));
    assert!(
        store
            .state
            .read()
            .await
            .outbox_events
            .iter()
            .all(|event| event.delivered_at.is_none())
    );
}

#[tokio::test]
async fn memory_store_rejects_corrupt_pending_outbox_event_ids() {
    let store = MemoryCatalogStore::new();
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

    store.state.write().await.outbox_events[0].event_id = "sha256:short".to_string();

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox event id does not match payload hash")
                && message.contains("event-id-hash=sha256:")
                && message.contains("event-type-hash=sha256:")
                && message.contains("payload-hash=sha256:")
        && !message.contains("sha256:short")
    ));
}

#[tokio::test]
async fn memory_store_rejects_blank_pending_outbox_event_types() {
    let store = MemoryCatalogStore::new();
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

    let blank_payload = serde_json::json!({
        "event-type": " ",
        "manifest-hash": "lakecat:test"
    });
    let blank_payload_hash = content_hash_json(&blank_payload).unwrap();
    {
        let mut state = store.state.write().await;
        state.outbox_events[0].event_id = blank_payload_hash;
        state.outbox_events[0].event_type = " ".to_string();
        state.outbox_events[0].payload = blank_payload;
    }

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox event type must not be empty")
                && message.contains("event-id-hash=sha256:")
                && message.contains("event-type-hash=sha256:")
                && message.contains("payload-hash=sha256:")
                && !message.contains("manifest-hash")
                && !message.contains("lakecat:test")
    ));
}

#[tokio::test]
async fn memory_store_rejects_blank_pending_outbox_payload_event_types() {
    let store = MemoryCatalogStore::new();
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

    let blank_payload = serde_json::json!({
        "event-type": " ",
        "manifest-hash": "lakecat:test"
    });
    let blank_payload_hash = content_hash_json(&blank_payload).unwrap();
    {
        let mut state = store.state.write().await;
        state.outbox_events[0].event_id = blank_payload_hash;
        state.outbox_events[0].payload = blank_payload;
    }

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox payload event-type must not be empty")
                && message.contains("event-id-hash=sha256:")
                && message.contains("event-type-hash=sha256:")
                && message.contains("payload-event-type-hash=sha256:")
                && message.contains("payload-hash=sha256:")
                && !message.contains("manifest-hash")
        && !message.contains("lakecat:test")
    ));
}

#[tokio::test]
async fn memory_store_rejects_missing_pending_outbox_payload_event_types() {
    let store = MemoryCatalogStore::new();
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

    let missing_payload = serde_json::json!({
        "manifest-hash": "lakecat:test"
    });
    let missing_payload_hash = content_hash_json(&missing_payload).unwrap();
    {
        let mut state = store.state.write().await;
        state.outbox_events[0].event_id = missing_payload_hash;
        state.outbox_events[0].payload = missing_payload;
    }

    let err = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox payload missing event-type")
                && message.contains("event-id-hash=sha256:")
                && message.contains("event-type-hash=sha256:")
                && message.contains("payload-hash=sha256:")
                && !message.contains("manifest-hash")
                && !message.contains("lakecat:test")
    ));
}

#[tokio::test]
async fn memory_store_rejects_blank_pending_outbox_sinks() {
    let store = MemoryCatalogStore::new();
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

    store.state.write().await.outbox_events[0].sink = " ".to_string();

    let err = store.pending_outbox_events(None, 10).await.unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("pending outbox event sink must not be empty")
                && message.contains("event-id-hash=sha256:")
                && message.contains("event-type-hash=sha256:")
                && message.contains("payload-hash=sha256:")
                && !message.contains("manifest-hash")
                && !message.contains("lakecat:test")
    ));
}

#[tokio::test]
async fn memory_store_rejects_audit_event_type_drift_before_outbox() {
    let store = MemoryCatalogStore::new();
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
    let state = store.state.read().await;
    assert!(state.audit_events.is_empty());
    assert!(state.outbox_events.is_empty());
}

#[tokio::test]
async fn memory_store_rejects_audit_events_without_request_hash() {
    let store = MemoryCatalogStore::new();
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
    let state = store.state.read().await;
    assert!(state.audit_events.is_empty());
    assert!(state.outbox_events.is_empty());
}

#[tokio::test]
async fn memory_store_rejects_audit_request_hash_drift_before_outbox() {
    let store = MemoryCatalogStore::new();
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
    let state = store.state.read().await;
    assert!(state.audit_events.is_empty());
    assert!(state.outbox_events.is_empty());
}

#[tokio::test]
async fn memory_store_rejects_audit_payload_table_scope_drift() {
    let store = MemoryCatalogStore::new();
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
    let state = store.state.read().await;
    assert!(state.audit_events.is_empty());
    assert!(state.outbox_events.is_empty());
}

#[tokio::test]
async fn memory_store_rejects_bare_table_name_audit_payload_scope() {
    let store = MemoryCatalogStore::new();
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
    let state = store.state.read().await;
    assert!(state.audit_events.is_empty());
    assert!(state.outbox_events.is_empty());
}

#[tokio::test]
async fn memory_store_rejects_audit_authorization_principal_drift() {
    let store = MemoryCatalogStore::new();
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
    let state = store.state.read().await;
    assert!(state.audit_events.is_empty());
    assert!(state.outbox_events.is_empty());
}

#[tokio::test]
async fn memory_store_rejects_audit_authorization_receipts_without_action() {
    let store = MemoryCatalogStore::new();
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
    let state = store.state.read().await;
    assert!(state.audit_events.is_empty());
    assert!(state.outbox_events.is_empty());
}

#[tokio::test]
async fn memory_store_orders_pending_outbox_events_deterministically() {
    let store = MemoryCatalogStore::new();
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
        let audit_event_id = audit_event_id(&event).unwrap();
        let outbox_payload = audit_outbox_payload(&audit_event_id, &event);
        let outbox_event = outbox_event_from_payload(&outbox_payload, event.created_at)
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
async fn memory_store_limits_pending_outbox_after_deterministic_ordering() {
    let store = MemoryCatalogStore::new();
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
        let audit_event_id = audit_event_id(&event).unwrap();
        let outbox_payload = audit_outbox_payload(&audit_event_id, &event);
        let outbox_event = outbox_event_from_payload(&outbox_payload, event.created_at)
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
async fn memory_store_rejects_deserialized_empty_table_locations() {
    let store = MemoryCatalogStore::new();
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
async fn memory_store_rejects_deserialized_invalid_table_metadata() {
    let store = MemoryCatalogStore::new();
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
    non_object_metadata.metadata = serde_json::json!(["not", "metadata"]);
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
async fn memory_store_rejects_table_record_map_scope_drift() {
    let store = MemoryCatalogStore::new();
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

    let key = table_key(&ident);
    store
        .state
        .write()
        .await
        .tables
        .get_mut(&key)
        .unwrap()
        .ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("other_events").unwrap(),
    );

    let commit = TableCommit {
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
    for err in [
        store.load_table(&ident).await.unwrap_err(),
        store.list_tables(&warehouse).await.unwrap_err(),
        store.commit_table(&ident, commit).await.unwrap_err(),
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

    let state = store.state.read().await;
    assert!(state.commits.is_empty());
    assert!(state.audit_events.is_empty());
    assert!(state.outbox_events.is_empty());
    assert!(state.soft_deletes.is_empty());
    drop(state);

    let restore_store = MemoryCatalogStore::new();
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
    restore_store
        .state
        .write()
        .await
        .tables
        .get_mut(&key)
        .unwrap()
        .ident = TableIdent::new(
        warehouse,
        namespace,
        TableName::new("other_events").unwrap(),
    );

    let err = restore_store
        .restore_table(&ident, Principal::anonymous(), None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table record row scope does not match")
    ));
    assert_eq!(restore_store.state.read().await.soft_deletes.len(), 1);
}

#[tokio::test]
async fn memory_store_rejects_deserialized_invalid_table_commits() {
    let store = MemoryCatalogStore::new();
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
    {
        let table = store.load_table(&ident).await.unwrap();
        assert_eq!(table.version, 0);
        assert_eq!(
            table.metadata_location.as_deref(),
            Some("file:///tmp/events/metadata/00000.json")
        );
        let state = store.state.read().await;
        assert!(
            state.commits.is_empty(),
            "invalid idempotency evidence must fail before pointer-log insertion"
        );
        assert!(
            state.audit_events.is_empty(),
            "invalid idempotency evidence must fail before audit insertion"
        );
        assert!(
            state.outbox_events.is_empty(),
            "invalid idempotency evidence must fail before outbox insertion"
        );
        assert!(
            state.idempotency.is_empty(),
            "invalid idempotency evidence must fail before idempotency replay state"
        );
    }

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
    assert_eq!(
        store.table_commit_records(&ident, 0, None).await.unwrap(),
        vec![]
    );
    assert_eq!(store.pending_outbox_events(None, 10).await.unwrap(), vec![]);
    let state = store.state.read().await;
    assert!(
        state.audit_events.is_empty(),
        "invalid commit metadata evidence must fail before audit insertion"
    );
    assert!(
        state.idempotency.is_empty(),
        "invalid commit metadata evidence must fail before idempotency replay state"
    );
}

#[tokio::test]
async fn memory_store_commit_records_table_commit_outbox_event() {
    let store = MemoryCatalogStore::new();
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
    let replayed = store.commit_table(&ident, commit).await.unwrap();
    assert_eq!(replayed.version, 1);

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
        pending[0].payload["commit"]["snapshot_id"],
        serde_json::json!(0)
    );
    assert_eq!(
        pending[0].payload["authorization-receipt"]["engine"],
        serde_json::json!("typesec")
    );
    let state = store.state.read().await;
    let audit_event = state.audit_events.first().expect("commit audit event");
    let audit_payload_hash = content_hash_json(&audit_event.payload).unwrap();
    let commit_request_hash = state
        .commits
        .first()
        .expect("commit pointer-log record")
        .record
        .request_hash
        .clone();
    assert_eq!(
        audit_event.request_hash.as_deref(),
        Some(audit_payload_hash.as_str())
    );
    assert_ne!(
        audit_event.request_hash.as_deref(),
        Some(commit_request_hash.as_str()),
        "audit request hash must bind the audit payload, not the inner commit request"
    );
    assert!(!pending[0].payload.to_string().contains("commit-1"));
}

#[tokio::test]
async fn memory_store_rejects_table_idempotency_request_hash_row_drift() {
    let store = MemoryCatalogStore::new();
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
    {
        let mut state = store.state.write().await;
        let replay = state
            .idempotency
            .get_mut(&format!("{}:commit-1", ident.stable_id()))
            .expect("idempotency replay state");
        replay.request_hash = "sha256:short".to_string();
    }

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
async fn memory_store_rejects_table_idempotency_response_scope_drift() {
    let store = MemoryCatalogStore::new();
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
    {
        let mut state = store.state.write().await;
        let replay = state
            .idempotency
            .get_mut(&format!("{}:commit-1", ident.stable_id()))
            .expect("idempotency replay state");
        replay.response.ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("other_events").unwrap(),
        );
    }

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
                if message.contains("table record row scope does not match")
        ));
    }
}

#[tokio::test]
async fn memory_store_rejects_table_idempotency_map_scope_drift() {
    let store = MemoryCatalogStore::new();
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
    {
        let mut state = store.state.write().await;
        let replay = state
            .idempotency
            .get_mut(&format!("{}:commit-1", ident.stable_id()))
            .expect("idempotency replay state");
        let other_ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("other_events").unwrap(),
        );
        replay.table_key = table_key(&other_ident);
    }

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
                if message.contains("idempotency record row scope does not match")
        ));
    }
}

#[tokio::test]
async fn memory_store_stale_pointer_conflict_uses_location_hashes() {
    let store = MemoryCatalogStore::new();
    let ident = table_ident("local", "default", "events").unwrap();
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
    let message = err.to_string();
    assert!(message.contains("expected-metadata-location-hash=sha256:"));
    assert!(message.contains("actual-metadata-location-hash=sha256:"));
    assert!(!message.contains("stale.json"));
    assert!(!message.contains("00000.json"));
}

#[tokio::test]
async fn memory_store_rejects_malformed_commit_history_records() {
    let store = MemoryCatalogStore::new();
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

    let base_record = store.state.read().await.commits[0].record.clone();

    let mut missing_format_record = base_record.clone();
    missing_format_record.format_version = None;
    store.state.write().await.commits[0].record = missing_format_record;

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
    store.state.write().await.commits[0].record = negative_snapshot_record;

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
    store.state.write().await.commits[0].record = missing_snapshot_record;

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
    store.state.write().await.commits[0].record = decorated_new_metadata_record;

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
    store.state.write().await.commits[0].record = credential_previous_metadata_record;

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

    let mut malformed_response_record = base_record.clone();
    malformed_response_record.response_hash = "sha256:short".to_string();
    store.state.write().await.commits[0].record = malformed_response_record;

    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains(
                "table commit record response hash must be full SHA-256 evidence"
            )
    ));

    let mut malformed_policy_record = base_record;
    malformed_policy_record.policy_hash = Some("sha256:short".to_string());
    store.state.write().await.commits[0].record = malformed_policy_record;

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
async fn memory_store_rejects_commit_history_scope_drift() {
    let store = MemoryCatalogStore::new();
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
        .create_table(TableRecord::new(
            other_ident.clone(),
            "file:///tmp/other_events".to_string(),
            Some("file:///tmp/other_events/metadata/00000.json".to_string()),
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

    let base_commit = store.state.read().await.commits[0].clone();
    store.state.write().await.commits[0].record.table = other_ident.clone();
    let err = store
        .table_commit_records(&ident, 0, None)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::Internal(message)
            if message.contains("table commit record table does not match requested table")
    ));

    store.state.write().await.commits[0] = base_commit;
    store.state.write().await.commits[0].table_key = table_key(&other_ident);
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
