use chrono::{DateTime, Utc};
use lakecat_core::{LakeCatResult, WarehouseName, content_hash_json};
use lakecat_store::{
    PolicyBinding, ProjectRecord, ServerRecord, TableRecord, ViewRecord, WarehouseRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub use qglake_bundle::*;

pub fn bootstrap_from_tables(
    warehouse: WarehouseName,
    tables: impl IntoIterator<Item = TableRecord>,
) -> LakeCatResult<QueryGraphBootstrap> {
    bootstrap_from_tables_with_policy_bindings(
        warehouse,
        tables.into_iter().map(|table| (table, Vec::new())),
    )
}

pub fn bootstrap_from_tables_with_policy_bindings(
    warehouse: WarehouseName,
    tables: impl IntoIterator<Item = (TableRecord, Vec<PolicyBinding>)>,
) -> LakeCatResult<QueryGraphBootstrap> {
    bootstrap_from_tables_views_with_policy_bindings(warehouse, tables, Vec::new())
}

pub fn bootstrap_from_tables_views_with_policy_bindings(
    warehouse: WarehouseName,
    tables: impl IntoIterator<Item = (TableRecord, Vec<PolicyBinding>)>,
    views: impl IntoIterator<Item = ViewRecord>,
) -> LakeCatResult<QueryGraphBootstrap> {
    bootstrap_from_tables_views_with_policy_bindings_and_tenant(
        warehouse,
        tables,
        views,
        QueryGraphTenantProjection::default(),
    )
}

pub fn bootstrap_from_tables_views_with_policy_bindings_and_tenant(
    warehouse: WarehouseName,
    tables: impl IntoIterator<Item = (TableRecord, Vec<PolicyBinding>)>,
    views: impl IntoIterator<Item = ViewRecord>,
    tenant: QueryGraphTenantProjection,
) -> LakeCatResult<QueryGraphBootstrap> {
    let generated_at = Utc::now();
    let tables = tables
        .into_iter()
        .map(|(table, policy_bindings)| {
            table_projection_from_table_with_policies(table, policy_bindings)
        })
        .collect::<Vec<_>>();
    let views = views
        .into_iter()
        .map(view_projection_from_view)
        .collect::<Vec<_>>();
    let graph =
        catalog_graph_from_tables_and_views_for_warehouse(&warehouse, &tables, &views, &tenant);
    let table_artifacts = tables
        .iter()
        .map(QueryGraphTableArtifactHashes::from_table)
        .collect::<LakeCatResult<Vec<_>>>()?;
    let view_artifacts = views
        .iter()
        .map(QueryGraphViewArtifactHashes::from_view)
        .collect::<LakeCatResult<Vec<_>>>()?;
    let graph_hash = graph_hash(&graph)?;
    let open_lineage = bootstrap_open_lineage(
        &warehouse,
        &tables,
        &views,
        &table_artifacts,
        &view_artifacts,
        &graph_hash,
        generated_at,
    );
    let mut manifest = QueryGraphBundleManifest::from_hashes(
        table_artifacts,
        view_artifacts,
        graph_hash,
        &open_lineage,
    )?;
    manifest.querygraph_import = Some(QueryGraphImportCompatibility::from_table_only_bundle(
        &warehouse,
        &manifest,
        &tables,
        &graph,
        &open_lineage,
        views.len(),
    )?);
    let bundle_payload = json!({
        "warehouse": warehouse.as_str(),
        "manifest": manifest,
        "tables": tables,
        "views": views,
        "graph": graph,
        "openLineage": open_lineage,
    });
    let bundle_hash = content_hash_json(&bundle_payload)?;
    let tables = serde_json::from_value(bundle_payload["tables"].clone()).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to rebuild QueryGraph table projections: {err}"
        ))
    })?;
    let graph = serde_json::from_value(bundle_payload["graph"].clone()).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to rebuild QueryGraph catalog graph: {err}"
        ))
    })?;
    let views = serde_json::from_value(bundle_payload["views"].clone()).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to rebuild QueryGraph view projections: {err}"
        ))
    })?;
    Ok(QueryGraphBootstrap {
        warehouse,
        generated_at,
        bundle_hash,
        manifest,
        tables,
        views,
        graph,
        open_lineage,
    })
}

