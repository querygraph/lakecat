use crate::*;

pub(crate) async fn bootstrap_export(
    catalog: String,
    output: PathBuf,
    principal: Option<String>,
) -> lakecat_core::LakeCatResult<()> {
    let (bundle, verification) = fetch_bootstrap_bundle(&catalog, principal.as_deref()).await?;
    write_bootstrap_bundle(&output, &bundle, &verification)
}

pub(crate) async fn fetch_bootstrap_bundle(
    catalog: &str,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<(QueryGraphBootstrap, QueryGraphBootstrapVerification)> {
    fetch_bootstrap_bundle_with_identity(catalog, principal, RequestIdentityMode::Principal).await
}

pub(crate) async fn fetch_bootstrap_bundle_with_identity(
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

pub(crate) fn write_bootstrap_bundle(
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

#[cfg(feature = "qglake-fixture")]
pub(crate) fn write_json_file<T: Serialize>(
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

pub(crate) async fn config(
    catalog: String,
    principal: Option<String>,
) -> lakecat_core::LakeCatResult<()> {
    let config = get_json::<CatalogConfigResponse>(
        &catalog,
        "/catalog/v1/config",
        principal.as_deref(),
        "catalog config",
    )
    .await?;
    print_json(&config)
}

pub(crate) async fn lineage_drain(
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
