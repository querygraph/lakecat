use std::{collections::BTreeMap, fs, path::PathBuf};

use lakecat_api::{
    CatalogConfigResponse, CreateNamespaceRequest, CreateTableRequest, LineageDrainResponse,
    ListNamespacesResponse, ListPolicyBindingsResponse, ListStorageProfilesResponse,
    LoadTableResponse, NamespaceResponse, PlanTableScanRequest, PlanTableScanResponse,
    PolicyBindingResponse, StorageProfileResponse, TableIdentifier, UpsertPolicyBindingRequest,
    UpsertStorageProfileRequest,
};
use lakecat_querygraph::{QueryGraphBootstrap, QueryGraphBootstrapVerification};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

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
    let endpoint = format!("{}/querygraph/v1/bootstrap", catalog.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut request = client.get(endpoint);
    if let Some(principal) = principal {
        request = request.header("x-lakecat-principal", principal);
    }
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
    Ok(())
}

async fn drain_lineage_outbox(
    catalog: &str,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<LineageDrainResponse> {
    post_json::<_, LineageDrainResponse>(
        catalog,
        "/management/v1/lineage/drain",
        principal,
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
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.get(endpoint);
    if let Some(principal) = principal {
        request = request.header("x-lakecat-principal", principal);
    }
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
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.put(endpoint).json(body);
    if let Some(principal) = principal {
        request = request.header("x-lakecat-principal", principal);
    }
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })?;
    decode_json_response(response, label).await
}

async fn post_json<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<T> {
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.post(endpoint).json(body);
    if let Some(principal) = principal {
        request = request.header("x-lakecat-principal", principal);
    }
    let response = request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })?;
    decode_json_response(response, label).await
}