pub fn tenant_projection_from_records(
    warehouse: &WarehouseName,
    warehouse_record: Option<&WarehouseRecord>,
    project_record: Option<&ProjectRecord>,
    server_record: Option<&ServerRecord>,
) -> QueryGraphTenantProjection {
    let project_id = warehouse_record
        .map(|record| record.project_id.clone())
        .or_else(|| project_record.map(|record| record.project_id.clone()))
        .unwrap_or_else(|| "default".to_string());
    let server_id = project_record
        .and_then(|record| record.server_id.clone())
        .or_else(|| server_record.map(|record| record.server_id.clone()))
        .unwrap_or_else(|| "default".to_string());
    QueryGraphTenantProjection {
        server_id,
        server_display_name: server_record.and_then(|record| record.display_name.clone()),
        server_endpoint_url_hash: server_record
            .and_then(|record| record.endpoint_url.as_deref())
            .map(server_endpoint_url_hash),
        project_id,
        project_display_name: project_record.and_then(|record| record.display_name.clone()),
        warehouse: Some(
            warehouse_record
                .map(|record| record.warehouse.as_str().to_string())
                .unwrap_or_else(|| warehouse.as_str().to_string()),
        ),
        warehouse_project_id: warehouse_record.map(|record| record.project_id.clone()),
        warehouse_storage_root_hash: warehouse_record
            .and_then(|record| record.storage_root.as_deref())
            .map(warehouse_storage_root_hash),
        source: if warehouse_record.is_some() || project_record.is_some() || server_record.is_some()
        {
            "lakecat-management-records".to_string()
        } else {
            "lakecat-querygraph-bootstrap".to_string()
        },
    }
}

pub fn view_projection_from_view(view: ViewRecord) -> QueryGraphViewProjection {
    let stable_id = view_stable_id(&view);
    let columns = json!(view.columns);
    let properties = json!(view.properties);
    let osi = view_osi_handoff(&view, &stable_id);
    QueryGraphViewProjection {
        stable_id,
        warehouse: view.warehouse.as_str().to_string(),
        namespace: view.namespace.parts().to_vec(),
        name: view.name.as_str().to_string(),
        view_version: view.view_version,
        sql: view.sql,
        dialect: view.dialect,
        schema_version: view.schema_version,
        columns,
        properties,
        osi,
    }
}

pub fn table_projection_from_table(table: TableRecord) -> QueryGraphTableProjection {
    table_projection_from_table_with_policies(table, Vec::new())
}

pub fn table_projection_from_table_with_policies(
    table: TableRecord,
    policy_bindings: Vec<PolicyBinding>,
) -> QueryGraphTableProjection {
    let stable_id = table.ident.stable_id();
    let fields = iceberg_fields(&table.metadata);
    let policy_bindings = policy_bindings
        .into_iter()
        .map(policy_binding_projection_from_binding)
        .collect::<Vec<_>>();
    let odrl = odrl_policy(&stable_id, &policy_bindings);
    let croissant = croissant_dataset(&table, &stable_id, &fields);
    let cdif = cdif_resource(&table, &stable_id, &fields, odrl.clone());
    let osi = osi_handoff(&table, &stable_id, &fields);
    QueryGraphTableProjection {
        ident: table.ident,
        stable_id,
        location: table.location,
        metadata_location: table.metadata_location,
        version: table.version,
        format_version: table.metadata.get("format-version").and_then(Value::as_i64),
        croissant,
        cdif,
        osi,
        odrl,
        policy_bindings,
    }
}

pub fn policy_binding_projection_from_binding(
    binding: PolicyBinding,
) -> QueryGraphPolicyBindingProjection {
    QueryGraphPolicyBindingProjection {
        policy_id: binding.policy_id,
        enforced: binding.enforced,
        namespace: binding
            .namespace
            .map(|namespace| namespace.parts().to_vec()),
        table: binding.table.map(|table| table.as_str().to_string()),
        odrl: binding.odrl,
    }
}

