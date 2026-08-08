use chrono::{DateTime, Utc};
use lakecat_core::{
    LakeCatResult, TableIdent, TableName, WarehouseName, content_hash_bytes, content_hash_json,
};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

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
        self.manifest
            .attach_view_receipt_evidence(&self.views, evidence)?;
        self.bundle_hash = self.computed_bundle_hash()?;
        Ok(self)
    }

    /// Return the verification-shaped claims captured while this bundle was
    /// constructed, without recomputing content hashes.
    ///
    /// This is only appropriate for an in-process bundle immediately produced
    /// by a trusted constructor that computed every manifest field. Consumers
    /// of deserialized or otherwise external bundles must call
    /// [`Self::verify_manifest`] instead.
    pub fn construction_summary(&self) -> LakeCatResult<QueryGraphBootstrapVerification> {
        let import_contract = self.manifest.querygraph_import.as_ref().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QueryGraph bootstrap manifest is missing querygraph-import compatibility contract"
                    .to_string(),
            )
        })?;
        Ok(self.verification_summary(import_contract))
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
            "QueryGraph bootstrap view projections",
            self.views.iter().map(|view| view.stable_id.as_str()),
        )?;
        let table_artifacts = index_duplicate_free_stable_ids(
            "QueryGraph bootstrap table artifacts",
            self.manifest
                .table_artifacts
                .iter()
                .map(|artifact| (artifact.stable_id.as_str(), artifact)),
        )?;
        let view_artifacts = index_duplicate_free_stable_ids(
            "QueryGraph bootstrap view artifacts",
            self.manifest
                .view_artifacts
                .iter()
                .map(|artifact| (artifact.stable_id.as_str(), artifact)),
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
            let expected = table_artifacts
                .get(table.stable_id.as_str())
                .ok_or_else(|| {
                    lakecat_core::LakeCatError::InvalidArgument(format!(
                        "QueryGraph bootstrap manifest is missing table {}",
                        table.stable_id
                    ))
                })?;
            expected.verify(table)?;
        }
        for view in &self.views {
            let expected = view_artifacts.get(view.stable_id.as_str()).ok_or_else(|| {
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

        Ok(self.verification_summary(import_contract))
    }

    fn verification_summary(
        &self,
        import_contract: &QueryGraphImportCompatibility,
    ) -> QueryGraphBootstrapVerification {
        QueryGraphBootstrapVerification {
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
            bundle_hash: self.bundle_hash.clone(),
            graph_hash: self.manifest.graph_hash.clone(),
            open_lineage_hash: self.manifest.open_lineage_hash.clone(),
            querygraph_import_hash: import_contract.table_only_bundle_hash.clone(),
            standards: self.manifest.standards.clone(),
        }
    }

    fn computed_bundle_hash(&self) -> LakeCatResult<String> {
        querygraph_bundle_hash(
            &self.warehouse,
            &self.manifest,
            &self.tables,
            &self.views,
            &self.graph,
            &self.open_lineage,
        )
    }
}

pub fn querygraph_bundle_hash(
    warehouse: &WarehouseName,
    manifest: &QueryGraphBundleManifest,
    tables: &[QueryGraphTableProjection],
    views: &[QueryGraphViewProjection],
    graph: &QueryGraphCatalogGraph,
    open_lineage: &Value,
) -> LakeCatResult<String> {
    content_hash_json(&CanonicalQueryGraphBundle {
        graph: CanonicalQueryGraph::from(graph),
        manifest: CanonicalQueryGraphBundleManifest::from(manifest),
        open_lineage,
        tables: CanonicalJsonSlice::new(tables),
        views: CanonicalJsonSlice::new(views),
        warehouse: warehouse.as_str(),
    })
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

    /// Validate and install receipt evidence before the enclosing bundle hash
    /// is computed.
    ///
    /// Existing bundles should use
    /// [`QueryGraphBootstrap::with_view_receipt_evidence`] so their bundle hash
    /// is refreshed after this manifest mutation.
    pub fn attach_view_receipt_evidence(
        &mut self,
        views: &[QueryGraphViewProjection],
        evidence: Vec<QueryGraphViewReceiptEvidence>,
    ) -> LakeCatResult<()> {
        validate_view_receipt_evidence(views, &evidence)?;
        let evidence_hash = if evidence.is_empty() {
            None
        } else {
            Some(view_receipt_evidence_hash(&evidence)?)
        };
        let import_contract = self.querygraph_import.as_mut().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QueryGraph bootstrap manifest is missing querygraph-import compatibility contract"
                    .to_string(),
            )
        })?;
        import_contract.view_receipt_evidence = evidence;
        import_contract.view_receipt_evidence_hash = evidence_hash;
        Ok(())
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
    let mut evidence_by_id = HashMap::with_capacity(evidence.len());
    for record in evidence {
        if evidence_by_id
            .insert(record.stable_id.as_str(), record)
            .is_some()
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QueryGraph bootstrap import contract view receipt evidence must be duplicate-free by stable id: {}",
                record.stable_id
            )));
        }
    }
    for view in views {
        let Some(record) = evidence_by_id.get(view.stable_id.as_str()) else {
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
    content_hash_json(&CanonicalQueryGraph::from(graph))
}

pub fn table_only_querygraph_import_hash(
    warehouse: &WarehouseName,
    manifest: &QueryGraphBundleManifest,
    tables: &[QueryGraphTableProjection],
    graph: &QueryGraphCatalogGraph,
    open_lineage: &Value,
) -> LakeCatResult<String> {
    content_hash_json(&TableOnlyQueryGraphImport {
        graph: CanonicalQueryGraph::from(graph),
        manifest: TableOnlyQueryGraphImportManifest::from(manifest),
        open_lineage,
        tables: TableOnlyQueryGraphImportTables(tables),
        warehouse: warehouse.as_str(),
    })
}

/// Borrowed JSON projection whose declaration order matches the sorted object
/// keys emitted by the original `serde_json::Value` hash contract.
#[derive(Serialize)]
struct CanonicalQueryGraph<'a> {
    edges: CanonicalJsonSlice<'a, QueryGraphEdge>,
    nodes: CanonicalJsonSlice<'a, QueryGraphNode>,
}

