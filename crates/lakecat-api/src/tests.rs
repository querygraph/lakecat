use super::*;
use std::collections::BTreeMap;

#[test]
fn catalog_config_defaults_pin_iceberg_v4_bridge_posture() {
    let defaults = CatalogConfigResponse::default()
        .defaults
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        defaults.get(LAKECAT_COMPATIBILITY_KEY).map(String::as_str),
        Some(LAKECAT_COMPATIBILITY_VALUE)
    );
    assert_eq!(
        defaults
            .get(LAKECAT_FORMAT_BASELINE_KEY)
            .map(String::as_str),
        Some(LAKECAT_FORMAT_BASELINE_VALUE)
    );
    assert_eq!(
        defaults.get(LAKECAT_FORMAT_V4_KEY).map(String::as_str),
        Some(LAKECAT_FORMAT_V4_VALUE)
    );
    assert_eq!(
        defaults
            .get(LAKECAT_FORMAT_V4_BRIDGE_KEY)
            .map(String::as_str),
        Some(LAKECAT_FORMAT_V4_BRIDGE_VALUE)
    );
    assert_eq!(
        defaults
            .get(LAKECAT_FORMAT_V4_TYPED_SAIL_KEY)
            .map(String::as_str),
        Some(LAKECAT_FORMAT_V4_TYPED_SAIL_VALUE)
    );
}

#[test]
fn catalog_config_endpoints_advertise_table_create_routes() {
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();

    assert!(endpoints.contains("POST /catalog/v1/namespaces/{namespace}/tables"));
    assert!(endpoints.contains("POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables"));
}

#[test]
fn catalog_config_endpoints_advertise_querygraph_integration_routes() {
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();

    assert!(endpoints.contains("POST /management/v1/lineage/drain"));
    assert!(endpoints.contains("GET /querygraph/v1/bootstrap"));
}