pub fn catalog_graph_from_tables(tables: &[QueryGraphTableProjection]) -> QueryGraphCatalogGraph {
    catalog_graph_from_tables_and_views(tables, &[])
}

pub fn catalog_graph_from_tables_and_views(
    tables: &[QueryGraphTableProjection],
    views: &[QueryGraphViewProjection],
) -> QueryGraphCatalogGraph {
    let warehouse = tables
        .first()
        .map(|table| table.ident.warehouse.clone())
        .or_else(|| {
            views
                .first()
                .and_then(|view| WarehouseName::new(view.warehouse.clone()).ok())
        })
        .unwrap_or_else(|| WarehouseName::new("default").expect("static warehouse name"));
    catalog_graph_from_tables_and_views_for_warehouse(
        &warehouse,
        tables,
        views,
        &QueryGraphTenantProjection::default(),
    )
}

pub fn catalog_graph_from_tables_and_views_for_warehouse(
    warehouse: &WarehouseName,
    tables: &[QueryGraphTableProjection],
    views: &[QueryGraphViewProjection],
    tenant: &QueryGraphTenantProjection,
) -> QueryGraphCatalogGraph {
    let mut nodes = BTreeMap::new();
    let mut edges = BTreeSet::new();
    insert_node(
        &mut nodes,
        QueryGraphNode {
            id: "lakecat:catalog".to_string(),
            label: "Catalog".to_string(),
            properties: json!({ "name": "LakeCat" }),
        },
    );
    insert_tenant_spine(&mut nodes, &mut edges, warehouse, tenant);
    for table in tables {
        let namespace_id = format!(
            "lakecat:namespace:{}:{}",
            table.ident.warehouse, table.ident.namespace
        );
        insert_node(
            &mut nodes,
            QueryGraphNode {
                id: namespace_id.clone(),
                label: "Namespace".to_string(),
                properties: json!({
                    "warehouse": table.ident.warehouse.as_str(),
                    "namespace": table.ident.namespace.path(),
                }),
            },
        );
        insert_node(
            &mut nodes,
            QueryGraphNode {
                id: table.stable_id.clone(),
                label: "IcebergTable".to_string(),
                properties: json!({
                    "name": table.ident.name.as_str(),
                    "location": table.location,
                    "metadataLocation": table.metadata_location,
                    "formatVersion": table.format_version,
                }),
            },
        );
        let policy_id = table
            .odrl
            .get("@id")
            .and_then(Value::as_str)
            .unwrap_or("lakecat:policy:unknown")
            .to_string();
        insert_node(
            &mut nodes,
            QueryGraphNode {
                id: policy_id.clone(),
                label: "ODRLPolicy".to_string(),
                properties: json!({ "target": table.stable_id }),
            },
        );
        edges.insert(QueryGraphEdge {
            from: "lakecat:catalog".to_string(),
            to: namespace_id.clone(),
            label: "HAS_NAMESPACE".to_string(),
        });
        edges.insert(QueryGraphEdge {
            from: warehouse_graph_id(&table.ident.warehouse),
            to: namespace_id.clone(),
            label: "HAS_NAMESPACE".to_string(),
        });
        edges.insert(QueryGraphEdge {
            from: namespace_id,
            to: table.stable_id.clone(),
            label: "CONTAINS_TABLE".to_string(),
        });
        edges.insert(QueryGraphEdge {
            from: table.stable_id.clone(),
            to: policy_id,
            label: "GOVERNED_BY".to_string(),
        });
    }
    for view in views {
        let namespace_id = format!(
            "lakecat:namespace:{}:{}",
            view.warehouse,
            view.namespace.join(".")
        );
        insert_node(
            &mut nodes,
            QueryGraphNode {
                id: namespace_id.clone(),
                label: "Namespace".to_string(),
                properties: json!({
                    "warehouse": view.warehouse,
                    "namespace": view.namespace.join("."),
                }),
            },
        );
        insert_node(
            &mut nodes,
            QueryGraphNode {
                id: view.stable_id.clone(),
                label: "View".to_string(),
                properties: json!({
                    "name": view.name,
                    "viewVersion": view.view_version,
                    "dialect": view.dialect,
                    "schemaVersion": view.schema_version,
                    "columns": view.columns,
                }),
            },
        );
        edges.insert(QueryGraphEdge {
            from: "lakecat:catalog".to_string(),
            to: namespace_id.clone(),
            label: "HAS_NAMESPACE".to_string(),
        });
        if let Ok(view_warehouse) = WarehouseName::new(view.warehouse.clone()) {
            edges.insert(QueryGraphEdge {
                from: warehouse_graph_id(&view_warehouse),
                to: namespace_id.clone(),
                label: "HAS_NAMESPACE".to_string(),
            });
        }
        edges.insert(QueryGraphEdge {
            from: namespace_id,
            to: view.stable_id.clone(),
            label: "CONTAINS_VIEW".to_string(),
        });
    }
    QueryGraphCatalogGraph {
        nodes: nodes.into_values().collect(),
        edges: edges.into_iter().collect(),
    }
}

