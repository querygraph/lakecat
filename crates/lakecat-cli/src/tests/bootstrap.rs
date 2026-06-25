use super::common::*;
use crate::*;

#[test]
fn qglake_bootstrap_projection_verifier_accepts_exported_policy_binding() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));

    verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
        .expect("QGLake bootstrap projection should include exported policy binding");
}

#[test]
fn qglake_bootstrap_projection_verifier_rejects_missing_policy_binding() {
    let mut projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    projection.policy_bindings.clear();

    let err = verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
        .expect_err("missing QGLake policy binding should be rejected");
    assert!(err.to_string().contains("events-agent-read"));
}

#[test]
fn qglake_bootstrap_projection_verifier_requires_policy_purpose() {
    let mut policy = qglake_odrl_policy("events");
    policy["lakecat:read-restriction"]
        .as_object_mut()
        .unwrap()
        .remove("purpose");
    let projection = qglake_querygraph_projection(policy);

    let err = verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap projection should require policy purpose");

    assert!(err.to_string().contains("policy purpose"));
}

#[test]
fn qglake_bootstrap_projection_verifier_requires_policy_ttl_cap() {
    let mut policy = qglake_odrl_policy("events");
    policy["lakecat:read-restriction"]
        .as_object_mut()
        .unwrap()
        .remove("max-credential-ttl-seconds");
    let projection = qglake_querygraph_projection(policy);

    let err = verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap projection should require policy TTL cap");

    assert!(err.to_string().contains("policy max credential TTL"));
}

#[test]
fn qglake_bootstrap_projection_verifier_rejects_embedded_odrl_drift() {
    let mut projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    projection.odrl["lakecat:policy-bindings"][0]["odrl"]["lakecat:read-restriction"]["max-credential-ttl-seconds"] =
        serde_json::json!(60);

    let err = verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap projection should reject embedded ODRL drift");

    assert!(err.to_string().contains("embedded ODRL policy binding"));
}

#[test]
fn qglake_bootstrap_verifier_requires_openlineage_output() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let bundle = qglake_querygraph_bundle(vec![projection], Vec::new());

    let err =
        verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events").unwrap_err();
    assert!(err.to_string().contains("OpenLineage output"));
}

#[test]
fn qglake_bootstrap_verifier_requires_openlineage_semantic_standards() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["standards"]
        .as_array_mut()
        .unwrap()
        .retain(|standard| standard.as_str() != Some("OpenLineage"));

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject missing OpenLineage standards facet");
    assert!(
        err.to_string().contains(
            "OpenLineage semantic bundle did not advertise required standard OpenLineage"
        )
    );
}

#[test]
fn qglake_bootstrap_verifier_requires_openlineage_artifact_hashes() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]["croissantHash"] =
        json!("sha256:wrong");

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject mismatched OpenLineage artifact hashes");
    assert!(
        err.to_string()
            .contains("table artifact croissantHash did not match manifest")
    );
}

#[test]
fn qglake_bootstrap_verifier_checks_every_openlineage_table_artifact() {
    let events = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let alerts = qglake_querygraph_projection_for("alerts", qglake_odrl_policy("alerts"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": events.stable_id.clone(),
                "metadataLocation": events.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![events, alerts], vec![output]);
    bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][1]["cdifHash"] =
        json!("sha256:wrong");

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject any mismatched table artifact hash");
    assert!(
        err.to_string()
            .contains("table artifact cdifHash did not match manifest")
    );
}

#[test]
fn qglake_bootstrap_verifier_requires_openlineage_envelope() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle.open_lineage["producer"] = json!("https://example.invalid/catalog");

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject non-LakeCat OpenLineage producer");
    assert!(err.to_string().contains("producer was not LakeCat"));
}

#[test]
fn qglake_bootstrap_verifier_requires_openlineage_datasource_uri() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "dataSource": {
                "uri": "file:///tmp/lakecat-qglake/wrong"
            },
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let bundle = qglake_querygraph_bundle(vec![projection], vec![output]);

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject mismatched OpenLineage data-source URI");
    assert!(err.to_string().contains("data-source URI"));
}

