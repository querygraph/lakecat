use crate::*;

pub(crate) fn verify_qglake_bootstrap_bundle(
    bundle: &QueryGraphBootstrap,
    namespace: &[String],
    table: &str,
) -> lakecat_core::LakeCatResult<()> {
    let projection = bundle
        .tables
        .iter()
        .find(|candidate| {
            candidate.ident.namespace.parts() == namespace && candidate.ident.name.as_str() == table
        })
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap did not include table {}.{}",
                namespace.join("."),
                table
            ))
        })?;
    verify_qglake_bootstrap_standards(bundle)?;
    verify_qglake_bootstrap_projection(projection, namespace, table)?;
    verify_qglake_bootstrap_graph(bundle, projection)?;
    verify_qglake_bootstrap_open_lineage(bundle, projection)?;
    bundle.verify_manifest()?;
    verify_qglake_querygraph_import_contract(bundle)?;
    Ok(())
}

pub(crate) const QGLAKE_BOOTSTRAP_STANDARDS: &[&str] = &[
    "Iceberg REST",
    "Croissant",
    "CDIF",
    "OSI handoff",
    "ODRL",
    "Grust catalog graph",
    "OpenLineage",
];

pub(crate) fn verify_qglake_bootstrap_standards(
    bundle: &QueryGraphBootstrap,
) -> lakecat_core::LakeCatResult<()> {
    for expected in QGLAKE_BOOTSTRAP_STANDARDS {
        if !bundle
            .manifest
            .standards
            .iter()
            .any(|standard| standard == *expected)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap manifest did not advertise required standard {expected}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_querygraph_import_contract(
    bundle: &QueryGraphBootstrap,
) -> lakecat_core::LakeCatResult<()> {
    let import_contract = bundle.manifest.querygraph_import.as_ref().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "QGLake bootstrap manifest did not include QueryGraph import compatibility evidence"
                .to_string(),
        )
    })?;
    if import_contract.schema_version != "lakecat.querygraph.import-compat.v1" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap QueryGraph import contract used unsupported schema {}",
            import_contract.schema_version
        )));
    }
    if import_contract.table_only_bundle_hash.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "QGLake bootstrap QueryGraph import contract did not include table-only bundle hash"
                .to_string(),
        ));
    }
    if import_contract.graph_hash != bundle.manifest.graph_hash {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap QueryGraph import contract graph hash did not match manifest: {}",
            import_contract.graph_hash
        )));
    }
    if import_contract.view_count != bundle.views.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap QueryGraph import contract view count {} did not match bundle views {}",
            import_contract.view_count,
            bundle.views.len()
        )));
    }
    if bundle.views.is_empty() {
        if !import_contract.view_receipt_evidence.is_empty()
            || import_contract.view_receipt_evidence_hash.is_some()
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap QueryGraph import contract carried view receipt evidence without views"
                    .to_string(),
            ));
        }
    } else {
        if import_contract.view_receipt_evidence.len() != bundle.views.len() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap QueryGraph import contract listed {} view receipt evidence record(s) for {} view(s)",
                import_contract.view_receipt_evidence.len(),
                bundle.views.len()
            )));
        }
        if import_contract
            .view_receipt_evidence_hash
            .as_deref()
            .map_or(true, str::is_empty)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap QueryGraph import contract did not include a view receipt evidence hash"
                    .to_string(),
            ));
        }
        for view in &bundle.views {
            let Some(evidence) = import_contract
                .view_receipt_evidence
                .iter()
                .find(|evidence| evidence.stable_id == view.stable_id)
            else {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QGLake bootstrap QueryGraph import contract is missing receipt evidence for {}",
                    view.stable_id
                )));
            };
            if evidence.view_version != view.view_version
                || evidence.receipt_hash.is_empty()
                || evidence.receipt_chain_hash.is_empty()
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QGLake bootstrap QueryGraph import contract receipt evidence for {} did not match the accepted view version and receipt chain",
                    view.stable_id
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_bootstrap_projection(
    projection: &lakecat_querygraph::QueryGraphTableProjection,
    namespace: &[String],
    table: &str,
) -> lakecat_core::LakeCatResult<()> {
    let expected_policy = format!("{table}-agent-read");
    let binding = projection
        .policy_bindings
        .iter()
        .find(|binding| binding.policy_id == expected_policy)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap table {} did not include policy binding {expected_policy}",
                projection.stable_id
            ))
        })?;
    if !binding.enforced {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy binding {expected_policy} is not enforced"
        )));
    }
    if binding.namespace.as_deref() != Some(namespace) || binding.table.as_deref() != Some(table) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy binding {expected_policy} is not scoped to {}.{table}",
            namespace.join(".")
        )));
    }
    let restriction = &binding.odrl["lakecat:read-restriction"];
    if restriction["allowed-columns"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy allowed columns were not exported as expected: {}",
            restriction["allowed-columns"].clone()
        )));
    }
    if restriction["row-predicate"]
        != json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        })
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy row predicate was not exported as expected: {}",
            restriction["row-predicate"].clone()
        )));
    }
    if restriction["purpose"] != json!("qglake-agent-demo") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy purpose was not exported as expected: {}",
            restriction["purpose"].clone()
        )));
    }
    if restriction["max-credential-ttl-seconds"] != json!(300) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy max credential TTL was not exported as expected: {}",
            restriction["max-credential-ttl-seconds"].clone()
        )));
    }
    let embedded_bindings = projection.odrl["lakecat:policy-bindings"]
        .as_array()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap ODRL table projection did not embed policy bindings for {expected_policy}"
            ))
        })?;
    let embedded_binding = embedded_bindings
        .iter()
        .find(|embedded| embedded["policy-id"] == expected_policy)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap ODRL table projection did not embed {expected_policy}"
            ))
        })?;
    if embedded_binding["odrl"] != binding.odrl {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap embedded ODRL policy binding {expected_policy} drifted from structured policy binding"
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_bootstrap_open_lineage(
    bundle: &QueryGraphBootstrap,
    projection: &lakecat_querygraph::QueryGraphTableProjection,
) -> lakecat_core::LakeCatResult<()> {
    if bundle.open_lineage["eventType"] != json!("COMPLETE") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage eventType was not COMPLETE: {}",
            bundle.open_lineage["eventType"].clone()
        )));
    }
    if bundle.open_lineage["producer"] != json!("https://querygraph.ai/lakecat") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage producer was not LakeCat: {}",
            bundle.open_lineage["producer"].clone()
        )));
    }
    if bundle.open_lineage["schemaURL"]
        != json!("https://openlineage.io/spec/2-0-2/OpenLineage.json")
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage schemaURL was not the expected OpenLineage schema: {}",
            bundle.open_lineage["schemaURL"].clone()
        )));
    }
    if bundle.open_lineage["job"]["name"] != json!("querygraph-bootstrap") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage job name was not querygraph-bootstrap: {}",
            bundle.open_lineage["job"]["name"].clone()
        )));
    }
    let expected_job_namespace = format!("lakecat.{}", bundle.warehouse.as_str());
    if bundle.open_lineage["job"]["namespace"] != json!(expected_job_namespace) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage job namespace did not match warehouse: {}",
            bundle.open_lineage["job"]["namespace"].clone()
        )));
    }
    let semantic_bundle = bundle
        .open_lineage
        .pointer("/run/facets/queryGraph_semanticBundle")
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap OpenLineage did not include queryGraph_semanticBundle facet"
                    .to_string(),
            )
        })?;
    if semantic_bundle["tableCount"] != json!(bundle.tables.len()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage tableCount did not match bundle tables: {}",
            semantic_bundle["tableCount"].clone()
        )));
    }
    if semantic_bundle["viewCount"] != json!(bundle.views.len()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage viewCount did not match bundle views: {}",
            semantic_bundle["viewCount"].clone()
        )));
    }
    for expected in QGLAKE_BOOTSTRAP_STANDARDS {
        let standards = semantic_bundle["standards"].as_array().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap OpenLineage semantic bundle did not list standards".to_string(),
            )
        })?;
        if !standards
            .iter()
            .any(|standard| standard.as_str() == Some(*expected))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap OpenLineage semantic bundle did not advertise required standard {expected}"
            )));
        }
    }
    if semantic_bundle["graphHash"] != json!(bundle.manifest.graph_hash) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage graph hash did not match manifest: {}",
            semantic_bundle["graphHash"].clone()
        )));
    }
    verify_qglake_open_lineage_artifacts(semantic_bundle, &bundle.manifest)?;
    let output = bundle
        .open_lineage
        .get("outputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|output| {
            output["name"] == projection.ident.name.as_str()
                && output["facets"]["queryGraph_catalog"]["stableId"] == projection.stable_id
        })
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap OpenLineage output did not include {}",
                projection.stable_id
            ))
        })?;
    if output["facets"]["queryGraph_catalog"]["metadataLocation"]
        != json!(projection.metadata_location)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage metadata location did not match table projection: {}",
            output["facets"]["queryGraph_catalog"]["metadataLocation"].clone()
        )));
    }
    if output["facets"]["dataSource"]["uri"] != json!(projection.location) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage data-source URI did not match table location: {}",
            output["facets"]["dataSource"]["uri"].clone()
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_open_lineage_artifacts(
    semantic_bundle: &Value,
    manifest: &lakecat_querygraph::QueryGraphBundleManifest,
) -> lakecat_core::LakeCatResult<()> {
    let table_artifacts = semantic_bundle["tableArtifacts"]
        .as_array()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap OpenLineage semantic bundle did not list table artifacts"
                    .to_string(),
            )
        })?;
    for manifest_artifact in &manifest.table_artifacts {
        let lineage_artifact = table_artifacts
            .iter()
            .find(|artifact| artifact["stableId"] == manifest_artifact.stable_id)
            .ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QGLake bootstrap OpenLineage semantic bundle did not include table artifact {}",
                    manifest_artifact.stable_id
                ))
            })?;
        verify_qglake_open_lineage_table_artifact(lineage_artifact, manifest_artifact)?;
    }

    let view_artifacts = semantic_bundle["viewArtifacts"].as_array().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "QGLake bootstrap OpenLineage semantic bundle did not list view artifacts".to_string(),
        )
    })?;
    for manifest_artifact in &manifest.view_artifacts {
        let lineage_artifact = view_artifacts
            .iter()
            .find(|artifact| artifact["stableId"] == manifest_artifact.stable_id)
            .ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QGLake bootstrap OpenLineage semantic bundle did not include view artifact {}",
                    manifest_artifact.stable_id
                ))
            })?;
        verify_qglake_open_lineage_view_artifact(lineage_artifact, manifest_artifact)?;
    }

    Ok(())
}

