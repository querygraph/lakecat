use lakecat_core::{Namespace, PrincipalKind, TableName, WarehouseName};

use super::*;

#[test]
fn projects_table_scan_to_openlineage_input() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let event = LineageEvent::new(
        LineageEventType::TableScanPlanned,
        Principal {
            subject: "agent:reader".to_string(),
            kind: PrincipalKind::Agent,
        },
        Some(table),
        json!({
            "planned-by": "lakecat-sail",
            "storage-location": "file:///tmp/events",
            "read-restriction": {
                "allowed-columns": ["event_id"],
                "row-predicate": {
                    "type": "eq",
                    "term": "event_id",
                    "value": "evt-1"
                }
            },
        }),
    );
    let projected = open_lineage_event(&event);
    assert_eq!(projected["eventType"], json!("COMPLETE"));
    assert_eq!(projected["job"]["name"], json!("table-scan-planned"));
    assert_eq!(projected["inputs"][0]["name"], json!("events"));
    assert_eq!(
        projected["inputs"][0]["facets"]["dataSource"]["uri"],
        json!("file:///tmp/events")
    );
    assert_eq!(
        projected["inputs"][0]["facets"]["lakecat_catalog"]["payload"]["read-restriction"]["allowed-columns"],
        json!(["event_id"])
    );
    assert_eq!(projected["outputs"], json!([]));
}

#[test]
fn projects_table_restore_to_openlineage_output() {
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let event = LineageEvent::new(
        LineageEventType::TableRestored,
        Principal::anonymous(),
        Some(table),
        json!({
            "metadata-location": "file:///tmp/events/metadata/00000.json",
            "version": 0,
        }),
    );
    let projected = open_lineage_event(&event);
    assert_eq!(projected["job"]["name"], json!("table-restored"));
    assert_eq!(projected["inputs"], json!([]));
    assert_eq!(projected["outputs"][0]["name"], json!("events"));
    assert_eq!(
        projected["outputs"][0]["facets"]["dataSource"]["uri"],
        json!("file:///tmp/events/metadata/00000.json")
    );
}

#[test]
fn projects_querygraph_bootstrap_to_openlineage_output() {
    let event = LineageEvent::new(
        LineageEventType::QueryGraphBootstrap,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "table-count": 1,
            "policy-binding-count": 1,
            "bundle-hash": "sha256:bundle",
            "graph-hash": "sha256:graph",
            "open-lineage-hash": "sha256:openlineage",
            "querygraph-import-hash": "sha256:querygraph-import",
            "authorization-receipt": {
                "request-identity": {
                    "attestation-state": "verified",
                    "typedid": "did:example:agent"
                }
            }
        }),
    );
    let projected = open_lineage_event(&event);
    assert_eq!(projected["job"]["name"], json!("querygraph-bootstrap"));
    assert_eq!(projected["inputs"], json!([]));
    assert_eq!(
        projected["outputs"][0]["namespace"],
        json!("lakecat.querygraph")
    );
    assert_eq!(projected["outputs"][0]["name"], json!("bootstrap"));
    assert_eq!(
        projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["tableCount"],
        json!(1)
    );
    assert_eq!(
        projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["bundleHash"],
        json!("sha256:bundle")
    );
    assert_eq!(
        projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["graphHash"],
        json!("sha256:graph")
    );
    assert_eq!(
        projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["openLineageHash"],
        json!("sha256:openlineage")
    );
    assert_eq!(
        projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["queryGraphImportHash"],
        json!("sha256:querygraph-import")
    );
    assert_eq!(
        projected["outputs"][0]["facets"]["queryGraph_bootstrap"]["payload"]["authorization-receipt"]
            ["request-identity"]["attestation-state"],
        json!("verified")
    );
}

