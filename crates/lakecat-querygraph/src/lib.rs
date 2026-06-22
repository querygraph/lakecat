use chrono::{DateTime, Utc};
use lakecat_core::{
    LakeCatResult, TableIdent, WarehouseName, content_hash_bytes, content_hash_json,
};
use lakecat_store::{
    PolicyBinding, ProjectRecord, ServerRecord, TableRecord, ViewRecord, WarehouseRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphBootstrap {
    pub warehouse: WarehouseName,
    pub generated_at: DateTime<Utc>,
    pub bundle_hash: String,
    pub manifest: QueryGraphBundleManifest,
    pub tables: Vec<QueryGraphTableProjection>,
    pub views: Vec<QueryGraphViewProjection>,
    pub graph: QueryGraphCatalogGraph,
    pub open_lineage: Value,
}

impl QueryGraphBootstrap {
    pub fn from_tables(
        warehouse: WarehouseName,
        tables: impl IntoIterator<Item = TableRecord>,
    ) -> LakeCatResult<Self> {
        Self::from_tables_with_policy_bindings(
            warehouse,
            tables.into_iter().map(|table| (table, Vec::new())),
        )
    }

    pub fn from_tables_with_policy_bindings(
        warehouse: WarehouseName,
        tables: impl IntoIterator<Item = (TableRecord, Vec<PolicyBinding>)>,
    ) -> LakeCatResult<Self> {
        Self::from_tables_views_with_policy_bindings(warehouse, tables, Vec::new())
    }

    pub fn from_tables_views_with_policy_bindings(
        warehouse: WarehouseName,
        tables: impl IntoIterator<Item = (TableRecord, Vec<PolicyBinding>)>,
        views: impl IntoIterator<Item = ViewRecord>,
    ) -> LakeCatResult<Self> {
        Self::from_tables_views_with_policy_bindings_and_tenant(
            warehouse,
            tables,
            views,
            QueryGraphTenantProjection::default(),
        )
    }

    pub fn from_tables_views_with_policy_bindings_and_tenant(
        warehouse: WarehouseName,
        tables: impl IntoIterator<Item = (TableRecord, Vec<PolicyBinding>)>,
        views: impl IntoIterator<Item = ViewRecord>,
        tenant: QueryGraphTenantProjection,
    ) -> LakeCatResult<Self> {
        let generated_at = Utc::now();
        let tables = tables
            .into_iter()
            .map(|(table, policy_bindings)| {
                QueryGraphTableProjection::from_table_with_policy_bindings(table, policy_bindings)
            })
            .collect::<Vec<_>>();
        let views = views
            .into_iter()
            .map(QueryGraphViewProjection::from_view)
            .collect::<Vec<_>>();
        let graph = QueryGraphCatalogGraph::from_tables_and_views_for_warehouse(
            &warehouse, &tables, &views, &tenant,
        );
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
        Ok(Self {
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

    pub fn with_view_receipt_evidence(
        mut self,
        evidence: Vec<QueryGraphViewReceiptEvidence>,
    ) -> LakeCatResult<Self> {
        validate_view_receipt_evidence(&self.views, &evidence)?;
        let evidence_hash = if evidence.is_empty() {
            None
        } else {
            Some(view_receipt_evidence_hash(&evidence)?)
        };
        let import_contract = self.manifest.querygraph_import.as_mut().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QueryGraph bootstrap manifest is missing querygraph-import compatibility contract"
                    .to_string(),
            )
        })?;
        import_contract.view_receipt_evidence = evidence;
        import_contract.view_receipt_evidence_hash = evidence_hash;
        self.bundle_hash = self.computed_bundle_hash()?;
        Ok(self)
    }

    pub fn verify_manifest(&self) -> LakeCatResult<QueryGraphBootstrapVerification> {
        if self.manifest.schema_version != "lakecat.querygraph.bootstrap.v1" {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "unsupported QueryGraph bootstrap manifest schema {}",
                self.manifest.schema_version
            )));
        }
        if self.manifest.table_artifacts.len() != self.tables.len() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap manifest lists {} table artifacts for {} tables",
                self.manifest.table_artifacts.len(),
                self.tables.len()
            )));
        }
        if self.manifest.view_artifacts.len() != self.views.len() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap manifest lists {} view artifacts for {} views",
                self.manifest.view_artifacts.len(),
                self.views.len()
            )));
        }
        validate_duplicate_free_stable_ids(
            "QueryGraph bootstrap table projections",
            self.tables.iter().map(|table| table.stable_id.as_str()),
        )?;
        validate_duplicate_free_stable_ids(
            "QueryGraph bootstrap table artifacts",
            self.manifest
                .table_artifacts
                .iter()
                .map(|artifact| artifact.stable_id.as_str()),
        )?;
        validate_duplicate_free_stable_ids(
            "QueryGraph bootstrap view projections",
            self.views.iter().map(|view| view.stable_id.as_str()),
        )?;
        validate_duplicate_free_stable_ids(
            "QueryGraph bootstrap view artifacts",
            self.manifest
                .view_artifacts
                .iter()
                .map(|artifact| artifact.stable_id.as_str()),
        )?;

        let open_lineage_hash = content_hash_json(&self.open_lineage)?;
        if self.manifest.open_lineage_hash != open_lineage_hash {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap OpenLineage hash mismatch: manifest {}, computed {}",
                self.manifest.open_lineage_hash, open_lineage_hash
            )));
        }
        let graph_hash = graph_hash(&self.graph)?;
        if self.manifest.graph_hash != graph_hash {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap graph hash mismatch: manifest {}, computed {}",
                self.manifest.graph_hash, graph_hash
            )));
        }

        for table in &self.tables {
            let expected = self
                .manifest
                .table_artifacts
                .iter()
                .find(|artifact| artifact.stable_id == table.stable_id)
                .ok_or_else(|| {
                    lakecat_core::LakeCatError::InvalidArgument(format!(
                        "QueryGraph bootstrap manifest is missing table {}",
                        table.stable_id
                    ))
                })?;
            expected.verify(table)?;
        }
        for view in &self.views {
            let expected = self
                .manifest
                .view_artifacts
                .iter()
                .find(|artifact| artifact.stable_id == view.stable_id)
                .ok_or_else(|| {
                    lakecat_core::LakeCatError::InvalidArgument(format!(
                        "QueryGraph bootstrap manifest is missing view {}",
                        view.stable_id
                    ))
                })?;
            expected.verify(view)?;
        }

        let import_contract = self.manifest.querygraph_import.as_ref().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QueryGraph bootstrap manifest is missing querygraph-import compatibility contract"
                    .to_string(),
            )
        })?;
        let table_only_bundle_hash = table_only_querygraph_import_hash(
            &self.warehouse,
            &self.manifest,
            &self.tables,
            &self.graph,
            &self.open_lineage,
        )?;
        if import_contract.table_only_bundle_hash != table_only_bundle_hash {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap import hash mismatch: manifest {}, computed {}",
                import_contract.table_only_bundle_hash, table_only_bundle_hash
            )));
        }
        if import_contract.view_count != self.views.len() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap import contract view count {} does not match bundle views {}",
                import_contract.view_count,
                self.views.len()
            )));
        }
        if import_contract.graph_hash != self.manifest.graph_hash {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap import contract graph hash {} does not match manifest {}",
                import_contract.graph_hash, self.manifest.graph_hash
            )));
        }
        validate_view_receipt_evidence(&self.views, &import_contract.view_receipt_evidence)?;
        if import_contract.view_receipt_evidence.is_empty() {
            if import_contract.view_receipt_evidence_hash.is_some() {
                return Err(lakecat_core::LakeCatError::InvalidArgument(
                    "QueryGraph bootstrap import contract has a receipt evidence hash without receipt evidence"
                        .to_string(),
                ));
            }
        } else {
            let evidence_hash = view_receipt_evidence_hash(&import_contract.view_receipt_evidence)?;
            if import_contract.view_receipt_evidence_hash.as_deref() != Some(evidence_hash.as_str())
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QueryGraph bootstrap import receipt evidence hash mismatch: manifest {:?}, computed {}",
                    import_contract.view_receipt_evidence_hash, evidence_hash
                )));
            }
        }

        let bundle_hash = self.computed_bundle_hash()?;
        if self.bundle_hash != bundle_hash {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap bundle hash mismatch: manifest {}, computed {}",
                self.bundle_hash, bundle_hash
            )));
        }

        Ok(QueryGraphBootstrapVerification {
            warehouse: self.warehouse.as_str().to_string(),
            table_count: self.tables.len(),
            view_count: self.views.len(),
            verified_tables: self
                .tables
                .iter()
                .map(|table| table.stable_id.clone())
                .collect(),
            verified_views: self
                .views
                .iter()
                .map(|view| view.stable_id.clone())
                .collect(),
            verified_view_versions: self
                .views
                .iter()
                .map(|view| (view.stable_id.clone(), view.view_version))
                .collect(),
            verified_view_receipt_hashes: import_contract
                .view_receipt_evidence
                .iter()
                .map(|evidence| (evidence.stable_id.clone(), evidence.receipt_hash.clone()))
                .collect(),
            verified_view_receipt_chain_hashes: import_contract
                .view_receipt_evidence
                .iter()
                .map(|evidence| {
                    (
                        evidence.stable_id.clone(),
                        evidence.receipt_chain_hash.clone(),
                    )
                })
                .collect(),
            bundle_hash,
            graph_hash,
            open_lineage_hash,
            querygraph_import_hash: import_contract.table_only_bundle_hash.clone(),
            standards: self.manifest.standards.clone(),
        })
    }

    fn computed_bundle_hash(&self) -> LakeCatResult<String> {
        content_hash_json(&json!({
            "warehouse": self.warehouse.as_str(),
            "manifest": self.manifest,
            "tables": self.tables,
            "views": self.views,
            "graph": self.graph,
            "openLineage": self.open_lineage,
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphTenantProjection {
    pub server_id: String,
    pub server_display_name: Option<String>,
    pub server_endpoint_url_hash: Option<String>,
    pub project_id: String,
    pub project_display_name: Option<String>,
    pub warehouse: Option<String>,
    pub warehouse_project_id: Option<String>,
    pub warehouse_storage_root_hash: Option<String>,
    pub source: String,
}

impl QueryGraphTenantProjection {
    pub fn from_records(
        warehouse: &WarehouseName,
        warehouse_record: Option<&WarehouseRecord>,
        project_record: Option<&ProjectRecord>,
        server_record: Option<&ServerRecord>,
    ) -> Self {
        let project_id = warehouse_record
            .map(|record| record.project_id.clone())
            .or_else(|| project_record.map(|record| record.project_id.clone()))
            .unwrap_or_else(|| "default".to_string());
        let server_id = project_record
            .and_then(|record| record.server_id.clone())
            .or_else(|| server_record.map(|record| record.server_id.clone()))
            .unwrap_or_else(|| "default".to_string());
        Self {
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
            source: if warehouse_record.is_some()
                || project_record.is_some()
                || server_record.is_some()
            {
                "lakecat-management-records".to_string()
            } else {
                "lakecat-querygraph-bootstrap".to_string()
            },
        }
    }
}

impl Default for QueryGraphTenantProjection {
    fn default() -> Self {
        Self {
            server_id: "default".to_string(),
            server_display_name: None,
            server_endpoint_url_hash: None,
            project_id: "default".to_string(),
            project_display_name: None,
            warehouse: None,
            warehouse_project_id: None,
            warehouse_storage_root_hash: None,
            source: "lakecat-querygraph-bootstrap".to_string(),
        }
    }
}

fn server_endpoint_url_hash(endpoint_url: &str) -> String {
    content_hash_json(&json!({"endpoint-url": endpoint_url}))
        .unwrap_or_else(|_| content_hash_bytes(endpoint_url.as_bytes()))
}

fn warehouse_storage_root_hash(storage_root: &str) -> String {
    content_hash_json(&json!({"storage-root": storage_root}))
        .unwrap_or_else(|_| content_hash_bytes(storage_root.as_bytes()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphBundleManifest {
    pub schema_version: String,
    pub producer: String,
    pub standards: Vec<String>,
    pub table_artifacts: Vec<QueryGraphTableArtifactHashes>,
    pub view_artifacts: Vec<QueryGraphViewArtifactHashes>,
    pub graph_hash: String,
    pub open_lineage_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub querygraph_import: Option<QueryGraphImportCompatibility>,
}

impl QueryGraphBundleManifest {
    fn from_hashes(
        table_artifacts: Vec<QueryGraphTableArtifactHashes>,
        view_artifacts: Vec<QueryGraphViewArtifactHashes>,
        graph_hash: String,
        open_lineage: &Value,
    ) -> LakeCatResult<Self> {
        Ok(Self {
            schema_version: "lakecat.querygraph.bootstrap.v1".to_string(),
            producer: "https://querygraph.ai/lakecat".to_string(),
            standards: querygraph_bootstrap_standards(),
            table_artifacts,
            view_artifacts,
            graph_hash,
            open_lineage_hash: content_hash_json(open_lineage)?,
            querygraph_import: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphImportCompatibility {
    pub schema_version: String,
    pub table_only_bundle_hash: String,
    pub view_count: usize,
    pub graph_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_receipt_evidence: Vec<QueryGraphViewReceiptEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_receipt_evidence_hash: Option<String>,
}

impl QueryGraphImportCompatibility {
    fn from_table_only_bundle(
        warehouse: &WarehouseName,
        manifest: &QueryGraphBundleManifest,
        tables: &[QueryGraphTableProjection],
        graph: &QueryGraphCatalogGraph,
        open_lineage: &Value,
        view_count: usize,
    ) -> LakeCatResult<Self> {
        Ok(Self {
            schema_version: "lakecat.querygraph.import-compat.v1".to_string(),
            table_only_bundle_hash: table_only_querygraph_import_hash(
                warehouse,
                manifest,
                tables,
                graph,
                open_lineage,
            )?,
            view_count,
            graph_hash: manifest.graph_hash.clone(),
            view_receipt_evidence: Vec::new(),
            view_receipt_evidence_hash: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphViewReceiptEvidence {
    pub stable_id: String,
    pub view_version: u64,
    pub receipt_hash: String,
    pub receipt_chain_hash: String,
}

fn validate_view_receipt_evidence(
    views: &[QueryGraphViewProjection],
    evidence: &[QueryGraphViewReceiptEvidence],
) -> LakeCatResult<()> {
    if views.is_empty() {
        if evidence.is_empty() {
            return Ok(());
        }
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "QueryGraph bootstrap import contract carries view receipt evidence for a bundle without views"
                .to_string(),
        ));
    }
    if evidence.len() != views.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QueryGraph bootstrap import contract lists {} view receipt evidence record(s) for {} view artifact(s)",
            evidence.len(),
            views.len()
        )));
    }
    for view in views {
        let Some(record) = evidence
            .iter()
            .find(|record| record.stable_id == view.stable_id)
        else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap import contract is missing view receipt evidence for {}",
                view.stable_id
            )));
        };
        if record.view_version != view.view_version {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap import contract view receipt evidence for {} has version {}, expected {}",
                view.stable_id, record.view_version, view.view_version
            )));
        }
        if record.receipt_hash.is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap import contract view receipt evidence for {} is missing a receipt hash",
                view.stable_id
            )));
        }
        if record.receipt_chain_hash.is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap import contract view receipt evidence for {} is missing a receipt-chain hash",
                view.stable_id
            )));
        }
    }
    Ok(())
}

