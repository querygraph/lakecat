use super::*;
use chrono::Utc;
#[cfg(feature = "grust-turso-local")]
use grust_graph::{
    CypherMutationExecutor, GraphAdminStore, GraphMutationCardinality, GraphMutationPlan,
    GraphMutationPlanOp, Label, NodeId, Props, Traversal,
};
use grust_graph::{
    CypherMutationOptions, GraphIndex, GraphStore, MemoryGraphStore, Value,
    execute_cypher_mutation_returning_with_options_on_store,
};
use lakecat_core::{Namespace, TableIdent, TableName, WarehouseName};
use std::sync::Arc;

#[test]
fn converts_server_event_to_valid_grust_graph_event() {
    let event = GraphEvent::server(
        GraphAction::Upserted,
        "prod",
        serde_json::json!({"server-id":"prod"}),
    )
    .with_event_id("lakecat:outbox:server-1");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 0);
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("Server".to_string()))
    );
    assert_eq!(
        graph.nodes[0].props.get("subject"),
        Some(&Value::String("lakecat:server:prod".to_string()))
    );
    GraphIndex::new(&graph).expect("server event graph should be valid");
}

#[test]
fn converts_table_event_to_valid_grust_graph() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let event = GraphEvent::table(
        GraphAction::Created,
        table,
        serde_json::json!({"kind":"test"}),
    )
    .with_event_id("lakecat:outbox:evt-1");
    let graph = graph_event_to_grust(&event);
    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 3);
    assert!(
        graph
            .edges
            .iter()
            .any(|edge| edge.label.as_str() == "AFFECTS_TABLE")
    );
    GraphIndex::new(&graph).expect("event graph should be valid");
}

#[test]
fn converts_policy_event_to_valid_grust_graph_event() {
    let event = GraphEvent::policy(
        GraphAction::Upserted,
        WarehouseName::new("local").unwrap(),
        "agent-read",
        serde_json::json!({"kind":"test"}),
    )
    .with_event_id("lakecat:outbox:policy-1");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 0);
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("Policy".to_string()))
    );
    assert_eq!(
        graph.nodes[0].props.get("action"),
        Some(&Value::String("upserted".to_string()))
    );
    GraphIndex::new(&graph).expect("policy event graph should be valid");
}

#[test]
fn converts_storage_profile_event_to_valid_grust_graph_event() {
    let event = GraphEvent::storage_profile(
        GraphAction::Upserted,
        WarehouseName::new("local").unwrap(),
        "s3-events",
        serde_json::json!({
            "storage-profile": {
                "profile-id": "s3-events",
                "provider": "s3",
                "secret-ref-present": true,
                "secret-ref-provider": "vault"
            }
        }),
    )
    .with_event_id("lakecat:outbox:storage-profile-1");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 0);
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("StorageProfile".to_string()))
    );
    assert_eq!(
        graph.nodes[0].props.get("subject"),
        Some(&Value::String(
            "lakecat:warehouse:local:storage-profile:s3-events".to_string()
        ))
    );
    GraphIndex::new(&graph).expect("storage profile event graph should be valid");
}

#[test]
fn converts_warehouse_event_to_valid_grust_graph_event() {
    let event = GraphEvent::warehouse(
        GraphAction::Upserted,
        WarehouseName::new("local").unwrap(),
        serde_json::json!({"warehouse":"local"}),
    )
    .with_event_id("lakecat:outbox:warehouse");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 0);
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("Warehouse".to_string()))
    );
    GraphIndex::new(&graph).expect("warehouse event graph should be valid");
}

#[test]
fn converts_project_event_to_valid_grust_graph_event() {
    let event = GraphEvent::project(
        GraphAction::Upserted,
        "default",
        serde_json::json!({"project-id":"default"}),
    )
    .with_event_id("lakecat:outbox:project");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 0);
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("Project".to_string()))
    );
    GraphIndex::new(&graph).expect("project event graph should be valid");
}

#[test]
fn converts_view_event_to_valid_grust_graph_event() {
    let event = GraphEvent::view(
        GraphAction::Upserted,
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        "events_view",
        serde_json::json!({"view":{"name":"events_view"}}),
    )
    .with_event_id("lakecat:outbox:view");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 0);
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("View".to_string()))
    );
    GraphIndex::new(&graph).expect("view event graph should be valid");
}

#[test]
fn converts_scan_plan_event_to_valid_grust_graph_event() {
    let event = GraphEvent::scan_plan(
        GraphAction::PlannedScan,
        "evt-scan",
        serde_json::json!({"kind":"test"}),
    )
    .with_event_id("lakecat:outbox:scan-1:scan-plan");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 0);
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("ScanPlan".to_string()))
    );
    assert_eq!(
        graph.nodes[0].props.get("action"),
        Some(&Value::String("planned-scan".to_string()))
    );
    GraphIndex::new(&graph).expect("scan plan event graph should be valid");
}

