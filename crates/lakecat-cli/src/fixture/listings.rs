use crate::*;

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_policy_list(
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

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_server_list(
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

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_project_list(
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

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_warehouse_list(
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

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_storage_profile_list(
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

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_table_commit_history(
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

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_table_commit_record_evidence(
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
    if !is_full_sha256_hash(&record.request_hash)
        || !is_full_sha256_hash(&record.response_hash)
        || !is_full_sha256_hash(&record.commit_hash)
        || !record
            .idempotency_key_sha256
            .as_deref()
            .is_some_and(is_full_sha256_hash)
        || record
            .policy_hash
            .as_deref()
            .is_some_and(|hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history for {warehouse}.{namespace_path}.{table} must expose full SHA-256 pointer-log hash evidence"
        )));
    }
    if record.format_version != Some(3) || record.snapshot_id.is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history for {warehouse}.{namespace_path}.{table} is missing Iceberg format/snapshot summary evidence"
        )));
    }
    Ok(())
}
