use crate::*;

#[test]
#[cfg(not(feature = "qglake-fixture"))]
fn qglake_fixture_requires_explicit_feature() {
    let err = match Command::parse(["qglake-fixture".to_string()]) {
        Ok(_) => panic!("qglake-fixture should require its explicit feature"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("qglake-fixture feature"));
}

#[test]
#[cfg(feature = "qglake-fixture")]
fn parses_qglake_fixture_command_defaults() {
    let command = Command::parse(["qglake-fixture".to_string()]).unwrap();
    match command {
        Command::QglakeFixture {
            warehouse,
            namespace,
            table,
            location,
            output,
            drain_output,
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
            assert_eq!(drain_output, None);
        }
        _ => panic!("expected qglake-fixture command"),
    }
}

#[test]
#[cfg(feature = "qglake-fixture")]
fn parses_qglake_fixture_drain_output() {
    let command = Command::parse([
        "qglake-fixture".to_string(),
        "--drain-output".to_string(),
        "target/qglake/lineage-drain.json".to_string(),
    ])
    .unwrap();
    match command {
        Command::QglakeFixture { drain_output, .. } => {
            assert_eq!(
                drain_output,
                Some(PathBuf::from("target/qglake/lineage-drain.json"))
            );
        }
        _ => panic!("expected qglake-fixture command"),
    }
}

#[test]
#[cfg(feature = "qglake-fixture")]
fn qglake_fixture_metadata_contains_restricted_raw_payload_column() {
    let (location, metadata_location) = qglake_test_fixture_urls("metadata");
    let metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
    let fields = metadata["schemas"][0]["fields"].as_array().unwrap();
    assert!(fields.iter().any(|field| field["name"] == "raw_payload"));
    assert!(metadata_has_manifest_list(&metadata));
    assert!(
        file_url_path(
            metadata["snapshots"][0]["manifest-list"].as_str().unwrap(),
            "test"
        )
        .unwrap()
        .exists()
    );
    let metadata_file = file_url_path(&metadata_location, "test").unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(metadata_file).unwrap()).unwrap()["format-version"],
        json!(3)
    );
}

#[test]
#[cfg(feature = "qglake-fixture")]
fn qglake_existing_table_verifier_accepts_matching_fixture_table() {
    let (location, metadata_location) = qglake_test_fixture_urls("matching");
    let response = LoadTableResponse {
        identifier: TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        metadata_location: Some(metadata_location.clone()),
        metadata: qglake_table_metadata(&location, &metadata_location).unwrap(),
        config: Vec::new(),
    };

    verify_qglake_existing_table(
        &response,
        &["default".to_string()],
        "events",
        &metadata_location,
    )
    .expect("matching QGLake fixture table should be accepted");
}

#[test]
#[cfg(feature = "qglake-fixture")]
fn qglake_existing_table_verifier_rejects_drifted_fixture_table() {
    let (location, metadata_location) = qglake_test_fixture_urls("drifted");
    let mut metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
    metadata["schemas"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|field| field["name"] != "raw_payload");
    write_qglake_metadata_file(
        &file_url_path(&metadata_location, "test").unwrap(),
        &metadata,
    )
    .unwrap();
    let response = LoadTableResponse {
        identifier: TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        metadata_location: Some(metadata_location.clone()),
        metadata,
        config: Vec::new(),
    };

    let err = verify_qglake_existing_table(
        &response,
        &["default".to_string()],
        "events",
        &metadata_location,
    )
    .expect_err("drifted QGLake fixture table should be rejected");
    assert!(err.to_string().contains("raw_payload"));
}

#[test]
#[cfg(feature = "qglake-fixture")]
fn qglake_existing_table_verifier_rejects_missing_metadata_pointer_file() {
    let (location, metadata_location) = qglake_test_fixture_urls("missing-pointer");
    let metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
    fs::remove_file(file_url_path(&metadata_location, "test").unwrap()).unwrap();
    let response = LoadTableResponse {
        identifier: TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        metadata_location: Some(metadata_location.clone()),
        metadata,
        config: Vec::new(),
    };

    let err = verify_qglake_existing_table(
        &response,
        &["default".to_string()],
        "events",
        &metadata_location,
    )
    .expect_err("missing QGLake metadata pointer should be rejected");
    assert!(err.to_string().contains("not readable"));
}

#[test]
#[cfg(feature = "qglake-fixture")]
fn qglake_existing_table_verifier_rejects_drifted_metadata_pointer_file() {
    let (location, metadata_location) = qglake_test_fixture_urls("drifted-pointer");
    let metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
    let metadata_file = file_url_path(&metadata_location, "test").unwrap();
    let mut drifted = metadata.clone();
    drifted["last-sequence-number"] = json!(99);
    write_qglake_metadata_file(&metadata_file, &drifted).unwrap();
    let response = LoadTableResponse {
        identifier: TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        metadata_location: Some(metadata_location.clone()),
        metadata,
        config: Vec::new(),
    };

    let err = verify_qglake_existing_table(
        &response,
        &["default".to_string()],
        "events",
        &metadata_location,
    )
    .expect_err("drifted QGLake metadata pointer should be rejected");
    assert!(err.to_string().contains("does not match"));
}

#[test]
#[cfg(feature = "qglake-fixture")]
fn qglake_existing_table_verifier_rejects_missing_manifest_list_file() {
    let (location, metadata_location) = qglake_test_fixture_urls("missing-manifest-list");
    let metadata = qglake_table_metadata(&location, &metadata_location).unwrap();
    fs::remove_file(
        file_url_path(
            metadata["snapshots"][0]["manifest-list"].as_str().unwrap(),
            "test",
        )
        .unwrap(),
    )
    .unwrap();
    let response = LoadTableResponse {
        identifier: TableIdentifier {
            namespace: vec!["default".to_string()],
            name: "events".to_string(),
        },
        metadata_location: Some(metadata_location.clone()),
        metadata,
        config: Vec::new(),
    };

    let err = verify_qglake_existing_table(
        &response,
        &["default".to_string()],
        "events",
        &metadata_location,
    )
    .expect_err("missing QGLake manifest list should be rejected");
    assert!(err.to_string().contains("manifest list"));
}

#[cfg(feature = "qglake-fixture")]
fn qglake_test_fixture_urls(name: &str) -> (String, String) {
    let root = std::env::temp_dir().join(format!(
        "lakecat-qglake-cli-{name}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let table_dir = root.join("events");
    let metadata_file = table_dir.join("metadata").join("00000.json");
    (
        Url::from_directory_path(&table_dir).unwrap().to_string(),
        Url::from_file_path(metadata_file).unwrap().to_string(),
    )
}