impl<'a> From<&'a QueryGraphCatalogGraph> for CanonicalQueryGraph<'a> {
    fn from(graph: &'a QueryGraphCatalogGraph) -> Self {
        Self {
            edges: CanonicalJsonSlice::new(&graph.edges),
            nodes: CanonicalJsonSlice::new(&graph.nodes),
        }
    }
}

trait CanonicalJson {
    type Projection<'a>: Serialize
    where
        Self: 'a;

    fn canonical_json(&self) -> Self::Projection<'_>;
}

struct CanonicalJsonSlice<'a, T>(&'a [T]);

impl<'a, T> CanonicalJsonSlice<'a, T> {
    fn new(values: &'a [T]) -> Self {
        Self(values)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T> Serialize for CanonicalJsonSlice<'_, T>
where
    T: CanonicalJson,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for value in self.0 {
            sequence.serialize_element(&value.canonical_json())?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct CanonicalQueryGraphEdge<'a> {
    from: &'a str,
    label: &'a str,
    to: &'a str,
}

impl CanonicalJson for QueryGraphEdge {
    type Projection<'a> = CanonicalQueryGraphEdge<'a>;

    fn canonical_json(&self) -> Self::Projection<'_> {
        CanonicalQueryGraphEdge {
            from: &self.from,
            label: &self.label,
            to: &self.to,
        }
    }
}

#[derive(Serialize)]
struct CanonicalQueryGraphNode<'a> {
    id: &'a str,
    label: &'a str,
    properties: &'a Value,
}

impl CanonicalJson for QueryGraphNode {
    type Projection<'a> = CanonicalQueryGraphNode<'a>;

    fn canonical_json(&self) -> Self::Projection<'_> {
        CanonicalQueryGraphNode {
            id: &self.id,
            label: &self.label,
            properties: &self.properties,
        }
    }
}

#[derive(Serialize)]
struct CanonicalQueryGraphBundle<'a> {
    graph: CanonicalQueryGraph<'a>,
    manifest: CanonicalQueryGraphBundleManifest<'a>,
    #[serde(rename = "openLineage")]
    open_lineage: &'a Value,
    tables: CanonicalJsonSlice<'a, QueryGraphTableProjection>,
    views: CanonicalJsonSlice<'a, QueryGraphViewProjection>,
    warehouse: &'a str,
}

