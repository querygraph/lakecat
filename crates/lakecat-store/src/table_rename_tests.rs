use std::sync::Arc;

use lakecat_core::{
    LakeCatError, Namespace, Principal, PrincipalKind, TableIdent, TableName, WarehouseName,
};
use serde_json::json;

use super::*;

fn ident(warehouse: &WarehouseName, namespace: &Namespace, name: &str) -> TableIdent {
    TableIdent::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new(name).unwrap(),
    )
}

fn table(ident: TableIdent) -> TableRecord {
    TableRecord::new(
        ident,
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        json!({
            "format-version": 3,
            "table-uuid": "b3b373c2-017a-4fa8-97f8-7f16cb555402",
            "current-snapshot-id": 0,
        }),
        Principal::anonymous(),
    )
}

fn rename_receipt(
    principal: &Principal,
    source: &TableIdent,
    destination: &TableIdent,
) -> serde_json::Value {
    json!({
        "principal": principal,
        "action": "table-rename",
        "table": source,
        "allowed": true,
        "engine": "test",
        "policy_hash": null,
        "context": {"destination-table": destination},
        "checked_at": chrono::Utc::now(),
    })
}

async fn exercise_table_rename_contract(store: Arc<dyn CatalogStore>) {
    let warehouse = WarehouseName::new("local").unwrap();
    let source_namespace = "source".parse::<Namespace>().unwrap();
    let destination_namespace = "destination".parse::<Namespace>().unwrap();
    for namespace in [&source_namespace, &destination_namespace] {
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .unwrap();
    }
    let source = ident(&warehouse, &source_namespace, "events");
    let destination = ident(&warehouse, &destination_namespace, "renamed_events");
    store.create_table(table(source.clone())).await.unwrap();
    let committed = store
        .commit_table(
            &source,
            TableCommit {
                requirements: vec![],
                updates: vec![json!({"action": "set-current-snapshot", "snapshot-id": 7})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(json!({
                    "format-version": 3,
                    "table-uuid": "b3b373c2-017a-4fa8-97f8-7f16cb555402",
                    "current-snapshot-id": 7,
                })),
                idempotency_key: Some("rename-contract-commit".to_string()),
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();
    let commit = store
        .table_commit_records(&source, 1, Some(1))
        .await
        .unwrap()
        .pop()
        .unwrap();
    let binding = PolicyBinding::new(
        "events-policy",
        warehouse.clone(),
        Some(source_namespace.clone()),
        Some(source.name.clone()),
        true,
        json!({"@type": "Policy"}),
    )
    .unwrap();
    store.upsert_policy_binding(binding).await.unwrap();
    let principal = Principal::new("did:example:renamer", PrincipalKind::Agent).unwrap();

    let renamed = store
        .rename_table(
            &source,
            &destination,
            principal.clone(),
            Some(rename_receipt(&principal, &source, &destination)),
        )
        .await
        .unwrap();

    assert_eq!(renamed.ident, destination);
    assert_eq!(renamed.location, committed.location);
    assert_eq!(renamed.metadata_location, committed.metadata_location);
    assert_eq!(renamed.metadata, committed.metadata);
    assert_eq!(renamed.created, committed.created);
    assert_eq!(renamed.version, committed.version);
    assert!(renamed.updated_at >= committed.updated_at);
    assert!(matches!(
        store.load_table(&source).await,
        Err(LakeCatError::NotFound { object, name })
            if object == "table" && name == source.stable_id()
    ));
    assert_eq!(store.load_table(&destination).await.unwrap(), renamed);

    assert!(
        store
            .table_commit_records(&source, 0, None)
            .await
            .unwrap()
            .is_empty()
    );
    let renamed_commits = store
        .table_commit_records(&destination, 0, None)
        .await
        .unwrap();
    assert_eq!(renamed_commits.len(), 1);
    assert_eq!(renamed_commits[0].table, destination);
    assert_eq!(renamed_commits[0].request_hash, commit.request_hash);
    assert_eq!(renamed_commits[0].response_hash, commit.response_hash);
    assert!(
        store
            .replay_table_commit(
                &source,
                "rename-contract-commit",
                commit.request_hash.as_str(),
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .replay_table_commit(
                &destination,
                "rename-contract-commit",
                commit.request_hash.as_str(),
            )
            .await
            .unwrap()
            .is_none()
    );

    assert!(
        store
            .policy_bindings_for_table(&source)
            .await
            .unwrap()
            .is_empty()
    );
    let destination_bindings = store.policy_bindings_for_table(&destination).await.unwrap();
    assert_eq!(destination_bindings.len(), 1);
    assert_eq!(
        destination_bindings[0].namespace.as_ref(),
        Some(&destination.namespace)
    );
    assert_eq!(
        destination_bindings[0].table.as_ref(),
        Some(&destination.name)
    );

    let events = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 100)
        .await
        .unwrap();
    let rename_event = events
        .iter()
        .find(|event| event.event_type == "table.renamed")
        .unwrap();
    rename_event.validate_pending().unwrap();
    let payload = rename_event.payload.get("payload").unwrap();
    assert_eq!(payload["table"], json!(&destination));
    assert_eq!(payload["source"], json!(&source));
    assert_eq!(payload["destination"], json!(&destination));
    assert_eq!(
        payload["authorization-receipt"]["action"],
        json!("table-rename")
    );

    let recommitted = store
        .commit_table(
            &destination,
            TableCommit {
                requirements: vec![],
                updates: vec![json!({"action": "set-current-snapshot", "snapshot-id": 8})],
                expected_previous_metadata_location: renamed.metadata_location.clone(),
                new_metadata_location: Some("file:///tmp/events/metadata/00002.json".to_string()),
                new_metadata: Some(json!({
                    "format-version": 3,
                    "table-uuid": "b3b373c2-017a-4fa8-97f8-7f16cb555402",
                    "current-snapshot-id": 8,
                })),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(recommitted.version, 2);
    assert_eq!(
        store
            .table_commit_records(&destination, 0, None)
            .await
            .unwrap()
            .len(),
        2
    );
}

async fn exercise_no_snapshot_commit_rename_contract(store: Arc<dyn CatalogStore>) {
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    let source = ident(&warehouse, &namespace, "empty_events");
    let destination = ident(&warehouse, &namespace, "renamed_empty_events");
    let mut initial = table(source.clone());
    initial.metadata["format-version"] = json!(2);
    initial.metadata["current-snapshot-id"] = json!(-1);
    store.create_table(initial).await.unwrap();

    store
        .commit_table(
            &source,
            TableCommit {
                requirements: Vec::new(),
                updates: vec![json!({
                    "action": "set-properties",
                    "updates": {"catalog-bench.state": "after"},
                })],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(json!({
                    "format-version": 2,
                    "table-uuid": "b3b373c2-017a-4fa8-97f8-7f16cb555402",
                    "current-snapshot-id": -1,
                    "properties": {"catalog-bench.state": "after"},
                })),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();

    let records = store.table_commit_records(&source, 0, None).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].snapshot_id, Some(0));

    store
        .rename_table(&source, &destination, Principal::anonymous(), None)
        .await
        .unwrap();

    assert_eq!(
        store.load_table(&destination).await.unwrap().ident,
        destination
    );
    let renamed_records = store
        .table_commit_records(&destination, 0, None)
        .await
        .unwrap();
    assert_eq!(renamed_records.len(), 1);
    assert_eq!(renamed_records[0].snapshot_id, Some(0));
}

async fn exercise_table_rename_failure_contract(store: Arc<dyn CatalogStore>) {
    let warehouse = WarehouseName::new("local").unwrap();
    let source_namespace = "source".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, source_namespace.clone())
        .await
        .unwrap();
    let source = ident(&warehouse, &source_namespace, "events");
    store.create_table(table(source.clone())).await.unwrap();
    let principal = Principal::anonymous();

    let missing_namespace = "missing".parse::<Namespace>().unwrap();
    let missing_destination = ident(&warehouse, &missing_namespace, "events");
    let event_count = store.pending_outbox_events(None, 100).await.unwrap().len();
    assert!(matches!(
        store
            .rename_table(&source, &missing_destination, principal.clone(), None)
            .await,
        Err(LakeCatError::NotFound { object, name })
            if object == "namespace" && name == missing_namespace.path()
    ));
    assert_eq!(
        store.pending_outbox_events(None, 100).await.unwrap().len(),
        event_count
    );
    assert_eq!(store.load_table(&source).await.unwrap().ident, source);

    let destination_namespace = "destination".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, destination_namespace.clone())
        .await
        .unwrap();
    let occupied = ident(&warehouse, &destination_namespace, "occupied");
    store.create_table(table(occupied.clone())).await.unwrap();
    assert!(matches!(
        store
            .rename_table(&source, &occupied, principal.clone(), None)
            .await,
        Err(LakeCatError::AlreadyExists { object, name })
            if object == "table" && name == occupied.stable_id()
    ));
    assert_eq!(store.load_table(&source).await.unwrap().ident, source);
    assert_eq!(store.load_table(&occupied).await.unwrap().ident, occupied);

    assert!(matches!(
        store
            .rename_table(&source, &source, principal.clone(), None)
            .await,
        Err(LakeCatError::AlreadyExists { object, name })
            if object == "table" && name == source.stable_id()
    ));

    let other_warehouse = WarehouseName::new("other").unwrap();
    let cross_warehouse = ident(&other_warehouse, &destination_namespace, "events");
    assert!(matches!(
        store
            .rename_table(&source, &cross_warehouse, principal.clone(), None)
            .await,
        Err(LakeCatError::InvalidArgument(message))
            if message.contains("cannot cross warehouses")
    ));

    let hidden = ident(&warehouse, &source_namespace, "hidden");
    store.create_table(table(hidden.clone())).await.unwrap();
    store
        .soft_delete_table(&hidden, principal.clone(), None)
        .await
        .unwrap();
    let hidden_destination = ident(&warehouse, &destination_namespace, "hidden_renamed");
    assert!(matches!(
        store
            .rename_table(&hidden, &hidden_destination, principal, None)
            .await,
        Err(LakeCatError::NotFound { object, name })
            if object == "table" && name == hidden.stable_id()
    ));
    assert!(matches!(
        store.load_table(&hidden).await,
        Err(LakeCatError::NotFound { .. })
    ));
    assert!(matches!(
        store.load_table(&hidden_destination).await,
        Err(LakeCatError::NotFound { .. })
    ));
}

#[tokio::test]
async fn memory_store_satisfies_table_rename_contract() {
    exercise_table_rename_contract(MemoryCatalogStore::new()).await;
}

#[tokio::test]
async fn memory_store_renames_after_no_snapshot_commit() {
    exercise_no_snapshot_commit_rename_contract(MemoryCatalogStore::new()).await;
}

#[tokio::test]
async fn memory_store_rejects_table_rename_without_partial_mutation() {
    exercise_table_rename_failure_contract(MemoryCatalogStore::new()).await;
}

#[cfg(feature = "turso-local")]
#[tokio::test]
async fn turso_store_satisfies_table_rename_contract() {
    exercise_table_rename_contract(turso_store::TursoCatalogStore::in_memory().await.unwrap())
        .await;
}

#[cfg(feature = "turso-local")]
#[tokio::test]
async fn turso_store_renames_after_no_snapshot_commit() {
    exercise_no_snapshot_commit_rename_contract(
        turso_store::TursoCatalogStore::in_memory().await.unwrap(),
    )
    .await;
}

#[cfg(feature = "turso-local")]
#[tokio::test]
async fn turso_store_rejects_table_rename_without_partial_mutation() {
    exercise_table_rename_failure_contract(
        turso_store::TursoCatalogStore::in_memory().await.unwrap(),
    )
    .await;
}

#[cfg(feature = "turso-local")]
#[tokio::test]
async fn turso_concurrent_table_renames_have_one_winner() {
    let store = turso_store::TursoCatalogStore::in_memory().await.unwrap();
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    store
        .create_namespace(&warehouse, namespace.clone())
        .await
        .unwrap();
    let source = ident(&warehouse, &namespace, "events");
    let first_destination = ident(&warehouse, &namespace, "events_first");
    let second_destination = ident(&warehouse, &namespace, "events_second");
    store.create_table(table(source.clone())).await.unwrap();

    let first_store = store.clone();
    let first_source = source.clone();
    let first = tokio::spawn(async move {
        first_store
            .rename_table(
                &first_source,
                &first_destination,
                Principal::anonymous(),
                None,
            )
            .await
    });
    let second_store = store.clone();
    let second_source = source.clone();
    let second = tokio::spawn(async move {
        second_store
            .rename_table(
                &second_source,
                &second_destination,
                Principal::anonymous(),
                None,
            )
            .await
    });
    let (first, second) = tokio::join!(first, second);
    let results = [first.unwrap(), second.unwrap()];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(LakeCatError::NotFound { .. })))
            .count(),
        1
    );
    assert!(matches!(
        store.load_table(&source).await,
        Err(LakeCatError::NotFound { .. })
    ));
    assert_eq!(store.list_tables(&warehouse).await.unwrap().len(), 1);
}
