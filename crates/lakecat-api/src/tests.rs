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
fn catalog_config_maps_serialize_as_json_objects() {
    let config = CatalogConfigResponse::default();
    let value = serde_json::to_value(&config).unwrap();

    // `defaults`/`overrides` must be JSON objects (string -> string), not the
    // legacy array-of-{key,value} shape stock clients cannot parse.
    let defaults = value["defaults"]
        .as_object()
        .expect("defaults should serialize as a JSON object");
    assert_eq!(
        defaults
            .get(LAKECAT_COMPATIBILITY_KEY)
            .and_then(serde_json::Value::as_str),
        Some(LAKECAT_COMPATIBILITY_VALUE)
    );
    assert!(
        value["overrides"].is_object(),
        "overrides should be a JSON object"
    );
    // `endpoints` stays an array of strings.
    assert!(value["endpoints"].is_array());
}

#[test]
fn empty_config_map_serializes_as_empty_object() {
    let response = NamespaceResponse {
        namespace: vec!["default".to_string()],
        properties: Vec::new(),
    };
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(value["properties"], serde_json::json!({}));
}

#[test]
fn config_map_deserializes_object_into_config_entries() {
    let json = serde_json::json!({
        "defaults": {"a": "1", "b": "2"},
        "overrides": {"c": "3"},
        "endpoints": ["GET /v1/config"],
    });
    let config: CatalogConfigResponse = serde_json::from_value(json).unwrap();
    assert_eq!(config.defaults.len(), 2);
    assert!(
        config
            .defaults
            .iter()
            .any(|e| e.key == "a" && e.value == "1")
    );
    assert!(
        config
            .defaults
            .iter()
            .any(|e| e.key == "b" && e.value == "2")
    );
    assert_eq!(config.overrides.len(), 1);
    assert_eq!(config.overrides[0].key, "c");
}

#[test]
fn config_map_round_trips_through_object_form() {
    let original = CatalogConfigResponse::default();
    let json = serde_json::to_string(&original).unwrap();
    // Sanity: the serialized form is the object shape, not an array.
    assert!(json.contains(&format!("\"{LAKECAT_COMPATIBILITY_KEY}\":")));
    let parsed: CatalogConfigResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(original, parsed);
}

#[test]
fn load_table_and_credential_configs_serialize_as_objects() {
    let table = LoadTableResponse {
        identifier: TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        metadata_location: Some("file:///tmp/meta.json".to_string()),
        metadata: serde_json::json!({}),
        config: vec![ConfigEntry::new("k", "v")],
    };
    let value = serde_json::to_value(&table).unwrap();
    assert_eq!(value["config"], serde_json::json!({"k": "v"}));
    let parsed: LoadTableResponse = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.config, table.config);

    let credential = StorageCredential {
        prefix: "file:///tmp".to_string(),
        config: vec![ConfigEntry::new("mode", "local")],
    };
    let value = serde_json::to_value(&credential).unwrap();
    assert_eq!(value["config"], serde_json::json!({"mode": "local"}));
    let parsed: StorageCredential = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.config, credential.config);
}

#[test]
fn catalog_config_endpoints_advertise_canonical_iceberg_routes() {
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();

    // Stock clients match against `<METHOD> /v1/{prefix}/...`.
    assert!(endpoints.contains("GET /v1/config"));
    assert!(endpoints.contains("POST /v1/{prefix}/namespaces"));
    assert!(endpoints.contains("POST /v1/{prefix}/namespaces/{namespace}/tables"));
    assert!(endpoints.contains("GET /v1/{prefix}/namespaces/{namespace}/tables/{table}"));
    // updateTable: bare POST on the table path (no `/commit` suffix).
    assert!(endpoints.contains("POST /v1/{prefix}/namespaces/{namespace}/tables/{table}"));
    assert!(endpoints.contains("DELETE /v1/{prefix}/namespaces/{namespace}/tables/{table}"));
    assert!(
        endpoints.contains("GET /v1/{prefix}/namespaces/{namespace}/tables/{table}/credentials")
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
fn list_tables_response_serializes_as_identifier_objects() {
    let response = ListTablesResponse {
        identifiers: vec![
            TableIdentifier {
                namespace: vec!["default".to_string()],
                name: "events".to_string(),
            },
            TableIdentifier {
                namespace: vec!["a".to_string(), "b".to_string()],
                name: "metrics".to_string(),
            },
        ],
    };
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(
        value["identifiers"],
        serde_json::json!([
            {"namespace": ["default"], "name": "events"},
            {"namespace": ["a", "b"], "name": "metrics"},
        ])
    );
    let parsed: ListTablesResponse = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, response);
}

#[test]
fn catalog_config_endpoints_advertise_table_list_routes() {
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();

    // listTables: GET on the `/tables` collection, advertised in all three
    // route families (canonical `/v1/{prefix}`, default, and `{warehouse}`).
    assert!(endpoints.contains("GET /v1/{prefix}/namespaces/{namespace}/tables"));
    assert!(endpoints.contains("GET /catalog/v1/namespaces/{namespace}/tables"));
    assert!(endpoints.contains("GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables"));
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