pub(crate) fn verify_qglake_open_lineage_table_artifact(
    lineage_artifact: &Value,
    manifest_artifact: &lakecat_querygraph::QueryGraphTableArtifactHashes,
) -> lakecat_core::LakeCatResult<()> {
    for (field, expected) in [
        ("croissantHash", manifest_artifact.croissant_hash.as_str()),
        ("cdifHash", manifest_artifact.cdif_hash.as_str()),
        ("osiHash", manifest_artifact.osi_hash.as_str()),
        ("odrlHash", manifest_artifact.odrl_hash.as_str()),
        (
            "policyBindingsHash",
            manifest_artifact.policy_bindings_hash.as_str(),
        ),
    ] {
        if lineage_artifact[field] != json!(expected) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap OpenLineage table artifact {field} did not match manifest: {}",
                lineage_artifact[field].clone()
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_open_lineage_view_artifact(
    lineage_artifact: &Value,
    manifest_artifact: &lakecat_querygraph::QueryGraphViewArtifactHashes,
) -> lakecat_core::LakeCatResult<()> {
    if lineage_artifact["osiHash"] != json!(manifest_artifact.osi_hash) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage view artifact osiHash did not match manifest: {}",
            lineage_artifact["osiHash"].clone()
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_bootstrap_graph(
    bundle: &QueryGraphBootstrap,
    projection: &lakecat_querygraph::QueryGraphTableProjection,
) -> lakecat_core::LakeCatResult<()> {
    let namespace_id = format!(
        "lakecat:namespace:{}:{}",
        projection.ident.warehouse, projection.ident.namespace
    );
    let warehouse_id = format!("lakecat:warehouse:{}", projection.ident.warehouse);
    let table_node = bundle
        .graph
        .nodes
        .iter()
        .find(|node| node.id == projection.stable_id && node.label == "IcebergTable")
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap graph did not include table node {}",
                projection.stable_id
            ))
        })?;
    if table_node.properties["metadataLocation"] != json!(projection.metadata_location) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph metadata location did not match table projection: {}",
            table_node.properties["metadataLocation"].clone()
        )));
    }

    let server_id = bundle
        .graph
        .edges
        .iter()
        .find(|edge| edge.from == "lakecat:catalog" && edge.label == "HAS_SERVER")
        .map(|edge| edge.to.as_str())
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap graph did not connect Catalog to a Server tenant anchor"
                    .to_string(),
            )
        })?;
    let server_node = require_qglake_graph_node_label(bundle, server_id, "Server")?;
    verify_qglake_tenant_root_redaction(server_node, "endpointUrl", "endpointUrlHash", "Server")?;

    let project_id = bundle
        .graph
        .edges
        .iter()
        .find(|edge| edge.from == server_id && edge.label == "HAS_PROJECT")
        .map(|edge| edge.to.as_str())
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap graph did not connect Server {server_id} to a Project tenant anchor"
            ))
        })?;
    require_qglake_graph_node_label(bundle, project_id, "Project")?;

    if !qglake_graph_has_edge(bundle, project_id, &warehouse_id, "HAS_WAREHOUSE") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph did not connect Project {project_id} to warehouse {}",
            projection.ident.warehouse
        )));
    }
    let warehouse_node = require_qglake_graph_node_label(bundle, &warehouse_id, "Warehouse")?;
    verify_qglake_tenant_root_redaction(
        warehouse_node,
        "storageRoot",
        "storageRootHash",
        "Warehouse",
    )?;
    require_qglake_graph_node_label(bundle, &namespace_id, "Namespace")?;

    if !qglake_graph_has_edge(bundle, &warehouse_id, &namespace_id, "HAS_NAMESPACE") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph did not connect warehouse {} to namespace {}",
            projection.ident.warehouse, projection.ident.namespace
        )));
    }

    let has_namespace_edge = bundle
        .graph
        .edges
        .iter()
        .any(|edge| edge.to == projection.stable_id && edge.label == "CONTAINS_TABLE");
    if !has_namespace_edge {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph did not connect namespace to table {}",
            projection.stable_id
        )));
    }
    Ok(())
}