fn insert_tenant_spine(
    nodes: &mut BTreeMap<String, QueryGraphNode>,
    edges: &mut BTreeSet<QueryGraphEdge>,
    warehouse: &WarehouseName,
    tenant: &QueryGraphTenantProjection,
) {
    let server_id = server_graph_id(&tenant.server_id);
    let project_id = project_graph_id(&tenant.project_id);
    let warehouse_id = warehouse_graph_id(warehouse);
    insert_node(
        nodes,
        QueryGraphNode {
            id: server_id.clone(),
            label: "Server".to_string(),
            properties: json!({
                "serverId": tenant.server_id,
                "displayName": tenant.server_display_name,
                "endpointUrlHash": tenant.server_endpoint_url_hash,
                "source": tenant.source
            }),
        },
    );
    insert_node(
        nodes,
        QueryGraphNode {
            id: project_id.clone(),
            label: "Project".to_string(),
            properties: json!({
                "projectId": tenant.project_id,
                "displayName": tenant.project_display_name,
                "serverId": tenant.server_id,
                "source": tenant.source
            }),
        },
    );
    insert_node(
        nodes,
        QueryGraphNode {
            id: warehouse_id.clone(),
            label: "Warehouse".to_string(),
            properties: json!({
                "warehouse": tenant
                    .warehouse
                    .as_deref()
                    .unwrap_or_else(|| warehouse.as_str()),
                "projectId": tenant
                    .warehouse_project_id
                    .as_deref()
                    .unwrap_or_else(|| tenant.project_id.as_str()),
                "storageRootHash": tenant.warehouse_storage_root_hash,
                "source": tenant.source
            }),
        },
    );
    edges.insert(QueryGraphEdge {
        from: "lakecat:catalog".to_string(),
        to: server_id.clone(),
        label: "HAS_SERVER".to_string(),
    });
    edges.insert(QueryGraphEdge {
        from: server_id,
        to: project_id.clone(),
        label: "HAS_PROJECT".to_string(),
    });
    edges.insert(QueryGraphEdge {
        from: project_id,
        to: warehouse_id,
        label: "HAS_WAREHOUSE".to_string(),
    });
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct IcebergFieldProjection {
    id: Option<i64>,
    name: String,
    data_type: String,
    required: bool,
    description: String,
    semantic_type: Option<String>,
}

fn croissant_dataset(
    table: &TableRecord,
    stable_id: &str,
    fields: &[IcebergFieldProjection],
) -> Value {
    json!({
        "@context": {
            "@vocab": "https://schema.org/",
            "cr": "http://mlcommons.org/croissant/",
            "dcat": "http://www.w3.org/ns/dcat#",
            "odrl": "http://www.w3.org/ns/odrl/2/"
        },
        "@type": "cr:Dataset",
        "@id": stable_id,
        "name": table.ident.name.as_str(),
        "description": format!("Iceberg table {} served by LakeCat for QueryGraph.", table.ident.stable_id()),
        "license": "https://spdx.org/licenses/Apache-2.0.html",
        "creator": [{"@type": "Organization", "name": "LakeCat"}],
        "keywords": ["lakecat", "iceberg", "sail", "querygraph"],
        "distribution": [{
            "@type": "cr:FileObject",
            "@id": format!("{stable_id}#metadata"),
            "name": "Iceberg table metadata",
            "contentUrl": table.metadata_location.as_deref().unwrap_or(&table.location),
            "encodingFormat": "application/vnd.apache.iceberg.metadata+json"
        }],
        "recordSet": [{
            "@type": "cr:RecordSet",
            "@id": format!("{stable_id}#record-set"),
            "name": table.ident.name.as_str(),
            "field": fields.iter().map(croissant_field).collect::<Vec<_>>()
        }]
    })
}

fn cdif_resource(
    table: &TableRecord,
    stable_id: &str,
    fields: &[IcebergFieldProjection],
    odrl: Value,
) -> Value {
    json!({
        "@context": {
            "cdif": "https://cdif.codata.org/",
            "dcat": "http://www.w3.org/ns/dcat#",
            "dct": "http://purl.org/dc/terms/",
            "odrl": "http://www.w3.org/ns/odrl/2/"
        },
        "@type": "dcat:Dataset",
        "@id": stable_id,
        "dct:title": table.ident.name.as_str(),
        "dct:description": format!("LakeCat CDIF projection for Iceberg table {}.", table.ident.stable_id()),
        "cdif:profile": [
            "https://cdif.codata.org/profile/discovery",
            "https://cdif.codata.org/profile/manifest",
            "https://cdif.codata.org/profile/data-description",
            "https://cdif.codata.org/profile/data-access",
            "https://cdif.codata.org/profile/access-rights",
            "https://cdif.codata.org/profile/data-integration",
            "https://cdif.codata.org/profile/provenance"
        ],
        "dcat:landingPage": format!("lakecat://{}", table.ident.stable_id()),
        "dcat:accessService": {
            "@type": "dcat:DataService",
            "endpointURL": format!("/catalog/v1/namespaces/{}/tables/{}", table.ident.namespace.path(), table.ident.name.as_str())
        },
        "dcat:distribution": [{
            "@type": "dcat:Distribution",
            "@id": format!("{stable_id}#metadata"),
            "dct:title": "Iceberg table metadata",
            "dcat:downloadURL": table.metadata_location.as_deref().unwrap_or(&table.location),
            "dcat:mediaType": "application/vnd.apache.iceberg.metadata+json"
        }],
        "cdif:dataElement": fields.iter().map(|field| {
            json!({
                "@type": "cdif:DataElement",
                "@id": format!("{stable_id}/field/{}", field.name),
                "dct:title": field.name,
                "dct:description": field.description,
                "cdif:dataType": field.data_type,
                "cdif:semanticType": field.semantic_type,
                "cdif:recordSet": format!("{stable_id}#record-set")
            })
        }).collect::<Vec<_>>(),
        "dct:accessRights": {
            "@type": "dct:RightsStatement",
            "@id": odrl.get("@id").and_then(Value::as_str),
            "dct:license": "https://spdx.org/licenses/Apache-2.0.html",
            "dct:description": "Access and usage must satisfy ODRL and TypeSec policy before agent use.",
            "odrl:policy": odrl
        }
    })
}

fn osi_handoff(table: &TableRecord, stable_id: &str, fields: &[IcebergFieldProjection]) -> Value {
    json!({
        "schemaVersion": "lakecat.querygraph.osi-handoff.v1",
        "standard": "Open Semantic Interchange",
        "ownership": {
            "authoritativeSystem": "QueryGraph",
            "lakecatRole": "catalog-discovery-handoff"
        },
        "dataset": {
            "stableId": stable_id,
            "name": safe_sql_name(table.ident.name.as_str()),
            "warehouse": table.ident.warehouse.as_str(),
            "namespace": table.ident.namespace.path(),
            "location": table.location,
            "metadataLocation": table.metadata_location,
            "source": {
                "type": "iceberg-rest",
                "catalog": "lakecat",
                "governedPlanner": "sail",
                "table": table.ident.stable_id()
            },
            "fields": fields.iter().map(|field| {
                json!({
                    "id": field.id,
                    "name": field.name,
                    "dataType": field.data_type,
                    "required": field.required,
                    "description": field.description,
                    "semanticType": field.semantic_type
                })
            }).collect::<Vec<_>>()
        },
        "policy": {
            "odrlPolicyId": format!("{stable_id}#odrl"),
            "governance": "TypeSec capabilities and ODRL constraints are enforced by LakeCat before governed Sail planning."
        },
        "queryGraphImport": {
            "semanticModelStatus": "delegated",
            "expectedOwner": "QueryGraph",
            "notes": "LakeCat does not publish metrics, dimensions, measures, joins, or business ontology claims as authoritative OSI semantics."
        }
    })
}

fn view_osi_handoff(view: &ViewRecord, stable_id: &str) -> Value {
    json!({
        "schemaVersion": "lakecat.querygraph.view-osi-handoff.v1",
        "standard": "Open Semantic Interchange",
        "ownership": {
            "authoritativeSystem": "QueryGraph",
            "lakecatRole": "catalog-view-discovery-handoff"
        },
        "view": {
            "stableId": stable_id,
            "name": safe_sql_name(view.name.as_str()),
            "warehouse": view.warehouse.as_str(),
            "namespace": view.namespace.path(),
            "viewVersion": view.view_version,
            "dialect": view.dialect,
            "schemaVersion": view.schema_version,
            "columns": view.columns,
            "sql": view.sql,
            "properties": view.properties
        },
        "policy": {
            "governance": "View access is governed by LakeCat and TypeSec before QueryGraph or agents materialize dependent reads."
        },
        "queryGraphImport": {
            "semanticModelStatus": "delegated",
            "expectedOwner": "QueryGraph",
            "notes": "LakeCat publishes catalog-owned view definitions, not authoritative business metrics, dimensions, measures, or joins."
        }
    })
}

fn odrl_policy(stable_id: &str, policy_bindings: &[QueryGraphPolicyBindingProjection]) -> Value {
    json!({
        "@type": "odrl:Policy",
        "@id": format!("{stable_id}#odrl"),
        "odrl:target": stable_id,
        "odrl:assigner": "did:web:querygraph.ai:lakecat",
        "lakecat:policy-bindings": policy_bindings,
        "odrl:permission": [
            {
                "odrl:action": "odrl:read",
                "odrl:assignee": "did:web:querygraph.ai:agent",
                "odrl:constraint": "typesec:catalog.table.load"
            },
            {
                "odrl:action": "querygraph:index",
                "odrl:assignee": "did:web:querygraph.ai:agent",
                "odrl:constraint": "typesec:catalog.table.plan_scan"
            }
        ],
        "odrl:prohibition": []
    })
}

fn bootstrap_open_lineage(
    warehouse: &WarehouseName,
    tables: &[QueryGraphTableProjection],
    views: &[QueryGraphViewProjection],
    table_artifacts: &[QueryGraphTableArtifactHashes],
    view_artifacts: &[QueryGraphViewArtifactHashes],
    graph_hash: &str,
    generated_at: DateTime<Utc>,
) -> Value {
    json!({
        "eventType": "COMPLETE",
        "eventTime": generated_at,
        "run": {
            "runId": format!("lakecat-querygraph-bootstrap-{}", warehouse.as_str()),
            "facets": {
                "queryGraph_semanticBundle": {
                    "_producer": "https://querygraph.ai/lakecat",
                    "_schemaURL": "https://querygraph.ai/schemas/openlineage/querygraph-semantic-bundle-facet/0.1.0.json",
                    "tableCount": tables.len(),
                    "viewCount": views.len(),
                    "standards": querygraph_bootstrap_standards(),
                    "graphHash": graph_hash,
                    "tableArtifacts": table_artifacts.iter().map(open_lineage_table_artifact).collect::<Vec<_>>(),
                    "viewArtifacts": view_artifacts.iter().map(open_lineage_view_artifact).collect::<Vec<_>>()
                }
            }
        },
        "job": {
            "namespace": format!("lakecat.{}", warehouse.as_str()),
            "name": "querygraph-bootstrap"
        },
        "inputs": [],
        "outputs": tables.iter().map(|table| {
            json!({
                "namespace": format!("lakecat.{}.{}", table.ident.warehouse, table.ident.namespace),
                "name": table.ident.name.as_str(),
                "facets": {
                    "dataSource": {
                        "_producer": "https://querygraph.ai/lakecat",
                        "_schemaURL": "https://openlineage.io/spec/facets/1-0-0/DatasourceDatasetFacet.json",
                        "name": "LakeCat",
                        "uri": table.location
                    },
                    "queryGraph_catalog": {
                        "_producer": "https://querygraph.ai/lakecat",
                        "_schemaURL": "https://querygraph.ai/schemas/openlineage/querygraph-catalog-facet/0.1.0.json",
                        "stableId": table.stable_id,
                        "metadataLocation": table.metadata_location,
                        "formatVersion": table.format_version
                    }
                }
            })
        }).chain(views.iter().map(|view| {
            json!({
                "namespace": format!("lakecat.{}.{}", view.warehouse, view.namespace.join(".")),
                "name": view.name,
                "facets": {
                    "queryGraph_catalogView": {
                        "_producer": "https://querygraph.ai/lakecat",
                        "_schemaURL": "https://querygraph.ai/schemas/openlineage/querygraph-catalog-view-facet/0.1.0.json",
                        "stableId": view.stable_id,
                        "viewVersion": view.view_version,
                        "dialect": view.dialect,
                        "schemaVersion": view.schema_version
                    }
                }
            })
        })).collect::<Vec<_>>(),
        "producer": "https://querygraph.ai/lakecat",
        "schemaURL": "https://openlineage.io/spec/2-0-2/OpenLineage.json"
    })
}

fn open_lineage_table_artifact(artifact: &QueryGraphTableArtifactHashes) -> Value {
    json!({
        "stableId": artifact.stable_id,
        "croissantHash": artifact.croissant_hash,
        "cdifHash": artifact.cdif_hash,
        "osiHash": artifact.osi_hash,
        "odrlHash": artifact.odrl_hash,
        "policyBindingsHash": artifact.policy_bindings_hash
    })
}

fn open_lineage_view_artifact(artifact: &QueryGraphViewArtifactHashes) -> Value {
    json!({
        "stableId": artifact.stable_id,
        "osiHash": artifact.osi_hash
    })
}

fn view_stable_id(view: &ViewRecord) -> String {
    format!(
        "lakecat:view:{}:{}:{}",
        view.warehouse, view.namespace, view.name
    )
}

fn croissant_field(field: &IcebergFieldProjection) -> Value {
    json!({
        "@type": "cr:Field",
        "name": field.name,
        "dataType": field.data_type,
        "description": field.description,
        "sameAs": field.semantic_type,
        "required": field.required,
        "source": field.id.map(|id| format!("iceberg-field-id:{id}"))
    })
}

fn iceberg_fields(metadata: &Value) -> Vec<IcebergFieldProjection> {
    let schema = current_schema(metadata)
        .or_else(|| metadata.get("schema"))
        .unwrap_or(&Value::Null);
    schema
        .get("fields")
        .and_then(Value::as_array)
        .map(|fields| fields.iter().map(iceberg_field).collect())
        .unwrap_or_default()
}

fn current_schema(metadata: &Value) -> Option<&Value> {
    let current_schema_id = metadata.get("current-schema-id").and_then(Value::as_i64)?;
    metadata
        .get("schemas")
        .and_then(Value::as_array)?
        .iter()
        .find(|schema| schema.get("schema-id").and_then(Value::as_i64) == Some(current_schema_id))
}

fn iceberg_field(field: &Value) -> IcebergFieldProjection {
    let name = field
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("field")
        .to_string();
    IcebergFieldProjection {
        id: field.get("id").and_then(Value::as_i64),
        data_type: field_type(field.get("type").unwrap_or(&Value::Null)),
        required: field
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        description: field
            .get("doc")
            .or_else(|| field.get("description"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Iceberg field {name}.")),
        semantic_type: field
            .get("semantic-type")
            .or_else(|| field.get("semanticType"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        name,
    }
}

fn field_type(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Object(map) => map
            .get("type")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| "struct".to_string()),
        _ => "unknown".to_string(),
    }
}

fn safe_sql_name(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "lakecat_value".to_string()
    } else {
        out.to_string()
    }
}

#[cfg(test)]
mod tests;
