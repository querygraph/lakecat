use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use lakecat_api::{
    CatalogConfigResponse, CommitTableRequest, CommitTableResponse, CreateNamespaceRequest,
    CreateTableRequest, FetchScanTasksRequest, FetchScanTasksResponse, LineageDrainEventSummary,
    LineageDrainResponse, ListNamespacesResponse, ListPolicyBindingsResponse, ListProjectsResponse,
    ListServersResponse, ListStorageProfilesResponse, ListTableCommitRecordsResponse,
    ListViewVersionReceiptChainsResponse, ListViewVersionReceiptsResponse, ListWarehousesResponse,
    LoadCredentialsResponse, LoadTableResponse, NamespaceResponse, PlanTableScanRequest,
    PlanTableScanResponse, PolicyBindingResponse, ProjectResponse, ServerResponse,
    StorageProfileResponse, TableIdentifier, UpsertPolicyBindingRequest, UpsertProjectRequest,
    UpsertServerRequest, UpsertStorageProfileRequest, UpsertViewRequest, UpsertWarehouseRequest,
    ViewResponse, WarehouseResponse,
};
use lakecat_core::{content_hash_bytes, content_hash_json};
use lakecat_querygraph::{QueryGraphBootstrap, QueryGraphBootstrapVerification};
use sail_iceberg::spec::{
    DataContentType, DataFile, DataFileFormat, FormatVersion, ManifestContentType, ManifestFile,
    ManifestListWriter, ManifestMetadata, ManifestWriterBuilder, TableMetadata,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;

const QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON: &str =
    "fine-grained read restriction requires Sail-planned reads";
const QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON: &str =
    "trusted human principal may use audited raw credential vending";

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("lakecat-cli: {err}");
        std::process::exit(1);
    }
}

async fn run() -> lakecat_core::LakeCatResult<()> {
    let command = Command::parse(std::env::args().skip(1))?;
    match command {
        Command::BootstrapExport {
            catalog,
            output,
            principal,
        } => bootstrap_export(catalog, output, principal).await,
        Command::Config { catalog, principal } => config(catalog, principal).await,
        Command::LineageDrain { catalog, principal } => lineage_drain(catalog, principal).await,
        Command::QglakeVerifyReplay {
            bundle,
            drain,
            principal,
            json,
        } => qglake_verify_replay(bundle, drain, principal, json),
        Command::QglakeVerifyHandoff { summary, json } => qglake_verify_handoff(summary, json),
        Command::PolicyList {
            catalog,
            warehouse,
            principal,
        } => print_json(
            &get_json::<ListPolicyBindingsResponse>(
                &catalog,
                &format!("/management/v1/warehouses/{warehouse}/policies"),
                principal.as_deref(),
                "policy list",
            )
            .await?,
        ),
        Command::PolicyUpsert {
            catalog,
            warehouse,
            policy,
            namespace,
            table,
            enforced,
            odrl,
            principal,
        } => print_json(
            &put_json::<_, PolicyBindingResponse>(
                &catalog,
                &format!("/management/v1/warehouses/{warehouse}/policies/{policy}"),
                principal.as_deref(),
                "policy upsert",
                &UpsertPolicyBindingRequest {
                    namespace,
                    table,
                    enforced,
                    odrl,
                },
            )
            .await?,
        ),
        Command::StorageProfileList {
            catalog,
            warehouse,
            principal,
        } => print_json(
            &get_json::<ListStorageProfilesResponse>(
                &catalog,
                &format!("/management/v1/warehouses/{warehouse}/storage-profiles"),
                principal.as_deref(),
                "storage profile list",
            )
            .await?,
        ),
        Command::StorageProfileUpsert {
            catalog,
            warehouse,
            profile,
            location_prefix,
            provider,
            issuance_mode,
            secret_ref,
            public_config,
            principal,
        } => print_json(
            &put_json::<_, StorageProfileResponse>(
                &catalog,
                &format!("/management/v1/warehouses/{warehouse}/storage-profiles/{profile}"),
                principal.as_deref(),
                "storage profile upsert",
                &UpsertStorageProfileRequest {
                    location_prefix,
                    provider,
                    issuance_mode,
                    secret_ref,
                    public_config,
                },
            )
            .await?,
        ),
        Command::QglakeFixture {
            catalog,
            warehouse,
            namespace,
            table,
            location,
            metadata_location,
            output,
            drain_output,
            principal,
        } => {
            qglake_fixture(
                catalog,
                warehouse,
                namespace,
                table,
                location,
                metadata_location,
                output,
                drain_output,
                principal,
            )
            .await
        }
    }
}

async fn bootstrap_export(
    catalog: String,
    output: PathBuf,
    principal: Option<String>,
) -> lakecat_core::LakeCatResult<()> {
    let (bundle, verification) = fetch_bootstrap_bundle(&catalog, principal.as_deref()).await?;
    write_bootstrap_bundle(&output, &bundle, &verification)
}

async fn fetch_bootstrap_bundle(
    catalog: &str,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<(QueryGraphBootstrap, QueryGraphBootstrapVerification)> {
    fetch_bootstrap_bundle_with_identity(catalog, principal, RequestIdentityMode::Principal).await
}

async fn fetch_bootstrap_bundle_with_identity(
    catalog: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<(QueryGraphBootstrap, QueryGraphBootstrapVerification)> {
    let endpoint = format!("{}/querygraph/v1/bootstrap", catalog.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut request = client.get(endpoint);
    request = identity_mode.apply(request, principal);
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request bootstrap bundle: {err}"))
    })?;
    let status = response.status();
    let body = response.bytes().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to read bootstrap response: {err}"))
    })?;
    if !status.is_success() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "bootstrap export failed with HTTP {status}: {}",
            String::from_utf8_lossy(&body)
        )));
    }
    let bundle: QueryGraphBootstrap = serde_json::from_slice(&body).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "LakeCat bootstrap response is not a QueryGraph bundle: {err}"
        ))
    })?;
    let verification = bundle.verify_manifest()?;
    Ok((bundle, verification))
}

fn write_bootstrap_bundle(
    output: &PathBuf,
    bundle: &QueryGraphBootstrap,
    verification: &QueryGraphBootstrapVerification,
) -> lakecat_core::LakeCatResult<()> {
    let pretty = serde_json::to_vec_pretty(&bundle).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to encode bootstrap bundle: {err}"))
    })?;
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to create output directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    fs::write(&output, pretty).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write bootstrap bundle {}: {err}",
            output.display()
        ))
    })?;
    println!(
        "wrote {} table(s) for warehouse {} to {}",
        verification.table_count,
        verification.warehouse,
        output.display()
    );
    println!("bundle {}", verification.bundle_hash);
    Ok(())
}

fn write_json_file<T: Serialize>(
    output: &PathBuf,
    value: &T,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let pretty = serde_json::to_vec_pretty(value).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to encode {label}: {err}"))
    })?;
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to create output directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    fs::write(output, pretty).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write {label} {}: {err}",
            output.display()
        ))
    })
}

async fn config(catalog: String, principal: Option<String>) -> lakecat_core::LakeCatResult<()> {
    let config = get_json::<CatalogConfigResponse>(
        &catalog,
        "/catalog/v1/config",
        principal.as_deref(),
        "catalog config",
    )
    .await?;
    print_json(&config)
}

async fn lineage_drain(
    catalog: String,
    principal: Option<String>,
) -> lakecat_core::LakeCatResult<()> {
    let response = drain_lineage_outbox(&catalog, principal.as_deref()).await?;
    println!("delivered {}", response.delivered);
    println!("graph events {}", response.graph_events);
    println!("lineage events {}", response.lineage_events);
    if let Some(hash) = response.authorization_receipt_hash.as_deref() {
        println!("authorization receipt {hash}");
    }
    if let Some(subject) = response.principal_subject.as_deref() {
        let kind = response.principal_kind.as_deref().unwrap_or("unknown");
        println!("principal {subject} ({kind})");
    }
    if let Some(state) = response.request_identity_state.as_deref() {
        println!("request identity {state}");
    }
    if !response.event_types.is_empty() {
        println!("event types {}", response.event_types.join(","));
    }
    Ok(())
}

fn qglake_verify_replay(
    bundle_path: PathBuf,
    drain_path: PathBuf,
    principal: Option<String>,
    json_output: bool,
) -> lakecat_core::LakeCatResult<()> {
    let bundle =
        read_typed_json_file::<QueryGraphBootstrap>(&bundle_path, "QueryGraph bootstrap bundle")?;
    let drain =
        read_typed_json_file::<LineageDrainResponse>(&drain_path, "lineage drain response")?;
    let verification = verify_qglake_replay_artifacts(&bundle, &drain, principal.as_deref())?;
    let scan_replay = qglake_scan_replay_line(&drain);
    let management_replay = qglake_management_replay_line(&drain);
    let credential_replay = qglake_credential_replay_line(&drain, principal.as_deref());
    let table_commit_history_replay = qglake_table_commit_history_replay_line(&drain);
    let replay_evidence = qglake_replay_evidence_json(&drain, principal.as_deref(), &verification);
    if json_output {
        print_json(&qglake_replay_verification_json(
            &verification,
            scan_replay,
            management_replay,
            credential_replay,
            table_commit_history_replay,
            replay_evidence,
        ))?;
        return Ok(());
    }
    println!("verified qglake replay evidence");
    println!("bundle {}", verification.bundle_hash);
    println!("querygraph import {}", verification.querygraph_import_hash);
    println!("tables {}", verification.table_count);
    println!("views {}", verification.view_count);
    if let Some(line) = scan_replay {
        println!("{line}");
    }
    if let Some(line) = management_replay {
        println!("{line}");
    }
    if let Some(line) = credential_replay {
        println!("{line}");
    }
    if let Some(line) = table_commit_history_replay {
        println!("{line}");
    }
    Ok(())
}

fn qglake_verify_handoff(
    summary_path: PathBuf,
    json_output: bool,
) -> lakecat_core::LakeCatResult<()> {
    let summary = read_json_file(&summary_path)?;
    let mut verification = verify_qglake_handoff_summary_value(&summary)?;
    let artifact_files = verify_qglake_handoff_artifact_files(&summary_path, &summary)?;
    let captured_output_semantics =
        verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)?;
    let bundle_artifact_semantics =
        verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)?;
    let querygraph_import_plan_semantics =
        verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)?;
    require_qglake_import_plan_graph_counts_match_bundle(
        &bundle_artifact_semantics,
        &querygraph_import_plan_semantics,
    )?;
    let lineage_drain_artifact_semantics =
        verify_qglake_handoff_lineage_drain_artifact_semantics(&summary_path, &summary)?;
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert("artifactFiles".to_string(), artifact_files);
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert(
            "capturedOutputSemantics".to_string(),
            captured_output_semantics,
        );
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert(
            "bundleArtifactSemantics".to_string(),
            bundle_artifact_semantics,
        );
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert(
            "querygraphImportPlanSemantics".to_string(),
            querygraph_import_plan_semantics,
        );
    verification
        .as_object_mut()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::Internal(
                "handoff verification must be an object".to_string(),
            )
        })?
        .insert(
            "lineageDrainArtifactSemantics".to_string(),
            lineage_drain_artifact_semantics,
        );
    if json_output {
        print_json(&verification)?;
        return Ok(());
    }
    let verification = verification.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal("handoff verification must be an object".to_string())
    })?;
    println!("verified qglake handoff summary");
    println!(
        "bundle {}",
        required_str(
            required_object(
                verification,
                "queryGraphBootstrapProof",
                "handoff verification"
            )?,
            "bundleHash",
            "handoff verification queryGraphBootstrapProof"
        )?
    );
    println!(
        "querygraph import {}",
        required_str(
            required_object(
                verification,
                "queryGraphBootstrapProof",
                "handoff verification"
            )?,
            "queryGraphImportHash",
            "handoff verification queryGraphBootstrapProof"
        )?
    );
    println!(
        "tables {}",
        required_u64(verification, "tableCount", "handoff verification")?
    );
    println!(
        "views {}",
        required_u64(verification, "viewCount", "handoff verification")?
    );
    Ok(())
}

fn verify_qglake_handoff_artifact_files(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));
    let bundle = verify_qglake_handoff_artifact_file(artifacts, "bundle", base_dir)?;
    let lineage_drain = verify_qglake_handoff_artifact_file(artifacts, "lineageDrain", base_dir)?;
    let querygraph_import_plan =
        verify_qglake_handoff_artifact_file(artifacts, "querygraphImportPlan", base_dir)?;
    let captured_outputs =
        verify_qglake_handoff_captured_outputs(artifacts, "capturedOutputs", base_dir)?;
    let path_aliases = verify_qglake_handoff_artifact_path_aliases(artifacts, summary, base_dir)?;
    Ok(json!({
        "bundle": bundle,
        "lineageDrain": lineage_drain,
        "querygraphImportPlan": querygraph_import_plan,
        "capturedOutputs": captured_outputs,
        "pathAliases": path_aliases,
        "serviceLogHash": required_value(artifacts, "serviceLogHash", "handoff summary artifacts")?,
    }))
}

fn verify_qglake_handoff_captured_outputs(
    artifacts: &serde_json::Map<String, Value>,
    field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let outputs = required_object(artifacts, field, "handoff summary artifacts")?;
    Ok(json!({
        "lakecatReplay": verify_qglake_handoff_artifact_file(outputs, "lakecatReplay", base_dir)?,
        "querygraphVerify": verify_qglake_handoff_artifact_file(outputs, "querygraphVerify", base_dir)?,
        "querygraphImport": verify_qglake_handoff_artifact_file(outputs, "querygraphImport", base_dir)?,
    }))
}

fn verify_qglake_handoff_artifact_path_aliases(
    artifacts: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let outputs = required_object(artifacts, "capturedOutputs", "handoff summary artifacts")?;
    let lakecat_replay = verify_qglake_handoff_path_alias(
        artifacts,
        outputs,
        "lakecatReplayOutput",
        "lakecatReplay",
        base_dir,
    )?;
    let querygraph_verify = verify_qglake_handoff_path_alias(
        artifacts,
        outputs,
        "querygraphVerifyOutput",
        "querygraphVerify",
        base_dir,
    )?;
    let querygraph_import = verify_qglake_handoff_path_alias(
        artifacts,
        outputs,
        "querygraphImportOutput",
        "querygraphImport",
        base_dir,
    )?;
    let handoff_verify_output =
        required_resolved_artifact_path(artifacts, "lakecatHandoffVerifyOutput", base_dir)?;
    let handoff_verify_output_hash =
        verify_qglake_handoff_verify_output_artifact(artifacts, summary, &handoff_verify_output)?;
    let service_log = verify_qglake_handoff_service_log(artifacts, base_dir)?;
    Ok(json!({
        "lakecatReplayOutput": lakecat_replay.display().to_string(),
        "querygraphVerifyOutput": querygraph_verify.display().to_string(),
        "querygraphImportOutput": querygraph_import.display().to_string(),
        "lakecatHandoffVerifyOutput": handoff_verify_output.display().to_string(),
        "lakecatHandoffVerifyOutputHash": handoff_verify_output_hash,
        "serviceLog": service_log.display().to_string(),
    }))
}

fn verify_qglake_handoff_verify_output_artifact(
    artifacts: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
    output_path: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let Some(expected_sha256) = artifacts.get("lakecatHandoffVerifyOutputHash") else {
        return Ok(Value::Null);
    };
    if expected_sha256.is_null() {
        return Ok(Value::Null);
    }
    let Some(expected_sha256) = expected_sha256
        .as_str()
        .filter(|value| is_sha256_hash(value))
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary artifacts.lakecatHandoffVerifyOutputHash must be null or a sha256 hash"
                .to_string(),
        ));
    };
    let bytes = fs::read(output_path).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read handoff artifact lakecatHandoffVerifyOutput at {}: {err}",
            output_path.display()
        ))
    })?;
    let actual_sha256 = content_hash_bytes(&bytes);
    if actual_sha256 != expected_sha256 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact lakecatHandoffVerifyOutput hash mismatch: expected={expected_sha256} actual={actual_sha256}"
        )));
    }
    let output: Value = serde_json::from_slice(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact lakecatHandoffVerifyOutput is not JSON: {err}"
        ))
    })?;
    let output = output.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff artifact lakecatHandoffVerifyOutput must be a JSON object".to_string(),
        )
    })?;
    require_string_eq(
        output,
        "schemaVersion",
        "lakecat.qglake.handoff-verification.v1",
        "lakecatHandoffVerifyOutput",
    )?;
    require_string_eq(output, "status", "verified", "lakecatHandoffVerifyOutput")?;
    for field in ["principal", "catalogUrl", "warehouse", "namespace", "table"] {
        require_value_match(
            output,
            field,
            required_value(summary, field, "handoff summary")?,
            "lakecatHandoffVerifyOutput",
        )?;
    }
    require_qglake_handoff_verify_output_matches_summary(output, summary)?;
    Ok(Value::String(actual_sha256))
}

fn require_qglake_handoff_verify_output_matches_summary(
    output: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    for field in [
        "tableCount",
        "viewCount",
        "verifiedTables",
        "verifiedViews",
        "standards",
    ] {
        require_value_match(
            output,
            field,
            required_value(querygraph, field, "querygraphVerification")?,
            "lakecatHandoffVerifyOutput",
        )?;
    }
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    for field in ["requestIdentityProof", "queryGraphBootstrapProof"] {
        require_value_match(
            output,
            field,
            required_value(lakecat, field, "lakecatReplayVerification")?,
            "lakecatHandoffVerifyOutput",
        )?;
    }
    require_qglake_handoff_verify_output_artifact_hashes_match_summary(output, summary)?;
    require_qglake_handoff_verify_output_semantic_sections_match_summary(output, summary)?;
    Ok(())
}

fn require_qglake_handoff_verify_output_semantic_sections_match_summary(
    output: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let import = required_object(summary, "querygraphImportVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;

    let captured = required_object(
        output,
        "capturedOutputSemantics",
        "lakecatHandoffVerifyOutput",
    )?;
    let captured_lakecat = required_object(
        captured,
        "lakecatReplay",
        "lakecatHandoffVerifyOutput.capturedOutputSemantics",
    )?;
    for field in ["requestIdentityProof", "queryGraphBootstrapProof"] {
        require_value_match(
            captured_lakecat,
            field,
            required_value(lakecat, field, "lakecatReplayVerification")?,
            "lakecatHandoffVerifyOutput.capturedOutputSemantics.lakecatReplay",
        )?;
    }
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        required_object(
            captured,
            "querygraphVerify",
            "lakecatHandoffVerifyOutput.capturedOutputSemantics",
        )?,
        querygraph,
        "lakecatHandoffVerifyOutput.capturedOutputSemantics.querygraphVerify",
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        required_object(
            captured,
            "querygraphImport",
            "lakecatHandoffVerifyOutput.capturedOutputSemantics",
        )?,
        import,
        "lakecatHandoffVerifyOutput.capturedOutputSemantics.querygraphImport",
    )?;

    let bundle = required_object(
        output,
        "bundleArtifactSemantics",
        "lakecatHandoffVerifyOutput",
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        bundle,
        querygraph,
        "lakecatHandoffVerifyOutput.bundleArtifactSemantics",
    )?;
    let import_plan = required_object(
        output,
        "querygraphImportPlanSemantics",
        "lakecatHandoffVerifyOutput",
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        import_plan,
        import,
        "lakecatHandoffVerifyOutput.querygraphImportPlanSemantics",
    )?;
    for field in ["graphNodes", "graphEdges"] {
        require_value_match(
            import_plan,
            field,
            required_value(
                bundle,
                field,
                "lakecatHandoffVerifyOutput.bundleArtifactSemantics",
            )?,
            "lakecatHandoffVerifyOutput.querygraphImportPlanSemantics",
        )?;
    }

    let lineage_drain = required_object(
        output,
        "lineageDrainArtifactSemantics",
        "lakecatHandoffVerifyOutput",
    )?;
    require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
        lineage_drain,
        querygraph,
        "lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics",
    )?;
    require_qglake_handoff_verify_output_lineage_drain_identity_match_summary(
        lineage_drain,
        required_object(lakecat, "requestIdentityProof", "lakecatReplayVerification")?,
    )?;
    Ok(())
}

fn require_qglake_handoff_verify_output_lineage_drain_identity_match_summary(
    semantics: &serde_json::Map<String, Value>,
    request_identity: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    for field in [
        "principalSubject",
        "principalKind",
        "authorizationReceiptHash",
        "requestIdentitySource",
        "requestIdentityState",
        "typedidEnvelopeHash",
        "typedidProofHash",
    ] {
        require_value_match(
            semantics,
            field,
            required_value(
                request_identity,
                field,
                "lakecatReplayVerification.requestIdentityProof",
            )?,
            "lakecatHandoffVerifyOutput.lineageDrainArtifactSemantics",
        )?;
    }
    Ok(())
}

fn require_qglake_handoff_verify_output_querygraph_semantics_match_summary(
    semantics: &serde_json::Map<String, Value>,
    expected: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    for field in [
        "tableCount",
        "viewCount",
        "verifiedTables",
        "verifiedViews",
        "bundleHash",
        "graphHash",
        "openLineageHash",
        "standards",
    ] {
        require_value_match(
            semantics,
            field,
            required_value(expected, field, label)?,
            label,
        )?;
    }
    require_value_match(
        semantics,
        "queryGraphImportHash",
        required_value(expected, "querygraphImportHash", label)?,
        label,
    )?;
    Ok(())
}

fn require_qglake_handoff_verify_output_artifact_hashes_match_summary(
    output: &serde_json::Map<String, Value>,
    summary: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let output_artifacts = required_object(output, "artifactFiles", "lakecatHandoffVerifyOutput")?;
    let summary_artifacts = required_object(summary, "artifacts", "handoff summary")?;
    for field in ["bundle", "lineageDrain", "querygraphImportPlan"] {
        let output_artifact = required_object(
            output_artifacts,
            field,
            "lakecatHandoffVerifyOutput.artifactFiles",
        )?;
        let summary_artifact =
            required_object(summary_artifacts, field, "handoff summary artifacts")?;
        require_value_match(
            output_artifact,
            "sha256",
            required_value(summary_artifact, "sha256", "handoff summary artifacts")?,
            "lakecatHandoffVerifyOutput.artifactFiles",
        )?;
    }
    let output_captures = required_object(
        output_artifacts,
        "capturedOutputs",
        "lakecatHandoffVerifyOutput.artifactFiles",
    )?;
    let summary_captures = required_object(
        summary_artifacts,
        "capturedOutputs",
        "handoff summary artifacts",
    )?;
    for field in ["lakecatReplay", "querygraphVerify", "querygraphImport"] {
        let output_capture = required_object(
            output_captures,
            field,
            "lakecatHandoffVerifyOutput.artifactFiles.capturedOutputs",
        )?;
        let summary_capture = required_object(
            summary_captures,
            field,
            "handoff summary artifacts.capturedOutputs",
        )?;
        require_value_match(
            output_capture,
            "sha256",
            required_value(
                summary_capture,
                "sha256",
                "handoff summary artifacts.capturedOutputs",
            )?,
            "lakecatHandoffVerifyOutput.artifactFiles.capturedOutputs",
        )?;
    }
    require_value_match(
        output_artifacts,
        "serviceLogHash",
        required_value(
            summary_artifacts,
            "serviceLogHash",
            "handoff summary artifacts",
        )?,
        "lakecatHandoffVerifyOutput.artifactFiles",
    )?;
    Ok(())
}

fn verify_qglake_handoff_service_log(
    artifacts: &serde_json::Map<String, Value>,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<PathBuf> {
    let service_log = required_resolved_artifact_path(artifacts, "serviceLog", base_dir)?;
    let expected_sha256 =
        require_hash_str(artifacts, "serviceLogHash", "handoff summary artifacts")?;
    let bytes = fs::read(&service_log).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read handoff artifact serviceLog at {}: {err}",
            service_log.display()
        ))
    })?;
    let actual_sha256 = content_hash_bytes(&bytes);
    if actual_sha256 != expected_sha256 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact serviceLog hash mismatch: expected={expected_sha256} actual={actual_sha256}"
        )));
    }
    Ok(service_log)
}

fn verify_qglake_handoff_path_alias(
    artifacts: &serde_json::Map<String, Value>,
    outputs: &serde_json::Map<String, Value>,
    alias_field: &str,
    captured_field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<PathBuf> {
    let alias_path = required_resolved_artifact_path(artifacts, alias_field, base_dir)?;
    let captured = required_object(outputs, captured_field, "handoff captured outputs")?;
    let captured_path = required_resolved_artifact_path(captured, "path", base_dir)?;
    if alias_path != captured_path {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact path alias {alias_field} does not match capturedOutputs.{captured_field}.path: alias={} captured={}",
            alias_path.display(),
            captured_path.display()
        )));
    }
    Ok(alias_path)
}

fn required_resolved_artifact_path(
    object: &serde_json::Map<String, Value>,
    field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<PathBuf> {
    let path = required_str(object, field, "handoff summary artifacts")?;
    if path.trim().is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact path {field} must be non-empty"
        )));
    }
    let path = PathBuf::from(path);
    Ok(if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    })
}

fn verify_qglake_handoff_artifact_file(
    artifacts: &serde_json::Map<String, Value>,
    field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let artifact = required_object(artifacts, field, "handoff summary artifacts")?;
    let expected_sha256 = require_hash_str(artifact, "sha256", field)?;
    let resolved_path = required_resolved_artifact_path(artifact, "path", base_dir)?;
    let bytes = fs::read(&resolved_path).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read handoff artifact {} at {}: {err}",
            field,
            resolved_path.display()
        ))
    })?;
    let actual_sha256 = content_hash_bytes(&bytes);
    if actual_sha256 != expected_sha256 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff artifact {field} hash mismatch: expected={expected_sha256} actual={actual_sha256}"
        )));
    }
    Ok(json!({
        "path": resolved_path.display().to_string(),
        "sha256": actual_sha256,
    }))
}

fn verify_qglake_handoff_captured_output_semantics(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let warehouse = required_str(summary, "warehouse", "handoff summary")?;
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let import = required_object(summary, "querygraphImportVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let outputs = required_object(artifacts, "capturedOutputs", "handoff summary artifacts")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));

    let lakecat_replay = read_qglake_handoff_artifact_json(outputs, "lakecatReplay", base_dir)?;
    let querygraph_verify =
        read_qglake_handoff_artifact_json(outputs, "querygraphVerify", base_dir)?;
    let querygraph_import =
        read_qglake_handoff_artifact_json(outputs, "querygraphImport", base_dir)?;

    let lakecat_replay = lakecat_replay.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "captured LakeCat replay output must be a JSON object".to_string(),
        )
    })?;
    let querygraph_verify = querygraph_verify.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "captured QueryGraph verify output must be a JSON object".to_string(),
        )
    })?;
    let querygraph_import = querygraph_import.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "captured QueryGraph import output must be a JSON object".to_string(),
        )
    })?;

    verify_lakecat_replay_capture_matches_summary(lakecat_replay, lakecat, querygraph)?;
    let table_scope = HandoffTableScope::from_summary(summary, warehouse)?;
    let view_scope = HandoffViewScope::from_lakecat(lakecat)?;
    verify_querygraph_capture_matches_summary(
        querygraph_verify,
        querygraph,
        &table_scope,
        &view_scope,
        "captured QueryGraph verify output",
    )?;
    require_querygraph_import_matches_verify(import, querygraph)?;
    verify_querygraph_capture_matches_summary(
        querygraph_import,
        import,
        &table_scope,
        &view_scope,
        "captured QueryGraph import output",
    )?;
    let request_identity = Value::Object(lakecat_replay_request_identity(lakecat_replay)?.clone());
    let querygraph_bootstrap =
        Value::Object(lakecat_replay_querygraph_bootstrap(lakecat_replay)?.clone());
    let governed_scan = Value::Object(lakecat_replay_scan(lakecat_replay)?.clone());
    let table_commit_history =
        Value::Object(lakecat_replay_table_commit_history(lakecat_replay)?.clone());
    let view_receipt_chain = Value::Object(lakecat_replay_views(lakecat_replay)?.clone());
    let management = Value::Object(lakecat_replay_management(lakecat_replay)?.clone());
    let storage_profile_upsert =
        Value::Object(lakecat_replay_storage_profile_upsert(lakecat_replay)?.clone());
    let credential_vending = Value::Object(lakecat_replay_credentials(lakecat_replay)?.clone());

    Ok(json!({
        "lakecatReplay": {
            "schemaVersion": required_str(lakecat_replay, "schema-version", "captured LakeCat replay output")?,
            "status": required_str(lakecat_replay, "status", "captured LakeCat replay output")?,
            "tableCount": required_u64(lakecat_replay, "table-count", "captured LakeCat replay output")?,
            "viewCount": required_u64(lakecat_replay, "view-count", "captured LakeCat replay output")?,
            "bundleHash": required_str(lakecat_replay, "bundle-hash", "captured LakeCat replay output")?,
            "graphHash": required_str(lakecat_replay, "graph-hash", "captured LakeCat replay output")?,
            "openLineageHash": required_str(lakecat_replay, "open-lineage-hash", "captured LakeCat replay output")?,
            "queryGraphImportHash": required_str(lakecat_replay, "querygraph-import-hash", "captured LakeCat replay output")?,
            "standards": required_value(lakecat_replay, "standards", "captured LakeCat replay output")?,
            "requestIdentityProof": request_identity,
            "queryGraphBootstrapProof": querygraph_bootstrap,
            "governedScanProof": governed_scan,
            "tableCommitHistoryProof": table_commit_history,
            "viewReceiptChainProof": view_receipt_chain,
            "managementProof": management,
            "storageProfileUpsertProof": storage_profile_upsert,
            "credentialVendingProof": credential_vending,
        },
        "querygraphVerify": querygraph_capture_semantics_json(querygraph_verify, "captured QueryGraph verify output")?,
        "querygraphImport": querygraph_capture_semantics_json(querygraph_import, "captured QueryGraph import output")?,
    }))
}

fn verify_qglake_handoff_bundle_artifact_semantics(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let warehouse = required_str(summary, "warehouse", "handoff summary")?;
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    let bootstrap = required_object(
        lakecat,
        "queryGraphBootstrapProof",
        "lakecatReplayVerification",
    )?;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));
    let bundle_value = read_qglake_handoff_artifact_json(artifacts, "bundle", base_dir)?;
    let bundle: QueryGraphBootstrap = serde_json::from_value(bundle_value).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff bundle artifact is not a QueryGraph bootstrap bundle: {err}"
        ))
    })?;
    let table_scope = HandoffTableScope::from_summary(summary, warehouse)?;
    let namespace = table_scope
        .namespace
        .split('.')
        .map(str::to_string)
        .collect::<Vec<_>>();
    verify_qglake_bootstrap_bundle(&bundle, &namespace, &table_scope.table)?;
    let verification = bundle.verify_manifest()?;
    require_string_match(
        querygraph,
        "bundleHash",
        verification.bundle_hash.as_str(),
        "querygraphVerification",
    )?;
    require_string_match(
        querygraph,
        "graphHash",
        verification.graph_hash.as_str(),
        "querygraphVerification",
    )?;
    require_string_match(
        querygraph,
        "openLineageHash",
        verification.open_lineage_hash.as_str(),
        "querygraphVerification",
    )?;
    require_string_match(
        querygraph,
        "querygraphImportHash",
        verification.querygraph_import_hash.as_str(),
        "querygraphVerification",
    )?;
    require_u64_match(
        querygraph,
        "tableCount",
        verification.table_count as u64,
        "querygraphVerification",
    )?;
    require_u64_match(
        querygraph,
        "viewCount",
        verification.view_count as u64,
        "querygraphVerification",
    )?;
    require_value_match(
        querygraph,
        "verifiedTables",
        &json!(verification.verified_tables),
        "querygraphVerification",
    )?;
    require_value_match(
        querygraph,
        "verifiedViews",
        &json!(verification.verified_views),
        "querygraphVerification",
    )?;
    require_value_match(
        querygraph,
        "standards",
        &json!(verification.standards),
        "querygraphVerification",
    )?;

    require_string_match(
        bootstrap,
        "bundleHash",
        verification.bundle_hash.as_str(),
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "graphHash",
        verification.graph_hash.as_str(),
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "openLineageHash",
        verification.open_lineage_hash.as_str(),
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "queryGraphImportHash",
        verification.querygraph_import_hash.as_str(),
        "queryGraphBootstrapProof",
    )?;
    Ok(json!({
        "warehouse": verification.warehouse,
        "tableCount": verification.table_count,
        "viewCount": verification.view_count,
        "verifiedTables": verification.verified_tables,
        "verifiedViews": verification.verified_views,
        "bundleHash": verification.bundle_hash,
        "graphHash": verification.graph_hash,
        "openLineageHash": verification.open_lineage_hash,
        "queryGraphImportHash": verification.querygraph_import_hash,
        "standards": verification.standards,
        "graphNodes": bundle.graph.nodes.len(),
        "graphEdges": bundle.graph.edges.len(),
    }))
}

fn require_qglake_import_plan_graph_counts_match_bundle(
    bundle_semantics: &Value,
    import_plan_semantics: &Value,
) -> lakecat_core::LakeCatResult<()> {
    let bundle = bundle_semantics.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal(
            "handoff bundle artifact semantics must be an object".to_string(),
        )
    })?;
    let import_plan = import_plan_semantics.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal(
            "handoff QueryGraph import plan semantics must be an object".to_string(),
        )
    })?;
    for field in ["graphNodes", "graphEdges"] {
        require_value_match(
            import_plan,
            field,
            required_value(bundle, field, "bundleArtifactSemantics")?,
            "querygraphImportPlanSemantics",
        )?;
    }
    Ok(())
}

fn verify_qglake_handoff_querygraph_import_plan_semantics(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let warehouse = required_str(summary, "warehouse", "handoff summary")?;
    let import = required_object(summary, "querygraphImportVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));
    let plan_value =
        read_qglake_handoff_artifact_json(artifacts, "querygraphImportPlan", base_dir)?;
    let plan = plan_value.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff QueryGraph import plan artifact must be a JSON object".to_string(),
        )
    })?;
    let verification = required_object(
        plan,
        "verification",
        "handoff QueryGraph import plan artifact",
    )?;
    let table_scope = HandoffTableScope::from_summary(summary, warehouse)?;
    let view_scope = HandoffViewScope::from_lakecat(lakecat)?;

    verify_querygraph_import_plan_verification_matches_summary(
        verification,
        import,
        &table_scope,
        &view_scope,
    )?;
    verify_querygraph_import_plan_artifact_lists(plan, verification)?;

    Ok(json!({
        "warehouse": required_str(verification, "warehouse", "handoff QueryGraph import plan artifact.verification")?,
        "tableCount": required_u64(verification, "table-count", "handoff QueryGraph import plan artifact.verification")?,
        "viewCount": required_u64(verification, "view-count", "handoff QueryGraph import plan artifact.verification")?,
        "verifiedTables": required_value(verification, "verified-tables", "handoff QueryGraph import plan artifact.verification")?,
        "verifiedViews": required_value(verification, "verified-views", "handoff QueryGraph import plan artifact.verification")?,
        "bundleHash": required_str(verification, "bundle-hash", "handoff QueryGraph import plan artifact.verification")?,
        "graphHash": required_str(verification, "graph-hash", "handoff QueryGraph import plan artifact.verification")?,
        "openLineageHash": required_str(verification, "open-lineage-hash", "handoff QueryGraph import plan artifact.verification")?,
        "queryGraphImportHash": required_str(verification, "querygraph-import-hash", "handoff QueryGraph import plan artifact.verification")?,
        "standards": required_value(verification, "standards", "handoff QueryGraph import plan artifact.verification")?,
        "graphNodes": required_u64(plan, "graph-nodes", "handoff QueryGraph import plan artifact")?,
        "graphEdges": required_u64(plan, "graph-edges", "handoff QueryGraph import plan artifact")?,
    }))
}

fn verify_querygraph_import_plan_verification_matches_summary(
    verification: &serde_json::Map<String, Value>,
    import: &serde_json::Map<String, Value>,
    table_scope: &HandoffTableScope,
    view_scope: &HandoffViewScope,
) -> lakecat_core::LakeCatResult<()> {
    let label = "handoff QueryGraph import plan artifact.verification";
    require_string_match(
        verification,
        "warehouse",
        table_scope.warehouse.as_str(),
        label,
    )?;
    require_verified_table_scope(verification, table_scope, label)?;
    require_verified_view_scope(verification, view_scope, label)?;
    require_u64_match(
        verification,
        "table-count",
        required_u64(import, "tableCount", "querygraphImportVerification")?,
        label,
    )?;
    require_u64_match(
        verification,
        "view-count",
        required_u64(import, "viewCount", "querygraphImportVerification")?,
        label,
    )?;
    require_string_match(
        verification,
        "bundle-hash",
        required_str(import, "bundleHash", "querygraphImportVerification")?,
        label,
    )?;
    require_string_match(
        verification,
        "graph-hash",
        required_str(import, "graphHash", "querygraphImportVerification")?,
        label,
    )?;
    require_string_match(
        verification,
        "open-lineage-hash",
        required_str(import, "openLineageHash", "querygraphImportVerification")?,
        label,
    )?;
    require_string_match(
        verification,
        "querygraph-import-hash",
        required_str(
            import,
            "querygraphImportHash",
            "querygraphImportVerification",
        )?,
        label,
    )?;
    require_value_match(
        verification,
        "verified-tables",
        required_value(import, "verifiedTables", "querygraphImportVerification")?,
        label,
    )?;
    require_value_match(
        verification,
        "verified-views",
        required_value(import, "verifiedViews", "querygraphImportVerification")?,
        label,
    )?;
    require_value_match(
        verification,
        "standards",
        required_value(import, "standards", "querygraphImportVerification")?,
        label,
    )
}

fn verify_querygraph_import_plan_artifact_lists(
    plan: &serde_json::Map<String, Value>,
    verification: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let label = "handoff QueryGraph import plan artifact";
    let table_count = required_u64(
        verification,
        "table-count",
        "handoff QueryGraph import plan artifact.verification",
    )?;
    let view_count = required_u64(
        verification,
        "view-count",
        "handoff QueryGraph import plan artifact.verification",
    )?;
    let tables = required_array(plan, "tables", label)?;
    let views = required_array(plan, "views", label)?;
    if tables.len() as u64 != table_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.tables count mismatch: expected={table_count} actual={}",
            tables.len()
        )));
    }
    if views.len() as u64 != view_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.views count mismatch: expected={view_count} actual={}",
            views.len()
        )));
    }
    require_import_plan_list_covers_verified_ids(
        tables,
        required_array(
            verification,
            "verified-tables",
            "handoff QueryGraph import plan artifact.verification",
        )?,
        "tables",
    )?;
    require_import_plan_list_covers_verified_ids(
        views,
        required_array(
            verification,
            "verified-views",
            "handoff QueryGraph import plan artifact.verification",
        )?,
        "views",
    )?;
    require_positive_u64(plan, "graph-nodes", label)?;
    require_positive_u64(plan, "graph-edges", label)?;
    Ok(())
}

fn require_governed_scan_stats_field_evidence(
    governed_scan: &serde_json::Map<String, Value>,
    planned_restriction: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let requested = required_string_array(
        governed_scan,
        "plannedRequestedStatsFields",
        "governedScanProof",
    )?;
    let effective = required_string_array(
        governed_scan,
        "plannedEffectiveStatsFields",
        "governedScanProof",
    )?;
    if requested.is_empty() || effective.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof stats-field evidence must preserve non-empty requested and effective fields".to_string(),
        ));
    }
    if requested.len() <= effective.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof plannedRequestedStatsFields must prove a wider request than plannedEffectiveStatsFields".to_string(),
        ));
    }
    let requested_set = requested
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &effective {
        if !requested_set.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "governedScanProof plannedEffectiveStatsFields contains {field} that was not requested"
            )));
        }
    }
    require_value_match(
        planned_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "plannedEffectiveStatsFields",
            "governedScanProof",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    Ok(())
}

fn require_governed_scan_projection_evidence(
    governed_scan: &serde_json::Map<String, Value>,
    planned_restriction: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let requested = required_string_array(
        governed_scan,
        "plannedRequestedProjection",
        "governedScanProof",
    )?;
    let effective = required_string_array(
        governed_scan,
        "plannedEffectiveProjection",
        "governedScanProof",
    )?;
    if requested.is_empty() || effective.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof missing requested/effective projection evidence".to_string(),
        ));
    }
    if requested.len() <= effective.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "governedScanProof plannedRequestedProjection does not prove projection narrowing versus plannedEffectiveProjection".to_string(),
        ));
    }
    let requested_set = requested
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &effective {
        if !requested_set.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "governedScanProof plannedEffectiveProjection contains {field} that was not requested"
            )));
        }
    }
    require_value_match(
        planned_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "plannedEffectiveProjection",
            "governedScanProof",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    Ok(())
}

fn require_import_plan_list_covers_verified_ids(
    records: &[Value],
    verified_ids: &[Value],
    field: &str,
) -> lakecat_core::LakeCatResult<()> {
    let label = "handoff QueryGraph import plan artifact";
    for (index, verified_id) in verified_ids.iter().enumerate() {
        let verified_id = verified_id.as_str().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.verification.verified-{field}[{index}] must be a string"
            ))
        })?;
        let found = records.iter().any(|record| {
            record
                .as_object()
                .and_then(|record| record.get("stable-id"))
                .and_then(Value::as_str)
                == Some(verified_id)
        });
        if !found {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.{field} must include stable-id {verified_id}"
            )));
        }
    }
    Ok(())
}

fn verify_qglake_handoff_lineage_drain_artifact_semantics(
    summary_path: &Path,
    summary: &Value,
) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    let principal = required_str(summary, "principal", "handoff summary")?;
    let warehouse = required_str(summary, "warehouse", "handoff summary")?;
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    let bootstrap = required_object(
        lakecat,
        "queryGraphBootstrapProof",
        "lakecatReplayVerification",
    )?;
    let policy_binding_count =
        required_u64(bootstrap, "policyBindingCount", "queryGraphBootstrapProof")? as usize;
    let artifacts = required_object(summary, "artifacts", "handoff summary")?;
    let base_dir = summary_path.parent().unwrap_or_else(|| Path::new(""));
    let drain_value = read_qglake_handoff_artifact_json(artifacts, "lineageDrain", base_dir)?;
    let drain: LineageDrainResponse = serde_json::from_value(drain_value).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff lineage drain artifact is not a LakeCat lineage-drain response: {err}"
        ))
    })?;
    let verification = qglake_verification_from_handoff_summary(warehouse, querygraph, lakecat)?;
    verify_qglake_lineage_drain(&drain, &verification, Some(principal), policy_binding_count)?;
    let replay_evidence = qglake_replay_evidence_json(&drain, Some(principal), &verification);
    let replay = qglake_replay_verification_json(
        &verification,
        qglake_scan_replay_line(&drain),
        qglake_management_replay_line(&drain),
        qglake_credential_replay_line(&drain, Some(principal)),
        qglake_table_commit_history_replay_line(&drain),
        replay_evidence,
    );
    let replay = replay.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::Internal(
            "lineage drain replay verification must be an object".to_string(),
        )
    })?;
    verify_lakecat_replay_capture_matches_summary(replay, lakecat, querygraph)?;

    Ok(json!({
        "delivered": drain.delivered,
        "eventTypes": drain.event_types,
        "graphEvents": drain.graph_events,
        "lineageEvents": drain.lineage_events,
        "principalSubject": drain.principal_subject,
        "principalKind": drain.principal_kind,
        "authorizationReceiptHash": drain.authorization_receipt_hash,
        "requestIdentitySource": drain.request_identity_source,
        "requestIdentityState": drain.request_identity_state,
        "typedidEnvelopeHash": drain.typedid_envelope_hash,
        "typedidProofHash": drain.typedid_proof_hash,
        "tableCount": verification.table_count,
        "viewCount": verification.view_count,
        "verifiedTables": verification.verified_tables,
        "verifiedViews": verification.verified_views,
        "bundleHash": verification.bundle_hash,
        "graphHash": verification.graph_hash,
        "openLineageHash": verification.open_lineage_hash,
        "queryGraphImportHash": verification.querygraph_import_hash,
        "standards": verification.standards,
    }))
}

fn qglake_verification_from_handoff_summary(
    warehouse: &str,
    querygraph: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<QueryGraphBootstrapVerification> {
    let view_receipts = required_object(
        lakecat,
        "viewReceiptChainProof",
        "lakecatReplayVerification",
    )?;
    let mut verified_view_versions = BTreeMap::new();
    let mut verified_view_receipt_hashes = BTreeMap::new();
    let mut verified_view_receipt_chain_hashes = BTreeMap::new();
    for (index, view) in required_array(view_receipts, "views", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        let stable_id = required_str(view, "stableId", "viewReceiptChainProof.views[]")?;
        verified_view_versions.insert(
            stable_id.to_string(),
            required_u64(view, "acceptedViewVersion", "viewReceiptChainProof.views[]")?,
        );
        verified_view_receipt_hashes.insert(
            stable_id.to_string(),
            require_hash_str(view, "acceptedReceiptHash", "viewReceiptChainProof.views[]")?
                .to_string(),
        );
        verified_view_receipt_chain_hashes.insert(
            stable_id.to_string(),
            require_hash_str(
                view,
                "acceptedReceiptChainHash",
                "viewReceiptChainProof.views[]",
            )?
            .to_string(),
        );
    }

    Ok(QueryGraphBootstrapVerification {
        warehouse: warehouse.to_string(),
        table_count: required_u64(querygraph, "tableCount", "querygraphVerification")? as usize,
        view_count: required_u64(querygraph, "viewCount", "querygraphVerification")? as usize,
        verified_tables: required_string_array(
            querygraph,
            "verifiedTables",
            "querygraphVerification",
        )?,
        verified_views: required_string_array(
            querygraph,
            "verifiedViews",
            "querygraphVerification",
        )?,
        verified_view_versions,
        verified_view_receipt_hashes,
        verified_view_receipt_chain_hashes,
        bundle_hash: required_str(querygraph, "bundleHash", "querygraphVerification")?.to_string(),
        graph_hash: required_str(querygraph, "graphHash", "querygraphVerification")?.to_string(),
        open_lineage_hash: required_str(querygraph, "openLineageHash", "querygraphVerification")?
            .to_string(),
        querygraph_import_hash: required_str(
            querygraph,
            "querygraphImportHash",
            "querygraphVerification",
        )?
        .to_string(),
        standards: required_string_array(querygraph, "standards", "querygraphVerification")?,
    })
}

fn read_qglake_handoff_artifact_json(
    artifacts: &serde_json::Map<String, Value>,
    field: &str,
    base_dir: &Path,
) -> lakecat_core::LakeCatResult<Value> {
    let artifact = required_object(artifacts, field, "handoff summary artifacts")?;
    let path = required_str(artifact, "path", field)?;
    let path = PathBuf::from(path);
    let resolved_path = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    let bytes = fs::read(&resolved_path).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read captured handoff output {} at {}: {err}",
            field,
            resolved_path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "captured handoff output {} at {} is not JSON: {err}",
            field,
            resolved_path.display()
        ))
    })
}

fn verify_lakecat_replay_capture_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    require_string_match(
        capture,
        "schema-version",
        required_str(lakecat, "schemaVersion", "lakecatReplayVerification")?,
        "captured LakeCat replay output",
    )?;
    require_string_match(
        capture,
        "status",
        required_str(lakecat, "status", "lakecatReplayVerification")?,
        "captured LakeCat replay output",
    )?;
    require_handoff_summary_fields_match_capture(
        capture,
        querygraph,
        "captured LakeCat replay output",
    )?;
    verify_lakecat_replay_request_identity_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_querygraph_bootstrap_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_scan_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_table_commit_history_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_views_match_summary(capture, lakecat)?;
    verify_lakecat_replay_management_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_storage_profile_matches_summary(capture, lakecat)?;
    verify_lakecat_replay_credentials_match_summary(capture, lakecat)
}

fn verify_lakecat_replay_request_identity_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_request_identity = lakecat_replay_request_identity(capture)?;
    let summary_request_identity =
        required_object(lakecat, "requestIdentityProof", "lakecatReplayVerification")?;

    for field in [
        "principalSubject",
        "principalKind",
        "requestIdentitySource",
        "requestIdentityState",
        "authorizationReceiptHash",
    ] {
        require_string_match(
            captured_request_identity,
            field,
            required_str(summary_request_identity, field, "requestIdentityProof")?,
            "captured LakeCat replay output.replay-evidence.requestIdentity",
        )?;
    }

    for field in ["typedidEnvelopeHash", "typedidProofHash"] {
        require_value_match(
            captured_request_identity,
            field,
            required_value(summary_request_identity, field, "requestIdentityProof")?,
            "captured LakeCat replay output.replay-evidence.requestIdentity",
        )?;
    }

    Ok(())
}

fn lakecat_replay_request_identity(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "requestIdentity",
        "captured LakeCat replay output.replay-evidence",
    )
}

fn verify_lakecat_replay_querygraph_bootstrap_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_bootstrap = lakecat_replay_querygraph_bootstrap(capture)?;
    let summary_bootstrap = required_object(
        lakecat,
        "queryGraphBootstrapProof",
        "lakecatReplayVerification",
    )?;

    for field in [
        "bundleHash",
        "graphHash",
        "openLineageHash",
        "queryGraphImportHash",
        "principalSubject",
        "principalKind",
        "requestIdentitySource",
        "requestIdentityState",
        "authorizationReceiptHash",
        "agentDelegationHash",
        "agentSummarySignatureHash",
    ] {
        require_string_match(
            captured_bootstrap,
            field,
            required_str(summary_bootstrap, field, "queryGraphBootstrapProof")?,
            "captured LakeCat replay output.replay-evidence.queryGraphBootstrap",
        )?;
    }

    for field in [
        "tableArtifactCount",
        "viewArtifactCount",
        "policyBindingCount",
        "standards",
        "typedidEnvelopeHash",
        "typedidProofHash",
        "viewVersionReceiptHashes",
        "replayEventHashes",
        "openLineageHashes",
    ] {
        require_value_match(
            captured_bootstrap,
            field,
            required_value(summary_bootstrap, field, "queryGraphBootstrapProof")?,
            "captured LakeCat replay output.replay-evidence.queryGraphBootstrap",
        )?;
    }

    Ok(())
}

fn lakecat_replay_querygraph_bootstrap(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "queryGraphBootstrap",
        "captured LakeCat replay output.replay-evidence",
    )
}

fn verify_lakecat_replay_scan_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_scan = lakecat_replay_scan(capture)?;
    let summary_scan = required_object(lakecat, "governedScanProof", "lakecatReplayVerification")?;

    for field in [
        "planTaskCount",
        "fileTaskCount",
        "deleteFileCount",
        "childPlanTaskCount",
        "planGraphEvents",
        "plannedReadRestriction",
        "fetchedReadRestriction",
        "plannedRequestedProjection",
        "plannedEffectiveProjection",
        "plannedRequestedStatsFields",
        "plannedEffectiveStatsFields",
        "fetchedRequiredProjection",
        "fetchedEffectiveProjection",
        "fetchedRequiredFilters",
        "plannedReplayEventHashes",
        "fetchedReplayEventHashes",
        "plannedOpenLineageHashes",
        "fetchedOpenLineageHashes",
    ] {
        require_value_match(
            captured_scan,
            field,
            required_value(summary_scan, field, "governedScanProof")?,
            "captured LakeCat replay output.replay-evidence.scan",
        )?;
    }

    Ok(())
}

fn lakecat_replay_scan(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "scan",
        "captured LakeCat replay output.replay-evidence",
    )
}

fn verify_lakecat_replay_table_commit_history_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_commit_history = lakecat_replay_table_commit_history(capture)?;
    let summary_commit_history = required_object(
        lakecat,
        "tableCommitHistoryProof",
        "lakecatReplayVerification",
    )?;

    for field in [
        "commitCount",
        "sequenceNumbers",
        "commitHashes",
        "graphEvents",
        "replayEventHashes",
        "openLineageHashes",
    ] {
        require_value_match(
            captured_commit_history,
            field,
            required_value(summary_commit_history, field, "tableCommitHistoryProof")?,
            "captured LakeCat replay output.replay-evidence.tableCommitHistory",
        )?;
    }

    Ok(())
}

fn lakecat_replay_table_commit_history(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "tableCommitHistory",
        "captured LakeCat replay output.replay-evidence",
    )
}

fn verify_lakecat_replay_views_match_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_views = lakecat_replay_views(capture)?;
    let summary_views = required_object(
        lakecat,
        "viewReceiptChainProof",
        "lakecatReplayVerification",
    )?;

    for field in ["viewCount", "views", "tombstoneReceipts", "receiptChains"] {
        require_value_match(
            captured_views,
            field,
            required_value(summary_views, field, "viewReceiptChainProof")?,
            "captured LakeCat replay output.replay-evidence.views",
        )?;
    }

    Ok(())
}

fn lakecat_replay_views(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "views",
        "captured LakeCat replay output.replay-evidence",
    )
}

fn verify_lakecat_replay_storage_profile_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_storage_profile = lakecat_replay_storage_profile_upsert(capture)?;
    let summary_storage_profile = required_object(
        lakecat,
        "storageProfileUpsertProof",
        "lakecatReplayVerification",
    )?;

    for field in [
        "profileId",
        "provider",
        "issuanceMode",
        "locationPrefixHash",
    ] {
        require_string_match(
            captured_storage_profile,
            field,
            required_str(summary_storage_profile, field, "storageProfileUpsertProof")?,
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert",
        )?;
    }

    for field in [
        "secretRefPresent",
        "secretRefProvider",
        "secretRefHash",
        "graphEvents",
        "replayEventHashes",
        "openLineageHashes",
    ] {
        require_value_match(
            captured_storage_profile,
            field,
            required_value(summary_storage_profile, field, "storageProfileUpsertProof")?,
            "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert",
        )?;
    }

    Ok(())
}

fn verify_lakecat_replay_management_matches_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_management = lakecat_replay_management(capture)?;
    let summary_management =
        required_object(lakecat, "managementProof", "lakecatReplayVerification")?;

    for field in [
        "serverCount",
        "serverGraphEvents",
        "projectCount",
        "projectGraphEvents",
        "warehouseCount",
        "warehouseGraphEvents",
        "policyBindingCount",
        "policyGraphEvents",
        "storageProfileCount",
        "storageProfileGraphEvents",
        "serverReplayEventHashes",
        "serverOpenLineageHashes",
        "projectReplayEventHashes",
        "projectOpenLineageHashes",
        "warehouseReplayEventHashes",
        "warehouseOpenLineageHashes",
        "policyReplayEventHashes",
        "policyOpenLineageHashes",
        "storageProfileReplayEventHashes",
        "storageProfileOpenLineageHashes",
    ] {
        require_value_match(
            captured_management,
            field,
            required_value(summary_management, field, "managementProof")?,
            "captured LakeCat replay output.replay-evidence.management",
        )?;
    }

    Ok(())
}

fn lakecat_replay_management(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "management",
        "captured LakeCat replay output.replay-evidence",
    )
}

fn lakecat_replay_storage_profile_upsert(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    let management = lakecat_replay_management(capture)?;
    required_object(
        management,
        "storageProfileUpsert",
        "captured LakeCat replay output.replay-evidence.management",
    )
}

fn verify_lakecat_replay_credentials_match_summary(
    capture: &serde_json::Map<String, Value>,
    lakecat: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let captured_credentials = lakecat_replay_credentials(capture)?;
    let summary_credentials = required_object(
        lakecat,
        "credentialVendingProof",
        "lakecatReplayVerification",
    )?;

    for branch in ["restricted", "trustedHuman"] {
        let captured = required_object(
            captured_credentials,
            branch,
            "captured LakeCat replay output.replay-evidence.credentials",
        )?;
        let summary = required_object(summary_credentials, branch, "credentialVendingProof")?;
        for field in ["principalSubject", "principalKind"] {
            require_string_match(
                captured,
                field,
                required_str(summary, field, "credentialVendingProof")?,
                "captured LakeCat replay output.replay-evidence.credentials",
            )?;
        }
        for field in [
            "credentialCount",
            "maxCredentialTtlSeconds",
            "replayEventHashes",
            "openLineageHashes",
        ] {
            require_value_match(
                captured,
                field,
                required_value(summary, field, "credentialVendingProof")?,
                "captured LakeCat replay output.replay-evidence.credentials",
            )?;
        }
        require_value_match(
            captured,
            "storageProfile",
            required_value(summary, "storageProfile", "credentialVendingProof")?,
            "captured LakeCat replay output.replay-evidence.credentials",
        )?;
    }

    let captured_restricted = required_object(
        captured_credentials,
        "restricted",
        "captured LakeCat replay output.replay-evidence.credentials",
    )?;
    let summary_restricted =
        required_object(summary_credentials, "restricted", "credentialVendingProof")?;
    require_string_match(
        captured_restricted,
        "blockReason",
        required_str(summary_restricted, "blockReason", "credentialVendingProof")?,
        "captured LakeCat replay output.replay-evidence.credentials.restricted",
    )?;
    require_value_match(
        captured_restricted,
        "rawCredentialExceptionAllowed",
        required_value(
            summary_restricted,
            "rawCredentialExceptionAllowed",
            "credentialVendingProof",
        )?,
        "captured LakeCat replay output.replay-evidence.credentials.restricted",
    )?;

    let captured_trusted = required_object(
        captured_credentials,
        "trustedHuman",
        "captured LakeCat replay output.replay-evidence.credentials",
    )?;
    let summary_trusted = required_object(
        summary_credentials,
        "trustedHuman",
        "credentialVendingProof",
    )?;
    for field in [
        "blockReason",
        "rawCredentialExceptionAllowed",
        "rawCredentialExceptionReason",
    ] {
        require_value_match(
            captured_trusted,
            field,
            required_value(summary_trusted, field, "credentialVendingProof")?,
            "captured LakeCat replay output.replay-evidence.credentials.trustedHuman",
        )?;
    }

    Ok(())
}

fn lakecat_replay_credentials(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(
        lakecat_replay_evidence(capture)?,
        "credentials",
        "captured LakeCat replay output.replay-evidence",
    )
}

fn lakecat_replay_evidence(
    capture: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&serde_json::Map<String, Value>> {
    required_object(capture, "replay-evidence", "captured LakeCat replay output")
}

fn verify_querygraph_capture_matches_summary(
    capture: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
    table_scope: &HandoffTableScope,
    view_scope: &HandoffViewScope,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_string_match(capture, "warehouse", table_scope.warehouse.as_str(), label)?;
    require_verified_table_scope(capture, table_scope, label)?;
    require_verified_view_scope(capture, view_scope, label)?;
    require_handoff_summary_fields_match_capture(capture, querygraph, label)?;
    require_querygraph_verified_ids_match_capture(capture, querygraph, label)
}

fn require_handoff_summary_fields_match_capture(
    capture: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_u64_match(
        capture,
        "table-count",
        required_u64(querygraph, "tableCount", "querygraphVerification")?,
        label,
    )?;
    require_u64_match(
        capture,
        "view-count",
        required_u64(querygraph, "viewCount", "querygraphVerification")?,
        label,
    )?;
    require_string_match(
        capture,
        "bundle-hash",
        required_str(querygraph, "bundleHash", "querygraphVerification")?,
        label,
    )?;
    require_string_match(
        capture,
        "graph-hash",
        required_str(querygraph, "graphHash", "querygraphVerification")?,
        label,
    )?;
    require_string_match(
        capture,
        "open-lineage-hash",
        required_str(querygraph, "openLineageHash", "querygraphVerification")?,
        label,
    )?;
    require_string_match(
        capture,
        "querygraph-import-hash",
        required_str(querygraph, "querygraphImportHash", "querygraphVerification")?,
        label,
    )?;
    require_value_match(
        capture,
        "standards",
        required_value(querygraph, "standards", "querygraphVerification")?,
        label,
    )
}

fn require_querygraph_verified_ids_match_capture(
    capture: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_value_match(
        capture,
        "verified-tables",
        required_value(querygraph, "verifiedTables", "querygraphVerification")?,
        label,
    )?;
    require_value_match(
        capture,
        "verified-views",
        required_value(querygraph, "verifiedViews", "querygraphVerification")?,
        label,
    )
}

fn querygraph_capture_semantics_json(
    capture: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<Value> {
    Ok(json!({
        "warehouse": required_str(capture, "warehouse", label)?,
        "verifiedTables": required_value(capture, "verified-tables", label)?,
        "verifiedViews": required_value(capture, "verified-views", label)?,
        "tableCount": required_u64(capture, "table-count", label)?,
        "viewCount": required_u64(capture, "view-count", label)?,
        "bundleHash": required_str(capture, "bundle-hash", label)?,
        "graphHash": required_str(capture, "graph-hash", label)?,
        "openLineageHash": required_str(capture, "open-lineage-hash", label)?,
        "queryGraphImportHash": required_str(capture, "querygraph-import-hash", label)?,
        "standards": required_value(capture, "standards", label)?,
    }))
}

struct HandoffTableScope {
    warehouse: String,
    namespace: String,
    table: String,
}

impl HandoffTableScope {
    fn from_summary(
        summary: &serde_json::Map<String, Value>,
        warehouse: &str,
    ) -> lakecat_core::LakeCatResult<Self> {
        Ok(Self {
            warehouse: warehouse.to_string(),
            namespace: require_non_empty_str(summary, "namespace", "handoff summary")?.to_string(),
            table: require_non_empty_str(summary, "table", "handoff summary")?.to_string(),
        })
    }

    fn stable_table_id(&self) -> String {
        format!(
            "lakecat:table:{}:{}:{}",
            self.warehouse, self.namespace, self.table
        )
    }
}

struct HandoffViewScope {
    stable_view_ids: Vec<String>,
}

impl HandoffViewScope {
    fn from_lakecat(lakecat: &serde_json::Map<String, Value>) -> lakecat_core::LakeCatResult<Self> {
        let views = required_object(
            lakecat,
            "viewReceiptChainProof",
            "lakecatReplayVerification",
        )?;
        let mut stable_view_ids = Vec::new();
        for (index, view) in required_array(views, "views", "viewReceiptChainProof")?
            .iter()
            .enumerate()
        {
            let view = view.as_object().ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "viewReceiptChainProof.views[{index}] must be an object"
                ))
            })?;
            stable_view_ids
                .push(required_str(view, "stableId", "viewReceiptChainProof.views[]")?.to_string());
        }
        Ok(Self { stable_view_ids })
    }
}

fn require_verified_table_scope(
    capture: &serde_json::Map<String, Value>,
    scope: &HandoffTableScope,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let expected_table = scope.stable_table_id();
    let tables = required_array(capture, "verified-tables", label)?;
    if !tables
        .iter()
        .any(|table| table.as_str() == Some(expected_table.as_str()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.verified-tables must include {expected_table}"
        )));
    }
    Ok(())
}

fn require_verified_view_scope(
    capture: &serde_json::Map<String, Value>,
    scope: &HandoffViewScope,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let views = required_array(capture, "verified-views", label)?;
    for expected_view in &scope.stable_view_ids {
        if !views
            .iter()
            .any(|view| view.as_str() == Some(expected_view.as_str()))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.verified-views must include {expected_view}"
            )));
        }
    }
    Ok(())
}

fn verify_qglake_handoff_summary_value(summary: &Value) -> lakecat_core::LakeCatResult<Value> {
    let summary = summary.as_object().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary root must be an object".to_string(),
        )
    })?;
    require_string_eq(
        summary,
        "schemaVersion",
        "lakecat.qglake.handoff-summary.v1",
        "handoff summary",
    )?;
    require_string_eq(summary, "status", "verified", "handoff summary")?;
    let principal = required_str(summary, "principal", "handoff summary")?;
    let scope = require_handoff_scope(summary)?;
    let querygraph = required_object(summary, "querygraphVerification", "handoff summary")?;
    let import = required_object(summary, "querygraphImportVerification", "handoff summary")?;
    if required_bool(import, "matchesVerify", "querygraphImportVerification")? != true {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary querygraphImportVerification.matchesVerify must be true".to_string(),
        ));
    }
    require_querygraph_import_matches_verify(import, querygraph)?;
    require_core_querygraph_hash_evidence(
        querygraph,
        "querygraphImportHash",
        "querygraphVerification",
    )?;
    require_core_querygraph_hash_evidence(
        import,
        "querygraphImportHash",
        "querygraphImportVerification",
    )?;
    require_qglake_standards_value(
        required_value(querygraph, "standards", "querygraphVerification")?,
        "querygraphVerification.standards",
    )?;
    let lakecat = required_object(summary, "lakecatReplayVerification", "handoff summary")?;
    require_string_eq(
        lakecat,
        "schemaVersion",
        "lakecat.qglake.replay-verification.v1",
        "lakecatReplayVerification",
    )?;
    require_string_eq(lakecat, "status", "verified", "lakecatReplayVerification")?;
    if required_bool(lakecat, "matchesQueryGraph", "lakecatReplayVerification")? != true {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary lakecatReplayVerification.matchesQueryGraph must be true".to_string(),
        ));
    }
    let request_identity =
        required_object(lakecat, "requestIdentityProof", "lakecatReplayVerification")?;
    require_string_match(
        request_identity,
        "principalSubject",
        principal,
        "requestIdentityProof",
    )?;
    require_string_eq(
        request_identity,
        "principalKind",
        "agent",
        "requestIdentityProof",
    )?;
    require_non_empty_str(
        request_identity,
        "requestIdentitySource",
        "requestIdentityProof",
    )?;
    require_non_empty_str(
        request_identity,
        "requestIdentityState",
        "requestIdentityProof",
    )?;
    require_hash_str(
        request_identity,
        "authorizationReceiptHash",
        "requestIdentityProof",
    )?;
    require_typedid_hash_pair(request_identity, "requestIdentityProof")?;

    let bootstrap = required_object(
        lakecat,
        "queryGraphBootstrapProof",
        "lakecatReplayVerification",
    )?;
    require_core_querygraph_hash_evidence(
        bootstrap,
        "queryGraphImportHash",
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "bundleHash",
        required_str(querygraph, "bundleHash", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "graphHash",
        required_str(querygraph, "graphHash", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "openLineageHash",
        required_str(querygraph, "openLineageHash", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "queryGraphImportHash",
        required_str(querygraph, "querygraphImportHash", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_u64_match(
        bootstrap,
        "tableArtifactCount",
        required_u64(querygraph, "tableCount", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_u64_match(
        bootstrap,
        "viewArtifactCount",
        required_u64(querygraph, "viewCount", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_positive_u64(bootstrap, "policyBindingCount", "queryGraphBootstrapProof")?;
    require_value_match(
        bootstrap,
        "standards",
        required_value(querygraph, "standards", "querygraphVerification")?,
        "queryGraphBootstrapProof",
    )?;
    require_string_match(
        bootstrap,
        "principalSubject",
        principal,
        "queryGraphBootstrapProof",
    )?;
    require_string_eq(
        bootstrap,
        "principalKind",
        "agent",
        "queryGraphBootstrapProof",
    )?;
    for field in ["requestIdentitySource", "requestIdentityState"] {
        require_string_match(
            bootstrap,
            field,
            required_str(request_identity, field, "requestIdentityProof")?,
            "queryGraphBootstrapProof",
        )?;
    }
    require_hash_str(
        bootstrap,
        "authorizationReceiptHash",
        "queryGraphBootstrapProof",
    )?;
    require_hash_str(bootstrap, "agentDelegationHash", "queryGraphBootstrapProof")?;
    require_hash_str(
        bootstrap,
        "agentSummarySignatureHash",
        "queryGraphBootstrapProof",
    )?;
    require_typedid_hash_pair(bootstrap, "queryGraphBootstrapProof")?;
    if required_u64(querygraph, "viewCount", "querygraphVerification")? > 0 {
        require_hash_array(
            bootstrap,
            "viewVersionReceiptHashes",
            "queryGraphBootstrapProof",
        )?;
    } else {
        required_array(
            bootstrap,
            "viewVersionReceiptHashes",
            "queryGraphBootstrapProof",
        )?;
    }
    require_hash_array(bootstrap, "replayEventHashes", "queryGraphBootstrapProof")?;
    require_hash_array(bootstrap, "openLineageHashes", "queryGraphBootstrapProof")?;

    let governed_scan = required_object(lakecat, "governedScanProof", "lakecatReplayVerification")?;
    require_positive_u64(governed_scan, "planTaskCount", "governedScanProof")?;
    require_positive_u64(governed_scan, "planGraphEvents", "governedScanProof")?;
    require_positive_u64(governed_scan, "fileTaskCount", "governedScanProof")?;
    require_positive_u64(governed_scan, "deleteFileCount", "governedScanProof")?;
    require_positive_u64(governed_scan, "childPlanTaskCount", "governedScanProof")?;
    let planned_restriction =
        required_object(governed_scan, "plannedReadRestriction", "governedScanProof")?;
    let fetched_restriction =
        required_object(governed_scan, "fetchedReadRestriction", "governedScanProof")?;
    require_read_restriction_evidence(
        planned_restriction,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_read_restriction_evidence(
        fetched_restriction,
        "governedScanProof.fetchedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "policy-hashes",
        required_value(
            fetched_restriction,
            "policy-hashes",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "allowed-columns",
        required_value(
            fetched_restriction,
            "allowed-columns",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "row-predicate",
        required_value(
            fetched_restriction,
            "row-predicate",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "purpose",
        required_value(
            fetched_restriction,
            "purpose",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        planned_restriction,
        "max-credential-ttl-seconds",
        required_value(
            fetched_restriction,
            "max-credential-ttl-seconds",
            "governedScanProof.fetchedReadRestriction",
        )?,
        "governedScanProof.plannedReadRestriction",
    )?;
    require_value_match(
        fetched_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "fetchedRequiredProjection",
            "governedScanProof",
        )?,
        "governedScanProof.fetchedReadRestriction",
    )?;
    require_value_match(
        fetched_restriction,
        "allowed-columns",
        required_value(
            governed_scan,
            "fetchedEffectiveProjection",
            "governedScanProof",
        )?,
        "governedScanProof.fetchedReadRestriction",
    )?;
    require_governed_scan_projection_evidence(governed_scan, planned_restriction)?;
    require_governed_scan_stats_field_evidence(governed_scan, planned_restriction)?;
    let fetched_required_filters =
        required_array(governed_scan, "fetchedRequiredFilters", "governedScanProof")?;
    let expected_fetched_filters = vec![
        required_value(
            fetched_restriction,
            "row-predicate",
            "governedScanProof.fetchedReadRestriction",
        )?
        .clone(),
    ];
    if fetched_required_filters.as_slice() != expected_fetched_filters.as_slice() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "governedScanProof fetchedRequiredFilters did not exactly preserve fetched row predicate: {}",
            Value::Array(fetched_required_filters.clone())
        )));
    }
    require_hash_array(
        governed_scan,
        "plannedReplayEventHashes",
        "governedScanProof",
    )?;
    require_hash_array(
        governed_scan,
        "fetchedReplayEventHashes",
        "governedScanProof",
    )?;
    require_hash_array(
        governed_scan,
        "plannedOpenLineageHashes",
        "governedScanProof",
    )?;
    require_hash_array(
        governed_scan,
        "fetchedOpenLineageHashes",
        "governedScanProof",
    )?;

    let commit_history = required_object(
        lakecat,
        "tableCommitHistoryProof",
        "lakecatReplayVerification",
    )?;
    require_table_commit_history_evidence(commit_history)?;

    let management = required_object(lakecat, "managementProof", "lakecatReplayVerification")?;
    require_management_evidence(
        management,
        required_u64(bootstrap, "policyBindingCount", "queryGraphBootstrapProof")?,
    )?;

    let storage_profile = required_object(
        lakecat,
        "storageProfileUpsertProof",
        "lakecatReplayVerification",
    )?;
    require_storage_profile_upsert_evidence(storage_profile)?;

    let credentials = required_object(
        lakecat,
        "credentialVendingProof",
        "lakecatReplayVerification",
    )?;
    require_credential_vending_evidence(credentials, principal, storage_profile)?;

    let views = required_object(
        lakecat,
        "viewReceiptChainProof",
        "lakecatReplayVerification",
    )?;
    require_querygraph_verified_scope(querygraph, &scope, views)?;
    require_u64_match(
        views,
        "viewCount",
        required_u64(querygraph, "viewCount", "querygraphVerification")?,
        "viewReceiptChainProof",
    )?;
    required_array(views, "views", "viewReceiptChainProof")?;
    required_array(views, "tombstoneReceipts", "viewReceiptChainProof")?;
    required_array(views, "receiptChains", "viewReceiptChainProof")?;
    require_bootstrap_view_receipt_hashes_match_views(bootstrap, views)?;
    require_view_tombstone_expected_versions(views)?;
    require_view_receipt_chain_evidence(views)?;

    Ok(json!({
        "schemaVersion": "lakecat.qglake.handoff-verification.v1",
        "status": "verified",
        "principal": principal,
        "catalogUrl": scope.catalog_url,
        "warehouse": scope.warehouse,
        "namespace": scope.namespace,
        "table": scope.table,
        "tableCount": required_u64(querygraph, "tableCount", "querygraphVerification")?,
        "viewCount": required_u64(querygraph, "viewCount", "querygraphVerification")?,
        "verifiedTables": required_value(querygraph, "verifiedTables", "querygraphVerification")?,
        "verifiedViews": required_value(querygraph, "verifiedViews", "querygraphVerification")?,
        "standards": required_value(querygraph, "standards", "querygraphVerification")?,
        "queryGraphBootstrapProof": bootstrap,
        "requestIdentityProof": request_identity,
    }))
}

fn require_querygraph_verified_scope(
    querygraph: &serde_json::Map<String, Value>,
    scope: &HandoffScope<'_>,
    views: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let table_count = required_u64(querygraph, "tableCount", "querygraphVerification")?;
    let verified_tables = required_array(querygraph, "verifiedTables", "querygraphVerification")?;
    if verified_tables.len() as u64 != table_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "querygraphVerification.verifiedTables length mismatch: expected={table_count} actual={}",
            verified_tables.len()
        )));
    }
    let expected_table = format!(
        "lakecat:table:{}:{}:{}",
        scope.warehouse, scope.namespace, scope.table
    );
    if !verified_tables
        .iter()
        .any(|table| table.as_str() == Some(expected_table.as_str()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "querygraphVerification.verifiedTables must include {expected_table}"
        )));
    }

    let view_count = required_u64(querygraph, "viewCount", "querygraphVerification")?;
    let verified_views = required_array(querygraph, "verifiedViews", "querygraphVerification")?;
    if verified_views.len() as u64 != view_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "querygraphVerification.verifiedViews length mismatch: expected={view_count} actual={}",
            verified_views.len()
        )));
    }
    for (index, view) in required_array(views, "views", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        let expected_view = required_str(view, "stableId", "viewReceiptChainProof.views[]")?;
        if !verified_views
            .iter()
            .any(|view| view.as_str() == Some(expected_view))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "querygraphVerification.verifiedViews must include {expected_view}"
            )));
        }
    }
    Ok(())
}

fn require_qglake_standards_value(value: &Value, label: &str) -> lakecat_core::LakeCatResult<()> {
    let standards = value.as_array().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("{label} must be an array"))
    })?;
    for expected in QGLAKE_BOOTSTRAP_STANDARDS {
        if !standards
            .iter()
            .any(|standard| standard.as_str() == Some(*expected))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} did not include required QGLake standard {expected}"
            )));
        }
    }
    Ok(())
}

fn require_querygraph_import_matches_verify(
    import: &serde_json::Map<String, Value>,
    querygraph: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    for field in [
        "tableCount",
        "viewCount",
        "verifiedTables",
        "verifiedViews",
        "bundleHash",
        "graphHash",
        "openLineageHash",
        "querygraphImportHash",
        "standards",
    ] {
        require_value_match(
            import,
            field,
            required_value(querygraph, field, "querygraphVerification")?,
            "querygraphImportVerification",
        )?;
    }
    Ok(())
}

fn require_core_querygraph_hash_evidence(
    value: &serde_json::Map<String, Value>,
    import_hash_field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    for field in ["bundleHash", "graphHash", "openLineageHash"] {
        require_hash_str(value, field, label)?;
    }
    require_hash_str(value, import_hash_field, label)?;
    Ok(())
}

struct HandoffScope<'a> {
    catalog_url: &'a str,
    warehouse: &'a str,
    namespace: &'a str,
    table: &'a str,
}

fn require_handoff_scope<'a>(
    summary: &'a serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<HandoffScope<'a>> {
    let catalog_url = require_handoff_catalog_url(summary)?;
    let warehouse = require_non_empty_str(summary, "warehouse", "handoff summary")?;
    let namespace = require_non_empty_str(summary, "namespace", "handoff summary")?;
    let table = require_non_empty_str(summary, "table", "handoff summary")?;
    Ok(HandoffScope {
        catalog_url,
        warehouse,
        namespace,
        table,
    })
}

fn require_handoff_catalog_url<'a>(
    summary: &'a serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<&'a str> {
    let catalog_url = require_non_empty_str(summary, "catalogUrl", "handoff summary")?;
    let parsed = Url::parse(catalog_url).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff summary catalogUrl must be an absolute HTTP(S) URL: {err}"
        ))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "handoff summary catalogUrl must be an absolute HTTP(S) URL with a host: {catalog_url}"
        )));
    }
    Ok(catalog_url)
}

fn require_storage_profile_upsert_evidence(
    storage_profile: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    require_non_empty_str(storage_profile, "profileId", "storageProfileUpsertProof")?;
    require_non_empty_str(storage_profile, "provider", "storageProfileUpsertProof")?;
    require_non_empty_str(storage_profile, "issuanceMode", "storageProfileUpsertProof")?;
    require_hash_str(
        storage_profile,
        "locationPrefixHash",
        "storageProfileUpsertProof",
    )?;
    if required_bool(
        storage_profile,
        "secretRefPresent",
        "storageProfileUpsertProof",
    )? {
        require_non_empty_str(
            storage_profile,
            "secretRefProvider",
            "storageProfileUpsertProof",
        )?;
        require_hash_str(
            storage_profile,
            "secretRefHash",
            "storageProfileUpsertProof",
        )?;
    } else if !matches!(
        required_value(
            storage_profile,
            "secretRefProvider",
            "storageProfileUpsertProof"
        )?,
        Value::Null
    ) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "storageProfileUpsertProof.secretRefProvider must be null when secretRefPresent is false"
                .to_string(),
        ));
    } else if !matches!(
        required_value(
            storage_profile,
            "secretRefHash",
            "storageProfileUpsertProof"
        )?,
        Value::Null
    ) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "storageProfileUpsertProof.secretRefHash must be null when secretRefPresent is false"
                .to_string(),
        ));
    }
    require_hash_array(
        storage_profile,
        "replayEventHashes",
        "storageProfileUpsertProof",
    )?;
    require_hash_array(
        storage_profile,
        "openLineageHashes",
        "storageProfileUpsertProof",
    )?;
    require_positive_u64(storage_profile, "graphEvents", "storageProfileUpsertProof")?;
    Ok(())
}

fn require_typedid_hash_pair(
    value: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let envelope_present = require_optional_hash_value(value, "typedidEnvelopeHash", label)?;
    let proof_present = require_optional_hash_value(value, "typedidProofHash", label)?;
    if proof_present && !envelope_present {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.typedidProofHash requires typedidEnvelopeHash"
        )));
    }
    Ok(())
}

fn require_management_evidence(
    management: &serde_json::Map<String, Value>,
    expected_policy_binding_count: u64,
) -> lakecat_core::LakeCatResult<()> {
    require_positive_u64(management, "serverCount", "managementProof")?;
    require_positive_u64(management, "serverGraphEvents", "managementProof")?;
    require_positive_u64(management, "projectCount", "managementProof")?;
    require_positive_u64(management, "projectGraphEvents", "managementProof")?;
    require_positive_u64(management, "warehouseCount", "managementProof")?;
    require_positive_u64(management, "warehouseGraphEvents", "managementProof")?;
    require_positive_u64(management, "policyGraphEvents", "managementProof")?;
    require_positive_u64(management, "storageProfileCount", "managementProof")?;
    require_positive_u64(management, "storageProfileGraphEvents", "managementProof")?;
    require_u64_match(
        management,
        "policyBindingCount",
        expected_policy_binding_count,
        "managementProof",
    )?;
    for field in [
        "serverReplayEventHashes",
        "serverOpenLineageHashes",
        "projectReplayEventHashes",
        "projectOpenLineageHashes",
        "warehouseReplayEventHashes",
        "warehouseOpenLineageHashes",
        "policyReplayEventHashes",
        "policyOpenLineageHashes",
        "storageProfileReplayEventHashes",
        "storageProfileOpenLineageHashes",
    ] {
        require_hash_array(management, field, "managementProof")?;
    }
    Ok(())
}

fn require_table_commit_history_evidence(
    commit_history: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let commit_count =
        require_positive_u64(commit_history, "commitCount", "tableCommitHistoryProof")?;
    let sequence_numbers =
        required_array(commit_history, "sequenceNumbers", "tableCommitHistoryProof")?;
    if sequence_numbers.len() as u64 != commit_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "tableCommitHistoryProof.sequenceNumbers length mismatch: expected={commit_count} actual={}",
            sequence_numbers.len()
        )));
    }
    let mut previous = 0;
    for (index, sequence_number) in sequence_numbers.iter().enumerate() {
        let Some(sequence_number) = sequence_number.as_u64() else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "tableCommitHistoryProof.sequenceNumbers[{index}] must be a positive integer"
            )));
        };
        if sequence_number == 0 {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "tableCommitHistoryProof.sequenceNumbers[{index}] must be positive"
            )));
        }
        if sequence_number <= previous {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "tableCommitHistoryProof.sequenceNumbers must be strictly increasing"
            )));
        }
        previous = sequence_number;
    }

    let commit_hashes = required_array(commit_history, "commitHashes", "tableCommitHistoryProof")?;
    if commit_hashes.len() as u64 != commit_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "tableCommitHistoryProof.commitHashes length mismatch: expected={commit_count} actual={}",
            commit_hashes.len()
        )));
    }
    require_hash_array(commit_history, "commitHashes", "tableCommitHistoryProof")?;
    require_positive_u64(commit_history, "graphEvents", "tableCommitHistoryProof")?;
    require_hash_array(
        commit_history,
        "replayEventHashes",
        "tableCommitHistoryProof",
    )?;
    require_hash_array(
        commit_history,
        "openLineageHashes",
        "tableCommitHistoryProof",
    )?;
    Ok(())
}

fn require_credential_vending_evidence(
    credentials: &serde_json::Map<String, Value>,
    principal: &str,
    storage_profile_upsert: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let restricted = required_object(credentials, "restricted", "credentialVendingProof")?;
    require_string_eq(
        restricted,
        "principalSubject",
        principal,
        "credentialVendingProof.restricted",
    )?;
    require_string_eq(
        restricted,
        "principalKind",
        "agent",
        "credentialVendingProof.restricted",
    )?;
    require_u64_match(
        restricted,
        "credentialCount",
        0,
        "credentialVendingProof.restricted",
    )?;
    if required_bool(
        restricted,
        "rawCredentialExceptionAllowed",
        "credentialVendingProof.restricted",
    )? != false
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "credentialVendingProof.restricted.rawCredentialExceptionAllowed must not allow a raw credential exception"
                .to_string(),
        ));
    }
    require_string_eq(
        restricted,
        "blockReason",
        QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON,
        "credentialVendingProof.restricted",
    )?;
    if let Some(reason) = restricted.get("rawCredentialExceptionReason") {
        if !reason.is_null() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "credentialVendingProof.restricted.rawCredentialExceptionReason must be null when raw credentials are blocked"
                    .to_string(),
            ));
        }
    }
    require_hash_array(
        restricted,
        "replayEventHashes",
        "credentialVendingProof.restricted",
    )?;
    require_hash_array(
        restricted,
        "openLineageHashes",
        "credentialVendingProof.restricted",
    )?;
    let restricted_ttl = require_positive_u64(
        restricted,
        "maxCredentialTtlSeconds",
        "credentialVendingProof.restricted",
    )?;
    require_credential_storage_profile_evidence(restricted, "credentialVendingProof.restricted")?;
    require_credential_storage_profile_matches_upsert(
        restricted,
        storage_profile_upsert,
        "credentialVendingProof.restricted",
    )?;

    let trusted = required_object(credentials, "trustedHuman", "credentialVendingProof")?;
    require_non_empty_str(
        trusted,
        "principalSubject",
        "credentialVendingProof.trustedHuman",
    )?;
    require_string_eq(
        trusted,
        "principalKind",
        "human",
        "credentialVendingProof.trustedHuman",
    )?;
    require_positive_u64(
        trusted,
        "credentialCount",
        "credentialVendingProof.trustedHuman",
    )?;
    if required_bool(
        trusted,
        "rawCredentialExceptionAllowed",
        "credentialVendingProof.trustedHuman",
    )? != true
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "handoff summary trusted-human proof must allow the audited raw credential exception"
                .to_string(),
        ));
    }
    require_string_eq(
        trusted,
        "rawCredentialExceptionReason",
        QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON,
        "credentialVendingProof.trustedHuman",
    )?;
    if !required_value(
        trusted,
        "blockReason",
        "credentialVendingProof.trustedHuman",
    )?
    .is_null()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "credentialVendingProof.trustedHuman.blockReason must be null for the audited raw credential exception"
                .to_string(),
        ));
    }
    require_hash_array(
        trusted,
        "replayEventHashes",
        "credentialVendingProof.trustedHuman",
    )?;
    require_hash_array(
        trusted,
        "openLineageHashes",
        "credentialVendingProof.trustedHuman",
    )?;
    let trusted_ttl = require_positive_u64(
        trusted,
        "maxCredentialTtlSeconds",
        "credentialVendingProof.trustedHuman",
    )?;
    if trusted_ttl != restricted_ttl {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "credentialVendingProof.trustedHuman.maxCredentialTtlSeconds mismatch: expected={restricted_ttl} actual={trusted_ttl}"
        )));
    }
    require_credential_storage_profile_evidence(trusted, "credentialVendingProof.trustedHuman")?;
    require_credential_storage_profile_matches_upsert(
        trusted,
        storage_profile_upsert,
        "credentialVendingProof.trustedHuman",
    )?;

    Ok(())
}

fn require_credential_storage_profile_matches_upsert(
    credential: &serde_json::Map<String, Value>,
    storage_profile_upsert: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let storage_profile = required_object(credential, "storageProfile", label)?;
    let storage_label = format!("{label}.storageProfile");
    for field in [
        "profileId",
        "provider",
        "issuanceMode",
        "locationPrefixHash",
        "secretRefPresent",
        "secretRefProvider",
        "secretRefHash",
    ] {
        require_value_match(
            storage_profile,
            field,
            required_value(storage_profile_upsert, field, "storageProfileUpsertProof")?,
            storage_label.as_str(),
        )?;
    }
    Ok(())
}

fn require_credential_storage_profile_evidence(
    credential: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let storage_profile = required_object(credential, "storageProfile", label)?;
    let storage_label = format!("{label}.storageProfile");
    require_non_empty_str(storage_profile, "profileId", storage_label.as_str())?;
    require_non_empty_str(storage_profile, "provider", storage_label.as_str())?;
    require_non_empty_str(storage_profile, "issuanceMode", storage_label.as_str())?;
    require_hash_str(
        storage_profile,
        "locationPrefixHash",
        storage_label.as_str(),
    )?;
    if required_bool(storage_profile, "secretRefPresent", storage_label.as_str())? {
        require_non_empty_str(storage_profile, "secretRefProvider", storage_label.as_str())?;
        require_hash_str(storage_profile, "secretRefHash", storage_label.as_str())?;
    } else if !matches!(
        required_value(storage_profile, "secretRefProvider", storage_label.as_str())?,
        Value::Null
    ) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{storage_label}.secretRefProvider must be null when secretRefPresent is false"
        )));
    } else if !matches!(
        required_value(storage_profile, "secretRefHash", storage_label.as_str())?,
        Value::Null
    ) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{storage_label}.secretRefHash must be null when secretRefPresent is false"
        )));
    }
    require_positive_u64(storage_profile, "graphEvents", storage_label.as_str())?;
    Ok(())
}

fn require_read_restriction_evidence(
    restriction: &serde_json::Map<String, Value>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let allowed_columns = required_array(restriction, "allowed-columns", label)?;
    if allowed_columns.is_empty() || allowed_columns.iter().any(|column| !column.is_string()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.allowed-columns must contain column names"
        )));
    }
    required_object(restriction, "row-predicate", label)?;
    require_non_empty_str(restriction, "purpose", label)?;
    require_hash_array(restriction, "policy-hashes", label)?;
    require_positive_u64(restriction, "max-credential-ttl-seconds", label)?;
    Ok(())
}

fn require_view_tombstone_expected_versions(
    views: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let mut accepted_versions = HashMap::new();
    for (index, view) in required_array(views, "views", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        accepted_versions.insert(
            required_str(view, "stableId", "viewReceiptChainProof.views[]")?.to_string(),
            required_u64(view, "acceptedViewVersion", "viewReceiptChainProof.views[]")?,
        );
    }

    for (index, receipt) in required_array(views, "tombstoneReceipts", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let receipt = receipt.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[{index}] must be an object"
            ))
        })?;
        let stable_id = required_str(
            receipt,
            "stableId",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        let expected_view_version = required_u64(
            receipt,
            "expectedViewVersion",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        let accepted_view_version = accepted_versions.get(stable_id).ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[{index}] references unknown accepted view {stable_id}"
            ))
        })?;
        if expected_view_version != *accepted_view_version {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[{index}].expectedViewVersion mismatch: expected={accepted_view_version} actual={expected_view_version}"
            )));
        }
    }

    Ok(())
}

fn require_bootstrap_view_receipt_hashes_match_views(
    bootstrap: &serde_json::Map<String, Value>,
    views: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let bootstrap_hashes = required_array(
        bootstrap,
        "viewVersionReceiptHashes",
        "queryGraphBootstrapProof",
    )?;
    let view_count = required_u64(views, "viewCount", "viewReceiptChainProof")?;
    if view_count == 0 {
        if bootstrap_hashes.is_empty() {
            return Ok(());
        }
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "queryGraphBootstrapProof.viewVersionReceiptHashes must be empty when viewReceiptChainProof.viewCount is 0"
                .to_string(),
        ));
    }

    require_hash_array(
        bootstrap,
        "viewVersionReceiptHashes",
        "queryGraphBootstrapProof",
    )?;
    let accepted_views = required_array(views, "views", "viewReceiptChainProof")?;
    if bootstrap_hashes.len() != accepted_views.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "queryGraphBootstrapProof.viewVersionReceiptHashes length mismatch with viewReceiptChainProof.views[].acceptedReceiptHash: expected={} actual={}",
            accepted_views.len(),
            bootstrap_hashes.len()
        )));
    }

    let bootstrap_hashes = bootstrap_hashes
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let mut accepted_hashes = BTreeSet::new();
    for (index, view) in accepted_views.iter().enumerate() {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        accepted_hashes.insert(require_hash_str(
            view,
            "acceptedReceiptHash",
            "viewReceiptChainProof.views[]",
        )?);
    }
    if bootstrap_hashes != accepted_hashes {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "queryGraphBootstrapProof.viewVersionReceiptHashes must match viewReceiptChainProof.views[].acceptedReceiptHash exactly"
                .to_string(),
        ));
    }

    Ok(())
}

fn require_view_receipt_chain_evidence(
    views: &serde_json::Map<String, Value>,
) -> lakecat_core::LakeCatResult<()> {
    let view_count = required_u64(views, "viewCount", "viewReceiptChainProof")?;
    if view_count == 0 {
        return Ok(());
    }

    let accepted_views = required_array(views, "views", "viewReceiptChainProof")?;
    if accepted_views.len() != view_count as usize {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "viewReceiptChainProof.views length mismatch: expected={view_count} actual={}",
            accepted_views.len()
        )));
    }

    let mut accepted_receipt_chain_hashes = Vec::with_capacity(accepted_views.len());
    for (index, view) in accepted_views.iter().enumerate() {
        let view = view.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must be an object"
            ))
        })?;
        require_non_empty_str(view, "stableId", "viewReceiptChainProof.views[]")?;
        require_non_empty_str(view, "warehouse", "viewReceiptChainProof.views[]")?;
        let namespace = required_array(view, "namespace", "viewReceiptChainProof.views[]")?;
        if namespace.is_empty()
            || namespace
                .iter()
                .any(|component| !component.as_str().is_some_and(|value| !value.is_empty()))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "viewReceiptChainProof.views[].namespace must contain namespace components"
                    .to_string(),
            ));
        }
        require_non_empty_str(view, "name", "viewReceiptChainProof.views[]")?;
        let view_version = required_u64(view, "viewVersion", "viewReceiptChainProof.views[]")?;
        let accepted_view_version =
            required_u64(view, "acceptedViewVersion", "viewReceiptChainProof.views[]")?;
        if view_version == 0 || view_version != accepted_view_version {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[{index}] must prove accepted view version: viewVersion={view_version} acceptedViewVersion={accepted_view_version}"
            )));
        }
        require_hash_str(view, "acceptedReceiptHash", "viewReceiptChainProof.views[]")?;
        require_positive_u64(view, "graphEvents", "viewReceiptChainProof.views[]")?;
        accepted_receipt_chain_hashes.push((
            required_str(view, "stableId", "viewReceiptChainProof.views[]")?.to_string(),
            accepted_view_version,
            require_hash_str(
                view,
                "acceptedReceiptChainHash",
                "viewReceiptChainProof.views[]",
            )?
            .to_string(),
        ));
        require_hash_array(view, "replayEventHashes", "viewReceiptChainProof.views[]")?;
        require_hash_array(view, "openLineageHashes", "viewReceiptChainProof.views[]")?;
    }

    let mut tombstoned_views = std::collections::HashSet::new();
    let mut tombstone_receipt_hashes = BTreeSet::new();
    for (index, receipt) in required_array(views, "tombstoneReceipts", "viewReceiptChainProof")?
        .iter()
        .enumerate()
    {
        let receipt = receipt.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.tombstoneReceipts[{index}] must be an object"
            ))
        })?;
        require_hash_array(
            receipt,
            "receiptHashes",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        for hash in required_array(
            receipt,
            "receiptHashes",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )? {
            if let Some(hash) = hash.as_str() {
                tombstone_receipt_hashes.insert(hash.to_string());
            }
        }
        require_hash_array(
            receipt,
            "replayEventHashes",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        require_hash_array(
            receipt,
            "openLineageHashes",
            "viewReceiptChainProof.tombstoneReceipts[]",
        )?;
        if let (Some(stable_id), Some(expected_version)) = (
            receipt.get("stableId").and_then(Value::as_str),
            receipt.get("expectedViewVersion").and_then(Value::as_u64),
        ) {
            tombstoned_views.insert((stable_id.to_string(), expected_version));
        }
    }

    let receipt_chains = required_array(views, "receiptChains", "viewReceiptChainProof")?;
    if receipt_chains.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "viewReceiptChainProof.receiptChains must contain verified receipt-chain evidence"
                .to_string(),
        ));
    }
    let mut verified_chain_hashes = std::collections::HashSet::new();
    let mut chain_receipt_hashes = BTreeSet::new();
    for (index, chain) in receipt_chains.iter().enumerate() {
        let chain = chain.as_object().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{index}] must be an object"
            ))
        })?;
        require_non_empty_str(chain, "warehouse", "viewReceiptChainProof.receiptChains[]")?;
        let namespace =
            required_array(chain, "namespace", "viewReceiptChainProof.receiptChains[]")?;
        if namespace.is_empty()
            || namespace
                .iter()
                .any(|component| !component.as_str().is_some_and(|value| !value.is_empty()))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "viewReceiptChainProof.receiptChains[].namespace must contain namespace components"
                    .to_string(),
            ));
        }
        let verified_chain_count = require_positive_u64(
            chain,
            "verifiedChainCount",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        let receipt_hashes = required_array(
            chain,
            "receiptHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        require_hash_array(
            chain,
            "receiptHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        for receipt_hash in receipt_hashes {
            if let Some(receipt_hash) = receipt_hash.as_str() {
                chain_receipt_hashes.insert(receipt_hash.to_string());
            }
        }
        let chain_hashes = required_array(
            chain,
            "chainHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        require_hash_array(
            chain,
            "chainHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        if chain_hashes.len() as u64 != verified_chain_count {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{index}].verifiedChainCount mismatch: expected={} actual={verified_chain_count}",
                chain_hashes.len()
            )));
        }
        if receipt_hashes.len() < chain_hashes.len() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.receiptChains[{index}].receiptHashes must cover every verified chain hash"
            )));
        }
        for chain_hash in chain_hashes {
            if let Some(chain_hash) = chain_hash.as_str() {
                verified_chain_hashes.insert(chain_hash.to_string());
            }
        }
        require_hash_array(
            chain,
            "replayEventHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
        require_hash_array(
            chain,
            "openLineageHashes",
            "viewReceiptChainProof.receiptChains[]",
        )?;
    }
    for (stable_id, accepted_view_version, accepted_chain_hash) in accepted_receipt_chain_hashes {
        if !verified_chain_hashes.contains(&accepted_chain_hash) {
            if tombstoned_views.contains(&(stable_id.clone(), accepted_view_version)) {
                continue;
            }
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "viewReceiptChainProof.views[].acceptedReceiptChainHash {accepted_chain_hash} is not covered by receiptChains[].chainHashes"
            )));
        }
    }
    if !tombstone_receipt_hashes.is_subset(&chain_receipt_hashes) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "viewReceiptChainProof.tombstoneReceipts[].receiptHashes must be covered by receiptChains[].receiptHashes"
                .to_string(),
        ));
    }

    Ok(())
}

fn qglake_management_replay_line(drain: &LineageDrainResponse) -> Option<String> {
    let storage_profile_upsert = qglake_drain_event(drain, "storage-profile.upserted")?;
    let credential_root = qglake_storage_profile_upsert_line(storage_profile_upsert)?;
    Some(format!(
        "management replay servers={} projects={} warehouses={} policies={} storage_profiles={} storage_profile_upserts={} credential_root={}",
        qglake_drain_event(drain, "server.listed")?
            .server_count
            .unwrap_or_default(),
        qglake_drain_event(drain, "project.listed")?
            .project_count
            .unwrap_or_default(),
        qglake_drain_event(drain, "warehouse.listed")?
            .warehouse_count
            .unwrap_or_default(),
        qglake_drain_event(drain, "policy-binding.listed")?.policy_binding_count,
        qglake_drain_event(drain, "storage-profile.listed")?
            .storage_profile_count
            .unwrap_or_default(),
        usize::from(storage_profile_upsert.storage_profile_id.is_some()),
        credential_root
    ))
}

fn qglake_storage_profile_upsert_line(event: &LineageDrainEventSummary) -> Option<String> {
    let profile_id = event.storage_profile_id.as_deref()?.trim();
    let provider = event.storage_profile_provider.as_deref()?.trim();
    let issuance_mode = event.storage_profile_issuance_mode.as_deref()?.trim();
    let location_prefix_hash = event
        .storage_profile_location_prefix_hash
        .as_deref()?
        .trim();
    if profile_id.is_empty()
        || provider.is_empty()
        || issuance_mode.is_empty()
        || !is_sha256_hash(location_prefix_hash)
    {
        return None;
    }
    let secret_ref = if event.storage_profile_secret_ref_present? {
        let provider = event
            .storage_profile_secret_ref_provider
            .as_deref()
            .filter(|provider| !provider.trim().is_empty())
            .unwrap_or("unknown");
        let hash = event
            .storage_profile_secret_ref_hash
            .as_deref()
            .filter(|hash| is_sha256_hash(hash))
            .unwrap_or("missing");
        format!("{provider}:secret_ref_hash={hash}")
    } else {
        if event.storage_profile_secret_ref_provider.is_some()
            || event.storage_profile_secret_ref_hash.is_some()
        {
            return None;
        }
        "none".to_string()
    };
    Some(format!(
        "{profile_id}:{provider}:{issuance_mode}:location_prefix_hash={location_prefix_hash}:secret_ref={secret_ref}"
    ))
}

fn qglake_replay_verification_json(
    verification: &QueryGraphBootstrapVerification,
    scan_replay: Option<String>,
    management_replay: Option<String>,
    credential_replay: Option<String>,
    table_commit_history_replay: Option<String>,
    replay_evidence: Value,
) -> Value {
    json!({
        "schema-version": "lakecat.qglake.replay-verification.v1",
        "status": "verified",
        "bundle-hash": verification.bundle_hash,
        "graph-hash": verification.graph_hash,
        "open-lineage-hash": verification.open_lineage_hash,
        "querygraph-import-hash": verification.querygraph_import_hash,
        "table-count": verification.table_count,
        "view-count": verification.view_count,
        "verified-tables": verification.verified_tables,
        "verified-views": verification.verified_views,
        "standards": verification.standards,
        "scan-replay": scan_replay,
        "management-replay": management_replay,
        "credential-replay": credential_replay,
        "table-commit-history-replay": table_commit_history_replay,
        "replay-evidence": replay_evidence,
    })
}

fn qglake_replay_evidence_json(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
    verification: &QueryGraphBootstrapVerification,
) -> Value {
    json!({
        "requestIdentity": qglake_request_identity_replay_evidence_json(drain),
        "queryGraphBootstrap": qglake_querygraph_bootstrap_replay_evidence_json(drain),
        "scan": qglake_scan_replay_evidence_json(drain),
        "management": qglake_management_replay_evidence_json(drain),
        "credentials": qglake_credential_replay_evidence_json(drain, principal),
        "tableCommitHistory": qglake_table_commit_history_replay_evidence_json(drain),
        "views": qglake_view_replay_evidence_json(drain, verification),
    })
}

fn qglake_request_identity_replay_evidence_json(drain: &LineageDrainResponse) -> Option<Value> {
    Some(json!({
        "principalSubject": drain.principal_subject.as_deref()?,
        "principalKind": drain.principal_kind.as_deref()?,
        "requestIdentitySource": drain.request_identity_source.as_deref()?,
        "authorizationReceiptHash": drain.authorization_receipt_hash.as_deref()?,
        "requestIdentityState": drain.request_identity_state.as_deref()?,
        "typedidEnvelopeHash": drain.typedid_envelope_hash.as_deref(),
        "typedidProofHash": drain.typedid_proof_hash.as_deref(),
    }))
}

fn qglake_querygraph_bootstrap_replay_evidence_json(drain: &LineageDrainResponse) -> Option<Value> {
    let bootstrap = qglake_drain_event(drain, "querygraph.bootstrap")?;
    Some(json!({
        "bundleHash": bootstrap.bundle_hash.as_deref(),
        "graphHash": bootstrap.graph_hash.as_deref(),
        "openLineageHash": bootstrap.open_lineage_hash.as_deref(),
        "queryGraphImportHash": bootstrap.querygraph_import_hash.as_deref(),
        "tableArtifactCount": bootstrap.table_artifact_count,
        "viewArtifactCount": bootstrap.view_artifact_count,
        "policyBindingCount": bootstrap.policy_binding_count,
        "standards": &bootstrap.standards,
        "principalSubject": bootstrap.principal_subject.as_deref(),
        "principalKind": bootstrap.principal_kind.as_deref(),
        "requestIdentitySource": bootstrap.request_identity_source.as_deref(),
        "requestIdentityState": bootstrap.request_identity_state.as_deref(),
        "authorizationReceiptHash": bootstrap.authorization_receipt_hash.as_deref(),
        "agentDelegationHash": bootstrap.agent_delegation_hash.as_deref(),
        "agentSummarySignatureHash": bootstrap.agent_summary_signature_hash.as_deref(),
        "typedidEnvelopeHash": bootstrap.typedid_envelope_hash.as_deref(),
        "typedidProofHash": bootstrap.typedid_proof_hash.as_deref(),
        "viewVersionReceiptHashes": &bootstrap.view_version_receipt_hashes,
        "replayEventHashes": &bootstrap.replay_event_hashes,
        "openLineageHashes": &bootstrap.replay_open_lineage_hashes,
    }))
}

fn qglake_scan_replay_evidence_json(drain: &LineageDrainResponse) -> Option<Value> {
    let planned = qglake_drain_event(drain, "table.scan-planned")?;
    let fetched = qglake_drain_event(drain, "table.scan-tasks-fetched")?;
    Some(json!({
        "planTaskCount": planned.scan_task_count.unwrap_or_default(),
        "planGraphEvents": planned.graph_events,
        "fileTaskCount": fetched.file_scan_task_count.unwrap_or_default(),
        "deleteFileCount": fetched.delete_file_count.unwrap_or_default(),
        "childPlanTaskCount": fetched.child_plan_task_count.unwrap_or_default(),
        "plannedReadRestriction": planned.read_restriction.as_ref(),
        "fetchedReadRestriction": fetched.read_restriction.as_ref(),
        "plannedRequestedProjection": &planned.requested_projection,
        "plannedEffectiveProjection": &planned.effective_projection,
        "plannedRequestedStatsFields": &planned.requested_stats_fields,
        "plannedEffectiveStatsFields": &planned.effective_stats_fields,
        "fetchedRequiredProjection": &fetched.required_projection,
        "fetchedEffectiveProjection": &fetched.effective_projection,
        "fetchedRequiredFilters": &fetched.required_filters,
        "plannedReplayEventHashes": &planned.replay_event_hashes,
        "fetchedReplayEventHashes": &fetched.replay_event_hashes,
        "plannedOpenLineageHashes": &planned.replay_open_lineage_hashes,
        "fetchedOpenLineageHashes": &fetched.replay_open_lineage_hashes,
    }))
}

fn qglake_management_replay_evidence_json(drain: &LineageDrainResponse) -> Option<Value> {
    let server = qglake_drain_event(drain, "server.listed")?;
    let project = qglake_drain_event(drain, "project.listed")?;
    let warehouse = qglake_drain_event(drain, "warehouse.listed")?;
    let policy = qglake_drain_event(drain, "policy-binding.listed")?;
    let storage_profile = qglake_drain_event(drain, "storage-profile.listed")?;
    let storage_profile_upsert = qglake_drain_event(drain, "storage-profile.upserted")?;
    Some(json!({
        "serverCount": server.server_count.unwrap_or_default(),
        "serverGraphEvents": server.graph_events,
        "projectCount": project.project_count.unwrap_or_default(),
        "projectGraphEvents": project.graph_events,
        "warehouseCount": warehouse.warehouse_count.unwrap_or_default(),
        "warehouseGraphEvents": warehouse.graph_events,
        "policyBindingCount": policy.policy_binding_count,
        "policyGraphEvents": policy.graph_events,
        "storageProfileCount": storage_profile.storage_profile_count.unwrap_or_default(),
        "storageProfileGraphEvents": storage_profile.graph_events,
        "serverReplayEventHashes": &server.replay_event_hashes,
        "serverOpenLineageHashes": &server.replay_open_lineage_hashes,
        "projectReplayEventHashes": &project.replay_event_hashes,
        "projectOpenLineageHashes": &project.replay_open_lineage_hashes,
        "warehouseReplayEventHashes": &warehouse.replay_event_hashes,
        "warehouseOpenLineageHashes": &warehouse.replay_open_lineage_hashes,
        "policyReplayEventHashes": &policy.replay_event_hashes,
        "policyOpenLineageHashes": &policy.replay_open_lineage_hashes,
        "storageProfileReplayEventHashes": &storage_profile.replay_event_hashes,
        "storageProfileOpenLineageHashes": &storage_profile.replay_open_lineage_hashes,
        "storageProfileUpsert": {
            "profileId": storage_profile_upsert.storage_profile_id.as_deref(),
            "provider": storage_profile_upsert.storage_profile_provider.as_deref(),
            "issuanceMode": storage_profile_upsert.storage_profile_issuance_mode.as_deref(),
            "locationPrefixHash": storage_profile_upsert.storage_profile_location_prefix_hash.as_deref(),
            "secretRefPresent": storage_profile_upsert.storage_profile_secret_ref_present.unwrap_or_default(),
            "secretRefProvider": storage_profile_upsert.storage_profile_secret_ref_provider.as_deref(),
            "secretRefHash": storage_profile_upsert.storage_profile_secret_ref_hash.as_deref(),
            "graphEvents": storage_profile_upsert.graph_events,
            "replayEventHashes": &storage_profile_upsert.replay_event_hashes,
            "openLineageHashes": &storage_profile_upsert.replay_open_lineage_hashes,
        },
    }))
}

fn qglake_credential_replay_evidence_json(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> Option<Value> {
    let restricted_subject = principal.unwrap_or("anonymous");
    let restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let restricted = qglake_credential_event(drain, restricted_subject, restricted_kind)?;
    let human = qglake_credential_event(drain, "human:qglake-operator", "human")?;
    Some(json!({
        "restricted": {
            "principalSubject": restricted.principal_subject.as_deref(),
            "principalKind": restricted.principal_kind.as_deref(),
            "credentialCount": restricted.credential_count.unwrap_or_default(),
            "rawCredentialExceptionAllowed": restricted.raw_credential_exception_allowed.unwrap_or_default(),
            "blockReason": restricted.credential_block_reason.as_deref(),
            "maxCredentialTtlSeconds": qglake_event_max_credential_ttl_seconds(restricted),
            "storageProfile": qglake_credential_storage_profile_evidence_json(restricted),
            "replayEventHashes": &restricted.replay_event_hashes,
            "openLineageHashes": &restricted.replay_open_lineage_hashes,
        },
        "trustedHuman": {
            "principalSubject": human.principal_subject.as_deref(),
            "principalKind": human.principal_kind.as_deref(),
            "credentialCount": human.credential_count.unwrap_or_default(),
            "rawCredentialExceptionAllowed": human.raw_credential_exception_allowed.unwrap_or_default(),
            "rawCredentialExceptionReason": human.raw_credential_exception_reason.as_deref(),
            "blockReason": human.credential_block_reason.as_deref(),
            "maxCredentialTtlSeconds": qglake_event_max_credential_ttl_seconds(human),
            "storageProfile": qglake_credential_storage_profile_evidence_json(human),
            "replayEventHashes": &human.replay_event_hashes,
            "openLineageHashes": &human.replay_open_lineage_hashes,
        }
    }))
}

fn qglake_event_max_credential_ttl_seconds(event: &LineageDrainEventSummary) -> Option<u64> {
    event
        .read_restriction
        .as_ref()
        .and_then(|restriction| restriction.get("max-credential-ttl-seconds"))
        .and_then(Value::as_u64)
}

fn qglake_event_read_restriction_purpose(event: &LineageDrainEventSummary) -> Option<&str> {
    event
        .read_restriction
        .as_ref()
        .and_then(|restriction| restriction.get("purpose"))
        .and_then(Value::as_str)
        .filter(|purpose| !purpose.trim().is_empty())
}

fn qglake_credential_storage_profile_evidence_json(event: &LineageDrainEventSummary) -> Value {
    json!({
        "profileId": event.storage_profile_id.as_deref(),
        "provider": event.storage_profile_provider.as_deref(),
        "issuanceMode": event.storage_profile_issuance_mode.as_deref(),
        "locationPrefixHash": event.storage_profile_location_prefix_hash.as_deref(),
        "secretRefPresent": event.storage_profile_secret_ref_present.unwrap_or_default(),
        "secretRefProvider": event.storage_profile_secret_ref_provider.as_deref(),
        "secretRefHash": event.storage_profile_secret_ref_hash.as_deref(),
        "graphEvents": event.graph_events,
    })
}

fn qglake_table_commit_history_replay_evidence_json(drain: &LineageDrainResponse) -> Option<Value> {
    let commit_history = qglake_drain_event(drain, "table.commits-listed")?;
    Some(json!({
        "commitCount": commit_history.table_commit_count.unwrap_or_default(),
        "sequenceNumbers": &commit_history.table_commit_sequence_numbers,
        "commitHashes": &commit_history.table_commit_hashes,
        "graphEvents": commit_history.graph_events,
        "replayEventHashes": &commit_history.replay_event_hashes,
        "openLineageHashes": &commit_history.replay_open_lineage_hashes,
    }))
}

fn qglake_view_replay_evidence_json(
    drain: &LineageDrainResponse,
    verification: &QueryGraphBootstrapVerification,
) -> Option<Value> {
    if verification.verified_views.is_empty() {
        return Some(json!({
            "viewCount": 0,
            "views": [],
            "tombstoneReceipts": [],
            "receiptChains": []
        }));
    }

    let views = verification
        .verified_views
        .iter()
        .map(|view_stable_id| {
            let view_replay = drain.events.iter().find(|event| {
                matches!(
                    event.event_type.as_str(),
                    "view.upserted" | "view.loaded" | "view.dropped"
                ) && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
            })?;
            Some(json!({
                "stableId": view_stable_id,
                "warehouse": view_replay.view_warehouse.as_deref(),
                "namespace": &view_replay.view_namespace,
                "name": view_replay.view_name.as_deref(),
                "viewVersion": view_replay.view_version,
                "acceptedViewVersion": verification.verified_view_versions.get(view_stable_id),
                "acceptedReceiptHash": verification.verified_view_receipt_hashes.get(view_stable_id),
                "acceptedReceiptChainHash": verification
                    .verified_view_receipt_chain_hashes
                    .get(view_stable_id),
                "eventType": view_replay.event_type,
                "expectedViewVersion": view_replay.expected_view_version,
                "graphEvents": view_replay.graph_events,
                "replayEventHashes": &view_replay.replay_event_hashes,
                "openLineageHashes": &view_replay.replay_open_lineage_hashes,
            }))
        })
        .collect::<Option<Vec<_>>>()?;

    let tombstone_receipts = drain
        .events
        .iter()
        .filter(|event| event.event_type == "view.version-receipts-listed")
        .map(|event| {
            let expected_view_version = event.view_stable_id.as_deref().and_then(|stable_id| {
                drain
                    .events
                    .iter()
                    .find(|candidate| {
                        candidate.event_type == "view.dropped"
                            && candidate.view_stable_id.as_deref() == Some(stable_id)
                    })
                    .and_then(|candidate| candidate.expected_view_version)
            });
            json!({
                "stableId": event.view_stable_id.as_deref(),
                "warehouse": event.view_warehouse.as_deref(),
                "namespace": &event.view_namespace,
                "name": event.view_name.as_deref(),
                "expectedViewVersion": expected_view_version,
                "receiptHashes": &event.view_version_receipt_hashes,
                "replayEventHashes": &event.replay_event_hashes,
                "openLineageHashes": &event.replay_open_lineage_hashes,
            })
        })
        .collect::<Vec<_>>();

    let receipt_chains = drain
        .events
        .iter()
        .filter(|event| event.event_type == "view.version-receipt-chains-listed")
        .map(|event| {
            json!({
                "warehouse": event.view_warehouse.as_deref(),
                "namespace": &event.view_namespace,
                "receiptHashes": &event.view_version_receipt_hashes,
                "chainHashes": &event.view_version_receipt_chain_hashes,
                "verifiedChainCount": event.view_version_receipt_chain_verified_count,
                "replayEventHashes": &event.replay_event_hashes,
                "openLineageHashes": &event.replay_open_lineage_hashes,
            })
        })
        .collect::<Vec<_>>();

    Some(json!({
        "viewCount": verification.view_count,
        "views": views,
        "tombstoneReceipts": tombstone_receipts,
        "receiptChains": receipt_chains,
    }))
}

fn qglake_credential_replay_line(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> Option<String> {
    let restricted_subject = principal.unwrap_or("anonymous");
    let restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let restricted = qglake_credential_event(drain, restricted_subject, restricted_kind)?;
    let human = qglake_credential_event(drain, "human:qglake-operator", "human")?;
    if restricted.credential_block_reason.as_deref()
        != Some(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
        || human.raw_credential_exception_reason.as_deref()
            != Some(QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON)
    {
        return None;
    }
    let restricted_profile = qglake_credential_storage_profile_line(restricted)?;
    let human_profile = qglake_credential_storage_profile_line(human)?;
    let restricted_ttl = qglake_event_max_credential_ttl_seconds(restricted)?;
    let human_ttl = qglake_event_max_credential_ttl_seconds(human)?;
    Some(format!(
        "credential replay restricted=blocked:sail-planned-read-required restricted_count={} restricted_ttl={} restricted_profile={} human=allowed:trusted-human-audited-raw human_count={} human_ttl={} human_profile={}",
        restricted.credential_count.unwrap_or_default(),
        restricted_ttl,
        restricted_profile,
        human.credential_count.unwrap_or_default(),
        human_ttl,
        human_profile
    ))
}

fn qglake_credential_storage_profile_line(event: &LineageDrainEventSummary) -> Option<String> {
    let profile_id = event.storage_profile_id.as_deref()?.trim();
    let provider = event.storage_profile_provider.as_deref()?.trim();
    let issuance_mode = event.storage_profile_issuance_mode.as_deref()?.trim();
    let location_prefix_hash = event
        .storage_profile_location_prefix_hash
        .as_deref()?
        .trim();
    let graph_events = event.graph_events;
    if profile_id.is_empty()
        || provider.is_empty()
        || issuance_mode.is_empty()
        || !is_sha256_hash(location_prefix_hash)
        || graph_events == 0
    {
        return None;
    }
    let secret_ref = if event.storage_profile_secret_ref_present? {
        let provider = event
            .storage_profile_secret_ref_provider
            .as_deref()
            .filter(|provider| !provider.trim().is_empty())
            .unwrap_or("unknown");
        let hash = event
            .storage_profile_secret_ref_hash
            .as_deref()
            .filter(|hash| is_sha256_hash(hash))
            .unwrap_or("missing");
        format!("{provider}:secret_ref_hash={hash}")
    } else {
        if event.storage_profile_secret_ref_provider.is_some()
            || event.storage_profile_secret_ref_hash.is_some()
        {
            return None;
        }
        "none".to_string()
    };
    Some(format!(
        "{}:{}:{}:location_prefix_hash={}:secret_ref={}:graph_events={}",
        profile_id, provider, issuance_mode, location_prefix_hash, secret_ref, graph_events
    ))
}

fn qglake_table_commit_history_replay_line(drain: &LineageDrainResponse) -> Option<String> {
    let commit_history = qglake_drain_event(drain, "table.commits-listed")?;
    Some(format!(
        "table commit history commits={} sequences={} hashes={} graph_events={}",
        commit_history.table_commit_count.unwrap_or_default(),
        join_u64s(&commit_history.table_commit_sequence_numbers),
        commit_history.table_commit_hashes.join(","),
        commit_history.graph_events
    ))
}

fn qglake_scan_replay_line(drain: &LineageDrainResponse) -> Option<String> {
    let planned = qglake_drain_event(drain, "table.scan-planned")?;
    let fetched = qglake_drain_event(drain, "table.scan-tasks-fetched")?;
    let planned_ttl = qglake_event_max_credential_ttl_seconds(planned)?;
    let fetched_ttl = qglake_event_max_credential_ttl_seconds(fetched)?;
    let planned_purpose = qglake_event_read_restriction_purpose(planned)?;
    let fetched_purpose = qglake_event_read_restriction_purpose(fetched)?;
    Some(format!(
        "scan replay plan_tasks={} plan_graph_events={} planned_ttl={} planned_purpose={} file_tasks={} delete_files={} child_plan_tasks={} fetched_ttl={} fetched_purpose={}",
        planned.scan_task_count.unwrap_or_default(),
        planned.graph_events,
        planned_ttl,
        planned_purpose,
        fetched.file_scan_task_count.unwrap_or_default(),
        fetched.delete_file_count.unwrap_or_default(),
        fetched.child_plan_task_count.unwrap_or_default(),
        fetched_ttl,
        fetched_purpose
    ))
}

fn qglake_drain_event<'a>(
    drain: &'a LineageDrainResponse,
    event_type: &str,
) -> Option<&'a LineageDrainEventSummary> {
    drain
        .events
        .iter()
        .find(|event| event.event_type == event_type)
}

fn qglake_credential_event<'a>(
    drain: &'a LineageDrainResponse,
    principal_subject: &str,
    principal_kind: &str,
) -> Option<&'a LineageDrainEventSummary> {
    drain.events.iter().find(|event| {
        event.event_type == "credentials.vend-attempted"
            && event.principal_subject.as_deref() == Some(principal_subject)
            && event.principal_kind.as_deref() == Some(principal_kind)
    })
}

fn join_u64s(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

async fn drain_lineage_outbox(
    catalog: &str,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<LineageDrainResponse> {
    drain_lineage_outbox_with_identity(catalog, principal, RequestIdentityMode::Principal).await
}

async fn drain_lineage_outbox_with_identity(
    catalog: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<LineageDrainResponse> {
    post_json_with_identity::<_, LineageDrainResponse>(
        catalog,
        "/management/v1/lineage/drain",
        principal,
        identity_mode,
        "lineage drain",
        &json!({}),
    )
    .await
}

async fn get_json<T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    label: &str,
) -> lakecat_core::LakeCatResult<T> {
    get_json_with_identity(
        catalog,
        path,
        principal,
        RequestIdentityMode::Principal,
        label,
    )
    .await
}

async fn get_json_with_identity<T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
) -> lakecat_core::LakeCatResult<T> {
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.get(endpoint);
    request = identity_mode.apply(request, principal);
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })?;
    decode_json_response(response, label).await
}

async fn put_json<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<T> {
    put_json_with_identity(
        catalog,
        path,
        principal,
        RequestIdentityMode::Principal,
        label,
        body,
    )
    .await
}

async fn put_json_with_identity<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<T> {
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.put(endpoint).json(body);
    request = identity_mode.apply(request, principal);
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })?;
    decode_json_response(response, label).await
}

async fn post_json_with_identity<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<T> {
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.post(endpoint).json(body);
    request = identity_mode.apply(request, principal);
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })?;
    decode_json_response(response, label).await
}

async fn post_json_with_identity_and_idempotency<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    idempotency_key: &str,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<T> {
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client
        .post(endpoint)
        .header("x-lakecat-idempotency-key", idempotency_key)
        .json(body);
    request = identity_mode.apply(request, principal);
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })?;
    decode_json_response(response, label).await
}

async fn post_json_or_conflict_with_identity<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<Option<T>> {
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.post(endpoint).json(body);
    request = identity_mode.apply(request, principal);
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })?;
    let status = response.status();
    let body = response.bytes().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to read {label} response: {err}"))
    })?;
    if status == reqwest::StatusCode::CONFLICT {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} failed with HTTP {status}: {}",
            String::from_utf8_lossy(&body)
        )));
    }
    serde_json::from_slice(&body).map(Some).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "LakeCat {label} response is not the expected JSON payload: {err}"
        ))
    })
}

async fn delete_with_identity(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.delete(endpoint);
    request = identity_mode.apply(request, principal);
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })?;
    let status = response.status();
    let body = response.bytes().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to read {label} response: {err}"))
    })?;
    if !status.is_success() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} failed with HTTP {status}: {}",
            String::from_utf8_lossy(&body)
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum RequestIdentityMode {
    Principal,
    AgentDid,
}

impl RequestIdentityMode {
    fn apply(
        self,
        request: reqwest::RequestBuilder,
        principal: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let Some(principal) = principal else {
            return request;
        };
        match self {
            Self::Principal => request.header("x-lakecat-principal", principal),
            Self::AgentDid => request
                .header("x-lakecat-agent-did", principal)
                .header(
                    "x-lakecat-agent-delegation",
                    format!("qglake-fixture-delegation:{principal}"),
                )
                .header(
                    "x-lakecat-agent-summary-signature",
                    format!("qglake-fixture-summary:{principal}"),
                ),
        }
    }
}

async fn decode_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
    label: &str,
) -> lakecat_core::LakeCatResult<T> {
    let status = response.status();
    let body = response.bytes().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to read {label} response: {err}"))
    })?;
    if !status.is_success() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} failed with HTTP {status}: {}",
            String::from_utf8_lossy(&body)
        )));
    }
    serde_json::from_slice(&body).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "LakeCat {label} response is not the expected JSON payload: {err}"
        ))
    })
}

fn print_json<T: Serialize>(value: &T) -> lakecat_core::LakeCatResult<()> {
    let pretty = serde_json::to_string_pretty(value).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to encode JSON response: {err}"))
    })?;
    println!("{pretty}");
    Ok(())
}

async fn qglake_fixture(
    catalog: String,
    warehouse: String,
    namespace: Vec<String>,
    table: String,
    location: String,
    metadata_location: String,
    output: PathBuf,
    drain_output: Option<PathBuf>,
    principal: Option<String>,
) -> lakecat_core::LakeCatResult<()> {
    let principal = principal.as_deref();
    let identity_mode = RequestIdentityMode::AgentDid;
    let namespace_path = namespace.join(".");
    let server = "lakecat-local";
    let project = "default";
    let storage_profile = format!("{table}-local");
    let policy = format!("{table}-agent-read");

    let _: ServerResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/servers/{server}"),
        principal,
        identity_mode,
        "server upsert",
        &UpsertServerRequest {
            display_name: Some("Local LakeCat".to_string()),
            endpoint_url: Some(catalog.clone()),
            properties: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;
    verify_qglake_server_list(&catalog, server, principal, identity_mode).await?;

    let _: ProjectResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/projects/{project}"),
        principal,
        identity_mode,
        "project upsert",
        &UpsertProjectRequest {
            server_id: Some(server.to_string()),
            display_name: Some("QGLake Project".to_string()),
            properties: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;
    verify_qglake_project_list(&catalog, project, principal, identity_mode).await?;

    let _: WarehouseResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/warehouses/{warehouse}"),
        principal,
        identity_mode,
        "warehouse upsert",
        &UpsertWarehouseRequest {
            project_id: Some(project.to_string()),
            storage_root: Some(location.clone()),
            properties: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;
    verify_qglake_warehouse_list(&catalog, &warehouse, principal, identity_mode).await?;

    ensure_qglake_namespace(&catalog, &namespace, principal, identity_mode).await?;
    ensure_qglake_table(
        &catalog,
        &namespace_path,
        &namespace,
        &table,
        &location,
        &metadata_location,
        principal,
        identity_mode,
    )
    .await?;
    verify_qglake_table_commit_history(
        &catalog,
        &warehouse,
        &namespace_path,
        &namespace,
        &table,
        principal,
        identity_mode,
    )
    .await?;
    let _: StorageProfileResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/warehouses/{warehouse}/storage-profiles/{storage_profile}"),
        principal,
        identity_mode,
        "storage profile upsert",
        &UpsertStorageProfileRequest {
            location_prefix: location.clone(),
            provider: "file".to_string(),
            issuance_mode: "local-file-no-secret".to_string(),
            secret_ref: None,
            public_config: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;

    verify_qglake_storage_profile_list(
        &catalog,
        &warehouse,
        &storage_profile,
        principal,
        identity_mode,
    )
    .await?;

    let _: PolicyBindingResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/warehouses/{warehouse}/policies/{policy}"),
        principal,
        identity_mode,
        "policy upsert",
        &UpsertPolicyBindingRequest {
            namespace: Some(namespace.clone()),
            table: Some(table.clone()),
            enforced: true,
            odrl: qglake_odrl_policy(&table),
        },
    )
    .await?;
    verify_qglake_policy_list(&catalog, &warehouse, &policy, principal, identity_mode).await?;

    verify_qglake_governed_scan(
        &catalog,
        &namespace_path,
        &table,
        &location,
        principal,
        identity_mode,
    )
    .await?;
    verify_qglake_credentials_blocked(&catalog, &namespace_path, &table, principal, identity_mode)
        .await?;
    verify_qglake_trusted_human_credentials(&catalog, &namespace_path, &table, &location).await?;
    let view = "active_customers_view";
    let view_version = ensure_qglake_transient_view(
        &catalog,
        &warehouse,
        &namespace_path,
        &namespace,
        view,
        &table,
        principal,
        identity_mode,
    )
    .await?;
    let (bundle, verification) =
        fetch_bootstrap_bundle_with_identity(&catalog, principal, identity_mode).await?;
    verify_qglake_bootstrap_bundle(&bundle, &namespace, &table)?;
    write_bootstrap_bundle(&output, &bundle, &verification)?;
    drop_qglake_transient_view(
        &catalog,
        &warehouse,
        &namespace_path,
        view,
        view_version,
        principal,
        identity_mode,
    )
    .await?;
    verify_qglake_view_receipt_chains(
        &catalog,
        &warehouse,
        &namespace_path,
        &namespace,
        view,
        principal,
        identity_mode,
    )
    .await?;
    let drain = drain_lineage_outbox_with_identity(&catalog, principal, identity_mode).await?;
    if let Some(drain_output) = drain_output.as_ref() {
        write_json_file(drain_output, &drain, "lineage drain response")?;
        println!("wrote lineage drain response to {}", drain_output.display());
    }
    verify_qglake_lineage_drain(
        &drain,
        &verification,
        principal,
        qglake_policy_binding_count(&bundle),
    )?;
    println!("drained {} lineage/outbox event(s)", drain.delivered);
    Ok(())
}

async fn ensure_qglake_namespace(
    catalog: &str,
    namespace: &[String],
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    if post_json_or_conflict_with_identity::<_, NamespaceResponse>(
        catalog,
        "/catalog/v1/namespaces",
        principal,
        identity_mode,
        "namespace create",
        &CreateNamespaceRequest {
            namespace: namespace.to_vec(),
        },
    )
    .await?
    .is_some()
    {
        return Ok(());
    }

    let namespaces = get_json_with_identity::<ListNamespacesResponse>(
        catalog,
        "/catalog/v1/namespaces",
        principal,
        identity_mode,
        "namespace list",
    )
    .await?;
    if namespace_list_contains(&namespaces, namespace) {
        return Ok(());
    }
    Err(lakecat_core::LakeCatError::InvalidArgument(format!(
        "namespace create conflicted but {} was not present in namespace list",
        namespace.join(".")
    )))
}

async fn ensure_qglake_table(
    catalog: &str,
    namespace_path: &str,
    namespace: &[String],
    table: &str,
    location: &str,
    metadata_location: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = post_json_or_conflict_with_identity::<_, LoadTableResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables"),
        principal,
        identity_mode,
        "table create",
        &CreateTableRequest {
            name: table.to_string(),
            location: location.to_string(),
            metadata_location: Some(metadata_location.to_string()),
            metadata: qglake_table_metadata(location, metadata_location)?,
        },
    )
    .await?;
    if let Some(response) = response {
        return verify_qglake_existing_table(&response, namespace, table, metadata_location);
    }

    let response = get_json_with_identity::<LoadTableResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}"),
        principal,
        identity_mode,
        "table load",
    )
    .await?;
    verify_qglake_existing_table(&response, namespace, table, metadata_location)
}

async fn ensure_qglake_transient_view(
    catalog: &str,
    warehouse: &str,
    namespace_path: &str,
    namespace: &[String],
    view: &str,
    table: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<u64> {
    let response = put_json_with_identity::<_, ViewResponse>(
        catalog,
        &format!("/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/views/{view}"),
        principal,
        identity_mode,
        "transient view upsert",
        &UpsertViewRequest {
            sql: format!(
                "select event_id from {}.{} where event_type = 'purchase'",
                namespace.join("."),
                table
            ),
            dialect: "spark-sql".to_string(),
            schema_version: Some(1),
            expected_view_version: None,
            columns: vec![lakecat_api::ViewColumnRequest {
                name: "event_id".to_string(),
                data_type: json!({"type": "long"}),
                nullable: false,
                comment: Some("Projected event identifier".to_string()),
            }],
            properties: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;
    if response.name != view || response.namespace != namespace {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view response did not match fixture target: {} {:?}",
            response.name, response.namespace
        )));
    }
    if response.view_version == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view expected a durable nonzero view version, got {}",
            response.view_version
        )));
    }
    Ok(response.view_version)
}

async fn drop_qglake_transient_view(
    catalog: &str,
    warehouse: &str,
    namespace_path: &str,
    view: &str,
    expected_view_version: u64,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    delete_with_identity(
        catalog,
        &format!(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/views/{view}?expected-view-version={expected_view_version}"
        ),
        principal,
        identity_mode,
        "transient view drop",
    )
    .await?;
    let receipts = get_json_with_identity::<ListViewVersionReceiptsResponse>(
        catalog,
        &format!(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/views/{view}/version-receipts"
        ),
        principal,
        identity_mode,
        "transient view receipt list",
    )
    .await?;
    verify_qglake_transient_view_tombstone_receipts(&receipts, view)
}

async fn verify_qglake_view_receipt_chains(
    catalog: &str,
    warehouse: &str,
    namespace_path: &str,
    namespace: &[String],
    view: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let chains = get_json_with_identity::<ListViewVersionReceiptChainsResponse>(
        catalog,
        &format!(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/view-version-receipt-chains"
        ),
        principal,
        identity_mode,
        "transient view receipt-chain list",
    )
    .await?;
    let Some(chain) = chains.chains.iter().find(|chain| {
        chain.warehouse == warehouse && chain.namespace == namespace && chain.name == view
    }) else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake receipt-chain read did not expose transient view {view}"
        )));
    };
    if !chain.tombstoned || chain.latest_operation != "drop" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake receipt-chain read did not expose transient view {view} as tombstoned"
        )));
    }
    if !chain.chain_verified
        || chain.chain_hash.is_empty()
        || chain.latest_view_version == 0
        || chain.receipt_count == 0
        || chain.receipts.is_empty()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake receipt-chain read for transient view {view} is missing a verified chain hash or versioned receipts"
        )));
    }
    let has_drop_receipt = chain.receipts.iter().any(|receipt| {
        receipt.name == view
            && receipt.operation == "drop"
            && !receipt.receipt_hash.is_empty()
            && !receipt.view_hash.is_empty()
    });
    if !has_drop_receipt {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake receipt-chain read for transient view {view} is missing a hashed drop receipt"
        )));
    }
    Ok(())
}

fn verify_qglake_transient_view_tombstone_receipts(
    receipts: &ListViewVersionReceiptsResponse,
    view: &str,
) -> lakecat_core::LakeCatResult<()> {
    let Some(drop_receipt) = receipts
        .receipts
        .iter()
        .find(|receipt| receipt.name == view && receipt.operation == "drop")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view {view} did not expose a drop tombstone receipt"
        )));
    };
    if drop_receipt.receipt_hash.is_empty() || drop_receipt.view_hash.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view {view} tombstone receipt is missing hashes"
        )));
    }
    if drop_receipt.previous_view_version != Some(drop_receipt.view_version) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view {view} tombstone receipt did not preserve the deleted view version"
        )));
    }
    Ok(())
}

fn namespace_list_contains(response: &ListNamespacesResponse, namespace: &[String]) -> bool {
    response
        .namespaces
        .iter()
        .any(|candidate| candidate == namespace)
}

fn verify_qglake_existing_table(
    response: &LoadTableResponse,
    namespace: &[String],
    table: &str,
    metadata_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    let expected = TableIdentifier {
        namespace: namespace.to_vec(),
        name: table.to_string(),
    };
    if response.identifier != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table identifier did not match fixture target: {:?}",
            response.identifier
        )));
    }
    if response.metadata_location.as_deref() != Some(metadata_location) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table metadata location did not match fixture target: {:?}",
            response.metadata_location
        )));
    }
    verify_qglake_metadata_pointer(metadata_location, &response.metadata)?;
    if response.metadata["format-version"] != json!(3) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table is not Iceberg v3 fixture metadata: {}",
            response.metadata["format-version"].clone()
        )));
    }
    if !metadata_has_field(&response.metadata, "raw_payload") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "existing QGLake table does not include restricted raw_payload column".to_string(),
        ));
    }
    if response.metadata["current-snapshot-id"].as_i64().is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "existing QGLake table does not include a current snapshot for governed scan planning"
                .to_string(),
        ));
    }
    if !metadata_has_manifest_list(&response.metadata) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "existing QGLake table does not include a snapshot manifest list for fetchScanTasks"
                .to_string(),
        ));
    }
    verify_qglake_manifest_lists(&response.metadata)?;
    Ok(())
}

fn verify_qglake_metadata_pointer(
    metadata_location: &str,
    expected_metadata: &Value,
) -> lakecat_core::LakeCatResult<()> {
    let metadata_file = file_url_path(metadata_location, "QGLake metadata location")?;
    let bytes = fs::read(&metadata_file).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table metadata location is not readable at {metadata_location}: {err}"
        ))
    })?;
    let pointer_metadata = serde_json::from_slice::<Value>(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table metadata location is not valid JSON at {metadata_location}: {err}"
        ))
    })?;
    if pointer_metadata != *expected_metadata {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "existing QGLake table metadata pointer does not match catalog metadata".to_string(),
        ));
    }
    Ok(())
}

fn metadata_has_field(metadata: &Value, field_name: &str) -> bool {
    metadata["schemas"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|schema| schema["fields"].as_array().into_iter().flatten())
        .any(|field| field["name"] == field_name)
}

fn metadata_has_manifest_list(metadata: &Value) -> bool {
    metadata["snapshots"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|snapshot| snapshot["manifest-list"].as_str().is_some())
}

fn verify_qglake_manifest_lists(metadata: &Value) -> lakecat_core::LakeCatResult<()> {
    for manifest_list in metadata["snapshots"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|snapshot| snapshot["manifest-list"].as_str())
    {
        let manifest_list_file = file_url_path(manifest_list, "QGLake manifest list")?;
        if !manifest_list_file.is_file() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "existing QGLake table manifest list is not readable at {manifest_list}"
            )));
        }
    }
    Ok(())
}

fn verify_qglake_bootstrap_bundle(
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

const QGLAKE_BOOTSTRAP_STANDARDS: &[&str] = &[
    "Iceberg REST",
    "Croissant",
    "CDIF",
    "OSI handoff",
    "ODRL",
    "Grust catalog graph",
    "OpenLineage",
];

fn verify_qglake_bootstrap_standards(
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

fn verify_qglake_querygraph_import_contract(
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

fn verify_qglake_bootstrap_projection(
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

fn verify_qglake_bootstrap_open_lineage(
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

fn verify_qglake_open_lineage_artifacts(
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

fn verify_qglake_open_lineage_table_artifact(
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

fn verify_qglake_open_lineage_view_artifact(
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

fn verify_qglake_bootstrap_graph(
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

fn require_qglake_graph_node_label<'a>(
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

fn verify_qglake_tenant_root_redaction(
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
        && !hash
            .as_str()
            .is_some_and(|value| value.starts_with("sha256:"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph {label} node {hash_field} must be sha256 hash evidence"
        )));
    }
    Ok(())
}

fn qglake_graph_has_edge(bundle: &QueryGraphBootstrap, from: &str, to: &str, label: &str) -> bool {
    bundle
        .graph
        .edges
        .iter()
        .any(|edge| edge.from == from && edge.to == to && edge.label == label)
}

async fn verify_qglake_policy_list(
    catalog: &str,
    warehouse: &str,
    policy: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListPolicyBindingsResponse>(
        catalog,
        &format!("/management/v1/warehouses/{warehouse}/policies"),
        principal,
        identity_mode,
        "qglake policy list",
    )
    .await?;
    if !response
        .policies
        .iter()
        .any(|binding| binding.policy_id == policy)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake policy list did not return expected binding {policy}"
        )));
    }
    Ok(())
}

async fn verify_qglake_server_list(
    catalog: &str,
    server: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListServersResponse>(
        catalog,
        "/management/v1/servers",
        principal,
        identity_mode,
        "qglake server list",
    )
    .await?;
    if !response
        .servers
        .iter()
        .any(|candidate| candidate.server_id == server)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake server list did not return expected server {server}"
        )));
    }
    Ok(())
}

async fn verify_qglake_project_list(
    catalog: &str,
    project: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListProjectsResponse>(
        catalog,
        "/management/v1/projects",
        principal,
        identity_mode,
        "qglake project list",
    )
    .await?;
    if !response
        .projects
        .iter()
        .any(|candidate| candidate.project_id == project)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake project list did not return expected project {project}"
        )));
    }
    Ok(())
}

async fn verify_qglake_warehouse_list(
    catalog: &str,
    warehouse: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListWarehousesResponse>(
        catalog,
        "/management/v1/warehouses",
        principal,
        identity_mode,
        "qglake warehouse list",
    )
    .await?;
    if !response
        .warehouses
        .iter()
        .any(|candidate| candidate.warehouse == warehouse)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake warehouse list did not return expected warehouse {warehouse}"
        )));
    }
    Ok(())
}

async fn verify_qglake_storage_profile_list(
    catalog: &str,
    warehouse: &str,
    storage_profile: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListStorageProfilesResponse>(
        catalog,
        &format!("/management/v1/warehouses/{warehouse}/storage-profiles"),
        principal,
        identity_mode,
        "qglake storage profile list",
    )
    .await?;
    if !response
        .storage_profiles
        .iter()
        .any(|profile| profile.profile_id == storage_profile)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake storage profile list did not return expected profile {storage_profile}"
        )));
    }
    Ok(())
}

async fn verify_qglake_table_commit_history(
    catalog: &str,
    warehouse: &str,
    namespace_path: &str,
    namespace: &[String],
    table: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let _: CommitTableResponse = post_json_with_identity_and_idempotency(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/commit"),
        principal,
        identity_mode,
        &format!("qglake:{warehouse}:{namespace_path}:{table}:commit-history"),
        "qglake table commit-history probe commit",
        &CommitTableRequest {
            requirements: Vec::new(),
            updates: Vec::new(),
            metadata_location: None,
            metadata: None,
        },
    )
    .await?;
    let response = get_json_with_identity::<ListTableCommitRecordsResponse>(
        catalog,
        &format!("/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/tables/{table}/commits"),
        principal,
        identity_mode,
        "qglake table commit history",
    )
    .await?;
    let Some(record) = response.commits.iter().find(|record| {
        record.warehouse == warehouse
            && record.namespace.as_slice() == namespace
            && record.table == table
    }) else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history did not expose a pointer-log record for {warehouse}.{namespace_path}.{table}"
        )));
    };
    verify_qglake_table_commit_record_evidence(record, warehouse, namespace_path, table)
}

fn verify_qglake_table_commit_record_evidence(
    record: &lakecat_api::TableCommitRecordResponse,
    warehouse: &str,
    namespace_path: &str,
    table: &str,
) -> lakecat_core::LakeCatResult<()> {
    if record.sequence_number == 0
        || record.request_hash.is_empty()
        || record.response_hash.is_empty()
        || record.commit_hash.is_empty()
        || record
            .idempotency_key_sha256
            .as_deref()
            .map_or(true, str::is_empty)
        || record.principal_subject.is_empty()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history for {warehouse}.{namespace_path}.{table} is missing compact pointer-log evidence"
        )));
    }
    if record.format_version != Some(3) || record.snapshot_id.is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history for {warehouse}.{namespace_path}.{table} is missing Iceberg format/snapshot summary evidence"
        )));
    }
    Ok(())
}

async fn verify_qglake_governed_scan(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    table_location: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let plan = post_json_with_identity::<_, PlanTableScanResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/plan"),
        principal,
        identity_mode,
        "qglake governed scan plan",
        &PlanTableScanRequest {
            select: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
                "raw_payload".to_string(),
            ],
            stats_fields: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
                "raw_payload".to_string(),
            ],
            case_sensitive: Some(true),
            ..empty_scan_request()
        },
    )
    .await?;
    verify_qglake_scan_plan(&plan)?;
    verify_qglake_fetch_scan_tasks(
        catalog,
        namespace_path,
        table,
        table_location,
        principal,
        identity_mode,
        &plan,
    )
    .await
}

fn empty_scan_request() -> PlanTableScanRequest {
    PlanTableScanRequest {
        projection: Vec::new(),
        select: Vec::new(),
        filters: Vec::new(),
        filter: None,
        limit: None,
        snapshot_id: None,
        case_sensitive: None,
        use_snapshot_schema: None,
        start_snapshot_id: None,
        end_snapshot_id: None,
        stats_fields: Vec::new(),
    }
}

fn verify_qglake_scan_plan(plan: &PlanTableScanResponse) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("scan plan", &plan.planned_by)?;
    if plan.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not expose an Iceberg REST plan-task token".to_string(),
        ));
    }
    if !plan
        .lakecat_plan_tasks
        .iter()
        .any(|task| task["task-type"] == json!("manifest-list"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not include a manifest-list task".to_string(),
        ));
    }
    let extension = plan
        .residual_filter
        .as_ref()
        .and_then(|filter| filter.get("lakecat:scan-request"))
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "qglake governed scan plan did not include lakecat:scan-request".to_string(),
            )
        })?;
    if extension.get("effective-projection")
        != Some(&json!(["event_id", "occurred_at", "severity"]))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed scan effective projection was not narrowed as expected: {}",
            extension
                .get("effective-projection")
                .cloned()
                .unwrap_or(Value::Null)
        )));
    }
    if extension.get("requested-stats-fields")
        != Some(&json!([
            "event_id",
            "occurred_at",
            "severity",
            "raw_payload"
        ]))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed scan requested stats fields were not preserved as expected: {}",
            extension
                .get("requested-stats-fields")
                .cloned()
                .unwrap_or(Value::Null)
        )));
    }
    for field in ["effective-stats-fields", "stats-fields"] {
        if extension.get(field) != Some(&json!(["event_id", "occurred_at", "severity"])) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed scan {field} were not narrowed as expected: {}",
                extension.get(field).cloned().unwrap_or(Value::Null)
            )));
        }
    }
    verify_qglake_plan_or_fetch_read_restriction(
        &extension["read-restriction"],
        plan.table.name.as_str(),
        "qglake governed scan",
    )?;
    Ok(())
}

fn verify_qglake_sail_planner(label: &str, planned_by: &str) -> lakecat_core::LakeCatResult<()> {
    if planned_by != "sail-rest-models" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed {label} was not planned by Sail REST models: {planned_by}"
        )));
    }
    Ok(())
}

fn qglake_policy_hash(table: &str) -> lakecat_core::LakeCatResult<String> {
    content_hash_json(&qglake_odrl_policy(table))
}

async fn verify_qglake_fetch_scan_tasks(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    table_location: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    plan: &PlanTableScanResponse,
) -> lakecat_core::LakeCatResult<()> {
    let plan_task = plan.plan_tasks.first().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not produce a plan-task token for fetch verification"
                .to_string(),
        )
    })?;
    let fetched = post_json_with_identity::<_, FetchScanTasksResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/tasks"),
        principal,
        identity_mode,
        "qglake governed scan task fetch",
        &FetchScanTasksRequest {
            plan_task: plan_task.clone(),
        },
    )
    .await?;
    verify_qglake_scan_tasks(&fetched, table_location)?;
    if fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not produce child plan-task tokens for manifest fetch verification"
                .to_string(),
        ));
    }
    for (index, child_plan_task) in fetched.plan_tasks.iter().enumerate() {
        let manifest_fetched = post_json_with_identity::<_, FetchScanTasksResponse>(
            catalog,
            &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/tasks"),
            principal,
            identity_mode,
            "qglake governed manifest scan task fetch",
            &FetchScanTasksRequest {
                plan_task: child_plan_task.clone(),
            },
        )
        .await?;
        let child_descriptor = fetched.lakecat_plan_tasks.get(index);
        if child_descriptor.and_then(|task| task["content"].as_str()) == Some("deletes") {
            verify_qglake_delete_manifest_scan_tasks(&manifest_fetched, table_location)?;
        } else {
            verify_qglake_leaf_scan_tasks(&manifest_fetched, table_location)?;
        }
    }
    Ok(())
}

fn verify_qglake_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_scan_task_common(fetched, table_location)?;
    if fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not expose a child Iceberg REST plan-task token"
                .to_string(),
        ));
    }
    if !fetched
        .lakecat_plan_tasks
        .iter()
        .any(|task| task["task-type"] == json!("manifest"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not include a manifest child task".to_string(),
        ));
    }
    if fetched.delete_files.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not expose Iceberg delete-file refs".to_string(),
        ));
    }
    if !fetched.file_scan_tasks.iter().any(|task| {
        task.get("delete-file-references")
            .and_then(Value::as_array)
            .is_some_and(|refs| !refs.is_empty())
    }) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not attach delete-file references to data tasks"
                .to_string(),
        ));
    }
    Ok(())
}

fn verify_qglake_leaf_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_scan_task_common(fetched, table_location)?;
    if !fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed manifest fetchScanTasks unexpectedly exposed {} child plan-task token(s)",
            fetched.plan_tasks.len()
        )));
    }
    if !fetched.lakecat_plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed manifest fetchScanTasks unexpectedly included {} LakeCat child task(s)",
            fetched.lakecat_plan_tasks.len()
        )));
    }
    Ok(())
}

fn verify_qglake_delete_manifest_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("delete manifest fetchScanTasks", &fetched.planned_by)?;
    if !fetched.plan_tasks.is_empty() || !fetched.lakecat_plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks unexpectedly exposed child tasks"
                .to_string(),
        ));
    }
    if !fetched.file_scan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks unexpectedly exposed data scan tasks"
                .to_string(),
        ));
    }
    if fetched.delete_files.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks produced no delete files".to_string(),
        ));
    }
    let table_prefix = format!("{}/", table_location.trim_end_matches('/'));
    for delete_file_path in fetched
        .delete_files
        .iter()
        .filter_map(|file| file.get("file-path").and_then(Value::as_str))
    {
        if !delete_file_path.starts_with(&table_prefix) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed delete file escaped table location {table_location}: {delete_file_path}"
            )));
        }
    }
    verify_qglake_fetch_restriction(fetched)?;
    Ok(())
}

fn verify_qglake_scan_task_common(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("fetchScanTasks", &fetched.planned_by)?;
    if fetched.file_scan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks produced no file scan tasks".to_string(),
        ));
    }
    let data_file_paths = fetched
        .file_scan_tasks
        .iter()
        .filter_map(|task| task.pointer("/data-file/file-path").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if data_file_paths.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks produced no data-file file paths".to_string(),
        ));
    }
    let table_prefix = format!("{}/", table_location.trim_end_matches('/'));
    for data_file_path in data_file_paths {
        if !data_file_path.starts_with(&table_prefix) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed fetchScanTasks data file escaped table location {table_location}: {data_file_path}"
            )));
        }
    }
    verify_qglake_fetch_restriction(fetched)
}

fn verify_qglake_fetch_restriction(
    fetched: &FetchScanTasksResponse,
) -> lakecat_core::LakeCatResult<()> {
    let extension = fetched
        .residual_filter
        .as_ref()
        .and_then(|filter| filter.get("lakecat:fetch-scan-tasks"))
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "qglake governed fetchScanTasks did not include lakecat:fetch-scan-tasks"
                    .to_string(),
            )
        })?;
    verify_qglake_plan_or_fetch_read_restriction(
        &extension["read-restriction"],
        fetched.table.name.as_str(),
        "qglake governed fetchScanTasks",
    )?;
    if extension["required-projection"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks required projection did not prove re-applied narrowing: {}",
            extension["required-projection"].clone()
        )));
    }
    if extension["effective-projection"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks effective projection did not prove re-applied narrowing: {}",
            extension["effective-projection"].clone()
        )));
    }
    if extension["required-filters"]
        .as_array()
        .and_then(|filters| filters.first())
        != Some(&json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        }))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks required filters did not prove re-applied row predicate: {}",
            extension["required-filters"].clone()
        )));
    }
    Ok(())
}

fn verify_qglake_plan_or_fetch_read_restriction(
    restriction: &Value,
    table: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if restriction["allowed-columns"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} allowed columns were not narrowed as expected: {}",
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
            "{label} row predicate was not enforced as expected: {}",
            restriction["row-predicate"].clone()
        )));
    }
    if restriction["purpose"] != json!("qglake-agent-demo") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} purpose was not preserved as expected: {}",
            restriction["purpose"].clone()
        )));
    }
    if restriction["max-credential-ttl-seconds"] != json!(300) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} max credential TTL was not preserved as expected: {}",
            restriction["max-credential-ttl-seconds"].clone()
        )));
    }
    let expected_policy_hash = qglake_policy_hash(table)?;
    let policy_hashes = restriction["policy-hashes"].as_array().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} read restriction did not include policy hashes"
        ))
    })?;
    if !policy_hashes
        .iter()
        .any(|hash| hash.as_str() == Some(expected_policy_hash.as_str()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} did not bind to expected ODRL policy hash {expected_policy_hash}: {}",
            restriction["policy-hashes"].clone()
        )));
    }
    Ok(())
}

async fn verify_qglake_credentials_blocked(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let credentials = get_json_with_identity::<LoadCredentialsResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/credentials"),
        principal,
        identity_mode,
        "qglake restricted credentials probe",
    )
    .await?;
    verify_qglake_credentials_response(&credentials)
}

fn verify_qglake_credentials_response(
    credentials: &LoadCredentialsResponse,
) -> lakecat_core::LakeCatResult<()> {
    if credentials.storage_credentials.is_empty() {
        return Ok(());
    }
    Err(lakecat_core::LakeCatError::InvalidArgument(format!(
        "qglake restricted table unexpectedly returned {} raw credential set(s)",
        credentials.storage_credentials.len()
    )))
}

async fn verify_qglake_trusted_human_credentials(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    let credentials = get_json_with_identity::<LoadCredentialsResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/credentials"),
        Some("human:qglake-operator"),
        RequestIdentityMode::Principal,
        "qglake trusted human credentials probe",
    )
    .await?;
    verify_qglake_trusted_human_credentials_response(&credentials, table_location)
}

fn verify_qglake_trusted_human_credentials_response(
    credentials: &LoadCredentialsResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    let Some(credential) = credentials.storage_credentials.first() else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake trusted human credentials probe returned no standard credential set"
                .to_string(),
        ));
    };
    if credential.prefix != table_location {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake trusted human credential prefix did not match table location {table_location}: {}",
            credential.prefix
        )));
    }
    if credential
        .config
        .iter()
        .any(|entry| entry.key.contains("secret") || entry.key.contains("token"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake trusted human local credentials unexpectedly exposed secret material"
                .to_string(),
        ));
    }
    Ok(())
}

fn verify_qglake_lineage_drain(
    drain: &LineageDrainResponse,
    verification: &QueryGraphBootstrapVerification,
    principal: Option<&str>,
    expected_policy_binding_count: usize,
) -> lakecat_core::LakeCatResult<()> {
    if drain.delivered == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain delivered no outbox events".to_string(),
        ));
    }
    if drain.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain emitted no lineage events".to_string(),
        ));
    }
    if drain.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain emitted no graph events".to_string(),
        ));
    }
    if !drain
        .event_types
        .iter()
        .any(|event_type| event_type == "querygraph.bootstrap")
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay querygraph.bootstrap".to_string(),
        ));
    }
    let expected_principal = principal.unwrap_or("anonymous");
    if drain.principal_subject.as_deref() != Some(expected_principal) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain read principal did not match accepted principal {expected_principal}"
        )));
    }
    let expected_principal_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    if drain.principal_kind.as_deref() != Some(expected_principal_kind) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain read principal kind did not match accepted principal kind {expected_principal_kind}"
        )));
    }
    if drain
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing SHA-256 authorization receipt hash".to_string(),
        ));
    }
    if drain
        .request_identity_source
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing request identity source".to_string(),
        ));
    }
    if drain
        .request_identity_state
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing request identity attestation state".to_string(),
        ));
    }
    verify_qglake_typedid_hash_pair(
        drain.typedid_envelope_hash.as_deref(),
        drain.typedid_proof_hash.as_deref(),
        "qglake lineage drain read",
    )?;
    let Some(bootstrap) = drain
        .events
        .iter()
        .find(|event| event.event_type == "querygraph.bootstrap")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not expose querygraph.bootstrap replay evidence".to_string(),
        ));
    };
    if bootstrap
        .bundle_hash
        .as_deref()
        .map_or(true, |hash| !is_sha256_hash(hash))
        || bootstrap
            .graph_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
        || bootstrap
            .open_lineage_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
        || bootstrap
            .querygraph_import_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing SHA-256 QueryGraph hashes".to_string(),
        ));
    }
    if bootstrap.principal_subject.as_deref() != Some(expected_principal) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain replay principal did not match accepted principal {expected_principal}"
        )));
    }
    if bootstrap.principal_kind.as_deref() != Some(expected_principal_kind) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain replay principal kind did not match accepted principal kind {expected_principal_kind}"
        )));
    }
    if bootstrap
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing SHA-256 authorization receipt hash"
                .to_string(),
        ));
    }
    if bootstrap
        .request_identity_source
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing request identity source".to_string(),
        ));
    }
    if bootstrap
        .request_identity_state
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing request identity attestation state"
                .to_string(),
        ));
    }
    if principal.is_some() {
        if bootstrap
            .agent_delegation_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence is missing SHA-256 agent delegation hash"
                    .to_string(),
            ));
        }
        if bootstrap
            .agent_summary_signature_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence is missing SHA-256 agent summary signature hash"
                    .to_string(),
            ));
        }
    }
    verify_qglake_typedid_hash_pair(
        bootstrap.typedid_envelope_hash.as_deref(),
        bootstrap.typedid_proof_hash.as_deref(),
        "qglake lineage drain bootstrap replay",
    )?;
    if bootstrap.bundle_hash.as_deref() != Some(verification.bundle_hash.as_str())
        || bootstrap.graph_hash.as_deref() != Some(verification.graph_hash.as_str())
        || bootstrap.open_lineage_hash.as_deref() != Some(verification.open_lineage_hash.as_str())
        || bootstrap.querygraph_import_hash.as_deref()
            != Some(verification.querygraph_import_hash.as_str())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.table_artifact_count == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence has no QueryGraph table artifacts".to_string(),
        ));
    }
    if bootstrap.table_artifact_count != verification.table_count
        || bootstrap.view_artifact_count != verification.view_count
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay artifact counts do not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if !verification.verified_views.is_empty()
        && (bootstrap.view_version_receipt_hashes.len() != verification.verified_views.len()
            || !qglake_has_sha256_hashes(&bootstrap.view_version_receipt_hashes))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing SHA-256 view version receipt hashes"
                .to_string(),
        ));
    }
    if !verification.verified_views.is_empty() {
        let accepted_view_receipt_hashes = verification
            .verified_view_receipt_hashes
            .values()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let replayed_view_receipt_hashes = bootstrap
            .view_version_receipt_hashes
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if accepted_view_receipt_hashes != replayed_view_receipt_hashes {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence view version receipt hashes do not match the accepted QueryGraph bundle".to_string(),
            ));
        }
    }
    if standards_set(&bootstrap.standards) != standards_set(&verification.standards) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay standards do not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.policy_binding_count != expected_policy_binding_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay policy binding count does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain bootstrap replay emitted no lineage projection".to_string(),
        ));
    }
    if !qglake_has_sha256_hashes(&bootstrap.replay_event_hashes)
        || !qglake_has_sha256_hashes(&bootstrap.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing SHA-256 sink receipt hashes"
                .to_string(),
        ));
    }
    verify_qglake_view_replay(drain, verification)?;
    verify_qglake_credential_replay(drain, principal)?;
    verify_qglake_management_list_replay(drain, expected_policy_binding_count)?;
    verify_qglake_credential_replay_matches_storage_profile_upsert(drain, principal)?;
    verify_qglake_table_commit_history_replay(drain)?;
    verify_qglake_scan_replay(drain)?;
    Ok(())
}

fn verify_qglake_scan_replay(drain: &LineageDrainResponse) -> lakecat_core::LakeCatResult<()> {
    let planned = qglake_drain_event(drain, "table.scan-planned").ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay scan planning evidence".to_string(),
        )
    })?;
    if planned.lineage_events == 0
        || planned.graph_events == 0
        || planned
            .authorization_receipt_hash
            .as_deref()
            .map_or(true, str::is_empty)
        || planned
            .request_identity_state
            .as_deref()
            .map_or(true, str::is_empty)
        || !qglake_has_sha256_hashes(&planned.replay_event_hashes)
        || !qglake_has_sha256_hashes(&planned.replay_open_lineage_hashes)
        || planned.scan_task_count.unwrap_or_default() == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing compact task, graph, or SHA-256 receipt evidence"
                .to_string(),
        ));
    }

    let fetched = qglake_drain_event(drain, "table.scan-tasks-fetched").ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay scan task fetch evidence".to_string(),
        )
    })?;
    if fetched.lineage_events == 0
        || fetched
            .authorization_receipt_hash
            .as_deref()
            .map_or(true, str::is_empty)
        || fetched
            .request_identity_state
            .as_deref()
            .map_or(true, str::is_empty)
        || !qglake_has_sha256_hashes(&fetched.replay_event_hashes)
        || !qglake_has_sha256_hashes(&fetched.replay_open_lineage_hashes)
        || fetched.file_scan_task_count.unwrap_or_default() == 0
        || fetched.delete_file_count.unwrap_or_default() == 0
        || fetched.child_plan_task_count.unwrap_or_default() == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay is missing compact file/delete task or SHA-256 receipt evidence"
                .to_string(),
        ));
    }
    verify_qglake_scan_restriction_replay(planned, fetched)?;
    Ok(())
}

fn verify_qglake_scan_restriction_replay(
    planned: &LineageDrainEventSummary,
    fetched: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    let planned_restriction = qglake_lineage_drain_read_restriction(planned, "scan planning")?;
    let fetched_restriction = qglake_lineage_drain_read_restriction(fetched, "scan task fetch")?;
    require_read_restriction_evidence(
        planned_restriction,
        "qglake lineage drain scan planning read restriction",
    )?;
    require_read_restriction_evidence(
        fetched_restriction,
        "qglake lineage drain scan task fetch read restriction",
    )?;
    for field in [
        "policy-hashes",
        "allowed-columns",
        "row-predicate",
        "purpose",
        "max-credential-ttl-seconds",
    ] {
        require_value_match(
            planned_restriction,
            field,
            required_value(
                fetched_restriction,
                field,
                "qglake lineage drain scan task fetch read restriction",
            )?,
            "qglake lineage drain scan planning read restriction",
        )?;
    }

    let fetched_allowed_columns = required_value(
        fetched_restriction,
        "allowed-columns",
        "qglake lineage drain scan task fetch read restriction",
    )?;
    let fetched_projection = Value::Array(
        fetched
            .required_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    if &fetched_projection != fetched_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay required projection does not match fetched read restriction"
                .to_string(),
        ));
    }
    let fetched_effective_projection = Value::Array(
        fetched
            .effective_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    if &fetched_effective_projection != fetched_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay effective projection does not match fetched read restriction"
                .to_string(),
        ));
    }

    let fetched_row_predicate = required_value(
        fetched_restriction,
        "row-predicate",
        "qglake lineage drain scan task fetch read restriction",
    )?;
    let expected_filters = vec![fetched_row_predicate.clone()];
    if fetched.required_filters.as_slice() != expected_filters.as_slice() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay required filters do not exactly preserve fetched row predicate"
                .to_string(),
        ));
    }
    if planned.requested_projection.is_empty() || planned.effective_projection.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing requested/effective projection evidence"
                .to_string(),
        ));
    }
    if planned.requested_projection.len() <= planned.effective_projection.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay does not prove projection narrowing"
                .to_string(),
        ));
    }
    let requested_projection = planned
        .requested_projection
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &planned.effective_projection {
        if !requested_projection.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain scan planning replay effective projection field {field} was not requested"
            )));
        }
    }
    let effective_projection = Value::Array(
        planned
            .effective_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    let planned_allowed_columns = required_value(
        planned_restriction,
        "allowed-columns",
        "qglake lineage drain scan planning read restriction",
    )?;
    if &effective_projection != planned_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay effective projection does not match planned read restriction"
                .to_string(),
        ));
    }
    if planned.requested_stats_fields.is_empty() || planned.effective_stats_fields.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing requested/effective stats-field evidence"
                .to_string(),
        ));
    }
    if planned.requested_stats_fields.len() <= planned.effective_stats_fields.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay does not prove stats-field narrowing"
                .to_string(),
        ));
    }
    let requested_stats = planned
        .requested_stats_fields
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &planned.effective_stats_fields {
        if !requested_stats.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain scan planning replay effective stats field {field} was not requested"
            )));
        }
    }
    let effective_stats = Value::Array(
        planned
            .effective_stats_fields
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    let planned_allowed_columns = required_value(
        planned_restriction,
        "allowed-columns",
        "qglake lineage drain scan planning read restriction",
    )?;
    if &effective_stats != planned_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay effective stats fields do not match planned read restriction"
                .to_string(),
        ));
    }
    Ok(())
}

fn qglake_lineage_drain_read_restriction<'a>(
    event: &'a LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a serde_json::Map<String, Value>> {
    event
        .read_restriction
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain {label} replay is missing governed read restriction evidence"
            ))
        })
}

fn verify_qglake_replay_artifacts(
    bundle: &QueryGraphBootstrap,
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<QueryGraphBootstrapVerification> {
    let verification = bundle.verify_manifest()?;
    verify_qglake_querygraph_import_contract(bundle)?;
    verify_qglake_lineage_drain(
        drain,
        &verification,
        principal,
        qglake_policy_binding_count(bundle),
    )?;
    Ok(verification)
}

fn verify_qglake_view_replay(
    drain: &LineageDrainResponse,
    verification: &QueryGraphBootstrapVerification,
) -> lakecat_core::LakeCatResult<()> {
    for view_stable_id in &verification.verified_views {
        let Some(view_replay) = drain.events.iter().find(|event| {
            matches!(
                event.event_type.as_str(),
                "view.upserted" | "view.loaded" | "view.dropped"
            ) && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
        }) else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain did not replay view evidence for {view_stable_id}"
            )));
        };
        if view_replay.graph_events == 0 || view_replay.lineage_events == 0 {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain view replay for {view_stable_id} did not emit graph and lineage projections"
            )));
        }
        if let Some(expected_version) = verification.verified_view_versions.get(view_stable_id) {
            if view_replay.view_version != Some(*expected_version) {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view replay for {view_stable_id} did not preserve accepted view version {expected_version}"
                )));
            }
        }
        if view_replay
            .view_warehouse
            .as_deref()
            .map_or(true, str::is_empty)
            || view_replay.view_namespace.is_empty()
            || view_replay.view_name.as_deref().map_or(true, str::is_empty)
            || !qglake_has_sha256_hashes(&view_replay.replay_event_hashes)
            || !qglake_has_sha256_hashes(&view_replay.replay_open_lineage_hashes)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain view replay for {view_stable_id} is missing compact identity or receipt hashes"
            )));
        }
        if drain.events.iter().any(|event| {
            event.event_type == "view.dropped"
                && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
        }) {
            let drop_replay = drain.events.iter().find(|event| {
                event.event_type == "view.dropped"
                    && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
            });
            if let (Some(drop_replay), Some(expected_version)) = (
                drop_replay,
                verification.verified_view_versions.get(view_stable_id),
            ) {
                if drop_replay.expected_view_version != Some(*expected_version) {
                    return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                        "qglake lineage drain view drop replay for {view_stable_id} did not preserve expected view version {expected_version}"
                    )));
                }
            }
            let Some(tombstone_receipts) = drain.events.iter().find(|event| {
                event.event_type == "view.version-receipts-listed"
                    && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
                    && qglake_has_sha256_hashes(&event.view_version_receipt_hashes)
            }) else {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} is missing SHA-256 tombstone receipt evidence"
                )));
            };
            if tombstone_receipts.lineage_events == 0 {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain tombstone receipt replay for {view_stable_id} emitted no lineage projection"
                )));
            }
            let Some(receipt_chain_read) = drain.events.iter().find(|event| {
                event.event_type == "view.version-receipt-chains-listed"
                    && event.view_warehouse == view_replay.view_warehouse
                    && event.view_namespace == view_replay.view_namespace
                    && qglake_has_sha256_hashes(&event.view_version_receipt_chain_hashes)
                    && event.view_version_receipt_chain_verified_count > 0
                    && qglake_has_sha256_hashes(&event.view_version_receipt_hashes)
            }) else {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} is missing SHA-256 namespace receipt-chain evidence for the accepted view namespace"
                )));
            };
            if receipt_chain_read.view_version_receipt_chain_hashes.len()
                != receipt_chain_read.view_version_receipt_chain_verified_count
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} verified-chain count does not match chain hash evidence"
                )));
            }
            if receipt_chain_read.view_version_receipt_hashes.len()
                < receipt_chain_read.view_version_receipt_chain_hashes.len()
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} receipt hashes do not cover verified chain hashes"
                )));
            }
            let tombstone_hashes = tombstone_receipts
                .view_version_receipt_hashes
                .iter()
                .collect::<BTreeSet<_>>();
            let receipt_chain_hashes = receipt_chain_read
                .view_version_receipt_hashes
                .iter()
                .collect::<BTreeSet<_>>();
            if !tombstone_hashes.is_subset(&receipt_chain_hashes) {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} tombstone receipt hashes are not covered by namespace receipt-chain evidence"
                )));
            }
            if receipt_chain_read.lineage_events == 0
                || !qglake_has_sha256_hashes(&receipt_chain_read.replay_event_hashes)
                || !qglake_has_sha256_hashes(&receipt_chain_read.replay_open_lineage_hashes)
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} is missing chain, lineage, or sink receipt hashes"
                )));
            }
        }
    }
    Ok(())
}

fn verify_qglake_credential_replay(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let expected_restricted_subject = principal.unwrap_or("anonymous");
    let expected_restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let Some(restricted_probe) =
        qglake_credential_event(drain, expected_restricted_subject, expected_restricted_kind)
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the restricted credential probe".to_string(),
        ));
    };
    if restricted_probe.credential_count != Some(0)
        || restricted_probe.raw_credential_exception_allowed != Some(false)
        || restricted_probe.credential_block_reason.as_deref()
            != Some(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
        || restricted_probe.raw_credential_exception_reason.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain restricted credential replay did not prove raw credentials were blocked"
                .to_string(),
        ));
    }
    verify_qglake_credential_lineage_projection(restricted_probe, "restricted")?;

    let Some(human_probe) = qglake_credential_event(drain, "human:qglake-operator", "human") else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the trusted human credential probe".to_string(),
        ));
    };
    if human_probe.credential_count.unwrap_or_default() == 0
        || human_probe.raw_credential_exception_allowed != Some(true)
        || human_probe.credential_block_reason.is_some()
        || human_probe.raw_credential_exception_reason.as_deref()
            != Some(QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain trusted human credential replay did not prove audited standard credential vending"
                .to_string(),
        ));
    }
    verify_qglake_credential_lineage_projection(human_probe, "trusted human")?;
    let restricted_ttl = qglake_event_max_credential_ttl_seconds(restricted_probe).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain restricted credential replay is missing max credential TTL evidence"
                .to_string(),
        )
    })?;
    let human_ttl = qglake_event_max_credential_ttl_seconds(human_probe).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain trusted human credential replay is missing max credential TTL evidence"
                .to_string(),
        )
    })?;
    if human_ttl != restricted_ttl {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain trusted human credential replay TTL cap mismatch: expected={restricted_ttl} actual={human_ttl}"
        )));
    }
    verify_qglake_credential_restriction_match(restricted_probe, human_probe)?;
    Ok(())
}

fn verify_qglake_credential_lineage_projection(
    event: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if event.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay emitted no lineage projection"
        )));
    }
    if event.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay emitted no credential-root graph projection"
        )));
    }
    if qglake_event_max_credential_ttl_seconds(event).is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing max credential TTL evidence"
        )));
    }
    let restriction = qglake_lineage_drain_read_restriction(event, &format!("{label} credential"))?;
    require_read_restriction_evidence(
        restriction,
        &format!("qglake lineage drain {label} credential read restriction"),
    )?;
    verify_qglake_credential_storage_profile_projection(event, label)?;
    if !qglake_has_sha256_hashes(&event.replay_event_hashes)
        || !qglake_has_sha256_hashes(&event.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing SHA-256 sink receipt hashes"
        )));
    }
    Ok(())
}

fn verify_qglake_credential_restriction_match(
    restricted: &LineageDrainEventSummary,
    human: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    let restricted_restriction =
        qglake_lineage_drain_read_restriction(restricted, "restricted credential")?;
    let human_restriction =
        qglake_lineage_drain_read_restriction(human, "trusted human credential")?;
    for field in [
        "policy-hashes",
        "allowed-columns",
        "row-predicate",
        "purpose",
        "max-credential-ttl-seconds",
    ] {
        require_value_match(
            restricted_restriction,
            field,
            required_value(
                human_restriction,
                field,
                "qglake lineage drain trusted human credential read restriction",
            )?,
            "qglake lineage drain restricted credential read restriction",
        )?;
    }
    Ok(())
}

fn verify_qglake_credential_storage_profile_projection(
    event: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if event
        .storage_profile_id
        .as_deref()
        .map_or(true, str::is_empty)
        || event
            .storage_profile_provider
            .as_deref()
            .map_or(true, str::is_empty)
        || event
            .storage_profile_issuance_mode
            .as_deref()
            .map_or(true, str::is_empty)
        || event
            .storage_profile_location_prefix_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
        || event.storage_profile_secret_ref_present.is_none()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing redacted storage-profile graph evidence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(false)
        && event.storage_profile_secret_ref_provider.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay carried a secret-ref provider without secret-ref presence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(false)
        && event.storage_profile_secret_ref_hash.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay carried a secret-ref hash without secret-ref presence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_provider
            .as_deref()
            .map_or(true, |provider| provider.trim().is_empty())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing secret-ref provider evidence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing secret-ref hash evidence"
        )));
    }
    Ok(())
}

fn verify_qglake_credential_replay_matches_storage_profile_upsert(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let Some(storage_profile_upsert) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.upserted")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile upsert evidence".to_string(),
        ));
    };
    let expected_restricted_subject = principal.unwrap_or("anonymous");
    let expected_restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let Some(restricted_probe) =
        qglake_credential_event(drain, expected_restricted_subject, expected_restricted_kind)
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the restricted credential probe".to_string(),
        ));
    };
    verify_qglake_credential_storage_profile_matches_upsert(
        restricted_probe,
        storage_profile_upsert,
        "restricted",
    )?;

    let Some(human_probe) = qglake_credential_event(drain, "human:qglake-operator", "human") else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the trusted human credential probe".to_string(),
        ));
    };
    verify_qglake_credential_storage_profile_matches_upsert(
        human_probe,
        storage_profile_upsert,
        "trusted human",
    )
}

fn verify_qglake_credential_storage_profile_matches_upsert(
    credential: &LineageDrainEventSummary,
    storage_profile_upsert: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if credential.storage_profile_id != storage_profile_upsert.storage_profile_id
        || credential.storage_profile_provider != storage_profile_upsert.storage_profile_provider
        || credential.storage_profile_issuance_mode
            != storage_profile_upsert.storage_profile_issuance_mode
        || credential.storage_profile_location_prefix_hash
            != storage_profile_upsert.storage_profile_location_prefix_hash
        || credential.storage_profile_secret_ref_present
            != storage_profile_upsert.storage_profile_secret_ref_present
        || credential.storage_profile_secret_ref_provider
            != storage_profile_upsert.storage_profile_secret_ref_provider
        || credential.storage_profile_secret_ref_hash
            != storage_profile_upsert.storage_profile_secret_ref_hash
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay storage-profile evidence does not match storage profile upsert replay"
        )));
    }
    Ok(())
}

fn verify_qglake_management_list_replay(
    drain: &LineageDrainResponse,
    expected_policy_binding_count: usize,
) -> lakecat_core::LakeCatResult<()> {
    let Some(policy_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "policy-binding.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay policy list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(policy_list, "policy list", true)?;
    if policy_list.policy_binding_count != expected_policy_binding_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy list replay count does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    let Some(storage_profile_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(storage_profile_list, "storage profile list", true)?;
    if storage_profile_list
        .storage_profile_count
        .unwrap_or_default()
        == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile list replay did not expose any storage profiles"
                .to_string(),
        ));
    }
    let Some(storage_profile_upsert) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.upserted")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile upsert evidence".to_string(),
        ));
    };
    verify_qglake_storage_profile_upsert_replay(storage_profile_upsert)?;
    let Some(server_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "server.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay server list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(server_list, "server list", false)?;
    if server_list.server_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain server list replay did not expose any servers".to_string(),
        ));
    }
    let Some(project_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "project.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay project list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(project_list, "project list", false)?;
    if project_list.project_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain project list replay did not expose any projects".to_string(),
        ));
    }
    let Some(warehouse_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "warehouse.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay warehouse list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(warehouse_list, "warehouse list", false)?;
    if warehouse_list.warehouse_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain warehouse list replay did not expose any warehouses".to_string(),
        ));
    }
    Ok(())
}

fn verify_qglake_table_commit_history_replay(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    let Some(commit_history) = drain
        .events
        .iter()
        .find(|event| event.event_type == "table.commits-listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay table commit history evidence".to_string(),
        ));
    };
    if commit_history.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay emitted no lineage projection"
                .to_string(),
        ));
    }
    if commit_history.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay emitted no graph projection"
                .to_string(),
        ));
    }
    if !qglake_has_sha256_hashes(&commit_history.replay_event_hashes)
        || !qglake_has_sha256_hashes(&commit_history.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing SHA-256 receipt hashes"
                .to_string(),
        ));
    }
    let commit_count = commit_history.table_commit_count.unwrap_or_default();
    if commit_count == 0
        || commit_history.table_commit_sequence_numbers.is_empty()
        || !qglake_has_sha256_hashes(&commit_history.table_commit_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
                .to_string(),
        ));
    }
    if commit_history.table_commit_sequence_numbers.len() != commit_count
        || commit_history.table_commit_hashes.len() != commit_count
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay count does not match sequence-number and commit-hash evidence"
                .to_string(),
        ));
    }
    let mut previous_sequence = 0;
    for sequence_number in &commit_history.table_commit_sequence_numbers {
        if *sequence_number == 0 || *sequence_number <= previous_sequence {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain table commit history replay sequence numbers must be positive and strictly increasing"
                    .to_string(),
            ));
        }
        previous_sequence = *sequence_number;
    }
    Ok(())
}

fn verify_qglake_storage_profile_upsert_replay(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_management_list_receipts(event, "storage profile upsert", true)?;
    if event
        .storage_profile_id
        .as_deref()
        .unwrap_or_default()
        .is_empty()
        || event
            .storage_profile_provider
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        || event
            .storage_profile_issuance_mode
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        || event
            .storage_profile_location_prefix_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
        || event.storage_profile_secret_ref_present.is_none()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay did not expose redacted credential-root evidence"
                .to_string(),
        ));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_provider
            .as_deref()
            .map_or(true, |provider| provider.trim().is_empty())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing secret-ref provider evidence"
                .to_string(),
        ));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_hash
            .as_deref()
            .map_or(true, |hash| !is_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing secret-ref hash evidence"
                .to_string(),
        ));
    }
    if event.storage_profile_secret_ref_present == Some(false)
        && (event.storage_profile_secret_ref_provider.is_some()
            || event.storage_profile_secret_ref_hash.is_some())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay carried secret-ref evidence without secret-ref presence"
                .to_string(),
        ));
    }
    Ok(())
}

fn verify_qglake_management_list_receipts(
    event: &LineageDrainEventSummary,
    label: &str,
    require_warehouse_scope: bool,
) -> lakecat_core::LakeCatResult<()> {
    if event.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay emitted no lineage projection"
        )));
    }
    if event.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay emitted no catalog graph projection"
        )));
    }
    if require_warehouse_scope
        && event
            .management_scope_warehouse
            .as_deref()
            .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing compact management scope"
        )));
    }
    if !qglake_has_sha256_hashes(&event.replay_event_hashes)
        || !qglake_has_sha256_hashes(&event.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing SHA-256 receipt hashes"
        )));
    }
    Ok(())
}

fn qglake_has_sha256_hashes(hashes: &[String]) -> bool {
    !hashes.is_empty() && hashes.iter().all(|hash| is_sha256_hash(hash))
}

fn verify_qglake_typedid_hash_pair(
    envelope_hash: Option<&str>,
    proof_hash: Option<&str>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if envelope_hash.is_some_and(|hash| !is_sha256_hash(hash)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID envelope hash must be SHA-256-shaped"
        )));
    }
    if proof_hash.is_some_and(|hash| !is_sha256_hash(hash)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID proof hash must be SHA-256-shaped"
        )));
    }
    if proof_hash.is_some() && envelope_hash.is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID proof hash requires an envelope hash"
        )));
    }
    Ok(())
}

fn standards_set(standards: &[String]) -> BTreeSet<&str> {
    standards.iter().map(String::as_str).collect()
}

fn qglake_policy_binding_count(bundle: &QueryGraphBootstrap) -> usize {
    bundle
        .tables
        .iter()
        .map(|table| table.policy_bindings.len())
        .sum()
}

fn qglake_table_metadata(
    location: &str,
    metadata_location: &str,
) -> lakecat_core::LakeCatResult<Value> {
    let metadata_file = file_url_path(metadata_location, "QGLake metadata location")?;
    let metadata_dir = metadata_file.parent().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake metadata location has no parent directory: {metadata_location}"
        ))
    })?;
    fs::create_dir_all(metadata_dir).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to create QGLake metadata directory {metadata_dir:?}: {err}"
        ))
    })?;

    let table_dir = file_url_path(location, "QGLake table location")?;
    let data_dir = table_dir.join("data");
    let delete_dir = table_dir.join("delete");
    fs::create_dir_all(&data_dir).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to create QGLake data directory {data_dir:?}: {err}"
        ))
    })?;
    fs::create_dir_all(&delete_dir).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to create QGLake delete directory {delete_dir:?}: {err}"
        ))
    })?;

    let manifest_list_path = metadata_dir.join("snap-42.avro");
    let manifest_path = metadata_dir.join("manifest-1.avro");
    let delete_manifest_path = metadata_dir.join("delete-manifest-1.avro");
    let data_file_path = data_dir.join("part-1.parquet");
    let delete_file_path = delete_dir.join("pos-delete-1.parquet");
    let manifest_list = file_path_url(&manifest_list_path, "QGLake manifest list")?;
    let manifest = file_path_url(&manifest_path, "QGLake data manifest")?;
    let delete_manifest = file_path_url(&delete_manifest_path, "QGLake delete manifest")?;
    let data_file = file_path_url(&data_file_path, "QGLake data file")?;
    let delete_file = file_path_url(&delete_file_path, "QGLake delete file")?;

    let metadata = json!({
        "format-version": 3,
        "table-uuid": "22222222-2222-2222-2222-222222222222",
        "location": location,
        "last-sequence-number": 8,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 4,
        "current-schema-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [
                {
                    "id": 1,
                    "name": "event_id",
                    "type": "string",
                    "required": true,
                    "doc": "Event identifier.",
                    "semantic-type": "https://schema.org/identifier"
                },
                {
                    "id": 2,
                    "name": "occurred_at",
                    "type": "timestamp",
                    "required": false,
                    "doc": "Event timestamp.",
                    "semantic-type": "https://schema.org/DateTime"
                },
                {
                    "id": 3,
                    "name": "severity",
                    "type": "string",
                    "required": false,
                    "doc": "Operational severity."
                },
                {
                    "id": 4,
                    "name": "raw_payload",
                    "type": "string",
                    "required": false,
                    "doc": "Raw event payload reserved for governed human/debug workflows."
                }
            ]
        }],
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 8,
            "timestamp-ms": 1710000000000_i64,
            "manifest-list": manifest_list,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{
            "timestamp-ms": 1710000000000_i64,
            "snapshot-id": 42
        }]
    });

    write_qglake_manifest_files(
        &metadata,
        &manifest_path,
        &manifest,
        &delete_manifest_path,
        &delete_manifest,
        &manifest_list_path,
        &data_file,
        &delete_file,
    )?;
    write_qglake_metadata_file(&metadata_file, &metadata)?;
    Ok(metadata)
}

fn write_qglake_metadata_file(
    metadata_file: &std::path::Path,
    metadata: &Value,
) -> lakecat_core::LakeCatResult<()> {
    let bytes = serde_json::to_vec_pretty(metadata).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QGLake table metadata JSON: {err}"
        ))
    })?;
    fs::write(metadata_file, bytes).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write QGLake table metadata {metadata_file:?}: {err}"
        ))
    })
}

fn write_qglake_manifest_files(
    metadata: &Value,
    manifest_path: &std::path::Path,
    manifest: &str,
    delete_manifest_path: &std::path::Path,
    delete_manifest: &str,
    manifest_list_path: &std::path::Path,
    data_file: &str,
    delete_file: &str,
) -> lakecat_core::LakeCatResult<()> {
    let table_metadata =
        TableMetadata::from_json(&serde_json::to_vec(metadata).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to encode QGLake table metadata for manifest writer: {err}"
            ))
        })?)
        .map_err(|err| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake fixture metadata is not valid Iceberg metadata: {err}"
            ))
        })?;
    let manifest_metadata = ManifestMetadata::new(
        Arc::new(
            table_metadata
                .current_schema()
                .ok_or_else(|| {
                    lakecat_core::LakeCatError::InvalidArgument(
                        "QGLake fixture metadata has no current schema".to_string(),
                    )
                })?
                .clone(),
        ),
        table_metadata.current_schema_id,
        table_metadata
            .default_partition_spec()
            .ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(
                    "QGLake fixture metadata has no default partition spec".to_string(),
                )
            })?
            .clone(),
        FormatVersion::V2,
        ManifestContentType::Data,
    );
    let mut manifest_writer = ManifestWriterBuilder::new(Some(42), None, manifest_metadata).build();
    manifest_writer.add(DataFile {
        content: DataContentType::Data,
        file_path: data_file.to_string(),
        file_format: DataFileFormat::Parquet,
        partition: Vec::new(),
        record_count: 3,
        file_size_in_bytes: 256,
        column_sizes: Default::default(),
        value_counts: Default::default(),
        null_value_counts: Default::default(),
        nan_value_counts: Default::default(),
        lower_bounds: Default::default(),
        upper_bounds: Default::default(),
        block_size_in_bytes: None,
        key_metadata: None,
        split_offsets: Vec::new(),
        equality_ids: Vec::new(),
        sort_order_id: None,
        first_row_id: None,
        partition_spec_id: 0,
        referenced_data_file: None,
        content_offset: None,
        content_size_in_bytes: None,
    });
    fs::write(
        manifest_path,
        manifest_writer.to_avro_bytes_v2().map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to encode QGLake data manifest: {err}"
            ))
        })?,
    )
    .map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write QGLake data manifest {manifest_path:?}: {err}"
        ))
    })?;

    let delete_manifest_metadata = ManifestMetadata::new(
        Arc::new(
            table_metadata
                .current_schema()
                .ok_or_else(|| {
                    lakecat_core::LakeCatError::InvalidArgument(
                        "QGLake fixture metadata has no current schema".to_string(),
                    )
                })?
                .clone(),
        ),
        table_metadata.current_schema_id,
        table_metadata
            .default_partition_spec()
            .ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(
                    "QGLake fixture metadata has no default partition spec".to_string(),
                )
            })?
            .clone(),
        FormatVersion::V2,
        ManifestContentType::Deletes,
    );
    let mut delete_writer =
        ManifestWriterBuilder::new(Some(42), None, delete_manifest_metadata).build();
    delete_writer.add(DataFile {
        content: DataContentType::PositionDeletes,
        file_path: delete_file.to_string(),
        file_format: DataFileFormat::Parquet,
        partition: Vec::new(),
        record_count: 1,
        file_size_in_bytes: 64,
        column_sizes: Default::default(),
        value_counts: Default::default(),
        null_value_counts: Default::default(),
        nan_value_counts: Default::default(),
        lower_bounds: Default::default(),
        upper_bounds: Default::default(),
        block_size_in_bytes: None,
        key_metadata: None,
        split_offsets: Vec::new(),
        equality_ids: Vec::new(),
        sort_order_id: None,
        first_row_id: None,
        partition_spec_id: 0,
        referenced_data_file: Some(data_file.to_string()),
        content_offset: None,
        content_size_in_bytes: None,
    });
    fs::write(
        delete_manifest_path,
        delete_writer.to_avro_bytes_v2().map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to encode QGLake delete manifest: {err}"
            ))
        })?,
    )
    .map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write QGLake delete manifest {delete_manifest_path:?}: {err}"
        ))
    })?;

    let mut list_writer = ManifestListWriter::new();
    list_writer.append(
        ManifestFile::builder()
            .with_manifest_path(manifest)
            .with_manifest_length(10)
            .with_partition_spec_id(0)
            .with_content(ManifestContentType::Data)
            .with_sequence_number(7)
            .with_min_sequence_number(7)
            .with_added_snapshot_id(42)
            .with_file_counts(1, 0, 0)
            .with_row_counts(3, 0, 0)
            .build()
            .map_err(|err| {
                lakecat_core::LakeCatError::Internal(format!(
                    "failed to build QGLake manifest-list entry: {err}"
                ))
            })?,
    );
    list_writer.append(
        ManifestFile::builder()
            .with_manifest_path(delete_manifest)
            .with_manifest_length(10)
            .with_partition_spec_id(0)
            .with_content(ManifestContentType::Deletes)
            .with_sequence_number(8)
            .with_min_sequence_number(8)
            .with_added_snapshot_id(42)
            .with_file_counts(1, 0, 0)
            .with_row_counts(1, 0, 0)
            .build()
            .map_err(|err| {
                lakecat_core::LakeCatError::Internal(format!(
                    "failed to build QGLake delete manifest-list entry: {err}"
                ))
            })?,
    );
    fs::write(
        manifest_list_path,
        list_writer.to_bytes(FormatVersion::V2).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to encode QGLake manifest list: {err}"
            ))
        })?,
    )
    .map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write QGLake manifest list {manifest_list_path:?}: {err}"
        ))
    })?;
    Ok(())
}

fn file_url_path(value: &str, label: &str) -> lakecat_core::LakeCatResult<PathBuf> {
    Url::parse(value)
        .map_err(|err| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be a file URL for local fixture generation: {value}: {err}"
            ))
        })?
        .to_file_path()
        .map_err(|_| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be a file URL for local fixture generation: {value}"
            ))
        })
}

fn file_path_url(path: &std::path::Path, label: &str) -> lakecat_core::LakeCatResult<String> {
    Url::from_file_path(path)
        .map_err(|_| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} path cannot be converted to a file URL: {path:?}"
            ))
        })
        .map(|url| url.to_string())
}

fn qglake_odrl_policy(table: &str) -> Value {
    json!({
        "@context": {
            "odrl": "http://www.w3.org/ns/odrl/2/",
            "lakecat": "https://querygraph.ai/lakecat/ns#",
            "typesec": "https://typesec.ai/ns#"
        },
        "uid": format!("lakecat:qglake:{table}:agent-read"),
        "type": "odrl:Set",
        "lakecat:read-restriction": {
            "allowed-columns": ["event_id", "occurred_at", "severity"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300
        },
        "permission": [{
            "target": table,
            "action": "odrl:read",
            "constraint": [{
                "leftOperand": "typesec:capability",
                "operator": "odrl:eq",
                "rightOperand": "catalog.table.plan_scan"
            }, {
                "leftOperand": "purpose",
                "operator": "odrl:eq",
                "rightOperand": "qglake-agent-demo"
            }]
        }]
    })
}

enum Command {
    BootstrapExport {
        catalog: String,
        output: PathBuf,
        principal: Option<String>,
    },
    Config {
        catalog: String,
        principal: Option<String>,
    },
    LineageDrain {
        catalog: String,
        principal: Option<String>,
    },
    QglakeVerifyReplay {
        bundle: PathBuf,
        drain: PathBuf,
        principal: Option<String>,
        json: bool,
    },
    QglakeVerifyHandoff {
        summary: PathBuf,
        json: bool,
    },
    PolicyList {
        catalog: String,
        warehouse: String,
        principal: Option<String>,
    },
    PolicyUpsert {
        catalog: String,
        warehouse: String,
        policy: String,
        namespace: Option<Vec<String>>,
        table: Option<String>,
        enforced: bool,
        odrl: Value,
        principal: Option<String>,
    },
    StorageProfileList {
        catalog: String,
        warehouse: String,
        principal: Option<String>,
    },
    StorageProfileUpsert {
        catalog: String,
        warehouse: String,
        profile: String,
        location_prefix: String,
        provider: String,
        issuance_mode: String,
        secret_ref: Option<String>,
        public_config: BTreeMap<String, String>,
        principal: Option<String>,
    },
    QglakeFixture {
        catalog: String,
        warehouse: String,
        namespace: Vec<String>,
        table: String,
        location: String,
        metadata_location: String,
        output: PathBuf,
        drain_output: Option<PathBuf>,
        principal: Option<String>,
    },
}

impl Command {
    fn parse(args: impl IntoIterator<Item = String>) -> lakecat_core::LakeCatResult<Self> {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            return Err(usage_error());
        };
        match command.as_str() {
            "bootstrap-export" => parse_bootstrap_export(args),
            "config" => parse_config(args),
            "lineage-drain" => parse_lineage_drain(args),
            "qglake-verify-replay" => parse_qglake_verify_replay(args),
            "qglake-verify-handoff" => parse_qglake_verify_handoff(args),
            "policy-list" => parse_policy_list(args),
            "policy-upsert" => parse_policy_upsert(args),
            "storage-profile-list" => parse_storage_profile_list(args),
            "storage-profile-upsert" => parse_storage_profile_upsert(args),
            "qglake-fixture" => parse_qglake_fixture(args),
            _ => Err(usage_error()),
        }
    }
}

fn parse_bootstrap_export(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut catalog = "http://127.0.0.1:8181".to_string();
    let mut output = None;
    let mut principal = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => catalog = next_arg(&mut args, "--catalog")?,
            "--output" => output = Some(PathBuf::from(next_arg(&mut args, "--output")?)),
            "--principal" => principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    let output = output.ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "missing required --output for bootstrap-export".to_string(),
        )
    })?;
    Ok(Command::BootstrapExport {
        catalog,
        output,
        principal,
    })
}

fn parse_config(args: impl Iterator<Item = String>) -> lakecat_core::LakeCatResult<Command> {
    let mut catalog = "http://127.0.0.1:8181".to_string();
    let mut principal = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => catalog = next_arg(&mut args, "--catalog")?,
            "--principal" => principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::Config { catalog, principal })
}

fn parse_lineage_drain(args: impl Iterator<Item = String>) -> lakecat_core::LakeCatResult<Command> {
    let mut catalog = "http://127.0.0.1:8181".to_string();
    let mut principal = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => catalog = next_arg(&mut args, "--catalog")?,
            "--principal" => principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::LineageDrain { catalog, principal })
}

fn parse_qglake_verify_replay(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut bundle = None;
    let mut drain = None;
    let mut principal = None;
    let mut json = false;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bundle" => bundle = Some(PathBuf::from(next_arg(&mut args, "--bundle")?)),
            "--drain" => drain = Some(PathBuf::from(next_arg(&mut args, "--drain")?)),
            "--principal" => principal = Some(next_arg(&mut args, "--principal")?),
            "--json" => json = true,
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::QglakeVerifyReplay {
        bundle: bundle.ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "missing required --bundle for qglake-verify-replay".to_string(),
            )
        })?,
        drain: drain.ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "missing required --drain for qglake-verify-replay".to_string(),
            )
        })?,
        principal,
        json,
    })
}

fn parse_qglake_verify_handoff(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut summary = None;
    let mut json = false;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--summary" => summary = Some(PathBuf::from(next_arg(&mut args, "--summary")?)),
            "--json" => json = true,
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::QglakeVerifyHandoff {
        summary: summary.ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "missing required --summary for qglake-verify-handoff".to_string(),
            )
        })?,
        json,
    })
}

fn parse_policy_list(args: impl Iterator<Item = String>) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::PolicyList {
        catalog: common.catalog,
        warehouse: common.warehouse,
        principal: common.principal,
    })
}

fn parse_policy_upsert(args: impl Iterator<Item = String>) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut policy = None;
    let mut namespace = None;
    let mut table = None;
    let mut enforced = true;
    let mut odrl = json!({});
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            "--policy" => policy = Some(next_arg(&mut args, "--policy")?),
            "--namespace" => {
                namespace = Some(parse_namespace(&next_arg(&mut args, "--namespace")?))
            }
            "--table" => table = Some(next_arg(&mut args, "--table")?),
            "--enforced" => enforced = parse_bool(&next_arg(&mut args, "--enforced")?)?,
            "--odrl-file" => {
                odrl = read_json_file(&PathBuf::from(next_arg(&mut args, "--odrl-file")?))?
            }
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::PolicyUpsert {
        catalog: common.catalog,
        warehouse: common.warehouse,
        policy: required(policy, "--policy")?,
        namespace,
        table,
        enforced,
        odrl,
        principal: common.principal,
    })
}

fn parse_storage_profile_list(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::StorageProfileList {
        catalog: common.catalog,
        warehouse: common.warehouse,
        principal: common.principal,
    })
}

fn parse_storage_profile_upsert(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut profile = None;
    let mut location_prefix = None;
    let mut provider = None;
    let mut issuance_mode = None;
    let mut secret_ref = None;
    let mut public_config = BTreeMap::new();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            "--profile" => profile = Some(next_arg(&mut args, "--profile")?),
            "--location-prefix" => {
                location_prefix = Some(next_arg(&mut args, "--location-prefix")?)
            }
            "--provider" => provider = Some(next_arg(&mut args, "--provider")?),
            "--issuance-mode" => issuance_mode = Some(next_arg(&mut args, "--issuance-mode")?),
            "--secret-ref" => secret_ref = Some(next_arg(&mut args, "--secret-ref")?),
            "--public-config" => {
                let (key, value) = parse_key_value(&next_arg(&mut args, "--public-config")?)?;
                public_config.insert(key, value);
            }
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::StorageProfileUpsert {
        catalog: common.catalog,
        warehouse: common.warehouse,
        profile: required(profile, "--profile")?,
        location_prefix: required(location_prefix, "--location-prefix")?,
        provider: required(provider, "--provider")?,
        issuance_mode: required(issuance_mode, "--issuance-mode")?,
        secret_ref,
        public_config,
        principal: common.principal,
    })
}

fn parse_qglake_fixture(
    args: impl Iterator<Item = String>,
) -> lakecat_core::LakeCatResult<Command> {
    let mut common = CommonArgs::default();
    let mut namespace = vec!["default".to_string()];
    let mut table = "events".to_string();
    let mut location = "file:///tmp/lakecat-qglake/events".to_string();
    let mut metadata_location = "file:///tmp/lakecat-qglake/events/metadata/00000.json".to_string();
    let mut output = PathBuf::from("target/qglake/lakecat-bootstrap.json");
    let mut drain_output = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => common.catalog = next_arg(&mut args, "--catalog")?,
            "--warehouse" => common.warehouse = next_arg(&mut args, "--warehouse")?,
            "--principal" => common.principal = Some(next_arg(&mut args, "--principal")?),
            "--namespace" => namespace = parse_namespace(&next_arg(&mut args, "--namespace")?),
            "--table" => table = next_arg(&mut args, "--table")?,
            "--location" => location = next_arg(&mut args, "--location")?,
            "--metadata-location" => {
                metadata_location = next_arg(&mut args, "--metadata-location")?
            }
            "--output" => output = PathBuf::from(next_arg(&mut args, "--output")?),
            "--drain-output" => {
                drain_output = Some(PathBuf::from(next_arg(&mut args, "--drain-output")?))
            }
            _ => return Err(usage_error()),
        }
    }
    Ok(Command::QglakeFixture {
        catalog: common.catalog,
        warehouse: common.warehouse,
        namespace,
        table,
        location,
        metadata_location,
        output,
        drain_output,
        principal: common.principal,
    })
}

struct CommonArgs {
    catalog: String,
    warehouse: String,
    principal: Option<String>,
}

impl Default for CommonArgs {
    fn default() -> Self {
        Self {
            catalog: "http://127.0.0.1:8181".to_string(),
            warehouse: "local".to_string(),
            principal: None,
        }
    }
}

fn required(value: Option<String>, flag: &str) -> lakecat_core::LakeCatResult<String> {
    value.ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("missing required {flag}"))
    })
}

fn parse_namespace(value: &str) -> Vec<String> {
    value
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_key_value(value: &str) -> lakecat_core::LakeCatResult<(String, String)> {
    let Some((key, value)) = value.split_once('=') else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "--public-config values must use key=value".to_string(),
        ));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "--public-config key cannot be empty".to_string(),
        ));
    }
    Ok((key.to_string(), value.to_string()))
}

fn parse_bool(value: &str) -> lakecat_core::LakeCatResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "invalid boolean value: {other}"
        ))),
    }
}

fn read_json_file(path: &PathBuf) -> lakecat_core::LakeCatResult<Value> {
    let bytes = fs::read(path).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to read JSON file {}: {err}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to parse JSON file {}: {err}",
            path.display()
        ))
    })
}

fn read_typed_json_file<T: DeserializeOwned>(
    path: &PathBuf,
    label: &str,
) -> lakecat_core::LakeCatResult<T> {
    serde_json::from_value(read_json_file(path)?).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} JSON file {} did not match expected shape: {err}",
            path.display()
        ))
    })
}

fn required_value<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a Value> {
    value.get(field).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} is missing required field {field}"
        ))
    })
}

fn required_object<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a serde_json::Map<String, Value>> {
    required_value(value, field, label)?
        .as_object()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label}.{field} must be an object"
            ))
        })
}

fn required_array<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a Vec<Value>> {
    value.get(field).and_then(Value::as_array).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("{label}.{field} must be an array"))
    })
}

fn required_string_array(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<Vec<String>> {
    required_array(value, field, label)?
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.as_str().map(ToString::to_string).ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "{label}.{field}[{index}] must be a string"
                ))
            })
        })
        .collect()
}

fn required_str<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    required_value(value, field, label)?
        .as_str()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!("{label}.{field} must be a string"))
        })
}

fn required_bool(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<bool> {
    value.get(field).and_then(Value::as_bool).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("{label}.{field} must be a boolean"))
    })
}

fn required_u64(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<u64> {
    value.get(field).and_then(Value::as_u64).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be a non-negative integer"
        ))
    })
}

fn require_positive_u64(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<u64> {
    let number = required_u64(value, field, label)?;
    if number == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be positive"
        )));
    }
    Ok(number)
}

fn require_non_empty_str<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    let string = required_str(value, field, label)?;
    if string.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must not be empty"
        )));
    }
    Ok(string)
}

fn require_hash_str<'a>(
    value: &'a serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a str> {
    let string = require_non_empty_str(value, field, label)?;
    if !is_sha256_hash(string) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be a sha256 hash"
        )));
    }
    Ok(string)
}

fn is_sha256_hash(string: &str) -> bool {
    string.starts_with("sha256:")
}

fn require_optional_hash_value(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<bool> {
    match required_value(value, field, label)? {
        Value::Null => Ok(false),
        Value::String(string) if is_sha256_hash(string) => Ok(true),
        _ => Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must be null or a sha256 hash"
        ))),
    }
}

fn require_hash_array(
    value: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let array = required_array(value, field, label)?;
    if array.is_empty()
        || array
            .iter()
            .any(|item| !item.as_str().is_some_and(is_sha256_hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} must contain sha256 hashes"
        )));
    }
    Ok(())
}

fn require_string_eq(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    require_string_match(value, field, expected, label)
}

fn require_string_match(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let actual = required_str(value, field, label)?;
    if actual != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} mismatch: expected={expected} actual={actual}"
        )));
    }
    Ok(())
}

fn require_u64_match(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: u64,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let actual = required_u64(value, field, label)?;
    if actual != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} mismatch: expected={expected} actual={actual}"
        )));
    }
    Ok(())
}

fn require_value_match(
    value: &serde_json::Map<String, Value>,
    field: &str,
    expected: &Value,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let actual = value.get(field).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} is missing required field {field}"
        ))
    })?;
    if actual != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label}.{field} mismatch"
        )));
    }
    Ok(())
}

fn next_arg(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> lakecat_core::LakeCatResult<String> {
    args.next().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!("missing value for {flag}"))
    })
}

fn usage_error() -> lakecat_core::LakeCatError {
    let commands = [
        "config",
        "bootstrap-export",
        "lineage-drain",
        "qglake-verify-replay",
        "qglake-verify-handoff",
        "storage-profile-list",
        "storage-profile-upsert",
        "policy-list",
        "policy-upsert",
        "qglake-fixture",
    ];
    lakecat_core::LakeCatError::InvalidArgument(format!(
        "usage: lakecat-cli <{}> [options]",
        commands.join("|")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const QGLAKE_TEST_LOCATION: &str = "file:///tmp/lakecat-qglake/events";

    fn qglake_add_management_receipt_hashes(management: &mut Value) {
        management["serverReplayEventHashes"] = json!(["sha256:server-list-replay-event"]);
        management["serverOpenLineageHashes"] = json!(["sha256:server-list-openlineage"]);
        management["projectReplayEventHashes"] = json!(["sha256:project-list-replay-event"]);
        management["projectOpenLineageHashes"] = json!(["sha256:project-list-openlineage"]);
        management["warehouseReplayEventHashes"] = json!(["sha256:warehouse-list-replay-event"]);
        management["warehouseOpenLineageHashes"] = json!(["sha256:warehouse-list-openlineage"]);
        management["policyReplayEventHashes"] = json!(["sha256:policy-list-replay-event"]);
        management["policyOpenLineageHashes"] = json!(["sha256:policy-list-openlineage"]);
        management["storageProfileReplayEventHashes"] =
            json!(["sha256:storage-profile-list-replay-event"]);
        management["storageProfileOpenLineageHashes"] =
            json!(["sha256:storage-profile-list-openlineage"]);
    }

    fn qglake_handoff_summary_json() -> Value {
        let mut summary = json!({
            "schemaVersion": "lakecat.qglake.handoff-summary.v1",
            "status": "verified",
            "catalogUrl": "http://127.0.0.1:18181",
            "principal": "did:example:agent",
            "warehouse": "local",
            "namespace": "default",
            "table": "events",
            "querygraphVerification": {
                "tableCount": 1,
                "viewCount": 1,
                "verifiedTables": [
                    "lakecat:table:local:default:events"
                ],
                "verifiedViews": [
                    "lakecat:view:local:default:active_customers_view"
                ],
                "bundleHash": "sha256:bundle",
                "graphHash": "sha256:graph",
                "openLineageHash": "sha256:openlineage",
                "querygraphImportHash": "sha256:querygraph-import",
                "standards": [
                    "Iceberg REST",
                    "Croissant",
                    "CDIF",
                    "OSI handoff",
                    "ODRL",
                    "Grust catalog graph",
                    "OpenLineage"
                ]
            },
            "querygraphImportVerification": {
                "matchesVerify": true,
                "tableCount": 1,
                "viewCount": 1,
                "verifiedTables": [
                    "lakecat:table:local:default:events"
                ],
                "verifiedViews": [
                    "lakecat:view:local:default:active_customers_view"
                ],
                "bundleHash": "sha256:bundle",
                "graphHash": "sha256:graph",
                "openLineageHash": "sha256:openlineage",
                "querygraphImportHash": "sha256:querygraph-import",
                "standards": [
                    "Iceberg REST",
                    "Croissant",
                    "CDIF",
                    "OSI handoff",
                    "ODRL",
                    "Grust catalog graph",
                    "OpenLineage"
                ]
            },
            "lakecatReplayVerification": {
                "schemaVersion": "lakecat.qglake.replay-verification.v1",
                "status": "verified",
                "matchesQueryGraph": true,
                "requestIdentityProof": {
                    "principalSubject": "did:example:agent",
                    "principalKind": "agent",
                    "requestIdentitySource": "x-lakecat-agent-did",
                    "requestIdentityState": "unverified",
                    "authorizationReceiptHash": "sha256:identity",
                    "typedidEnvelopeHash": null,
                    "typedidProofHash": null
                },
                "queryGraphBootstrapProof": {
                    "bundleHash": "sha256:bundle",
                    "graphHash": "sha256:graph",
                    "openLineageHash": "sha256:openlineage",
                    "queryGraphImportHash": "sha256:querygraph-import",
                    "tableArtifactCount": 1,
                    "viewArtifactCount": 1,
                    "policyBindingCount": 1,
                    "standards": [
                        "Iceberg REST",
                        "Croissant",
                        "CDIF",
                        "OSI handoff",
                        "ODRL",
                        "Grust catalog graph",
                        "OpenLineage"
                    ],
                    "principalSubject": "did:example:agent",
                    "principalKind": "agent",
                    "requestIdentitySource": "x-lakecat-agent-did",
                    "requestIdentityState": "unverified",
                    "authorizationReceiptHash": "sha256:identity",
                    "agentDelegationHash": "sha256:delegation",
                    "agentSummarySignatureHash": "sha256:summary",
                    "typedidEnvelopeHash": null,
                    "typedidProofHash": null,
                    "viewVersionReceiptHashes": ["sha256:view-receipt"],
                    "replayEventHashes": ["sha256:bootstrap-replay"],
                    "openLineageHashes": ["sha256:bootstrap-openlineage"]
                },
                "governedScanProof": {
                    "planTaskCount": 1,
                    "planGraphEvents": 1,
                    "fileTaskCount": 1,
                    "deleteFileCount": 1,
                    "childPlanTaskCount": 2,
                    "plannedReadRestriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": ["sha256:scan-policy"]
                    },
                    "fetchedReadRestriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": ["sha256:scan-policy"]
                    },
                    "plannedRequestedProjection": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "plannedEffectiveProjection": ["event_id", "occurred_at", "severity"],
                    "plannedRequestedStatsFields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "plannedEffectiveStatsFields": ["event_id", "occurred_at", "severity"],
                    "fetchedRequiredProjection": ["event_id", "occurred_at", "severity"],
                    "fetchedEffectiveProjection": ["event_id", "occurred_at", "severity"],
                    "fetchedRequiredFilters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }],
                    "plannedReplayEventHashes": ["sha256:scan-plan-replay"],
                    "fetchedReplayEventHashes": ["sha256:scan-fetch-replay"],
                    "plannedOpenLineageHashes": ["sha256:scan-plan-openlineage"],
                    "fetchedOpenLineageHashes": ["sha256:scan-fetch-openlineage"]
                },
                "tableCommitHistoryProof": {
                    "commitCount": 1,
                    "sequenceNumbers": [1],
                    "commitHashes": ["sha256:commit"],
                        "graphEvents": 1,
                    "replayEventHashes": ["sha256:commit-replay"],
                    "openLineageHashes": ["sha256:commit-openlineage"]
                },
                "managementProof": {
                    "serverCount": 1,
                    "serverGraphEvents": 1,
                    "projectCount": 1,
                    "projectGraphEvents": 1,
                    "warehouseCount": 1,
                    "warehouseGraphEvents": 1,
                    "policyBindingCount": 1,
                    "policyGraphEvents": 1,
                    "storageProfileCount": 1,
                    "storageProfileGraphEvents": 1
                },
                "viewReceiptChainProof": {
                    "viewCount": 1,
                    "views": [{
                        "stableId": "lakecat:view:local:default:active_customers_view",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "name": "active_customers_view",
                        "viewVersion": 1,
                        "acceptedViewVersion": 1,
                        "acceptedReceiptHash": "sha256:view-receipt",
                        "acceptedReceiptChainHash": "sha256:view-receipt-chain",
                        "eventType": "view.upserted",
                        "expectedViewVersion": null,
                        "graphEvents": 1,
                        "replayEventHashes": ["sha256:view-replay"],
                        "openLineageHashes": ["sha256:view-openlineage"]
                    }],
                    "tombstoneReceipts": [{
                        "stableId": "lakecat:view:local:default:active_customers_view",
                        "expectedViewVersion": 1,
                        "receiptHashes": ["sha256:tombstone"],
                        "replayEventHashes": ["sha256:tombstone-replay"],
                        "openLineageHashes": ["sha256:tombstone-openlineage"]
                    }],
                    "receiptChains": [{
                        "warehouse": "local",
                        "namespace": ["default"],
                        "verifiedChainCount": 1,
                        "receiptHashes": ["sha256:chain-receipt", "sha256:tombstone"],
                        "chainHashes": ["sha256:view-receipt-chain"],
                        "replayEventHashes": ["sha256:chain-replay"],
                        "openLineageHashes": ["sha256:chain-openlineage"]
                    }]
                },
                "storageProfileUpsertProof": {
                    "profileId": "events-local",
                    "provider": "file",
                    "issuanceMode": "local-file-no-secret",
                    "locationPrefixHash": "sha256:storage-location-prefix",
                    "secretRefPresent": false,
                    "secretRefProvider": null,
                            "secretRefHash": null,
                        "graphEvents": 1,
                    "replayEventHashes": ["sha256:storage-replay"],
                    "openLineageHashes": ["sha256:storage-openlineage"]
                },
                "credentialVendingProof": {
                    "restricted": {
                        "principalSubject": "did:example:agent",
                        "principalKind": "agent",
                        "credentialCount": 0,
                        "rawCredentialExceptionAllowed": false,
                        "blockReason": QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON,
                        "maxCredentialTtlSeconds": 300,
                        "storageProfile": {
                            "profileId": "events-local",
                            "provider": "file",
                            "issuanceMode": "local-file-no-secret",
                            "locationPrefixHash": "sha256:storage-location-prefix",
                            "secretRefPresent": false,
                            "secretRefProvider": null,
                            "secretRefHash": null,
                            "graphEvents": 2
                        },
                        "replayEventHashes": ["sha256:restricted-replay"],
                        "openLineageHashes": ["sha256:restricted-openlineage"]
                    },
                    "trustedHuman": {
                        "principalSubject": "human:qglake-operator",
                        "principalKind": "human",
                        "credentialCount": 1,
                        "rawCredentialExceptionAllowed": true,
                        "rawCredentialExceptionReason": QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON,
                        "blockReason": null,
                        "maxCredentialTtlSeconds": 300,
                        "storageProfile": {
                            "profileId": "events-local",
                            "provider": "file",
                            "issuanceMode": "local-file-no-secret",
                            "locationPrefixHash": "sha256:storage-location-prefix",
                            "secretRefPresent": false,
                            "secretRefProvider": null,
                            "secretRefHash": null,
                            "graphEvents": 2
                        },
                        "replayEventHashes": ["sha256:human-replay"],
                        "openLineageHashes": ["sha256:human-openlineage"]
                    }
                },
                "replayEvidence": {}
            }
        });
        qglake_add_management_receipt_hashes(
            &mut summary["lakecatReplayVerification"]["managementProof"],
        );
        summary
    }

    fn qglake_handoff_summary_json_with_artifacts(dir: &Path) -> Value {
        let bundle = dir.join("lakecat-bootstrap.json");
        let drain = dir.join("lineage-drain.json");
        let import_plan = dir.join("querygraph-import-plan.json");
        let lakecat_replay = dir.join("lakecat-replay.txt");
        let querygraph_verify = dir.join("querygraph-verify.json");
        let querygraph_import = dir.join("querygraph-import.json");
        let lakecat_handoff_verify = dir.join("lakecat-handoff-verify.json");
        let service_log = dir.join("lakecat-service.log");
        let mut lakecat_replay_json = json!({
            "schema-version": "lakecat.qglake.replay-verification.v1",
            "status": "verified",
            "table-count": 1,
            "view-count": 1,
            "bundle-hash": "sha256:bundle",
            "graph-hash": "sha256:graph",
            "open-lineage-hash": "sha256:openlineage",
            "querygraph-import-hash": "sha256:querygraph-import",
            "standards": [
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "ODRL",
                "Grust catalog graph",
                "OpenLineage"
            ],
            "replay-evidence": {
                "requestIdentity": {
                    "principalSubject": "did:example:agent",
                    "principalKind": "agent",
                    "requestIdentitySource": "x-lakecat-agent-did",
                    "requestIdentityState": "unverified",
                    "authorizationReceiptHash": "sha256:identity",
                    "typedidEnvelopeHash": null,
                    "typedidProofHash": null
                },
                "queryGraphBootstrap": {
                    "bundleHash": "sha256:bundle",
                    "graphHash": "sha256:graph",
                    "openLineageHash": "sha256:openlineage",
                    "queryGraphImportHash": "sha256:querygraph-import",
                    "tableArtifactCount": 1,
                    "viewArtifactCount": 1,
                    "policyBindingCount": 1,
                    "standards": [
                        "Iceberg REST",
                        "Croissant",
                        "CDIF",
                        "OSI handoff",
                        "ODRL",
                        "Grust catalog graph",
                        "OpenLineage"
                    ],
                    "principalSubject": "did:example:agent",
                    "principalKind": "agent",
                    "requestIdentitySource": "x-lakecat-agent-did",
                    "requestIdentityState": "unverified",
                    "authorizationReceiptHash": "sha256:identity",
                    "agentDelegationHash": "sha256:delegation",
                    "agentSummarySignatureHash": "sha256:summary",
                    "typedidEnvelopeHash": null,
                    "typedidProofHash": null,
                    "viewVersionReceiptHashes": ["sha256:view-receipt"],
                    "replayEventHashes": ["sha256:bootstrap-replay"],
                    "openLineageHashes": ["sha256:bootstrap-openlineage"]
                },
                "scan": {
                    "planTaskCount": 1,
                    "planGraphEvents": 1,
                    "fileTaskCount": 1,
                    "deleteFileCount": 1,
                    "childPlanTaskCount": 2,
                    "plannedReadRestriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": ["sha256:scan-policy"]
                    },
                    "fetchedReadRestriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": ["sha256:scan-policy"]
                    },
                    "plannedRequestedProjection": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "plannedEffectiveProjection": ["event_id", "occurred_at", "severity"],
                    "plannedRequestedStatsFields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "plannedEffectiveStatsFields": ["event_id", "occurred_at", "severity"],
                    "fetchedRequiredProjection": ["event_id", "occurred_at", "severity"],
                    "fetchedEffectiveProjection": ["event_id", "occurred_at", "severity"],
                    "fetchedRequiredFilters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }],
                    "plannedReplayEventHashes": ["sha256:scan-plan-replay"],
                    "fetchedReplayEventHashes": ["sha256:scan-fetch-replay"],
                    "plannedOpenLineageHashes": ["sha256:scan-plan-openlineage"],
                    "fetchedOpenLineageHashes": ["sha256:scan-fetch-openlineage"]
                },
                "tableCommitHistory": {
                    "commitCount": 1,
                    "sequenceNumbers": [1],
                    "commitHashes": ["sha256:commit"],
                        "graphEvents": 1,
                    "replayEventHashes": ["sha256:commit-replay"],
                    "openLineageHashes": ["sha256:commit-openlineage"]
                },
                "management": {
                    "serverCount": 1,
                    "serverGraphEvents": 1,
                    "projectCount": 1,
                    "projectGraphEvents": 1,
                    "warehouseCount": 1,
                    "warehouseGraphEvents": 1,
                    "policyBindingCount": 1,
                    "policyGraphEvents": 1,
                    "storageProfileCount": 1,
                    "storageProfileGraphEvents": 1,
                    "storageProfileUpsert": {
                        "profileId": "events-local",
                        "provider": "file",
                        "issuanceMode": "local-file-no-secret",
                        "locationPrefixHash": "sha256:storage-location-prefix",
                        "secretRefPresent": false,
                        "secretRefProvider": null,
                        "secretRefHash": null,
                        "graphEvents": 1,
                        "replayEventHashes": ["sha256:storage-replay"],
                        "openLineageHashes": ["sha256:storage-openlineage"]
                    }
                },
                "views": {
                    "viewCount": 1,
                    "views": [{
                        "stableId": "lakecat:view:local:default:active_customers_view",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "name": "active_customers_view",
                        "viewVersion": 1,
                        "acceptedViewVersion": 1,
                        "acceptedReceiptHash": "sha256:view-receipt",
                        "acceptedReceiptChainHash": "sha256:view-receipt-chain",
                        "eventType": "view.upserted",
                        "expectedViewVersion": null,
                        "graphEvents": 1,
                        "replayEventHashes": ["sha256:view-replay"],
                        "openLineageHashes": ["sha256:view-openlineage"]
                    }],
                    "tombstoneReceipts": [{
                        "stableId": "lakecat:view:local:default:active_customers_view",
                        "expectedViewVersion": 1,
                        "receiptHashes": ["sha256:tombstone"],
                        "replayEventHashes": ["sha256:tombstone-replay"],
                        "openLineageHashes": ["sha256:tombstone-openlineage"]
                    }],
                    "receiptChains": [{
                        "warehouse": "local",
                        "namespace": ["default"],
                        "verifiedChainCount": 1,
                        "receiptHashes": ["sha256:chain-receipt", "sha256:tombstone"],
                        "chainHashes": ["sha256:view-receipt-chain"],
                        "replayEventHashes": ["sha256:chain-replay"],
                        "openLineageHashes": ["sha256:chain-openlineage"]
                    }]
                },
                "credentials": {
                    "restricted": {
                        "principalSubject": "did:example:agent",
                        "principalKind": "agent",
                        "credentialCount": 0,
                        "rawCredentialExceptionAllowed": false,
                        "blockReason": QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON,
                        "maxCredentialTtlSeconds": 300,
                        "storageProfile": {
                            "profileId": "events-local",
                            "provider": "file",
                            "issuanceMode": "local-file-no-secret",
                            "locationPrefixHash": "sha256:storage-location-prefix",
                            "secretRefPresent": false,
                            "secretRefProvider": null,
                            "secretRefHash": null,
                            "graphEvents": 2
                        },
                        "replayEventHashes": ["sha256:restricted-replay"],
                        "openLineageHashes": ["sha256:restricted-openlineage"]
                    },
                    "trustedHuman": {
                        "principalSubject": "human:qglake-operator",
                        "principalKind": "human",
                        "credentialCount": 1,
                        "rawCredentialExceptionAllowed": true,
                        "rawCredentialExceptionReason": QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON,
                        "blockReason": null,
                        "maxCredentialTtlSeconds": 300,
                        "storageProfile": {
                            "profileId": "events-local",
                            "provider": "file",
                            "issuanceMode": "local-file-no-secret",
                            "locationPrefixHash": "sha256:storage-location-prefix",
                            "secretRefPresent": false,
                            "secretRefProvider": null,
                            "secretRefHash": null,
                            "graphEvents": 2
                        },
                        "replayEventHashes": ["sha256:human-replay"],
                        "openLineageHashes": ["sha256:human-openlineage"]
                    }
                }
            }
        });
        qglake_add_management_receipt_hashes(
            &mut lakecat_replay_json["replay-evidence"]["management"],
        );
        let querygraph_capture_json = json!({
            "warehouse": "local",
            "table-count": 1,
            "view-count": 1,
            "verified-tables": [
                "lakecat:table:local:default:events"
            ],
            "verified-views": [
                "lakecat:view:local:default:active_customers_view"
            ],
            "bundle-hash": "sha256:bundle",
            "graph-hash": "sha256:graph",
            "open-lineage-hash": "sha256:openlineage",
            "querygraph-import-hash": "sha256:querygraph-import",
            "standards": [
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "ODRL",
                "Grust catalog graph",
                "OpenLineage"
            ]
        });
        let lakecat_replay_bytes =
            serde_json::to_vec_pretty(&lakecat_replay_json).expect("LakeCat replay JSON bytes");
        let querygraph_verify_bytes =
            serde_json::to_vec_pretty(&querygraph_capture_json).expect("verify JSON bytes");
        let querygraph_import_bytes =
            serde_json::to_vec_pretty(&querygraph_capture_json).expect("import JSON bytes");
        fs::write(&bundle, b"bundle").expect("write bundle");
        fs::write(&drain, b"drain").expect("write drain");
        fs::write(&import_plan, b"import-plan").expect("write import plan");
        fs::write(&lakecat_replay, &lakecat_replay_bytes).expect("write LakeCat replay");
        fs::write(&querygraph_verify, &querygraph_verify_bytes).expect("write QueryGraph verify");
        fs::write(&querygraph_import, &querygraph_import_bytes).expect("write QueryGraph import");
        fs::write(&service_log, b"service log").expect("write service log");

        let mut summary = qglake_handoff_summary_json();
        summary["artifacts"] = json!({
            "bundle": {
                "path": bundle,
                "sha256": content_hash_bytes(b"bundle")
            },
            "lineageDrain": {
                "path": drain,
                "sha256": content_hash_bytes(b"drain")
            },
            "querygraphImportPlan": {
                "path": import_plan,
                "sha256": content_hash_bytes(b"import-plan")
            },
            "lakecatReplayOutput": lakecat_replay,
            "lakecatHandoffVerifyOutput": lakecat_handoff_verify,
            "querygraphVerifyOutput": querygraph_verify,
            "querygraphImportOutput": querygraph_import,
            "capturedOutputs": {
                "lakecatReplay": {
                    "path": lakecat_replay,
                    "sha256": content_hash_bytes(&lakecat_replay_bytes)
                },
                "querygraphVerify": {
                    "path": querygraph_verify,
                    "sha256": content_hash_bytes(&querygraph_verify_bytes)
                },
                "querygraphImport": {
                    "path": querygraph_import,
                    "sha256": content_hash_bytes(&querygraph_import_bytes)
                }
            },
            "serviceLog": service_log,
            "serviceLogHash": content_hash_bytes(b"service log")
        });
        summary
    }

    fn qglake_bind_handoff_verify_output_artifact(dir: &Path, summary: &mut Value) -> Value {
        let output = json!({
            "schemaVersion": "lakecat.qglake.handoff-verification.v1",
            "status": "verified",
            "principal": summary["principal"].clone(),
            "catalogUrl": summary["catalogUrl"].clone(),
            "warehouse": summary["warehouse"].clone(),
            "namespace": summary["namespace"].clone(),
            "table": summary["table"].clone(),
            "tableCount": summary["querygraphVerification"]["tableCount"].clone(),
            "viewCount": summary["querygraphVerification"]["viewCount"].clone(),
            "verifiedTables": summary["querygraphVerification"]["verifiedTables"].clone(),
            "verifiedViews": summary["querygraphVerification"]["verifiedViews"].clone(),
            "standards": summary["querygraphVerification"]["standards"].clone(),
            "requestIdentityProof": summary["lakecatReplayVerification"]["requestIdentityProof"].clone(),
            "queryGraphBootstrapProof": summary["lakecatReplayVerification"]["queryGraphBootstrapProof"].clone(),
            "artifactFiles": {
                "bundle": {
                    "sha256": summary["artifacts"]["bundle"]["sha256"].clone()
                },
                "lineageDrain": {
                    "sha256": summary["artifacts"]["lineageDrain"]["sha256"].clone()
                },
                "querygraphImportPlan": {
                    "sha256": summary["artifacts"]["querygraphImportPlan"]["sha256"].clone()
                },
                "capturedOutputs": {
                    "lakecatReplay": {
                        "sha256": summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"].clone()
                    },
                    "querygraphVerify": {
                        "sha256": summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"].clone()
                    },
                    "querygraphImport": {
                        "sha256": summary["artifacts"]["capturedOutputs"]["querygraphImport"]["sha256"].clone()
                    }
                },
                "serviceLogHash": summary["artifacts"]["serviceLogHash"].clone()
            },
            "capturedOutputSemantics": {
                "lakecatReplay": {
                    "requestIdentityProof": summary["lakecatReplayVerification"]["requestIdentityProof"].clone(),
                    "queryGraphBootstrapProof": summary["lakecatReplayVerification"]["queryGraphBootstrapProof"].clone()
                },
                "querygraphVerify": {
                    "tableCount": summary["querygraphVerification"]["tableCount"].clone(),
                    "viewCount": summary["querygraphVerification"]["viewCount"].clone(),
                    "verifiedTables": summary["querygraphVerification"]["verifiedTables"].clone(),
                    "verifiedViews": summary["querygraphVerification"]["verifiedViews"].clone(),
                    "bundleHash": summary["querygraphVerification"]["bundleHash"].clone(),
                    "graphHash": summary["querygraphVerification"]["graphHash"].clone(),
                    "openLineageHash": summary["querygraphVerification"]["openLineageHash"].clone(),
                    "queryGraphImportHash": summary["querygraphVerification"]["querygraphImportHash"].clone(),
                    "standards": summary["querygraphVerification"]["standards"].clone()
                },
                "querygraphImport": {
                    "tableCount": summary["querygraphImportVerification"]["tableCount"].clone(),
                    "viewCount": summary["querygraphImportVerification"]["viewCount"].clone(),
                    "verifiedTables": summary["querygraphImportVerification"]["verifiedTables"].clone(),
                    "verifiedViews": summary["querygraphImportVerification"]["verifiedViews"].clone(),
                    "bundleHash": summary["querygraphImportVerification"]["bundleHash"].clone(),
                    "graphHash": summary["querygraphImportVerification"]["graphHash"].clone(),
                    "openLineageHash": summary["querygraphImportVerification"]["openLineageHash"].clone(),
                    "queryGraphImportHash": summary["querygraphImportVerification"]["querygraphImportHash"].clone(),
                    "standards": summary["querygraphImportVerification"]["standards"].clone()
                }
            },
            "bundleArtifactSemantics": {
                "tableCount": summary["querygraphVerification"]["tableCount"].clone(),
                "viewCount": summary["querygraphVerification"]["viewCount"].clone(),
                "verifiedTables": summary["querygraphVerification"]["verifiedTables"].clone(),
                "verifiedViews": summary["querygraphVerification"]["verifiedViews"].clone(),
                "bundleHash": summary["querygraphVerification"]["bundleHash"].clone(),
                "graphHash": summary["querygraphVerification"]["graphHash"].clone(),
                "openLineageHash": summary["querygraphVerification"]["openLineageHash"].clone(),
                "queryGraphImportHash": summary["querygraphVerification"]["querygraphImportHash"].clone(),
                "standards": summary["querygraphVerification"]["standards"].clone(),
                "graphNodes": 6,
                "graphEdges": 7
            },
            "querygraphImportPlanSemantics": {
                "tableCount": summary["querygraphImportVerification"]["tableCount"].clone(),
                "viewCount": summary["querygraphImportVerification"]["viewCount"].clone(),
                "verifiedTables": summary["querygraphImportVerification"]["verifiedTables"].clone(),
                "verifiedViews": summary["querygraphImportVerification"]["verifiedViews"].clone(),
                "bundleHash": summary["querygraphImportVerification"]["bundleHash"].clone(),
                "graphHash": summary["querygraphImportVerification"]["graphHash"].clone(),
                "openLineageHash": summary["querygraphImportVerification"]["openLineageHash"].clone(),
                "queryGraphImportHash": summary["querygraphImportVerification"]["querygraphImportHash"].clone(),
                "standards": summary["querygraphImportVerification"]["standards"].clone(),
                "graphNodes": 6,
                "graphEdges": 7
            },
            "lineageDrainArtifactSemantics": {
                "delivered": 12,
                "eventTypes": [
                    "table.scan-planned",
                    "table.scan-tasks-fetched",
                    "credentials.vend-attempted",
                    "credentials.vend-attempted",
                    "view.upserted",
                    "policy-binding.listed",
                    "storage-profile.listed",
                    "storage-profile.upserted",
                    "server.listed",
                    "project.listed",
                    "warehouse.listed",
                    "table.commits-listed",
                    "querygraph.bootstrap"
                ],
                "graphEvents": 8,
                "lineageEvents": 13,
                "principalSubject": summary["lakecatReplayVerification"]["requestIdentityProof"]["principalSubject"].clone(),
                "principalKind": summary["lakecatReplayVerification"]["requestIdentityProof"]["principalKind"].clone(),
                "authorizationReceiptHash": summary["lakecatReplayVerification"]["requestIdentityProof"]["authorizationReceiptHash"].clone(),
                "requestIdentitySource": summary["lakecatReplayVerification"]["requestIdentityProof"]["requestIdentitySource"].clone(),
                "requestIdentityState": summary["lakecatReplayVerification"]["requestIdentityProof"]["requestIdentityState"].clone(),
                "typedidEnvelopeHash": summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"].clone(),
                "typedidProofHash": summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidProofHash"].clone(),
                "tableCount": summary["querygraphVerification"]["tableCount"].clone(),
                "viewCount": summary["querygraphVerification"]["viewCount"].clone(),
                "verifiedTables": summary["querygraphVerification"]["verifiedTables"].clone(),
                "verifiedViews": summary["querygraphVerification"]["verifiedViews"].clone(),
                "bundleHash": summary["querygraphVerification"]["bundleHash"].clone(),
                "graphHash": summary["querygraphVerification"]["graphHash"].clone(),
                "openLineageHash": summary["querygraphVerification"]["openLineageHash"].clone(),
                "queryGraphImportHash": summary["querygraphVerification"]["querygraphImportHash"].clone(),
                "standards": summary["querygraphVerification"]["standards"].clone()
            },
        });
        let bytes = serde_json::to_vec_pretty(&output).expect("handoff verify JSON bytes");
        fs::write(dir.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        output
    }

    fn qglake_handoff_summary_json_with_verified_bundle(
        dir: &Path,
    ) -> (Value, QueryGraphBootstrap) {
        let mut summary = qglake_handoff_summary_json_with_artifacts(dir);
        let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
        let output = serde_json::json!({
            "name": "events",
            "facets": {
                "dataSource": {
                    "uri": QGLAKE_TEST_LOCATION
                },
                "queryGraph_catalog": {
                    "stableId": projection.stable_id.clone(),
                    "metadataLocation": projection.metadata_location.clone()
                }
            }
        });
        let bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
        qglake_write_handoff_bundle_artifact(dir, &mut summary, &bundle);
        (summary, bundle)
    }

    fn qglake_write_handoff_bundle_artifact(
        dir: &Path,
        summary: &mut Value,
        bundle: &QueryGraphBootstrap,
    ) {
        let verification = bundle.verify_manifest().expect("bundle should verify");
        let bytes = serde_json::to_vec_pretty(bundle).expect("bundle JSON");
        fs::write(dir.join("lakecat-bootstrap.json"), &bytes).expect("write bundle");
        summary["artifacts"]["bundle"]["sha256"] = json!(content_hash_bytes(&bytes));
        for section in ["querygraphVerification", "querygraphImportVerification"] {
            summary[section]["tableCount"] = json!(verification.table_count);
            summary[section]["viewCount"] = json!(verification.view_count);
            summary[section]["verifiedTables"] = json!(verification.verified_tables);
            summary[section]["verifiedViews"] = json!(verification.verified_views);
            summary[section]["bundleHash"] = json!(verification.bundle_hash);
            summary[section]["graphHash"] = json!(verification.graph_hash);
            summary[section]["openLineageHash"] = json!(verification.open_lineage_hash);
            summary[section]["querygraphImportHash"] = json!(verification.querygraph_import_hash);
            summary[section]["standards"] = json!(verification.standards);
        }
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["bundleHash"] =
            json!(verification.bundle_hash);
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["graphHash"] =
            json!(verification.graph_hash);
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["openLineageHash"] =
            json!(verification.open_lineage_hash);
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["queryGraphImportHash"] =
            json!(verification.querygraph_import_hash);
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["tableArtifactCount"] =
            json!(verification.table_count);
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewArtifactCount"] =
            json!(verification.view_count);
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["standards"] =
            json!(verification.standards);
    }

    fn qglake_write_handoff_import_plan_artifact(dir: &Path, summary: &mut Value) -> Value {
        let import = summary["querygraphImportVerification"].clone();
        let tables = import["verifiedTables"]
            .as_array()
            .expect("verified tables array")
            .iter()
            .map(|stable_id| {
                json!({
                    "stable-id": stable_id,
                    "croissant-name": "events",
                    "cdif-title": "events",
                    "osi-model": "events",
                    "odrl-policy": "events#odrl"
                })
            })
            .collect::<Vec<_>>();
        let views = import["verifiedViews"]
            .as_array()
            .expect("verified views array")
            .iter()
            .map(|stable_id| {
                json!({
                    "stable-id": stable_id,
                    "name": "active_customers_view",
                    "view-version": 1,
                    "dialect": "ansi",
                    "osi-model": "active_customers_view"
                })
            })
            .collect::<Vec<_>>();
        let plan = json!({
            "verification": {
                "warehouse": summary["warehouse"],
                "table-count": import["tableCount"],
                "view-count": import["viewCount"],
                "verified-tables": import["verifiedTables"],
                "verified-views": import["verifiedViews"],
                "bundle-hash": import["bundleHash"],
                "graph-hash": import["graphHash"],
                "open-lineage-hash": import["openLineageHash"],
                "querygraph-import-hash": import["querygraphImportHash"],
                "standards": import["standards"]
            },
            "graph-nodes": 6,
            "graph-edges": 5,
            "tables": tables,
            "views": views
        });
        let bytes = serde_json::to_vec_pretty(&plan).expect("import plan JSON");
        fs::write(dir.join("querygraph-import-plan.json"), &bytes).expect("write import plan");
        summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));
        plan
    }

    fn qglake_handoff_lineage_verification() -> QueryGraphBootstrapVerification {
        QueryGraphBootstrapVerification {
            warehouse: "local".to_string(),
            table_count: 1,
            view_count: 1,
            verified_tables: vec!["lakecat:table:local:default:events".to_string()],
            verified_views: vec!["lakecat:view:local:default:active_customers_view".to_string()],
            verified_view_versions: BTreeMap::from([(
                "lakecat:view:local:default:active_customers_view".to_string(),
                1,
            )]),
            verified_view_receipt_hashes: BTreeMap::from([(
                "lakecat:view:local:default:active_customers_view".to_string(),
                "sha256:view-receipt".to_string(),
            )]),
            verified_view_receipt_chain_hashes: BTreeMap::from([(
                "lakecat:view:local:default:active_customers_view".to_string(),
                "sha256:view-receipt-chain".to_string(),
            )]),
            bundle_hash: "sha256:bundle".to_string(),
            graph_hash: "sha256:graph".to_string(),
            open_lineage_hash: "sha256:openlineage".to_string(),
            querygraph_import_hash: "sha256:querygraph-import".to_string(),
            standards: qglake_lineage_standards(),
        }
    }

    fn qglake_handoff_lineage_drain() -> LineageDrainResponse {
        let verification = qglake_handoff_lineage_verification();
        let mut view = qglake_view_lineage_summary();
        view.view_name = Some("active_customers_view".to_string());
        view.view_stable_id = Some("lakecat:view:local:default:active_customers_view".to_string());
        view.view_version = Some(1);
        view.expected_view_version = None;
        view.replay_event_hashes = vec!["sha256:view-replay".to_string()];
        view.replay_open_lineage_hashes = vec!["sha256:view-openlineage".to_string()];

        LineageDrainResponse {
            delivered: 12,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "policy-binding.listed".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 8,
            lineage_events: 13,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:identity".to_string()),
            request_identity_state: Some("unverified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary_for(&verification, 1),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                view,
                qglake_policy_list_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
                qglake_table_commit_history_lineage_summary(),
                qglake_scan_planned_lineage_summary(),
                qglake_scan_tasks_fetched_lineage_summary(),
            ],
        }
    }

    fn qglake_write_handoff_lineage_drain_artifact(
        dir: &Path,
        summary: &mut Value,
        drain: &LineageDrainResponse,
    ) {
        let verification = qglake_handoff_lineage_verification();
        for section in ["querygraphVerification", "querygraphImportVerification"] {
            summary[section]["tableCount"] = json!(verification.table_count);
            summary[section]["viewCount"] = json!(verification.view_count);
            summary[section]["verifiedTables"] = json!(verification.verified_tables);
            summary[section]["verifiedViews"] = json!(verification.verified_views);
            summary[section]["bundleHash"] = json!(verification.bundle_hash);
            summary[section]["graphHash"] = json!(verification.graph_hash);
            summary[section]["openLineageHash"] = json!(verification.open_lineage_hash);
            summary[section]["querygraphImportHash"] = json!(verification.querygraph_import_hash);
            summary[section]["standards"] = json!(verification.standards);
        }
        let replay = qglake_replay_verification_json(
            &verification,
            qglake_scan_replay_line(drain),
            qglake_management_replay_line(drain),
            qglake_credential_replay_line(drain, Some("did:example:agent")),
            qglake_table_commit_history_replay_line(drain),
            qglake_replay_evidence_json(drain, Some("did:example:agent"), &verification),
        );
        summary["lakecatReplayVerification"] = json!({
            "schemaVersion": replay["schema-version"],
            "status": replay["status"],
            "matchesQueryGraph": true,
            "requestIdentityProof": replay["replay-evidence"]["requestIdentity"],
            "queryGraphBootstrapProof": replay["replay-evidence"]["queryGraphBootstrap"],
            "governedScanProof": replay["replay-evidence"]["scan"],
            "tableCommitHistoryProof": replay["replay-evidence"]["tableCommitHistory"],
            "viewReceiptChainProof": replay["replay-evidence"]["views"],
            "managementProof": replay["replay-evidence"]["management"],
            "storageProfileUpsertProof": replay["replay-evidence"]["management"]["storageProfileUpsert"],
            "credentialVendingProof": replay["replay-evidence"]["credentials"],
            "replayEvidence": replay["replay-evidence"],
        });
        let bytes = serde_json::to_vec_pretty(drain).expect("lineage drain JSON");
        fs::write(dir.join("lineage-drain.json"), &bytes).expect("write lineage drain");
        summary["artifacts"]["lineageDrain"]["sha256"] = json!(content_hash_bytes(&bytes));
    }

    fn qglake_resync_bundle_hashes(bundle: &mut QueryGraphBootstrap) {
        let graph_hash = content_hash_json(&serde_json::to_value(&bundle.graph).unwrap()).unwrap();
        bundle.manifest.graph_hash = graph_hash.clone();
        bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["graphHash"] =
            json!(graph_hash);
        bundle.manifest.open_lineage_hash = content_hash_json(&bundle.open_lineage).unwrap();
        if let Some(import) = bundle.manifest.querygraph_import.as_mut() {
            import.graph_hash = bundle.manifest.graph_hash.clone();
        }
        let import_hash = qglake_querygraph_import_hash(
            &bundle.warehouse,
            &bundle.manifest,
            &bundle.tables,
            &bundle.graph,
            &bundle.open_lineage,
        );
        if let Some(import) = bundle.manifest.querygraph_import.as_mut() {
            import.table_only_bundle_hash = import_hash;
        }
        bundle.bundle_hash = content_hash_json(&serde_json::json!({
            "warehouse": bundle.warehouse.as_str(),
            "manifest": &bundle.manifest,
            "tables": &bundle.tables,
            "views": &bundle.views,
            "graph": &bundle.graph,
            "openLineage": &bundle.open_lineage,
        }))
        .unwrap();
    }

    fn qglake_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("lakecat-{label}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn parses_config_command_defaults() {
        let command = Command::parse(["config".to_string()]).unwrap();
        match command {
            Command::Config { catalog, principal } => {
                assert_eq!(catalog, "http://127.0.0.1:8181");
                assert_eq!(principal, None);
            }
            _ => panic!("expected config command"),
        }
    }

    #[test]
    fn parses_bootstrap_export_command() {
        let command = Command::parse([
            "bootstrap-export".to_string(),
            "--catalog".to_string(),
            "http://localhost:9000".to_string(),
            "--output".to_string(),
            "bundle.json".to_string(),
            "--principal".to_string(),
            "alice".to_string(),
        ])
        .unwrap();
        match command {
            Command::BootstrapExport {
                catalog,
                output,
                principal,
            } => {
                assert_eq!(catalog, "http://localhost:9000");
                assert_eq!(output, PathBuf::from("bundle.json"));
                assert_eq!(principal.as_deref(), Some("alice"));
            }
            _ => panic!("expected bootstrap-export command"),
        }
    }

    #[test]
    fn parses_lineage_drain_command() {
        let command = Command::parse([
            "lineage-drain".to_string(),
            "--catalog".to_string(),
            "http://localhost:9000".to_string(),
            "--principal".to_string(),
            "did:example:agent".to_string(),
        ])
        .unwrap();
        match command {
            Command::LineageDrain { catalog, principal } => {
                assert_eq!(catalog, "http://localhost:9000");
                assert_eq!(principal.as_deref(), Some("did:example:agent"));
            }
            _ => panic!("expected lineage-drain command"),
        }
    }

    #[test]
    fn parses_qglake_verify_replay_command() {
        let command = Command::parse([
            "qglake-verify-replay".to_string(),
            "--bundle".to_string(),
            "target/qglake/lakecat-bootstrap.json".to_string(),
            "--drain".to_string(),
            "target/qglake/lineage-drain.json".to_string(),
            "--principal".to_string(),
            "did:example:agent".to_string(),
            "--json".to_string(),
        ])
        .unwrap();
        match command {
            Command::QglakeVerifyReplay {
                bundle,
                drain,
                principal,
                json,
            } => {
                assert_eq!(
                    bundle,
                    PathBuf::from("target/qglake/lakecat-bootstrap.json")
                );
                assert_eq!(drain, PathBuf::from("target/qglake/lineage-drain.json"));
                assert_eq!(principal.as_deref(), Some("did:example:agent"));
                assert!(json);
            }
            _ => panic!("expected qglake-verify-replay command"),
        }
    }

    #[test]
    fn parses_qglake_verify_handoff_command() {
        let command = Command::parse([
            "qglake-verify-handoff".to_string(),
            "--summary".to_string(),
            "target/qglake-handoff/handoff-summary.json".to_string(),
            "--json".to_string(),
        ])
        .unwrap();
        match command {
            Command::QglakeVerifyHandoff { summary, json } => {
                assert_eq!(
                    summary,
                    PathBuf::from("target/qglake-handoff/handoff-summary.json")
                );
                assert!(json);
            }
            _ => panic!("expected qglake-verify-handoff command"),
        }
    }

    #[test]
    fn qglake_handoff_summary_verifier_accepts_compact_proofs() {
        let summary = qglake_handoff_summary_json();
        let verification =
            verify_qglake_handoff_summary_value(&summary).expect("handoff summary should verify");

        assert_eq!(
            verification["schemaVersion"],
            json!("lakecat.qglake.handoff-verification.v1")
        );
        assert_eq!(verification["status"], json!("verified"));
        assert_eq!(verification["principal"], json!("did:example:agent"));
        assert_eq!(verification["warehouse"], json!("local"));
        assert_eq!(verification["namespace"], json!("default"));
        assert_eq!(verification["table"], json!("events"));
        assert_eq!(verification["tableCount"], json!(1));
        assert_eq!(verification["viewCount"], json!(1));
        assert_eq!(
            verification["verifiedTables"],
            json!(["lakecat:table:local:default:events"])
        );
        assert_eq!(
            verification["verifiedViews"],
            json!(["lakecat:view:local:default:active_customers_view"])
        );
        assert_eq!(
            verification["queryGraphBootstrapProof"]["bundleHash"],
            json!("sha256:bundle")
        );
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_catalog_scope() {
        let mut summary = qglake_handoff_summary_json();
        summary.as_object_mut().unwrap().remove("warehouse");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing warehouse scope");

        assert!(err.to_string().contains("handoff summary"));
        assert!(err.to_string().contains("warehouse"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_empty_catalog_scope() {
        let mut summary = qglake_handoff_summary_json();
        summary["namespace"] = json!("");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject empty namespace scope");

        assert!(err.to_string().contains("handoff summary"));
        assert!(err.to_string().contains("namespace"));
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_malformed_catalog_url() {
        let mut summary = qglake_handoff_summary_json();
        summary["catalogUrl"] = json!("not a url");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject malformed catalog URLs");

        assert!(err.to_string().contains("catalogUrl"));
        assert!(err.to_string().contains("HTTP(S) URL"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_non_http_catalog_url() {
        let mut summary = qglake_handoff_summary_json();
        summary["catalogUrl"] = json!("file:///tmp/lakecat");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject non-HTTP catalog URLs");

        assert!(err.to_string().contains("catalogUrl"));
        assert!(err.to_string().contains("HTTP(S) URL"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_verified_table_scope() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphVerification"]["verifiedTables"] =
            json!(["lakecat:table:local:default:other"]);
        summary["querygraphImportVerification"]["verifiedTables"] =
            json!(["lakecat:table:local:default:other"]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject table scope drift");

        assert!(err.to_string().contains("querygraphVerification"));
        assert!(err.to_string().contains("verifiedTables"));
        assert!(
            err.to_string()
                .contains("lakecat:table:local:default:events")
        );
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_verified_table_count_mismatch() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphVerification"]["verifiedTables"] = json!([
            "lakecat:table:local:default:events",
            "lakecat:table:local:default:other"
        ]);
        summary["querygraphImportVerification"]["verifiedTables"] = json!([
            "lakecat:table:local:default:events",
            "lakecat:table:local:default:other"
        ]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject verified table count drift");

        assert!(err.to_string().contains("querygraphVerification"));
        assert!(err.to_string().contains("verifiedTables length mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_verified_view_scope() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphVerification"]["verifiedViews"] =
            json!(["lakecat:view:local:default:other_view"]);
        summary["querygraphImportVerification"]["verifiedViews"] =
            json!(["lakecat:view:local:default:other_view"]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject view scope drift");

        assert!(err.to_string().contains("querygraphVerification"));
        assert!(err.to_string().contains("verifiedViews"));
        assert!(
            err.to_string()
                .contains("lakecat:view:local:default:active_customers_view")
        );
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_verified_view_count_mismatch() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphVerification"]["verifiedViews"] = json!([
            "lakecat:view:local:default:active_customers_view",
            "lakecat:view:local:default:other_view"
        ]);
        summary["querygraphImportVerification"]["verifiedViews"] = json!([
            "lakecat:view:local:default:active_customers_view",
            "lakecat:view:local:default:other_view"
        ]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject verified view count drift");

        assert!(err.to_string().contains("querygraphVerification"));
        assert!(err.to_string().contains("verifiedViews length mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_import_verified_table_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphImportVerification"]["verifiedTables"] =
            json!(["lakecat:table:local:default:events_other"]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject import verified-table drift");

        assert!(err.to_string().contains("querygraphImportVerification"));
        assert!(err.to_string().contains("verifiedTables mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_import_hash_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphImportVerification"]["querygraphImportHash"] =
            json!("sha256:other-querygraph-import");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject import hash drift");

        assert!(err.to_string().contains("querygraphImportVerification"));
        assert!(err.to_string().contains("querygraphImportHash mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_core_bundle_hash_shape() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphVerification"]["bundleHash"] = json!("not-a-sha256-hash");
        summary["querygraphImportVerification"]["bundleHash"] = json!("not-a-sha256-hash");
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["bundleHash"] =
            json!("not-a-sha256-hash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject malformed bundle proof anchors");

        assert!(err.to_string().contains("querygraphVerification"));
        assert!(err.to_string().contains("bundleHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_core_import_hash_shape() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphVerification"]["querygraphImportHash"] = json!("not-a-sha256-hash");
        summary["querygraphImportVerification"]["querygraphImportHash"] =
            json!("not-a-sha256-hash");
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["queryGraphImportHash"] =
            json!("not-a-sha256-hash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject malformed QueryGraph import proof anchors");

        assert!(err.to_string().contains("querygraphVerification"));
        assert!(err.to_string().contains("querygraphImportHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_required_standards() {
        let mut summary = qglake_handoff_summary_json();
        let incomplete = json!([
            "Iceberg REST",
            "Croissant",
            "CDIF",
            "OSI handoff",
            "Grust catalog graph",
            "OpenLineage"
        ]);
        summary["querygraphVerification"]["standards"] = incomplete.clone();
        summary["querygraphImportVerification"]["standards"] = incomplete.clone();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["standards"] = incomplete;

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject incomplete QGLake standards");

        assert!(err.to_string().contains("querygraphVerification.standards"));
        assert!(err.to_string().contains("ODRL"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_bootstrap_hash_mismatch() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["bundleHash"] =
            json!("sha256:other");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject mismatched bootstrap hash");
        assert!(err.to_string().contains("bundleHash mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_management_policy_count_match() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["managementProof"]["policyBindingCount"] = json!(2);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject management policy-count drift");

        assert!(err.to_string().contains("managementProof"));
        assert!(err.to_string().contains("policyBindingCount mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_management_graph_events() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["managementProof"]["serverGraphEvents"] = json!(0);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing management graph proof");

        assert!(err.to_string().contains("managementProof"));
        assert!(err.to_string().contains("serverGraphEvents"));
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_management_receipt_hashes() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["managementProof"]["storageProfileReplayEventHashes"] =
            json!([]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing management receipt hashes");

        assert!(err.to_string().contains("managementProof"));
        assert!(err.to_string().contains("storageProfileReplayEventHashes"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_request_identity_typedid_hash_shape() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"] =
            json!("not-a-sha256-hash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject invalid request identity TypeDID hash");

        assert!(err.to_string().contains("requestIdentityProof"));
        assert!(err.to_string().contains("typedidEnvelopeHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_request_identity_typedid_proof_without_envelope() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidProofHash"] =
            json!("sha256:typedid-proof");

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject request identity TypeDID proof without envelope",
        );

        assert!(err.to_string().contains("requestIdentityProof"));
        assert!(err.to_string().contains("typedidProofHash"));
        assert!(err.to_string().contains("typedidEnvelopeHash"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_bootstrap_typedid_hash_shape() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidEnvelopeHash"] =
            json!("not-a-sha256-hash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject invalid bootstrap TypeDID hash");

        assert!(err.to_string().contains("queryGraphBootstrapProof"));
        assert!(err.to_string().contains("typedidEnvelopeHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_bootstrap_typedid_proof_without_envelope() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidProofHash"] =
            json!("sha256:typedid-proof");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject bootstrap TypeDID proof without envelope");

        assert!(err.to_string().contains("queryGraphBootstrapProof"));
        assert!(err.to_string().contains("typedidProofHash"));
        assert!(err.to_string().contains("typedidEnvelopeHash"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_allows_distinct_bootstrap_typedid_envelope() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"] =
            json!("sha256:typedid-envelope");
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidEnvelopeHash"] =
            json!("sha256:other-typedid-envelope");

        verify_qglake_handoff_summary_value(&summary)
            .expect("handoff summary should allow distinct request/bootstrap TypeDID envelopes");
    }

    #[test]
    fn qglake_handoff_summary_verifier_allows_distinct_bootstrap_typedid_proof() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"] =
            json!("sha256:typedid-envelope");
        summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidProofHash"] =
            json!("sha256:typedid-proof");
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidEnvelopeHash"] =
            json!("sha256:typedid-envelope");
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["typedidProofHash"] =
            json!("sha256:other-typedid-proof");

        verify_qglake_handoff_summary_value(&summary)
            .expect("handoff summary should allow distinct request/bootstrap TypeDID proofs");
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_bootstrap_identity_source_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["requestIdentitySource"] =
            json!("authorization-bearer");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject bootstrap identity source drift");

        assert!(err.to_string().contains("queryGraphBootstrapProof"));
        assert!(err.to_string().contains("requestIdentitySource mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_bootstrap_identity_state_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["requestIdentityState"] =
            json!("verified");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject bootstrap identity state drift");

        assert!(err.to_string().contains("queryGraphBootstrapProof"));
        assert!(err.to_string().contains("requestIdentityState mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_allows_distinct_bootstrap_authorization_receipt() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["authorizationReceiptHash"] =
            json!("sha256:other-authorization");

        verify_qglake_handoff_summary_value(&summary).expect(
            "handoff summary should allow distinct request/bootstrap authorization receipts",
        );
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_storage_profile_issuance_mode() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]
            .as_object_mut()
            .unwrap()
            .remove("issuanceMode");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing issuance mode");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("issuanceMode"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_storage_profile_location_prefix_hash() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]
            .as_object_mut()
            .unwrap()
            .remove("locationPrefixHash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing location-prefix hash");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("locationPrefixHash"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_storage_profile_location_hash_shape() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["locationPrefixHash"] =
            json!("not-a-sha256-hash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject invalid location-prefix hash evidence");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("locationPrefixHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_storage_profile_graph_events() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]
            .as_object_mut()
            .unwrap()
            .remove("graphEvents");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing storage-profile graph evidence");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("graphEvents"));

        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["graphEvents"] = json!(0);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject empty storage-profile graph evidence");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("graphEvents"));
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_secret_ref_provider_when_present() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
            json!(true);
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
            Value::Null;

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject secret-ref evidence without provider");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("secretRefProvider"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_secret_ref_hash_when_present() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
            json!(true);
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
            json!("vault");
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
            Value::Null;

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject secret-ref evidence without hash");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("secretRefHash"));

        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
            json!(true);
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
            json!("vault");
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
            json!("not-a-sha256-hash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject malformed secret-ref hash");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("secretRefHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_secret_ref_provider_when_absent() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
            json!(false);
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
            json!("vault");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject a provider when no secret ref is present");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("secretRefProvider"));
        assert!(err.to_string().contains("null"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_secret_ref_hash_when_absent() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
            json!(false);
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
            json!("sha256:storage-secret-ref");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject a hash when no secret ref is present");

        assert!(err.to_string().contains("storageProfileUpsertProof"));
        assert!(err.to_string().contains("secretRefHash"));
        assert!(err.to_string().contains("null"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_governed_scan_read_restriction() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]
            .as_object_mut()
            .unwrap()
            .remove("plannedReadRestriction");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing governed scan restriction");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("plannedReadRestriction"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_scan_delete_file_count() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]
            .as_object_mut()
            .unwrap()
            .remove("deleteFileCount");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing scan delete-file count");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("deleteFileCount"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_scan_plan_graph_events() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["planGraphEvents"] = json!(0);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing scan-plan graph proof");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("planGraphEvents"));
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_scan_child_plan_task_count() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["childPlanTaskCount"] = json!(0);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject empty scan child-plan-task proof");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("childPlanTaskCount"));
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_scan_stats_field_evidence() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]
            .as_object_mut()
            .unwrap()
            .remove("plannedRequestedStatsFields");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing scan stats-field evidence");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("plannedRequestedStatsFields"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_scan_projection_evidence() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]
            .as_object_mut()
            .unwrap()
            .remove("plannedRequestedProjection");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing scan projection evidence");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("plannedRequestedProjection"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_scan_projection_widening() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["plannedEffectiveProjection"] =
            json!(["event_id", "occurred_at", "severity", "raw_payload"]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject widened scan projection");

        assert!(err.to_string().contains("governedScanProof"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_unrequested_effective_scan_projection() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["plannedRequestedProjection"] =
            json!(["event_id", "occurred_at", "severity", "raw_payload"]);
        summary["lakecatReplayVerification"]["governedScanProof"]["plannedEffectiveProjection"] =
            json!(["event_id", "occurred_at", "tenant_id"]);

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject effective projection fields that were never requested",
        );

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("plannedEffectiveProjection"));
        assert!(err.to_string().contains("tenant_id"));
        assert!(err.to_string().contains("not requested"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_effective_scan_stats_field_evidence() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]
            .as_object_mut()
            .unwrap()
            .remove("plannedEffectiveStatsFields");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing effective stats-field evidence");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("plannedEffectiveStatsFields"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_scan_stats_field_widening() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["plannedEffectiveStatsFields"] =
            json!(["event_id", "occurred_at", "severity", "raw_payload"]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject widened scan stats fields");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(
            err.to_string()
                .contains("must prove a wider request than plannedEffectiveStatsFields")
        );
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_unrequested_effective_scan_stats_field() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["plannedRequestedStatsFields"] =
            json!(["event_id", "occurred_at", "severity", "raw_payload"]);
        summary["lakecatReplayVerification"]["governedScanProof"]["plannedEffectiveStatsFields"] =
            json!(["event_id", "occurred_at", "tenant_id"]);

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject effective stats fields that were never requested",
        );

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("plannedEffectiveStatsFields"));
        assert!(err.to_string().contains("tenant_id"));
        assert!(err.to_string().contains("not requested"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_scan_restriction_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["fetchedReadRestriction"]["allowed-columns"] =
            json!(["event_id", "raw_payload"]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject drifted scan restriction");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("allowed-columns mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_scan_restriction_purpose() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]
            .as_object_mut()
            .unwrap()
            .remove("purpose");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing scan restriction purpose");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("purpose"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_scan_restriction_purpose_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["fetchedReadRestriction"]["purpose"] =
            json!("different-purpose");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject drifted scan restriction purpose");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("purpose mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_scan_restriction_ttl_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["fetchedReadRestriction"]["max-credential-ttl-seconds"] =
            json!(60);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject drifted scan restriction TTL cap");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(
            err.to_string()
                .contains("max-credential-ttl-seconds mismatch")
        );
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_fetch_requirement_evidence() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]
            .as_object_mut()
            .unwrap()
            .remove("fetchedRequiredProjection");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing fetch requirement evidence");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("fetchedRequiredProjection"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_fetch_effective_projection_evidence() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]
            .as_object_mut()
            .unwrap()
            .remove("fetchedEffectiveProjection");

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject missing fetch effective projection evidence",
        );

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("fetchedEffectiveProjection"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_fetch_filter_evidence() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]
            .as_object_mut()
            .unwrap()
            .remove("fetchedRequiredFilters");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing fetched filter evidence");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("fetchedRequiredFilters"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_extra_fetch_filter_evidence() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["fetchedRequiredFilters"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "type": "eq",
                "term": "tenant_id",
                "value": "other"
            }));

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject extra fetched filter evidence");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("fetchedRequiredFilters"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_scan_openlineage_hashes() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["governedScanProof"]["fetchedOpenLineageHashes"] =
            json!([]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing scan OpenLineage hashes");

        assert!(err.to_string().contains("governedScanProof"));
        assert!(err.to_string().contains("fetchedOpenLineageHashes"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_table_commit_history_count_match() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitCount"] = json!(2);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject commit-history count drift");

        assert!(err.to_string().contains("tableCommitHistoryProof"));
        assert!(err.to_string().contains("sequenceNumbers"));
        assert!(err.to_string().contains("length mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_positive_commit_sequences() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["sequenceNumbers"] =
            json!([0]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject non-positive commit sequences");

        assert!(err.to_string().contains("tableCommitHistoryProof"));
        assert!(err.to_string().contains("sequenceNumbers"));
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_increasing_commit_sequences() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitCount"] = json!(2);
        summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["sequenceNumbers"] =
            json!([1, 1]);
        summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitHashes"] =
            json!(["sha256:first", "sha256:second"]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject duplicate commit sequences");

        assert!(err.to_string().contains("tableCommitHistoryProof"));
        assert!(err.to_string().contains("sequenceNumbers"));
        assert!(err.to_string().contains("strictly increasing"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_commit_history_replay_hashes() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["replayEventHashes"] =
            json!([]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing commit-history replay hashes");

        assert!(err.to_string().contains("tableCommitHistoryProof"));
        assert!(err.to_string().contains("replayEventHashes"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_commit_history_graph_events() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["graphEvents"] = json!(0);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing commit-history graph projection");

        assert!(err.to_string().contains("tableCommitHistoryProof"));
        assert!(err.to_string().contains("graphEvents"));
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_restricted_credential_hashes() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["replayEventHashes"] =
            json!([]);

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject missing restricted credential replay hashes",
        );

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("replayEventHashes"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_restricted_raw_exception_flag() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]
            .as_object_mut()
            .unwrap()
            .remove("rawCredentialExceptionAllowed");

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should require restricted raw credential exception evidence",
        );

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("rawCredentialExceptionAllowed"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_restricted_raw_exception() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["rawCredentialExceptionAllowed"] =
            json!(true);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject restricted raw credential exceptions");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(
            err.to_string()
                .contains("must not allow a raw credential exception")
        );
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_restricted_exception_reason() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["rawCredentialExceptionReason"] =
            json!("trusted-human-override");

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject a raw credential exception reason on blocked restricted proofs",
        );

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("rawCredentialExceptionReason"));
        assert!(err.to_string().contains("must be null"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_credential_ttl_cap() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]
            .as_object_mut()
            .unwrap()
            .remove("maxCredentialTtlSeconds");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing credential TTL evidence");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("maxCredentialTtlSeconds"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_credential_ttl_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["maxCredentialTtlSeconds"] =
            json!(60);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject credential TTL drift");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("maxCredentialTtlSeconds mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_credential_storage_profile_graph_evidence() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]
            .as_object_mut()
            .unwrap()
            .remove("storageProfile");

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject credential proof without storage-profile graph evidence",
        );

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("storageProfile"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_credential_location_prefix_hash() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]
            ["storageProfile"]
            .as_object_mut()
            .unwrap()
            .remove("locationPrefixHash");

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject credential proof without storage-scope hash evidence",
        );

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("locationPrefixHash"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_credential_secret_ref_provider_when_present() {
        let mut summary = qglake_handoff_summary_json();
        let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"]
            ["restricted"]["storageProfile"]
            .as_object_mut()
            .unwrap();
        storage_profile.insert("secretRefPresent".to_string(), json!(true));
        storage_profile.insert("secretRefProvider".to_string(), Value::Null);
        storage_profile.insert(
            "secretRefHash".to_string(),
            json!("sha256:credential-secret-ref"),
        );

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject credential secret-ref presence without provider",
        );

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("secretRefProvider"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_credential_secret_ref_hash_when_present() {
        let mut summary = qglake_handoff_summary_json();
        let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"]
            ["trustedHuman"]["storageProfile"]
            .as_object_mut()
            .unwrap();
        storage_profile.insert("secretRefPresent".to_string(), json!(true));
        storage_profile.insert("secretRefProvider".to_string(), json!("vault"));
        storage_profile.insert("secretRefHash".to_string(), Value::Null);

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject credential secret-ref presence without hash",
        );

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("secretRefHash"));

        let mut summary = qglake_handoff_summary_json();
        let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"]
            ["trustedHuman"]["storageProfile"]
            .as_object_mut()
            .unwrap();
        storage_profile.insert("secretRefPresent".to_string(), json!(true));
        storage_profile.insert("secretRefProvider".to_string(), json!("vault"));
        storage_profile.insert("secretRefHash".to_string(), json!("not-a-sha256-hash"));

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject malformed credential secret-ref hash");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("secretRefHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_credential_secret_ref_hash_when_absent() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["storageProfile"]
            ["secretRefHash"] = json!("sha256:credential-secret-ref");

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject credential secret-ref hash without presence",
        );

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("secretRefHash"));
        assert!(err.to_string().contains("null"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_credential_storage_profile_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["storageProfile"]
            ["profileId"] = json!("other-profile");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject credential storage-profile drift");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("profileId"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_credential_secret_ref_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
            json!(true);
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
            json!("vault");
        summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
            json!("sha256:storage-secret-ref");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject credential secret-ref state drift");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("secretRefPresent"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_trusted_human_exception_reason() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["rawCredentialExceptionReason"] =
            json!("because I feel like it");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject unaudited trusted-human exception reasons");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("rawCredentialExceptionReason"));
        assert!(
            err.to_string()
                .contains(QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON)
        );
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_trusted_human_null_block_reason() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]
            .as_object_mut()
            .unwrap()
            .remove("blockReason");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should require trusted-human block reason proof");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("blockReason"));

        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["blockReason"] =
            json!("blocked");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject blocked trusted-human credential proof");

        assert!(err.to_string().contains("credentialVendingProof"));
        assert!(err.to_string().contains("blockReason must be null"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_view_tombstone_expected_version() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["expectedViewVersion"] =
            Value::Null;

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject unguarded view tombstones");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("expectedViewVersion"));
        assert!(err.to_string().contains("non-negative integer"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_view_tombstone_version_mismatch() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["expectedViewVersion"] =
            json!(99);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject stale view tombstone guards");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("expectedViewVersion mismatch"));
        assert!(err.to_string().contains("expected=1 actual=99"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_view_accepted_receipt_hashes() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptHash"] =
            json!("not-a-sha256-hash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject invalid accepted view receipt hashes");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("acceptedReceiptHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_bootstrap_view_receipt_hash_drift() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewVersionReceiptHashes"] =
            json!(["sha256:other-view-receipt"]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject spliced bootstrap view receipt hashes");

        assert!(
            err.to_string()
                .contains("queryGraphBootstrapProof.viewVersionReceiptHashes")
        );
        assert!(err.to_string().contains("acceptedReceiptHash"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_view_graph_events() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["graphEvents"] =
            json!(0);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing accepted-view graph projection");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("graphEvents"));
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_view_accepted_receipt_chain_hashes() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptChainHash"] =
            json!("not-a-sha256-hash");

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject invalid accepted view receipt-chain hashes");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("acceptedReceiptChainHash"));
        assert!(err.to_string().contains("sha256"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_uncovered_view_receipt_chain_hash() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptChainHash"] =
            json!("sha256:uncovered-view-receipt-chain");
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"] =
            json!([]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject accepted view chain hashes not covered by namespace chain evidence");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("acceptedReceiptChainHash"));
        assert!(err.to_string().contains("receiptChains"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_view_count_mismatch() {
        let mut summary = qglake_handoff_summary_json();
        summary["querygraphVerification"]["viewCount"] = json!(2);
        summary["querygraphVerification"]["verifiedViews"] = json!([
            "lakecat:view:local:default:active_customers_view",
            "lakecat:view:local:default:other_view"
        ]);
        summary["querygraphImportVerification"]["viewCount"] = json!(2);
        summary["querygraphImportVerification"]["verifiedViews"] = json!([
            "lakecat:view:local:default:active_customers_view",
            "lakecat:view:local:default:other_view"
        ]);
        summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewArtifactCount"] =
            json!(2);
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["viewCount"] = json!(2);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject view-count drift");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("views length mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_unaccepted_view_version() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedViewVersion"] =
            json!(2);
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["expectedViewVersion"] =
            json!(2);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject unaccepted view version evidence");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("accepted view version"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_view_receipt_chain_identity() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["namespace"] =
            json!([]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject receipt chains without namespace identity");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("namespace"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_view_receipt_chain_hashes() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chainHashes"] =
            json!([]);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject missing view receipt-chain hashes");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("chainHashes"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_requires_verified_view_receipt_chain_count() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["verifiedChainCount"] =
            json!(0);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject unverified view receipt chains");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("verifiedChainCount"));
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_view_receipt_chain_count_mismatch() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["verifiedChainCount"] =
            json!(2);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject mismatched chain counts");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("verifiedChainCount mismatch"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_view_receipt_hash_undercoverage() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chainHashes"] =
            json!(["sha256:chain-a", "sha256:chain-b"]);
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["verifiedChainCount"] =
            json!(2);

        let err = verify_qglake_handoff_summary_value(&summary)
            .expect_err("handoff summary should reject receipt hashes that under-cover chains");

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("receiptHashes"));
        assert!(err.to_string().contains("cover"));
    }

    #[test]
    fn qglake_handoff_summary_verifier_rejects_uncovered_view_tombstone_receipts() {
        let mut summary = qglake_handoff_summary_json();
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["receiptHashes"] =
            json!(["sha256:uncovered-tombstone"]);

        let err = verify_qglake_handoff_summary_value(&summary).expect_err(
            "handoff summary should reject tombstone receipts outside the namespace chain",
        );

        assert!(err.to_string().contains("viewReceiptChainProof"));
        assert!(err.to_string().contains("tombstoneReceipts"));
        assert!(err.to_string().contains("receiptChains"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_accepts_matching_files() {
        let temp = qglake_temp_dir("handoff-artifacts-ok");
        let summary_path = temp.join("handoff-summary.json");
        let summary = qglake_handoff_summary_json_with_artifacts(&temp);
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let verification = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect("artifact hashes should verify");
        assert_eq!(
            verification["bundle"]["sha256"],
            json!(content_hash_bytes(b"bundle"))
        );
        assert_eq!(
            verification["capturedOutputs"]["querygraphVerify"]["sha256"],
            summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"]
        );
        assert_eq!(
            verification["pathAliases"]["querygraphVerifyOutput"],
            summary["artifacts"]["querygraphVerifyOutput"]
        );
        assert_eq!(
            verification["pathAliases"]["serviceLog"],
            summary["artifacts"]["serviceLog"]
        );
    }

    #[test]
    fn qglake_handoff_artifact_verifier_accepts_handoff_verify_output_hash() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-ok");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let verification = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect("artifact hashes should verify with handoff verifier output");

        assert_eq!(
            verification["pathAliases"]["lakecatHandoffVerifyOutputHash"],
            summary["artifacts"]["lakecatHandoffVerifyOutputHash"]
        );
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_hash_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-hash-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        fs::write(
            temp.join("lakecat-handoff-verify.json"),
            b"tampered verification",
        )
        .expect("tamper handoff verify output");
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier output hash drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_scope_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-scope-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["table"] = json!("other_events");
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier output scope drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(err.to_string().contains("table mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_semantic_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-semantic-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["verifiedTables"] = json!(["lakecat:table:local:default:other_events"]);
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier semantic drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(err.to_string().contains("verifiedTables mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_proof_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-proof-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["queryGraphBootstrapProof"]["bundleHash"] = json!("sha256:other-bundle");
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier proof drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(
            err.to_string()
                .contains("queryGraphBootstrapProof mismatch")
        );
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_artifact_hash_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-artifact-hash-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["artifactFiles"]["bundle"]["sha256"] = json!("sha256:other-bundle-bytes");
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier artifact hash drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(err.to_string().contains("artifactFiles"));
        assert!(err.to_string().contains("sha256 mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_capture_hash_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-capture-hash-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["artifactFiles"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!("sha256:other-replay-capture");
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier capture hash drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(err.to_string().contains("capturedOutputs"));
        assert!(err.to_string().contains("sha256 mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_captured_semantic_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-captured-semantic-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["capturedOutputSemantics"]["querygraphVerify"]["graphHash"] =
            json!("sha256:other-graph");
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier captured semantic drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(err.to_string().contains("capturedOutputSemantics"));
        assert!(err.to_string().contains("graphHash mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_lineage_identity_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-lineage-identity-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["lineageDrainArtifactSemantics"]["requestIdentitySource"] =
            json!("x-lakecat-human-did");
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier identity drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(err.to_string().contains("lineageDrainArtifactSemantics"));
        assert!(err.to_string().contains("requestIdentitySource mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_handoff_verify_output_graph_count_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-self-verify-graph-count-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut output = qglake_bind_handoff_verify_output_artifact(&temp, &mut summary);
        output["querygraphImportPlanSemantics"]["graphNodes"] = json!(5);
        let bytes = serde_json::to_vec_pretty(&output).expect("drifted handoff verify JSON");
        fs::write(temp.join("lakecat-handoff-verify.json"), &bytes)
            .expect("write drifted handoff verify output");
        summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject handoff verifier graph count drift");

        assert!(err.to_string().contains("lakecatHandoffVerifyOutput"));
        assert!(err.to_string().contains("querygraphImportPlanSemantics"));
        assert!(err.to_string().contains("graphNodes mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_drifted_path_alias() {
        let temp = qglake_temp_dir("handoff-artifacts-alias-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        summary["artifacts"]["querygraphVerifyOutput"] =
            json!(temp.join("other-querygraph-verify.json"));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject alias drift");

        assert!(err.to_string().contains("querygraphVerifyOutput"));
        assert!(
            err.to_string()
                .contains("capturedOutputs.querygraphVerify.path")
        );
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_service_log_hash_drift() {
        let temp = qglake_temp_dir("handoff-artifacts-service-log-drift");
        let summary_path = temp.join("handoff-summary.json");
        let summary = qglake_handoff_summary_json_with_artifacts(&temp);
        fs::write(temp.join("lakecat-service.log"), b"tampered service log")
            .expect("tamper service log");
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact verifier should reject service log hash drift");

        assert!(err.to_string().contains("serviceLog"));
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_accept_matching_files() {
        let temp = qglake_temp_dir("handoff-captured-semantics-ok");
        let summary_path = temp.join("handoff-summary.json");
        let summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let semantics = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect("captured output semantics should verify");

        assert_eq!(
            semantics["lakecatReplay"]["schemaVersion"],
            json!("lakecat.qglake.replay-verification.v1")
        );
        assert_eq!(
            semantics["querygraphVerify"]["bundleHash"],
            json!("sha256:bundle")
        );
        assert_eq!(
            semantics["querygraphVerify"]["verifiedTables"],
            json!(["lakecat:table:local:default:events"])
        );
        assert_eq!(
            semantics["querygraphVerify"]["verifiedViews"],
            json!(["lakecat:view:local:default:active_customers_view"])
        );
        assert_eq!(
            semantics["querygraphImport"]["queryGraphImportHash"],
            json!("sha256:querygraph-import")
        );
        assert_eq!(
            semantics["lakecatReplay"]["storageProfileUpsertProof"]["locationPrefixHash"],
            json!("sha256:storage-location-prefix")
        );
        assert_eq!(
            semantics["lakecatReplay"]["requestIdentityProof"]["principalSubject"],
            json!("did:example:agent")
        );
        assert_eq!(
            semantics["lakecatReplay"]["queryGraphBootstrapProof"]["agentDelegationHash"],
            json!("sha256:delegation")
        );
        assert_eq!(
            semantics["lakecatReplay"]["governedScanProof"]["planTaskCount"],
            json!(1)
        );
        assert_eq!(
            semantics["lakecatReplay"]["managementProof"]["policyBindingCount"],
            json!(1)
        );
        assert_eq!(
            semantics["lakecatReplay"]["tableCommitHistoryProof"]["commitHashes"],
            json!(["sha256:commit"])
        );
        assert_eq!(
            semantics["lakecatReplay"]["tableCommitHistoryProof"]["graphEvents"],
            json!(1)
        );
        assert_eq!(
            semantics["lakecatReplay"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptHash"],
            json!("sha256:view-receipt")
        );
        assert_eq!(
            semantics["lakecatReplay"]["credentialVendingProof"]["restricted"]["blockReason"],
            json!(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
        );
    }

    #[test]
    fn qglake_handoff_bundle_artifact_semantics_accept_verified_bundle() {
        let temp = qglake_temp_dir("handoff-bundle-semantics-ok");
        let summary_path = temp.join("handoff-summary.json");
        let (summary, bundle) = qglake_handoff_summary_json_with_verified_bundle(&temp);
        let bundle_verification = bundle.verify_manifest().expect("bundle verifies");

        let semantics = verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)
            .expect("bundle artifact semantics should verify");

        assert_eq!(
            semantics["bundleHash"],
            json!(bundle_verification.bundle_hash)
        );
        assert_eq!(
            semantics["verifiedTables"],
            json!(["lakecat:table:local:default:events"])
        );
        assert_eq!(semantics["viewCount"], json!(0));
    }

    #[test]
    fn qglake_handoff_bundle_artifact_semantics_rejects_detached_tenant_graph() {
        let temp = qglake_temp_dir("handoff-bundle-semantics-tenant-drift");
        let summary_path = temp.join("handoff-summary.json");
        let (mut summary, mut bundle) = qglake_handoff_summary_json_with_verified_bundle(&temp);
        bundle.graph.edges.retain(|edge| edge.label != "HAS_SERVER");
        qglake_resync_bundle_hashes(&mut bundle);
        qglake_write_handoff_bundle_artifact(&temp, &mut summary, &bundle);

        let err = verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)
            .expect_err("bundle artifact semantics should reject detached tenant graph");

        assert!(err.to_string().contains("Catalog to a Server"));
    }

    #[test]
    fn qglake_handoff_querygraph_import_plan_semantics_accept_matching_plan() {
        let temp = qglake_temp_dir("handoff-import-plan-semantics-ok");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        qglake_write_handoff_import_plan_artifact(&temp, &mut summary);

        let semantics =
            verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
                .expect("QueryGraph import plan artifact semantics should verify");

        assert_eq!(
            semantics["verifiedTables"],
            json!(["lakecat:table:local:default:events"])
        );
        assert_eq!(
            semantics["verifiedViews"],
            json!(["lakecat:view:local:default:active_customers_view"])
        );
        assert_eq!(semantics["graphNodes"], json!(6));
    }

    #[test]
    fn qglake_handoff_querygraph_import_plan_semantics_rejects_table_drift() {
        let temp = qglake_temp_dir("handoff-import-plan-semantics-table-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
        plan["tables"][0]["stable-id"] = json!("lakecat:table:local:default:other_events");
        let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
        fs::write(temp.join("querygraph-import-plan.json"), &bytes)
            .expect("write drifted import plan");
        summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));

        let err = verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
            .expect_err("QueryGraph import plan artifact semantics should reject table drift");

        assert!(
            err.to_string()
                .contains("tables must include stable-id lakecat:table:local:default:events")
        );
    }

    #[test]
    fn qglake_handoff_artifact_semantics_reject_saved_import_plan_graph_count_drift() {
        let temp = qglake_temp_dir("handoff-import-plan-graph-drift");
        let summary_path = temp.join("handoff-summary.json");
        let (mut summary, _) = qglake_handoff_summary_json_with_verified_bundle(&temp);
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"] = json!([]);
        let mut plan = qglake_write_handoff_import_plan_artifact(&temp, &mut summary);
        plan["graph-nodes"] = json!(5);
        let bytes = serde_json::to_vec_pretty(&plan).expect("drifted import plan JSON");
        fs::write(temp.join("querygraph-import-plan.json"), &bytes)
            .expect("write drifted import plan");
        summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));
        fs::write(
            &summary_path,
            serde_json::to_vec_pretty(&summary).expect("summary JSON"),
        )
        .expect("write summary");

        let bundle_semantics =
            verify_qglake_handoff_bundle_artifact_semantics(&summary_path, &summary)
                .expect("bundle artifact semantics should verify");
        let import_plan_semantics =
            verify_qglake_handoff_querygraph_import_plan_semantics(&summary_path, &summary)
                .expect("import plan artifact semantics should verify before cross-check");
        let err = require_qglake_import_plan_graph_counts_match_bundle(
            &bundle_semantics,
            &import_plan_semantics,
        )
        .expect_err("handoff should reject saved import-plan graph count drift");

        assert!(err.to_string().contains("querygraphImportPlanSemantics"));
        assert!(err.to_string().contains("graphNodes mismatch"));
    }

    #[test]
    fn qglake_handoff_rejects_import_plan_graph_count_drift() {
        let bundle = json!({
            "graphNodes": 8,
            "graphEdges": 8
        });
        let import_plan = json!({
            "graphNodes": 7,
            "graphEdges": 8
        });

        let err = require_qglake_import_plan_graph_counts_match_bundle(&bundle, &import_plan)
            .expect_err("handoff should reject import-plan graph count drift");

        assert!(err.to_string().contains("querygraphImportPlanSemantics"));
        assert!(err.to_string().contains("graphNodes mismatch"));
    }

    #[test]
    fn qglake_handoff_lineage_drain_artifact_semantics_accept_matching_drain() {
        let temp = qglake_temp_dir("handoff-lineage-drain-semantics-ok");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let drain = qglake_handoff_lineage_drain();
        qglake_write_handoff_lineage_drain_artifact(&temp, &mut summary, &drain);

        let semantics =
            verify_qglake_handoff_lineage_drain_artifact_semantics(&summary_path, &summary)
                .expect("lineage drain artifact semantics should verify");

        assert_eq!(semantics["delivered"], json!(12));
        assert_eq!(
            semantics["verifiedViews"],
            json!(["lakecat:view:local:default:active_customers_view"])
        );
        assert_eq!(
            semantics["queryGraphImportHash"],
            json!("sha256:querygraph-import")
        );
        assert_eq!(
            semantics["requestIdentitySource"],
            json!("x-lakecat-agent-did")
        );
        assert_eq!(semantics["requestIdentityState"], json!("unverified"));
        assert_eq!(semantics["typedidEnvelopeHash"], Value::Null);
        assert_eq!(semantics["typedidProofHash"], Value::Null);
    }

    #[test]
    fn qglake_handoff_lineage_drain_artifact_semantics_rejects_replay_drift() {
        let temp = qglake_temp_dir("handoff-lineage-drain-semantics-replay-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drain = qglake_handoff_lineage_drain();
        qglake_write_handoff_lineage_drain_artifact(&temp, &mut summary, &drain);
        let bootstrap = drain
            .events
            .iter_mut()
            .find(|event| event.event_type == "querygraph.bootstrap")
            .expect("bootstrap replay event");
        bootstrap.replay_event_hashes = vec!["sha256:drifted-bootstrap-replay".to_string()];
        let bytes = serde_json::to_vec_pretty(&drain).expect("drifted lineage drain JSON");
        fs::write(temp.join("lineage-drain.json"), &bytes).expect("write drifted lineage drain");
        summary["artifacts"]["lineageDrain"]["sha256"] = json!(content_hash_bytes(&bytes));

        let err = verify_qglake_handoff_lineage_drain_artifact_semantics(&summary_path, &summary)
            .expect_err("lineage drain artifact semantics should reject replay drift");

        assert!(
            err.to_string()
                .contains("captured LakeCat replay output.replay-evidence.queryGraphBootstrap")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_summary_verified_table_drift() {
        let temp = qglake_temp_dir("handoff-captured-summary-table-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        summary["querygraphVerification"]["verifiedTables"] =
            json!(["lakecat:table:local:default:events_other"]);

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured output semantics should reject summary verified-table drift");

        assert!(
            err.to_string()
                .contains("captured QueryGraph verify output.verified-tables mismatch")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_summary_verified_view_drift() {
        let temp = qglake_temp_dir("handoff-captured-summary-view-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        summary["querygraphVerification"]["verifiedViews"] =
            json!(["lakecat:view:local:default:active_customers_view_other"]);

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured output semantics should reject summary verified-view drift");

        assert!(
            err.to_string()
                .contains("captured QueryGraph verify output.verified-views mismatch")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_import_summary_drift() {
        let temp = qglake_temp_dir("handoff-captured-import-summary-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        summary["querygraphImportVerification"]["verifiedTables"] =
            json!(["lakecat:table:local:default:events_other"]);

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured output semantics should reject import summary drift");

        assert!(err.to_string().contains("querygraphImportVerification"));
        assert!(err.to_string().contains("verifiedTables mismatch"));
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_hash_mismatch() {
        let temp = qglake_temp_dir("handoff-artifacts-mismatch");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        fs::write(temp.join("lakecat-bootstrap.json"), b"tampered").expect("tamper bundle");
        summary["artifacts"]["bundle"]["sha256"] = json!(content_hash_bytes(b"bundle"));

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("artifact hashes should reject tampered files");
        assert!(
            err.to_string()
                .contains("handoff artifact bundle hash mismatch")
        );
    }

    #[test]
    fn qglake_handoff_artifact_verifier_rejects_captured_output_mismatch() {
        let temp = qglake_temp_dir("handoff-captured-output-mismatch");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        fs::write(temp.join("querygraph-verify.json"), b"tampered")
            .expect("tamper QueryGraph verify output");
        summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
            json!(content_hash_bytes(b"querygraph-verify"));

        let err = verify_qglake_handoff_artifact_files(&summary_path, &summary)
            .expect_err("captured output hashes should reject tampered files");
        assert!(
            err.to_string()
                .contains("handoff artifact querygraphVerify hash mismatch")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_summary_drift() {
        let temp = qglake_temp_dir("handoff-captured-semantics-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let drifted = json!({
            "warehouse": "local",
            "table-count": 1,
            "view-count": 1,
            "verified-tables": [
                "lakecat:table:local:default:events"
            ],
            "verified-views": [
                "lakecat:view:local:default:active_customers_view"
            ],
            "bundle-hash": "sha256:other-bundle",
            "graph-hash": "sha256:graph",
            "open-lineage-hash": "sha256:openlineage",
            "querygraph-import-hash": "sha256:querygraph-import",
            "standards": [
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "ODRL",
                "Grust catalog graph",
                "OpenLineage"
            ]
        });
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("querygraph-verify.json"), &drifted_bytes)
            .expect("write drifted QueryGraph verify output");
        summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured output semantics should reject drift");
        assert!(
            err.to_string()
                .contains("captured QueryGraph verify output.bundle-hash mismatch")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_querygraph_warehouse_drift() {
        let temp = qglake_temp_dir("handoff-captured-warehouse-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("querygraph-verify.json")).expect("read QueryGraph verify");
        drifted["warehouse"] = json!("other");
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("querygraph-verify.json"), &drifted_bytes)
            .expect("write drifted QueryGraph verify output");
        summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured QueryGraph warehouse drift should be rejected");

        assert!(
            err.to_string()
                .contains("captured QueryGraph verify output.warehouse mismatch")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_querygraph_table_scope_drift() {
        let temp = qglake_temp_dir("handoff-captured-table-scope-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("querygraph-verify.json")).expect("read QueryGraph verify");
        drifted["verified-tables"] = json!(["lakecat:table:local:default:other"]);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("querygraph-verify.json"), &drifted_bytes)
            .expect("write drifted QueryGraph verify output");
        summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured QueryGraph table-scope drift should be rejected");

        assert!(
            err.to_string()
                .contains("captured QueryGraph verify output.verified-tables")
        );
        assert!(
            err.to_string()
                .contains("lakecat:table:local:default:events")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_querygraph_view_scope_drift() {
        let temp = qglake_temp_dir("handoff-captured-view-scope-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("querygraph-verify.json")).expect("read QueryGraph verify");
        drifted["verified-views"] = json!(["lakecat:view:local:default:other_view"]);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("querygraph-verify.json"), &drifted_bytes)
            .expect("write drifted QueryGraph verify output");
        summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured QueryGraph view-scope drift should be rejected");

        assert!(
            err.to_string()
                .contains("captured QueryGraph verify output.verified-views")
        );
        assert!(
            err.to_string()
                .contains("lakecat:view:local:default:active_customers_view")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_storage_profile_drift() {
        let temp = qglake_temp_dir("handoff-captured-storage-profile-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["management"]["storageProfileUpsert"]["locationPrefixHash"] =
            json!("sha256:other-storage-location-prefix");
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay storage-profile proof drift should be rejected");
        assert!(
            err.to_string().contains(
                "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert.locationPrefixHash mismatch"
            )
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_storage_profile_graph_drift() {
        let temp = qglake_temp_dir("handoff-captured-storage-profile-graph-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["management"]["storageProfileUpsert"]["graphEvents"] = json!(2);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay storage-profile graph proof drift should be rejected");
        assert!(
            err.to_string().contains(
                "captured LakeCat replay output.replay-evidence.management.storageProfileUpsert.graphEvents mismatch"
            )
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_governed_scan_drift() {
        let temp = qglake_temp_dir("handoff-captured-scan-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["scan"]["planTaskCount"] = json!(2);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay governed scan proof drift should be rejected");
        assert!(err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.scan.planTaskCount mismatch"
        ));
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_scan_projection_drift() {
        let temp = qglake_temp_dir("handoff-captured-scan-projection-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["scan"]["fetchedRequiredProjection"] =
            json!(["event_id", "occurred_at", "severity", "raw_payload"]);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay scan projection proof drift should be rejected");
        assert!(err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.scan.fetchedRequiredProjection mismatch"
        ));
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_scan_stats_field_drift() {
        let temp = qglake_temp_dir("handoff-captured-scan-stats-field-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["scan"]["plannedEffectiveStatsFields"] =
            json!(["event_id", "occurred_at", "severity", "raw_payload"]);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay scan stats-field proof drift should be rejected");
        assert!(err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.scan.plannedEffectiveStatsFields mismatch"
        ));
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_scan_filter_drift() {
        let temp = qglake_temp_dir("handoff-captured-scan-filter-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["scan"]["fetchedRequiredFilters"] = json!([{
            "type": "not-eq",
            "term": "severity",
            "value": "trace"
        }]);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay scan filter proof drift should be rejected");
        assert!(err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.scan.fetchedRequiredFilters mismatch"
        ));
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_table_commit_history_drift() {
        let temp = qglake_temp_dir("handoff-captured-table-commit-history-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["tableCommitHistory"]["commitCount"] = json!(2);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay commit-history proof drift should be rejected");
        assert!(err.to_string().contains(
            "captured LakeCat replay output.replay-evidence.tableCommitHistory.commitCount mismatch"
        ));
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_view_receipt_chain_drift() {
        let temp = qglake_temp_dir("handoff-captured-view-receipt-chain-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["views"]["views"][0]["acceptedReceiptHash"] =
            json!("sha256:other-view-receipt");
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay view receipt-chain proof drift should be rejected");
        assert!(
            err.to_string()
                .contains("captured LakeCat replay output.replay-evidence.views.views mismatch")
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_request_identity_drift() {
        let temp = qglake_temp_dir("handoff-captured-request-identity-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["requestIdentity"]["authorizationReceiptHash"] =
            json!("sha256:other-identity");
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay request-identity proof drift should be rejected");
        assert!(
            err.to_string().contains(
                "captured LakeCat replay output.replay-evidence.requestIdentity.authorizationReceiptHash mismatch"
            )
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_querygraph_bootstrap_drift() {
        let temp = qglake_temp_dir("handoff-captured-querygraph-bootstrap-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["queryGraphBootstrap"]["agentDelegationHash"] =
            json!("sha256:other-delegation");
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay QueryGraph bootstrap proof drift should be rejected");
        assert!(
            err.to_string().contains(
                "captured LakeCat replay output.replay-evidence.queryGraphBootstrap.agentDelegationHash mismatch"
            )
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_credential_drift() {
        let temp = qglake_temp_dir("handoff-captured-credential-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["credentials"]["restricted"]["blockReason"] =
            json!("raw credentials allowed");
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay credential proof drift should be rejected");
        assert!(
            err.to_string().contains(
                "captured LakeCat replay output.replay-evidence.credentials.restricted.blockReason mismatch"
            )
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_restricted_exception_drift() {
        let temp = qglake_temp_dir("handoff-captured-restricted-exception-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["credentials"]["restricted"]["rawCredentialExceptionAllowed"] =
            json!(true);
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay restricted exception drift should be rejected");
        assert!(
            err.to_string().contains(
                "captured LakeCat replay output.replay-evidence.credentials.restricted.rawCredentialExceptionAllowed mismatch"
            )
        );
    }

    #[test]
    fn qglake_handoff_captured_output_semantics_rejects_trusted_block_reason_drift() {
        let temp = qglake_temp_dir("handoff-captured-trusted-block-reason-drift");
        let summary_path = temp.join("handoff-summary.json");
        let mut summary = qglake_handoff_summary_json_with_artifacts(&temp);
        let mut drifted =
            read_json_file(&temp.join("lakecat-replay.txt")).expect("read LakeCat replay output");
        drifted["replay-evidence"]["credentials"]["trustedHuman"]["blockReason"] = json!("blocked");
        let drifted_bytes = serde_json::to_vec_pretty(&drifted).expect("drifted JSON bytes");
        fs::write(temp.join("lakecat-replay.txt"), &drifted_bytes)
            .expect("write drifted LakeCat replay output");
        summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"] =
            json!(content_hash_bytes(&drifted_bytes));

        let err = verify_qglake_handoff_captured_output_semantics(&summary_path, &summary)
            .expect_err("captured replay trusted-human block reason drift should be rejected");
        assert!(
            err.to_string().contains(
                "captured LakeCat replay output.replay-evidence.credentials.trustedHuman.blockReason mismatch"
            )
        );
    }

    #[test]
    fn parses_storage_profile_upsert_command() {
        let command = Command::parse([
            "storage-profile-upsert".to_string(),
            "--profile".to_string(),
            "local-events".to_string(),
            "--location-prefix".to_string(),
            "file:///tmp/events".to_string(),
            "--provider".to_string(),
            "file".to_string(),
            "--issuance-mode".to_string(),
            "local-file-no-secret".to_string(),
            "--public-config".to_string(),
            "lakecat.test=true".to_string(),
        ])
        .unwrap();
        match command {
            Command::StorageProfileUpsert {
                warehouse,
                profile,
                location_prefix,
                provider,
                issuance_mode,
                public_config,
                ..
            } => {
                assert_eq!(warehouse, "local");
                assert_eq!(profile, "local-events");
                assert_eq!(location_prefix, "file:///tmp/events");
                assert_eq!(provider, "file");
                assert_eq!(issuance_mode, "local-file-no-secret");
                assert_eq!(public_config["lakecat.test"], "true");
            }
            _ => panic!("expected storage-profile-upsert command"),
        }
    }

    #[test]
    fn parses_policy_upsert_command() {
        let command = Command::parse([
            "policy-upsert".to_string(),
            "--policy".to_string(),
            "agent-read".to_string(),
            "--namespace".to_string(),
            "default.analytics".to_string(),
            "--table".to_string(),
            "events".to_string(),
            "--enforced".to_string(),
            "false".to_string(),
        ])
        .unwrap();
        match command {
            Command::PolicyUpsert {
                policy,
                namespace,
                table,
                enforced,
                ..
            } => {
                assert_eq!(policy, "agent-read");
                assert_eq!(
                    namespace,
                    Some(vec!["default".to_string(), "analytics".to_string()])
                );
                assert_eq!(table.as_deref(), Some("events"));
                assert!(!enforced);
            }
            _ => panic!("expected policy-upsert command"),
        }
    }

    #[test]
    fn parses_qglake_fixture_command_defaults() {
        let command = Command::parse(["qglake-fixture".to_string()]).unwrap();
        match command {
            Command::QglakeFixture {
                warehouse,
                namespace,
                table,
                location,
                output,
                drain_output,
                ..
            } => {
                assert_eq!(warehouse, "local");
                assert_eq!(namespace, vec!["default".to_string()]);
                assert_eq!(table, "events");
                assert_eq!(location, "file:///tmp/lakecat-qglake/events");
                assert_eq!(
                    output,
                    PathBuf::from("target/qglake/lakecat-bootstrap.json")
                );
                assert_eq!(drain_output, None);
            }
            _ => panic!("expected qglake-fixture command"),
        }
    }

    #[test]
    fn parses_qglake_fixture_drain_output() {
        let command = Command::parse([
            "qglake-fixture".to_string(),
            "--drain-output".to_string(),
            "target/qglake/lineage-drain.json".to_string(),
        ])
        .unwrap();
        match command {
            Command::QglakeFixture { drain_output, .. } => {
                assert_eq!(
                    drain_output,
                    Some(PathBuf::from("target/qglake/lineage-drain.json"))
                );
            }
            _ => panic!("expected qglake-fixture command"),
        }
    }

    #[test]
    fn qglake_fixture_metadata_contains_restricted_raw_payload_column() {
        let (location, metadata_location) = qglake_test_fixture_urls("metadata");
        let metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
        let fields = metadata["schemas"][0]["fields"].as_array().unwrap();
        assert!(fields.iter().any(|field| field["name"] == "raw_payload"));
        assert!(metadata_has_manifest_list(&metadata));
        assert!(
            file_url_path(
                metadata["snapshots"][0]["manifest-list"].as_str().unwrap(),
                "test"
            )
            .unwrap()
            .exists()
        );
        let metadata_file = file_url_path(&metadata_location, "test").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(metadata_file).unwrap()).unwrap()["format-version"],
            json!(3)
        );
    }

    #[test]
    fn qglake_namespace_validator_accepts_matching_namespace() {
        let response = ListNamespacesResponse {
            namespaces: vec![
                vec!["default".to_string()],
                vec!["demo".to_string(), "ops".to_string()],
            ],
        };

        assert!(namespace_list_contains(
            &response,
            &["demo".to_string(), "ops".to_string()]
        ));
        assert!(!namespace_list_contains(
            &response,
            &["missing".to_string()]
        ));
    }

    #[test]
    fn qglake_existing_table_verifier_accepts_matching_fixture_table() {
        let (location, metadata_location) = qglake_test_fixture_urls("matching");
        let response = LoadTableResponse {
            identifier: TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            metadata_location: Some(metadata_location.clone()),
            metadata: qglake_table_metadata(&location, &metadata_location).unwrap(),
            config: Vec::new(),
        };

        verify_qglake_existing_table(
            &response,
            &["default".to_string()],
            "events",
            &metadata_location,
        )
        .expect("matching QGLake fixture table should be accepted");
    }

    #[test]
    fn qglake_existing_table_verifier_rejects_drifted_fixture_table() {
        let (location, metadata_location) = qglake_test_fixture_urls("drifted");
        let mut metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
        metadata["schemas"][0]["fields"]
            .as_array_mut()
            .unwrap()
            .retain(|field| field["name"] != "raw_payload");
        write_qglake_metadata_file(
            &file_url_path(&metadata_location, "test").unwrap(),
            &metadata,
        )
        .unwrap();
        let response = LoadTableResponse {
            identifier: TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            metadata_location: Some(metadata_location.clone()),
            metadata,
            config: Vec::new(),
        };

        let err = verify_qglake_existing_table(
            &response,
            &["default".to_string()],
            "events",
            &metadata_location,
        )
        .expect_err("drifted QGLake fixture table should be rejected");
        assert!(err.to_string().contains("raw_payload"));
    }

    #[test]
    fn qglake_existing_table_verifier_rejects_missing_metadata_pointer_file() {
        let (location, metadata_location) = qglake_test_fixture_urls("missing-pointer");
        let metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
        fs::remove_file(file_url_path(&metadata_location, "test").unwrap()).unwrap();
        let response = LoadTableResponse {
            identifier: TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            metadata_location: Some(metadata_location.clone()),
            metadata,
            config: Vec::new(),
        };

        let err = verify_qglake_existing_table(
            &response,
            &["default".to_string()],
            "events",
            &metadata_location,
        )
        .expect_err("missing QGLake metadata pointer should be rejected");
        assert!(err.to_string().contains("not readable"));
    }

    #[test]
    fn qglake_existing_table_verifier_rejects_drifted_metadata_pointer_file() {
        let (location, metadata_location) = qglake_test_fixture_urls("drifted-pointer");
        let metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
        let metadata_file = file_url_path(&metadata_location, "test").unwrap();
        let mut drifted = metadata.clone();
        drifted["last-sequence-number"] = json!(99);
        write_qglake_metadata_file(&metadata_file, &drifted).unwrap();
        let response = LoadTableResponse {
            identifier: TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            metadata_location: Some(metadata_location.clone()),
            metadata,
            config: Vec::new(),
        };

        let err = verify_qglake_existing_table(
            &response,
            &["default".to_string()],
            "events",
            &metadata_location,
        )
        .expect_err("drifted QGLake metadata pointer should be rejected");
        assert!(err.to_string().contains("does not match"));
    }

    #[test]
    fn qglake_existing_table_verifier_rejects_missing_manifest_list_file() {
        let (location, metadata_location) = qglake_test_fixture_urls("missing-manifest-list");
        let metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
        fs::remove_file(
            file_url_path(
                metadata["snapshots"][0]["manifest-list"].as_str().unwrap(),
                "test",
            )
            .unwrap(),
        )
        .unwrap();
        let response = LoadTableResponse {
            identifier: TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            metadata_location: Some(metadata_location.clone()),
            metadata,
            config: Vec::new(),
        };

        let err = verify_qglake_existing_table(
            &response,
            &["default".to_string()],
            "events",
            &metadata_location,
        )
        .expect_err("missing QGLake manifest list should be rejected");
        assert!(err.to_string().contains("manifest list"));
    }

    fn qglake_test_fixture_urls(name: &str) -> (String, String) {
        let root = std::env::temp_dir().join(format!(
            "lakecat-qglake-cli-{name}-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let table_dir = root.join("events");
        let metadata_file = table_dir.join("metadata").join("00000.json");
        (
            Url::from_directory_path(&table_dir).unwrap().to_string(),
            Url::from_file_path(metadata_file).unwrap().to_string(),
        )
    }

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

        let err =
            verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
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

        let err =
            verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
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

        let err =
            verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
                .expect_err("QGLake bootstrap projection should require policy TTL cap");

        assert!(err.to_string().contains("policy max credential TTL"));
    }

    #[test]
    fn qglake_bootstrap_projection_verifier_rejects_embedded_odrl_drift() {
        let mut projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
        projection.odrl["lakecat:policy-bindings"][0]["odrl"]["lakecat:read-restriction"]["max-credential-ttl-seconds"] =
            serde_json::json!(60);

        let err =
            verify_qglake_bootstrap_projection(&projection, &["default".to_string()], "events")
                .expect_err("QGLake bootstrap projection should reject embedded ODRL drift");

        assert!(err.to_string().contains("embedded ODRL policy binding"));
    }

    #[test]
    fn qglake_bootstrap_verifier_requires_openlineage_output() {
        let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
        let bundle = qglake_querygraph_bundle(vec![projection], Vec::new());

        let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
            .unwrap_err();
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
        assert!(err.to_string().contains(
            "OpenLineage semantic bundle did not advertise required standard OpenLineage"
        ));
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
        server.properties["endpointUrlHash"] = json!("sha256:endpoint-url");
        qglake_resync_bundle_hashes(&mut bundle);

        let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
            .expect_err("QGLake bootstrap should reject raw tenant server endpoint URLs");
        assert!(err.to_string().contains("raw endpointUrl"));
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
        warehouse.properties["storageRootHash"] = json!("sha256:storage-root");
        qglake_resync_bundle_hashes(&mut bundle);

        let err = verify_qglake_bootstrap_bundle(&bundle, &["default".to_string()], "events")
            .expect_err("QGLake bootstrap should reject raw tenant warehouse storage roots");
        assert!(err.to_string().contains("raw storageRoot"));
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
        bundle.graph.edges.retain(|edge| {
            !(edge.from == "lakecat:warehouse:local" && edge.label == "HAS_NAMESPACE")
        });

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

    #[test]
    fn qglake_replay_artifact_verifier_accepts_matching_bundle_and_drain() {
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
        let verification = bundle.verify_manifest().unwrap();
        let policy_binding_count = qglake_policy_binding_count(&bundle);
        let drain = LineageDrainResponse {
            delivered: 11,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 12,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary_for(&verification, policy_binding_count),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
                qglake_table_commit_history_lineage_summary(),
                qglake_scan_planned_lineage_summary(),
                qglake_scan_tasks_fetched_lineage_summary(),
            ],
        };

        let replay_verification =
            verify_qglake_replay_artifacts(&bundle, &drain, Some("did:example:agent"))
                .expect("matching saved bundle and lineage drain should verify");
        assert_eq!(replay_verification.bundle_hash, verification.bundle_hash);
        assert_eq!(
            replay_verification.querygraph_import_hash,
            verification.querygraph_import_hash
        );
        let replay_json = qglake_replay_verification_json(
            &replay_verification,
            qglake_scan_replay_line(&drain),
            qglake_management_replay_line(&drain),
            qglake_credential_replay_line(&drain, Some("did:example:agent")),
            qglake_table_commit_history_replay_line(&drain),
            qglake_replay_evidence_json(&drain, Some("did:example:agent"), &replay_verification),
        );
        assert_eq!(
            replay_json["schema-version"],
            json!("lakecat.qglake.replay-verification.v1")
        );
        assert_eq!(
            replay_json["replay-evidence"]["scan"]["planTaskCount"],
            json!(1)
        );
        assert_eq!(
            replay_json["replay-evidence"]["requestIdentity"]["principalSubject"],
            json!("did:example:agent")
        );
        assert_eq!(
            replay_json["replay-evidence"]["requestIdentity"]["principalKind"],
            json!("agent")
        );
        assert_eq!(
            replay_json["replay-evidence"]["requestIdentity"]["requestIdentitySource"],
            json!("x-lakecat-agent-did")
        );
        assert_eq!(
            replay_json["replay-evidence"]["requestIdentity"]["requestIdentityState"],
            json!("verified")
        );
        assert_eq!(
            replay_json["replay-evidence"]["requestIdentity"]["authorizationReceiptHash"],
            json!("sha256:lineage-read")
        );
        assert_eq!(
            replay_json["replay-evidence"]["queryGraphBootstrap"]["bundleHash"],
            json!(verification.bundle_hash)
        );
        assert_eq!(
            replay_json["replay-evidence"]["queryGraphBootstrap"]["queryGraphImportHash"],
            json!(verification.querygraph_import_hash)
        );
        assert_eq!(
            replay_json["replay-evidence"]["queryGraphBootstrap"]["policyBindingCount"],
            json!(1)
        );
        assert_eq!(
            replay_json["replay-evidence"]["queryGraphBootstrap"]["agentDelegationHash"],
            json!("sha256:delegation")
        );
        assert_eq!(
            replay_json["replay-evidence"]["queryGraphBootstrap"]["agentSummarySignatureHash"],
            json!("sha256:summary")
        );
        assert_eq!(
            replay_json["replay-evidence"]["management"]["policyBindingCount"],
            json!(1)
        );
        assert_eq!(
            replay_json["replay-evidence"]["management"]["storageProfileUpsert"]["provider"],
            json!("file")
        );
        assert_eq!(
            replay_json["replay-evidence"]["management"]["storageProfileUpsert"]["issuanceMode"],
            json!("local-file-no-secret")
        );
        assert_eq!(
            replay_json["replay-evidence"]["management"]["storageProfileUpsert"]["locationPrefixHash"],
            json!("sha256:storage-location-prefix")
        );
        assert_eq!(
            replay_json["replay-evidence"]["management"]["storageProfileUpsert"]["secretRefPresent"],
            json!(false)
        );
        assert_eq!(
            replay_json["replay-evidence"]["credentials"]["restricted"]["blockReason"],
            json!(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
        );
        assert_eq!(
            replay_json["replay-evidence"]["credentials"]["trustedHuman"]["rawCredentialExceptionAllowed"],
            json!(true)
        );
        assert_eq!(
            replay_json["replay-evidence"]["tableCommitHistory"]["sequenceNumbers"],
            json!([1])
        );
        assert_eq!(
            replay_json["replay-evidence"]["views"]["viewCount"],
            json!(0)
        );

        let view_verification = qglake_view_lineage_verification();
        let mut bootstrap_with_view = qglake_bootstrap_lineage_summary();
        bootstrap_with_view.view_artifact_count = 1;
        bootstrap_with_view.view_version_receipt_hashes =
            vec!["sha256:view-version-receipt".to_string()];
        let view_drain = LineageDrainResponse {
            delivered: 15,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "view.dropped".to_string(),
                "view.version-receipts-listed".to_string(),
                "view.version-receipt-chains-listed".to_string(),
                "policy-binding.listed".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 16,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view,
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
                qglake_view_drop_lineage_summary(),
                qglake_view_tombstone_receipt_lineage_summary(),
                qglake_view_receipt_chain_lineage_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
                qglake_table_commit_history_lineage_summary(),
                qglake_scan_planned_lineage_summary(),
                qglake_scan_tasks_fetched_lineage_summary(),
            ],
        };
        let view_replay_json =
            qglake_replay_evidence_json(&view_drain, Some("did:example:agent"), &view_verification);
        assert_eq!(
            view_replay_json["views"]["views"][0]["stableId"],
            json!("lakecat:view:local:default:active_customers")
        );
        assert_eq!(
            view_replay_json["views"]["views"][0]["acceptedViewVersion"],
            json!(2)
        );
        assert_eq!(
            view_replay_json["views"]["views"][0]["expectedViewVersion"],
            json!(1)
        );
        assert_eq!(
            view_replay_json["views"]["views"][0]["acceptedReceiptHash"],
            json!("sha256:view-version-receipt")
        );
        assert_eq!(
            view_replay_json["views"]["views"][0]["acceptedReceiptChainHash"],
            json!("sha256:view-receipt-chain")
        );
        assert_eq!(
            view_replay_json["views"]["tombstoneReceipts"][0]["expectedViewVersion"],
            json!(2)
        );
        assert_eq!(
            view_replay_json["views"]["tombstoneReceipts"][0]["receiptHashes"],
            json!(["sha256:view-drop-receipt"])
        );
        assert_eq!(
            view_replay_json["views"]["receiptChains"][0]["chainHashes"],
            json!(["sha256:view-receipt-chain"])
        );
        assert_eq!(
            view_replay_json["views"]["receiptChains"][0]["verifiedChainCount"],
            json!(1)
        );
    }

    #[test]
    fn qglake_commit_history_verifier_requires_iceberg_summary() {
        let record = qglake_table_commit_record_summary();
        verify_qglake_table_commit_record_evidence(&record, "local", "default", "events")
            .expect("QGLake commit history should accept compact Iceberg summary evidence");

        let mut missing_summary = record;
        missing_summary.format_version = None;
        missing_summary.snapshot_id = None;
        let err = verify_qglake_table_commit_record_evidence(
            &missing_summary,
            "local",
            "default",
            "events",
        )
        .expect_err("QGLake commit history should require format/snapshot summary evidence");
        assert!(err.to_string().contains(
            "qglake table commit history for local.default.events is missing Iceberg format/snapshot summary evidence"
        ));
    }

    #[test]
    fn qglake_fixture_policy_installs_read_restriction() {
        let policy = qglake_odrl_policy("events");
        assert_eq!(
            policy["lakecat:read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id", "occurred_at", "severity"])
        );
        assert_eq!(
            policy["lakecat:read-restriction"]["row-predicate"],
            serde_json::json!({
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            })
        );
        assert_eq!(
            policy["lakecat:read-restriction"]["purpose"],
            serde_json::json!("qglake-agent-demo")
        );
        assert_eq!(
            policy["lakecat:read-restriction"]["max-credential-ttl-seconds"],
            serde_json::json!(300)
        );
        let restriction = lakecat_security::ReadRestriction::from_odrl_policies([&policy])
            .expect("qglake policy should parse as LakeCat read restriction");
        assert_eq!(
            restriction.allowed_columns.as_deref(),
            Some(
                &[
                    "event_id".to_string(),
                    "occurred_at".to_string(),
                    "severity".to_string()
                ][..]
            )
        );
        assert_eq!(
            restriction.row_predicate,
            Some(serde_json::json!({
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            }))
        );
        assert_eq!(restriction.purpose.as_deref(), Some("qglake-agent-demo"));
        assert_eq!(restriction.max_credential_ttl_seconds, Some(300));
    }

    #[test]
    fn qglake_scan_plan_verifier_requires_governed_projection() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let plan = PlanTableScanResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            status: "completed".to_string(),
            snapshot_id: None,
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
            lakecat_plan_tasks: qglake_manifest_plan_tasks(),
            file_scan_tasks: Vec::new(),
            delete_files: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:scan-request": {
                    "requested-projection": [
                        "event_id",
                        "occurred_at",
                        "severity",
                        "raw_payload"
                    ],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                    "stats-fields": ["event_id", "occurred_at", "severity"],
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    }
                }
            })),
        };

        verify_qglake_scan_plan(&plan).unwrap();
    }

    #[test]
    fn qglake_scan_plan_verifier_rejects_missing_plan_task_token() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let plan = PlanTableScanResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            status: "completed".to_string(),
            snapshot_id: None,
            plan_tasks: Vec::new(),
            lakecat_plan_tasks: qglake_manifest_plan_tasks(),
            file_scan_tasks: Vec::new(),
            delete_files: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:scan-request": {
                    "requested-projection": [
                        "event_id",
                        "occurred_at",
                        "severity",
                        "raw_payload"
                    ],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                    "stats-fields": ["event_id", "occurred_at", "severity"],
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    }
                }
            })),
        };

        let err = verify_qglake_scan_plan(&plan)
            .expect_err("QGLake governed scan should expose a plan-task token");
        assert!(err.to_string().contains("plan-task token"));
    }

    #[test]
    fn qglake_scan_plan_verifier_rejects_missing_manifest_list_task() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let plan = PlanTableScanResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            status: "completed".to_string(),
            snapshot_id: None,
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
            lakecat_plan_tasks: vec![serde_json::json!({"task-type": "metadata-only"})],
            file_scan_tasks: Vec::new(),
            delete_files: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:scan-request": {
                    "requested-projection": [
                        "event_id",
                        "occurred_at",
                        "severity",
                        "raw_payload"
                    ],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                    "stats-fields": ["event_id", "occurred_at", "severity"],
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    }
                }
            })),
        };

        let err = verify_qglake_scan_plan(&plan)
            .expect_err("QGLake governed scan should expose a manifest-list task");
        assert!(err.to_string().contains("manifest-list task"));
    }

    #[test]
    fn qglake_scan_plan_verifier_rejects_non_sail_planner() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let plan = PlanTableScanResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "memory-test-planner".to_string(),
            status: "completed".to_string(),
            snapshot_id: None,
            plan_tasks: Vec::new(),
            lakecat_plan_tasks: Vec::new(),
            file_scan_tasks: Vec::new(),
            delete_files: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:scan-request": {
                    "requested-projection": [
                        "event_id",
                        "occurred_at",
                        "severity",
                        "raw_payload"
                    ],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                    "stats-fields": ["event_id", "occurred_at", "severity"],
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    }
                }
            })),
        };

        let err = verify_qglake_scan_plan(&plan)
            .expect_err("QGLake governed scan should require Sail planning");
        assert!(err.to_string().contains("not planned by Sail REST models"));
    }

    #[test]
    fn qglake_scan_plan_verifier_requires_policy_hash_binding() {
        let plan = PlanTableScanResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            status: "completed".to_string(),
            snapshot_id: None,
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
            lakecat_plan_tasks: qglake_manifest_plan_tasks(),
            file_scan_tasks: Vec::new(),
            delete_files: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:scan-request": {
                    "requested-projection": [
                        "event_id",
                        "occurred_at",
                        "severity",
                        "raw_payload"
                    ],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                    "stats-fields": ["event_id", "occurred_at", "severity"],
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300
                    }
                }
            })),
        };

        let err = verify_qglake_scan_plan(&plan)
            .expect_err("QGLake governed scan should require a policy hash binding");
        assert!(
            err.to_string()
                .contains("read restriction did not include policy hashes")
        );
    }

    #[test]
    fn qglake_scan_plan_verifier_requires_read_restriction_purpose() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let plan = PlanTableScanResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            status: "completed".to_string(),
            snapshot_id: None,
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
            lakecat_plan_tasks: qglake_manifest_plan_tasks(),
            file_scan_tasks: Vec::new(),
            delete_files: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:scan-request": {
                    "requested-projection": [
                        "event_id",
                        "occurred_at",
                        "severity",
                        "raw_payload"
                    ],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                    "stats-fields": ["event_id", "occurred_at", "severity"],
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
                }
            })),
        };

        let err = verify_qglake_scan_plan(&plan)
            .expect_err("QGLake governed scan should require read restriction purpose");
        assert!(err.to_string().contains("purpose"));
    }

    #[test]
    fn qglake_scan_plan_verifier_requires_read_restriction_ttl_cap() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let plan = PlanTableScanResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            status: "completed".to_string(),
            snapshot_id: None,
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest-list".to_string()],
            lakecat_plan_tasks: qglake_manifest_plan_tasks(),
            file_scan_tasks: Vec::new(),
            delete_files: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:scan-request": {
                    "requested-projection": [
                        "event_id",
                        "occurred_at",
                        "severity",
                        "raw_payload"
                    ],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "requested-stats-fields": ["event_id", "occurred_at", "severity", "raw_payload"],
                    "effective-stats-fields": ["event_id", "occurred_at", "severity"],
                    "stats-fields": ["event_id", "occurred_at", "severity"],
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "policy-hashes": [expected_policy_hash]
                    }
                }
            })),
        };

        let err = verify_qglake_scan_plan(&plan)
            .expect_err("QGLake governed scan should require read restriction TTL cap");
        assert!(err.to_string().contains("max credential TTL"));
    }

    fn qglake_manifest_plan_tasks() -> Vec<Value> {
        vec![serde_json::json!({
            "task-type": "manifest-list",
            "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro"
        })]
    }

    fn qglake_manifest_child_plan_tasks() -> Vec<Value> {
        vec![serde_json::json!({
            "task-type": "manifest",
            "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro",
            "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/manifest-42.avro",
            "content": "data"
        })]
    }

    fn qglake_file_scan_task_with_delete_ref() -> Value {
        serde_json::json!({
            "data-file": {
                "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
            },
            "delete-file-references": [0]
        })
    }

    fn qglake_delete_files() -> Vec<Value> {
        vec![serde_json::json!({
            "content": "position-deletes",
            "file-path": "file:///tmp/lakecat-qglake/events/delete/pos-delete-1.parquet"
        })]
    }

    #[test]
    fn qglake_leaf_fetch_scan_tasks_verifier_accepts_terminal_manifest_expansion() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:manifest".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: Vec::new(),
            lakecat_plan_tasks: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        verify_qglake_leaf_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect("QGLake leaf manifest fetch should be terminal and governed");
    }

    #[test]
    fn qglake_leaf_fetch_scan_tasks_verifier_rejects_more_child_tasks() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:manifest".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:unexpected".to_string()],
            lakecat_plan_tasks: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_leaf_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake leaf manifest fetch should be terminal");
        assert!(err.to_string().contains("unexpectedly exposed"));
    }

    #[test]
    fn qglake_delete_manifest_fetch_scan_tasks_verifier_accepts_terminal_delete_work() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:delete-manifest".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: Vec::new(),
            delete_files: qglake_delete_files(),
            plan_tasks: Vec::new(),
            lakecat_plan_tasks: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        verify_qglake_delete_manifest_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect("QGLake delete manifest fetch should be terminal and governed");
    }

    #[test]
    fn qglake_delete_manifest_fetch_scan_tasks_verifier_rejects_escaped_delete_files() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:delete-manifest".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: Vec::new(),
            delete_files: vec![serde_json::json!({
                "content": "position-deletes",
                "file-path": "file:///tmp/lakecat-qglake/other/delete/pos-delete-1.parquet"
            })],
            plan_tasks: Vec::new(),
            lakecat_plan_tasks: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_delete_manifest_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake delete manifest fetch should reject escaped delete files");
        assert!(
            err.to_string()
                .contains("delete file escaped table location")
        );
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_requires_reapplied_policy_hash_binding() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
            delete_files: qglake_delete_files(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION).unwrap();
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_requires_effective_projection() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
            delete_files: qglake_delete_files(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require effective projection proof");
        assert!(err.to_string().contains("effective projection"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_requires_delete_file_refs() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: qglake_delete_files(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require delete-file references");
        assert!(err.to_string().contains("delete-file references"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_requires_delete_files() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require delete-file entries");
        assert!(err.to_string().contains("delete-file refs"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_accepts_multiple_manifest_children() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
            delete_files: qglake_delete_files(),
            plan_tasks: vec![
                "lakecat:sail-json-hmac:manifest:1".to_string(),
                "lakecat:sail-json-hmac:manifest:2".to_string(),
            ],
            lakecat_plan_tasks: vec![
                serde_json::json!({
                    "task-type": "manifest",
                    "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro",
                    "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/manifest-42-a.avro",
                    "content": "data"
                }),
                serde_json::json!({
                    "task-type": "manifest",
                    "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro",
                    "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/delete-manifest-42.avro",
                    "content": "deletes"
                }),
            ],
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect("QGLake manifest-list fetch should accept multiple child manifests");
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_missing_child_plan_task_token() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: Vec::new(),
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should expose child plan-task tokens");
        assert!(
            err.to_string()
                .contains("child Iceberg REST plan-task token")
        );
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_missing_manifest_child_task() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: vec![serde_json::json!({"task-type": "metadata-only"})],
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should expose manifest child tasks");
        assert!(err.to_string().contains("manifest child task"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_non_sail_planner() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "memory-test-planner".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require Sail planning");
        assert!(err.to_string().contains("not planned by Sail REST models"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_empty_scan_work() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: Vec::new(),
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require scan work");
        assert!(err.to_string().contains("produced no file scan tasks"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_placeholder_scan_work() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({"placeholder": true})],
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require data-file paths");
        assert!(err.to_string().contains("no data-file file paths"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_escaped_data_file_paths() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/other-table/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should reject escaped data files");
        assert!(err.to_string().contains("escaped table location"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_widened_allowed_columns() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": [
                            "event_id",
                            "occurred_at",
                            "severity",
                            "raw_payload"
                        ],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should reject widened columns");
        assert!(err.to_string().contains("allowed columns"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_missing_policy_hash_binding() {
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require a policy hash binding");
        assert!(
            err.to_string()
                .contains("read restriction did not include policy hashes")
        );
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_requires_read_restriction_purpose() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
            delete_files: qglake_delete_files(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require read restriction purpose");
        assert!(err.to_string().contains("purpose"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_requires_read_restriction_ttl_cap() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
            delete_files: qglake_delete_files(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"],
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require read restriction TTL cap");
        assert!(err.to_string().contains("max credential TTL"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_missing_required_projection() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
            delete_files: qglake_delete_files(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-filters": [{
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    }]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require required projection evidence");
        assert!(err.to_string().contains("required projection"));
    }

    #[test]
    fn qglake_fetch_scan_tasks_verifier_rejects_missing_required_filters() {
        let expected_policy_hash = qglake_policy_hash("events").unwrap();
        let fetched = FetchScanTasksResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "sail-rest-models".to_string(),
            plan_task: "lakecat:sail-json-hmac:test".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![qglake_file_scan_task_with_delete_ref()],
            delete_files: qglake_delete_files(),
            plan_tasks: vec!["lakecat:sail-json-hmac:manifest".to_string()],
            lakecat_plan_tasks: qglake_manifest_child_plan_tasks(),
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not-eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "purpose": "qglake-agent-demo",
                        "max-credential-ttl-seconds": 300,
                        "policy-hashes": [expected_policy_hash]
                    },
                    "required-projection": ["event_id", "occurred_at", "severity"],
                    "effective-projection": ["event_id", "occurred_at", "severity"]
                }
            })),
        };

        let err = verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake governed fetch should require row-predicate proof");
        assert!(err.to_string().contains("required filters"));
    }

    #[test]
    fn qglake_credentials_verifier_requires_empty_raw_credentials() {
        verify_qglake_credentials_response(&LoadCredentialsResponse {
            storage_credentials: Vec::new(),
        })
        .expect("QGLake restricted table should accept empty raw credentials");

        let err = verify_qglake_credentials_response(&LoadCredentialsResponse {
            storage_credentials: vec![lakecat_api::StorageCredential {
                prefix: "file:///tmp/lakecat-qglake/events".to_string(),
                config: vec![lakecat_api::ConfigEntry::new(
                    "lakecat.credential-mode",
                    "local-file-no-secret",
                )],
            }],
        })
        .expect_err("QGLake restricted table should reject raw credentials");
        assert!(
            err.to_string()
                .contains("qglake restricted table unexpectedly returned")
        );
    }

    #[test]
    fn qglake_trusted_human_credentials_verifier_requires_standard_local_credentials() {
        verify_qglake_trusted_human_credentials_response(
            &LoadCredentialsResponse {
                storage_credentials: vec![lakecat_api::StorageCredential {
                    prefix: QGLAKE_TEST_LOCATION.to_string(),
                    config: vec![lakecat_api::ConfigEntry::new(
                        "lakecat.credential-mode",
                        "local-file-no-secret",
                    )],
                }],
            },
            QGLAKE_TEST_LOCATION,
        )
        .expect("trusted human path should accept standard non-secret credentials");

        let err = verify_qglake_trusted_human_credentials_response(
            &LoadCredentialsResponse {
                storage_credentials: Vec::new(),
            },
            QGLAKE_TEST_LOCATION,
        )
        .expect_err("trusted human path should require a standard credential set");
        assert!(
            err.to_string()
                .contains("returned no standard credential set")
        );

        let err = verify_qglake_trusted_human_credentials_response(
            &LoadCredentialsResponse {
                storage_credentials: vec![lakecat_api::StorageCredential {
                    prefix: QGLAKE_TEST_LOCATION.to_string(),
                    config: vec![lakecat_api::ConfigEntry::new("aws.session-token", "token")],
                }],
            },
            QGLAKE_TEST_LOCATION,
        )
        .expect_err("trusted human local credentials should not expose secrets");
        assert!(err.to_string().contains("secret material"));
    }

    #[test]
    fn qglake_commit_history_replay_line_summarizes_verified_evidence() {
        let line = qglake_table_commit_history_replay_line(&LineageDrainResponse {
            delivered: 1,
            event_types: vec!["table.commits-listed".to_string()],
            graph_events: 1,
            lineage_events: 1,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![qglake_table_commit_history_lineage_summary()],
        })
        .expect("commit-history replay line should be present");

        assert_eq!(
            line,
            "table commit history commits=1 sequences=1 hashes=sha256:table-commit graph_events=1"
        );
    }

    #[test]
    fn qglake_scan_replay_line_summarizes_verified_evidence() {
        let line = qglake_scan_replay_line(&LineageDrainResponse {
            delivered: 2,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
            ],
            graph_events: 1,
            lineage_events: 2,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_scan_planned_lineage_summary(),
                qglake_scan_tasks_fetched_lineage_summary(),
            ],
        })
        .expect("scan replay line should be present");

        assert_eq!(
            line,
            "scan replay plan_tasks=1 plan_graph_events=1 planned_ttl=300 planned_purpose=qglake-agent-demo file_tasks=1 delete_files=1 child_plan_tasks=1 fetched_ttl=300 fetched_purpose=qglake-agent-demo"
        );
    }

    #[test]
    fn qglake_scan_replay_rejects_missing_stats_field_evidence() {
        let mut planned = qglake_scan_planned_lineage_summary();
        planned.requested_stats_fields = Vec::new();

        let err = verify_qglake_scan_restriction_replay(
            &planned,
            &qglake_scan_tasks_fetched_lineage_summary(),
        )
        .expect_err("scan replay should reject missing stats-field evidence");

        assert!(
            err.to_string()
                .contains("missing requested/effective stats-field evidence")
        );
    }

    #[test]
    fn qglake_scan_replay_rejects_missing_projection_evidence() {
        let mut planned = qglake_scan_planned_lineage_summary();
        planned.requested_projection = Vec::new();

        let err = verify_qglake_scan_restriction_replay(
            &planned,
            &qglake_scan_tasks_fetched_lineage_summary(),
        )
        .expect_err("scan replay should reject missing projection evidence");

        assert!(
            err.to_string()
                .contains("missing requested/effective projection evidence")
        );
    }

    #[test]
    fn qglake_scan_replay_rejects_widened_effective_projection() {
        let mut planned = qglake_scan_planned_lineage_summary();
        planned.effective_projection = vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
            "raw_payload".to_string(),
        ];

        let err = verify_qglake_scan_restriction_replay(
            &planned,
            &qglake_scan_tasks_fetched_lineage_summary(),
        )
        .expect_err("scan replay should reject widened effective projection");

        assert!(
            err.to_string()
                .contains("does not prove projection narrowing")
        );
    }

    #[test]
    fn qglake_scan_replay_rejects_unrequested_effective_projection() {
        let mut planned = qglake_scan_planned_lineage_summary();
        planned.requested_projection = vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
            "raw_payload".to_string(),
        ];
        planned.effective_projection = vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "tenant_id".to_string(),
        ];

        let err = verify_qglake_scan_restriction_replay(
            &planned,
            &qglake_scan_tasks_fetched_lineage_summary(),
        )
        .expect_err(
            "scan replay should reject effective projection fields that were never requested",
        );

        assert!(err.to_string().contains("tenant_id"));
        assert!(err.to_string().contains("was not requested"));
    }

    #[test]
    fn qglake_scan_replay_rejects_missing_fetched_effective_projection() {
        let mut fetched = qglake_scan_tasks_fetched_lineage_summary();
        fetched.effective_projection = Vec::new();

        let err =
            verify_qglake_scan_restriction_replay(&qglake_scan_planned_lineage_summary(), &fetched)
                .expect_err("scan replay should reject missing fetched effective projection");

        assert!(
            err.to_string()
                .contains("fetch replay effective projection does not match")
        );
    }

    #[test]
    fn qglake_scan_replay_rejects_drifted_fetched_effective_projection() {
        let mut fetched = qglake_scan_tasks_fetched_lineage_summary();
        fetched.effective_projection = vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "raw_payload".to_string(),
        ];

        let err =
            verify_qglake_scan_restriction_replay(&qglake_scan_planned_lineage_summary(), &fetched)
                .expect_err("scan replay should reject drifted fetched effective projection");

        assert!(
            err.to_string()
                .contains("fetch replay effective projection does not match")
        );
    }

    #[test]
    fn qglake_scan_replay_rejects_widened_effective_stats_fields() {
        let mut planned = qglake_scan_planned_lineage_summary();
        planned.effective_stats_fields = vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
            "raw_payload".to_string(),
        ];

        let err = verify_qglake_scan_restriction_replay(
            &planned,
            &qglake_scan_tasks_fetched_lineage_summary(),
        )
        .expect_err("scan replay should reject widened effective stats fields");

        assert!(
            err.to_string()
                .contains("does not prove stats-field narrowing")
        );
    }

    #[test]
    fn qglake_scan_replay_rejects_unrequested_effective_stats_fields() {
        let mut planned = qglake_scan_planned_lineage_summary();
        planned.requested_stats_fields = vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
            "raw_payload".to_string(),
        ];
        planned.effective_stats_fields = vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "tenant_id".to_string(),
        ];

        let err = verify_qglake_scan_restriction_replay(
            &planned,
            &qglake_scan_tasks_fetched_lineage_summary(),
        )
        .expect_err("scan replay should reject effective stats fields that were never requested");

        assert!(err.to_string().contains("tenant_id"));
        assert!(err.to_string().contains("was not requested"));
    }

    #[test]
    fn qglake_management_replay_line_summarizes_verified_evidence() {
        let line = qglake_management_replay_line(&LineageDrainResponse {
            delivered: 5,
            event_types: vec![
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "policy-binding.listed".to_string(),
                "storage-profile.listed".to_string(),
            ],
            graph_events: 0,
            lineage_events: 5,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
            ],
        })
        .expect("management replay line should be present");

        assert_eq!(
            line,
            "management replay servers=1 projects=1 warehouses=1 policies=1 storage_profiles=1 storage_profile_upserts=1 credential_root=events-local:file:local-file-no-secret:location_prefix_hash=sha256:storage-location-prefix:secret_ref=none"
        );

        let mut upsert_without_location_hash = qglake_storage_profile_upsert_lineage_summary();
        upsert_without_location_hash.storage_profile_location_prefix_hash = None;
        assert!(
            qglake_management_replay_line(&LineageDrainResponse {
                delivered: 5,
                event_types: vec![
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                ],
                graph_events: 0,
                lineage_events: 5,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    upsert_without_location_hash,
                ],
            })
            .is_none(),
            "management replay line should require storage-profile location hash"
        );

        let mut upsert_with_contradictory_secret_ref =
            qglake_storage_profile_upsert_lineage_summary();
        upsert_with_contradictory_secret_ref.storage_profile_secret_ref_provider =
            Some("vault".to_string());
        assert!(
            qglake_management_replay_line(&LineageDrainResponse {
                delivered: 5,
                event_types: vec![
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                ],
                graph_events: 0,
                lineage_events: 5,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    upsert_with_contradictory_secret_ref,
                ],
            })
            .is_none(),
            "management replay line should reject secret-ref provider without presence"
        );
    }

    #[test]
    fn qglake_credential_replay_line_summarizes_verified_evidence() {
        let line = qglake_credential_replay_line(
            &LineageDrainResponse {
                delivered: 2,
                event_types: vec![
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                ],
                graph_events: 0,
                lineage_events: 2,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                ],
            },
            Some("did:example:agent"),
        )
        .expect("credential replay line should be present");

        assert_eq!(
            line,
            "credential replay restricted=blocked:sail-planned-read-required restricted_count=0 restricted_ttl=300 restricted_profile=events-local:file:local-file-no-secret:location_prefix_hash=sha256:storage-location-prefix:secret_ref=none:graph_events=2 human=allowed:trusted-human-audited-raw human_count=1 human_ttl=300 human_profile=events-local:file:local-file-no-secret:location_prefix_hash=sha256:storage-location-prefix:secret_ref=none:graph_events=2"
        );
    }

    #[test]
    fn qglake_lineage_drain_verifier_requires_delivered_events() {
        let verification = qglake_lineage_verification();
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 0,
                event_types: Vec::new(),
                graph_events: 0,
                lineage_events: 0,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: Vec::new(),
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject zero deliveries");
        assert!(
            err.to_string()
                .contains("qglake lineage drain delivered no outbox events")
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 0,
                lineage_events: 0,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: Vec::new(),
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing lineage emissions");
        assert!(
            err.to_string()
                .contains("qglake lineage drain emitted no lineage events")
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 0,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: Vec::new(),
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing graph emissions");
        assert!(
            err.to_string()
                .contains("qglake lineage drain emitted no graph events")
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["table.scan-planned".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: Vec::new(),
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require bootstrap replay");
        assert!(
            err.to_string()
                .contains("qglake lineage drain did not replay querygraph.bootstrap")
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: None,
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: Vec::new(),
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require read authorization proof");
        assert!(
            err.to_string().contains(
                "qglake lineage drain read is missing SHA-256 authorization receipt hash"
            )
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: None,
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: Vec::new(),
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require read request identity proof");
        assert!(
            err.to_string().contains(
                "qglake lineage drain read is missing request identity attestation state"
            )
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: Vec::new(),
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require bootstrap evidence");
        assert!(
            err.to_string().contains(
                "qglake lineage drain did not expose querygraph.bootstrap replay evidence"
            )
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: Vec::new(),
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require sink receipt evidence");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence is missing SHA-256 sink receipt hashes"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: None,
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require QueryGraph import replay hash");
        assert!(
            err.to_string().contains(
                "qglake lineage drain replay evidence is missing SHA-256 QueryGraph hashes"
            )
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:other-bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject mismatched replay hashes");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence does not match the accepted QueryGraph bundle"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:other".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject mismatched replay principal");
        assert!(err.to_string().contains(
            "qglake lineage drain replay principal did not match accepted principal did:example:agent"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("human".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject non-agent replay principals");
        assert!(err.to_string().contains(
            "qglake lineage drain replay principal kind did not match accepted principal kind agent"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: None,
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing authorization receipt proof");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence is missing SHA-256 authorization receipt hash"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: None,
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing request identity state");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence is missing request identity attestation state"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: None,
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing agent delegation proof");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence is missing SHA-256 agent delegation hash"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: None,
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing agent summary proof");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence is missing SHA-256 agent summary signature hash"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 2,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject mismatched artifact counts");
        assert!(err.to_string().contains(
            "qglake lineage drain replay artifact counts do not match the accepted QueryGraph bundle"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: vec!["OpenLineage".to_string()],
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject mismatched replay standards");
        assert!(err.to_string().contains(
            "qglake lineage drain replay standards do not match the accepted QueryGraph bundle"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 1,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 0,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject mismatched policy binding counts");
        assert!(err.to_string().contains(
            "qglake lineage drain replay policy binding count does not match the accepted QueryGraph bundle"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some("sha256:delegation".to_string()),
                    agent_summary_signature_hash: Some("sha256:summary".to_string()),
                    graph_events: 0,
                    lineage_events: 0,
                    bundle_hash: Some("sha256:bundle".to_string()),
                    graph_hash: Some("sha256:graph".to_string()),
                    open_lineage_hash: Some("sha256:openlineage".to_string()),
                    querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    project_count: None,
                    server_count: None,
                    storage_profile_count: None,
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing bootstrap lineage projection");
        assert!(
            err.to_string()
                .contains("qglake lineage drain bootstrap replay emitted no lineage projection")
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("not-a-sha256-hash".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![qglake_bootstrap_lineage_summary()],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject malformed read authorization hash");
        assert!(
            err.to_string().contains(
                "qglake lineage drain read is missing SHA-256 authorization receipt hash"
            )
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: Some("sha256:typedid-proof".to_string()),
                events: vec![qglake_bootstrap_lineage_summary()],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject request TypeDID proof without envelope");
        assert!(
            err.to_string()
                .contains("qglake lineage drain read TypeDID proof hash requires an envelope hash")
        );

        let mut bootstrap_malformed_agent_hash = qglake_bootstrap_lineage_summary();
        bootstrap_malformed_agent_hash.agent_delegation_hash =
            Some("not-a-sha256-hash".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![bootstrap_malformed_agent_hash],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject malformed bootstrap agent delegation hash");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence is missing SHA-256 agent delegation hash"
        ));

        let mut bootstrap_typedid_without_envelope = qglake_bootstrap_lineage_summary();
        bootstrap_typedid_without_envelope.typedid_proof_hash =
            Some("sha256:typedid-proof".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![bootstrap_typedid_without_envelope],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject bootstrap TypeDID proof without envelope");
        assert!(err.to_string().contains(
            "qglake lineage drain bootstrap replay TypeDID proof hash requires an envelope hash"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 3,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 3,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_human_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require restricted credential replay");
        let err = err.to_string();
        assert!(
            err.contains("qglake lineage drain did not replay the restricted credential probe"),
            "{err}"
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 3,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 3,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require trusted human credential replay");
        let err = err.to_string();
        assert!(
            err.contains("qglake lineage drain did not replay the trusted human credential probe"),
            "{err}"
        );

        let mut restricted_without_receipts = qglake_restricted_credential_summary();
        restricted_without_receipts.lineage_events = 0;
        restricted_without_receipts.replay_event_hashes.clear();
        restricted_without_receipts
            .replay_open_lineage_hashes
            .clear();
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 3,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    restricted_without_receipts,
                    qglake_human_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing restricted credential receipts");
        assert!(err.to_string().contains(
            "qglake lineage drain restricted credential replay emitted no lineage projection"
        ));

        let mut human_without_openlineage = qglake_human_credential_summary();
        human_without_openlineage.replay_open_lineage_hashes.clear();
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    human_without_openlineage,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing trusted human credential receipts");
        assert!(err.to_string().contains(
            "qglake lineage drain trusted human credential replay is missing SHA-256 sink receipt hashes"
        ));

        let mut human_without_exception_reason = qglake_human_credential_summary();
        human_without_exception_reason.raw_credential_exception_reason = None;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    human_without_exception_reason,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing trusted human exception reason");
        assert!(err.to_string().contains(
            "qglake lineage drain trusted human credential replay did not prove audited standard credential vending"
        ));

        let mut restricted_with_exception_reason = qglake_restricted_credential_summary();
        restricted_with_exception_reason.raw_credential_exception_reason =
            Some("trusted-human-override".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    restricted_with_exception_reason,
                    qglake_human_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject restricted raw exception reasons");
        assert!(err.to_string().contains(
            "qglake lineage drain restricted credential replay did not prove raw credentials were blocked"
        ));

        let mut human_without_ttl = qglake_human_credential_summary();
        human_without_ttl.read_restriction = None;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    human_without_ttl,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing credential TTL evidence");
        assert!(
            err.to_string()
                .contains("trusted human credential replay is missing max credential TTL evidence")
        );

        let mut human_with_drifted_ttl = qglake_human_credential_summary();
        human_with_drifted_ttl.read_restriction = Some(json!({
            "allowed-columns": ["event_id", "occurred_at", "severity"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 60,
            "policy-hashes": ["sha256:scan-policy"]
        }));
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    human_with_drifted_ttl,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject credential TTL drift");
        assert!(
            err.to_string()
                .contains("trusted human credential replay TTL cap mismatch")
        );

        let mut human_without_purpose = qglake_human_credential_summary();
        human_without_purpose.read_restriction = Some(json!({
            "allowed-columns": ["event_id", "occurred_at", "severity"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "max-credential-ttl-seconds": 300,
            "policy-hashes": ["sha256:scan-policy"]
        }));
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    human_without_purpose,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing credential restriction purpose");
        assert!(err.to_string().contains(
            "qglake lineage drain trusted human credential read restriction is missing required field purpose"
        ));

        let mut human_with_drifted_purpose = qglake_human_credential_summary();
        human_with_drifted_purpose.read_restriction = Some(json!({
            "allowed-columns": ["event_id", "occurred_at", "severity"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "other-purpose",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": ["sha256:scan-policy"]
        }));
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    human_with_drifted_purpose,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject credential restriction purpose drift");
        assert!(err.to_string().contains(
            "qglake lineage drain restricted credential read restriction.purpose mismatch"
        ));

        let mut restricted_without_location_hash = qglake_restricted_credential_summary();
        restricted_without_location_hash.storage_profile_location_prefix_hash = None;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    restricted_without_location_hash,
                    qglake_human_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing credential storage-scope hash");
        assert!(err.to_string().contains(
            "qglake lineage drain restricted credential replay is missing redacted storage-profile graph evidence"
        ));

        let mut restricted_missing_secret_ref_provider = qglake_restricted_credential_summary();
        restricted_missing_secret_ref_provider.storage_profile_secret_ref_present = Some(true);
        restricted_missing_secret_ref_provider.storage_profile_secret_ref_provider = None;
        restricted_missing_secret_ref_provider.storage_profile_secret_ref_hash =
            Some("sha256:credential-secret-ref".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    restricted_missing_secret_ref_provider,
                    qglake_human_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err(
            "QGLake lineage drain should reject credential secret-ref presence without provider",
        );
        assert!(err.to_string().contains(
            "qglake lineage drain restricted credential replay is missing secret-ref provider evidence"
        ));

        let mut restricted_missing_secret_ref_hash = qglake_restricted_credential_summary();
        restricted_missing_secret_ref_hash.storage_profile_secret_ref_present = Some(true);
        restricted_missing_secret_ref_hash.storage_profile_secret_ref_provider =
            Some("vault".to_string());
        restricted_missing_secret_ref_hash.storage_profile_secret_ref_hash = None;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    restricted_missing_secret_ref_hash,
                    qglake_human_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err(
            "QGLake lineage drain should reject credential secret-ref presence without hash",
        );
        assert!(err.to_string().contains(
            "qglake lineage drain restricted credential replay is missing secret-ref hash evidence"
        ));

        let mut restricted_hash_without_secret_ref = qglake_restricted_credential_summary();
        restricted_hash_without_secret_ref.storage_profile_secret_ref_hash =
            Some("sha256:credential-secret-ref".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    restricted_hash_without_secret_ref,
                    qglake_human_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err(
            "QGLake lineage drain should reject credential secret-ref hash without presence",
        );
        assert!(err.to_string().contains(
            "qglake lineage drain restricted credential replay carried a secret-ref hash without secret-ref presence"
        ));

        let view_verification = qglake_view_lineage_verification();
        let mut bootstrap_with_view = qglake_bootstrap_lineage_summary();
        bootstrap_with_view.view_artifact_count = 1;
        bootstrap_with_view.view_version_receipt_hashes =
            vec!["sha256:view-version-receipt".to_string()];
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require accepted view replay");
        assert!(err.to_string().contains(
            "qglake lineage drain did not replay view evidence for lakecat:view:local:default:active_customers"
        ));

        let mut bootstrap_missing_view_receipt = bootstrap_with_view.clone();
        bootstrap_missing_view_receipt
            .view_version_receipt_hashes
            .clear();
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 9,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 3,
                lineage_events: 10,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_missing_view_receipt,
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require view version receipt hashes");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence is missing SHA-256 view version receipt hashes"
        ));

        let mut bootstrap_malformed_view_receipt = bootstrap_with_view.clone();
        bootstrap_malformed_view_receipt.view_version_receipt_hashes =
            vec!["not-a-sha256-hash".to_string()];
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 9,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 3,
                lineage_events: 10,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_malformed_view_receipt,
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject malformed view version receipt hashes");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence is missing SHA-256 view version receipt hashes"
        ));

        let mut bootstrap_drifted_view_receipt = bootstrap_with_view.clone();
        bootstrap_drifted_view_receipt.view_version_receipt_hashes =
            vec!["sha256:other-view-version-receipt".to_string()];
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 9,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 3,
                lineage_events: 10,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_drifted_view_receipt,
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject view receipt hash drift");
        assert!(err.to_string().contains(
            "qglake lineage drain replay evidence view version receipt hashes do not match the accepted QueryGraph bundle"
        ));

        let mut mismatched_view_replay = qglake_view_lineage_summary();
        mismatched_view_replay.view_version = Some(3);
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 9,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 3,
                lineage_events: 10,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    mismatched_view_replay,
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject mismatched view replay version");
        assert!(err.to_string().contains(
            "qglake lineage drain view replay for lakecat:view:local:default:active_customers did not preserve accepted view version 2"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 4,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 4,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require policy list replay");
        assert!(
            err.to_string()
                .contains("qglake lineage drain did not replay policy list evidence")
        );

        let mut policy_list_mismatch = qglake_policy_list_lineage_summary();
        policy_list_mismatch.policy_binding_count = 0;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 5,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 5,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    policy_list_mismatch,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject mismatched policy list replay");
        assert!(err.to_string().contains(
            "qglake lineage drain policy list replay count does not match the accepted QueryGraph bundle"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 5,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 5,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require storage profile list replay");
        assert!(
            err.to_string()
                .contains("qglake lineage drain did not replay storage profile list evidence")
        );

        let mut empty_storage_profile_list = qglake_storage_profile_list_lineage_summary();
        empty_storage_profile_list.storage_profile_count = Some(0);
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 6,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 6,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                    empty_storage_profile_list,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject empty storage profile list replay");
        assert!(err.to_string().contains(
            "qglake lineage drain storage profile list replay did not expose any storage profiles"
        ));

        let mut malformed_storage_profile_upsert = qglake_storage_profile_upsert_lineage_summary();
        malformed_storage_profile_upsert.storage_profile_location_prefix_hash =
            Some("not-a-hash".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 7,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "storage-profile.upserted".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 7,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    malformed_storage_profile_upsert,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject malformed storage-profile upsert hash");
        assert!(err.to_string().contains(
            "qglake lineage drain storage profile upsert replay did not expose redacted credential-root evidence"
        ));

        let mut contradictory_storage_profile_upsert =
            qglake_storage_profile_upsert_lineage_summary();
        contradictory_storage_profile_upsert.storage_profile_secret_ref_provider =
            Some("vault".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 7,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "storage-profile.upserted".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 7,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    contradictory_storage_profile_upsert,
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject contradictory secret-ref evidence");
        assert!(err.to_string().contains(
            "qglake lineage drain storage profile upsert replay carried secret-ref evidence without secret-ref presence"
        ));

        let mut restricted_profile_drift = qglake_restricted_credential_summary();
        restricted_profile_drift.storage_profile_id = Some("other-profile".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 7,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "storage-profile.upserted".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 7,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    restricted_profile_drift,
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject credential storage-profile drift");
        let err = err.to_string();
        assert!(
            err.contains(
                "qglake lineage drain restricted credential replay storage-profile evidence does not match storage profile upsert replay"
            ),
            "{err}"
        );

        let mut upsert_secret_ref_drift = qglake_storage_profile_upsert_lineage_summary();
        upsert_secret_ref_drift.storage_profile_secret_ref_present = Some(true);
        upsert_secret_ref_drift.storage_profile_secret_ref_provider = Some("vault".to_string());
        upsert_secret_ref_drift.storage_profile_secret_ref_hash =
            Some("sha256:storage-secret-ref".to_string());
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 7,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "storage-profile.upserted".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 7,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    upsert_secret_ref_drift,
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject credential secret-ref state drift");
        assert!(err.to_string().contains(
            "qglake lineage drain restricted credential replay storage-profile evidence does not match storage profile upsert replay"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 6,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 6,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require server list replay");
        assert!(
            err.to_string()
                .contains("qglake lineage drain did not replay server list evidence")
        );

        let mut server_list_without_graph = qglake_server_list_lineage_summary();
        server_list_without_graph.graph_events = 0;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 10,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "storage-profile.upserted".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 10,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    server_list_without_graph,
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require management graph projection");
        assert!(err.to_string().contains(
            "qglake lineage drain server list replay emitted no catalog graph projection"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 10,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 11,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require tombstone receipt evidence for dropped accepted views");
        assert!(err.to_string().contains(
            "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers is missing SHA-256 tombstone receipt evidence"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 11,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 12,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require namespace receipt-chain evidence for dropped accepted views");
        assert!(err.to_string().contains(
            "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers is missing SHA-256 namespace receipt-chain evidence"
        ));

        let mut drifted_receipt_chain = qglake_view_receipt_chain_lineage_summary();
        drifted_receipt_chain.view_namespace = vec!["other_namespace".to_string()];
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 12,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 13,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    drifted_receipt_chain,
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject receipt-chain namespace drift");
        assert!(err.to_string().contains(
            "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers is missing SHA-256 namespace receipt-chain evidence for the accepted view namespace"
        ));

        let mut receipt_chain_count_drift = qglake_view_receipt_chain_lineage_summary();
        receipt_chain_count_drift.view_version_receipt_chain_verified_count = 2;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 12,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 13,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    receipt_chain_count_drift,
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject receipt-chain count drift");
        assert!(err.to_string().contains(
            "qglake lineage drain namespace receipt-chain replay for lakecat:view:local:default:active_customers verified-chain count does not match chain hash evidence"
        ));

        let mut uncovered_tombstone_chain = qglake_view_receipt_chain_lineage_summary();
        uncovered_tombstone_chain.view_version_receipt_hashes =
            vec!["sha256:other-view-receipt".to_string()];
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 12,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 13,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    uncovered_tombstone_chain,
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err(
            "QGLake lineage drain should reject tombstone receipts outside the namespace chain",
        );
        assert!(err.to_string().contains(
            "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers tombstone receipt hashes are not covered by namespace receipt-chain evidence"
        ));

        let mut unguarded_drop_replay = qglake_view_drop_lineage_summary();
        unguarded_drop_replay.expected_view_version = None;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 16,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 16,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    unguarded_drop_replay,
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_table_commit_history_lineage_summary(),
                    qglake_scan_planned_lineage_summary(),
                    qglake_scan_tasks_fetched_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require guarded drop replay for accepted views");
        assert!(err.to_string().contains(
            "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers did not preserve expected view version 2"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 12,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 14,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require table commit history replay");
        assert!(
            err.to_string()
                .contains("qglake lineage drain did not replay table commit history evidence")
        );

        let mut commit_history_without_summary = qglake_table_commit_history_lineage_summary();
        commit_history_without_summary.table_commit_count = None;
        commit_history_without_summary
            .table_commit_sequence_numbers
            .clear();
        commit_history_without_summary.table_commit_hashes.clear();
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 13,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 14,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    commit_history_without_summary,
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require compact table commit history summary");
        assert!(err.to_string().contains(
            "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
        ));

        let mut commit_history_with_malformed_hash = qglake_table_commit_history_lineage_summary();
        commit_history_with_malformed_hash.table_commit_hashes =
            vec!["not-a-sha256-hash".to_string()];
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 13,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 14,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    commit_history_with_malformed_hash,
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject malformed table commit hashes");
        assert!(err.to_string().contains(
            "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
        ));

        let mut commit_history_with_count_drift = qglake_table_commit_history_lineage_summary();
        commit_history_with_count_drift.table_commit_count = Some(2);
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 13,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 14,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    commit_history_with_count_drift,
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject table commit count drift");
        assert!(err.to_string().contains(
            "qglake lineage drain table commit history replay count does not match sequence-number and commit-hash evidence"
        ));

        let mut commit_history_with_duplicate_sequence =
            qglake_table_commit_history_lineage_summary();
        commit_history_with_duplicate_sequence.table_commit_count = Some(2);
        commit_history_with_duplicate_sequence.table_commit_sequence_numbers = vec![1, 1];
        commit_history_with_duplicate_sequence.table_commit_hashes = vec![
            "sha256:table-commit-one".to_string(),
            "sha256:table-commit-two".to_string(),
        ];
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 13,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 14,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    commit_history_with_duplicate_sequence,
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject duplicate table commit sequences");
        assert!(err.to_string().contains(
            "qglake lineage drain table commit history replay sequence numbers must be positive and strictly increasing"
        ));

        let mut commit_history_without_graph = qglake_table_commit_history_lineage_summary();
        commit_history_without_graph.graph_events = 0;
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 14,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 13,
                lineage_events: 15,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    commit_history_without_graph,
                    qglake_scan_planned_lineage_summary(),
                    qglake_scan_tasks_fetched_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require table commit graph projection");
        assert!(err.to_string().contains(
            "qglake lineage drain table commit history replay emitted no graph projection"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 14,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 15,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_table_commit_history_lineage_summary(),
                    qglake_scan_planned_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require fetched scan task replay");
        assert!(
            err.to_string()
                .contains("qglake lineage drain did not replay scan task fetch evidence")
        );

        let mut fetched_with_restriction_drift = qglake_scan_tasks_fetched_lineage_summary();
        fetched_with_restriction_drift.read_restriction = Some(json!({
            "allowed-columns": ["event_id", "occurred_at"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": ["sha256:scan-policy"]
        }));
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 15,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 16,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_table_commit_history_lineage_summary(),
                    qglake_scan_planned_lineage_summary(),
                    fetched_with_restriction_drift,
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject scan restriction drift");
        assert!(
            err.to_string()
                .contains("qglake lineage drain scan planning read restriction")
        );
        assert!(err.to_string().contains("allowed-columns"));

        let mut fetched_with_filter_drift = qglake_scan_tasks_fetched_lineage_summary();
        fetched_with_filter_drift.required_filters = vec![json!({
            "type": "not-eq",
            "term": "severity",
            "value": "info"
        })];
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 15,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 16,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_table_commit_history_lineage_summary(),
                    qglake_scan_planned_lineage_summary(),
                    fetched_with_filter_drift,
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject fetched filter drift");
        assert!(err.to_string().contains(
            "qglake lineage drain scan task fetch replay required filters do not exactly preserve fetched row predicate"
        ));

        verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 15,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "view.dropped".to_string(),
                    "view.version-receipts-listed".to_string(),
                    "view.version-receipt-chains-listed".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 4,
                lineage_events: 16,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view.clone(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_view_drop_lineage_summary(),
                    qglake_view_tombstone_receipt_lineage_summary(),
                    qglake_view_receipt_chain_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_table_commit_history_lineage_summary(),
                    qglake_scan_planned_lineage_summary(),
                    qglake_scan_tasks_fetched_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect("QGLake lineage drain should accept dropped view evidence with namespace receipt-chain evidence");

        verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 12,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "view.upserted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 3,
                lineage_events: 13,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    bootstrap_with_view,
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_view_lineage_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_table_commit_history_lineage_summary(),
                    qglake_scan_planned_lineage_summary(),
                    qglake_scan_tasks_fetched_lineage_summary(),
                ],
            },
            &view_verification,
            Some("did:example:agent"),
            1,
        )
        .expect("QGLake lineage drain should accept replayed view evidence");

        verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 11,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "table.scan-tasks-fetched".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "credentials.vend-attempted".to_string(),
                    "policy-binding.listed".to_string(),
                    "storage-profile.listed".to_string(),
                    "server.listed".to_string(),
                    "project.listed".to_string(),
                    "warehouse.listed".to_string(),
                    "table.commits-listed".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 12,
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some("sha256:lineage-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                events: vec![
                    qglake_bootstrap_lineage_summary(),
                    qglake_restricted_credential_summary(),
                    qglake_human_credential_summary(),
                    qglake_policy_list_lineage_summary(),
                    qglake_storage_profile_list_lineage_summary(),
                    qglake_storage_profile_upsert_lineage_summary(),
                    qglake_server_list_lineage_summary(),
                    qglake_project_list_lineage_summary(),
                    qglake_warehouse_list_lineage_summary(),
                    qglake_table_commit_history_lineage_summary(),
                    qglake_scan_planned_lineage_summary(),
                    qglake_scan_tasks_fetched_lineage_summary(),
                ],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect("QGLake lineage drain should accept delivered outbox events");
    }

    #[test]
    fn qglake_lineage_drain_verifier_requires_management_receipt_hash_shape() {
        let verification = qglake_handoff_lineage_verification();
        let mut drain = qglake_handoff_lineage_drain();
        let policy_list = drain
            .events
            .iter_mut()
            .find(|event| event.event_type == "policy-binding.listed")
            .expect("policy list replay fixture");
        policy_list.replay_event_hashes = vec!["not-a-sha256-hash".to_string()];

        let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
            .expect_err("QGLake lineage drain should reject malformed management receipt hashes");

        assert!(err.to_string().contains("policy list replay"));
        assert!(err.to_string().contains("SHA-256 receipt hashes"));
    }

    #[test]
    fn qglake_lineage_drain_verifier_requires_scan_receipt_hash_shape() {
        let verification = qglake_handoff_lineage_verification();
        let mut drain = qglake_handoff_lineage_drain();
        let scan_plan = drain
            .events
            .iter_mut()
            .find(|event| event.event_type == "table.scan-planned")
            .expect("scan plan replay fixture");
        scan_plan.replay_open_lineage_hashes = vec!["not-a-sha256-hash".to_string()];

        let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
            .expect_err("QGLake lineage drain should reject malformed scan receipt hashes");

        assert!(err
            .to_string()
            .contains("qglake lineage drain scan planning replay is missing compact task, graph, or SHA-256 receipt evidence"));
    }

    #[test]
    fn qglake_lineage_drain_verifier_requires_scan_plan_graph_events() {
        let verification = qglake_handoff_lineage_verification();
        let mut drain = qglake_handoff_lineage_drain();
        let scan_plan = drain
            .events
            .iter_mut()
            .find(|event| event.event_type == "table.scan-planned")
            .expect("scan plan replay fixture");
        scan_plan.graph_events = 0;

        let err = verify_qglake_lineage_drain(&drain, &verification, Some("did:example:agent"), 1)
            .expect_err("QGLake lineage drain should reject missing scan-plan graph proof");

        assert!(err
            .to_string()
            .contains("qglake lineage drain scan planning replay is missing compact task, graph, or SHA-256 receipt evidence"));
    }

    fn qglake_view_lineage_verification() -> QueryGraphBootstrapVerification {
        let mut verification = qglake_lineage_verification();
        verification.view_count = 1;
        verification.verified_views =
            vec!["lakecat:view:local:default:active_customers".to_string()];
        verification.verified_view_versions =
            BTreeMap::from([("lakecat:view:local:default:active_customers".to_string(), 2)]);
        verification.verified_view_receipt_hashes = BTreeMap::from([(
            "lakecat:view:local:default:active_customers".to_string(),
            "sha256:view-version-receipt".to_string(),
        )]);
        verification.verified_view_receipt_chain_hashes = BTreeMap::from([(
            "lakecat:view:local:default:active_customers".to_string(),
            "sha256:view-receipt-chain".to_string(),
        )]);
        verification
    }

    fn qglake_lineage_verification() -> QueryGraphBootstrapVerification {
        QueryGraphBootstrapVerification {
            warehouse: "local".to_string(),
            table_count: 1,
            view_count: 0,
            verified_tables: vec!["local.default.events".to_string()],
            verified_views: Vec::new(),
            verified_view_versions: BTreeMap::new(),
            verified_view_receipt_hashes: BTreeMap::new(),
            verified_view_receipt_chain_hashes: BTreeMap::new(),
            bundle_hash: "sha256:bundle".to_string(),
            graph_hash: "sha256:graph".to_string(),
            open_lineage_hash: "sha256:openlineage".to_string(),
            querygraph_import_hash: "sha256:querygraph-import".to_string(),
            standards: vec![
                "Iceberg REST".to_string(),
                "Croissant".to_string(),
                "CDIF".to_string(),
                "OSI handoff".to_string(),
                "ODRL".to_string(),
                "Grust catalog graph".to_string(),
                "OpenLineage".to_string(),
            ],
        }
    }

    fn qglake_table_commit_record_summary() -> lakecat_api::TableCommitRecordResponse {
        lakecat_api::TableCommitRecordResponse {
            warehouse: "local".to_string(),
            namespace: vec!["default".to_string()],
            table: "events".to_string(),
            previous_metadata_location: Some(
                "file:///tmp/lakecat-qglake/events/metadata/00000.json".to_string(),
            ),
            new_metadata_location: Some(
                "file:///tmp/lakecat-qglake/events/metadata/00000.json".to_string(),
            ),
            sequence_number: 1,
            format_version: Some(3),
            snapshot_id: Some(42),
            policy_hash: None,
            request_hash: "sha256:request".to_string(),
            response_hash: "sha256:response".to_string(),
            idempotency_key_sha256: Some("sha256:idempotency".to_string()),
            commit_hash: "sha256:commit".to_string(),
            principal_subject: "did:example:agent".to_string(),
            principal_kind: "agent".to_string(),
            committed_at: "2026-06-19T00:00:00Z".to_string(),
        }
    }

    fn qglake_bootstrap_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-bootstrap".to_string(),
            event_type: "querygraph.bootstrap".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 0,
            lineage_events: 1,
            bundle_hash: Some("sha256:bundle".to_string()),
            graph_hash: Some("sha256:graph".to_string()),
            open_lineage_hash: Some("sha256:openlineage".to_string()),
            querygraph_import_hash: Some("sha256:querygraph-import".to_string()),
            table_artifact_count: 1,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 1,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: None,
            standards: qglake_lineage_standards(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:replay-event".to_string()],
            replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
        }
    }

    fn qglake_bootstrap_lineage_summary_for(
        verification: &QueryGraphBootstrapVerification,
        policy_binding_count: usize,
    ) -> LineageDrainEventSummary {
        let mut summary = qglake_bootstrap_lineage_summary();
        summary.bundle_hash = Some(verification.bundle_hash.clone());
        summary.graph_hash = Some(verification.graph_hash.clone());
        summary.open_lineage_hash = Some(verification.open_lineage_hash.clone());
        summary.querygraph_import_hash = Some(verification.querygraph_import_hash.clone());
        summary.table_artifact_count = verification.table_count;
        summary.view_artifact_count = verification.view_count;
        summary.view_version_receipt_hashes = verification
            .verified_view_receipt_hashes
            .values()
            .cloned()
            .collect();
        summary.policy_binding_count = policy_binding_count;
        summary.standards = verification.standards.clone();
        summary
    }

    fn qglake_view_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-view".to_string(),
            event_type: "view.upserted".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:view-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 2,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: Some("local".to_string()),
            view_namespace: vec!["default".to_string()],
            view_name: Some("active_customers".to_string()),
            view_stable_id: Some("lakecat:view:local:default:active_customers".to_string()),
            view_version: Some(2),
            expected_view_version: Some(1),
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: None,
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:view-replay-event".to_string()],
            replay_open_lineage_hashes: vec!["sha256:view-replay-openlineage".to_string()],
        }
    }

    fn qglake_view_drop_lineage_summary() -> LineageDrainEventSummary {
        let mut summary = qglake_view_lineage_summary();
        summary.event_id = "evt-view-drop".to_string();
        summary.event_type = "view.dropped".to_string();
        summary.expected_view_version = Some(2);
        summary.replay_event_hashes = vec!["sha256:view-drop-replay-event".to_string()];
        summary.replay_open_lineage_hashes =
            vec!["sha256:view-drop-replay-openlineage".to_string()];
        summary
    }

    fn qglake_view_tombstone_receipt_lineage_summary() -> LineageDrainEventSummary {
        let mut summary = qglake_view_lineage_summary();
        summary.event_id = "evt-view-receipts".to_string();
        summary.event_type = "view.version-receipts-listed".to_string();
        summary.graph_events = 0;
        summary.lineage_events = 1;
        summary.expected_view_version = None;
        summary.view_version_receipt_hashes = vec!["sha256:view-drop-receipt".to_string()];
        summary.replay_event_hashes = vec!["sha256:view-receipts-replay-event".to_string()];
        summary.replay_open_lineage_hashes =
            vec!["sha256:view-receipts-replay-openlineage".to_string()];
        summary
    }

    fn qglake_view_receipt_chain_lineage_summary() -> LineageDrainEventSummary {
        let mut summary = qglake_view_lineage_summary();
        summary.event_id = "evt-view-receipt-chains".to_string();
        summary.event_type = "view.version-receipt-chains-listed".to_string();
        summary.graph_events = 0;
        summary.lineage_events = 1;
        summary.view_stable_id = None;
        summary.view_warehouse = Some("local".to_string());
        summary.view_namespace = vec!["default".to_string()];
        summary.view_name = None;
        summary.view_version = None;
        summary.expected_view_version = None;
        summary.view_version_receipt_hashes = vec!["sha256:view-drop-receipt".to_string()];
        summary.view_version_receipt_chain_hashes = vec!["sha256:view-receipt-chain".to_string()];
        summary.view_version_receipt_chain_verified_count = 1;
        summary.replay_event_hashes = vec!["sha256:view-receipt-chains-replay-event".to_string()];
        summary.replay_open_lineage_hashes =
            vec!["sha256:view-receipt-chains-replay-openlineage".to_string()];
        summary
    }

    fn qglake_table_commit_history_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-table-commits".to_string(),
            event_type: "table.commits-listed".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:table-commits-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 1,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: Some(1),
            table_commit_sequence_numbers: vec![1],
            table_commit_hashes: vec!["sha256:table-commit".to_string()],
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: Some("local".to_string()),
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:table-commits-replay-event".to_string()],
            replay_open_lineage_hashes: vec!["sha256:table-commits-openlineage".to_string()],
        }
    }

    fn qglake_read_restriction_summary() -> Value {
        json!({
            "allowed-columns": ["event_id", "occurred_at", "severity"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300,
            "policy-hashes": ["sha256:scan-policy"]
        })
    }

    fn qglake_scan_planned_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-scan-planned".to_string(),
            event_type: "table.scan-planned".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:scan-planned-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 1,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: Some(1),
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: Some(qglake_read_restriction_summary()),
            required_projection: Vec::new(),
            requested_projection: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
                "raw_payload".to_string(),
            ],
            effective_projection: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
            ],
            required_filters: Vec::new(),
            requested_stats_fields: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
                "raw_payload".to_string(),
            ],
            effective_stats_fields: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
            ],
            management_scope_project_id: None,
            management_scope_warehouse: Some("local".to_string()),
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:scan-planned-replay".to_string()],
            replay_open_lineage_hashes: vec!["sha256:scan-planned-openlineage".to_string()],
        }
    }

    fn qglake_scan_tasks_fetched_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-scan-tasks-fetched".to_string(),
            event_type: "table.scan-tasks-fetched".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:scan-fetch-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 0,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: Some(1),
            delete_file_count: Some(1),
            child_plan_task_count: Some(1),
            read_restriction: Some(qglake_read_restriction_summary()),
            required_projection: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
            ],
            requested_projection: Vec::new(),
            effective_projection: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
            ],
            required_filters: vec![json!({
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            })],
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: Some("local".to_string()),
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:scan-fetch-replay".to_string()],
            replay_open_lineage_hashes: vec!["sha256:scan-fetch-openlineage".to_string()],
        }
    }

    fn qglake_policy_list_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-policy-list".to_string(),
            event_type: "policy-binding.listed".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:policy-list-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 1,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 1,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: Some("local".to_string()),
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:policy-list-replay-event".to_string()],
            replay_open_lineage_hashes: vec!["sha256:policy-list-openlineage".to_string()],
        }
    }

    fn qglake_storage_profile_list_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-storage-profile-list".to_string(),
            event_type: "storage-profile.listed".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(
                "sha256:storage-profile-list-authorization".to_string(),
            ),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 1,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: Some(1),
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: Some("local".to_string()),
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:storage-profile-list-replay-event".to_string()],
            replay_open_lineage_hashes: vec!["sha256:storage-profile-list-openlineage".to_string()],
        }
    }

    fn qglake_storage_profile_upsert_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-storage-profile-upsert".to_string(),
            event_type: "storage-profile.upserted".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(
                "sha256:storage-profile-upsert-authorization".to_string(),
            ),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 1,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: Some("events-local".to_string()),
            storage_profile_provider: Some("file".to_string()),
            storage_profile_issuance_mode: Some("local-file-no-secret".to_string()),
            storage_profile_location_prefix_hash: Some(
                "sha256:storage-location-prefix".to_string(),
            ),
            storage_profile_secret_ref_present: Some(false),
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: Some("local".to_string()),
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:storage-profile-upsert-replay-event".to_string()],
            replay_open_lineage_hashes: vec![
                "sha256:storage-profile-upsert-openlineage".to_string(),
            ],
        }
    }

    fn qglake_server_list_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-server-list".to_string(),
            event_type: "server.listed".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:server-list-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 1,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: Some(1),
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: None,
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:server-list-replay-event".to_string()],
            replay_open_lineage_hashes: vec!["sha256:server-list-openlineage".to_string()],
        }
    }

    fn qglake_project_list_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-project-list".to_string(),
            event_type: "project.listed".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:project-list-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 1,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: Some(1),
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: None,
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:project-list-replay-event".to_string()],
            replay_open_lineage_hashes: vec!["sha256:project-list-openlineage".to_string()],
        }
    }

    fn qglake_warehouse_list_lineage_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-warehouse-list".to_string(),
            event_type: "warehouse.listed".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:warehouse-list-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 1,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: None,
            storage_profile_provider: None,
            storage_profile_issuance_mode: None,
            storage_profile_location_prefix_hash: None,
            storage_profile_secret_ref_present: None,
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: Some(1),
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: None,
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: None,
            standards: Vec::new(),
            credential_count: None,
            credential_block_reason: None,
            raw_credential_exception_allowed: None,
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:warehouse-list-replay-event".to_string()],
            replay_open_lineage_hashes: vec!["sha256:warehouse-list-openlineage".to_string()],
        }
    }

    fn qglake_restricted_credential_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-agent-credentials".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("sha256:agent-credential-authorization".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: Some("sha256:delegation".to_string()),
            agent_summary_signature_hash: Some("sha256:summary".to_string()),
            graph_events: 2,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: Some("events-local".to_string()),
            storage_profile_provider: Some("file".to_string()),
            storage_profile_issuance_mode: Some("local-file-no-secret".to_string()),
            storage_profile_location_prefix_hash: Some(
                "sha256:storage-location-prefix".to_string(),
            ),
            storage_profile_secret_ref_present: Some(false),
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: Some(qglake_read_restriction_summary()),
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: None,
            standards: Vec::new(),
            credential_count: Some(0),
            credential_block_reason: Some(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON.to_string()),
            raw_credential_exception_allowed: Some(false),
            raw_credential_exception_reason: None,
            replay_event_hashes: vec!["sha256:restricted-credential-replay".to_string()],
            replay_open_lineage_hashes: vec![
                "sha256:restricted-credential-openlineage".to_string(),
            ],
        }
    }

    fn qglake_human_credential_summary() -> LineageDrainEventSummary {
        LineageDrainEventSummary {
            event_id: "evt-human-credentials".to_string(),
            event_type: "credentials.vend-attempted".to_string(),
            principal_subject: Some("human:qglake-operator".to_string()),
            principal_kind: Some("human".to_string()),
            authorization_receipt_hash: Some("sha256:human-credential-authorization".to_string()),
            request_identity_state: Some("header-principal".to_string()),
            request_identity_source: Some("x-lakecat-principal".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            agent_delegation_hash: None,
            agent_summary_signature_hash: None,
            graph_events: 2,
            lineage_events: 1,
            bundle_hash: None,
            graph_hash: None,
            open_lineage_hash: None,
            querygraph_import_hash: None,
            table_artifact_count: 0,
            view_artifact_count: 0,
            view_version_receipt_hashes: Vec::new(),
            view_version_receipt_chain_hashes: Vec::new(),
            view_version_receipt_chain_verified_count: 0,
            view_warehouse: None,
            view_namespace: Vec::new(),
            view_name: None,
            view_stable_id: None,
            view_version: None,
            expected_view_version: None,
            policy_binding_count: 0,
            project_count: None,
            server_count: None,
            storage_profile_count: None,
            storage_profile_id: Some("events-local".to_string()),
            storage_profile_provider: Some("file".to_string()),
            storage_profile_issuance_mode: Some("local-file-no-secret".to_string()),
            storage_profile_location_prefix_hash: Some(
                "sha256:storage-location-prefix".to_string(),
            ),
            storage_profile_secret_ref_present: Some(false),
            storage_profile_secret_ref_provider: None,
            storage_profile_secret_ref_hash: None,
            warehouse_count: None,
            table_commit_count: None,
            table_commit_sequence_numbers: Vec::new(),
            table_commit_hashes: Vec::new(),
            scan_task_count: None,
            file_scan_task_count: None,
            delete_file_count: None,
            child_plan_task_count: None,
            read_restriction: Some(qglake_read_restriction_summary()),
            required_projection: Vec::new(),
            requested_projection: Vec::new(),
            effective_projection: Vec::new(),
            required_filters: Vec::new(),
            requested_stats_fields: Vec::new(),
            effective_stats_fields: Vec::new(),
            management_scope_project_id: None,
            management_scope_warehouse: None,
            standards: Vec::new(),
            credential_count: Some(1),
            credential_block_reason: None,
            raw_credential_exception_allowed: Some(true),
            raw_credential_exception_reason: Some(
                QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON.to_string(),
            ),
            replay_event_hashes: vec!["sha256:human-credential-replay".to_string()],
            replay_open_lineage_hashes: vec!["sha256:human-credential-openlineage".to_string()],
        }
    }

    fn qglake_lineage_standards() -> Vec<String> {
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

    fn qglake_querygraph_projection(
        policy: serde_json::Value,
    ) -> lakecat_querygraph::QueryGraphTableProjection {
        qglake_querygraph_projection_for("events", policy)
    }

    fn qglake_querygraph_projection_for(
        table: &str,
        policy: serde_json::Value,
    ) -> lakecat_querygraph::QueryGraphTableProjection {
        let warehouse = lakecat_core::WarehouseName::new("local").unwrap();
        let namespace = lakecat_core::Namespace::new(vec!["default".to_string()]).unwrap();
        let table_name = lakecat_core::TableName::new(table).unwrap();
        let ident = lakecat_core::TableIdent::new(warehouse, namespace, table_name);
        let stable_id = ident.stable_id();
        lakecat_querygraph::QueryGraphTableProjection {
            ident,
            stable_id: stable_id.clone(),
            location: format!("file:///tmp/lakecat-qglake/{table}"),
            metadata_location: Some(format!(
                "file:///tmp/lakecat-qglake/{table}/metadata/00000.json"
            )),
            version: 0,
            format_version: Some(3),
            croissant: serde_json::json!({}),
            cdif: serde_json::json!({}),
            osi: serde_json::json!({}),
            odrl: serde_json::json!({
                "lakecat:policy-bindings": [{
                    "policy-id": "events-agent-read",
                    "odrl": policy.clone()
                }]
            }),
            policy_bindings: vec![lakecat_querygraph::QueryGraphPolicyBindingProjection {
                policy_id: "events-agent-read".to_string(),
                enforced: true,
                namespace: Some(vec!["default".to_string()]),
                table: Some("events".to_string()),
                odrl: policy,
            }],
        }
    }

    fn qglake_querygraph_bundle(
        tables: Vec<lakecat_querygraph::QueryGraphTableProjection>,
        open_lineage_outputs: Vec<serde_json::Value>,
    ) -> QueryGraphBootstrap {
        let table_count = tables.len();
        let table_artifacts = tables
            .iter()
            .map(|table| lakecat_querygraph::QueryGraphTableArtifactHashes {
                stable_id: table.stable_id.clone(),
                croissant_hash: content_hash_json(&table.croissant).unwrap(),
                cdif_hash: content_hash_json(&table.cdif).unwrap(),
                osi_hash: content_hash_json(&table.osi).unwrap(),
                odrl_hash: content_hash_json(&table.odrl).unwrap(),
                policy_bindings_hash: content_hash_json(
                    &serde_json::to_value(&table.policy_bindings).unwrap(),
                )
                .unwrap(),
            })
            .collect::<Vec<_>>();
        let open_lineage_outputs = open_lineage_outputs
            .into_iter()
            .map(|mut output| {
                if output.pointer("/facets/dataSource/uri").is_none() {
                    output["facets"]["dataSource"] = serde_json::json!({
                        "uri": "file:///tmp/lakecat-qglake/events"
                    });
                }
                output
            })
            .collect::<Vec<_>>();
        let graph = lakecat_querygraph::QueryGraphCatalogGraph::from_tables(&tables);
        let graph_hash = content_hash_json(&serde_json::to_value(&graph).unwrap()).unwrap();
        let open_lineage = serde_json::json!({
            "eventType": "COMPLETE",
            "job": {
                "namespace": "lakecat.local",
                "name": "querygraph-bootstrap"
            },
            "producer": "https://querygraph.ai/lakecat",
            "schemaURL": "https://openlineage.io/spec/2-0-2/OpenLineage.json",
            "run": {
                "facets": {
                    "queryGraph_semanticBundle": {
                        "tableCount": table_count,
                        "viewCount": 0,
                        "standards": [
                            "Iceberg REST",
                            "Croissant",
                            "CDIF",
                            "OSI handoff",
                            "ODRL",
                            "Grust catalog graph",
                            "OpenLineage"
                        ],
                        "graphHash": graph_hash,
                        "tableArtifacts": table_artifacts.iter().map(qglake_open_lineage_table_artifact).collect::<Vec<_>>(),
                        "viewArtifacts": []
                    }
                }
            },
            "outputs": open_lineage_outputs
        });
        let mut manifest = lakecat_querygraph::QueryGraphBundleManifest {
            schema_version: "lakecat.querygraph.bootstrap.v1".to_string(),
            producer: "https://querygraph.ai/lakecat".to_string(),
            standards: vec![
                "Iceberg REST".to_string(),
                "Croissant".to_string(),
                "CDIF".to_string(),
                "OSI handoff".to_string(),
                "ODRL".to_string(),
                "Grust catalog graph".to_string(),
                "OpenLineage".to_string(),
            ],
            table_artifacts,
            view_artifacts: Vec::new(),
            graph_hash,
            open_lineage_hash: content_hash_json(&open_lineage).unwrap(),
            querygraph_import: None,
        };
        let warehouse = lakecat_core::WarehouseName::new("local").unwrap();
        manifest.querygraph_import = Some(lakecat_querygraph::QueryGraphImportCompatibility {
            schema_version: "lakecat.querygraph.import-compat.v1".to_string(),
            table_only_bundle_hash: qglake_querygraph_import_hash(
                &warehouse,
                &manifest,
                &tables,
                &graph,
                &open_lineage,
            ),
            view_count: 0,
            graph_hash: manifest.graph_hash.clone(),
            view_receipt_evidence: Vec::new(),
            view_receipt_evidence_hash: None,
        });
        let bundle_hash = content_hash_json(&serde_json::json!({
            "warehouse": warehouse.as_str(),
            "manifest": &manifest,
            "tables": &tables,
            "views": Vec::<serde_json::Value>::new(),
            "graph": &graph,
            "openLineage": &open_lineage,
        }))
        .unwrap();
        QueryGraphBootstrap {
            warehouse,
            generated_at: chrono::Utc::now(),
            bundle_hash,
            manifest,
            tables,
            views: Vec::new(),
            graph,
            open_lineage,
        }
    }

    fn qglake_querygraph_import_hash(
        warehouse: &lakecat_core::WarehouseName,
        manifest: &lakecat_querygraph::QueryGraphBundleManifest,
        tables: &[lakecat_querygraph::QueryGraphTableProjection],
        graph: &lakecat_querygraph::QueryGraphCatalogGraph,
        open_lineage: &serde_json::Value,
    ) -> String {
        content_hash_json(&serde_json::json!({
            "warehouse": warehouse.as_str(),
            "manifest": {
                "schema-version": manifest.schema_version,
                "producer": manifest.producer,
                "standards": manifest.standards,
                "table-artifacts": manifest.table_artifacts.iter().map(|artifact| serde_json::json!({
                    "stable-id": artifact.stable_id,
                    "croissant-hash": artifact.croissant_hash,
                    "cdif-hash": artifact.cdif_hash,
                    "osi-hash": artifact.osi_hash,
                    "odrl-hash": artifact.odrl_hash,
                })).collect::<Vec<_>>(),
                "open-lineage-hash": manifest.open_lineage_hash,
            },
            "tables": tables.iter().map(|table| serde_json::json!({
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
            })).collect::<Vec<_>>(),
            "graph": graph,
            "openLineage": open_lineage,
        }))
        .unwrap()
    }

    fn qglake_open_lineage_table_artifact(
        artifact: &lakecat_querygraph::QueryGraphTableArtifactHashes,
    ) -> Value {
        serde_json::json!({
            "stableId": artifact.stable_id,
            "croissantHash": artifact.croissant_hash,
            "cdifHash": artifact.cdif_hash,
            "osiHash": artifact.osi_hash,
            "odrlHash": artifact.odrl_hash,
            "policyBindingsHash": artifact.policy_bindings_hash
        })
    }
}
