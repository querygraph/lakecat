use super::*;
use lakecat_store::ViewColumnRecord;
use std::collections::BTreeMap;

use lakecat_core::{Namespace, Principal, TableName};

fn is_full_sha256_hash(value: &str) -> bool {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return false;
    };
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn querygraph_test_table(name: &str) -> TableRecord {
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        Namespace::new(vec!["default".to_string()]).unwrap(),
        TableName::new(name).unwrap(),
    );
    TableRecord::new(
        ident,
        format!("file:///tmp/{name}"),
        Some(format!("file:///tmp/{name}/metadata/00000.json")),
        json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [{"id": 1, "name": "event_id", "type": "string"}]
            }]
        }),
        Principal::anonymous(),
    )
}

fn querygraph_test_view(name: &str) -> ViewRecord {
    ViewRecord::new(
        WarehouseName::new("local").unwrap(),
        Namespace::new(vec!["default".to_string()]).unwrap(),
        TableName::new(name).unwrap(),
        "select event_id from events",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap()
}

#[test]
fn projects_iceberg_table_into_querygraph_bundle() {
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        Namespace::new(vec!["default".to_string()]).unwrap(),
        TableName::new("events").unwrap(),
    );
    let table = TableRecord::new(
        ident,
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [{
                    "id": 1,
                    "name": "event_id",
                    "type": "string",
                    "required": true,
                    "doc": "Event identifier.",
                    "semantic-type": "https://schema.org/identifier"
                }]
            }]
        }),
        Principal::anonymous(),
    );

    let bundle =
        QueryGraphBootstrap::from_tables(WarehouseName::new("local").unwrap(), vec![table])
            .unwrap();

    assert_eq!(bundle.tables.len(), 1);
    assert_eq!(bundle.tables[0].format_version, Some(3));
    assert_eq!(
        bundle.manifest.schema_version,
        "lakecat.querygraph.bootstrap.v1"
    );
    assert_eq!(bundle.manifest.table_artifacts.len(), 1);
    assert_eq!(
        bundle.manifest.table_artifacts[0].stable_id,
        bundle.tables[0].stable_id
    );
    assert_eq!(
        bundle.manifest.table_artifacts[0].croissant_hash,
        content_hash_json(&bundle.tables[0].croissant).unwrap()
    );
    assert_eq!(
        bundle.manifest.table_artifacts[0].cdif_hash,
        content_hash_json(&bundle.tables[0].cdif).unwrap()
    );
    assert_eq!(
        bundle.manifest.table_artifacts[0].osi_hash,
        content_hash_json(&bundle.tables[0].osi).unwrap()
    );
    assert_eq!(
        bundle.manifest.table_artifacts[0].odrl_hash,
        content_hash_json(&bundle.tables[0].odrl).unwrap()
    );
    assert_eq!(
        bundle.manifest.table_artifacts[0].policy_bindings_hash,
        content_hash_json(&policy_bindings_value(&bundle.tables[0]).unwrap()).unwrap()
    );
    assert_eq!(
        bundle.manifest.open_lineage_hash,
        content_hash_json(&bundle.open_lineage).unwrap()
    );
    assert_eq!(
        bundle.manifest.graph_hash,
        graph_hash(&bundle.graph).unwrap()
    );
    let import_contract = bundle
        .manifest
        .querygraph_import
        .as_ref()
        .expect("QueryGraph import compatibility contract");
    assert_eq!(
        import_contract.schema_version,
        "lakecat.querygraph.import-compat.v1"
    );
    assert_eq!(import_contract.view_count, 0);
    assert_eq!(import_contract.graph_hash, bundle.manifest.graph_hash);
    assert_eq!(
        import_contract.table_only_bundle_hash,
        table_only_querygraph_import_hash(
            &bundle.warehouse,
            &bundle.manifest,
            &bundle.tables,
            &bundle.graph,
            &bundle.open_lineage
        )
        .unwrap()
    );
    assert!(bundle.manifest.standards.iter().any(|item| item == "CDIF"));
    assert!(
        bundle
            .manifest
            .standards
            .iter()
            .any(|item| item == "Grust catalog graph")
    );
    assert!(
        bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["standards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "CDIF")
    );
    assert_eq!(
        bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["graphHash"],
        bundle.manifest.graph_hash
    );
    assert_eq!(
        bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]["stableId"],
        bundle.manifest.table_artifacts[0].stable_id
    );
    assert_eq!(
        bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]["croissantHash"],
        bundle.manifest.table_artifacts[0].croissant_hash
    );
    assert_eq!(
        bundle.tables[0].cdif["dct:accessRights"]["odrl:policy"]["@type"],
        "odrl:Policy"
    );
    assert!(
        bundle
            .graph
            .edges
            .iter()
            .any(|edge| edge.label == "GOVERNED_BY")
    );
    assert!(
        bundle
            .graph
            .nodes
            .iter()
            .any(|node| node.id == "lakecat:server:default" && node.label == "Server")
    );
    assert!(
        bundle
            .graph
            .nodes
            .iter()
            .any(|node| node.id == "lakecat:project:default" && node.label == "Project")
    );
    assert!(
        bundle
            .graph
            .nodes
            .iter()
            .any(|node| node.id == "lakecat:warehouse:local" && node.label == "Warehouse")
    );
    assert!(bundle.graph.edges.iter().any(|edge| {
        edge.from == "lakecat:catalog"
            && edge.to == "lakecat:server:default"
            && edge.label == "HAS_SERVER"
    }));
    assert!(bundle.graph.edges.iter().any(|edge| {
        edge.from == "lakecat:server:default"
            && edge.to == "lakecat:project:default"
            && edge.label == "HAS_PROJECT"
    }));
    assert!(bundle.graph.edges.iter().any(|edge| {
        edge.from == "lakecat:project:default"
            && edge.to == "lakecat:warehouse:local"
            && edge.label == "HAS_WAREHOUSE"
    }));
    assert!(bundle.graph.edges.iter().any(|edge| {
        edge.from == "lakecat:warehouse:local"
            && edge.to == "lakecat:namespace:local:default"
            && edge.label == "HAS_NAMESPACE"
    }));
    assert_eq!(
        bundle.tables[0].osi["schemaVersion"],
        "lakecat.querygraph.osi-handoff.v1"
    );
    assert_eq!(
        bundle.tables[0].osi["ownership"]["authoritativeSystem"],
        "QueryGraph"
    );
    assert_eq!(
        bundle.tables[0].osi["queryGraphImport"]["semanticModelStatus"],
        "delegated"
    );
    assert!(bundle.tables[0].osi.get("semantic_model").is_none());
    assert_eq!(bundle.open_lineage["eventType"], "COMPLETE");
    let verification = bundle.verify_manifest().unwrap();
    assert_eq!(verification.table_count, 1);
    assert_eq!(verification.bundle_hash, bundle.bundle_hash);
    assert_eq!(verification.graph_hash, bundle.manifest.graph_hash);
    assert_eq!(
        verification.querygraph_import_hash,
        import_contract.table_only_bundle_hash
    );
}

