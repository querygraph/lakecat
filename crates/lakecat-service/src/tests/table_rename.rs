use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use http::{Method, Request, StatusCode};
use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
use lakecat_graph::GraphAction;
use lakecat_lineage::LineageEventType;
use lakecat_security::CatalogAction;
use lakecat_store::{
    CatalogStore, MemoryCatalogStore, PolicyBinding, ProjectRecord, TableRecord, WarehouseRecord,
};
use serde_json::json;
use tower::ServiceExt;

use super::common::{RecordingGovernance, RecordingGraph, RecordingLineage};
use crate::*;

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
            "table-uuid": "7c608e39-c327-41c1-a5aa-7a126915f955",
            "current-snapshot-id": 0,
        }),
        Principal::anonymous(),
    )
}

async fn post_rename(
    app: &axum::Router,
    uri: &str,
    source: &TableIdent,
    destination: &TableIdent,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "source": {
                            "namespace": source.namespace.parts(),
                            "name": source.name.as_str(),
                        },
                        "destination": {
                            "namespace": destination.namespace.parts(),
                            "name": destination.name.as_str(),
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn table_rename_preserves_multipart_identity_authorization_and_replay() {
    let warehouse = WarehouseName::new("local").unwrap();
    let source_namespace = Namespace::new(vec!["a.b".to_string(), "source".to_string()]).unwrap();
    let destination_namespace =
        Namespace::new(vec!["a.b".to_string(), "destination".to_string()]).unwrap();
    let source = ident(&warehouse, &source_namespace, "events");
    let destination = ident(&warehouse, &destination_namespace, "renamed_events");
    let store = MemoryCatalogStore::new();
    for namespace in [&source_namespace, &destination_namespace] {
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .unwrap();
    }
    store.create_table(table(source.clone())).await.unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "source-table-policy",
                warehouse.clone(),
                Some(source_namespace.clone()),
                Some(source.name.clone()),
                true,
                json!({"@type": "Policy"}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    store
        .upsert_policy_binding(
            PolicyBinding::new(
                "destination-namespace-policy",
                warehouse.clone(),
                Some(destination_namespace.clone()),
                None,
                true,
                json!({"@type": "Policy"}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let governance = Arc::new(RecordingGovernance::default());
    let graph = Arc::new(RecordingGraph::default());
    let lineage = Arc::new(RecordingLineage::default());
    let state = LakeCatState::new(warehouse, store.clone()).with_integrations(
        default_sail_engine(),
        governance.clone(),
        graph.clone(),
        lineage.clone(),
    );
    let app = app(state.clone());

    let response = post_rename(&app, "/catalog/v1/tables/rename", &source, &destination).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(store.load_table(&source).await.is_err());
    assert_eq!(
        store.load_table(&destination).await.unwrap().ident,
        destination
    );

    assert_eq!(
        governance.actions.lock().await.as_slice(),
        &[CatalogAction::TableRename]
    );
    let contexts = governance.contexts.lock().await;
    let context = contexts.last().unwrap();
    assert_eq!(context["destination-table"], json!(&destination));
    let source_policy_ids = context["policy-bindings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|binding| binding["policy-id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(source_policy_ids.contains(&"source-table-policy"));
    let destination_policy_ids = context["destination-policy-bindings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|binding| binding["policy-id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(destination_policy_ids.contains(&"destination-namespace-policy"));
    drop(contexts);

    let response = drain_outbox_once(&state, 10).await.unwrap();
    assert_eq!(response.delivered, 1);
    assert_eq!(response.event_types, ["table.renamed"]);
    assert_eq!(response.graph_events, 1);
    assert_eq!(response.lineage_events, 1);
    let graph_events = graph.events.lock().await;
    assert_eq!(graph_events.len(), 1);
    assert_eq!(graph_events[0].action, GraphAction::Renamed);
    assert_eq!(graph_events[0].table.as_ref(), Some(&destination));
    assert_eq!(graph_events[0].properties["source"], json!(&source));
    drop(graph_events);
    let lineage_events = lineage.events.lock().await;
    assert_eq!(lineage_events.len(), 1);
    assert_eq!(lineage_events[0].event_type, LineageEventType::TableRenamed);
    assert_eq!(lineage_events[0].table.as_ref(), Some(&destination));
    assert_eq!(lineage_events[0].payload["source"], json!(&source));
}

#[tokio::test]
async fn warehouse_prefixed_table_rename_returns_standard_errors_without_partial_state() {
    let warehouse = WarehouseName::new("local").unwrap();
    let source_namespace = "source".parse::<Namespace>().unwrap();
    let destination_namespace = "destination".parse::<Namespace>().unwrap();
    let source = ident(&warehouse, &source_namespace, "events");
    let occupied = ident(&warehouse, &destination_namespace, "occupied");
    let free_destination = ident(&warehouse, &destination_namespace, "renamed_events");
    let missing_source = ident(&warehouse, &source_namespace, "missing");
    let missing_namespace = "missing_namespace".parse::<Namespace>().unwrap();
    let missing_namespace_destination = ident(&warehouse, &missing_namespace, "events");
    let store = MemoryCatalogStore::new();
    store
        .upsert_project(
            ProjectRecord::new(
                "default",
                None,
                Some("Default".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    store
        .upsert_warehouse(
            WarehouseRecord::new(
                warehouse.clone(),
                "default",
                Some("file:///tmp".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    for namespace in [&source_namespace, &destination_namespace] {
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .unwrap();
    }
    store.create_table(table(source.clone())).await.unwrap();
    store.create_table(table(occupied.clone())).await.unwrap();
    let app = app(LakeCatState::new(warehouse, store.clone()));

    let response = post_rename(
        &app,
        "/catalog/v1/local/tables/rename",
        &missing_source,
        &free_destination,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = post_rename(&app, "/catalog/v1/local/tables/rename", &source, &occupied).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(error["error"]["type"], json!("AlreadyExistsException"));

    let response = post_rename(
        &app,
        "/catalog/v1/local/tables/rename",
        &source,
        &missing_namespace_destination,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(store.load_table(&source).await.unwrap().ident, source);
    assert_eq!(store.load_table(&occupied).await.unwrap().ident, occupied);
    assert!(
        store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap()
            .is_empty()
    );

    let response = post_rename(
        &app,
        "/catalog/v1/local/tables/rename",
        &source,
        &free_destination,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        store.load_table(&free_destination).await.unwrap().ident,
        free_destination
    );
}

#[tokio::test]
async fn table_rename_evidence_rejects_identity_drift_before_projection() {
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default".parse::<Namespace>().unwrap();
    let source = ident(&warehouse, &namespace, "events");
    let destination = ident(&warehouse, &namespace, "renamed_events");
    let store = MemoryCatalogStore::new();
    store.create_namespace(&warehouse, namespace).await.unwrap();
    store.create_table(table(source.clone())).await.unwrap();
    store
        .rename_table(
            &source,
            &destination,
            Principal::anonymous(),
            Some(json!({
                "principal": Principal::anonymous(),
                "action": "table-rename",
                "table": &source,
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "context": {"destination-table": &destination},
                "checked_at": chrono::Utc::now(),
            })),
        )
        .await
        .unwrap();
    let mut event = store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
        .await
        .unwrap()
        .pop()
        .unwrap();
    event.payload["payload"]["source"] = json!(&destination);
    event.event_id = lakecat_core::content_hash_json(&event.payload).unwrap();

    let err = validate_outbox_event_evidence(&event).unwrap_err();
    assert!(
        err.to_string()
            .contains("source and destination must differ")
    );
}
