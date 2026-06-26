use lakecat_core::{LakeCatError, content_hash_bytes};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use lakecat_store::StorageProfile;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, ObjectStoreScheme, PutMode, PutPayload};
use url::Url;

use crate::*;

/// Process-global cache of constructed object stores, keyed by scheme+authority
/// (one client per bucket/host). Building an object store — credential-chain
/// resolution, the HTTP client, and its connection pool — is expensive; the
/// per-object `Path` is cheap. So we derive the path on every call and reuse the
/// client across commits instead of rebuilding it per request.
fn object_store_cache() -> &'static RwLock<HashMap<String, Arc<dyn ObjectStore>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Arc<dyn ObjectStore>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

pub(crate) fn metadata_object_store(
    location: &str,
) -> Result<(Arc<dyn ObjectStore>, ObjectPath), LakeCatError> {
    let url = Url::parse(location).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "invalid metadata location {}; {}",
            metadata_location_hash_context(location),
            backend_error_hash_context(err)
        ))
    })?;
    // Derive (scheme, object path) exactly as `object_store::parse_url_opts` does
    // internally, without constructing a client.
    let (scheme, object_path) = ObjectStoreScheme::parse(&url).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "metadata object location {} is not supported or is not configured: {}",
            metadata_location_hash_context(location),
            backend_error_hash_context(err)
        ))
    })?;
    let cache_key = format!("{scheme:?}|{}", url.authority());
    if let Some(store) = object_store_cache()
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&cache_key)
        .cloned()
    {
        return Ok((store, object_path));
    }
    // Cache miss: build the client once (the expensive step) and memoize it.
    let (store, _path) = object_store::parse_url_opts(&url, std::env::vars()).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "metadata object location {} is not supported or is not configured: {}",
            metadata_location_hash_context(location),
            backend_error_hash_context(err)
        ))
    })?;
    let store: Arc<dyn ObjectStore> = Arc::from(store);
    object_store_cache()
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(cache_key, store.clone());
    Ok((store, object_path))
}

pub(crate) async fn write_planned_metadata(
    commit_plan: &lakecat_core::sail::CommitPlan,
) -> Result<Option<PlannedMetadataWrite>, LakeCatError> {
    if !commit_plan.metadata_write_required {
        return Ok(None);
    }
    let Some(location) = commit_plan.new_metadata_location.as_deref() else {
        return Ok(None);
    };
    let (store, object_path) = metadata_object_store(location)?;
    let payload = serde_json::to_vec_pretty(&commit_plan.new_metadata)
        .map_err(|err| LakeCatError::Internal(format!("failed to encode metadata JSON: {err}")))?;
    store
        .put_opts(
            &object_path,
            PutPayload::from(payload),
            PutMode::Create.into(),
        )
        .await
        .map_err(|err| match err {
            object_store::Error::AlreadyExists { .. } => LakeCatError::Conflict(format!(
                "metadata object {} already exists; refusing to overwrite existing metadata",
                metadata_location_hash_context(location)
            )),
            err => metadata_object_write_error(location, err),
        })?;
    Ok(Some(PlannedMetadataWrite {
        location: location.to_string(),
    }))
}

pub(crate) fn validate_planned_metadata_location(
    commit_plan: &lakecat_core::sail::CommitPlan,
    current_metadata_location: Option<&str>,
    storage_profile: &StorageProfile,
) -> Result<(), LakeCatError> {
    if !commit_plan.metadata_write_required {
        return Ok(());
    }
    let Some(new_metadata_location) = commit_plan.new_metadata_location.as_deref() else {
        return Err(LakeCatError::InvalidArgument(
            "metadata object commit requires a new metadata location".to_string(),
        ));
    };
    if current_metadata_location == Some(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object commit must not overwrite the current metadata location; {}",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if location_has_dot_path_segment(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} must not contain dot path segments",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if location_has_query_or_fragment(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} must not include query strings or fragments",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if location_has_userinfo(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} must not include userinfo",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if location_has_credential_marker(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} must not contain credential material",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if !location_is_strictly_within_prefix(
        new_metadata_location,
        storage_profile.location_prefix.as_str(),
    ) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} is outside the selected storage profile prefix or is not a child object; storage-profile-prefix-hash={}",
            metadata_location_hash_context(new_metadata_location),
            content_hash_bytes(storage_profile.location_prefix.as_bytes())
        )));
    }
    Ok(())
}

pub(crate) fn location_has_dot_path_segment(location: &str) -> bool {
    let path = location
        .split_once(['?', '#'])
        .map_or(location, |(path, _)| path);
    path.split('/').any(is_dot_path_segment)
}

pub(crate) fn location_has_query_or_fragment(location: &str) -> bool {
    Url::parse(location)
        .map(|url| url.query().is_some() || url.fragment().is_some())
        .unwrap_or_else(|_| location.contains(['?', '#']))
}

pub(crate) fn location_has_userinfo(location: &str) -> bool {
    Url::parse(location)
        .map(|url| !url.username().is_empty() || url.password().is_some())
        .unwrap_or(false)
}

pub(crate) fn location_has_credential_marker(location: &str) -> bool {
    const MARKERS: [&str; 6] = [
        "token=",
        "secret=",
        "credential=",
        "password=",
        "access_key=",
        "session_token=",
    ];

    fn contains_marker(value: &str) -> bool {
        let normalized = value.to_ascii_lowercase();
        MARKERS.iter().any(|marker| normalized.contains(marker))
    }

    contains_marker(location)
        || location
            .split(['/', '?', '#'])
            .filter_map(percent_decode_segment)
            .filter_map(|segment| String::from_utf8(segment).ok())
            .any(|segment| contains_marker(&segment))
}

