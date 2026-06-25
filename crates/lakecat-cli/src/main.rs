#[cfg(feature = "qglake-fixture")]
pub(crate) use std::sync::Arc;
pub(crate) use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

pub(crate) use chrono::{DateTime, SecondsFormat, Utc};
pub(crate) use lakecat_api::{
    CatalogConfigResponse, ConfigEntry, LAKECAT_COMPATIBILITY_KEY, LAKECAT_COMPATIBILITY_VALUE,
    LAKECAT_FORMAT_BASELINE_KEY, LAKECAT_FORMAT_BASELINE_VALUE, LAKECAT_FORMAT_V4_BRIDGE_KEY,
    LAKECAT_FORMAT_V4_BRIDGE_VALUE, LAKECAT_FORMAT_V4_KEY, LAKECAT_FORMAT_V4_TYPED_SAIL_KEY,
    LAKECAT_FORMAT_V4_TYPED_SAIL_VALUE, LAKECAT_FORMAT_V4_VALUE, LineageDrainEventSummary,
    LineageDrainResponse, ListPolicyBindingsResponse, ListStorageProfilesResponse,
    PolicyBindingResponse, StorageProfileResponse, UpsertPolicyBindingRequest,
    UpsertStorageProfileRequest, ViewVersionReceiptChainResponse, ViewVersionReceiptResponse,
};
#[cfg(feature = "qglake-fixture")]
pub(crate) use lakecat_api::{
    CommitTableRequest, CommitTableResponse, CreateNamespaceRequest, CreateTableRequest,
    FetchScanTasksRequest, ListProjectsResponse, ListServersResponse,
    ListTableCommitRecordsResponse, ListViewVersionReceiptChainsResponse,
    ListViewVersionReceiptsResponse, ListWarehousesResponse, LoadTableResponse, NamespaceResponse,
    PlanTableScanRequest, ProjectResponse, ServerResponse, TableIdentifier, UpsertProjectRequest,
    UpsertServerRequest, UpsertViewRequest, UpsertWarehouseRequest, ViewResponse,
    WarehouseResponse,
};
#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) use lakecat_api::{
    FetchScanTasksResponse, ListNamespacesResponse, LoadCredentialsResponse, PlanTableScanResponse,
};
pub(crate) use lakecat_core::{content_hash_bytes, content_hash_json};
pub(crate) use lakecat_querygraph::{QueryGraphBootstrap, QueryGraphBootstrapVerification};
#[cfg(feature = "qglake-fixture")]
pub(crate) use sail_iceberg::spec::{
    DataContentType, DataFile, DataFileFormat, FormatVersion, ManifestContentType, ManifestFile,
    ManifestListWriter, ManifestMetadata, ManifestWriterBuilder, TableMetadata,
};
pub(crate) use serde::{Serialize, de::DeserializeOwned};
pub(crate) use serde_json::{Value, json};
pub(crate) use url::Url;

mod cli;
mod commands;
mod fixture;
mod http;
mod lineage;
mod replay_evidence;
mod verify_handoff;
mod verify_proof;
mod verify_receipts;
mod verify_replay;

pub(crate) use cli::*;
pub(crate) use commands::*;
pub(crate) use fixture::*;
pub(crate) use http::*;
pub(crate) use lineage::*;
pub(crate) use replay_evidence::*;
pub(crate) use verify_handoff::*;
pub(crate) use verify_proof::*;
pub(crate) use verify_receipts::*;
pub(crate) use verify_replay::*;

pub(crate) const QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON: &str =
    "fine-grained read restriction requires Sail-planned reads";
pub(crate) const QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON: &str =
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
        #[cfg(feature = "qglake-fixture")]
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

#[cfg(test)]
mod tests;
