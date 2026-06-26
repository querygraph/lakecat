use super::*;

#[test]
fn server_event_uses_stable_catalog_subject() {
    let event = GraphEvent::server(
        GraphAction::Upserted,
        "prod",
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Server);
    assert_eq!(event.subject, "lakecat:server:prod");
    assert!(event.table.is_none());
}

#[test]
fn project_event_uses_stable_catalog_subject() {
    let event = GraphEvent::project(
        GraphAction::Upserted,
        "default",
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Project);
    assert_eq!(event.subject, "lakecat:project:default");
    assert!(event.table.is_none());
}

#[test]
fn namespace_event_uses_stable_catalog_subject() {
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = "default.ops".parse::<Namespace>().unwrap();
    let event = GraphEvent::namespace(
        GraphAction::Created,
        warehouse,
        namespace,
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Namespace);
    assert_eq!(
        event.subject,
        "lakecat:warehouse:local:namespace:default.ops"
    );
    assert!(event.table.is_none());
}

#[test]
fn policy_event_uses_stable_catalog_subject() {
    let warehouse = WarehouseName::new("local").unwrap();
    let event = GraphEvent::policy(
        GraphAction::Upserted,
        warehouse,
        "agent-read",
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Policy);
    assert_eq!(event.subject, "lakecat:warehouse:local:policy:agent-read");
    assert!(event.table.is_none());
}

#[test]
fn storage_profile_event_uses_stable_catalog_subject() {
    let warehouse = WarehouseName::new("local").unwrap();
    let event = GraphEvent::storage_profile(
        GraphAction::Upserted,
        warehouse,
        "s3-events",
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::StorageProfile);
    assert_eq!(
        event.subject,
        "lakecat:warehouse:local:storage-profile:s3-events"
    );
    assert!(event.table.is_none());
}

#[test]
fn warehouse_event_uses_stable_catalog_subject() {
    let event = GraphEvent::warehouse(
        GraphAction::Upserted,
        WarehouseName::new("local").unwrap(),
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Warehouse);
    assert_eq!(event.subject, "lakecat:warehouse:local");
    assert!(event.table.is_none());
}

#[test]
fn view_event_uses_stable_catalog_subject() {
    let event = GraphEvent::view(
        GraphAction::Upserted,
        WarehouseName::new("local").unwrap(),
        "default.analytics".parse::<Namespace>().unwrap(),
        "events_view",
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::View);
    assert_eq!(
        event.subject,
        "lakecat:warehouse:local:namespace:default.analytics:view:events_view"
    );
    assert!(event.table.is_none());
}

#[test]
fn scan_plan_event_uses_stable_catalog_subject() {
    let event = GraphEvent::scan_plan(
        GraphAction::PlannedScan,
        "evt-scan",
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::ScanPlan);
    assert_eq!(event.subject, "lakecat:scan-plan:evt-scan");
    assert!(event.table.is_none());
}

#[test]
fn commit_event_uses_stable_catalog_subject() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        lakecat_core::TableName::new("events").unwrap(),
    );
    let event = GraphEvent::commit(
        GraphAction::Committed,
        &table,
        7,
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Commit);
    assert_eq!(
        event.subject,
        "lakecat:commit:lakecat:table:local:default:events:7"
    );
    assert_eq!(event.table.as_ref(), Some(&table));
}

#[test]
fn principal_event_uses_stable_catalog_subject() {
    let principal =
        Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent).unwrap();
    let event = GraphEvent::principal(
        GraphAction::Loaded,
        &principal,
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Principal);
    assert_eq!(event.subject, "lakecat:principal:did:example:agent");
    assert!(event.table.is_none());
}

#[test]
fn column_event_uses_stable_catalog_subject() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        lakecat_core::TableName::new("events").unwrap(),
    );
    let event = GraphEvent::column(
        GraphAction::Created,
        &table,
        "1",
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Column);
    assert_eq!(
        event.subject,
        "lakecat:column:lakecat:table:local:default:events:1"
    );
    assert_eq!(event.table.as_ref(), Some(&table));
}

#[test]
fn snapshot_event_uses_stable_catalog_subject() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        lakecat_core::TableName::new("events").unwrap(),
    );
    let event = GraphEvent::snapshot(
        GraphAction::Created,
        &table,
        "42",
        serde_json::json!({"kind": "test"}),
    );

    assert_eq!(event.label, GraphNodeLabel::Snapshot);
    assert_eq!(
        event.subject,
        "lakecat:snapshot:lakecat:table:local:default:events:42"
    );
    assert_eq!(event.table.as_ref(), Some(&table));
}

#[test]
fn graph_event_validation_rejects_blank_projection_identity() {
    let mut event = GraphEvent::server(
        GraphAction::Upserted,
        "prod",
        serde_json::json!({"kind": "test"}),
    )
    .with_event_id(" ");
    let err = event.validate().unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("catalog graph event id must not be blank")
    ));

    event.event_id = Some("lakecat:outbox:evt-valid".to_string());
    event.subject = " ".to_string();
    let err = event.validate().unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("catalog graph event subject must not be blank")
    ));
}

#[test]
fn graph_event_validation_rejects_non_object_properties() {
    let mut event = GraphEvent::server(
        GraphAction::Upserted,
        "prod",
        serde_json::json!({"kind": "test"}),
    );
    event.properties = serde_json::json!("not-an-object");

    let err = event.validate().unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("catalog graph event properties must be a JSON object")
    ));
}

#[test]
fn graph_event_validation_requires_table_identity_for_table_scoped_labels() {
    let event = GraphEvent {
        event_id: Some("lakecat:outbox:evt-tableless-commit".to_string()),
        subject: "lakecat:commit:missing-table:7".to_string(),
        label: GraphNodeLabel::Commit,
        action: GraphAction::Committed,
        table: None,
        properties: serde_json::json!({"kind": "test"}),
        emitted_at: Utc::now(),
    };

    let err = event.validate().unwrap_err();
    assert!(matches!(
        err,
        LakeCatError::InvalidArgument(message)
            if message.contains("catalog graph event label Commit requires table identity")
    ));
}