pub(crate) fn is_dot_path_segment(segment: &str) -> bool {
    let Some(decoded) = percent_decode_segment(segment) else {
        return segment == "." || segment == "..";
    };
    decoded.as_slice() == b"." || decoded.as_slice() == b".."
}

pub(crate) fn percent_decode_segment(segment: &str) -> Option<Vec<u8>> {
    if !segment.as_bytes().contains(&b'%') {
        return None;
    }
    let mut decoded = Vec::with_capacity(segment.len());
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let Some(high) = hex_value(bytes[index + 1]) else {
                decoded.push(bytes[index]);
                index += 1;
                continue;
            };
            let Some(low) = hex_value(bytes[index + 2]) else {
                decoded.push(bytes[index]);
                index += 1;
                continue;
            };
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Some(decoded)
}

pub(crate) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn location_is_within_prefix(location: &str, prefix: &str) -> bool {
    if location == prefix {
        return true;
    }
    location_is_strictly_within_prefix(location, prefix)
}

pub(crate) fn location_is_strictly_within_prefix(location: &str, prefix: &str) -> bool {
    if location == prefix {
        return false;
    }
    if prefix.ends_with('/') {
        location.starts_with(prefix)
    } else {
        location
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
    }
}

pub(crate) async fn cleanup_planned_metadata(
    write: Option<PlannedMetadataWrite>,
    previous_metadata_location: Option<&str>,
) -> Result<(), LakeCatError> {
    let Some(write) = write else {
        return Ok(());
    };
    if previous_metadata_location == Some(write.location.as_str()) {
        return Ok(());
    }
    let (store, object_path) = metadata_object_store(&write.location)?;
    for attempt in 0..METADATA_CLEANUP_DELETE_ATTEMPTS {
        match store.delete(&object_path).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => return Ok(()),
            Err(err) if attempt + 1 == METADATA_CLEANUP_DELETE_ATTEMPTS => {
                return Err(metadata_cleanup_error(&write.location, err));
            }
            Err(_) => tokio::time::sleep(metadata_cleanup_retry_delay(attempt)).await,
        }
    }
    unreachable!("metadata cleanup retry loop must return")
}

pub(crate) const METADATA_CLEANUP_DELETE_ATTEMPTS: usize = 3;

pub(crate) fn metadata_cleanup_retry_delay(attempt: usize) -> std::time::Duration {
    std::time::Duration::from_millis(25 * (attempt as u64 + 1))
}

pub(crate) fn metadata_object_write_error(
    metadata_location: &str,
    err: impl std::fmt::Display,
) -> LakeCatError {
    LakeCatError::Internal(format!(
        "failed to write metadata object {}: {}",
        metadata_location_hash_context(metadata_location),
        error_detail_hash_context(err)
    ))
}

pub(crate) fn metadata_cleanup_error(
    metadata_location: &str,
    err: impl std::fmt::Display,
) -> LakeCatError {
    LakeCatError::Internal(format!(
        "failed to clean up uncommitted metadata object {}: {}",
        metadata_location_hash_context(metadata_location),
        error_detail_hash_context(err)
    ))
}

pub(crate) fn metadata_location_hash_context(metadata_location: &str) -> String {
    format!(
        "metadata-location-hash={}",
        content_hash_bytes(metadata_location.as_bytes())
    )
}

pub(crate) fn error_detail_hash_context(err: impl std::fmt::Display) -> String {
    format!(
        "error-detail-hash={}",
        content_hash_bytes(err.to_string().as_bytes())
    )
}

pub(crate) fn backend_error_hash_context(err: impl std::fmt::Display) -> String {
    format!(
        "backend-error-hash={}",
        content_hash_bytes(err.to_string().as_bytes())
    )
}

pub(crate) async fn cleanup_planned_metadata_after_commit_error(
    write: Option<PlannedMetadataWrite>,
    previous_metadata_location: Option<&str>,
    commit_error: LakeCatError,
) -> LakeCatError {
    match cleanup_planned_metadata(write, previous_metadata_location).await {
        Ok(()) => commit_error,
        Err(cleanup_error) => commit_error_with_cleanup_failure(commit_error, cleanup_error),
    }
}

pub(crate) fn commit_error_with_cleanup_failure(
    commit_error: LakeCatError,
    cleanup_error: LakeCatError,
) -> LakeCatError {
    let cleanup_context = format!(
        "metadata cleanup also failed; {}",
        error_detail_hash_context(cleanup_error)
    );
    match commit_error {
        LakeCatError::InvalidArgument(message) => {
            LakeCatError::InvalidArgument(format!("{message}; {cleanup_context}"))
        }
        LakeCatError::NotFound { object, name } => LakeCatError::NotFound {
            object,
            name: format!("{name}; {cleanup_context}"),
        },
        LakeCatError::Conflict(message) => {
            LakeCatError::Conflict(format!("{message}; {cleanup_context}"))
        }
        LakeCatError::NotSupported(message) => {
            LakeCatError::NotSupported(format!("{message}; {cleanup_context}"))
        }
        LakeCatError::Internal(message) => {
            LakeCatError::Internal(format!("{message}; {cleanup_context}"))
        }
    }
}
