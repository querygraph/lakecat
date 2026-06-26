use crate::*;

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_credentials_blocked(
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

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_credentials_response(
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

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_trusted_human_credentials(
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

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_trusted_human_credentials_response(
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