fn view_receipt_evidence_hash(evidence: &[QueryGraphViewReceiptEvidence]) -> LakeCatResult<String> {
    let value = serde_json::to_value(evidence).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QueryGraph view receipt evidence: {err}"
        ))
    })?;
    content_hash_json(&value)
}

fn querygraph_bootstrap_standards() -> Vec<String> {
    vec![
        "Iceberg REST".to_string(),
        "Croissant".to_string(),
        "CDIF".to_string(),
        "OSI handoff".to_string(),
        "ODRL".to_string(),
        "Grust catalog graph".to_string(),
        "OpenLineage".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphViewArtifactHashes {
    pub stable_id: String,
    pub osi_hash: String,
}

impl QueryGraphViewArtifactHashes {
    fn from_view(view: &QueryGraphViewProjection) -> LakeCatResult<Self> {
        Ok(Self {
            stable_id: view.stable_id.clone(),
            osi_hash: content_hash_json(&view.osi)?,
        })
    }

    fn verify(&self, view: &QueryGraphViewProjection) -> LakeCatResult<()> {
        verify_hash("view OSI", &self.osi_hash, &view.osi)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphTableArtifactHashes {
    pub stable_id: String,
    pub croissant_hash: String,
    pub cdif_hash: String,
    pub osi_hash: String,
    pub odrl_hash: String,
    pub policy_bindings_hash: String,
}

impl QueryGraphTableArtifactHashes {
    fn from_table(table: &QueryGraphTableProjection) -> LakeCatResult<Self> {
        Ok(Self {
            stable_id: table.stable_id.clone(),
            croissant_hash: content_hash_json(&table.croissant)?,
            cdif_hash: content_hash_json(&table.cdif)?,
            osi_hash: content_hash_json(&table.osi)?,
            odrl_hash: content_hash_json(&table.odrl)?,
            policy_bindings_hash: content_hash_json(&policy_bindings_value(table)?)?,
        })
    }

    fn verify(&self, table: &QueryGraphTableProjection) -> LakeCatResult<()> {
        verify_hash("Croissant", &self.croissant_hash, &table.croissant)?;
        verify_hash("CDIF", &self.cdif_hash, &table.cdif)?;
        verify_hash("OSI", &self.osi_hash, &table.osi)?;
        verify_hash("ODRL", &self.odrl_hash, &table.odrl)?;
        verify_hash(
            "policy bindings",
            &self.policy_bindings_hash,
            &policy_bindings_value(table)?,
        )?;
        Ok(())
    }
}

fn policy_bindings_value(table: &QueryGraphTableProjection) -> LakeCatResult<Value> {
    serde_json::to_value(&table.policy_bindings).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QueryGraph policy bindings: {err}"
        ))
    })
}