#[test]
fn projects_policy_bindings_into_querygraph_bundle() {
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = Namespace::new(vec!["default".to_string()]).unwrap();
    let table_name = TableName::new("events").unwrap();
    let ident = TableIdent::new(warehouse.clone(), namespace.clone(), table_name.clone());
    let table = TableRecord::new(
        ident,
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [{
                    "id": 1,
                    "name": "event_id",
                    "type": "string",
                    "required": true
                }]
            }]
        }),
        Principal::anonymous(),
    );
    let policy = PolicyBinding::new(
        "agent-read",
        warehouse.clone(),
        Some(namespace),
        Some(table_name),
        true,
        json!({
            "uid": "policy:agent-read",
            "lakecat:read-restriction": {
                "allowed-columns": ["event_id"]
            }
        }),
    )
    .unwrap();

    let bundle = QueryGraphBootstrap::from_tables_with_policy_bindings(
        warehouse,
        vec![(table, vec![policy])],
    )
    .unwrap();

    assert_eq!(bundle.tables[0].policy_bindings.len(), 1);
    assert_eq!(bundle.tables[0].policy_bindings[0].policy_id, "agent-read");
    assert_eq!(
        bundle.tables[0].policy_bindings[0].odrl["lakecat:read-restriction"]["allowed-columns"],
        json!(["event_id"])
    );
    assert_eq!(
        bundle.tables[0].odrl["lakecat:policy-bindings"][0]["odrl"]["lakecat:read-restriction"]["allowed-columns"],
        json!(["event_id"])
    );
    let verification = bundle.verify_manifest().unwrap();
    assert_eq!(verification.table_count, 1);
}