#[test]
fn converts_commit_event_to_valid_grust_graph_event() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let event = GraphEvent::commit(
        GraphAction::Committed,
        &table,
        7,
        serde_json::json!({"kind":"test"}),
    )
    .with_event_id("lakecat:outbox:commit-1:commit");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 3);
    assert!(
        graph
            .edges
            .iter()
            .any(|edge| edge.label.as_str() == "AFFECTS_TABLE")
    );
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("Commit".to_string()))
    );
    assert_eq!(
        graph.nodes[0].props.get("action"),
        Some(&Value::String("committed".to_string()))
    );
    GraphIndex::new(&graph).expect("commit event graph should be valid");
}

#[test]
fn converts_principal_event_to_valid_grust_graph_event() {
    let principal =
        lakecat_core::Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent)
            .unwrap();
    let event = GraphEvent::principal(
        GraphAction::Loaded,
        &principal,
        serde_json::json!({"kind":"test"}),
    )
    .with_event_id("lakecat:outbox:evt-1:principal");
    let graph = graph_event_to_grust(&event);

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.edges.len(), 0);
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("Principal".to_string()))
    );
    GraphIndex::new(&graph).expect("principal event graph should be valid");
}

#[test]
fn converts_column_event_to_valid_grust_graph_event() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let event = GraphEvent::column(
        GraphAction::Created,
        &table,
        "1",
        serde_json::json!({"field":{"id":1,"name":"event_id"}}),
    )
    .with_event_id("lakecat:outbox:evt-1:column:1");
    let graph = graph_event_to_grust(&event);

    assert!(graph.nodes.len() >= 2);
    assert!(!graph.edges.is_empty());
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("Column".to_string()))
    );
    GraphIndex::new(&graph).expect("column event graph should be valid");
}

#[test]
fn converts_snapshot_event_to_valid_grust_graph_event() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let event = GraphEvent::snapshot(
        GraphAction::Created,
        &table,
        "42",
        serde_json::json!({"snapshot":{"snapshot-id":42}}),
    )
    .with_event_id("lakecat:outbox:evt-1:snapshot:42");
    let graph = graph_event_to_grust(&event);

    assert!(graph.nodes.len() >= 2);
    assert!(!graph.edges.is_empty());
    assert_eq!(graph.nodes[0].label.as_str(), "CatalogEvent");
    assert_eq!(
        graph.nodes[0].props.get("label"),
        Some(&Value::String("Snapshot".to_string()))
    );
    GraphIndex::new(&graph).expect("snapshot event graph should be valid");
}

#[tokio::test]
async fn grust_cypher_can_query_lakecat_catalog_projection_boundary() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let table_id = table.stable_id();
    let event = GraphEvent::table(
        GraphAction::Created,
        table,
        serde_json::json!({"kind":"test"}),
    )
    .with_event_id("lakecat:outbox:evt-1");
    let graph = graph_event_to_grust(&event);
    let store = MemoryGraphStore::new();
    store.put_graph(&graph).await.expect("catalog graph write");

    let result = execute_cypher_mutation_returning_with_options_on_store(
                &store,
                &format!(
                    "MATCH (t:Table {{id: '{table_id}'}}) SET t.querygraph_ready = true RETURN t.id AS id, t.querygraph_ready AS ready"
                ),
                CypherMutationOptions::default(),
            )
            .await
            .expect("Grust Cypher mutation over LakeCat graph");

    assert_eq!(result.table.columns, vec!["id", "ready"]);
    assert_eq!(
        result.table.rows,
        vec![vec![Value::String(table_id), Value::Bool(true)]]
    );
}

#[tokio::test]
async fn grust_sink_rejects_malformed_catalog_projection_event() {
    let store = Arc::new(MemoryGraphStore::new());
    let sink = GrustCatalogGraphSink::new(store.clone());
    let event = GraphEvent {
        event_id: Some("lakecat:outbox:evt-bad-graph".to_string()),
        subject: "lakecat:commit:missing-table:7".to_string(),
        label: GraphNodeLabel::Commit,
        action: GraphAction::Committed,
        table: None,
        properties: serde_json::json!({"kind": "test"}),
        emitted_at: Utc::now(),
    };

    let err = crate::CatalogGraphSink::emit(sink.as_ref(), event)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("catalog graph event label Commit requires table identity")
    ));
}

