use crate::*;

/// Build the absolute request URL from a catalog base and an API path,
/// trimming a single trailing slash from the base (identical to the inline
/// expression previously repeated in every request helper).
fn lakecat_endpoint(catalog: &str, path: &str) -> String {
    format!("{}{}", catalog.trim_end_matches('/'), path)
}

/// Send a prepared request, mapping transport failures to the same
/// `Internal` error previously constructed inline in every request helper.
async fn send_request(
    request: reqwest::RequestBuilder,
    label: &str,
) -> lakecat_core::LakeCatResult<reqwest::Response> {
    request.send().await.map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to request {label}: {err}"))
    })
}

pub(crate) async fn get_json<T: DeserializeOwned>(
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

pub(crate) async fn get_json_with_identity<T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
) -> lakecat_core::LakeCatResult<T> {
    let mut request = reqwest::Client::new().get(lakecat_endpoint(catalog, path));
    request = identity_mode.apply(request, principal);
    let response = send_request(request, label).await?;
    decode_json_response(response, label).await
}

pub(crate) async fn put_json<B: Serialize, T: DeserializeOwned>(
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

pub(crate) async fn put_json_with_identity<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<T> {
    let mut request = reqwest::Client::new()
        .put(lakecat_endpoint(catalog, path))
        .json(body);
    request = identity_mode.apply(request, principal);
    let response = send_request(request, label).await?;
    decode_json_response(response, label).await
}

pub(crate) async fn post_json_with_identity<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<T> {
    let mut request = reqwest::Client::new()
        .post(lakecat_endpoint(catalog, path))
        .json(body);
    request = identity_mode.apply(request, principal);
    let response = send_request(request, label).await?;
    decode_json_response(response, label).await
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn post_json_with_identity_and_idempotency<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    idempotency_key: &str,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<T> {
    let mut request = reqwest::Client::new()
        .post(lakecat_endpoint(catalog, path))
        .header("x-lakecat-idempotency-key", idempotency_key)
        .json(body);
    request = identity_mode.apply(request, principal);
    let response = send_request(request, label).await?;
    decode_json_response(response, label).await
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn post_json_or_conflict_with_identity<B: Serialize, T: DeserializeOwned>(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
    body: &B,
) -> lakecat_core::LakeCatResult<Option<T>> {
    let mut request = reqwest::Client::new()
        .post(lakecat_endpoint(catalog, path))
        .json(body);
    request = identity_mode.apply(request, principal);
    let response = send_request(request, label).await?;
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

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn delete_with_identity(
    catalog: &str,
    path: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let mut request = reqwest::Client::new().delete(lakecat_endpoint(catalog, path));
    request = identity_mode.apply(request, principal);
    let response = send_request(request, label).await?;
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
#[cfg_attr(not(feature = "qglake-fixture"), allow(dead_code))]
pub(crate) enum RequestIdentityMode {
    Principal,
    AgentDid,
}

impl RequestIdentityMode {
    pub(crate) fn apply(
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

pub(crate) async fn decode_json_response<T: DeserializeOwned>(
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

pub(crate) fn print_json<T: Serialize>(value: &T) -> lakecat_core::LakeCatResult<()> {
    let pretty = serde_json::to_string_pretty(value).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!("failed to encode JSON response: {err}"))
    })?;
    println!("{pretty}");
    Ok(())
}