#[test]
fn projects_control_plane_upserts_to_openlineage_outputs() {
    let config = open_lineage_event(&LineageEvent::new(
        LineageEventType::CatalogConfigRead,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "authorization-receipt": {
                "principal": Principal::anonymous(),
                "action": "catalog-config",
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now()
            }
        }),
    ));
    assert_eq!(config["job"]["name"], json!("catalog-config-read"));
    assert_eq!(
        config["outputs"][0]["namespace"],
        json!("lakecat.catalog-config")
    );
    assert_eq!(config["outputs"][0]["name"], json!("local"));
    assert_eq!(
        config["outputs"][0]["facets"]["lakecat_catalogConfig"]["payload"]["warehouse"],
        json!("local")
    );

    let namespace_list = open_lineage_event(&LineageEvent::new(
        LineageEventType::NamespaceListed,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "namespace-count": 2
        }),
    ));
    assert_eq!(namespace_list["job"]["name"], json!("namespace-listed"));
    assert_eq!(
        namespace_list["outputs"][0]["namespace"],
        json!("lakecat.namespace-list")
    );
    assert_eq!(namespace_list["outputs"][0]["name"], json!("local"));
    assert_eq!(
        namespace_list["outputs"][0]["facets"]["lakecat_namespace"]["payload"]["namespace-count"],
        json!(2)
    );

    let namespace_load = open_lineage_event(&LineageEvent::new(
        LineageEventType::NamespaceLoaded,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "namespace": ["default"]
        }),
    ));
    assert_eq!(namespace_load["job"]["name"], json!("namespace-loaded"));
    assert_eq!(
        namespace_load["outputs"][0]["namespace"],
        json!("lakecat.namespace")
    );
    assert_eq!(namespace_load["outputs"][0]["name"], json!("default"));
    assert_eq!(
        namespace_load["outputs"][0]["facets"]["lakecat_namespace"]["payload"]["namespace"],
        json!(["default"])
    );

    let policy = open_lineage_event(&LineageEvent::new(
        LineageEventType::PolicyBindingUpserted,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "policy": {
                "policy-id": "agent-read",
                "odrl": {
                    "uid": "policy:agent-read"
                }
            }
        }),
    ));
    assert_eq!(policy["job"]["name"], json!("policy-binding-upserted"));
    assert_eq!(policy["outputs"][0]["namespace"], json!("lakecat.policy"));
    assert_eq!(policy["outputs"][0]["name"], json!("agent-read"));
    assert_eq!(
        policy["outputs"][0]["facets"]["lakecat_policyBinding"]["payload"]["policy"]["odrl"]["uid"],
        json!("policy:agent-read")
    );

    let policy_list = open_lineage_event(&LineageEvent::new(
        LineageEventType::PolicyBindingListed,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "policy-count": 2
        }),
    ));
    assert_eq!(policy_list["job"]["name"], json!("policy-binding-listed"));
    assert_eq!(
        policy_list["outputs"][0]["namespace"],
        json!("lakecat.policy-list")
    );
    assert_eq!(policy_list["outputs"][0]["name"], json!("local"));
    assert_eq!(
        policy_list["outputs"][0]["facets"]["lakecat_policyBinding"]["payload"]["policy-count"],
        json!(2)
    );

    let project = open_lineage_event(&LineageEvent::new(
        LineageEventType::ProjectUpserted,
        Principal::anonymous(),
        None,
        json!({
            "project-id": "default",
            "project-record": {
                "display-name": "Default Project"
            }
        }),
    ));
    assert_eq!(project["job"]["name"], json!("project-upserted"));
    assert_eq!(project["outputs"][0]["namespace"], json!("lakecat.project"));
    assert_eq!(project["outputs"][0]["name"], json!("default"));
    assert_eq!(
        project["outputs"][0]["facets"]["lakecat_project"]["payload"]["project-record"]["display-name"],
        json!("Default Project")
    );

    let project_list = open_lineage_event(&LineageEvent::new(
        LineageEventType::ProjectListed,
        Principal::anonymous(),
        None,
        json!({
            "project-count": 3
        }),
    ));
    assert_eq!(project_list["job"]["name"], json!("project-listed"));
    assert_eq!(
        project_list["outputs"][0]["namespace"],
        json!("lakecat.project-list")
    );
    assert_eq!(project_list["outputs"][0]["name"], json!("projects"));
    assert_eq!(
        project_list["outputs"][0]["facets"]["lakecat_project"]["payload"]["project-count"],
        json!(3)
    );

    let server = open_lineage_event(&LineageEvent::new(
        LineageEventType::ServerUpserted,
        Principal::anonymous(),
        None,
        json!({
            "server-id": "prod",
            "server-record": {
                "display-name": "Production"
            }
        }),
    ));
    assert_eq!(server["job"]["name"], json!("server-upserted"));
    assert_eq!(server["outputs"][0]["namespace"], json!("lakecat.server"));
    assert_eq!(server["outputs"][0]["name"], json!("prod"));
    assert_eq!(
        server["outputs"][0]["facets"]["lakecat_server"]["payload"]["server-record"]["display-name"],
        json!("Production")
    );

    let server_list = open_lineage_event(&LineageEvent::new(
        LineageEventType::ServerListed,
        Principal::anonymous(),
        None,
        json!({
            "server-count": 2
        }),
    ));
    assert_eq!(server_list["job"]["name"], json!("server-listed"));
    assert_eq!(
        server_list["outputs"][0]["namespace"],
        json!("lakecat.server-list")
    );
    assert_eq!(server_list["outputs"][0]["name"], json!("servers"));
    assert_eq!(
        server_list["outputs"][0]["facets"]["lakecat_server"]["payload"]["server-count"],
        json!(2)
    );

    let storage_profile = open_lineage_event(&LineageEvent::new(
        LineageEventType::StorageProfileUpserted,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "storage-profile": {
                "profile-id": "s3-events",
                "location-prefix": "s3://lakecat/events",
                "provider": "s3",
                "issuance-mode": "secret-ref"
            }
        }),
    ));
    assert_eq!(
        storage_profile["job"]["name"],
        json!("storage-profile-upserted")
    );
    assert_eq!(
        storage_profile["outputs"][0]["namespace"],
        json!("lakecat.storage-profile")
    );
    assert_eq!(storage_profile["outputs"][0]["name"], json!("s3-events"));
    assert_eq!(
        storage_profile["outputs"][0]["facets"]["lakecat_storageProfile"]["warehouse"],
        json!("local")
    );
    assert_eq!(
        storage_profile["outputs"][0]["facets"]["lakecat_storageProfile"]["payload"]["storage-profile"]
            ["provider"],
        json!("s3")
    );

    let storage_profile_list = open_lineage_event(&LineageEvent::new(
        LineageEventType::StorageProfileListed,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "storage-profile-count": 2
        }),
    ));
    assert_eq!(
        storage_profile_list["job"]["name"],
        json!("storage-profile-listed")
    );
    assert_eq!(
        storage_profile_list["outputs"][0]["namespace"],
        json!("lakecat.storage-profile-list")
    );
    assert_eq!(storage_profile_list["outputs"][0]["name"], json!("local"));
    assert_eq!(
        storage_profile_list["outputs"][0]["facets"]["lakecat_storageProfile"]["payload"]["storage-profile-count"],
        json!(2)
    );

    let view = open_lineage_event(&LineageEvent::new(
        LineageEventType::ViewUpserted,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "namespace": ["default"],
            "view": {
                "name": "events_view",
                "dialect": "sql",
                "schema-version": 3
            }
        }),
    ));
    assert_eq!(view["job"]["name"], json!("view-upserted"));
    assert_eq!(view["outputs"][0]["namespace"], json!("lakecat.view"));
    assert_eq!(view["outputs"][0]["name"], json!("events_view"));
    assert_eq!(
        view["outputs"][0]["facets"]["lakecat_view"]["warehouse"],
        json!("local")
    );
    assert_eq!(
        view["outputs"][0]["facets"]["lakecat_view"]["namespace"],
        json!(["default"])
    );

    let view_list = open_lineage_event(&LineageEvent::new(
        LineageEventType::ViewListed,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "namespace": ["default"],
            "view-count": 2
        }),
    ));
    assert_eq!(view_list["job"]["name"], json!("view-listed"));
    assert_eq!(
        view_list["outputs"][0]["namespace"],
        json!("lakecat.view-list")
    );
    assert_eq!(view_list["outputs"][0]["name"], json!("default"));
    assert_eq!(
        view_list["outputs"][0]["facets"]["lakecat_view"]["warehouse"],
        json!("local")
    );
    assert_eq!(
        view_list["outputs"][0]["facets"]["lakecat_view"]["namespace"],
        json!(["default"])
    );
    assert_eq!(
        view_list["outputs"][0]["facets"]["lakecat_view"]["payload"]["view-count"],
        json!(2)
    );

    let warehouse = open_lineage_event(&LineageEvent::new(
        LineageEventType::WarehouseUpserted,
        Principal::anonymous(),
        None,
        json!({
            "warehouse": "local",
            "warehouse-record": {
                "storage-root": "file:///tmp/lakecat"
            }
        }),
    ));
    assert_eq!(warehouse["job"]["name"], json!("warehouse-upserted"));
    assert_eq!(
        warehouse["outputs"][0]["namespace"],
        json!("lakecat.warehouse")
    );
    assert_eq!(warehouse["outputs"][0]["name"], json!("local"));
    assert_eq!(
        warehouse["outputs"][0]["facets"]["lakecat_warehouse"]["payload"]["warehouse-record"]["storage-root"],
        json!("file:///tmp/lakecat")
    );

    let warehouse_list = open_lineage_event(&LineageEvent::new(
        LineageEventType::WarehouseListed,
        Principal::anonymous(),
        None,
        json!({
            "project-id": "analytics",
            "warehouse-count": 2
        }),
    ));
    assert_eq!(warehouse_list["job"]["name"], json!("warehouse-listed"));
    assert_eq!(
        warehouse_list["outputs"][0]["namespace"],
        json!("lakecat.warehouse-list")
    );
    assert_eq!(warehouse_list["outputs"][0]["name"], json!("analytics"));
    assert_eq!(
        warehouse_list["outputs"][0]["facets"]["lakecat_warehouse"]["payload"]["warehouse-count"],
        json!(2)
    );
}

