use std::sync::Arc;

use lakecat_core::{LakeCatError, Namespace, Principal, TableIdent, TableName, WarehouseName};
use serde_json::json;

use super::*;

async fn exercise_invalid_snapshot_commit_is_atomic(store: Arc<dyn CatalogStore>) {
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let ident = TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new("events").unwrap(),
    );
    store.create_namespace(&warehouse, namespace).await.unwrap();
    store
        .create_table(TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            json!({
                "format-version": 2,
                "table-uuid": "b3b373c2-017a-4fa8-97f8-7f16cb555402",
                "current-snapshot-id": -1,
            }),
            Principal::anonymous(),
        ))
        .await
        .unwrap();
    let before = store.load_table(&ident).await.unwrap();

    let err = store
        .commit_table(
            &ident,
            TableCommit {
                requirements: Vec::new(),
                updates: vec![json!({"action": "set-properties", "updates": {"state": "bad"}})],
                expected_previous_metadata_location: before.metadata_location.clone(),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(json!({
                    "format-version": 2,
                    "table-uuid": "b3b373c2-017a-4fa8-97f8-7f16cb555402",
                    "current-snapshot-id": -2,
                    "properties": {"state": "bad"},
                })),
                idempotency_key: Some("invalid-snapshot".to_string()),
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
            if message.contains("table commit record snapshot id must be non-negative")
    ));
    assert_eq!(store.load_table(&ident).await.unwrap(), before);
    assert!(
        store
            .table_commit_records(&ident, 0, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .replay_table_commit(
                &ident,
                "invalid-snapshot",
                &format!("sha256:{}", "0".repeat(64))
            )
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn memory_invalid_snapshot_commit_is_atomic() {
    exercise_invalid_snapshot_commit_is_atomic(MemoryCatalogStore::new()).await;
}

#[cfg(feature = "turso-local")]
#[tokio::test]
async fn turso_invalid_snapshot_commit_is_atomic() {
    exercise_invalid_snapshot_commit_is_atomic(
        turso_store::TursoCatalogStore::in_memory().await.unwrap(),
    )
    .await;
}