#[cfg(feature = "grust-turso-local")]
#[tokio::test]
async fn grust_turso_store_persists_lakecat_catalog_projection_boundary() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let table_id = table.stable_id();
    let event = GraphEvent::table(
        GraphAction::Created,
        table,
        serde_json::json!({"kind":"turso-test"}),
    )
    .with_event_id("lakecat:outbox:evt-turso");
    let graph = graph_event_to_grust(&event);
    let store = grust_turso::TursoGraphStore::in_memory()
        .await
        .expect("Grust Turso graph store");
    store.bootstrap().await.expect("Grust Turso bootstrap");

    store
        .put_graph(&graph)
        .await
        .expect("catalog graph write to Turso");
    let table_node = store
        .get_node(&NodeId::new(table_id.clone()))
        .await
        .expect("catalog table node read from Turso")
        .expect("table node persisted in Turso");

    assert_eq!(table_node.id.as_str(), table_id);
    assert_eq!(table_node.label.as_str(), "Table");
    assert_eq!(
        table_node.props.get("warehouse"),
        Some(&Value::String("local".to_string()))
    );
}

#[cfg(feature = "grust-turso-local")]
#[tokio::test]
async fn grust_turso_sink_emits_lakecat_catalog_projection_boundary() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let table_id = table.stable_id();
    let event = GraphEvent::table(
        GraphAction::Created,
        table,
        serde_json::json!({"kind":"turso-sink-test"}),
    )
    .with_event_id("lakecat:outbox:evt-turso-sink");
    let store = Arc::new(
        grust_turso::TursoGraphStore::in_memory()
            .await
            .expect("Grust Turso graph store"),
    );
    store.bootstrap().await.expect("Grust Turso bootstrap");
    let sink = GrustCatalogGraphSink::new(store.clone());

    crate::CatalogGraphSink::emit(sink.as_ref(), event)
        .await
        .expect("LakeCat graph sink should emit through Grust Turso");
    let table_node = store
        .get_node(&NodeId::new(table_id.clone()))
        .await
        .expect("catalog table node read from Turso")
        .expect("table node persisted by sink in Turso");

    assert_eq!(table_node.id.as_str(), table_id);
    assert_eq!(table_node.label.as_str(), "Table");
    assert_eq!(
        table_node.props.get("warehouse"),
        Some(&Value::String("local".to_string()))
    );
}

#[cfg(feature = "grust-turso-local")]
#[tokio::test]
async fn grust_turso_store_traverses_lakecat_catalog_projection_boundary() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let table_id = table.stable_id();
    let event = GraphEvent::table(
        GraphAction::Created,
        table,
        serde_json::json!({"kind":"turso-cypher-test"}),
    )
    .with_event_id("lakecat:outbox:evt-turso-cypher");
    let graph = graph_event_to_grust(&event);
    let store = grust_turso::TursoGraphStore::in_memory()
        .await
        .expect("Grust Turso graph store");
    store.bootstrap().await.expect("Grust Turso bootstrap");
    store
        .put_graph(&graph)
        .await
        .expect("catalog graph write to Turso");

    let affected_tables = store
        .traverse(
            Traversal::from_node("lakecat:outbox:evt-turso-cypher")
                .out("AFFECTS_TABLE")
                .to("Table"),
        )
        .await
        .expect("Grust Turso traversal over LakeCat graph");

    assert_eq!(affected_tables.len(), 1);
    assert_eq!(affected_tables[0].id.as_str(), table_id);
    assert_eq!(affected_tables[0].label.as_str(), "Table");
    assert_eq!(
        affected_tables[0].props.get("warehouse"),
        Some(&Value::String("local".to_string()))
    );
}

#[cfg(feature = "grust-turso-local")]
#[tokio::test]
async fn grust_turso_store_runs_cypher_over_lakecat_catalog_projection_boundary() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let table_id = table.stable_id();
    let event = GraphEvent::table(
        GraphAction::Created,
        table,
        serde_json::json!({"kind":"turso-cypher-test"}),
    )
    .with_event_id("lakecat:outbox:evt-turso-cypher-query");
    let graph = graph_event_to_grust(&event);
    let store = grust_turso::TursoGraphStore::in_memory()
        .await
        .expect("Grust Turso graph store");
    store.bootstrap().await.expect("Grust Turso bootstrap");
    store
        .put_graph(&graph)
        .await
        .expect("catalog graph write to Turso");

    let result = execute_cypher_mutation_returning_with_options_on_store(
                &store,
                &format!(
                    "MATCH (t:Table {{id: '{table_id}'}}) SET t.querygraph_ready = true RETURN t.id AS id, t.querygraph_ready AS ready"
                ),
                CypherMutationOptions::default(),
            )
            .await
            .expect("Grust Cypher mutation over Turso-backed LakeCat graph");

    assert_eq!(result.table.columns, vec!["id", "ready"]);
    assert_eq!(
        result.table.rows,
        vec![vec![Value::String(table_id), Value::Bool(true)]]
    );
}