#[test]
fn projects_credential_vend_attempt_to_openlineage_run_facet() {
    let event = LineageEvent::new(
        LineageEventType::CredentialsVendAttempted,
        Principal {
            subject: "agent:reader".to_string(),
            kind: PrincipalKind::Agent,
        },
        None,
        json!({
            "credential-count": 0,
            "lakecat:raw-credential-exception": {
                "allowed": false,
                "reason": "fine-grained read restriction requires Sail-planned reads"
            },
        }),
    );
    let projected = open_lineage_event(&event);
    assert_eq!(
        projected["job"]["name"],
        json!("credentials-vend-attempted")
    );
    assert_eq!(projected["inputs"], json!([]));
    assert_eq!(projected["outputs"], json!([]));
    assert_eq!(
        projected["run"]["facets"]["lakecat_catalogEvent"]["payload"]["credential-count"],
        json!(0)
    );
    assert_eq!(
        projected["run"]["facets"]["lakecat_catalogEvent"]["payload"]["lakecat:raw-credential-exception"]
            ["allowed"],
        json!(false)
    );
}

#[tokio::test]
async fn hash_sink_receipts_include_openlineage_hash() {
    let sink = HashOnlyLineageSink;
    let receipt = sink
        .emit(LineageEvent::new(
            LineageEventType::NamespaceCreated,
            Principal::anonymous(),
            None,
            json!({"namespace": ["default"]}),
        ))
        .await
        .unwrap();
    assert_eq!(receipt.sink, "lakecat-openlineage-hash");
    assert!(!receipt.event_hash.is_empty());
    assert!(!receipt.open_lineage_hash.is_empty());
}