fn graph_hash(graph: &QueryGraphCatalogGraph) -> LakeCatResult<String> {
    let value = serde_json::to_value(graph).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QueryGraph catalog graph: {err}"
        ))
    })?;
    content_hash_json(&value)
}

fn table_only_querygraph_import_hash(
    warehouse: &WarehouseName,
    manifest: &QueryGraphBundleManifest,
    tables: &[QueryGraphTableProjection],
    graph: &QueryGraphCatalogGraph,
    open_lineage: &Value,
) -> LakeCatResult<String> {
    let graph = serde_json::to_value(graph).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QueryGraph catalog graph for import hash: {err}"
        ))
    })?;
    content_hash_json(&json!({
        "warehouse": warehouse.as_str(),
        "manifest": table_only_querygraph_import_manifest(manifest),
        "tables": tables
            .iter()
            .map(table_only_querygraph_import_table)
            .collect::<Vec<_>>(),
        "graph": graph,
        "openLineage": open_lineage,
    }))
}

fn table_only_querygraph_import_manifest(manifest: &QueryGraphBundleManifest) -> Value {
    json!({
        "schema-version": manifest.schema_version,
        "producer": manifest.producer,
        "standards": manifest.standards,
        "table-artifacts": manifest
            .table_artifacts
            .iter()
            .map(table_only_querygraph_import_table_artifact)
            .collect::<Vec<_>>(),
        "open-lineage-hash": manifest.open_lineage_hash,
    })
}