#[test]
fn projects_catalog_views_into_querygraph_bundle() {
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = Namespace::new(vec!["default".to_string()]).unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace,
        TableName::new("active_customers").unwrap(),
        "select id, email from customers where active",
        "sql",
        Some(1),
        BTreeMap::from([("semantic-domain".to_string(), "customer".to_string())]),
        Principal::anonymous(),
    )
    .unwrap()
    .with_columns(vec![
        ViewColumnRecord {
            name: "id".to_string(),
            data_type: json!("int"),
            nullable: false,
            comment: Some("Customer identifier".to_string()),
        },
        ViewColumnRecord {
            name: "email".to_string(),
            data_type: json!("string"),
            nullable: true,
            comment: None,
        },
    ])
    .unwrap();

    let bundle = QueryGraphBootstrap::from_tables_views_with_policy_bindings(
        warehouse,
        Vec::new(),
        vec![view],
    )
    .unwrap()
    .with_view_receipt_evidence(vec![QueryGraphViewReceiptEvidence {
        stable_id: "lakecat:view:local:default:active_customers".to_string(),
        view_version: 1,
        receipt_hash: "sha256:view-version-receipt".to_string(),
        receipt_chain_hash: "sha256:view-receipt-chain".to_string(),
    }])
    .unwrap();

    assert_eq!(bundle.tables.len(), 0);
    assert_eq!(bundle.views.len(), 1);
    assert_eq!(bundle.views[0].name, "active_customers");
    assert_eq!(bundle.views[0].view_version, 1);
    assert_eq!(bundle.views[0].columns[0]["name"], json!("id"));
    assert_eq!(bundle.manifest.view_artifacts.len(), 1);
    assert_eq!(
        bundle.manifest.view_artifacts[0].stable_id,
        bundle.views[0].stable_id
    );
    assert_eq!(
        bundle.manifest.view_artifacts[0].osi_hash,
        content_hash_json(&bundle.views[0].osi).unwrap()
    );
    assert!(
        bundle
            .graph
            .edges
            .iter()
            .any(|edge| edge.label == "CONTAINS_VIEW")
    );
    assert_eq!(
        bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["viewCount"],
        json!(1)
    );
    assert_eq!(
        bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["viewArtifacts"][0]["stableId"],
        bundle.manifest.view_artifacts[0].stable_id
    );
    assert_eq!(
        bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["viewArtifacts"][0]["osiHash"],
        bundle.manifest.view_artifacts[0].osi_hash
    );
    assert_eq!(
        bundle.views[0].osi["view"]["columns"][0]["comment"],
        json!("Customer identifier")
    );
    assert_eq!(bundle.views[0].osi["view"]["viewVersion"], json!(1));
    let graph_view = bundle
        .graph
        .nodes
        .iter()
        .find(|node| node.id == bundle.views[0].stable_id)
        .unwrap();
    assert_eq!(graph_view.properties["viewVersion"], json!(1));
    assert_eq!(
        bundle.open_lineage["outputs"][0]["facets"]["queryGraph_catalogView"]["viewVersion"],
        json!(1)
    );
    let verification = bundle.verify_manifest().unwrap();
    assert_eq!(verification.view_count, 1);
    assert_eq!(verification.verified_views[0], bundle.views[0].stable_id);
    assert_eq!(
        verification
            .verified_view_versions
            .get(&bundle.views[0].stable_id),
        Some(&1)
    );
    assert_eq!(
        verification
            .verified_view_receipt_hashes
            .get(&bundle.views[0].stable_id)
            .map(String::as_str),
        Some("sha256:view-version-receipt")
    );
    assert_eq!(
        verification
            .verified_view_receipt_chain_hashes
            .get(&bundle.views[0].stable_id)
            .map(String::as_str),
        Some("sha256:view-receipt-chain")
    );
    let expected_evidence_hash = view_receipt_evidence_hash(
        &bundle
            .manifest
            .querygraph_import
            .as_ref()
            .unwrap()
            .view_receipt_evidence,
    )
    .unwrap();
    assert_eq!(
        bundle
            .manifest
            .querygraph_import
            .as_ref()
            .unwrap()
            .view_receipt_evidence_hash
            .as_deref(),
        Some(expected_evidence_hash.as_str())
    );
}