async fn post_json_or_conflict<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<Option<T>> {
    let endpoint = format!("{}{}", catalog.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut request = client.post(endpoint).json(body);
    if let Some(principal) = principal {
        request = request.header("x-lakecat-principal", principal);
    }
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
    let namespace_path = namespace.join(".");
    let storage_profile = format!("{table}-local");
    let policy = format!("{table}-agent-read");

    ensure_qglake_namespace(&catalog, &namespace, principal).await?;
    ensure_qglake_table(
        &catalog,
        &namespace_path,
        &namespace,
        &table,
        &location,
        &metadata_location,
        principal,
    )
    .await?;
    let _: StorageProfileResponse = put_json(
        &catalog,
        &format!("/management/v1/warehouses/{warehouse}/storage-profiles/{storage_profile}"),
        principal,
        "storage profile upsert",
        &UpsertStorageProfileRequest {
            location_prefix: location,
            provider: "file".to_string(),
            issuance_mode: "local-file-no-secret".to_string(),
            secret_ref: None,
            public_config: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;

    let _: PolicyBindingResponse = put_json(
        &catalog,
        &format!("/management/v1/warehouses/{warehouse}/policies/{policy}"),
        principal,
        "policy upsert",
        &UpsertPolicyBindingRequest {
            namespace: Some(namespace.clone()),
            table: Some(table.clone()),
            enforced: true,
            odrl: qglake_odrl_policy(&table),
        },
    )
    .await?;

    verify_qglake_governed_scan(&catalog, &namespace_path, &table, principal).await?;
    let (bundle, verification) = fetch_bootstrap_bundle(&catalog, principal).await?;
    verify_qglake_bootstrap_bundle(&bundle, &namespace, &table)?;
    write_bootstrap_bundle(&output, &bundle, &verification)?;
    let drain = drain_lineage_outbox(&catalog, principal).await?;
    println!("drained {} lineage/outbox event(s)", drain.delivered);
    Ok(())
}

async fn ensure_qglake_namespace(
    catalog: &str,
    namespace: &[String],
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    if post_json_or_conflict::<_, NamespaceResponse>(
        catalog,
        "/catalog/v1/namespaces",
        principal,
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

    let namespaces = get_json::<ListNamespacesResponse>(
        catalog,
        "/catalog/v1/namespaces",
        principal,
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
) -> lakecat_core::LakeCatResult<()> {
    let response = post_json_or_conflict::<_, LoadTableResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables"),
        principal,
        "table create",
        &CreateTableRequest {
            name: table.to_string(),
            location: location.to_string(),
            metadata_location: Some(metadata_location.to_string()),
            metadata: qglake_table_metadata(),
        },
    )
    .await?;
    if let Some(response) = response {
        return verify_qglake_existing_table(&response, namespace, table, metadata_location);
    }

    let response = get_json::<LoadTableResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}"),
        principal,
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
    verify_qglake_bootstrap_projection(projection, namespace, table)?;
    verify_qglake_bootstrap_open_lineage(bundle, projection)
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
    Ok(())
}

async fn verify_qglake_governed_scan(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let plan = post_json::<_, PlanTableScanResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/plan"),
        principal,
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
    verify_qglake_scan_plan(&plan)
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
    Ok(())
}

fn qglake_table_metadata() -> Value {
    json!({
        "format-version": 3,
        "current-schema-id": 1,
        "schemas": [{
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
        }]
    })
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
        let metadata = qglake_table_metadata();
        let fields = metadata["schemas"][0]["fields"].as_array().unwrap();
        assert!(fields.iter().any(|field| field["name"] == "raw_payload"));
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
        let response = LoadTableResponse {
            identifier: TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            metadata_location: Some("file:///tmp/lakecat-qglake/events/metadata.json".to_string()),
            metadata: qglake_table_metadata(),
            config: Vec::new(),
        };

        verify_qglake_existing_table(
            &response,
            &["default".to_string()],
            "events",
            "file:///tmp/lakecat-qglake/events/metadata.json",
        )
        .expect("matching QGLake fixture table should be accepted");
    }

    #[test]
    fn qglake_existing_table_verifier_rejects_drifted_fixture_table() {
        let mut metadata = qglake_table_metadata();
        metadata["schemas"][0]["fields"]
            .as_array_mut()
            .unwrap()
            .retain(|field| field["name"] != "raw_payload");
        let response = LoadTableResponse {
            identifier: TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            metadata_location: Some("file:///tmp/lakecat-qglake/events/metadata.json".to_string()),
            metadata,
            config: Vec::new(),
        };

        let err = verify_qglake_existing_table(
            &response,
            &["default".to_string()],
            "events",
            "file:///tmp/lakecat-qglake/events/metadata.json",
        )
        .expect_err("drifted QGLake fixture table should be rejected");
        assert!(err.to_string().contains("raw_payload"));
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
        let plan = PlanTableScanResponse {
            table: lakecat_api::TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            planned_by: "lakecat-sail".to_string(),
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
                        }
                    }
                }
            })),
        };

        verify_qglake_scan_plan(&plan).unwrap();
    }

    fn qglake_querygraph_projection(
        policy: serde_json::Value,
    ) -> lakecat_querygraph::QueryGraphTableProjection {
        let warehouse = lakecat_core::WarehouseName::new("local").unwrap();
        let namespace = lakecat_core::Namespace::new(vec!["default".to_string()]).unwrap();
        let table = lakecat_core::TableName::new("events").unwrap();
        let ident = lakecat_core::TableIdent::new(warehouse, namespace, table);
        let stable_id = ident.stable_id();
        lakecat_querygraph::QueryGraphTableProjection {
            ident,
            stable_id: stable_id.clone(),
            location: "file:///tmp/lakecat-qglake/events".to_string(),
            metadata_location: Some(
                "file:///tmp/lakecat-qglake/events/metadata/00000.json".to_string(),
            ),
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
        QueryGraphBootstrap {
            warehouse: lakecat_core::WarehouseName::new("local").unwrap(),
            generated_at: chrono::Utc::now(),
            bundle_hash: "test".to_string(),
            manifest: lakecat_querygraph::QueryGraphBundleManifest {
                schema_version: "lakecat.querygraph.bootstrap.v1".to_string(),
                producer: "https://querygraph.ai/lakecat".to_string(),
                standards: vec!["ODRL".to_string(), "OpenLineage".to_string()],
                table_artifacts: Vec::new(),
                open_lineage_hash: "test".to_string(),
            },
            tables,
            graph: lakecat_querygraph::QueryGraphCatalogGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            open_lineage: serde_json::json!({
                "outputs": open_lineage_outputs
            }),
        }
    }
}