pub(crate) fn require_qglake_graph_node_label<'a>(
    bundle: &'a QueryGraphBootstrap,
    node_id: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a lakecat_querygraph::QueryGraphNode> {
    bundle
        .graph
        .nodes
        .iter()
        .find(|node| node.id == node_id && node.label == label)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap graph did not include {label} node {node_id}"
            ))
        })
}

pub(crate) fn verify_qglake_tenant_root_redaction(
    node: &lakecat_querygraph::QueryGraphNode,
    raw_field: &str,
    hash_field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if !node
        .properties
        .get(raw_field)
        .unwrap_or(&Value::Null)
        .is_null()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph {label} node must not expose raw {raw_field}; use {hash_field}"
        )));
    }
    if let Some(hash) = node.properties.get(hash_field)
        && !hash.is_null()
        && !hash.as_str().is_some_and(is_full_sha256_hash)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph {label} node {hash_field} must be full SHA-256 hash evidence"
        )));
    }
    Ok(())
}

pub(crate) fn qglake_graph_has_edge(
    bundle: &QueryGraphBootstrap,
    from: &str,
    to: &str,
    label: &str,
) -> bool {
    bundle
        .graph
        .edges
        .iter()
        .any(|edge| edge.from == from && edge.to == to && edge.label == label)
}