#[test]
fn tenant_records_project_full_hash_evidence_without_raw_roots() {
    let warehouse = WarehouseName::new("local").unwrap();
    let server = ServerRecord::new(
        "prod-server",
        Some("Production LakeCat".to_string()),
        Some("https://lakecat.example.com".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let project = ProjectRecord::new(
        "analytics",
        Some("prod-server".to_string()),
        Some("Analytics".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let warehouse_record = WarehouseRecord::new(
        warehouse.clone(),
        "analytics",
        Some("file:///tmp/lakecat-analytics".to_string()),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();
    let tenant = QueryGraphTenantProjection::from_records(
        &warehouse,
        Some(&warehouse_record),
        Some(&project),
        Some(&server),
    );

    assert!(
        tenant
            .server_endpoint_url_hash
            .as_deref()
            .is_some_and(is_full_sha256_hash)
    );
    assert!(
        tenant
            .warehouse_storage_root_hash
            .as_deref()
            .is_some_and(is_full_sha256_hash)
    );

    let graph =
        QueryGraphCatalogGraph::from_tables_and_views_for_warehouse(&warehouse, &[], &[], &tenant);
    let server_node = graph
        .nodes
        .iter()
        .find(|node| node.id == "lakecat:server:prod-server")
        .expect("tenant graph should include durable server node");
    assert_eq!(server_node.label, "Server");
    assert!(
        server_node.properties.get("endpointUrl").is_none()
            || server_node.properties["endpointUrl"].is_null()
    );
    assert!(
        server_node.properties["endpointUrlHash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );

    let warehouse_node = graph
        .nodes
        .iter()
        .find(|node| node.id == "lakecat:warehouse:local")
        .expect("tenant graph should include durable warehouse node");
    assert_eq!(warehouse_node.label, "Warehouse");
    assert!(
        warehouse_node.properties.get("storageRoot").is_none()
            || warehouse_node.properties["storageRoot"].is_null()
    );
    assert!(
        warehouse_node.properties["storageRootHash"]
            .as_str()
            .is_some_and(is_full_sha256_hash)
    );

    let graph_json = serde_json::to_string(&graph).unwrap();
    assert!(!graph_json.contains("https://lakecat.example.com"));
    assert!(!graph_json.contains("file:///tmp/lakecat-analytics"));
}

#[test]
fn querygraph_catalog_graph_deduplicates_shared_namespace_nodes() {
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = Namespace::new(vec!["default".to_string()]).unwrap();
    let table = TableRecord::new(
        TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        ),
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [{
                    "id": 1,
                    "name": "event_id",
                    "type": "string",
                    "required": true
                }]
            }]
        }),
        Principal::anonymous(),
    );
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace,
        TableName::new("active_customers").unwrap(),
        "select id from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();

    let bundle = QueryGraphBootstrap::from_tables_views_with_policy_bindings(
        warehouse,
        vec![(table, Vec::new())],
        vec![view],
    )
    .unwrap()
    .with_view_receipt_evidence(vec![QueryGraphViewReceiptEvidence {
        stable_id: "lakecat:view:local:default:active_customers".to_string(),
        view_version: 1,
        receipt_hash: "sha256:view-version-receipt".to_string(),
        receipt_chain_hash: "sha256:view-receipt-chain".to_string(),
    }])
    .unwrap();

    let namespace_id = "lakecat:namespace:local:default";
    assert_eq!(
        bundle
            .graph
            .nodes
            .iter()
            .filter(|node| node.id == namespace_id)
            .count(),
        1
    );
    assert_eq!(
        bundle
            .graph
            .edges
            .iter()
            .filter(|edge| edge.from == "lakecat:catalog"
                && edge.to == namespace_id
                && edge.label == "HAS_NAMESPACE")
            .count(),
        1
    );
    assert_eq!(
        bundle
            .graph
            .edges
            .iter()
            .filter(|edge| edge.from == "lakecat:warehouse:local"
                && edge.to == namespace_id
                && edge.label == "HAS_NAMESPACE")
            .count(),
        1
    );
    bundle.verify_manifest().unwrap();
}

#[test]
fn verification_rejects_missing_view_receipt_evidence() {
    let warehouse = WarehouseName::new("local").unwrap();
    let namespace = Namespace::new(vec!["default".to_string()]).unwrap();
    let view = ViewRecord::new(
        warehouse.clone(),
        namespace,
        TableName::new("active_customers").unwrap(),
        "select id from customers where active",
        "sql",
        Some(1),
        BTreeMap::new(),
        Principal::anonymous(),
    )
    .unwrap();

    let bundle = QueryGraphBootstrap::from_tables_views_with_policy_bindings(
        warehouse,
        Vec::new(),
        vec![view],
    )
    .unwrap();

    let err = bundle.verify_manifest().unwrap_err();
    assert!(
        err.to_string()
            .contains("view receipt evidence record(s) for 1 view artifact")
    );
}

#[test]
fn verification_rejects_querygraph_bundle_hash_mismatch() {
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        Namespace::new(vec!["default".to_string()]).unwrap(),
        TableName::new("events").unwrap(),
    );
    let table = TableRecord::new(
        ident,
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [{"id": 1, "name": "event_id", "type": "string"}]
            }]
        }),
        Principal::anonymous(),
    );
    let mut bundle =
        QueryGraphBootstrap::from_tables(WarehouseName::new("local").unwrap(), vec![table])
            .unwrap();
    bundle.bundle_hash = "sha256:bad".to_string();

    let err = bundle.verify_manifest().unwrap_err();
    assert!(err.to_string().contains("bundle hash mismatch"));
}

