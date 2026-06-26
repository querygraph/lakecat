use chrono::{DateTime, Utc};
use lakecat_core::{
    LakeCatResult, TableIdent, WarehouseName, content_hash_bytes, content_hash_json,
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

pub fn server_endpoint_url_hash(endpoint_url: &str) -> String {
    content_hash_json(&json!({"endpoint-url": endpoint_url}))
        .unwrap_or_else(|_| content_hash_bytes(endpoint_url.as_bytes()))
}

pub fn warehouse_storage_root_hash(storage_root: &str) -> String {
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
    pub fn from_hashes(
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
    pub fn from_table_only_bundle(
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

pub fn validate_view_receipt_evidence(
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

pub fn view_receipt_evidence_hash(
    evidence: &[QueryGraphViewReceiptEvidence],
) -> LakeCatResult<String> {
    let value = serde_json::to_value(evidence).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QueryGraph view receipt evidence: {err}"
        ))
    })?;
    content_hash_json(&value)
}

pub fn querygraph_bootstrap_standards() -> Vec<String> {
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
    pub fn from_view(view: &QueryGraphViewProjection) -> LakeCatResult<Self> {
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
    pub fn from_table(table: &QueryGraphTableProjection) -> LakeCatResult<Self> {
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

pub fn policy_bindings_value(table: &QueryGraphTableProjection) -> LakeCatResult<Value> {
    serde_json::to_value(&table.policy_bindings).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QueryGraph policy bindings: {err}"
        ))
    })
}

pub fn graph_hash(graph: &QueryGraphCatalogGraph) -> LakeCatResult<String> {
    let value = serde_json::to_value(graph).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QueryGraph catalog graph: {err}"
        ))
    })?;
    content_hash_json(&value)
}

pub fn table_only_querygraph_import_hash(
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphPolicyBindingProjection {
    pub policy_id: String,
    pub enforced: bool,
    pub namespace: Option<Vec<String>>,
    pub table: Option<String>,
    pub odrl: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QueryGraphCatalogGraph {
    pub nodes: Vec<QueryGraphNode>,
    pub edges: Vec<QueryGraphEdge>,
}

pub fn server_graph_id(server_id: &str) -> String {
    format!("lakecat:server:{server_id}")
}

pub fn project_graph_id(project_id: &str) -> String {
    format!("lakecat:project:{project_id}")
}

pub fn warehouse_graph_id(warehouse: &WarehouseName) -> String {
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

pub fn insert_node(nodes: &mut BTreeMap<String, QueryGraphNode>, node: QueryGraphNode) {
    nodes.entry(node.id.clone()).or_insert(node);
}

pub fn verify_hash(label: &str, expected: &str, value: &Value) -> LakeCatResult<()> {
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