#[derive(Serialize)]
struct CanonicalQueryGraphBundleManifest<'a> {
    #[serde(rename = "graph-hash")]
    graph_hash: &'a str,
    #[serde(rename = "open-lineage-hash")]
    open_lineage_hash: &'a str,
    producer: &'a str,
    #[serde(rename = "querygraph-import", skip_serializing_if = "Option::is_none")]
    querygraph_import: Option<CanonicalQueryGraphImport<'a>>,
    #[serde(rename = "schema-version")]
    schema_version: &'a str,
    standards: &'a [String],
    #[serde(rename = "table-artifacts")]
    table_artifacts: CanonicalJsonSlice<'a, QueryGraphTableArtifactHashes>,
    #[serde(rename = "view-artifacts")]
    view_artifacts: CanonicalJsonSlice<'a, QueryGraphViewArtifactHashes>,
}

impl<'a> From<&'a QueryGraphBundleManifest> for CanonicalQueryGraphBundleManifest<'a> {
    fn from(manifest: &'a QueryGraphBundleManifest) -> Self {
        Self {
            graph_hash: &manifest.graph_hash,
            open_lineage_hash: &manifest.open_lineage_hash,
            producer: &manifest.producer,
            querygraph_import: manifest
                .querygraph_import
                .as_ref()
                .map(CanonicalQueryGraphImport::from),
            schema_version: &manifest.schema_version,
            standards: &manifest.standards,
            table_artifacts: CanonicalJsonSlice::new(&manifest.table_artifacts),
            view_artifacts: CanonicalJsonSlice::new(&manifest.view_artifacts),
        }
    }
}

#[derive(Serialize)]
struct CanonicalQueryGraphImport<'a> {
    #[serde(rename = "graph-hash")]
    graph_hash: &'a str,
    #[serde(rename = "schema-version")]
    schema_version: &'a str,
    #[serde(rename = "table-only-bundle-hash")]
    table_only_bundle_hash: &'a str,
    #[serde(rename = "view-count")]
    view_count: usize,
    #[serde(
        rename = "view-receipt-evidence",
        skip_serializing_if = "CanonicalJsonSlice::is_empty"
    )]
    view_receipt_evidence: CanonicalJsonSlice<'a, QueryGraphViewReceiptEvidence>,
    #[serde(
        rename = "view-receipt-evidence-hash",
        skip_serializing_if = "Option::is_none"
    )]
    view_receipt_evidence_hash: Option<&'a str>,
}