#[cfg(feature = "grust-turso-local")]
#[tokio::test]
async fn grust_turso_store_patches_lakecat_catalog_projection_nodes() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let table_id = table.stable_id();
    let event = GraphEvent::table(
        GraphAction::Created,
        table,
        serde_json::json!({"kind":"turso-matched-node-test"}),
    )
    .with_event_id("lakecat:outbox:evt-turso-matched-node");
    let graph = graph_event_to_grust(&event);
    let store = grust_turso::TursoGraphStore::in_memory()
        .await
        .expect("Grust Turso graph store");
    store.bootstrap().await.expect("Grust Turso bootstrap");
    store
        .put_graph(&graph)
        .await
        .expect("catalog graph write to Turso");

    let report = store
        .execute_cypher_mutation_plan(&GraphMutationPlan::new(vec![
            GraphMutationPlanOp::PatchMatchingNodes {
                label: Some(Label::new("Table")),
                props: Props::from([("id".to_string(), Value::from(table_id.as_str()))]),
                predicates: Vec::new(),
                patch: Props::from([("querygraph_ready".to_string(), Value::from(true))]),
                cardinality: GraphMutationCardinality::SingleIdentity,
            },
        ]))
        .await
        .expect("Grust Turso matched-node patch over LakeCat graph");

    assert_eq!(report.matched_rows, 1);
    assert_eq!(report.node_patches, 1);
    assert_eq!(report.changed_nodes, 1);
    let table_node = store
        .get_node(&NodeId::new(table_id.clone()))
        .await
        .expect("catalog table node read from Turso")
        .expect("table node patched in Turso");
    assert_eq!(
        table_node.props.get("querygraph_ready"),
        Some(&Value::Bool(true))
    );
}

#[tokio::test]
async fn grust_cypher_can_query_catalog_event_taxonomy_labels() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let principal =
        lakecat_core::Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent)
            .unwrap();
    let events = vec![
        GraphEvent::column(
            GraphAction::Created,
            &table,
            "1",
            serde_json::json!({"field":{"id":1,"name":"event_id"}}),
        )
        .with_event_id("lakecat:outbox:evt-1:column:1"),
        GraphEvent::snapshot(
            GraphAction::Created,
            &table,
            "42",
            serde_json::json!({"snapshot":{"snapshot-id":42}}),
        )
        .with_event_id("lakecat:outbox:evt-1:snapshot:42"),
        GraphEvent::commit(
            GraphAction::Committed,
            &table,
            7,
            serde_json::json!({"sequence-number":7}),
        )
        .with_event_id("lakecat:outbox:evt-1:commit"),
        GraphEvent::principal(
            GraphAction::Loaded,
            &principal,
            serde_json::json!({"principal-kind":"agent"}),
        )
        .with_event_id("lakecat:outbox:evt-1:principal"),
        GraphEvent::scan_plan(
            GraphAction::PlannedScan,
            "evt-scan",
            serde_json::json!({"read-restriction":{"allowed-columns":["event_id"]}}),
        )
        .with_event_id("lakecat:outbox:evt-1:scan-plan"),
    ];
    let store = MemoryGraphStore::new();
    for event in events {
        let graph = graph_event_to_grust(&event);
        store.put_graph(&graph).await.expect("catalog graph write");
    }

    let result = execute_cypher_mutation_returning_with_options_on_store(
                &store,
                "MATCH (e:CatalogEvent {label: 'Column'}) SET e.querygraph_seen = true RETURN e.subject AS subject, e.action AS action, e.querygraph_seen AS seen",
                CypherMutationOptions::default(),
            )
            .await
            .expect("Grust Cypher mutation over LakeCat column event");

    assert_eq!(result.table.columns, vec!["subject", "action", "seen"]);
    assert_eq!(
        result.table.rows,
        vec![vec![
            Value::String("lakecat:column:lakecat:table:local:default:events:1".to_string(),),
            Value::String("created".to_string()),
            Value::Bool(true),
        ]]
    );

    let result = execute_cypher_mutation_returning_with_options_on_store(
                &store,
                "MATCH (e:CatalogEvent {label: 'Snapshot'}) SET e.querygraph_seen = true RETURN e.subject AS subject, e.action AS action, e.querygraph_seen AS seen",
                CypherMutationOptions::default(),
            )
            .await
            .expect("Grust Cypher query over LakeCat snapshot event");

    assert_eq!(result.table.columns, vec!["subject", "action", "seen"]);
    assert_eq!(
        result.table.rows,
        vec![vec![
            Value::String("lakecat:snapshot:lakecat:table:local:default:events:42".to_string(),),
            Value::String("created".to_string()),
            Value::Bool(true),
        ]]
    );
}