fn table_only_querygraph_import_table_artifact(artifact: &QueryGraphTableArtifactHashes) -> Value {
    json!({
        "stable-id": artifact.stable_id,
        "croissant-hash": artifact.croissant_hash,
        "cdif-hash": artifact.cdif_hash,
        "osi-hash": artifact.osi_hash,
        "odrl-hash": artifact.odrl_hash,
    })
}

fn table_only_querygraph_import_table(table: &QueryGraphTableProjection) -> Value {
    json!({
        "ident": table.ident,
        "stable-id": table.stable_id,
        "location": table.location,
        "metadata-location": table.metadata_location,
        "version": table.version,
        "format-version": table.format_version,
        "croissant": table.croissant,
        "cdif": table.cdif,
        "osi": table.osi,
        "odrl": table.odrl,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphBootstrapVerification {
    pub warehouse: String,
    pub table_count: usize,
    pub view_count: usize,
    pub verified_tables: Vec<String>,
    pub verified_views: Vec<String>,
    #[serde(default)]
    pub verified_view_versions: BTreeMap<String, u64>,
    #[serde(default)]
    pub verified_view_receipt_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub verified_view_receipt_chain_hashes: BTreeMap<String, String>,
    pub bundle_hash: String,
    pub graph_hash: String,
    pub open_lineage_hash: String,
    pub querygraph_import_hash: String,
    pub standards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphViewProjection {
    pub stable_id: String,
    pub warehouse: String,
    pub namespace: Vec<String>,
    pub name: String,
    pub view_version: u64,
    pub sql: String,
    pub dialect: String,
    pub schema_version: Option<u64>,
    pub columns: Value,
    pub properties: Value,
    pub osi: Value,
}

impl QueryGraphViewProjection {
    pub fn from_view(view: ViewRecord) -> Self {
        let stable_id = view_stable_id(&view);
        let columns = json!(view.columns);
        let properties = json!(view.properties);
        let osi = view_osi_handoff(&view, &stable_id);
        Self {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphTableProjection {
    pub ident: TableIdent,
    pub stable_id: String,
    pub location: String,
    pub metadata_location: Option<String>,
    pub version: u64,
    pub format_version: Option<i64>,
    pub croissant: Value,
    pub cdif: Value,
    pub osi: Value,
    pub odrl: Value,
    pub policy_bindings: Vec<QueryGraphPolicyBindingProjection>,
}

impl QueryGraphTableProjection {
    pub fn from_table(table: TableRecord) -> Self {
        Self::from_table_with_policy_bindings(table, Vec::new())
    }

    pub fn from_table_with_policy_bindings(
        table: TableRecord,
        policy_bindings: Vec<PolicyBinding>,
    ) -> Self {
        let stable_id = table.ident.stable_id();
        let fields = iceberg_fields(&table.metadata);
        let policy_bindings = policy_bindings
            .into_iter()
            .map(QueryGraphPolicyBindingProjection::from_binding)
            .collect::<Vec<_>>();
        let odrl = odrl_policy(&stable_id, &policy_bindings);
        let croissant = croissant_dataset(&table, &stable_id, &fields);
        let cdif = cdif_resource(&table, &stable_id, &fields, odrl.clone());
        let osi = osi_handoff(&table, &stable_id, &fields);
        Self {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphPolicyBindingProjection {
    pub policy_id: String,
    pub enforced: bool,
    pub namespace: Option<Vec<String>>,
    pub table: Option<String>,
    pub odrl: Value,
}

impl QueryGraphPolicyBindingProjection {
    fn from_binding(binding: PolicyBinding) -> Self {
        Self {
            policy_id: binding.policy_id,
            enforced: binding.enforced,
            namespace: binding
                .namespace
                .map(|namespace| namespace.parts().to_vec()),
            table: binding.table.map(|table| table.as_str().to_string()),
            odrl: binding.odrl,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphCatalogGraph {
    pub nodes: Vec<QueryGraphNode>,
    pub edges: Vec<QueryGraphEdge>,
}

impl QueryGraphCatalogGraph {
    pub fn from_tables(tables: &[QueryGraphTableProjection]) -> Self {
        Self::from_tables_and_views(tables, &[])
    }

    pub fn from_tables_and_views(
        tables: &[QueryGraphTableProjection],
        views: &[QueryGraphViewProjection],
    ) -> Self {
        let warehouse = tables
            .first()
            .map(|table| table.ident.warehouse.clone())
            .or_else(|| {
                views
                    .first()
                    .and_then(|view| WarehouseName::new(view.warehouse.clone()).ok())
            })
            .unwrap_or_else(|| WarehouseName::new("default").expect("static warehouse name"));
        Self::from_tables_and_views_for_warehouse(
            &warehouse,
            tables,
            views,
            &QueryGraphTenantProjection::default(),
        )
    }

    pub fn from_tables_and_views_for_warehouse(
        warehouse: &WarehouseName,
        tables: &[QueryGraphTableProjection],
        views: &[QueryGraphViewProjection],
        tenant: &QueryGraphTenantProjection,
    ) -> Self {
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
        Self {
            nodes: nodes.into_values().collect(),
            edges: edges.into_iter().collect(),
        }
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

fn server_graph_id(server_id: &str) -> String {
    format!("lakecat:server:{server_id}")
}

fn project_graph_id(project_id: &str) -> String {
    format!("lakecat:project:{project_id}")
}

fn warehouse_graph_id(warehouse: &WarehouseName) -> String {
    format!("lakecat:warehouse:{}", warehouse.as_str())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryGraphNode {
    pub id: String,
    pub label: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryGraphEdge {
    pub from: String,
    pub to: String,
    pub label: String,
}

impl Ord for QueryGraphEdge {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.from, &self.to, &self.label).cmp(&(&other.from, &other.to, &other.label))
    }
}

impl PartialOrd for QueryGraphEdge {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn insert_node(nodes: &mut BTreeMap<String, QueryGraphNode>, node: QueryGraphNode) {
    nodes.entry(node.id.clone()).or_insert(node);
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

fn verify_hash(label: &str, expected: &str, value: &Value) -> LakeCatResult<()> {
    let computed = content_hash_json(value)?;
    if expected != computed {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QueryGraph bootstrap {label} hash mismatch: manifest {expected}, computed {computed}"
        )));
    }
    Ok(())
}

fn validate_duplicate_free_stable_ids<'a>(
    label: &str,
    values: impl IntoIterator<Item = &'a str>,
) -> LakeCatResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be duplicate-free by stable id: {value}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
            bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]
                ["stableId"],
            bundle.manifest.table_artifacts[0].stable_id
        );
        assert_eq!(
            bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]
                ["croissantHash"],
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
            bundle.tables[0].odrl["lakecat:policy-bindings"][0]["odrl"]["lakecat:read-restriction"]
                ["allowed-columns"],
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

        let graph = QueryGraphCatalogGraph::from_tables_and_views_for_warehouse(
            &warehouse,
            &[],
            &[],
            &tenant,
        );
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
}