#[test]
fn verification_rejects_duplicate_table_projection_stable_ids() {
    let table = querygraph_test_table("events");
    let bundle = QueryGraphBootstrap::from_tables(
        WarehouseName::new("local").unwrap(),
        vec![table.clone(), table],
    )
    .unwrap();

    let err = bundle.verify_manifest().unwrap_err();
    assert!(
        err.to_string()
            .contains("QueryGraph bootstrap table projections must be duplicate-free")
    );
}

#[test]
fn verification_rejects_duplicate_table_artifact_stable_ids() {
    let mut bundle = QueryGraphBootstrap::from_tables(
        WarehouseName::new("local").unwrap(),
        vec![
            querygraph_test_table("events"),
            querygraph_test_table("orders"),
        ],
    )
    .unwrap();
    bundle.manifest.table_artifacts[1].stable_id =
        bundle.manifest.table_artifacts[0].stable_id.clone();

    let err = bundle.verify_manifest().unwrap_err();
    assert!(
        err.to_string()
            .contains("QueryGraph bootstrap table artifacts must be duplicate-free")
    );
}

#[test]
fn verification_rejects_duplicate_view_projection_stable_ids() {
    let view = querygraph_test_view("active_events");
    let bundle = QueryGraphBootstrap::from_tables_views_with_policy_bindings(
        WarehouseName::new("local").unwrap(),
        Vec::new(),
        vec![view.clone(), view],
    )
    .unwrap();

    let err = bundle.verify_manifest().unwrap_err();
    assert!(
        err.to_string()
            .contains("QueryGraph bootstrap view projections must be duplicate-free")
    );
}

#[test]
fn verification_rejects_duplicate_view_artifact_stable_ids() {
    let mut bundle = QueryGraphBootstrap::from_tables_views_with_policy_bindings(
        WarehouseName::new("local").unwrap(),
        Vec::new(),
        vec![
            querygraph_test_view("active_events"),
            querygraph_test_view("recent_events"),
        ],
    )
    .unwrap();
    bundle.manifest.view_artifacts[1].stable_id =
        bundle.manifest.view_artifacts[0].stable_id.clone();

    let err = bundle.verify_manifest().unwrap_err();
    assert!(
        err.to_string()
            .contains("QueryGraph bootstrap view artifacts must be duplicate-free")
    );
}

#[test]
fn verification_rejects_querygraph_graph_hash_mismatch() {
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        Namespace::new(vec!["default".to_string()]).unwrap(),
        TableName::new("events").unwrap(),
    );
    let table = TableRecord::new(
        ident,
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [{"id": 1, "name": "event_id", "type": "string"}]
            }]
        }),
        Principal::anonymous(),
    );
    let mut bundle =
        QueryGraphBootstrap::from_tables(WarehouseName::new("local").unwrap(), vec![table])
            .unwrap();
    bundle.graph.nodes.clear();

    let err = bundle.verify_manifest().unwrap_err();
    assert!(err.to_string().contains("graph hash mismatch"));
}

#[test]
fn verification_rejects_querygraph_import_hash_mismatch() {
    let ident = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        Namespace::new(vec!["default".to_string()]).unwrap(),
        TableName::new("events").unwrap(),
    );
    let table = TableRecord::new(
        ident,
        "file:///tmp/events".to_string(),
        Some("file:///tmp/events/metadata/00000.json".to_string()),
        json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "fields": [{"id": 1, "name": "event_id", "type": "string"}]
            }]
        }),
        Principal::anonymous(),
    );
    let mut bundle =
        QueryGraphBootstrap::from_tables(WarehouseName::new("local").unwrap(), vec![table])
            .unwrap();
    bundle
        .manifest
        .querygraph_import
        .as_mut()
        .unwrap()
        .table_only_bundle_hash = "sha256:bad".to_string();

    let err = bundle.verify_manifest().unwrap_err();
    assert!(err.to_string().contains("import hash mismatch"));
}
