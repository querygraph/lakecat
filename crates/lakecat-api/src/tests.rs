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
        properties: BTreeMap::new(),
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
    let endpoints = CatalogConfigResponse::default().endpoints;

    assert_eq!(
        endpoints,
        LAKECAT_ICEBERG_REST_ENDPOINTS
            .iter()
            .map(|endpoint| (*endpoint).to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        endpoints
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        endpoints.len(),
        "advertised endpoints must be unique"
    );
}

#[test]
fn register_table_request_uses_the_iceberg_wire_shape() {
    let request: RegisterTableRequest = serde_json::from_value(serde_json::json!({
        "name": "events",
        "metadata-location": "s3://warehouse/events/metadata/00000.json"
    }))
    .unwrap();
    assert!(!request.overwrite);
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        serde_json::json!({
            "name": "events",
            "metadata-location": "s3://warehouse/events/metadata/00000.json",
            "overwrite": false
        })
    );
}

#[test]
fn rename_table_request_preserves_multipart_iceberg_identifiers() {
    let request: RenameTableRequest = serde_json::from_value(serde_json::json!({
        "source": {"namespace": ["a.b", "source"], "name": "events"},
        "destination": {"namespace": ["a.b", "destination"], "name": "renamed_events"}
    }))
    .unwrap();
    assert_eq!(request.source.namespace, ["a.b", "source"]);
    assert_eq!(request.destination.namespace, ["a.b", "destination"]);
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        serde_json::json!({
            "source": {"namespace": ["a.b", "source"], "name": "events"},
            "destination": {"namespace": ["a.b", "destination"], "name": "renamed_events"}
        })
    );
}

#[test]
fn catalog_config_endpoints_exclude_mount_and_control_plane_routes() {
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();

    assert!(endpoints.iter().all(|endpoint| endpoint.contains(" /v1/")));
    assert!(
        !endpoints
            .iter()
            .any(|endpoint| endpoint.contains(" /catalog/"))
    );
    assert!(
        !endpoints
            .iter()
            .any(|endpoint| endpoint.contains(" /management/"))
    );
    assert!(
        !endpoints
            .iter()
            .any(|endpoint| endpoint.contains(" /querygraph/"))
    );
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
fn catalog_config_endpoints_advertise_table_collection_registration_and_rename_routes() {
    let endpoints = CatalogConfigResponse::default()
        .endpoints
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();

    // listTables is represented once with the OpenAPI `{prefix}` placeholder;
    // deployment mount aliases do not belong in protocol capability data.
    assert!(endpoints.contains("GET /v1/{prefix}/namespaces/{namespace}/tables"));
    assert!(endpoints.contains("POST /v1/{prefix}/namespaces/{namespace}/register"));
    assert!(endpoints.contains("POST /v1/{prefix}/tables/rename"));
}

#[test]
fn namespace_protocol_types_match_iceberg_json_shapes() {
    let create: CreateNamespaceRequest = serde_json::from_value(serde_json::json!({
        "namespace": ["accounting", "tax"],
        "properties": {"owner": "finance"}
    }))
    .unwrap();
    assert_eq!(
        create.properties.get("owner").map(String::as_str),
        Some("finance")
    );

    let list = ListNamespacesResponse {
        next_page_token: Some("opaque".to_string()),
        namespaces: vec![vec!["accounting".to_string()]],
    };
    assert_eq!(
        serde_json::to_value(list).unwrap(),
        serde_json::json!({
            "next-page-token": "opaque",
            "namespaces": [["accounting"]]
        })
    );

    let update: UpdateNamespacePropertiesRequest = serde_json::from_value(serde_json::json!({
        "removals": ["legacy"],
        "updates": {"owner": "finance"}
    }))
    .unwrap();
    assert_eq!(update.removals, vec!["legacy"]);
    assert_eq!(
        update.updates.get("owner").map(String::as_str),
        Some("finance")
    );
}