impl<'a> From<&'a QueryGraphImportCompatibility> for CanonicalQueryGraphImport<'a> {
    fn from(import: &'a QueryGraphImportCompatibility) -> Self {
        Self {
            graph_hash: &import.graph_hash,
            schema_version: &import.schema_version,
            table_only_bundle_hash: &import.table_only_bundle_hash,
            view_count: import.view_count,
            view_receipt_evidence: CanonicalJsonSlice::new(&import.view_receipt_evidence),
            view_receipt_evidence_hash: import.view_receipt_evidence_hash.as_deref(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct CanonicalQueryGraphViewReceiptEvidence<'a> {
    receipt_chain_hash: &'a str,
    receipt_hash: &'a str,
    stable_id: &'a str,
    view_version: u64,
}

impl CanonicalJson for QueryGraphViewReceiptEvidence {
    type Projection<'a> = CanonicalQueryGraphViewReceiptEvidence<'a>;

    fn canonical_json(&self) -> Self::Projection<'_> {
        CanonicalQueryGraphViewReceiptEvidence {
            receipt_chain_hash: &self.receipt_chain_hash,
            receipt_hash: &self.receipt_hash,
            stable_id: &self.stable_id,
            view_version: self.view_version,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct CanonicalQueryGraphTableArtifact<'a> {
    cdif_hash: &'a str,
    croissant_hash: &'a str,
    odrl_hash: &'a str,
    osi_hash: &'a str,
    policy_bindings_hash: &'a str,
    stable_id: &'a str,
}

impl CanonicalJson for QueryGraphTableArtifactHashes {
    type Projection<'a> = CanonicalQueryGraphTableArtifact<'a>;

    fn canonical_json(&self) -> Self::Projection<'_> {
        CanonicalQueryGraphTableArtifact {
            cdif_hash: &self.cdif_hash,
            croissant_hash: &self.croissant_hash,
            odrl_hash: &self.odrl_hash,
            osi_hash: &self.osi_hash,
            policy_bindings_hash: &self.policy_bindings_hash,
            stable_id: &self.stable_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct CanonicalQueryGraphViewArtifact<'a> {
    osi_hash: &'a str,
    stable_id: &'a str,
}

impl CanonicalJson for QueryGraphViewArtifactHashes {
    type Projection<'a> = CanonicalQueryGraphViewArtifact<'a>;

    fn canonical_json(&self) -> Self::Projection<'_> {
        CanonicalQueryGraphViewArtifact {
            osi_hash: &self.osi_hash,
            stable_id: &self.stable_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct CanonicalQueryGraphTable<'a> {
    cdif: &'a Value,
    croissant: &'a Value,
    format_version: Option<i64>,
    ident: CanonicalTableIdent<'a>,
    location: &'a str,
    metadata_location: Option<&'a str>,
    odrl: &'a Value,
    osi: &'a Value,
    policy_bindings: CanonicalJsonSlice<'a, QueryGraphPolicyBindingProjection>,
    stable_id: &'a str,
    version: u64,
}

impl CanonicalJson for QueryGraphTableProjection {
    type Projection<'a> = CanonicalQueryGraphTable<'a>;

    fn canonical_json(&self) -> Self::Projection<'_> {
        CanonicalQueryGraphTable {
            cdif: &self.cdif,
            croissant: &self.croissant,
            format_version: self.format_version,
            ident: CanonicalTableIdent::from(&self.ident),
            location: &self.location,
            metadata_location: self.metadata_location.as_deref(),
            odrl: &self.odrl,
            osi: &self.osi,
            policy_bindings: CanonicalJsonSlice::new(&self.policy_bindings),
            stable_id: &self.stable_id,
            version: self.version,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct CanonicalQueryGraphPolicyBinding<'a> {
    enforced: bool,
    namespace: Option<&'a [String]>,
    odrl: &'a Value,
    policy_id: &'a str,
    table: Option<&'a str>,
}

impl CanonicalJson for QueryGraphPolicyBindingProjection {
    type Projection<'a> = CanonicalQueryGraphPolicyBinding<'a>;

    fn canonical_json(&self) -> Self::Projection<'_> {
        CanonicalQueryGraphPolicyBinding {
            enforced: self.enforced,
            namespace: self.namespace.as_deref(),
            odrl: &self.odrl,
            policy_id: &self.policy_id,
            table: self.table.as_deref(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct CanonicalQueryGraphView<'a> {
    columns: &'a Value,
    dialect: &'a str,
    name: &'a str,
    namespace: &'a [String],
    osi: &'a Value,
    properties: &'a Value,
    schema_version: Option<u64>,
    sql: &'a str,
    stable_id: &'a str,
    view_version: u64,
    warehouse: &'a str,
}

impl CanonicalJson for QueryGraphViewProjection {
    type Projection<'a> = CanonicalQueryGraphView<'a>;

    fn canonical_json(&self) -> Self::Projection<'_> {
        CanonicalQueryGraphView {
            columns: &self.columns,
            dialect: &self.dialect,
            name: &self.name,
            namespace: &self.namespace,
            osi: &self.osi,
            properties: &self.properties,
            schema_version: self.schema_version,
            sql: &self.sql,
            stable_id: &self.stable_id,
            view_version: self.view_version,
            warehouse: &self.warehouse,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TableOnlyQueryGraphImport<'a> {
    graph: CanonicalQueryGraph<'a>,
    manifest: TableOnlyQueryGraphImportManifest<'a>,
    #[serde(rename = "openLineage")]
    open_lineage: &'a Value,
    tables: TableOnlyQueryGraphImportTables<'a>,
    warehouse: &'a str,
}

#[derive(Serialize)]
struct TableOnlyQueryGraphImportManifest<'a> {
    #[serde(rename = "open-lineage-hash")]
    open_lineage_hash: &'a str,
    producer: &'a str,
    #[serde(rename = "schema-version")]
    schema_version: &'a str,
    standards: &'a [String],
    #[serde(rename = "table-artifacts")]
    table_artifacts: TableOnlyQueryGraphImportTableArtifacts<'a>,
}

impl<'a> From<&'a QueryGraphBundleManifest> for TableOnlyQueryGraphImportManifest<'a> {
    fn from(manifest: &'a QueryGraphBundleManifest) -> Self {
        Self {
            open_lineage_hash: &manifest.open_lineage_hash,
            producer: &manifest.producer,
            schema_version: &manifest.schema_version,
            standards: &manifest.standards,
            table_artifacts: TableOnlyQueryGraphImportTableArtifacts(&manifest.table_artifacts),
        }
    }
}

struct TableOnlyQueryGraphImportTableArtifacts<'a>(&'a [QueryGraphTableArtifactHashes]);

impl Serialize for TableOnlyQueryGraphImportTableArtifacts<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for artifact in self.0 {
            sequence.serialize_element(&TableOnlyQueryGraphImportTableArtifact {
                cdif_hash: &artifact.cdif_hash,
                croissant_hash: &artifact.croissant_hash,
                odrl_hash: &artifact.odrl_hash,
                osi_hash: &artifact.osi_hash,
                stable_id: &artifact.stable_id,
            })?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct TableOnlyQueryGraphImportTableArtifact<'a> {
    cdif_hash: &'a str,
    croissant_hash: &'a str,
    odrl_hash: &'a str,
    osi_hash: &'a str,
    stable_id: &'a str,
}

struct TableOnlyQueryGraphImportTables<'a>(&'a [QueryGraphTableProjection]);

impl Serialize for TableOnlyQueryGraphImportTables<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for table in self.0 {
            sequence.serialize_element(&TableOnlyQueryGraphImportTable::from(table))?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct TableOnlyQueryGraphImportTable<'a> {
    cdif: &'a Value,
    croissant: &'a Value,
    format_version: Option<i64>,
    ident: CanonicalTableIdent<'a>,
    location: &'a str,
    metadata_location: Option<&'a str>,
    odrl: &'a Value,
    osi: &'a Value,
    stable_id: &'a str,
    version: u64,
}

impl<'a> From<&'a QueryGraphTableProjection> for TableOnlyQueryGraphImportTable<'a> {
    fn from(table: &'a QueryGraphTableProjection) -> Self {
        Self {
            cdif: &table.cdif,
            croissant: &table.croissant,
            format_version: table.format_version,
            ident: CanonicalTableIdent::from(&table.ident),
            location: &table.location,
            metadata_location: table.metadata_location.as_deref(),
            odrl: &table.odrl,
            osi: &table.osi,
            stable_id: &table.stable_id,
            version: table.version,
        }
    }
}

#[derive(Serialize)]
struct CanonicalTableIdent<'a> {
    name: &'a TableName,
    namespace: &'a lakecat_core::Namespace,
    warehouse: &'a WarehouseName,
}

impl<'a> From<&'a TableIdent> for CanonicalTableIdent<'a> {
    fn from(ident: &'a TableIdent) -> Self {
        Self {
            name: &ident.name,
            namespace: &ident.namespace,
            warehouse: &ident.warehouse,
        }
    }
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
    let mut seen = HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be duplicate-free by stable id: {value}"
            )));
        }
    }
    Ok(())
}

fn index_duplicate_free_stable_ids<'a, T>(
    label: &str,
    values: impl IntoIterator<Item = (&'a str, &'a T)>,
) -> LakeCatResult<HashMap<&'a str, &'a T>> {
    let values = values.into_iter();
    let mut index = HashMap::with_capacity(values.size_hint().0);
    for (stable_id, value) in values {
        if index.insert(stable_id, value).is_some() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be duplicate-free by stable id: {stable_id}"
            )));
        }
    }
    Ok(index)
}