#[test]
fn qglake_bootstrap_verifier_requires_graph_table_anchor() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle.graph.nodes.clear();

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject missing graph table anchor");
    assert!(err.to_string().contains("graph did not include table node"));
}

#[test]
fn qglake_bootstrap_verifier_requires_graph_tenant_spine() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle.graph.edges.retain(|edge| edge.label != "HAS_SERVER");

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject a missing tenant spine");
    assert!(err.to_string().contains("Catalog to a Server"));
}

#[test]
fn qglake_bootstrap_verifier_rejects_raw_server_endpoint_url() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    let server = bundle
        .graph
        .nodes
        .iter_mut()
        .find(|node| node.label == "Server")
        .expect("fixture should include Server node");
    server.properties["endpointUrl"] = json!("https://lakecat.example.com?token=raw");
    server.properties["endpointUrlHash"] =
        json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    qglake_resync_bundle_hashes(&mut bundle);

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject raw tenant server endpoint URLs");
    assert!(err.to_string().contains("raw endpointUrl"));
}

#[test]
fn qglake_bootstrap_verifier_rejects_short_server_endpoint_hash() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    let server = bundle
        .graph
        .nodes
        .iter_mut()
        .find(|node| node.label == "Server")
        .expect("fixture should include Server node");
    server.properties["endpointUrlHash"] = json!("sha256:endpoint-url");
    qglake_resync_bundle_hashes(&mut bundle);

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject short tenant endpoint hash evidence");
    assert!(err.to_string().contains("endpointUrlHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_bootstrap_verifier_rejects_raw_warehouse_storage_root() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    let warehouse = bundle
        .graph
        .nodes
        .iter_mut()
        .find(|node| node.label == "Warehouse")
        .expect("fixture should include Warehouse node");
    warehouse.properties["storageRoot"] = json!("file:///tmp/lakecat?token=raw");
    warehouse.properties["storageRootHash"] =
        json!("sha256:1111111111111111111111111111111111111111111111111111111111111111");
    qglake_resync_bundle_hashes(&mut bundle);

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject raw tenant warehouse storage roots");
    assert!(err.to_string().contains("raw storageRoot"));
}

#[test]
fn qglake_bootstrap_verifier_rejects_short_warehouse_storage_root_hash() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    let warehouse = bundle
        .graph
        .nodes
        .iter_mut()
        .find(|node| node.label == "Warehouse")
        .expect("fixture should include Warehouse node");
    warehouse.properties["storageRootHash"] = json!("sha256:storage-root");
    qglake_resync_bundle_hashes(&mut bundle);

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject short tenant storage-root hash evidence");
    assert!(err.to_string().contains("storageRootHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_bootstrap_verifier_requires_graph_warehouse_namespace_edge() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle
        .graph
        .edges
        .retain(|edge| !(edge.from == "lakecat:warehouse:local" && edge.label == "HAS_NAMESPACE"));

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject a detached warehouse namespace");
    assert!(err.to_string().contains("warehouse local to namespace"));
}

#[test]
fn qglake_bootstrap_verifier_requires_manifest_standards() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle
        .manifest
        .standards
        .retain(|standard| standard != "CDIF");

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject missing QueryGraph standards");
    assert!(err.to_string().contains("required standard CDIF"));
}

#[test]
fn qglake_bootstrap_verifier_requires_querygraph_import_contract() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle.manifest.querygraph_import = None;

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject missing QueryGraph import contract");
    assert!(err.to_string().contains("querygraph-import"));
}

#[test]
fn qglake_bootstrap_verifier_requires_manifest_hash_integrity() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let mut bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    bundle.tables[0].croissant["tampered"] = json!(true);

    let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect_err("QGLake bootstrap should reject tampered artifact content");
    assert!(err.to_string().contains("Croissant hash mismatch"));
}

#[test]
fn qglake_bootstrap_verifier_accepts_policy_and_openlineage_export() {
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let bundle = qglake_querygraph_bundle(vec![projection], vec![output]);

    verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
        .expect("QGLake bootstrap should include policy binding and OpenLineage output");
}
