use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use http::{Method, Request, StatusCode};
use lakecat_api::ListNamespacesQuery;
use lakecat_core::{Namespace, WarehouseName};
use lakecat_security::CatalogAction;
use lakecat_store::{CatalogStore, MemoryCatalogStore};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::common::RecordingGovernance;
use crate::{LakeCatState, app, namespace_page, parse_rest_namespace};

async fn request_json(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&body).unwrap())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

#[test]
fn rest_namespace_codec_uses_the_iceberg_unit_separator() {
    let namespace = parse_rest_namespace("accounting\u{001f}tax").unwrap();
    assert_eq!(namespace.parts(), &["accounting", "tax"]);

    let dotted_component = parse_rest_namespace("accounting.tax").unwrap();
    assert_eq!(dotted_component.parts(), &["accounting.tax"]);
}

#[test]
fn namespace_pagination_is_immediate_stable_and_token_checked() {
    let parent = Namespace::new(vec!["accounting".to_string()]).unwrap();
    let namespaces = vec![
        parent.clone(),
        Namespace::new(vec!["accounting".to_string(), "tax".to_string()]).unwrap(),
        Namespace::new(vec![
            "accounting".to_string(),
            "tax".to_string(),
            "paid".to_string(),
        ])
        .unwrap(),
        Namespace::new(vec!["sales".to_string()]).unwrap(),
    ];
    let first = namespace_page(
        namespaces.clone(),
        None,
        &ListNamespacesQuery {
            page_token: Some(String::new()),
            page_size: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(first.namespaces, vec![parent.clone()]);
    let second = namespace_page(
        namespaces.clone(),
        None,
        &ListNamespacesQuery {
            page_token: first.next_page_token,
            page_size: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(second.namespaces[0].parts(), &["sales"]);
    assert!(second.next_page_token.is_none());

    let children = namespace_page(namespaces, Some(&parent), &ListNamespacesQuery::default())
        .unwrap()
        .namespaces;
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].parts(), &["accounting", "tax"]);

    let invalid = namespace_page(
        Vec::new(),
        None,
        &ListNamespacesQuery {
            page_token: Some("forged".to_string()),
            page_size: Some(1),
            ..Default::default()
        },
    );
    assert!(invalid.is_err());
}

#[tokio::test]
async fn namespace_routes_preserve_hierarchy_properties_pagination_and_errors() {
    let store = MemoryCatalogStore::new();
    let governance = Arc::new(RecordingGovernance::default());
    let mut state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone());
    state.governance = governance.clone();
    let app = app(state);

    let (status, created) = request_json(
        &app,
        Method::POST,
        "/catalog/v1/namespaces",
        Some(json!({
            "namespace": ["accounting"],
            "properties": {"owner": "finance", "remove": "before"}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["properties"]["owner"], "finance");
    for namespace in [json!(["sales"]), json!(["accounting", "tax"])] {
        let (status, _) = request_json(
            &app,
            Method::POST,
            "/catalog/v1/namespaces",
            Some(json!({"namespace": namespace, "properties": {}})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (status, top_level) = request_json(&app, Method::GET, "/catalog/v1/namespaces", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(top_level["namespaces"], json!([["accounting"], ["sales"]]));
    let (status, children) = request_json(
        &app,
        Method::GET,
        "/catalog/v1/namespaces?parent=accounting",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(children["namespaces"], json!([["accounting", "tax"]]));

    let (status, child) = request_json(
        &app,
        Method::GET,
        "/catalog/v1/namespaces/accounting%1Ftax",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(child["namespace"], json!(["accounting", "tax"]));

    let (status, first_page) = request_json(
        &app,
        Method::GET,
        "/catalog/v1/namespaces?pageToken=&pageSize=1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first_page["namespaces"], json!([["accounting"]]));
    let token = first_page["next-page-token"].as_str().unwrap();
    let (status, second_page) = request_json(
        &app,
        Method::GET,
        &format!("/catalog/v1/namespaces?pageToken={token}&pageSize=1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second_page["namespaces"], json!([["sales"]]));
    assert!(second_page["next-page-token"].is_null());

    let (status, update) = request_json(
        &app,
        Method::POST,
        "/catalog/v1/namespaces/accounting/properties",
        Some(json!({
            "removals": ["remove", "missing"],
            "updates": {"state": "after"}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update["updated"], json!(["state"]));
    assert_eq!(update["removed"], json!(["remove"]));
    assert_eq!(update["missing"], json!(["missing"]));
    let (_, loaded) =
        request_json(&app, Method::GET, "/catalog/v1/namespaces/accounting", None).await;
    assert_eq!(
        loaded["properties"],
        json!({"owner": "finance", "state": "after"})
    );

    let (status, overlap) = request_json(
        &app,
        Method::POST,
        "/catalog/v1/namespaces/accounting/properties",
        Some(json!({
            "removals": ["owner"],
            "updates": {"owner": "other"}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(overlap["error"]["type"], "UnprocessableEntityException");
    let (_, loaded_after_overlap) =
        request_json(&app, Method::GET, "/catalog/v1/namespaces/accounting", None).await;
    assert_eq!(loaded_after_overlap["properties"], loaded["properties"]);

    let (status, duplicate) = request_json(
        &app,
        Method::POST,
        "/catalog/v1/namespaces",
        Some(json!({"namespace": ["accounting"], "properties": {}})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate["error"]["type"], "AlreadyExistsException");
    let (status, missing_parent) = request_json(
        &app,
        Method::GET,
        "/catalog/v1/namespaces?parent=missing",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing_parent["error"]["type"], "NoSuchNamespaceException");

    assert!(
        governance
            .actions
            .lock()
            .await
            .contains(&CatalogAction::NamespaceUpdate)
    );
    let events = store.pending_outbox_events(None, 100).await.unwrap();
    let update_event = events
        .iter()
        .find(|event| event.event_type == "namespace.properties-updated")
        .unwrap();
    let evidence = serde_json::to_string(&update_event.payload).unwrap();
    assert!(!evidence.contains("finance"));
    assert!(!evidence.contains("after"));

    let (status, drain) =
        request_json(&app, Method::POST, "/management/v1/lineage/drain", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        drain["event-types"]
            .as_array()
            .unwrap()
            .contains(&json!("namespace.properties-updated"))
    );
    assert!(
        store
            .pending_outbox_events(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
}
