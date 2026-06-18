use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::Arc,
};

use lakecat_api::{
    CatalogConfigResponse, CreateNamespaceRequest, CreateTableRequest, FetchScanTasksRequest,
    FetchScanTasksResponse, LineageDrainEventSummary, LineageDrainResponse, ListNamespacesResponse,
    ListPolicyBindingsResponse, ListStorageProfilesResponse, LoadCredentialsResponse,
    LoadTableResponse, NamespaceResponse, PlanTableScanRequest, PlanTableScanResponse,
    PolicyBindingResponse, StorageProfileResponse, TableIdentifier, UpsertPolicyBindingRequest,
    UpsertStorageProfileRequest,
};
use lakecat_core::content_hash_json;
use lakecat_querygraph::{QueryGraphBootstrap, QueryGraphBootstrapVerification};
use sail_iceberg::spec::{
    DataContentType, DataFile, DataFileFormat, FormatVersion, ManifestContentType, ManifestFile,
    ManifestListWriter, ManifestMetadata, ManifestWriterBuilder, TableMetadata,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;

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
    if !response.event_types.is_empty() {
        println!("event types {}", response.event_types.join(","));
    }
    Ok(())
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
    principal: Option<String>,
) -> lakecat_core::LakeCatResult<()> {
    let principal = principal.as_deref();
    let identity_mode = RequestIdentityMode::AgentDid;
    let namespace_path = namespace.join(".");
    let storage_profile = format!("{table}-local");
    let policy = format!("{table}-agent-read");

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
    let (bundle, verification) =
        fetch_bootstrap_bundle_with_identity(&catalog, principal, identity_mode).await?;
    verify_qglake_bootstrap_bundle(&bundle, &namespace, &table)?;
    write_bootstrap_bundle(&output, &bundle, &verification)?;
    let drain = drain_lineage_outbox_with_identity(&catalog, principal, identity_mode).await?;
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
            "type": "not_eq",
            "term": "severity",
            "value": "debug"
        })
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy row predicate was not exported as expected: {}",
            restriction["row-predicate"].clone()
        )));
    }
    if projection.odrl["lakecat:policy-bindings"][0]["policy-id"] != expected_policy {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap ODRL table projection did not embed {expected_policy}"
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
    if extension["read-restriction"]["row-predicate"]
        != json!({
            "type": "not_eq",
            "term": "severity",
            "value": "debug"
        })
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed scan row predicate was not enforced as expected: {}",
            extension["read-restriction"]["row-predicate"].clone()
        )));
    }
    let expected_policy_hash = qglake_policy_hash(plan.table.name.as_str())?;
    let policy_hashes = extension["read-restriction"]["policy-hashes"]
        .as_array()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "qglake governed scan read restriction did not include policy hashes".to_string(),
            )
        })?;
    if !policy_hashes
        .iter()
        .any(|hash| hash.as_str() == Some(expected_policy_hash.as_str()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed scan did not bind to expected ODRL policy hash {expected_policy_hash}: {}",
            extension["read-restriction"]["policy-hashes"].clone()
        )));
    }
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
    for child_plan_task in &fetched.plan_tasks {
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
        verify_qglake_leaf_scan_tasks(&manifest_fetched, table_location)?;
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
    if extension["read-restriction"]["allowed-columns"]
        != json!(["event_id", "occurred_at", "severity"])
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks allowed columns were not re-applied as expected: {}",
            extension["read-restriction"]["allowed-columns"].clone()
        )));
    }
    if extension["read-restriction"]["row-predicate"]
        != json!({
            "type": "not_eq",
            "term": "severity",
            "value": "debug"
        })
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks row predicate was not re-applied as expected: {}",
            extension["read-restriction"]["row-predicate"].clone()
        )));
    }
    let expected_policy_hash = qglake_policy_hash(fetched.table.name.as_str())?;
    let policy_hashes = extension["read-restriction"]["policy-hashes"]
        .as_array()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "qglake governed fetchScanTasks read restriction did not include policy hashes"
                    .to_string(),
            )
        })?;
    if !policy_hashes
        .iter()
        .any(|hash| hash.as_str() == Some(expected_policy_hash.as_str()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks did not bind to expected ODRL policy hash {expected_policy_hash}: {}",
            extension["read-restriction"]["policy-hashes"].clone()
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
    let Some(bootstrap) = drain
        .events
        .iter()
        .find(|event| event.event_type == "querygraph.bootstrap")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not expose querygraph.bootstrap replay evidence".to_string(),
        ));
    };
    if bootstrap.bundle_hash.as_deref().map_or(true, str::is_empty)
        || bootstrap.graph_hash.as_deref().map_or(true, str::is_empty)
        || bootstrap
            .open_lineage_hash
            .as_deref()
            .map_or(true, str::is_empty)
        || bootstrap
            .querygraph_import_hash
            .as_deref()
            .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing QueryGraph hashes".to_string(),
        ));
    }
    let expected_principal = principal.unwrap_or("anonymous");
    if bootstrap.principal_subject.as_deref() != Some(expected_principal) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain replay principal did not match accepted principal {expected_principal}"
        )));
    }
    let expected_principal_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    if bootstrap.principal_kind.as_deref() != Some(expected_principal_kind) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain replay principal kind did not match accepted principal kind {expected_principal_kind}"
        )));
    }
    if bootstrap
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing authorization receipt hash"
                .to_string(),
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
            .map_or(true, str::is_empty)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence is missing agent delegation hash".to_string(),
            ));
        }
        if bootstrap
            .agent_summary_signature_hash
            .as_deref()
            .map_or(true, str::is_empty)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence is missing agent summary signature hash"
                    .to_string(),
            ));
        }
    }
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
    if bootstrap.replay_event_hashes.is_empty()
        || bootstrap.replay_event_hashes.iter().any(String::is_empty)
        || bootstrap.replay_open_lineage_hashes.is_empty()
        || bootstrap
            .replay_open_lineage_hashes
            .iter()
            .any(String::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing sink receipt hashes".to_string(),
        ));
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
    fs::create_dir_all(&data_dir).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to create QGLake data directory {data_dir:?}: {err}"
        ))
    })?;

    let manifest_list_path = metadata_dir.join("snap-42.avro");
    let manifest_path = metadata_dir.join("manifest-1.avro");
    let data_file_path = data_dir.join("part-1.parquet");
    let manifest_list = file_path_url(&manifest_list_path, "QGLake manifest list")?;
    let manifest = file_path_url(&manifest_path, "QGLake data manifest")?;
    let data_file = file_path_url(&data_file_path, "QGLake data file")?;

    let metadata = json!({
        "format-version": 3,
        "table-uuid": "22222222-2222-2222-2222-222222222222",
        "location": location,
        "last-sequence-number": 7,
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
            "sequence-number": 7,
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
        &manifest_list_path,
        &data_file,
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
    manifest_list_path: &std::path::Path,
    data_file: &str,
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
                "type": "not_eq",
                "term": "severity",
                "value": "debug"
            },
            "max-credential-ttl-seconds": 300
        },
        "permission": [{
            "target": table,
            "action": "odrl:read",
            "constraint": [{
                "leftOperand": "typesec:capability",
                "operator": "odrl:eq",
                "rightOperand": "catalog.table.plan_scan"
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
    fn qglake_fixture_policy_installs_read_restriction() {
        let policy = qglake_odrl_policy("events");
        assert_eq!(
            policy["lakecat:read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id", "occurred_at", "severity"])
        );
        assert_eq!(
            policy["lakecat:read-restriction"]["row-predicate"],
            serde_json::json!({
                "type": "not_eq",
                "term": "severity",
                "value": "debug"
            })
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
                "type": "not_eq",
                "term": "severity",
                "value": "debug"
            }))
        );
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
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
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
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
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
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
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
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
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
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        }
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
            "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/manifest-42.avro"
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
                }
            })),
        };

        let err = verify_qglake_leaf_scan_tasks(&fetched, QGLAKE_TEST_LOCATION)
            .expect_err("QGLake leaf manifest fetch should be terminal");
        assert!(err.to_string().contains("unexpectedly exposed"));
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
                }
            })),
        };

        verify_qglake_scan_tasks(&fetched, QGLAKE_TEST_LOCATION).unwrap();
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
            file_scan_tasks: vec![serde_json::json!({
                "data-file": {
                    "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
                }
            })],
            delete_files: Vec::new(),
            plan_tasks: vec![
                "lakecat:sail-json-hmac:manifest:1".to_string(),
                "lakecat:sail-json-hmac:manifest:2".to_string(),
            ],
            lakecat_plan_tasks: vec![
                serde_json::json!({
                    "task-type": "manifest",
                    "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro",
                    "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/manifest-42-a.avro"
                }),
                serde_json::json!({
                    "task-type": "manifest",
                    "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro",
                    "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/manifest-42-b.avro"
                }),
            ],
            residual_filter: Some(serde_json::json!({
                "lakecat:fetch-scan-tasks": {
                    "read-restriction": {
                        "allowed-columns": ["event_id", "occurred_at", "severity"],
                        "row-predicate": {
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        },
                        "policy-hashes": [expected_policy_hash]
                    }
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
                            "type": "not_eq",
                            "term": "severity",
                            "value": "debug"
                        }
                    }
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
    fn qglake_lineage_drain_verifier_requires_delivered_events() {
        let verification = qglake_lineage_verification();
        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 0,
                event_types: Vec::new(),
                graph_events: 0,
                lineage_events: 0,
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
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
                    replay_event_hashes: Vec::new(),
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should require sink receipt evidence");
        assert!(
            err.to_string()
                .contains("qglake lineage drain replay evidence is missing sink receipt hashes")
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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
            err.to_string()
                .contains("qglake lineage drain replay evidence is missing QueryGraph hashes")
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:other".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("human".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: None,
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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
            "qglake lineage drain replay evidence is missing authorization receipt hash"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: None,
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect_err("QGLake lineage drain should reject missing agent delegation proof");
        assert!(
            err.to_string()
                .contains("qglake lineage drain replay evidence is missing agent delegation hash")
        );

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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
            "qglake lineage drain replay evidence is missing agent summary signature hash"
        ));

        let err = verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 1,
                event_types: vec!["querygraph.bootstrap".to_string()],
                graph_events: 1,
                lineage_events: 1,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: vec!["OpenLineage".to_string()],
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
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 0,
                    standards: qglake_lineage_standards(),
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
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
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

        verify_qglake_lineage_drain(
            &LineageDrainResponse {
                delivered: 2,
                event_types: vec![
                    "table.scan-planned".to_string(),
                    "querygraph.bootstrap".to_string(),
                ],
                graph_events: 1,
                lineage_events: 2,
                events: vec![LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some("sha256:authorization".to_string()),
                    request_identity_state: Some("verified".to_string()),
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
                    policy_binding_count: 1,
                    standards: qglake_lineage_standards(),
                    replay_event_hashes: vec!["sha256:replay-event".to_string()],
                    replay_open_lineage_hashes: vec!["sha256:replay-openlineage".to_string()],
                }],
            },
            &verification,
            Some("did:example:agent"),
            1,
        )
        .expect("QGLake lineage drain should accept delivered outbox events");
    }

    fn qglake_lineage_verification() -> QueryGraphBootstrapVerification {
        QueryGraphBootstrapVerification {
            warehouse: "local".to_string(),
            table_count: 1,
            view_count: 0,
            verified_tables: vec!["local.default.events".to_string()],
            verified_views: Vec::new(),
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
